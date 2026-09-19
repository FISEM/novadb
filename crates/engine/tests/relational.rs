//! The relational half of the README's promise: standard SQL, end to end.

mod common;

use common::*;
use serde_json::{json, Value};

// --- CREATE / DROP ----------------------------------------------------------

#[test]
fn create_table_then_list_it() {
    let db = db();
    run(&db, "CREATE TABLE person (id INTEGER PRIMARY KEY, name TEXT);");
    assert_eq!(db.list_tables().unwrap(), vec!["person".to_string()]);
}

#[test]
fn creating_the_same_table_twice_is_an_error() {
    let db = db();
    run(&db, "CREATE TABLE person (id INTEGER PRIMARY KEY);");
    assert!(err(&db, "CREATE TABLE person (id INTEGER PRIMARY KEY);").contains("already exists"));
}

#[test]
fn if_not_exists_makes_a_repeat_create_a_no_op() {
    let db = db();
    run(&db, "CREATE TABLE person (id INTEGER PRIMARY KEY);");
    run(&db, "CREATE TABLE IF NOT EXISTS person (id INTEGER PRIMARY KEY);");
    assert_eq!(db.list_tables().unwrap().len(), 1);
}

#[test]
fn drop_table_removes_it() {
    let db = db();
    run(&db, "CREATE TABLE person (id INTEGER PRIMARY KEY);");
    run(&db, "DROP TABLE person;");
    assert!(db.list_tables().unwrap().is_empty());
}

#[test]
fn dropping_a_missing_table_is_an_error_unless_if_exists() {
    let db = db();
    assert!(err(&db, "DROP TABLE ghost;").contains("does not exist"));
    run(&db, "DROP TABLE IF EXISTS ghost;");
}

// --- INSERT / SELECT --------------------------------------------------------

#[test]
fn insert_then_select_all() {
    let db = db();
    people(&db);
    assert_eq!(rows(&db, "SELECT * FROM person;").len(), 3);
}

#[test]
fn select_projects_only_the_named_columns() {
    let db = db();
    people(&db);
    let row = rows(&db, "SELECT name FROM person WHERE id = 1;").remove(0);
    assert_eq!(row.get("name"), Some(&json!("alice")));
    assert_eq!(row.get("age"), None, "age was not selected");
}

#[test]
fn select_from_an_unknown_table_names_the_table() {
    let db = db();
    assert!(err(&db, "SELECT * FROM ghost;").contains("ghost"));
}

#[test]
fn insert_with_the_wrong_number_of_values_is_rejected() {
    let db = db();
    people(&db);
    assert!(err(&db, "INSERT INTO person (id, name) VALUES (4);").contains("column count"));
}

#[test]
fn a_column_alias_renames_the_output() {
    let db = db();
    people(&db);
    let row = rows(&db, "SELECT name AS who FROM person WHERE id = 1;").remove(0);
    assert_eq!(row.get("who"), Some(&json!("alice")));
}

// --- WHERE ------------------------------------------------------------------

#[test]
fn where_filters_on_comparison() {
    let db = db();
    people(&db);
    assert_eq!(col(&db, "SELECT name FROM person WHERE age > 28 ORDER BY name;", "name"),
               vec![json!("alice"), json!("carol")]);
}

#[test]
fn where_combines_with_and_or_not() {
    let db = db();
    people(&db);
    assert_eq!(col(&db, "SELECT name FROM person WHERE age > 28 AND name = 'alice';", "name"),
               vec![json!("alice")]);
    assert_eq!(col(&db, "SELECT name FROM person WHERE NOT age > 28;", "name"),
               vec![json!("bob")]);
    assert_eq!(col(&db, "SELECT name FROM person WHERE age < 26 OR age > 34 ORDER BY name;", "name"),
               vec![json!("bob"), json!("carol")]);
}

#[test]
fn where_supports_in_and_between() {
    let db = db();
    people(&db);
    assert_eq!(col(&db, "SELECT name FROM person WHERE id IN (1, 3) ORDER BY id;", "name"),
               vec![json!("alice"), json!("carol")]);
    assert_eq!(col(&db, "SELECT name FROM person WHERE age BETWEEN 26 AND 31;", "name"),
               vec![json!("alice")]);
}

#[test]
fn where_supports_is_null() {
    let db = db();
    run(&db, "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT);
              INSERT INTO t (id, v) VALUES (1, 'x'), (2, NULL);");
    assert_eq!(col(&db, "SELECT id FROM t WHERE v IS NULL;", "id"), vec![json!(2)]);
    assert_eq!(col(&db, "SELECT id FROM t WHERE v IS NOT NULL;", "id"), vec![json!(1)]);
}

#[test]
fn like_matches_percent_and_underscore() {
    let db = db();
    people(&db);
    assert_eq!(col(&db, "SELECT name FROM person WHERE name LIKE 'a%';", "name"),
               vec![json!("alice")]);
    assert_eq!(col(&db, "SELECT name FROM person WHERE name LIKE 'b_b';", "name"),
               vec![json!("bob")]);
}

// --- UPDATE / DELETE --------------------------------------------------------

#[test]
fn update_changes_only_matching_rows() {
    let db = db();
    people(&db);
    run(&db, "UPDATE person SET age = 31 WHERE name = 'alice';");
    assert_eq!(col(&db, "SELECT age FROM person ORDER BY id;", "age"),
               vec![json!(31), json!(25), json!(35)]);
}

#[test]
fn update_without_a_filter_touches_every_row() {
    let db = db();
    people(&db);
    run(&db, "UPDATE person SET age = 0;");
    assert_eq!(col(&db, "SELECT age FROM person;", "age"), vec![json!(0); 3]);
}

#[test]
fn update_can_read_the_current_value() {
    let db = db();
    people(&db);
    run(&db, "UPDATE person SET age = age + 1 WHERE id = 2;");
    assert_eq!(col(&db, "SELECT age FROM person WHERE id = 2;", "age"), vec![json!(26.0)]);
}

#[test]
fn delete_removes_only_matching_rows() {
    let db = db();
    people(&db);
    run(&db, "DELETE FROM person WHERE age < 30;");
    assert_eq!(col(&db, "SELECT name FROM person ORDER BY id;", "name"),
               vec![json!("alice"), json!("carol")]);
}

#[test]
fn delete_without_a_filter_empties_the_table() {
    let db = db();
    people(&db);
    run(&db, "DELETE FROM person;");
    assert!(rows(&db, "SELECT * FROM person;").is_empty());
}

// --- ORDER BY / LIMIT / OFFSET ---------------------------------------------

#[test]
fn order_by_ascends_by_default_and_descends_on_request() {
    let db = db();
    people(&db);
    assert_eq!(col(&db, "SELECT name FROM person ORDER BY age;", "name"),
               vec![json!("bob"), json!("alice"), json!("carol")]);
    assert_eq!(col(&db, "SELECT name FROM person ORDER BY age DESC;", "name"),
               vec![json!("carol"), json!("alice"), json!("bob")]);
}

#[test]
fn limit_and_offset_page_through_the_result() {
    let db = db();
    people(&db);
    assert_eq!(col(&db, "SELECT name FROM person ORDER BY id LIMIT 2;", "name"),
               vec![json!("alice"), json!("bob")]);
    assert_eq!(col(&db, "SELECT name FROM person ORDER BY id LIMIT 2 OFFSET 1;", "name"),
               vec![json!("bob"), json!("carol")]);
    assert!(rows(&db, "SELECT * FROM person LIMIT 0;").is_empty());
}

// --- Aggregates and GROUP BY ------------------------------------------------

#[test]
fn aggregates_over_the_whole_table() {
    let db = db();
    people(&db);
    let row = rows(&db,
        "SELECT COUNT(*) AS n, SUM(age) AS total, MIN(age) AS lo, MAX(age) AS hi, AVG(age) AS mean
         FROM person;").remove(0);
    assert_eq!(row.get("n"), Some(&json!(3)));
    assert_eq!(row.get("total").and_then(Value::as_f64), Some(90.0));
    assert_eq!(row.get("lo").and_then(Value::as_f64), Some(25.0));
    assert_eq!(row.get("hi").and_then(Value::as_f64), Some(35.0));
    assert_eq!(row.get("mean").and_then(Value::as_f64), Some(30.0));
}

#[test]
fn group_by_buckets_rows_and_having_filters_the_buckets() {
    let db = db();
    run(&db, "CREATE TABLE pet (id INTEGER PRIMARY KEY, species TEXT);
              INSERT INTO pet (id, species) VALUES
                (1, 'cat'), (2, 'cat'), (3, 'dog'), (4, 'bird');");
    let grouped = rows(&db,
        "SELECT species, COUNT(*) AS n FROM pet GROUP BY species HAVING COUNT(*) > 1;");
    assert_eq!(grouped.len(), 1);
    assert_eq!(grouped[0].get("species"), Some(&json!("cat")));
    assert_eq!(grouped[0].get("n"), Some(&json!(2)));
}

#[test]
fn count_of_an_empty_table_is_zero() {
    let db = db();
    run(&db, "CREATE TABLE empty (id INTEGER PRIMARY KEY);");
    let row = rows(&db, "SELECT COUNT(*) AS n FROM empty;").remove(0);
    assert_eq!(row.get("n"), Some(&json!(0)));
}

// --- JOIN -------------------------------------------------------------------

fn people_and_pets(db: &engine::Database) {
    run(db, "CREATE TABLE person (id INTEGER PRIMARY KEY, name TEXT);
             CREATE TABLE pet (id INTEGER PRIMARY KEY, owner_id INTEGER, name TEXT);
             INSERT INTO person (id, name) VALUES (1, 'alice'), (2, 'bob'), (3, 'carol');
             INSERT INTO pet (id, owner_id, name) VALUES (1, 1, 'rex'), (2, 1, 'mia'), (3, 2, 'kit');");
}

#[test]
fn inner_join_keeps_only_matched_rows() {
    let db = db();
    people_and_pets(&db);
    let names = col(&db,
        "SELECT pet.name AS pet FROM person JOIN pet ON pet.owner_id = person.id
         ORDER BY pet.name;", "pet");
    assert_eq!(names, vec![json!("kit"), json!("mia"), json!("rex")]);
    assert_eq!(names.len(), 3, "carol has no pet, so she contributes no row");
}

#[test]
fn left_join_keeps_unmatched_left_rows_with_nulls() {
    let db = db();
    people_and_pets(&db);
    let rows = rows(&db,
        "SELECT person.name AS who, pet.name AS pet FROM person
         LEFT JOIN pet ON pet.owner_id = person.id ORDER BY person.id, pet.id;");
    assert_eq!(rows.len(), 4, "alice twice, bob once, carol once with NULL");
    let carol = rows.last().unwrap();
    assert_eq!(carol.get("who"), Some(&json!("carol")));
    assert_eq!(carol.get("pet"), Some(&Value::Null));
}

#[test]
fn a_join_can_be_filtered_and_aggregated() {
    let db = db();
    people_and_pets(&db);
    let counts = rows(&db,
        "SELECT person.name AS who, COUNT(*) AS n FROM person
         JOIN pet ON pet.owner_id = person.id GROUP BY person.name ORDER BY person.name;");
    assert_eq!(counts.len(), 2);
    assert_eq!(counts[0].get("who"), Some(&json!("alice")));
    assert_eq!(counts[0].get("n"), Some(&json!(2)));
}

// --- Set operations and CTEs ------------------------------------------------

#[test]
fn union_deduplicates_and_union_all_does_not() {
    let db = db();
    run(&db, "CREATE TABLE a (id INTEGER PRIMARY KEY, v INTEGER);
              CREATE TABLE b (id INTEGER PRIMARY KEY, v INTEGER);
              INSERT INTO a (id, v) VALUES (1, 1), (2, 2);
              INSERT INTO b (id, v) VALUES (1, 2), (2, 3);");
    assert_eq!(rows(&db, "SELECT v FROM a UNION SELECT v FROM b;").len(), 3);
    assert_eq!(rows(&db, "SELECT v FROM a UNION ALL SELECT v FROM b;").len(), 4);
}

#[test]
fn a_cte_can_be_selected_from() {
    let db = db();
    people(&db);
    assert_eq!(col(&db,
        "WITH adults AS (SELECT name FROM person WHERE age >= 30)
         SELECT name FROM adults ORDER BY name;", "name"),
        vec![json!("alice"), json!("carol")]);
}

#[test]
fn a_recursive_cte_terminates_and_counts() {
    let db = db();
    run(&db, "CREATE TABLE seed (id INTEGER PRIMARY KEY, n INTEGER);
              INSERT INTO seed (id, n) VALUES (1, 1);");
    let out = rows(&db,
        "WITH RECURSIVE counter AS (
           SELECT n FROM seed
           UNION ALL
           SELECT n + 1 AS n FROM counter WHERE n < 5
         )
         SELECT n FROM counter ORDER BY n;");
    assert_eq!(out.len(), 5, "1 through 5");
}

// --- Scalar functions -------------------------------------------------------

#[test]
fn string_functions_transform_values() {
    let db = db();
    people(&db);
    let row = rows(&db,
        "SELECT UPPER(name) AS up, LOWER('MIA') AS down, LENGTH(name) AS len,
                CONCAT(name, '!') AS shout
         FROM person WHERE id = 1;").remove(0);
    assert_eq!(row.get("up"), Some(&json!("ALICE")));
    assert_eq!(row.get("down"), Some(&json!("mia")));
    assert_eq!(row.get("len").and_then(Value::as_f64), Some(5.0));
    assert_eq!(row.get("shout"), Some(&json!("alice!")));
}

#[test]
fn numeric_functions_transform_values() {
    let db = db();
    people(&db);
    let row = rows(&db,
        "SELECT ABS(0 - age) AS positive, ROUND(2.7) AS rounded FROM person WHERE id = 1;").remove(0);
    assert_eq!(row.get("positive").and_then(Value::as_f64), Some(30.0));
    assert_eq!(row.get("rounded").and_then(Value::as_f64), Some(3.0));
}

#[test]
fn coalesce_returns_the_first_non_null() {
    let db = db();
    run(&db, "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT);
              INSERT INTO t (id, v) VALUES (1, NULL);");
    assert_eq!(col(&db, "SELECT COALESCE(v, 'fallback') AS got FROM t;", "got"),
               vec![json!("fallback")]);
}

// --- Arithmetic -------------------------------------------------------------

#[test]
fn arithmetic_evaluates_in_the_select_list() {
    let db = db();
    people(&db);
    let row = rows(&db,
        "SELECT age + 1 AS plus, age - 1 AS minus, age * 2 AS twice FROM person WHERE id = 2;")
        .remove(0);
    assert_eq!(row.get("plus").and_then(Value::as_f64), Some(26.0));
    assert_eq!(row.get("minus").and_then(Value::as_f64), Some(24.0));
    assert_eq!(row.get("twice").and_then(Value::as_f64), Some(50.0));
}

// --- Multiple statements ----------------------------------------------------

#[test]
fn a_batch_returns_one_result_per_statement() {
    let db = db();
    let results = run(&db,
        "CREATE TABLE t (id INTEGER PRIMARY KEY);
         INSERT INTO t (id) VALUES (1);
         SELECT * FROM t;");
    assert_eq!(results.len(), 3);
}
