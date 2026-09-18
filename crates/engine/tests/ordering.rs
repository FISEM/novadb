//! ORDER BY resolution.
//!
//! Regression tests for a bug where ORDER BY was evaluated against the
//! *projected* row: any sort key the projection had dropped or renamed
//! silently resolved to NULL, so the rows came back in insertion order with
//! no error at all. Wrong answers, quietly.

mod common;

use common::*;
use serde_json::json;

fn people(db: &engine::Database) {
    // Inserted in an order that is neither by name nor by age, so a no-op
    // sort cannot accidentally look correct.
    run(db, "CREATE TABLE person (id INTEGER PRIMARY KEY, name TEXT, age INTEGER);
             INSERT INTO person (id, name, age) VALUES
               (1, 'carol', 35), (2, 'alice', 30), (3, 'bob', 25);");
}

#[test]
fn order_by_a_column_that_is_selected() {
    let db = db();
    people(&db);
    assert_eq!(col(&db, "SELECT name, age FROM person ORDER BY age;", "name"),
               vec![json!("bob"), json!("alice"), json!("carol")]);
}

#[test]
fn order_by_a_column_the_projection_dropped() {
    let db = db();
    people(&db);
    assert_eq!(col(&db, "SELECT name FROM person ORDER BY age;", "name"),
               vec![json!("bob"), json!("alice"), json!("carol")]);
}

#[test]
fn order_by_an_output_alias() {
    let db = db();
    people(&db);
    assert_eq!(col(&db, "SELECT name AS who FROM person ORDER BY who;", "who"),
               vec![json!("alice"), json!("bob"), json!("carol")]);
}

#[test]
fn an_output_alias_shadows_a_source_column_of_the_same_name() {
    // SQL resolves an ORDER BY name against the output list first: `name`
    // here means the aliased age, so the rows sort numerically.
    let db = db();
    people(&db);
    assert_eq!(col(&db, "SELECT age AS name FROM person ORDER BY name;", "name"),
               vec![json!(25), json!(30), json!(35)]);
}

#[test]
fn order_by_a_qualified_column_the_projection_renamed() {
    let db = db();
    run(&db, "CREATE TABLE person (id INTEGER PRIMARY KEY, name TEXT);
              CREATE TABLE pet (id INTEGER PRIMARY KEY, owner_id INTEGER, name TEXT);
              INSERT INTO person (id, name) VALUES (1, 'alice'), (2, 'bob');
              INSERT INTO pet (id, owner_id, name) VALUES (1, 1, 'rex'), (2, 2, 'kit'), (3, 1, 'mia');");
    assert_eq!(col(&db,
        "SELECT pet.name AS pet FROM person JOIN pet ON pet.owner_id = person.id
         ORDER BY pet.name;", "pet"),
        vec![json!("kit"), json!("mia"), json!("rex")]);
}

#[test]
fn order_by_an_expression_over_a_dropped_column() {
    let db = db();
    people(&db);
    assert_eq!(col(&db, "SELECT name FROM person ORDER BY age * -1;", "name"),
               vec![json!("carol"), json!("alice"), json!("bob")]);
}

#[test]
fn order_by_desc_reverses_a_dropped_column_too() {
    let db = db();
    people(&db);
    assert_eq!(col(&db, "SELECT name FROM person ORDER BY age DESC;", "name"),
               vec![json!("carol"), json!("alice"), json!("bob")]);
}

#[test]
fn a_second_order_key_breaks_ties() {
    let db = db();
    run(&db, "CREATE TABLE t (id INTEGER PRIMARY KEY, grp TEXT, n INTEGER);
              INSERT INTO t (id, grp, n) VALUES (1, 'b', 2), (2, 'a', 2), (3, 'a', 1);");
    assert_eq!(col(&db, "SELECT id FROM t ORDER BY grp, n;", "id"),
               vec![json!(3), json!(2), json!(1)]);
}

#[test]
fn order_by_applies_before_limit() {
    let db = db();
    people(&db);
    assert_eq!(col(&db, "SELECT name FROM person ORDER BY age LIMIT 1;", "name"),
               vec![json!("bob")], "the youngest, not the first inserted");
}

#[test]
fn distinct_survives_ordering_by_a_dropped_column() {
    let db = db();
    run(&db, "CREATE TABLE t (id INTEGER PRIMARY KEY, grp TEXT, n INTEGER);
              INSERT INTO t (id, grp, n) VALUES (1, 'b', 3), (2, 'a', 1), (3, 'b', 2);");
    let got = col(&db, "SELECT DISTINCT grp FROM t ORDER BY grp;", "grp");
    assert_eq!(got, vec![json!("a"), json!("b")]);
}

#[test]
fn ordering_a_union_uses_its_output_columns() {
    let db = db();
    run(&db, "CREATE TABLE a (id INTEGER PRIMARY KEY, v INTEGER);
              CREATE TABLE b (id INTEGER PRIMARY KEY, v INTEGER);
              INSERT INTO a (id, v) VALUES (1, 3);
              INSERT INTO b (id, v) VALUES (1, 1), (2, 2);");
    assert_eq!(col(&db, "SELECT v FROM a UNION ALL SELECT v FROM b ORDER BY v;", "v"),
               vec![json!(1), json!(2), json!(3)]);
}
