//! Running shutup pipelines: joining, grouping, and following links.

mod common;

use common::db;
use engine::{Database, ExecResult};
use serde_json::{json, Map, Value};

fn records(db: &Database, source: &str) -> Vec<Map<String, Value>> {
    let results = db.run(source)
        .unwrap_or_else(|e| panic!("failed: {e}\n--- source ---\n{source}"));
    match results.into_iter().last().expect("at least one statement") {
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

fn people_and_pets(db: &Database) {
    db.execute(
        "CREATE TABLE person (id INTEGER PRIMARY KEY, name TEXT);
         CREATE TABLE pet (id INTEGER PRIMARY KEY, owner_id INTEGER, name TEXT, species TEXT, age INTEGER);
         INSERT INTO person (id, name) VALUES (1, 'alice'), (2, 'bob'), (3, 'carol');
         INSERT INTO pet (id, owner_id, name, species, age) VALUES
           (1, 1, 'rex', 'dog', 5), (2, 1, 'mia', 'cat', 3),
           (3, 2, 'kit', 'cat', 7), (4, 2, 'sam', 'bird', 1);",
    )
    .expect("fixture");
}

/// alice(1) -> bob(2) -> carol(3) -> dave(4), plus a `follows` link 1 -> 3.
fn social(db: &Database) {
    db.execute(
        "CREATE TABLE person (id INTEGER PRIMARY KEY, name TEXT, age INTEGER);
         CREATE TABLE edges (id INTEGER PRIMARY KEY, from_id INTEGER, to_id INTEGER, label TEXT);
         INSERT INTO person (id, name, age) VALUES
           (1, 'alice', 30), (2, 'bob', 25), (3, 'carol', 35), (4, 'dave', 40);
         INSERT INTO edges (id, from_id, to_id, label) VALUES
           (1, 1, 2, 'knows'), (2, 2, 3, 'knows'), (3, 3, 4, 'knows'), (4, 1, 3, 'follows');",
    )
    .expect("fixture");
}

// --- join -------------------------------------------------------------------

#[test]
fn join_pairs_records_with_their_matches() {
    let db = db();
    people_and_pets(&db);
    let names = field(&db,
        "person | join pet on pet.owner_id == person.id | sort pet.name | show pet.name", "name");
    assert_eq!(names, vec![json!("kit"), json!("mia"), json!("rex"), json!("sam")]);
}

#[test]
fn join_drops_records_with_no_match() {
    let db = db();
    people_and_pets(&db);
    let rows = records(&db, "person | join pet on pet.owner_id == person.id");
    assert_eq!(rows.len(), 4, "carol has no pet, so she contributes nothing");
}

#[test]
fn keep_all_keeps_them_with_nothing_on_the_other_side() {
    let db = db();
    people_and_pets(&db);
    let rows = records(&db,
        "person
            join pet on pet.owner_id == person.id keep all
            sort person.id, pet.id
            show who: person.name, animal: pet.name");
    assert_eq!(rows.len(), 5);
    let last = rows.last().expect("a row");
    assert_eq!(last.get("who"), Some(&json!("carol")));
    assert_eq!(last.get("animal"), Some(&Value::Null));
}

#[test]
fn both_sides_are_reachable_by_name() {
    let db = db();
    people_and_pets(&db);
    let row = records(&db,
        "person
            where person.id == 1
            join pet on pet.owner_id == person.id
            where pet.name == \"rex\"
            show owner: person.name, pet: pet.name")
        .remove(0);
    assert_eq!(row.get("owner"), Some(&json!("alice")));
    assert_eq!(row.get("pet"), Some(&json!("rex")));
}

// --- group by ---------------------------------------------------------------

#[test]
fn group_by_folds_each_group_into_one_record() {
    let db = db();
    people_and_pets(&db);
    let rows = records(&db, "pet | group by species | show species, n: count() | sort species");
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].get("species"), Some(&json!("bird")));
    assert_eq!(rows[1].get("species"), Some(&json!("cat")));
    assert_eq!(rows[1].get("n"), Some(&json!(2)));
}

#[test]
fn the_counting_words_all_work() {
    let db = db();
    people_and_pets(&db);
    let row = records(&db,
        "pet
            group by species
            show species, n: count(), sum: total(age), mean: average(age), low: lowest(age), high: highest(age)
            where species == \"cat\"")
        .remove(0);
    assert_eq!(row.get("n"), Some(&json!(2)));
    assert_eq!(row.get("sum").and_then(Value::as_f64), Some(10.0));
    assert_eq!(row.get("mean").and_then(Value::as_f64), Some(5.0));
    assert_eq!(row.get("low").and_then(Value::as_f64), Some(3.0));
    assert_eq!(row.get("high").and_then(Value::as_f64), Some(7.0));
}

#[test]
fn where_after_the_show_filters_groups() {
    let db = db();
    people_and_pets(&db);
    let rows = records(&db, "pet | group by species | show species, n: count() | where n > 1");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("species"), Some(&json!("cat")));
}

#[test]
fn counting_the_whole_collection_needs_no_group() {
    let db = db();
    people_and_pets(&db);
    let row = records(&db, "pet | show n: count(), oldest: highest(age)").remove(0);
    assert_eq!(row.get("n"), Some(&json!(4)));
    assert_eq!(row.get("oldest").and_then(Value::as_f64), Some(7.0));
}

#[test]
fn counting_an_empty_collection_is_zero() {
    let db = db();
    db.execute("CREATE TABLE empty (id INTEGER PRIMARY KEY);").expect("fixture");
    assert_eq!(records(&db, "empty | show n: count()")[0].get("n"), Some(&json!(0)));
}

#[test]
fn a_step_other_than_show_after_group_by_is_refused() {
    let db = db();
    people_and_pets(&db);
    let message = failure(&db, "pet | group by species | where species == \"cat\"");
    assert!(message.contains("show"), "names what should come next: {message}");
}

#[test]
fn a_pipeline_cannot_end_on_a_group_by() {
    let db = db();
    people_and_pets(&db);
    failure(&db, "pet | group by species");
}

// --- follow -----------------------------------------------------------------

#[test]
fn follow_takes_one_step_along_a_link() {
    let db = db();
    social(&db);
    assert_eq!(field(&db, "person | where id == 1 | follow knows | show name", "name"),
               vec![json!("bob")]);
}

#[test]
fn follow_only_takes_links_with_that_label() {
    let db = db();
    social(&db);
    assert_eq!(field(&db, "person | where id == 1 | follow follows | show name", "name"),
               vec![json!("carol")]);
}

#[test]
fn follow_twice_goes_two_steps() {
    let db = db();
    social(&db);
    assert_eq!(field(&db, "person | where id == 1 | follow knows | follow knows | show name", "name"),
               vec![json!("carol")]);
}

#[test]
fn keep_following_goes_all_the_way() {
    let db = db();
    social(&db);
    let mut reached = field(&db, "person | where id == 1 | keep following knows | show name", "name");
    reached.sort_by_key(|v| v.as_str().unwrap_or("").to_string());
    assert_eq!(reached, vec![json!("bob"), json!("carol"), json!("dave")]);
}

#[test]
fn keep_following_stops_on_a_loop() {
    // alice -> bob -> alice. Failing this test means hanging, which is the
    // failure worth catching.
    let db = db();
    db.execute(
        "CREATE TABLE person (id INTEGER PRIMARY KEY, name TEXT);
         CREATE TABLE edges (id INTEGER PRIMARY KEY, from_id INTEGER, to_id INTEGER, label TEXT);
         INSERT INTO person (id, name) VALUES (1, 'alice'), (2, 'bob');
         INSERT INTO edges (id, from_id, to_id, label) VALUES (1, 1, 2, 'knows'), (2, 2, 1, 'knows');",
    )
    .expect("fixture");
    let reached = field(&db, "person | where id == 1 | keep following knows | show name", "name");
    assert!(!reached.is_empty() && reached.len() <= 2, "got {reached:?}");
}

#[test]
fn backward_goes_against_the_arrow() {
    let db = db();
    social(&db);
    assert_eq!(field(&db, "person | where id == 2 | follow knows backward | show name", "name"),
               vec![json!("alice")]);
}

#[test]
fn following_from_somewhere_with_no_links_finds_nothing() {
    let db = db();
    social(&db);
    assert!(records(&db, "person | where id == 4 | keep following knows").is_empty());
}

#[test]
fn what_follow_lands_on_composes_with_everything_else() {
    let db = db();
    social(&db);
    assert_eq!(field(&db,
        "person | where id == 1 | keep following knows | where age > 30 | sort age | show name",
        "name"),
        vec![json!("carol"), json!("dave")]);
}

#[test]
fn the_links_collection_stays_an_ordinary_collection() {
    let db = db();
    social(&db);
    assert_eq!(records(&db, "edges | where label == \"knows\"").len(), 3);
}

#[test]
fn following_without_a_links_collection_says_so() {
    let db = db();
    db.execute("CREATE TABLE person (id INTEGER PRIMARY KEY);").expect("fixture");
    let message = failure(&db, "person | follow knows");
    assert!(message.contains("edges"), "names what is missing: {message}");
}
