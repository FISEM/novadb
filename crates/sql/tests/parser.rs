//! Parser and lexer contract: what the grammar accepts, and what it refuses.

use sql::ast::*;
use sql::parse_statements;

fn parse(text: &str) -> Vec<Statement> {
    parse_statements(text).unwrap_or_else(|e| panic!("parse failed: {e}\n--- sql ---\n{text}"))
}

fn error(text: &str) -> String {
    match parse_statements(text) {
        Ok(ast) => panic!("expected a parse error, got {ast:?}\n--- sql ---\n{text}"),
        Err(e) => e.to_string(),
    }
}

fn select_of(stmt: &Statement) -> &SelectStmt {
    match stmt {
        Statement::Select(s) => s,
        other => panic!("expected a SELECT, got {other:?}"),
    }
}

fn core_of(stmt: &Statement) -> &SelectCore {
    match &select_of(stmt).body {
        SetExpr::Select(core) => core,
        other => panic!("expected a plain SELECT body, got {other:?}"),
    }
}

// --- Statement boundaries ---------------------------------------------------

#[test]
fn a_semicolon_separates_statements() {
    assert_eq!(parse("SELECT 1; SELECT 2;").len(), 2);
}

#[test]
fn a_trailing_semicolon_is_optional() {
    assert_eq!(parse("SELECT 1").len(), 1);
}

#[test]
fn empty_input_parses_to_no_statements() {
    assert!(parse("").is_empty());
    assert!(parse("   \n  ").is_empty());
}

#[test]
fn line_comments_are_ignored() {
    assert_eq!(parse("-- a note\nSELECT 1;").len(), 1);
}

// --- Literals and quoting ---------------------------------------------------

#[test]
fn single_quotes_make_a_string() {
    let core = parse("SELECT 'text';");
    match &core_of(&core[0]).projection[0] {
        SelectItem::Expr { expr: Expr::Literal(Literal::Text(s)), .. } => assert_eq!(s, "text"),
        other => panic!("expected a text literal, got {other:?}"),
    }
}

#[test]
fn double_quotes_make_an_identifier_not_a_string() {
    // The README calls this out explicitly as the trap for newcomers.
    let core = parse("SELECT \"text\";");
    match &core_of(&core[0]).projection[0] {
        SelectItem::Expr { expr: Expr::Column(c), .. } => assert_eq!(c.name, "text"),
        other => panic!("expected a quoted identifier, got {other:?}"),
    }
}

#[test]
fn an_unterminated_string_is_an_error() {
    assert!(!error("SELECT 'unterminated;").is_empty());
}

#[test]
fn integers_floats_booleans_and_null_all_parse() {
    parse("SELECT 1, 1.5, TRUE, FALSE, NULL;");
}

#[test]
fn keywords_are_case_insensitive() {
    assert_eq!(parse("select 1;").len(), 1);
    assert_eq!(parse("SeLeCt 1;").len(), 1);
}

// --- Shape of parsed statements ---------------------------------------------

#[test]
fn create_table_captures_columns_and_constraints() {
    let stmts = parse("CREATE TABLE person (id INTEGER PRIMARY KEY, name TEXT NOT NULL);");
    match &stmts[0] {
        Statement::CreateTable(c) => {
            assert_eq!(c.name, "person");
            assert_eq!(c.columns.len(), 2);
            assert!(c.columns[0].primary_key);
            assert!(c.columns[1].not_null);
            assert!(!c.if_not_exists);
        }
        other => panic!("expected CREATE TABLE, got {other:?}"),
    }
}

#[test]
fn create_table_with_no_column_list_is_schemaless() {
    match &parse("CREATE TABLE session;")[0] {
        Statement::CreateTable(c) => assert!(c.columns.is_empty()),
        other => panic!("expected CREATE TABLE, got {other:?}"),
    }
}

#[test]
fn insert_captures_every_value_row() {
    match &parse("INSERT INTO t (a, b) VALUES (1, 2), (3, 4);")[0] {
        Statement::Insert(i) => {
            assert_eq!(i.columns.as_ref().map(Vec::len), Some(2));
            assert_eq!(i.values.len(), 2);
        }
        other => panic!("expected INSERT, got {other:?}"),
    }
}

#[test]
fn update_captures_assignments_and_filter() {
    match &parse("UPDATE t SET a = 1, b = 2 WHERE id = 3;")[0] {
        Statement::Update(u) => {
            assert_eq!(u.assignments.len(), 2);
            assert!(u.filter.is_some());
        }
        other => panic!("expected UPDATE, got {other:?}"),
    }
}

#[test]
fn delete_without_a_where_clause_has_no_filter() {
    match &parse("DELETE FROM t;")[0] {
        Statement::Delete(d) => assert!(d.filter.is_none()),
        other => panic!("expected DELETE, got {other:?}"),
    }
}

#[test]
fn a_table_alias_is_captured_with_or_without_as() {
    for text in ["SELECT * FROM person p;", "SELECT * FROM person AS p;"] {
        let stmts = parse(text);
        let from = core_of(&stmts[0]).from.as_ref().expect("a FROM clause");
        assert_eq!(from.alias.as_deref(), Some("p"), "in: {text}");
    }
}

#[test]
fn limit_and_offset_land_on_the_statement() {
    let stmts = parse("SELECT * FROM t LIMIT 10 OFFSET 5;");
    let select = select_of(&stmts[0]);
    assert_eq!(select.limit, Some(10));
    assert_eq!(select.offset, Some(5));
}

#[test]
fn order_by_captures_direction() {
    let stmts = parse("SELECT * FROM t ORDER BY a, b DESC;");
    let order = &select_of(&stmts[0]).order_by;
    assert_eq!(order.len(), 2);
    assert!(!order[0].desc);
    assert!(order[1].desc);
}

#[test]
fn a_recursive_cte_is_marked_recursive() {
    let stmts = parse(
        "WITH RECURSIVE r AS (SELECT 1 AS n UNION ALL SELECT n + 1 AS n FROM r) SELECT n FROM r;");
    let ctes = &select_of(&stmts[0]).ctes;
    assert_eq!(ctes.len(), 1);
    assert!(ctes[0].recursive);
}

#[test]
fn a_join_captures_its_kind() {
    let stmts = parse("SELECT * FROM a LEFT JOIN b ON a.id = b.a_id;");
    let joins = &core_of(&stmts[0]).joins;
    assert_eq!(joins.len(), 1);
    assert!(matches!(joins[0].kind, JoinKind::Left));
}

// --- Operator precedence ----------------------------------------------------

#[test]
fn multiplication_binds_tighter_than_addition() {
    let stmts = parse("SELECT 1 + 2 * 3;");
    match &core_of(&stmts[0]).projection[0] {
        SelectItem::Expr { expr: Expr::BinaryOp { op, right, .. }, .. } => {
            assert!(matches!(op, BinOp::Add), "the top node is the addition");
            assert!(matches!(**right, Expr::BinaryOp { op: BinOp::Mul, .. }),
                    "the multiplication sits underneath");
        }
        other => panic!("expected a binary expression, got {other:?}"),
    }
}

#[test]
fn and_binds_tighter_than_or() {
    let stmts = parse("SELECT * FROM t WHERE a OR b AND c;");
    match core_of(&stmts[0]).filter.as_ref().expect("a filter") {
        Expr::BinaryOp { op, right, .. } => {
            assert!(matches!(op, BinOp::Or), "the top node is the OR");
            assert!(matches!(**right, Expr::BinaryOp { op: BinOp::And, .. }));
        }
        other => panic!("expected a binary expression, got {other:?}"),
    }
}

#[test]
fn parentheses_override_precedence() {
    let stmts = parse("SELECT (1 + 2) * 3;");
    match &core_of(&stmts[0]).projection[0] {
        SelectItem::Expr { expr: Expr::BinaryOp { op, .. }, .. } => {
            assert!(matches!(op, BinOp::Mul));
        }
        other => panic!("expected a binary expression, got {other:?}"),
    }
}

// --- Graph sugar ------------------------------------------------------------

#[test]
fn a_graph_hop_desugars_into_a_join() {
    // The README's claim: `>` is sugar, not a new execution path.
    let stmts = parse("SELECT p2.name FROM person p1 > knows > person p2;");
    assert!(!core_of(&stmts[0]).joins.is_empty(), "the hop became a JOIN");
}

#[test]
fn a_starred_graph_hop_desugars_into_a_recursive_cte() {
    let stmts = parse("SELECT p2.name FROM person p1 > knows* > person p2;");
    let select = select_of(&stmts[0]);
    assert!(select.ctes.iter().any(|c| c.recursive),
            "the variable-depth hop became a WITH RECURSIVE");
}

#[test]
fn a_less_than_comparison_against_a_negative_number_still_parses() {
    // The README explains that `<` traversal is unimplemented precisely to
    // keep this working. Pin it, so adding `<` later cannot break it silently.
    parse("SELECT * FROM t WHERE x < -5;");
}

// --- Errors -----------------------------------------------------------------

#[test]
fn a_missing_from_target_is_an_error() {
    assert!(!error("SELECT * FROM;").is_empty());
}

#[test]
fn an_unknown_statement_keyword_is_an_error() {
    assert!(!error("FLY TO THE MOON;").is_empty());
}

#[test]
fn an_unclosed_parenthesis_is_an_error() {
    assert!(!error("SELECT (1 + 2;").is_empty());
}

#[test]
fn a_parse_error_does_not_panic_on_arbitrary_input() {
    for junk in ["((((", "'", "SELECT SELECT", ";;;", "\0", "SELECT * FROM t WHERE"] {
        let _ = parse_statements(junk);
    }
}
