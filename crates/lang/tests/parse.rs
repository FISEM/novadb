//! What shutup's parser accepts, and what it refuses.

use lang::*;

fn stmts(source: &str) -> Vec<Statement> {
    parse(source).unwrap_or_else(|e| panic!("parse failed: {e}\n--- source ---\n{source}"))
}

fn query(source: &str) -> Pipeline {
    let mut all = stmts(source);
    assert_eq!(all.len(), 1, "expected one statement, got {}", all.len());
    match all.remove(0) {
        Statement::Query(p) => p,
        other => panic!("expected a query, got {other:?}"),
    }
}

fn error(source: &str) -> ParseError {
    match parse(source) {
        Ok(ast) => panic!("expected an error, got {ast:?}\n--- source ---\n{source}"),
        Err(e) => e,
    }
}

/// The single step of a one-step pipeline.
fn only_step(source: &str) -> Step {
    let mut p = query(source);
    assert_eq!(p.steps.len(), 1, "expected one step");
    p.steps.remove(0)
}

/// The expression of a one-`where` pipeline.
fn filter(source: &str) -> Expr {
    match only_step(source) {
        Step::Where(e) => e,
        other => panic!("expected a where, got {other:?}"),
    }
}

// --- Sources and shape ------------------------------------------------------

#[test]
fn a_bare_collection_name_is_a_query() {
    let p = query("person");
    assert_eq!(p.source.expect("a source").name, "person");
    assert!(p.steps.is_empty());
}

#[test]
fn steps_follow_on_one_line_after_pipes() {
    let p = query("person | where age > 30 | show name");
    assert_eq!(p.source.expect("a source").name, "person");
    assert_eq!(p.steps.len(), 2);
}

#[test]
fn steps_follow_indented_under_the_source() {
    let p = query("person\n    where age > 30\n    show name\n");
    assert_eq!(p.source.expect("a source").name, "person");
    assert_eq!(p.steps.len(), 2);
}

#[test]
fn the_indented_and_one_line_forms_parse_the_same() {
    assert_eq!(
        query("person\n    where age > 30\n    show name"),
        query("person | where age > 30 | show name")
    );
}

#[test]
fn an_indented_line_may_itself_hold_pipes() {
    let p = query("person\n    where age > 30 | show name\n");
    assert_eq!(p.steps.len(), 2);
}

#[test]
fn a_pipeline_can_open_with_a_step_and_have_no_source() {
    let p = query("where age > 30 | show name");
    assert!(p.source.is_none(), "a piece to pipe records into");
    assert_eq!(p.steps.len(), 2);
}

#[test]
fn blank_lines_and_comments_are_ignored() {
    let p = query("# a note\nperson\n\n    # another\n    where age > 30\n");
    assert_eq!(p.steps.len(), 1);
}

#[test]
fn statements_separate_on_newlines_and_semicolons() {
    assert_eq!(stmts("person\nsession").len(), 2);
    assert_eq!(stmts("person; session").len(), 2);
    assert_eq!(stmts("person\n").len(), 1);
}

#[test]
fn empty_input_is_no_statements() {
    assert!(stmts("").is_empty());
    assert!(stmts("   \n\n  # just a comment\n").is_empty());
}

// --- Steps ------------------------------------------------------------------

#[test]
fn show_keeps_renames_and_computes() {
    match only_step("person | show name, years: age, adult: age >= 18") {
        Step::Show(items) => {
            assert_eq!(items.len(), 3);
            assert_eq!(items[0].alias, None, "a bare field keeps its own name");
            assert_eq!(items[1].alias.as_deref(), Some("years"));
            assert_eq!(items[2].alias.as_deref(), Some("adult"));
        }
        other => panic!("expected a show, got {other:?}"),
    }
}

#[test]
fn sort_is_up_unless_told_otherwise() {
    match only_step("person | sort dept, age down") {
        Step::Sort(keys) => {
            assert_eq!(keys.len(), 2);
            assert_eq!(keys[0].direction, Direction::Up);
            assert_eq!(keys[1].direction, Direction::Down, "down binds to its own key");
        }
        other => panic!("expected a sort, got {other:?}"),
    }
}

#[test]
fn sort_up_can_be_said_out_loud() {
    match only_step("person | sort age up") {
        Step::Sort(keys) => assert_eq!(keys[0].direction, Direction::Up),
        other => panic!("expected a sort, got {other:?}"),
    }
}

#[test]
fn take_and_skip_carry_their_count() {
    assert!(matches!(only_step("person | take 10"), Step::Take { count: 10, .. }));
    assert!(matches!(only_step("person | skip 5"), Step::Skip { count: 5, .. }));
}

#[test]
fn unique_takes_nothing() {
    assert!(matches!(only_step("person | unique"), Step::Unique));
}

#[test]
fn join_captures_its_target_and_condition() {
    match only_step("person | join pet on pet.owner_id == person.id") {
        Step::Join(j) => {
            assert_eq!(j.collection.name, "pet");
            assert!(!j.keep_all);
        }
        other => panic!("expected a join, got {other:?}"),
    }
}

#[test]
fn keep_all_marks_the_join_and_does_not_swallow_the_condition() {
    match only_step("person | join pet on pet.owner_id == person.id keep all") {
        Step::Join(j) => {
            assert!(j.keep_all);
            assert!(matches!(j.on, Expr::Comparison { .. }), "the condition survived");
        }
        other => panic!("expected a join, got {other:?}"),
    }
}

#[test]
fn group_by_captures_its_keys() {
    match only_step("pet | group by species, age") {
        Step::GroupBy(keys) => assert_eq!(keys.len(), 2),
        other => panic!("expected a group by, got {other:?}"),
    }
}

#[test]
fn follow_takes_one_step_by_default() {
    match only_step("person | follow knows") {
        Step::Follow(f) => {
            assert_eq!(f.link, "knows");
            assert!(!f.repeat);
            assert!(!f.backward);
        }
        other => panic!("expected a follow, got {other:?}"),
    }
}

#[test]
fn keep_following_goes_all_the_way() {
    match only_step("person | keep following knows") {
        Step::Follow(f) => {
            assert!(f.repeat);
            assert!(!f.backward);
        }
        other => panic!("expected a follow, got {other:?}"),
    }
}

#[test]
fn backward_goes_against_the_arrow() {
    match only_step("person | follow knows backward") {
        Step::Follow(f) => {
            assert!(f.backward);
            assert!(!f.repeat);
        }
        other => panic!("expected a follow, got {other:?}"),
    }
}

#[test]
fn keep_following_backward_is_allowed() {
    match only_step("person | keep following knows backward") {
        Step::Follow(f) => assert!(f.repeat && f.backward),
        other => panic!("expected a follow, got {other:?}"),
    }
}

#[test]
fn set_assigns_with_an_equals_sign() {
    match only_step("person | set age = age + 1, seen = True") {
        Step::Set(assignments) => {
            assert_eq!(assignments.len(), 2);
            assert_eq!(assignments[0].field, "age");
            assert_eq!(assignments[1].field, "seen");
        }
        other => panic!("expected a set, got {other:?}"),
    }
}

#[test]
fn delete_takes_nothing() {
    assert!(matches!(only_step("person | delete"), Step::Delete { .. }));
}

#[test]
fn a_bare_name_in_step_position_is_a_defined_piece() {
    match only_step("session | recent") {
        Step::Named(s) => assert_eq!(s.name, "recent"),
        other => panic!("expected a named step, got {other:?}"),
    }
}

// --- Expressions ------------------------------------------------------------

#[test]
fn a_plain_comparison_is_a_chain_of_one() {
    match filter("person | where age > 30") {
        Expr::Comparison { rest, .. } => {
            assert_eq!(rest.len(), 1);
            assert_eq!(rest[0].0, CompareOp::Greater);
        }
        other => panic!("expected a comparison, got {other:?}"),
    }
}

#[test]
fn comparisons_chain_as_in_python() {
    match filter("person | where 18 < age < 65") {
        Expr::Comparison { rest, .. } => {
            assert_eq!(rest.len(), 2, "one node, two links, middle read once");
        }
        other => panic!("expected a comparison, got {other:?}"),
    }
}

#[test]
fn and_binds_tighter_than_or() {
    match filter("person | where a or b and c") {
        Expr::Binary { op: BinOp::Or, right, .. } => {
            assert!(matches!(*right, Expr::Binary { op: BinOp::And, .. }));
        }
        other => panic!("expected an or at the top, got {other:?}"),
    }
}

#[test]
fn multiplication_binds_tighter_than_addition() {
    match filter("person | where 1 + 2 * 3 > 0") {
        Expr::Comparison { first, .. } => match *first {
            Expr::Binary { op: BinOp::Add, right, .. } => {
                assert!(matches!(*right, Expr::Binary { op: BinOp::Multiply, .. }));
            }
            other => panic!("expected an add, got {other:?}"),
        },
        other => panic!("expected a comparison, got {other:?}"),
    }
}

#[test]
fn parentheses_override_precedence() {
    match filter("person | where (1 + 2) * 3 > 0") {
        Expr::Comparison { first, .. } => {
            assert!(matches!(*first, Expr::Binary { op: BinOp::Multiply, .. }));
        }
        other => panic!("expected a comparison, got {other:?}"),
    }
}

#[test]
fn not_negates() {
    assert!(matches!(filter("person | where not ready"), Expr::Unary { op: UnOp::Not, .. }));
}

#[test]
fn in_and_not_in_are_captured() {
    match filter("person | where device in [\"mobile\", \"tablet\"]") {
        Expr::In { negated, .. } => assert!(!negated),
        other => panic!("expected an in, got {other:?}"),
    }
    match filter("person | where device not in [\"mobile\"]") {
        Expr::In { negated, .. } => assert!(negated),
        other => panic!("expected an in, got {other:?}"),
    }
}

#[test]
fn is_none_and_is_not_none_are_captured() {
    match filter("person | where nickname is None") {
        Expr::IsNone { negated, .. } => assert!(!negated),
        other => panic!("expected an is None, got {other:?}"),
    }
    match filter("person | where nickname is not None") {
        Expr::IsNone { negated, .. } => assert!(negated),
        other => panic!("expected an is None, got {other:?}"),
    }
}

#[test]
fn a_bare_field_is_a_valid_condition() {
    match filter("person | where nickname") {
        Expr::Field { path, .. } => assert_eq!(path, vec!["nickname".to_string()]),
        other => panic!("expected a field, got {other:?}"),
    }
}

#[test]
fn a_dotted_path_keeps_every_piece() {
    match filter("person | where p.address.city") {
        Expr::Field { path, .. } => assert_eq!(path, vec!["p", "address", "city"]),
        other => panic!("expected a field, got {other:?}"),
    }
}

#[test]
fn both_quote_characters_make_a_string() {
    for source in ["person | where name == \"alice\"", "person | where name == 'alice'"] {
        match filter(source) {
            Expr::Comparison { rest, .. } => match &rest[0].1 {
                Expr::Literal { value: Literal::String(s), .. } => assert_eq!(s, "alice"),
                other => panic!("expected a string, got {other:?}"),
            },
            other => panic!("expected a comparison, got {other:?}"),
        }
    }
}

#[test]
fn true_false_and_none_are_literals() {
    for (source, expected) in [
        ("person | where flag == True", Literal::Bool(true)),
        ("person | where flag == False", Literal::Bool(false)),
        ("person | where flag == None", Literal::None),
    ] {
        match filter(source) {
            Expr::Comparison { rest, .. } => match &rest[0].1 {
                Expr::Literal { value, .. } => assert_eq!(*value, expected),
                other => panic!("expected a literal, got {other:?}"),
            },
            other => panic!("expected a comparison, got {other:?}"),
        }
    }
}

#[test]
fn whole_numbers_stay_whole() {
    match filter("person | where age == 30") {
        Expr::Comparison { rest, .. } => match &rest[0].1 {
            Expr::Literal { value: Literal::Number(n), .. } => {
                assert_eq!(n.as_i64(), Some(30), "not 30.0");
            }
            other => panic!("expected a number, got {other:?}"),
        },
        other => panic!("expected a comparison, got {other:?}"),
    }
}

#[test]
fn calls_and_methods_are_told_apart() {
    assert!(matches!(filter("person | where len(name) > 3"),
                     Expr::Comparison { first, .. } if matches!(*first, Expr::Call { .. })));
    assert!(matches!(filter("person | where name.startswith(\"a\")"), Expr::Method { .. }));
}

#[test]
fn indexing_is_captured() {
    assert!(matches!(filter("person | where tags[0] == \"x\""),
                     Expr::Comparison { first, .. } if matches!(*first, Expr::Index { .. })));
}

#[test]
fn lists_and_records_are_literals() {
    assert!(matches!(filter("person | where [1, 2] == x"),
                     Expr::Comparison { first, .. } if matches!(*first, Expr::List { .. })));
    assert!(matches!(filter("person | where { a: 1 } == x"),
                     Expr::Comparison { first, .. } if matches!(*first, Expr::RecordLiteral { .. })));
}

// --- Statements that are not queries ---------------------------------------

#[test]
fn define_with_a_body_states_a_shape() {
    let mut all = stmts("define person\n    id: number key\n    name: string\n    age: number?\n");
    match all.remove(0) {
        Statement::DefineShape(d) => {
            assert_eq!(d.name, "person");
            let fields = d.fields.expect("a body");
            assert_eq!(fields.len(), 3);
            assert!(fields[0].key);
            assert_eq!(fields[0].kind, TypeName::Number);
            assert_eq!(fields[1].kind, TypeName::String);
            assert!(fields[2].optional);
        }
        other => panic!("expected a shape, got {other:?}"),
    }
}

#[test]
fn define_with_no_body_makes_no_claim() {
    match stmts("define session").remove(0) {
        Statement::DefineShape(d) => assert!(d.fields.is_none(), "no body is not an empty body"),
        other => panic!("expected a shape, got {other:?}"),
    }
}

#[test]
fn a_shape_fits_on_one_line() {
    assert_eq!(
        stmts("define person { id: number key, name: string }"),
        stmts("define person\n    id: number key\n    name: string"),
    );
}

#[test]
fn define_as_names_a_pipeline_with_a_source() {
    match stmts("define adults as\n    person\n        where age >= 18\n").remove(0) {
        Statement::DefineName(d) => {
            assert_eq!(d.name, "adults");
            assert_eq!(d.pipeline.source.expect("a source").name, "person");
            assert_eq!(d.pipeline.steps.len(), 1);
        }
        other => panic!("expected a name, got {other:?}"),
    }
}

#[test]
fn define_as_names_a_piece_with_no_source() {
    match stmts("define recent as\n    where created > 1000\n    sort created down\n").remove(0) {
        Statement::DefineName(d) => {
            assert!(d.pipeline.source.is_none());
            assert_eq!(d.pipeline.steps.len(), 2);
        }
        other => panic!("expected a name, got {other:?}"),
    }
}

#[test]
fn a_named_pipeline_fits_on_one_line() {
    match stmts("define adults as person | where age >= 18").remove(0) {
        Statement::DefineName(d) => {
            assert_eq!(d.pipeline.source.expect("a source").name, "person");
            assert_eq!(d.pipeline.steps.len(), 1);
        }
        other => panic!("expected a name, got {other:?}"),
    }
}

#[test]
fn add_takes_one_record_indented_or_inline() {
    for source in [
        "add person\n    id: 1\n    name: \"alice\"\n",
        "add person { id: 1, name: \"alice\" }",
    ] {
        match stmts(source).remove(0) {
            Statement::Add(a) => {
                assert_eq!(a.collection.name, "person");
                assert_eq!(a.record.len(), 2);
                assert_eq!(a.record[0].0, "id");
            }
            other => panic!("expected an add, got {other:?}"),
        }
    }
}

#[test]
fn remove_notes_whether_it_may_be_missing() {
    match stmts("remove person").remove(0) {
        Statement::Remove { name, if_exists, .. } => {
            assert_eq!(name, "person");
            assert!(!if_exists);
        }
        other => panic!("expected a remove, got {other:?}"),
    }
    match stmts("remove person if exists").remove(0) {
        Statement::Remove { if_exists, .. } => assert!(if_exists),
        other => panic!("expected a remove, got {other:?}"),
    }
}

// --- Errors -----------------------------------------------------------------

#[test]
fn an_error_points_at_the_text_it_means() {
    let e = error("person | where age > ");
    assert!(e.span.start <= e.span.end);
    assert!(!e.message.is_empty());
}

#[test]
fn an_error_message_is_a_sentence() {
    let e = error("person | take");
    assert!(e.message.ends_with('.'), "not a fragment: {:?}", e.message);
}

#[test]
fn an_error_names_the_fix_where_there_is_one() {
    let e = error("person | take");
    assert!(e.help.is_some(), "no help offered for: {:?}", e.message);
}

#[test]
fn an_unterminated_string_is_an_error() {
    error("person | where name == \"alice");
}

#[test]
fn an_unclosed_bracket_is_an_error() {
    error("person | where (1 + 2");
}

#[test]
fn inconsistent_indentation_is_an_error() {
    error("person\n    where age > 30\n   show name\n");
}

#[test]
fn the_parser_does_not_panic_on_junk() {
    for junk in ["((((", "'", "|||", ";;;", "\0", "person |", "define", "show", "|"] {
        let _ = parse(junk);
    }
}

// --- Statements that end on a block ----------------------------------------
//
// The newline that separated two statements is gone once a block has closed,
// so these are the shapes a real script is made of, and the shapes a parser
// tested only on one-liners gets wrong.

#[test]
fn two_indented_statements_can_follow_each_other() {
    assert_eq!(stmts("define person\n    id: number key\n\ndefine pet\n    id: number key\n").len(), 2);
}

#[test]
fn an_indented_statement_can_be_followed_by_a_one_line_one() {
    assert_eq!(stmts("define person\n    id: number key\n\nadd person { id: 1 }\n").len(), 2);
}

#[test]
fn an_indented_pipeline_can_be_followed_by_another_statement() {
    assert_eq!(stmts("person\n    where age > 30\n\nsession\n").len(), 2);
}

#[test]
fn a_whole_script_of_mixed_shapes_parses() {
    let script = "\
define person
    id: number key
    name: string

define pet
    id: number key
    owner_id: number

add person { id: 1, name: \"alice\" }
add pet { id: 1, owner_id: 1 }

person
    join pet on pet.owner_id == person.id
    show person.name, pet.id
";
    assert_eq!(stmts(script).len(), 5);
}
