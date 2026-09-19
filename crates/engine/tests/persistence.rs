//! Durability: what a file-backed database still holds after it is reopened.

mod common;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use common::*;
use engine::Database;
use serde_json::json;

/// A unique temp path that deletes itself when the test ends, pass or panic.
struct TempDb(PathBuf);

impl TempDb {
    fn new(tag: &str) -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir()
            .join(format!("novadb-test-{}-{tag}-{n}.redb", std::process::id()));
        let _ = std::fs::remove_file(&path);
        TempDb(path)
    }

    fn open(&self) -> Database {
        Database::open(&self.0).expect("open the database file")
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn rows_survive_closing_and_reopening() {
    let file = TempDb::new("rows");
    {
        let db = file.open();
        run(&db, "CREATE TABLE person (id INTEGER PRIMARY KEY, name TEXT);
                  INSERT INTO person (id, name) VALUES (1, 'alice'), (2, 'bob');");
    }
    let db = file.open();
    assert_eq!(col(&db, "SELECT name FROM person ORDER BY id;", "name"),
               vec![json!("alice"), json!("bob")]);
}

#[test]
fn the_schema_survives_a_reopen() {
    let file = TempDb::new("schema");
    {
        let db = file.open();
        run(&db, "CREATE TABLE person (id INTEGER PRIMARY KEY, name TEXT);");
    }
    let db = file.open();
    assert_eq!(db.list_tables().unwrap(), vec!["person".to_string()]);
    // The table is known, so a bare INSERT can still use the declared order.
    run(&db, "INSERT INTO person VALUES (1, 'alice');");
    assert_eq!(col(&db, "SELECT name FROM person;", "name"), vec![json!("alice")]);
}

#[test]
fn a_dropped_table_stays_dropped() {
    let file = TempDb::new("dropped");
    {
        let db = file.open();
        run(&db, "CREATE TABLE person (id INTEGER PRIMARY KEY);
                  DROP TABLE person;");
    }
    let db = file.open();
    assert!(db.list_tables().unwrap().is_empty());
}

#[test]
fn ids_keep_climbing_after_a_reopen() {
    // A sequence that restarts would overwrite existing rows.
    let file = TempDb::new("ids");
    {
        let db = file.open();
        run(&db, "CREATE TABLE t (id INTEGER PRIMARY KEY, v INTEGER);
                  INSERT INTO t (v) VALUES (1), (2);");
    }
    let db = file.open();
    run(&db, "INSERT INTO t (v) VALUES (3);");
    assert_eq!(rows(&db, "SELECT * FROM t;").len(), 3, "the third row did not overwrite a first one");
}

#[test]
fn updates_and_deletes_persist() {
    let file = TempDb::new("mutations");
    {
        let db = file.open();
        run(&db, "CREATE TABLE t (id INTEGER PRIMARY KEY, v INTEGER);
                  INSERT INTO t (id, v) VALUES (1, 1), (2, 2), (3, 3);
                  UPDATE t SET v = 99 WHERE id = 1;
                  DELETE FROM t WHERE id = 3;");
    }
    let db = file.open();
    assert_eq!(col(&db, "SELECT v FROM t ORDER BY id;", "v"), vec![json!(99), json!(2)]);
}

#[test]
fn two_databases_do_not_share_state() {
    let a = TempDb::new("iso-a");
    let b = TempDb::new("iso-b");
    let da = a.open();
    run(&da, "CREATE TABLE only_in_a (id INTEGER PRIMARY KEY);");
    let db_ = b.open();
    assert!(db_.list_tables().unwrap().is_empty());
}

#[test]
fn an_in_memory_database_starts_empty_every_time() {
    assert!(db().list_tables().unwrap().is_empty());
    assert!(db().list_tables().unwrap().is_empty());
}
