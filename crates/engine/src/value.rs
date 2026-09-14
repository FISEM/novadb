use serde_json::{Number, Value};
use sql::ast::Literal;

pub fn literal_to_value(lit: &Literal) -> Value {
    match lit {
        Literal::Null => Value::Null,
        Literal::Bool(b) => Value::Bool(*b),
        Literal::Int(i) => Value::Number((*i).into()),
        Literal::Float(f) => Number::from_f64(*f).map(Value::Number).unwrap_or(Value::Null),
        Literal::Text(s) => Value::String(s.clone()),
    }
}

pub fn is_truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Null => false,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

pub fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

pub fn compare_values(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    use std::cmp::Ordering;
    match (a, b) {
        (Value::Null, Value::Null) => Some(Ordering::Equal),
        (Value::Null, _) => Some(Ordering::Less),
        (_, Value::Null) => Some(Ordering::Greater),
        (Value::Bool(x), Value::Bool(y)) => x.partial_cmp(y),
        (Value::String(x), Value::String(y)) => x.partial_cmp(y),
        _ => {
            let (x, y) = (as_f64(a)?, as_f64(b)?);
            x.partial_cmp(&y)
        }
    }
}

pub fn values_equal(a: &Value, b: &Value) -> bool {
    if a == b {
        return true;
    }
    compare_values(a, b) == Some(std::cmp::Ordering::Equal)
}

pub fn like_match(text: &str, pattern: &str) -> bool {
    // Translate SQL LIKE pattern (% and _) into a simple matcher.
    fn helper(t: &[u8], p: &[u8]) -> bool {
        match p.first() {
            None => t.is_empty(),
            Some(b'%') => helper(t, &p[1..]) || (!t.is_empty() && helper(&t[1..], p)),
            Some(b'_') => !t.is_empty() && helper(&t[1..], &p[1..]),
            Some(c) => !t.is_empty() && t[0].eq_ignore_ascii_case(c) && helper(&t[1..], &p[1..]),
        }
    }
    helper(text.as_bytes(), pattern.as_bytes())
}
