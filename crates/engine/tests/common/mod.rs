//! Helpers shared by the engine's integration tests.
//!
//! Every test drives the engine exactly the way the HTTP server does:
//! text in, `ExecResult` out, against a fresh in-memory database.

#![allow(dead_code)]

use engine::{Database, ExecResult};
use serde_json::{Map, Value};

/// A fresh, empty database. No filesystem, so tests stay independent.
pub fn db() -> Database {
    Database::open_in_memory().expect("in-memory database")
}

/// Runs `sql`, expecting every statement to succeed.
pub fn run(db: &Database, sql: &str) -> Vec<ExecResult> {
    db.execute(sql).unwrap_or_else(|e| panic!("statement failed: {e}\n--- sql ---\n{sql}"))
}

/// Runs `sql` and returns the rows of its final statement, which must be a SELECT.
pub fn rows(db: &Database, sql: &str) -> Vec<Map<String, Value>> {
    match run(db, sql).pop().expect("at least one statement") {
        ExecResult::Select { rows } => rows,
        other => panic!("expected a SELECT result, got {other:?}"),
    }
}

/// The values of one column, in row order.
pub fn col(db: &Database, sql: &str, name: &str) -> Vec<Value> {
    rows(db, sql)
        .into_iter()
        .map(|r| r.get(name).cloned().unwrap_or(Value::Null))
        .collect()
}

/// The error message produced by `sql`, which must fail.
pub fn err(db: &Database, sql: &str) -> String {
    match db.execute(sql) {
        Ok(ok) => panic!("expected an error, got {ok:?}\n--- sql ---\n{sql}"),
        Err(e) => e.to_string(),
    }
}

/// A `person` table with three rows, used by most tests.
pub fn people(db: &Database) {
    run(
        db,
        "CREATE TABLE person (id INTEGER PRIMARY KEY, name TEXT, age INTEGER);
         INSERT INTO person (id, name, age) VALUES
           (1, 'alice', 30), (2, 'bob', 25), (3, 'carol', 35);",
    );
}
