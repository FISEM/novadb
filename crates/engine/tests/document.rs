//! Schemaless tables — the README's "MongoDB-style collection" claim.

mod common;

use common::*;
use serde_json::{json, Value};

#[test]
fn a_table_can_be_created_with_no_columns_at_all() {
    let db = db();
    run(&db, "CREATE TABLE session;");
    assert_eq!(db.list_tables().unwrap(), vec!["session".to_string()]);
}

#[test]
fn rows_of_different_shapes_coexist_in_one_collection() {
    let db = db();
    run(&db, "CREATE TABLE session;
              INSERT INTO session (user_id, device) VALUES (1, 'mobile');
              INSERT INTO session (payload, ts) VALUES ('{\"foo\": \"bar\"}', 12345);");
    let rows = rows(&db, "SELECT * FROM session;");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].get("device"), Some(&json!("mobile")));
    assert_eq!(rows[0].get("payload"), None, "the first row never had a payload");
    assert_eq!(rows[1].get("ts"), Some(&json!(12345)));
    assert_eq!(rows[1].get("user_id"), None, "the second row never had a user_id");
}

#[test]
fn a_schemaless_row_still_gets_an_id() {
    let db = db();
    run(&db, "CREATE TABLE session;
              INSERT INTO session (device) VALUES ('mobile');");
    let row = rows(&db, "SELECT * FROM session;").remove(0);
    assert!(row.get("id").is_some(), "the engine assigns an id: {row:?}");
}

#[test]
fn schemaless_rows_can_be_filtered_and_ordered_like_any_table() {
    let db = db();
    run(&db, "CREATE TABLE event;
              INSERT INTO event (kind, n) VALUES ('click', 3);
              INSERT INTO event (kind, n) VALUES ('view', 1);
              INSERT INTO event (kind, n) VALUES ('click', 2);");
    assert_eq!(col(&db, "SELECT n FROM event WHERE kind = 'click' ORDER BY n;", "n"),
               vec![json!(2), json!(3)]);
}

#[test]
fn a_field_absent_from_a_row_reads_as_null() {
    let db = db();
    run(&db, "CREATE TABLE session;
              INSERT INTO session (a) VALUES (1);
              INSERT INTO session (b) VALUES (2);");
    assert_eq!(col(&db, "SELECT a FROM session;", "a"), vec![json!(1), Value::Null]);
}

#[test]
fn undeclared_columns_are_accepted_on_a_typed_table_too() {
    // The README is explicit that this is intentional, and that a typed
    // CREATE TABLE is a convention rather than an enforced constraint.
    // Pinning it here means the day that changes, it changes on purpose.
    let db = db();
    run(&db, "CREATE TABLE person (id INTEGER PRIMARY KEY, name TEXT);
              INSERT INTO person (id, name, nickname) VALUES (1, 'alice', 'al');");
    let row = rows(&db, "SELECT * FROM person;").remove(0);
    assert_eq!(row.get("nickname"), Some(&json!("al")));
}

#[test]
fn insert_without_a_column_list_uses_the_declared_schema() {
    let db = db();
    run(&db, "CREATE TABLE person (id INTEGER PRIMARY KEY, name TEXT);
              INSERT INTO person VALUES (1, 'alice');");
    let row = rows(&db, "SELECT * FROM person;").remove(0);
    assert_eq!(row.get("name"), Some(&json!("alice")));
}

#[test]
fn a_json_string_survives_a_round_trip() {
    let db = db();
    run(&db, "CREATE TABLE doc;
              INSERT INTO doc (payload) VALUES ('{\"items\": 3}');");
    assert_eq!(col(&db, "SELECT payload FROM doc;", "payload"), vec![json!("{\"items\": 3}")]);
}

#[test]
fn a_schemaless_table_can_be_updated_and_deleted_from() {
    let db = db();
    run(&db, "CREATE TABLE session;
              INSERT INTO session (device, n) VALUES ('mobile', 1), ('desktop', 2);");
    run(&db, "UPDATE session SET device = 'tablet' WHERE n = 1;");
    assert_eq!(col(&db, "SELECT device FROM session ORDER BY n;", "device"),
               vec![json!("tablet"), json!("desktop")]);
    run(&db, "DELETE FROM session WHERE n = 2;");
    assert_eq!(rows(&db, "SELECT * FROM session;").len(), 1);
}
