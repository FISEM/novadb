//! Running nova pipelines.
//!
//! A pipeline is a fold: each step takes the records the step above produced
//! and hands records to the step below. The value semantics — truthiness,
//! ordering, what equals what — are shared with the rest of the engine, so a
//! record behaves the same however it was asked for.

use lang::{
    BinOp, CompareOp, Expr, Literal, Pipeline, Statement, Step, UnOp,
};
use serde_json::{Map, Number, Value};

use crate::exec::Row;
use crate::value::{compare_values, is_truthy, values_equal};
use crate::{EngineError, ExecResult, Result};

pub(crate) fn run_statement(store: &storage::Store, stmt: &Statement) -> Result<ExecResult> {
    match stmt {
        Statement::Query(pipeline) => {
            let rows = run_pipeline(store, pipeline)?;
            Ok(ExecResult::Select { rows })
        }
        other => Err(EngineError::Unsupported(format!(
            "{} is not running yet",
            statement_name(other)
        ))),
    }
}

fn statement_name(stmt: &Statement) -> &'static str {
    match stmt {
        Statement::Query(_) => "this query",
        Statement::DefineShape(_) => "define",
        Statement::DefineName(_) => "define … as",
        Statement::Add(_) => "add",
        Statement::Remove { .. } => "remove",
    }
}

/// What flows between steps: records, or records already split into groups.
enum Stream {
    Records(Vec<Row>),
    Groups(Vec<Vec<Row>>),
}

fn run_pipeline(store: &storage::Store, pipeline: &Pipeline) -> Result<Vec<Map<String, Value>>> {
    let source = pipeline.source.as_ref().ok_or_else(|| {
        EngineError::Unsupported(
            "this pipeline starts with a step, so it needs records piped into it".to_string(),
        )
    })?;

    let mut stream = Stream::Records(read(store, &source.name)?);
    // What fields the records are known to have. `None` before any `show`,
    // because a collection makes no promises about its records. After a
    // `show` the answer is exact, and a later step naming anything else is
    // asking for a field that is provably gone.
    let mut known: Option<Vec<String>> = None;

    let collection = source.name.clone();
    for step in &pipeline.steps {
        if let Some(names) = &known {
            check_fields(step, names)?;
        }
        if let Step::Show(items) = step {
            known = Some(
                items
                    .iter()
                    .map(|i| i.alias.clone().unwrap_or_else(|| output_name(&i.value)))
                    .collect(),
            );
        }
        stream = apply(store, &collection, stream, step)?;
    }

    Ok(match stream {
        Stream::Records(rows) => rows.iter().map(Row::merged).collect(),
        Stream::Groups(_) => {
            return Err(EngineError::Unsupported(
                "this pipeline ends on a 'group by', which produces groups rather than records"
                    .to_string(),
            ))
        }
    })
}

/// Every record in a collection, each carrying its id.
fn read(store: &storage::Store, name: &str) -> Result<Vec<Row>> {
    let rows = store.scan_table(name).map_err(|e| match e {
        storage::StorageError::TableNotFound(t) => EngineError::UnknownTable(t),
        other => EngineError::Storage(other),
    })?;
    Ok(rows
        .into_iter()
        .map(|(id, mut record)| {
            record.entry("id".to_string()).or_insert(Value::Number(id.into()));
            Row::single(name.to_string(), record)
        })
        .collect())
}

fn apply(
    store: &storage::Store,
    collection: &str,
    stream: Stream,
    step: &Step,
) -> Result<Stream> {
    let rows = match (stream, step) {
        // A group is not a record, so only a `show` can read one.
        (Stream::Groups(groups), Step::Show(items)) => {
            let mut out = Vec::with_capacity(groups.len());
            for group in &groups {
                out.push(fold_group(group, items)?);
            }
            return Ok(Stream::Records(out));
        }
        (Stream::Groups(_), _) => {
            return Err(EngineError::Unsupported(
                "a 'group by' makes groups, so the next step has to be a 'show'".to_string(),
            ))
        }
        (Stream::Records(rows), _) => rows,
    };

    if let Step::GroupBy(keys) = step {
        return Ok(Stream::Groups(split_into_groups(rows, keys)?));
    }

    Ok(Stream::Records(match step {
        Step::Where(condition) => {
            let mut kept = Vec::with_capacity(rows.len());
            for row in rows {
                if is_truthy(&eval(Scope::on(&row), condition)?) {
                    kept.push(row);
                }
            }
            kept
        }

        // A `show` that counts, with no `group by` above it, folds everything
        // that reached it into one record. Nothing is being guessed here:
        // Cypher's trouble is that it infers the grouping *key*, and with no
        // `group by` written there is no key to infer.
        Step::Show(items) if items.iter().any(|i| counts(&i.value)) => {
            vec![fold_group(&rows, items)?]
        }

        Step::Show(items) => {
            let mut out = Vec::with_capacity(rows.len());
            for row in &rows {
                let mut record = Map::new();
                for item in items {
                    let name = match &item.alias {
                        Some(alias) => alias.clone(),
                        None => output_name(&item.value),
                    };
                    record.insert(name, eval(Scope::on(row), &item.value)?);
                }
                // A projection makes new records, so nothing is qualified by a
                // collection any more: only what `show` kept still exists.
                out.push(Row::single(String::new(), record));
            }
            out
        }

        Step::Sort(keys) => {
            let mut keyed = Vec::with_capacity(rows.len());
            for row in rows {
                let mut values = Vec::with_capacity(keys.len());
                for key in keys {
                    values.push(eval(Scope::on(&row), &key.value)?);
                }
                keyed.push((values, row));
            }
            keyed.sort_by(|(a, _), (b, _)| {
                for (i, key) in keys.iter().enumerate() {
                    let order = compare_values(&a[i], &b[i]).unwrap_or(std::cmp::Ordering::Equal);
                    let order = match key.direction {
                        lang::Direction::Up => order,
                        lang::Direction::Down => order.reverse(),
                    };
                    if order != std::cmp::Ordering::Equal {
                        return order;
                    }
                }
                std::cmp::Ordering::Equal
            });
            keyed.into_iter().map(|(_, row)| row).collect()
        }

        Step::Take { count, .. } => rows.into_iter().take((*count).max(0) as usize).collect(),
        Step::Skip { count, .. } => rows.into_iter().skip((*count).max(0) as usize).collect(),

        Step::Unique => {
            let mut seen: Vec<Map<String, Value>> = Vec::new();
            let mut out = Vec::new();
            for row in rows {
                let record = row.merged();
                if !seen.iter().any(|s| *s == record) {
                    seen.push(record);
                    out.push(row);
                }
            }
            out
        }

        Step::Join(join) => {
            let right = read(store, &join.collection.name)?;
            let mut out = Vec::new();
            for left in &rows {
                let mut matched = false;
                for other in &right {
                    let mut candidate = left.clone();
                    candidate
                        .sources
                        .push((join.collection.name.clone(), other.merged()));
                    if is_truthy(&eval(Scope::on(&candidate), &join.on)?) {
                        matched = true;
                        out.push(candidate);
                    }
                }
                if !matched && join.keep_all {
                    let mut candidate = left.clone();
                    candidate.sources.push((join.collection.name.clone(), Map::new()));
                    out.push(candidate);
                }
            }
            out
        }

        Step::Follow(follow) => follow_links(store, collection, &rows, follow)?,

        other => {
            return Err(EngineError::Unsupported(format!(
                "'{}' is not running yet",
                step_name(other)
            )))
        }
    }))
}

/// True when an expression asks for something that folds a group.
fn counts(expr: &Expr) -> bool {
    match expr {
        Expr::Call { name, args, .. } => {
            COUNTING.contains(&name.as_str()) || args.iter().any(counts)
        }
        Expr::Binary { left, right, .. } => counts(left) || counts(right),
        Expr::Unary { value, .. } | Expr::IsNone { value, .. } => counts(value),
        Expr::Comparison { first, rest, .. } => {
            counts(first) || rest.iter().any(|(_, e)| counts(e))
        }
        Expr::In { value, options, .. } => counts(value) || counts(options),
        Expr::Index { value, index, .. } => counts(value) || counts(index),
        Expr::Method { value, args, .. } => counts(value) || args.iter().any(counts),
        Expr::List { items, .. } => items.iter().any(counts),
        Expr::RecordLiteral { fields, .. } => fields.iter().any(|(_, e)| counts(e)),
        Expr::Field { .. } | Expr::Literal { .. } => false,
    }
}

/// Turns one group into one record. Whatever does not count is read from the
/// group's first record, which is where its grouping key lives.
fn fold_group(group: &[Row], items: &[lang::ShowItem]) -> Result<Row> {
    let empty = Row::empty();
    let scope = Scope { row: group.first().unwrap_or(&empty), group: Some(group) };
    let mut record = Map::new();
    for item in items {
        let name = match &item.alias {
            Some(alias) => alias.clone(),
            None => output_name(&item.value),
        };
        record.insert(name, eval(scope, &item.value)?);
    }
    Ok(Row::single(String::new(), record))
}

/// Splits records into groups, keeping the order each group first appeared in.
fn split_into_groups(rows: Vec<Row>, keys: &[Expr]) -> Result<Vec<Vec<Row>>> {
    let mut seen: Vec<Vec<Value>> = Vec::new();
    let mut groups: Vec<Vec<Row>> = Vec::new();
    for row in rows {
        let mut key = Vec::with_capacity(keys.len());
        for expr in keys {
            key.push(eval(Scope::on(&row), expr)?);
        }
        match seen.iter().position(|k| *k == key) {
            Some(at) => groups[at].push(row),
            None => {
                seen.push(key);
                groups.push(vec![row]);
            }
        }
    }
    Ok(groups)
}

/// Walks a link, one step or as far as it goes.
///
/// Links live in a collection called `edges`, holding `from_id`, `to_id` and
/// `label`. A link lands on a record of the collection the pipeline started
/// from, which is what makes friends-of-friends work and what stops a link
/// from reaching another collection. See docs/language.md.
fn follow_links(
    store: &storage::Store,
    collection: &str,
    rows: &[Row],
    follow: &lang::Follow,
) -> Result<Vec<Row>> {
    let edges = read(store, "edges").map_err(|e| match e {
        EngineError::UnknownTable(_) => EngineError::Unsupported(
            "following a link needs a collection called 'edges', holding from_id, to_id and label"
                .to_string(),
        ),
        other => other,
    })?;

    let label = Value::String(follow.link.clone());
    let mut links: Vec<(String, Value)> = Vec::new();
    for edge in &edges {
        let record = edge.merged();
        if record.get("label") != Some(&label) {
            continue;
        }
        let (from, to) = (record.get("from_id"), record.get("to_id"));
        let (start, end) = if follow.backward { (to, from) } else { (from, to) };
        if let (Some(start), Some(end)) = (start, end) {
            links.push((key_of(start), end.clone()));
        }
    }

    let mut frontier: Vec<String> = rows.iter().map(|r| key_of(&r.get(None, "id"))).collect();
    let mut reached: Vec<String> = Vec::new();
    loop {
        let mut next = Vec::new();
        for (start, end) in &links {
            if frontier.iter().any(|f| f == start) {
                let end_key = key_of(end);
                if !reached.contains(&end_key) {
                    reached.push(end_key.clone());
                    next.push(end_key);
                }
            }
        }
        if next.is_empty() || !follow.repeat {
            break;
        }
        frontier = next;
    }

    Ok(read(store, collection)?
        .into_iter()
        .filter(|row| reached.contains(&key_of(&row.get(None, "id"))))
        .collect())
}

/// A value as a string, so ids can be matched whatever they are made of.
fn key_of(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn step_name(step: &Step) -> &'static str {
    match step {
        Step::Where(_) => "where",
        Step::Show(_) => "show",
        Step::Sort(_) => "sort",
        Step::Take { .. } => "take",
        Step::Skip { .. } => "skip",
        Step::Unique => "unique",
        Step::Join(_) => "join",
        Step::GroupBy(_) => "group by",
        Step::Follow(f) if f.repeat => "keep following",
        Step::Follow(_) => "follow",
        Step::Set(_) => "set",
        Step::Delete { .. } => "delete",
        Step::Named(_) => "a defined name",
    }
}

/// Refuses a step that reads a field an earlier `show` did not keep.
///
/// Returning nothing here instead would be a wrong answer with no error,
/// which is the failure this language exists to avoid.
fn check_fields(step: &Step, known: &[String]) -> Result<()> {
    let mut wanted = Vec::new();
    match step {
        Step::Where(e) => collect_fields(e, &mut wanted),
        Step::Show(items) => items.iter().for_each(|i| collect_fields(&i.value, &mut wanted)),
        Step::Sort(keys) => keys.iter().for_each(|k| collect_fields(&k.value, &mut wanted)),
        Step::GroupBy(keys) => keys.iter().for_each(|k| collect_fields(k, &mut wanted)),
        Step::Set(assignments) => {
            assignments.iter().for_each(|a| collect_fields(&a.value, &mut wanted))
        }
        Step::Join(join) => collect_fields(&join.on, &mut wanted),
        Step::Take { .. } | Step::Skip { .. } | Step::Unique | Step::Delete { .. }
        | Step::Follow(_) | Step::Named(_) => {}
    }

    for name in wanted {
        if !known.iter().any(|k| *k == name) {
            let kept = known.join(", ");
            return Err(EngineError::Unsupported(format!(
                "'{name}' was dropped by an earlier 'show', which kept only {kept}. \
                 Move this step above the show, or add {name} to it."
            )));
        }
    }
    Ok(())
}

/// The first piece of every dotted name an expression reads.
fn collect_fields(expr: &Expr, out: &mut Vec<String>) {
    match expr {
        Expr::Field { path, .. } => {
            if let Some(first) = path.first() {
                out.push(first.clone());
            }
        }
        Expr::Binary { left, right, .. } => {
            collect_fields(left, out);
            collect_fields(right, out);
        }
        Expr::Unary { value, .. } | Expr::IsNone { value, .. } => collect_fields(value, out),
        Expr::Comparison { first, rest, .. } => {
            collect_fields(first, out);
            rest.iter().for_each(|(_, e)| collect_fields(e, out));
        }
        Expr::In { value, options, .. } => {
            collect_fields(value, out);
            collect_fields(options, out);
        }
        Expr::Index { value, index, .. } => {
            collect_fields(value, out);
            collect_fields(index, out);
        }
        Expr::Call { args, .. } => args.iter().for_each(|a| collect_fields(a, out)),
        Expr::Method { value, args, .. } => {
            collect_fields(value, out);
            args.iter().for_each(|a| collect_fields(a, out));
        }
        Expr::List { items, .. } => items.iter().for_each(|i| collect_fields(i, out)),
        Expr::RecordLiteral { fields, .. } => {
            fields.iter().for_each(|(_, e)| collect_fields(e, out))
        }
        Expr::Literal { .. } => {}
    }
}

/// The name a `show` item takes when it was not given one.
fn output_name(expr: &Expr) -> String {
    match expr {
        Expr::Field { path, .. } => path.last().cloned().unwrap_or_else(|| "value".to_string()),
        Expr::Call { name, .. } => name.clone(),
        Expr::Method { name, .. } => name.clone(),
        _ => "value".to_string(),
    }
}

// --- Expressions ------------------------------------------------------------

/// What an expression is evaluated against: one record, and — inside a
/// `show` that follows a `group by` — the whole group behind it, which is
/// what the counting words fold.
#[derive(Clone, Copy)]
struct Scope<'a> {
    row: &'a Row,
    group: Option<&'a [Row]>,
}

impl<'a> Scope<'a> {
    fn on(row: &'a Row) -> Self {
        Scope { row, group: None }
    }
}

fn eval(scope: Scope<'_>, expr: &Expr) -> Result<Value> {
    let row = scope.row;
    Ok(match expr {
        Expr::Literal { value, .. } => match value {
            Literal::Number(n) => Value::Number(n.clone()),
            Literal::String(s) => Value::String(s.clone()),
            Literal::Bool(b) => Value::Bool(*b),
            Literal::None => Value::Null,
        },

        Expr::Field { path, .. } => read_path(row, path),

        Expr::Binary { left, op, right, .. } => match op {
            // `and` and `or` hand back one of their sides, as in Python, so
            // `v or "fallback"` is what COALESCE used to be.
            BinOp::And => {
                let l = eval(scope, left)?;
                if is_truthy(&l) {
                    eval(scope, right)?
                } else {
                    l
                }
            }
            BinOp::Or => {
                let l = eval(scope, left)?;
                if is_truthy(&l) {
                    l
                } else {
                    eval(scope, right)?
                }
            }
            _ => arithmetic(*op, &eval(scope, left)?, &eval(scope, right)?)?,
        },

        Expr::Unary { op, value, .. } => {
            let v = eval(scope, value)?;
            match op {
                UnOp::Not => Value::Bool(!is_truthy(&v)),
                UnOp::Negate => arithmetic(BinOp::Subtract, &Value::Number(Number::from(0)), &v)?,
            }
        }

        Expr::Comparison { first, rest, .. } => {
            let mut left = eval(scope, first)?;
            for (op, next) in rest {
                let right = eval(scope, next)?;
                if !compare(*op, &left, &right) {
                    return Ok(Value::Bool(false));
                }
                left = right;
            }
            Value::Bool(true)
        }

        Expr::In { value, options, negated, .. } => {
            let needle = eval(scope, value)?;
            let haystack = eval(scope, options)?;
            let found = match &haystack {
                Value::Array(items) => items.iter().any(|i| values_equal(i, &needle)),
                Value::Object(fields) => match &needle {
                    Value::String(key) => fields.contains_key(key),
                    _ => false,
                },
                Value::String(text) => match &needle {
                    Value::String(part) => text.contains(part.as_str()),
                    _ => false,
                },
                _ => false,
            };
            Value::Bool(found != *negated)
        }

        Expr::IsNone { value, negated, .. } => {
            Value::Bool(eval(scope, value)?.is_null() != *negated)
        }

        Expr::List { items, .. } => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(eval(scope, item)?);
            }
            Value::Array(out)
        }

        Expr::RecordLiteral { fields, .. } => {
            let mut out = Map::new();
            for (name, value) in fields {
                out.insert(name.clone(), eval(scope, value)?);
            }
            Value::Object(out)
        }

        Expr::Index { value, index, .. } => {
            let target = eval(scope, value)?;
            let key = eval(scope, index)?;
            match (&target, &key) {
                (Value::Array(items), Value::Number(n)) => n
                    .as_i64()
                    .and_then(|i| usize::try_from(i).ok())
                    .and_then(|i| items.get(i))
                    .cloned()
                    .unwrap_or(Value::Null),
                (Value::Object(fields), Value::String(name)) => {
                    fields.get(name).cloned().unwrap_or(Value::Null)
                }
                _ => Value::Null,
            }
        }

        Expr::Call { name, args, .. } => {
            let mut values = Vec::with_capacity(args.len());
            for arg in args {
                values.push(eval(scope, arg)?);
            }
            call(scope, name, args, &values)?
        }

        Expr::Method { value, name, args, .. } => {
            let target = eval(scope, value)?;
            let mut values = Vec::with_capacity(args.len());
            for arg in args {
                values.push(eval(scope, arg)?);
            }
            method(&target, name, &values)?
        }
    })
}

/// Reads a dotted path. The first piece may name a collection the record came
/// from; otherwise it is a field, and the rest walks into whatever it holds.
fn read_path(row: &Row, path: &[String]) -> Value {
    let Some((first, rest)) = path.split_first() else {
        return Value::Null;
    };

    let mut value = if row.sources.iter().any(|(alias, _)| alias == first) {
        match rest.split_first() {
            Some((field, deeper)) => {
                let start = row.get(Some(first), field);
                return walk(start, deeper);
            }
            None => return Value::Null,
        }
    } else {
        row.get(None, first)
    };
    value = walk(value, rest);
    value
}

fn walk(mut value: Value, path: &[String]) -> Value {
    for name in path {
        value = match value {
            Value::Object(fields) => fields.get(name).cloned().unwrap_or(Value::Null),
            _ => return Value::Null,
        };
    }
    value
}

fn compare(op: CompareOp, left: &Value, right: &Value) -> bool {
    use std::cmp::Ordering::*;
    match op {
        CompareOp::Equal => values_equal(left, right),
        CompareOp::NotEqual => !values_equal(left, right),
        _ => match compare_values(left, right) {
            None => false,
            Some(order) => match op {
                CompareOp::Less => order == Less,
                CompareOp::LessOrEqual => order != Greater,
                CompareOp::Greater => order == Greater,
                CompareOp::GreaterOrEqual => order != Less,
                _ => unreachable!("handled above"),
            },
        },
    }
}

/// Arithmetic. Whole numbers stay whole, because nova has one `number` type
/// and turning 26 into 26.0 would be the language inventing a distinction it
/// says it does not have.
fn arithmetic(op: BinOp, left: &Value, right: &Value) -> Result<Value> {
    let (Some(l), Some(r)) = (as_number(left), as_number(right)) else {
        return Ok(Value::Null);
    };

    if let (Some(a), Some(b)) = (l.as_i64(), r.as_i64()) {
        let whole = match op {
            BinOp::Add => a.checked_add(b),
            BinOp::Subtract => a.checked_sub(b),
            BinOp::Multiply => a.checked_mul(b),
            BinOp::Remainder => (b != 0).then(|| a % b),
            _ => None,
        };
        if let Some(n) = whole {
            return Ok(Value::Number(Number::from(n)));
        }
    }

    let (a, b) = (l.as_f64().unwrap_or(0.0), r.as_f64().unwrap_or(0.0));
    let result = match op {
        BinOp::Add => a + b,
        BinOp::Subtract => a - b,
        BinOp::Multiply => a * b,
        BinOp::Divide => {
            if b == 0.0 {
                return Ok(Value::Null);
            }
            a / b
        }
        BinOp::Remainder => {
            if b == 0.0 {
                return Ok(Value::Null);
            }
            a % b
        }
        BinOp::And | BinOp::Or => unreachable!("handled before this point"),
    };
    Ok(Number::from_f64(result).map(Value::Number).unwrap_or(Value::Null))
}

fn as_number(value: &Value) -> Option<Number> {
    match value {
        Value::Number(n) => Some(n.clone()),
        Value::Bool(b) => Some(Number::from(i64::from(*b))),
        Value::String(s) => s
            .parse::<i64>()
            .ok()
            .map(Number::from)
            .or_else(|| s.parse::<f64>().ok().and_then(Number::from_f64)),
        _ => None,
    }
}

/// The words that fold a group rather than reading one record.
const COUNTING: &[&str] = &["count", "total", "average", "lowest", "highest"];

fn counting(scope: Scope<'_>, name: &str, args: &[Expr]) -> Result<Option<Value>> {
    if !COUNTING.contains(&name) {
        return Ok(None);
    }
    // With no `group by` above it, a counting word folds everything that
    // reached this step. Nothing is being guessed: Cypher's trouble is that
    // it infers the grouping *key*, and here there is no key to infer.
    let rows: &[Row] = match scope.group {
        Some(group) => group,
        None => std::slice::from_ref(scope.row),
    };

    if name == "count" {
        return Ok(Some(Value::Number(Number::from(rows.len() as i64))));
    }

    let argument = args.first().ok_or_else(|| {
        EngineError::Unsupported(format!("'{name}' needs a field to work on, like '{name}(age)'"))
    })?;

    let mut values = Vec::with_capacity(rows.len());
    for row in rows {
        let value = eval(Scope { row, group: None }, argument)?;
        if !value.is_null() {
            values.push(value);
        }
    }
    if values.is_empty() {
        return Ok(Some(Value::Null));
    }

    Ok(Some(match name {
        "lowest" | "highest" => {
            let wanted = if name == "lowest" {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
            let mut best = values[0].clone();
            for value in &values[1..] {
                if compare_values(value, &best) == Some(wanted) {
                    best = value.clone();
                }
            }
            best
        }
        _ => {
            let sum: f64 = values.iter().filter_map(|v| as_number(v)?.as_f64()).sum();
            match name {
                "total" => number(sum),
                _ => number(sum / values.len() as f64),
            }
        }
    }))
}

fn call(scope: Scope<'_>, name: &str, args: &[Expr], values: &[Value]) -> Result<Value> {
    if let Some(folded) = counting(scope, name, args)? {
        return Ok(folded);
    }
    let args = values;
    let first = args.first();
    Ok(match (name, first) {
        ("len", Some(v)) => Value::Number(Number::from(length(v) as i64)),
        ("abs", Some(v)) => match as_number(v).and_then(|n| n.as_f64()) {
            Some(f) => number(f.abs()),
            None => Value::Null,
        },
        ("round", Some(v)) => match as_number(v).and_then(|n| n.as_f64()) {
            Some(f) => Value::Number(Number::from(f.round() as i64)),
            None => Value::Null,
        },
        ("number", Some(v)) => as_number(v).map(Value::Number).unwrap_or(Value::Null),
        ("string", Some(v)) => Value::String(match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        }),
        _ => {
            return Err(EngineError::Unsupported(format!(
                "there is no '{name}' to call here"
            )))
        }
    })
}

fn length(value: &Value) -> usize {
    match value {
        Value::String(s) => s.chars().count(),
        Value::Array(items) => items.len(),
        Value::Object(fields) => fields.len(),
        _ => 0,
    }
}

fn number(value: f64) -> Value {
    if value.fract() == 0.0 && value.abs() < 9.0e15 {
        Value::Number(Number::from(value as i64))
    } else {
        Number::from_f64(value).map(Value::Number).unwrap_or(Value::Null)
    }
}

fn method(target: &Value, name: &str, args: &[Value]) -> Result<Value> {
    let text = match target {
        Value::String(s) => s.as_str(),
        _ => "",
    };
    let arg = args.first().and_then(|v| match v {
        Value::String(s) => Some(s.as_str()),
        _ => None,
    });

    Ok(match (name, arg) {
        ("upper", _) => Value::String(text.to_uppercase()),
        ("lower", _) => Value::String(text.to_lowercase()),
        ("startswith", Some(part)) => Value::Bool(text.starts_with(part)),
        ("endswith", Some(part)) => Value::Bool(text.ends_with(part)),
        _ => {
            return Err(EngineError::Unsupported(format!(
                "there is no '{name}' to call on this"
            )))
        }
    })
}
