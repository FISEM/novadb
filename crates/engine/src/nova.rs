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
        stream = apply(store, stream, step)?;
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

fn apply(_store: &storage::Store, stream: Stream, step: &Step) -> Result<Stream> {
    let rows = match (stream, step) {
        (Stream::Groups(_), _) => {
            return Err(EngineError::Unsupported(
                "only a 'show' can follow a 'group by'".to_string(),
            ))
        }
        (Stream::Records(rows), _) => rows,
    };

    Ok(Stream::Records(match step {
        Step::Where(condition) => {
            let mut kept = Vec::with_capacity(rows.len());
            for row in rows {
                if is_truthy(&eval(&row, condition)?) {
                    kept.push(row);
                }
            }
            kept
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
                    record.insert(name, eval(row, &item.value)?);
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
                    values.push(eval(&row, &key.value)?);
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

        other => {
            return Err(EngineError::Unsupported(format!(
                "'{}' is not running yet",
                step_name(other)
            )))
        }
    }))
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

fn eval(row: &Row, expr: &Expr) -> Result<Value> {
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
                let l = eval(row, left)?;
                if is_truthy(&l) {
                    eval(row, right)?
                } else {
                    l
                }
            }
            BinOp::Or => {
                let l = eval(row, left)?;
                if is_truthy(&l) {
                    l
                } else {
                    eval(row, right)?
                }
            }
            _ => arithmetic(*op, &eval(row, left)?, &eval(row, right)?)?,
        },

        Expr::Unary { op, value, .. } => {
            let v = eval(row, value)?;
            match op {
                UnOp::Not => Value::Bool(!is_truthy(&v)),
                UnOp::Negate => arithmetic(BinOp::Subtract, &Value::Number(Number::from(0)), &v)?,
            }
        }

        Expr::Comparison { first, rest, .. } => {
            let mut left = eval(row, first)?;
            for (op, next) in rest {
                let right = eval(row, next)?;
                if !compare(*op, &left, &right) {
                    return Ok(Value::Bool(false));
                }
                left = right;
            }
            Value::Bool(true)
        }

        Expr::In { value, options, negated, .. } => {
            let needle = eval(row, value)?;
            let haystack = eval(row, options)?;
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
            Value::Bool(eval(row, value)?.is_null() != *negated)
        }

        Expr::List { items, .. } => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(eval(row, item)?);
            }
            Value::Array(out)
        }

        Expr::RecordLiteral { fields, .. } => {
            let mut out = Map::new();
            for (name, value) in fields {
                out.insert(name.clone(), eval(row, value)?);
            }
            Value::Object(out)
        }

        Expr::Index { value, index, .. } => {
            let target = eval(row, value)?;
            let key = eval(row, index)?;
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
                values.push(eval(row, arg)?);
            }
            call(name, &values)?
        }

        Expr::Method { value, name, args, .. } => {
            let target = eval(row, value)?;
            let mut values = Vec::with_capacity(args.len());
            for arg in args {
                values.push(eval(row, arg)?);
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

fn call(name: &str, args: &[Value]) -> Result<Value> {
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
