//! Tests ported from the upstream journey specs: `parser/journey.spec.js` (the grammar and
//! its database calls) and `journeyDb.spec.js` (the database alone). Inputs and expectations
//! are upstream's; `expect(...).not.toThrow()` becomes `parse` answering `Ok`, and the
//! database-only specs call [`Db`] directly, as upstream calls `journeyDb` instead of a
//! parser.
//!
//! Skipped, and why:
//! - Nothing in these two files asserts rendering or configuration; the folder's renderer
//!   (`journeyRenderer.ts`, `svgDraw.js`, `styles.js`) has no spec here and is out of scope
//!   for a parser port. `journeyDb`'s `getConfig` getter is configuration and is not ported.
//! - The two `clear` blocks' `beforeEach` adds records before clearing them. The ported tests
//!   do the same against a fresh [`Db`] and read the cleared state, which is what the
//!   `it.each` rows compare.
//!
//! The last tests cover behavior the specs do not pin down: Jison's rule order and
//! start-condition quirks, JavaScript's `Number` for the score, UTF-16 actor order, CRLF,
//! and the spans of parse errors.

use super::ast::Task;
use super::db::Db;
use super::{Ast, parse};
use crate::error::ParseError;

fn ast(body: &str) -> Ast {
    parse(body).unwrap_or_else(|error| panic!("expected {body:?} to parse, got {error}"))
}

fn error(body: &str) -> ParseError {
    match parse(body) {
        Ok(ast) => panic!("expected {body:?} to fail, parsed {ast:?}"),
        Err(error) => error,
    }
}

fn task(section: &str, text: &str, people: &[&str], score: f64) -> Task {
    Task {
        section: section.to_string(),
        task_type: section.to_string(),
        people: people.iter().map(|person| person.to_string()).collect(),
        task: text.to_string(),
        score,
    }
}

fn place(error: &ParseError) -> (u32, u32) {
    let span = error.span.expect("the error carries a span");
    (span.line, span.column)
}

// packages/mermaid/src/diagrams/user-journey/parser/journey.spec.js

#[test]
fn handles_a_title_definition() {
    assert!(parse("journey\ntitle Adding journey diagram functionality to mermaid").is_ok());
}

#[test]
fn handles_an_accessibility_description() {
    let body = "journey\n\
        accDescr: A user journey for family shopping\n\
        title Adding journey diagram functionality to mermaid\n\
        section Order from website";
    assert!(parse(body).is_ok());
}

#[test]
fn handles_a_multiline_accessibility_description() {
    let body = "journey\n\
        accDescr {\n        A user journey for\n        family shopping\n      }\
        title Adding journey diagram functionality to mermaid\n\
        accTitle: Adding acc journey diagram functionality to mermaid\n\
        section Order from website";
    let ast = ast(body);
    assert_eq!(ast.acc_description, "A user journey for\nfamily shopping");
    assert_eq!(
        ast.diagram_title,
        "Adding journey diagram functionality to mermaid"
    );
    assert_eq!(
        ast.acc_title,
        "Adding acc journey diagram functionality to mermaid"
    );
}

#[test]
fn handles_an_accessibility_title() {
    let ast = ast("journey\n    accTitle: The title\n    section Order from website");
    assert_eq!(ast.acc_description, "");
    assert_eq!(ast.acc_title, "The title");
}

#[test]
fn handles_a_section_definition() {
    let body = "journey\n\
        title Adding journey diagram functionality to mermaid\n\
        section Order from website";
    assert!(parse(body).is_ok());
}

#[test]
fn handles_multiline_section_titles_with_different_line_breaks() {
    let body = "journey\n\
        title Adding gantt diagram functionality to mermaid\n\
        section Line1<br>Line2<br/>Line3</br />Line4<br\t/>Line5";
    assert!(parse(body).is_ok());
}

#[test]
fn handles_a_task_definition() {
    let body = "journey\n\
        title Adding journey diagram functionality to mermaid\n\
        section Documentation\n\
        A task: 5: Alice, Bob, Charlie\n\
        B task: 3:Bob, Charlie\n\
        C task: 5\n\
        D task: 5: Charlie, Alice\n\
        E task: 5:\n\
        section Another section\n\
        P task: 5:\n\
        Q task: 5:\n\
        R task: 5:";
    let ast = ast(body);
    assert_eq!(ast.tasks.len(), 8);
    assert_eq!(
        ast.tasks[0],
        task("Documentation", "A task", &["Alice", "Bob", "Charlie"], 5.0)
    );
    assert_eq!(
        ast.tasks[1],
        task("Documentation", "B task", &["Bob", "Charlie"], 3.0)
    );
    assert_eq!(ast.tasks[2], task("Documentation", "C task", &[], 5.0));
    assert_eq!(
        ast.tasks[3],
        task("Documentation", "D task", &["Charlie", "Alice"], 5.0)
    );
    assert_eq!(ast.tasks[4], task("Documentation", "E task", &[""], 5.0));
    assert_eq!(ast.tasks[5], task("Another section", "P task", &[""], 5.0));
    assert_eq!(ast.tasks[6], task("Another section", "Q task", &[""], 5.0));
    assert_eq!(ast.tasks[7], task("Another section", "R task", &[""], 5.0));
    assert_eq!(ast.sections, ["Documentation", "Another section"]);
}

// packages/mermaid/src/diagrams/user-journey/journeyDb.spec.js

#[test]
fn clear_empties_tasks_acc_title_sections_and_actors() {
    let mut db = Db::new();
    db.add_section("weekends skip test".to_string());
    db.add_task("test1", "4: id1, id3");
    db.add_task("test2", "2: id2");
    db.clear();
    let ast = db.into_ast();
    assert_eq!(ast.tasks, Vec::<Task>::new());
    assert_eq!(ast.acc_title, "");
    assert_eq!(ast.sections, Vec::<String>::new());
    assert_eq!(ast.actors, Vec::<String>::new());
}

#[test]
fn clear_empties_tasks_acc_title_acc_description_and_sections() {
    let mut db = Db::new();
    db.add_section("weekends skip test".to_string());
    db.add_task("test1", "3: id1, id3");
    db.add_task("test2", "1: id2");
    db.clear();
    let ast = db.into_ast();
    assert_eq!(ast.tasks, Vec::<Task>::new());
    assert_eq!(ast.acc_title, "");
    assert_eq!(ast.acc_description, "");
    assert_eq!(ast.sections, Vec::<String>::new());
}

#[test]
fn tasks_and_actors_are_added() {
    let mut db = Db::new();
    db.set_acc_title("Shopping".to_string());
    db.set_acc_description("A user journey for family shopping".to_string());
    db.add_section("Journey to the shops".to_string());
    db.add_task("Get car keys", ":5:Dad");
    db.add_task("Go to car", ":3:Dad, Mum, Child#1, Child#2");
    db.add_task("Drive to supermarket", ":4:Dad");
    db.add_section("Do shopping".to_string());
    db.add_task("Go shopping", ":5:Mum");
    let ast = db.into_ast();

    assert_eq!(ast.acc_title, "Shopping");
    assert_eq!(ast.acc_description, "A user journey for family shopping");
    assert_eq!(
        ast.tasks,
        [
            task("Journey to the shops", "Get car keys", &["Dad"], 5.0),
            task(
                "Journey to the shops",
                "Go to car",
                &["Dad", "Mum", "Child#1", "Child#2"],
                3.0
            ),
            task(
                "Journey to the shops",
                "Drive to supermarket",
                &["Dad"],
                4.0
            ),
            task("Do shopping", "Go shopping", &["Mum"], 5.0),
        ]
    );
    assert_eq!(ast.actors, ["Child#1", "Child#2", "Dad", "Mum"]);
    assert_eq!(ast.sections, ["Journey to the shops", "Do shopping"]);
}

// Behavior the specs above do not pin down.

#[test]
fn parses_a_header_alone() {
    for body in ["journey", "  journey  ", "journey\n", "journey\n\n\n"] {
        let ast = ast(body);
        assert!(ast.tasks.is_empty(), "{body:?}");
        assert!(ast.sections.is_empty(), "{body:?}");
        assert_eq!(ast.diagram_title, "");
    }
}

#[test]
fn skips_the_blank_lines_preprocess_leaves_before_the_header() {
    // Upstream's parser receives text that starts with the header; this crate's preprocess
    // keeps a blank line where the frontmatter was.
    assert!(parse("\n\njourney\nsection A").is_ok());
}

#[test]
fn keywords_are_case_insensitive() {
    let ast = ast("JOURNEY\nSECTION A\nA TASK: 5: Bob");
    assert_eq!(ast.sections, ["A"]);
    assert_eq!(ast.tasks, [task("A", "A TASK", &["Bob"], 5.0)]);
}

#[test]
fn the_journey_keyword_ends_at_a_word_boundary() {
    let error = error("journeyX: 5");
    assert_eq!(place(&error), (1, 1));
    assert!(error.message.contains("taskName"), "{}", error.message);
}

#[test]
fn the_header_must_come_first() {
    let error = error("title foo");
    assert_eq!(place(&error), (1, 1));
    assert!(error.message.contains("journey"), "{}", error.message);
}

#[test]
fn a_bare_title_takes_the_next_line() {
    // The title rule's `\s` matches the line break.
    let ast = ast("journey\ntitle\nsection A");
    assert_eq!(ast.diagram_title, "section A");
    assert!(ast.sections.is_empty());
}

#[test]
fn a_title_keeps_the_text_after_its_first_space() {
    assert_eq!(
        ast("journey\ntitle  two spaces").diagram_title,
        " two spaces"
    );
    assert_eq!(ast("journey\ntitle a: b").diagram_title, "a: b");
    assert_eq!(ast("journey\ntitle X # comment").diagram_title, "X ");
}

#[test]
fn sections_keep_their_text_and_duplicates() {
    let ast = ast("journey\nsection A\nsection A");
    assert_eq!(ast.sections, ["A", "A"]);
}

#[test]
fn a_task_name_keeps_trailing_spaces() {
    let ast = ast("journey\nA : 5: x");
    assert_eq!(ast.tasks, [task("", "A ", &["x"], 5.0)]);
}

#[test]
fn tasks_before_a_section_belong_to_no_section() {
    let ast = ast("journey\nA: 5: x");
    assert_eq!(ast.tasks[0].section, "");
    assert_eq!(ast.tasks[0].task_type, "");
}

#[test]
fn statements_need_no_newline_between_them() {
    assert_eq!(
        ast("journey A: 5").tasks,
        [task("", "A", &[], 5.0)],
        "the header and a task can share a line"
    );
}

#[test]
fn a_task_data_splits_its_people_on_commas_and_trims_them() {
    let ast = ast("journey\nA: 5: Alice, Bob ,  Charlie ");
    assert_eq!(ast.tasks[0].people, ["Alice", "Bob", "Charlie"]);
}

#[test]
fn an_empty_people_part_is_one_empty_person() {
    let ast = ast("journey\nA: 5:");
    assert_eq!(ast.tasks[0].people, [""]);
    assert_eq!(ast.actors, [""]);
}

#[test]
fn extra_task_data_pieces_are_dropped() {
    let ast = ast("journey\nA: 5: p, q: r");
    assert_eq!(ast.tasks[0].score, 5.0);
    assert_eq!(ast.tasks[0].people, ["p", "q"]);
}

#[test]
fn a_score_is_a_javascript_number() {
    let ast = ast("journey\n\
         A: 0x10: x\n\
         B: 0b101: x\n\
         C: 1e3: x\n\
         D: : x\n\
         E: -2.5: x\n\
         F: nope: x");
    let scores: Vec<f64> = ast.tasks.iter().map(|task| task.score).collect();
    assert_eq!(&scores[..5], &[16.0, 5.0, 1000.0, 0.0, -2.5]);
    assert!(scores[5].is_nan());
}

#[test]
fn a_hash_starts_a_comment() {
    let ast = ast("journey\n# a comment\nA: 5: x#note\n# another");
    assert_eq!(ast.tasks, [task("", "A", &["x"], 5.0)]);
}

#[test]
fn a_percent_line_is_a_comment() {
    let ast = ast("journey\n%% a comment\n% another\nA: 5: x");
    assert_eq!(ast.tasks, [task("", "A", &["x"], 5.0)]);
}

#[test]
fn a_line_whose_second_and_third_characters_are_percent_is_a_comment() {
    // The second comment rule starts with any character but `}`.
    let ast = ast("journey\nx%%note: 5\nA: 5: y");
    assert_eq!(ast.tasks, [task("", "A", &["y"], 5.0)]);
}

#[test]
fn a_semicolon_is_an_invalid_token() {
    let error = error("journey\nA: 5: x; y");
    assert_eq!(place(&error), (2, 8));
    assert!(error.message.contains("INVALID"), "{}", error.message);
}

#[test]
fn a_task_name_needs_task_data() {
    let error = error("journey\nA");
    assert_eq!(place(&error), (2, 2));
    assert!(error.message.contains("task data"), "{}", error.message);
}

#[test]
fn a_colon_without_data_is_a_syntax_error() {
    let error = error("journey\nA:");
    assert_eq!(place(&error), (2, 2));
    assert!(error.message.contains("task data"), "{}", error.message);
}

#[test]
fn an_empty_accessibility_title_at_the_end_of_input_is_rejected() {
    // Jison's exclusive start condition consumes the empty value, so the grammar's explicit
    // EOF token never arrives.
    for (body, place_at) in [
        ("journey\naccTitle:", (2, 10)),
        ("journey\naccTitle:\n", (3, 1)),
    ] {
        let error = error(body);
        assert_eq!(place(&error), place_at, "{body:?}");
        assert!(error.message.contains("end of input"), "{}", error.message);
    }
}

#[test]
fn an_accessibility_title_that_only_holds_whitespace_takes_the_next_line() {
    let ast = ast("journey\naccTitle: \nsection A");
    assert_eq!(ast.acc_title, "section A");
    assert!(ast.sections.is_empty());
}

#[test]
fn an_unterminated_multiline_description_is_rejected() {
    assert!(parse("journey\naccDescr {line1").is_err());
    assert!(parse("journey\naccDescr {").is_err());
}

#[test]
fn a_multiline_description_stops_at_the_brace() {
    let ast = ast("journey\naccDescr {line1\nline2}\nA: 5: x");
    assert_eq!(ast.acc_description, "line1\nline2");
    assert_eq!(ast.tasks, [task("", "A", &["x"], 5.0)]);
}

#[test]
fn a_multiline_description_collapses_the_indentation_of_its_lines() {
    let ast = ast("journey\naccDescr {\n    line1\n    line2\n}\nsection A");
    assert_eq!(ast.acc_description, "line1\nline2");
}

#[test]
fn accessibility_values_are_trimmed() {
    let ast = ast("journey\naccTitle:  x  \naccDescr:   a  b  \nsection A");
    assert_eq!(ast.acc_title, "x");
    assert_eq!(ast.acc_description, "a  b");
}

#[test]
fn crlf_leaves_the_carriage_return_in_a_title_and_a_section() {
    let ast = ast("journey\r\ntitle X\r\nsection A\r\nA task: 5: x\r\n");
    assert_eq!(ast.diagram_title, "X\r");
    assert_eq!(ast.sections, ["A\r"]);
    assert_eq!(ast.tasks, [task("A\r", "A task", &["x"], 5.0)]);
}

#[test]
fn actors_are_sorted_by_utf16_code_unit() {
    let ast = ast("journey\nA: 5: \u{e000}, \u{10000}");
    // A surrogate pair sorts before U+E000 in UTF-16, though U+E000 sorts first by byte.
    assert_eq!(ast.actors, ["\u{10000}", "\u{e000}"]);
}

#[test]
fn parses_through_the_crate_entry_point() {
    let text = "---\ntitle: A journey\n---\njourney\nsection A\nA: 5: x";
    match crate::parse(text).expect("the frontmatter is removed before parsing") {
        crate::Diagram::UserJourney(ast) => {
            assert_eq!(ast.sections, ["A"]);
            assert_eq!(ast.tasks, [task("A", "A", &["x"], 5.0)]);
        }
        other => panic!("expected a journey diagram, got {other:?}"),
    }
}
