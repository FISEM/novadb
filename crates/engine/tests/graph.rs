//! Graph traversal — the `>` sugar that desugars to JOIN / WITH RECURSIVE.

mod common;

use common::*;
use serde_json::json;

/// alice(1) -> bob(2) -> carol(3) -> dave(4), plus a `follows` edge 1 -> 3.
fn social(db: &engine::Database) {
    run(db, "CREATE TABLE person (id INTEGER PRIMARY KEY, name TEXT);
             CREATE TABLE edges (id INTEGER PRIMARY KEY, from_id INTEGER, to_id INTEGER, label TEXT);
             INSERT INTO person (id, name) VALUES
               (1, 'alice'), (2, 'bob'), (3, 'carol'), (4, 'dave');
             INSERT INTO edges (id, from_id, to_id, label) VALUES
               (1, 1, 2, 'knows'), (2, 2, 3, 'knows'), (3, 3, 4, 'knows'), (4, 1, 3, 'follows');");
}

#[test]
fn a_single_hop_follows_one_edge() {
    let db = db();
    social(&db);
    assert_eq!(col(&db,
        "SELECT p2.name AS name FROM person p1 > knows > person p2 WHERE p1.id = 1;", "name"),
        vec![json!("bob")]);
}

#[test]
fn a_hop_only_follows_edges_with_that_label() {
    let db = db();
    social(&db);
    assert_eq!(col(&db,
        "SELECT p2.name AS name FROM person p1 > follows > person p2 WHERE p1.id = 1;", "name"),
        vec![json!("carol")]);
}

#[test]
fn chained_hops_walk_a_fixed_depth() {
    let db = db();
    social(&db);
    assert_eq!(col(&db,
        "SELECT p3.name AS name FROM person p1 > knows > person p2 > knows > person p3
         WHERE p1.id = 1;", "name"),
        vec![json!("carol")]);
}

#[test]
fn a_starred_hop_reaches_any_depth() {
    let db = db();
    social(&db);
    let mut reached = col(&db,
        "SELECT p2.name AS name FROM person p1 > knows* > person p2 WHERE p1.id = 1;", "name");
    reached.sort_by_key(|v| v.as_str().unwrap_or("").to_string());
    assert_eq!(reached, vec![json!("bob"), json!("carol"), json!("dave")]);
}

#[test]
fn a_starred_hop_terminates_on_a_cycle() {
    // alice -> bob -> alice. A naive traversal would loop forever; this test
    // fails by hanging, which is exactly the failure worth catching.
    let db = db();
    run(&db, "CREATE TABLE person (id INTEGER PRIMARY KEY, name TEXT);
              CREATE TABLE edges (id INTEGER PRIMARY KEY, from_id INTEGER, to_id INTEGER, label TEXT);
              INSERT INTO person (id, name) VALUES (1, 'alice'), (2, 'bob');
              INSERT INTO edges (id, from_id, to_id, label) VALUES
                (1, 1, 2, 'knows'), (2, 2, 1, 'knows');");
    let reached = col(&db,
        "SELECT p2.name AS name FROM person p1 > knows* > person p2 WHERE p1.id = 1;", "name");
    assert!(!reached.is_empty(), "bob is reachable");
    assert!(reached.len() <= 2, "at most two people exist, got {reached:?}");
}

#[test]
fn a_starred_hop_from_an_isolated_node_returns_nothing() {
    let db = db();
    social(&db);
    assert!(rows(&db,
        "SELECT p2.name AS name FROM person p1 > knows* > person p2 WHERE p1.id = 4;").is_empty());
}

#[test]
fn a_traversal_can_mix_variable_and_fixed_hops() {
    let db = db();
    social(&db);
    let reached = col(&db,
        "SELECT p3.name AS name FROM person p1 > knows* > person p2 > follows > person p3
         WHERE p1.id = 1;", "name");
    assert!(reached.is_empty() || reached.contains(&json!("carol")),
            "only alice has a follows edge, got {reached:?}");
}

#[test]
fn a_traversal_result_can_be_filtered_and_ordered() {
    let db = db();
    social(&db);
    assert_eq!(col(&db,
        "SELECT p2.name AS name FROM person p1 > knows* > person p2
         WHERE p1.id = 1 AND p2.id > 2 ORDER BY p2.id;", "name"),
        vec![json!("carol"), json!("dave")]);
}

#[test]
fn the_edges_table_stays_an_ordinary_table() {
    // The README's promise: no hidden graph storage, any SQL tool can read it.
    let db = db();
    social(&db);
    assert_eq!(rows(&db, "SELECT * FROM edges WHERE label = 'knows';").len(), 3);
}
