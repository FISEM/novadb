use std::cmp::Ordering;
use std::collections::HashMap;

use serde_json::{Map, Value};
use sql::ast::*;

use crate::value::*;
use crate::{EngineError, Result};

#[derive(Debug, Clone)]
pub struct Row {
    pub sources: Vec<(String, Map<String, Value>)>,
}

impl Row {
    pub fn single(alias: String, map: Map<String, Value>) -> Self {
        Row { sources: vec![(alias, map)] }
    }

    pub fn empty() -> Self {
        Row { sources: Vec::new() }
    }

    pub fn get(&self, table: Option<&str>, name: &str) -> Value {
        match table {
            Some(t) => self
                .sources
                .iter()
                .find(|(alias, _)| alias == t)
                .and_then(|(_, m)| m.get(name))
                .cloned()
                .unwrap_or(Value::Null),
            None => {
                for (_, m) in self.sources.iter().rev() {
                    if let Some(v) = m.get(name) {
                        return v.clone();
                    }
                }
                Value::Null
            }
        }
    }

    pub fn merged(&self) -> Map<String, Value> {
        let mut out = Map::new();
        for (_, m) in &self.sources {
            for (k, v) in m {
                out.insert(k.clone(), v.clone());
            }
        }
        out
    }
}

pub struct ExecCtx<'a> {
    pub store: &'a storage::Store,
    pub ctes: HashMap<String, Vec<Map<String, Value>>>,
}

impl<'a> ExecCtx<'a> {
    pub fn new(store: &'a storage::Store) -> Self {
        ExecCtx { store, ctes: HashMap::new() }
    }

    fn resolve_source(&self, table_ref: &TableRef) -> Result<(String, Vec<Map<String, Value>>)> {
        let alias = table_ref.alias.clone().unwrap_or_else(|| table_ref.name.clone());
        if let Some(rows) = self.ctes.get(&table_ref.name) {
            return Ok((alias, rows.clone()));
        }
        let rows = self
            .store
            .scan_table(&table_ref.name)
            .map_err(|e| match e {
                storage::StorageError::TableNotFound(t) => EngineError::UnknownTable(t),
                other => EngineError::Storage(other),
            })?
            .into_iter()
            .map(|(id, mut row)| {
                row.entry("id".to_string()).or_insert(Value::Number(id.into()));
                row
            })
            .collect();
        Ok((alias, rows))
    }

    pub fn eval_select_stmt(&mut self, stmt: &SelectStmt) -> Result<Vec<Map<String, Value>>> {
        for cte in &stmt.ctes {
            let rows = self.eval_cte(cte)?;
            self.ctes.insert(cte.name.clone(), rows);
        }

        let mut rows = self.eval_set_expr(&stmt.body)?;

        if !stmt.order_by.is_empty() {
            rows.sort_by(|a, b| {
                for item in &stmt.order_by {
                    let va = eval_expr_on_map(a, &item.expr).unwrap_or(Value::Null);
                    let vb = eval_expr_on_map(b, &item.expr).unwrap_or(Value::Null);
                    let ord = compare_values(&va, &vb).unwrap_or(Ordering::Equal);
                    let ord = if item.desc { ord.reverse() } else { ord };
                    if ord != Ordering::Equal {
                        return ord;
                    }
                }
                Ordering::Equal
            });
        }

        let offset = stmt.offset.unwrap_or(0).max(0) as usize;
        if offset > 0 {
            rows = rows.into_iter().skip(offset).collect();
        }
        if let Some(limit) = stmt.limit {
            rows.truncate(limit.max(0) as usize);
        }

        Ok(rows)
    }

    fn eval_cte(&mut self, cte: &CteDef) -> Result<Vec<Map<String, Value>>> {
        if cte.recursive {
            if let SetExpr::Union { left, all, right } = cte.query.as_ref() {
                let base = self.eval_set_expr(left)?;
                let mut result = base.clone();
                let mut working = base;
                let mut guard = 0;
                loop {
                    guard += 1;
                    if guard > 100_000 || working.is_empty() {
                        break;
                    }
                    self.ctes.insert(cte.name.clone(), working.clone());
                    let mut next = self.eval_set_expr(right)?;
                    self.ctes.remove(&cte.name);
                    if !all {
                        next.retain(|r| !result.iter().any(|existing| existing == r));
                    }
                    if next.is_empty() {
                        break;
                    }
                    result.extend(next.clone());
                    working = next;
                }
                return Ok(result);
            }
        }
        self.eval_set_expr(&cte.query)
    }

    fn eval_set_expr(&mut self, set: &SetExpr) -> Result<Vec<Map<String, Value>>> {
        match set {
            SetExpr::Select(core) => self.eval_select_core(core),
            SetExpr::Union { left, all, right } => {
                let mut l = self.eval_set_expr(left)?;
                let r = self.eval_set_expr(right)?;
                if *all {
                    l.extend(r);
                } else {
                    for row in r {
                        if !l.iter().any(|existing| existing == &row) {
                            l.push(row);
                        }
                    }
                    dedup_keep_order(&mut l);
                }
                Ok(l)
            }
        }
    }

    fn eval_select_core(&mut self, core: &SelectCore) -> Result<Vec<Map<String, Value>>> {
        let mut rows: Vec<Row> = match &core.from {
            None => vec![Row::empty()],
            Some(table_ref) => {
                let (alias, source_rows) = self.resolve_source(table_ref)?;
                source_rows.into_iter().map(|m| Row::single(alias.clone(), m)).collect()
            }
        };

        for join in &core.joins {
            let (alias, right_rows) = self.resolve_source(&join.table)?;
            let mut next = Vec::new();
            for left_row in &rows {
                let mut matched = false;
                for right_row in &right_rows {
                    let mut candidate = left_row.clone();
                    candidate.sources.push((alias.clone(), right_row.clone()));
                    if is_truthy(&eval_expr(&candidate, &join.on)?) {
                        matched = true;
                        next.push(candidate);
                    }
                }
                if !matched && matches!(join.kind, JoinKind::Left) {
                    let mut candidate = left_row.clone();
                    candidate.sources.push((alias.clone(), Map::new()));
                    next.push(candidate);
                }
            }
            rows = next;
        }

        if let Some(filter) = &core.filter {
            let mut kept = Vec::with_capacity(rows.len());
            for row in rows {
                if is_truthy(&eval_expr(&row, filter)?) {
                    kept.push(row);
                }
            }
            rows = kept;
        }

        let needs_aggregation = !core.group_by.is_empty() || projection_has_aggregate(&core.projection);

        let mut out = if needs_aggregation {
            let groups = group_rows(&rows, &core.group_by)?;
            let mut out = Vec::with_capacity(groups.len());
            for group in &groups {
                if let Some(having) = &core.having {
                    if !is_truthy(&eval_expr_group(group, having)?) {
                        continue;
                    }
                }
                let mut map = Map::new();
                for item in &core.projection {
                    match item {
                        SelectItem::Wildcard => {
                            if let Some(first) = group.first() {
                                for (k, v) in first.merged() {
                                    map.insert(k, v);
                                }
                            }
                        }
                        SelectItem::Expr { expr, alias } => {
                            let name = alias.clone().unwrap_or_else(|| expr_display_name(expr));
                            map.insert(name, eval_expr_group(group, expr)?);
                        }
                    }
                }
                out.push(map);
            }
            out
        } else {
            let mut out = Vec::with_capacity(rows.len());
            for row in &rows {
                let mut map = Map::new();
                for item in &core.projection {
                    match item {
                        SelectItem::Wildcard => {
                            for (k, v) in row.merged() {
                                map.insert(k, v);
                            }
                        }
                        SelectItem::Expr { expr, alias } => {
                            let name = alias.clone().unwrap_or_else(|| expr_display_name(expr));
                            map.insert(name, eval_expr(row, expr)?);
                        }
                    }
                }
                out.push(map);
            }
            out
        };

        if core.distinct {
            dedup_keep_order(&mut out);
        }

        Ok(out)
    }
}

fn dedup_keep_order(rows: &mut Vec<Map<String, Value>>) {
    let mut seen: Vec<Map<String, Value>> = Vec::new();
    rows.retain(|r| {
        if seen.iter().any(|s| s == r) {
            false
        } else {
            seen.push(r.clone());
            true
        }
    });
}

fn group_rows(rows: &[Row], group_by: &[Expr]) -> Result<Vec<Vec<Row>>> {
    if group_by.is_empty() {
        // No GROUP BY: the whole input is a single group (even if empty, so that
        // aggregates like COUNT(*) still produce one result row).
        return Ok(vec![rows.to_vec()]);
    }
    let mut keys: Vec<String> = Vec::new();
    let mut groups: Vec<Vec<Row>> = Vec::new();
    for row in rows {
        let mut key_parts = Vec::with_capacity(group_by.len());
        for expr in group_by {
            key_parts.push(eval_expr(row, expr)?);
        }
        let key = serde_json::to_string(&key_parts).unwrap_or_default();
        if let Some(idx) = keys.iter().position(|k| k == &key) {
            groups[idx].push(row.clone());
        } else {
            keys.push(key);
            groups.push(vec![row.clone()]);
        }
    }
    Ok(groups)
}

fn projection_has_aggregate(items: &[SelectItem]) -> bool {
    items.iter().any(|item| match item {
        SelectItem::Expr { expr, .. } => expr_has_aggregate(expr),
        SelectItem::Wildcard => false,
    })
}

fn expr_has_aggregate(expr: &Expr) -> bool {
    match expr {
        Expr::CountStar => true,
        Expr::FunctionCall { name, args } => is_aggregate_name(name) || args.iter().any(expr_has_aggregate),
        Expr::BinaryOp { left, right, .. } => expr_has_aggregate(left) || expr_has_aggregate(right),
        Expr::UnaryOp { expr, .. } => expr_has_aggregate(expr),
        Expr::IsNull { expr, .. } => expr_has_aggregate(expr),
        Expr::Between { expr, low, high, .. } => {
            expr_has_aggregate(expr) || expr_has_aggregate(low) || expr_has_aggregate(high)
        }
        Expr::InList { expr, list, .. } => expr_has_aggregate(expr) || list.iter().any(expr_has_aggregate),
        _ => false,
    }
}

fn is_aggregate_name(name: &str) -> bool {
    matches!(name.to_ascii_uppercase().as_str(), "COUNT" | "SUM" | "AVG" | "MIN" | "MAX")
}

fn expr_display_name(expr: &Expr) -> String {
    match expr {
        Expr::Column(c) => c.name.clone(),
        Expr::FunctionCall { name, .. } => name.clone(),
        Expr::CountStar => "count".to_string(),
        _ => "expr".to_string(),
    }
}

pub fn eval_expr_on_map(map: &Map<String, Value>, expr: &Expr) -> Result<Value> {
    let row = Row::single(String::new(), map.clone());
    eval_expr(&row, expr)
}

pub fn eval_expr(row: &Row, expr: &Expr) -> Result<Value> {
    match expr {
        Expr::Literal(lit) => Ok(literal_to_value(lit)),
        Expr::CountStar => Ok(Value::Null),
        Expr::Column(c) => Ok(row.get(c.table.as_deref(), &c.name)),
        Expr::BinaryOp { left, op, right } => {
            let l = eval_expr(row, left)?;
            let r = eval_expr(row, right)?;
            apply_binop(op, l, r)
        }
        Expr::UnaryOp { op, expr } => apply_unop(op, eval_expr(row, expr)?),
        Expr::IsNull { expr, negated } => {
            let v = eval_expr(row, expr)?;
            Ok(Value::Bool(matches!(v, Value::Null) != *negated))
        }
        Expr::InList { expr, list, negated } => {
            let v = eval_expr(row, expr)?;
            let mut found = false;
            for item in list {
                let iv = eval_expr(row, item)?;
                if values_equal(&v, &iv) {
                    found = true;
                    break;
                }
            }
            Ok(Value::Bool(found != *negated))
        }
        Expr::Between { expr, low, high, negated } => {
            let v = eval_expr(row, expr)?;
            let lo = eval_expr(row, low)?;
            let hi = eval_expr(row, high)?;
            let within = compare_values(&v, &lo).map(|o| o != Ordering::Less).unwrap_or(false)
                && compare_values(&v, &hi).map(|o| o != Ordering::Greater).unwrap_or(false);
            Ok(Value::Bool(within != *negated))
        }
        Expr::FunctionCall { name, args } => eval_scalar_function(row, name, args),
    }
}

fn eval_expr_group(group: &[Row], expr: &Expr) -> Result<Value> {
    if !expr_has_aggregate(expr) {
        return match group.first() {
            Some(row) => eval_expr(row, expr),
            None => Ok(Value::Null),
        };
    }
    match expr {
        Expr::CountStar => Ok(Value::Number(group.len().into())),
        Expr::FunctionCall { name, args } if is_aggregate_name(name) => eval_aggregate(group, name, args),
        Expr::BinaryOp { left, op, right } => {
            let l = eval_expr_group(group, left)?;
            let r = eval_expr_group(group, right)?;
            apply_binop(op, l, r)
        }
        Expr::UnaryOp { op, expr } => apply_unop(op, eval_expr_group(group, expr)?),
        _ => match group.first() {
            Some(row) => eval_expr(row, expr),
            None => Ok(Value::Null),
        },
    }
}

fn eval_aggregate(group: &[Row], name: &str, args: &[Expr]) -> Result<Value> {
    match name.to_ascii_uppercase().as_str() {
        "COUNT" => {
            if matches!(args.first(), Some(Expr::CountStar)) || args.is_empty() {
                Ok(Value::Number(group.len().into()))
            } else {
                let mut count = 0i64;
                for row in group {
                    if !matches!(eval_expr(row, &args[0])?, Value::Null) {
                        count += 1;
                    }
                }
                Ok(Value::Number(count.into()))
            }
        }
        "SUM" | "AVG" => {
            let mut sum = 0.0;
            let mut count = 0i64;
            for row in group {
                if let Some(v) = args.first() {
                    let val = eval_expr(row, v)?;
                    if let Some(f) = as_f64(&val) {
                        sum += f;
                        count += 1;
                    }
                }
            }
            if count == 0 {
                return Ok(Value::Null);
            }
            let result = if name.eq_ignore_ascii_case("AVG") { sum / count as f64 } else { sum };
            Ok(serde_json::Number::from_f64(result).map(Value::Number).unwrap_or(Value::Null))
        }
        "MIN" | "MAX" => {
            let mut best: Option<Value> = None;
            for row in group {
                if let Some(arg) = args.first() {
                    let val = eval_expr(row, arg)?;
                    if matches!(val, Value::Null) {
                        continue;
                    }
                    best = Some(match &best {
                        None => val,
                        Some(current) => {
                            let ord = compare_values(&val, current).unwrap_or(Ordering::Equal);
                            let take_new = if name.eq_ignore_ascii_case("MIN") { ord == Ordering::Less } else { ord == Ordering::Greater };
                            if take_new { val } else { current.clone() }
                        }
                    });
                }
            }
            Ok(best.unwrap_or(Value::Null))
        }
        other => Err(EngineError::Unsupported(format!("unknown aggregate function {other}"))),
    }
}

fn eval_scalar_function(row: &Row, name: &str, args: &[Expr]) -> Result<Value> {
    let vals = args.iter().map(|a| eval_expr(row, a)).collect::<Result<Vec<_>>>()?;
    match name.to_ascii_uppercase().as_str() {
        "UPPER" => Ok(text_arg(&vals, 0).map(|s| Value::String(s.to_uppercase())).unwrap_or(Value::Null)),
        "LOWER" => Ok(text_arg(&vals, 0).map(|s| Value::String(s.to_lowercase())).unwrap_or(Value::Null)),
        "LENGTH" | "LEN" => Ok(text_arg(&vals, 0).map(|s| Value::Number((s.chars().count() as i64).into())).unwrap_or(Value::Null)),
        "ABS" => Ok(vals.first().and_then(as_f64).map(|f| f.abs()).and_then(serde_json::Number::from_f64).map(Value::Number).unwrap_or(Value::Null)),
        "ROUND" => {
            let precision = vals.get(1).and_then(as_f64).unwrap_or(0.0) as i32;
            let factor = 10f64.powi(precision);
            Ok(vals.first().and_then(as_f64).map(|f| (f * factor).round() / factor).and_then(serde_json::Number::from_f64).map(Value::Number).unwrap_or(Value::Null))
        }
        "COALESCE" => Ok(vals.into_iter().find(|v| !matches!(v, Value::Null)).unwrap_or(Value::Null)),
        "CONCAT" => {
            let mut s = String::new();
            for v in &vals {
                s.push_str(&display_value(v));
            }
            Ok(Value::String(s))
        }
        other => Err(EngineError::Unsupported(format!("unknown function {other}"))),
    }
}

fn text_arg(vals: &[Value], idx: usize) -> Option<String> {
    vals.get(idx).map(display_value)
}

fn display_value(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn apply_unop(op: &UnOp, v: Value) -> Result<Value> {
    match op {
        UnOp::Not => Ok(Value::Bool(!is_truthy(&v))),
        UnOp::Neg => Ok(as_f64(&v)
            .and_then(|f| serde_json::Number::from_f64(-f))
            .map(Value::Number)
            .unwrap_or(Value::Null)),
    }
}

fn apply_binop(op: &BinOp, l: Value, r: Value) -> Result<Value> {
    use BinOp::*;
    Ok(match op {
        And => Value::Bool(is_truthy(&l) && is_truthy(&r)),
        Or => Value::Bool(is_truthy(&l) || is_truthy(&r)),
        Eq => Value::Bool(values_equal(&l, &r)),
        NotEq => Value::Bool(!values_equal(&l, &r)),
        Lt => Value::Bool(compare_values(&l, &r) == Some(Ordering::Less)),
        LtEq => Value::Bool(matches!(compare_values(&l, &r), Some(Ordering::Less) | Some(Ordering::Equal))),
        Gt => Value::Bool(compare_values(&l, &r) == Some(Ordering::Greater)),
        GtEq => Value::Bool(matches!(compare_values(&l, &r), Some(Ordering::Greater) | Some(Ordering::Equal))),
        Add => numeric_binop(&l, &r, |a, b| a + b),
        Sub => numeric_binop(&l, &r, |a, b| a - b),
        Mul => numeric_binop(&l, &r, |a, b| a * b),
        Div => numeric_binop(&l, &r, |a, b| a / b),
        Mod => numeric_binop(&l, &r, |a, b| a % b),
        Concat => Value::String(format!("{}{}", display_value(&l), display_value(&r))),
        Like => {
            let text = display_value(&l);
            let pattern = display_value(&r);
            Value::Bool(like_match(&text, &pattern))
        }
        JsonGet => match (&l, &r) {
            (Value::Object(map), Value::String(key)) => map.get(key).cloned().unwrap_or(Value::Null),
            (Value::Array(arr), idx) => as_f64(idx).and_then(|i| arr.get(i as usize)).cloned().unwrap_or(Value::Null),
            _ => Value::Null,
        },
        JsonGetText => match (&l, &r) {
            (Value::Object(map), Value::String(key)) => map.get(key).map(display_value).map(Value::String).unwrap_or(Value::Null),
            (Value::Array(arr), idx) => as_f64(idx)
                .and_then(|i| arr.get(i as usize))
                .map(display_value)
                .map(Value::String)
                .unwrap_or(Value::Null),
            _ => Value::Null,
        },
    })
}

fn numeric_binop(l: &Value, r: &Value, f: impl Fn(f64, f64) -> f64) -> Value {
    match (as_f64(l), as_f64(r)) {
        (Some(a), Some(b)) => serde_json::Number::from_f64(f(a, b)).map(Value::Number).unwrap_or(Value::Null),
        _ => Value::Null,
    }
}
