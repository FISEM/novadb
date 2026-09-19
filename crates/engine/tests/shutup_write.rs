//! Running shutup statements that change things.

mod common;

use common::db;
use engine::{Database, ExecResult};
use serde_json::{json, Map, Value};

fn run(db: &Database, source: &str) -> Vec<ExecResult> {
    db.run(source).unwrap_or_else(|e| panic!("failed: {e}\n--- source ---\n{source}"))
}

fn records(db: &Database, source: &str) -> Vec<Map<String, Value>> {
    match run(db, source).pop().expect("at least one statement") {
        ExecResult::Select { rows } => rows,
        other => panic!("expected records, got {other:?}"),
    }
}

fn field(db: &Database, source: &str, name: &str) -> Vec<Value> {
    records(db, source)
        .into_iter()
        .map(|r| r.get(name).cloned().unwrap_or(Value::Null))
        .collect()
}

fn failure(db: &Database, source: &str) -> String {
    match db.run(source) {
        Ok(ok) => panic!("expected a failure, got {ok:?}\n--- source ---\n{source}"),
        Err(e) => e.to_string(),
    }
}

// --- define -----------------------------------------------------------------

#[test]
fn define_makes_a_collection() {
    let db = db();
    run(&db, "define person { id: number key, name: string }");
    assert_eq!(db.list_tables().unwrap(), vec!["person".to_string()]);
}

#[test]
fn define_indented_makes_the_same_collection() {
    let db = db();
    run(&db, "define person\n    id: number key\n    name: string\n    age: number?\n");
    assert_eq!(db.list_tables().unwrap(), vec!["person".to_string()]);
}

#[test]
fn define_with_no_body_makes_a_collection_that_claims_nothing() {
    let db = db();
    run(&db, "define session");
    run(&db, "add session { device: \"mobile\" }");
    run(&db, "add session { payload: \"{}\", ts: 12345 }");
    let rows = records(&db, "session");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].get("device"), Some(&json!("mobile")));
    assert_eq!(rows[0].get("payload"), None, "the first record never had one");
}

#[test]
fn defining_the_same_collection_twice_says_so() {
    let db = db();
    run(&db, "define person { id: number key }");
    assert!(failure(&db, "define person { id: number key }").contains("person"));
}

#[test]
fn a_shape_is_a_claim_and_not_a_constraint() {
    // The reference is explicit that nothing rejects a record that disagrees.
    // Pinning it means the day it changes, it changes on purpose.
    let db = db();
    run(&db, "define person { id: number key, name: string }");
    run(&db, "add person { id: 1, name: \"alice\", nickname: \"al\" }");
    assert_eq!(field(&db, "person | show nickname", "nickname"), vec![json!("al")]);
}

// --- add --------------------------------------------------------------------

#[test]
fn add_puts_a_record_in() {
    let db = db();
    run(&db, "define person { id: number key, name: string }");
    run(&db, "add person { id: 1, name: \"alice\" }");
    assert_eq!(field(&db, "person | show name", "name"), vec![json!("alice")]);
}

#[test]
fn add_indented_puts_the_same_record_in() {
    let db = db();
    run(&db, "define person { id: number key, name: string }");
    run(&db, "add person\n    id: 1\n    name: \"alice\"\n");
    assert_eq!(field(&db, "person | show name", "name"), vec![json!("alice")]);
}

#[test]
fn add_reports_what_it_put_in() {
    let db = db();
    run(&db, "define person { id: number key }");
    match run(&db, "add person { id: 1 }").remove(0) {
        ExecResult::Inserted { ids } => assert_eq!(ids.len(), 1),
        other => panic!("expected an inserted, got {other:?}"),
    }
}

#[test]
fn adding_to_a_collection_that_is_not_there_names_it() {
    let db = db();
    assert!(failure(&db, "add ghost { id: 1 }").contains("ghost"));
}

#[test]
fn a_value_in_add_can_be_worked_out() {
    let db = db();
    run(&db, "define t { id: number key, n: number }");
    run(&db, "add t { id: 1, n: 2 * 3 }");
    assert_eq!(field(&db, "t | show n", "n"), vec![json!(6)]);
}

// --- set --------------------------------------------------------------------

fn people(db: &Database) {
    run(db, "define person { id: number key, name: string, age: number }");
    run(db, "add person { id: 1, name: \"alice\", age: 30 }");
    run(db, "add person { id: 2, name: \"bob\", age: 25 }");
    run(db, "add person { id: 3, name: \"carol\", age: 35 }");
}

#[test]
fn set_changes_only_the_records_that_reached_it() {
    let db = db();
    people(&db);
    run(&db, "person | where name == \"alice\" | set age = 31");
    assert_eq!(field(&db, "person | sort id | show age", "age"),
               vec![json!(31), json!(25), json!(35)]);
}

#[test]
fn set_can_read_the_value_it_is_replacing() {
    let db = db();
    people(&db);
    run(&db, "person | where id == 2 | set age = age + 1");
    assert_eq!(field(&db, "person | where id == 2 | show age", "age"), vec![json!(26)]);
}

#[test]
fn set_changes_several_fields_at_once() {
    let db = db();
    people(&db);
    run(&db, "person | where id == 1 | set age = 31, seen = True");
    let row = records(&db, "person | where id == 1 | show age, seen").remove(0);
    assert_eq!(row.get("age"), Some(&json!(31)));
    assert_eq!(row.get("seen"), Some(&json!(true)));
}

#[test]
fn set_with_nothing_above_it_touches_every_record() {
    let db = db();
    people(&db);
    run(&db, "person | set age = 0");
    assert_eq!(field(&db, "person | show age", "age"), vec![json!(0); 3]);
}

#[test]
fn set_reports_how_many_it_changed() {
    let db = db();
    people(&db);
    match run(&db, "person | where age > 28 | set age = 0").remove(0) {
        ExecResult::Updated { count } => assert_eq!(count, 2),
        other => panic!("expected an updated, got {other:?}"),
    }
}

// --- delete -----------------------------------------------------------------

#[test]
fn delete_removes_only_the_records_that_reached_it() {
    let db = db();
    people(&db);
    run(&db, "person | where age < 30 | delete");
    assert_eq!(field(&db, "person | sort id | show name", "name"),
               vec![json!("alice"), json!("carol")]);
}

#[test]
fn delete_with_nothing_above_it_empties_the_collection() {
    let db = db();
    people(&db);
    run(&db, "person | delete");
    assert!(records(&db, "person").is_empty());
    assert_eq!(db.list_tables().unwrap(), vec!["person".to_string()],
               "the collection is still there, it is just empty");
}

#[test]
fn delete_reports_how_many_it_removed() {
    let db = db();
    people(&db);
    match run(&db, "person | where age < 30 | delete").remove(0) {
        ExecResult::Deleted { count } => assert_eq!(count, 1),
        other => panic!("expected a deleted, got {other:?}"),
    }
}

#[test]
fn dropping_the_last_step_previews_what_it_would_have_done() {
    let db = db();
    people(&db);
    let about_to_go = field(&db, "person | where age < 30 | show name", "name");
    run(&db, "person | where age < 30 | delete");
    let left = field(&db, "person | show name", "name");
    assert_eq!(about_to_go, vec![json!("bob")]);
    assert!(!left.contains(&json!("bob")));
}

// --- Writing after the records stopped being the collection's ---------------

#[test]
fn set_after_a_show_is_refused_rather_than_guessed_at() {
    let db = db();
    people(&db);
    let message = failure(&db, "person | show name | set name = \"x\"");
    assert!(message.contains("show") || message.contains("record"),
            "says why it cannot: {message}");
}

#[test]
fn delete_after_a_join_is_refused() {
    let db = db();
    people(&db);
    run(&db, "define pet { id: number key, owner_id: number }");
    run(&db, "add pet { id: 1, owner_id: 1 }");
    failure(&db, "person | join pet on pet.owner_id == person.id | delete");
}

// --- remove -----------------------------------------------------------------

#[test]
fn remove_throws_a_collection_away() {
    let db = db();
    people(&db);
    run(&db, "remove person");
    assert!(db.list_tables().unwrap().is_empty());
}

#[test]
fn removing_something_that_is_not_there_says_so_unless_told_not_to() {
    let db = db();
    assert!(failure(&db, "remove ghost").contains("ghost"));
    run(&db, "remove ghost if exists");
}

// --- Several statements -----------------------------------------------------

#[test]
fn a_run_of_statements_returns_one_result_each() {
    let db = db();
    let results = run(&db,
        "define person { id: number key, name: string }\n\
         add person { id: 1, name: \"alice\" }\n\
         person | show name\n");
    assert_eq!(results.len(), 3);
}

#[test]
fn writes_survive_being_written() {
    let db = db();
    people(&db);
    run(&db, "person | where id == 1 | set age = 99");
    run(&db, "person | where id == 3 | delete");
    assert_eq!(field(&db, "person | sort id | show age", "age"), vec![json!(99), json!(25)]);
}
