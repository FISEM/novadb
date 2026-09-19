//! Running shutup pipelines: the reading steps.

mod common;

use common::db;
use engine::{Database, ExecResult};
use serde_json::{json, Map, Value};

/// Runs shutup source, expecting every statement to succeed.
fn run(db: &Database, source: &str) -> Vec<ExecResult> {
    db.run(source).unwrap_or_else(|e| panic!("failed: {e}\n--- source ---\n{source}"))
}

/// The records produced by the last statement, which must read.
fn records(db: &Database, source: &str) -> Vec<Map<String, Value>> {
    match run(db, source).pop().expect("at least one statement") {
        ExecResult::Select { rows } => rows,
        other => panic!("expected records, got {other:?}"),
    }
}

/// One field of each record, in order.
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

/// Three people, inserted in an order that is neither by name nor by age.
fn people(db: &Database) {
    db.execute(
        "CREATE TABLE person (id INTEGER PRIMARY KEY, name TEXT, age INTEGER, dept TEXT);
         INSERT INTO person (id, name, age, dept) VALUES
           (1, 'carol', 35, 'eng'), (2, 'alice', 30, 'eng'), (3, 'bob', 25, 'sales');",
    )
    .expect("fixture");
}

// --- Sources ----------------------------------------------------------------

#[test]
fn a_bare_collection_name_reads_every_record() {
    let db = db();
    people(&db);
    assert_eq!(records(&db, "person").len(), 3);
}

#[test]
fn reading_a_collection_that_is_not_there_names_it() {
    let db = db();
    assert!(failure(&db, "ghost").contains("ghost"));
}

#[test]
fn records_carry_their_id() {
    let db = db();
    people(&db);
    assert!(records(&db, "person")[0].contains_key("id"));
}

// --- where ------------------------------------------------------------------

#[test]
fn where_keeps_matching_records() {
    let db = db();
    people(&db);
    assert_eq!(field(&db, "person | where age > 28 | sort name | show name", "name"),
               vec![json!("alice"), json!("carol")]);
}

#[test]
fn several_where_steps_stack() {
    let db = db();
    people(&db);
    assert_eq!(field(&db, "person | where age > 28 | where dept == \"eng\" | show name | sort name", "name"),
               vec![json!("alice"), json!("carol")]);
}

#[test]
fn comparisons_chain() {
    let db = db();
    people(&db);
    assert_eq!(field(&db, "person | where 28 < age < 32 | show name", "name"),
               vec![json!("alice")]);
}

#[test]
fn and_or_and_not_work() {
    let db = db();
    people(&db);
    assert_eq!(field(&db, "person | where age > 28 and dept == \"sales\"", "name"), Vec::<Value>::new());
    assert_eq!(field(&db, "person | where not (age > 28) | show name", "name"), vec![json!("bob")]);
    assert_eq!(field(&db, "person | where age < 26 or age > 34 | sort age | show name", "name"),
               vec![json!("bob"), json!("carol")]);
}

#[test]
fn in_and_not_in_work() {
    let db = db();
    people(&db);
    assert_eq!(field(&db, "person | where dept in [\"sales\"] | show name", "name"), vec![json!("bob")]);
    assert_eq!(field(&db, "person | where dept not in [\"eng\"] | show name", "name"), vec![json!("bob")]);
}

#[test]
fn is_none_finds_missing_fields() {
    let db = db();
    db.execute("CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT);
                INSERT INTO t (id, v) VALUES (1, 'x'), (2, NULL);").expect("fixture");
    assert_eq!(field(&db, "t | where v is None | show id", "id"), vec![json!(2)]);
    assert_eq!(field(&db, "t | where v is not None | show id", "id"), vec![json!(1)]);
}

#[test]
fn a_bare_field_is_true_when_it_holds_something() {
    let db = db();
    db.execute("CREATE TABLE t (id INTEGER PRIMARY KEY, nickname TEXT);
                INSERT INTO t (id, nickname) VALUES (1, 'al'), (2, ''), (3, NULL);").expect("fixture");
    assert_eq!(field(&db, "t | where nickname | show id", "id"), vec![json!(1)],
               "empty text and None are both false, as in Python");
}

// --- show -------------------------------------------------------------------

#[test]
fn show_keeps_only_the_named_fields() {
    let db = db();
    people(&db);
    let r = records(&db, "person | where id == 1 | show name").remove(0);
    assert_eq!(r.get("name"), Some(&json!("carol")));
    assert_eq!(r.get("age"), None);
}

#[test]
fn show_renames_and_computes() {
    let db = db();
    people(&db);
    let r = records(&db, "person | where id == 2 | show who: name, adult: age >= 18").remove(0);
    assert_eq!(r.get("who"), Some(&json!("alice")));
    assert_eq!(r.get("adult"), Some(&json!(true)));
}

#[test]
fn reading_a_field_a_show_dropped_is_refused_rather_than_answered_wrongly() {
    let db = db();
    people(&db);
    let message = failure(&db, "person | show name | where age > 30");
    assert!(message.contains("age"), "names the field: {message}");
    assert!(message.contains("show"), "names what dropped it: {message}");
}

#[test]
fn a_show_output_can_be_read_by_the_steps_below_it() {
    let db = db();
    people(&db);
    assert_eq!(field(&db, "person | show who: name | where who == \"bob\" | show who", "who"),
               vec![json!("bob")]);
}

// --- sort, take, skip, unique ----------------------------------------------

#[test]
fn sort_goes_up_unless_told_down() {
    let db = db();
    people(&db);
    assert_eq!(field(&db, "person | sort age | show name", "name"),
               vec![json!("bob"), json!("alice"), json!("carol")]);
    assert_eq!(field(&db, "person | sort age down | show name", "name"),
               vec![json!("carol"), json!("alice"), json!("bob")]);
}

#[test]
fn a_second_sort_key_breaks_ties() {
    let db = db();
    people(&db);
    assert_eq!(field(&db, "person | sort dept, age down | show name", "name"),
               vec![json!("carol"), json!("alice"), json!("bob")]);
}

#[test]
fn sort_can_use_a_field_a_later_show_drops() {
    let db = db();
    people(&db);
    assert_eq!(field(&db, "person | sort age | show name", "name"),
               vec![json!("bob"), json!("alice"), json!("carol")],
               "the pipeline sorts before it projects, so the question never arises");
}

#[test]
fn take_and_skip_page_through() {
    let db = db();
    people(&db);
    assert_eq!(field(&db, "person | sort age | take 2 | show name", "name"),
               vec![json!("bob"), json!("alice")]);
    assert_eq!(field(&db, "person | sort age | skip 1 | take 1 | show name", "name"),
               vec![json!("alice")]);
    assert!(records(&db, "person | take 0").is_empty());
}

#[test]
fn unique_drops_repeats() {
    let db = db();
    people(&db);
    assert_eq!(field(&db, "person | show dept | unique | sort dept", "dept"),
               vec![json!("eng"), json!("sales")]);
}

// --- Expressions ------------------------------------------------------------

#[test]
fn arithmetic_works() {
    let db = db();
    people(&db);
    let r = records(&db, "person | where id == 3 | show plus: age + 1, twice: age * 2, rest: age % 4")
        .remove(0);
    assert_eq!(r.get("plus").and_then(Value::as_f64), Some(26.0));
    assert_eq!(r.get("twice").and_then(Value::as_f64), Some(50.0));
    assert_eq!(r.get("rest").and_then(Value::as_f64), Some(1.0));
}

#[test]
fn multiplication_binds_tighter_than_addition() {
    let db = db();
    people(&db);
    let r = records(&db, "person | where id == 1 | show n: 1 + 2 * 3").remove(0);
    assert_eq!(r.get("n").and_then(Value::as_f64), Some(7.0));
}

#[test]
fn python_built_ins_work() {
    let db = db();
    people(&db);
    let r = records(&db,
        "person | where id == 2 | show n: len(name), up: name.upper(), starts: name.startswith(\"a\")")
        .remove(0);
    assert_eq!(r.get("n").and_then(Value::as_f64), Some(5.0));
    assert_eq!(r.get("up"), Some(&json!("ALICE")));
    assert_eq!(r.get("starts"), Some(&json!(true)));
}

#[test]
fn or_returns_the_first_thing_that_is_there() {
    let db = db();
    db.execute("CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT);
                INSERT INTO t (id, v) VALUES (1, NULL);").expect("fixture");
    assert_eq!(field(&db, "t | show got: v or \"fallback\"", "got"), vec![json!("fallback")],
               "no COALESCE needed");
}

#[test]
fn none_compares_like_an_ordinary_value() {
    let db = db();
    db.execute("CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT);
                INSERT INTO t (id, v) VALUES (1, NULL), (2, 'x');").expect("fixture");
    assert_eq!(field(&db, "t | where v == None | show id", "id"), vec![json!(1)],
               "None == None is true, unlike SQL");
}

#[test]
fn sorting_puts_none_first() {
    let db = db();
    db.execute("CREATE TABLE t (id INTEGER PRIMARY KEY, v INTEGER);
                INSERT INTO t (id, v) VALUES (1, 5), (2, NULL);").expect("fixture");
    assert_eq!(field(&db, "t | sort v | show id", "id"), vec![json!(2), json!(1)]);
}

// --- The indented form ------------------------------------------------------

#[test]
fn an_indented_pipeline_runs_the_same_as_a_piped_one() {
    let db = db();
    people(&db);
    let indented = field(&db, "person\n    where age > 28\n    sort name\n    show name\n", "name");
    let piped = field(&db, "person | where age > 28 | sort name | show name", "name");
    assert_eq!(indented, piped);
    assert_eq!(indented, vec![json!("alice"), json!("carol")]);
}
