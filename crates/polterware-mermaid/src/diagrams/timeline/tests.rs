//! Tests ported from upstream's `diagrams/timeline/timeline.spec.js`, plus a few that cover
//! this port's adaptations.
//!
//! Inputs and expectations are upstream's, including the leading indentation of the
//! template literals and the trailing spaces the event tokens keep. Upstream seeds the
//! database through `timelineDB.clear()` in `beforeEach`; each parse here starts from a
//! fresh database, so the clearing has no counterpart.
//!
//! Nothing is skipped: the timeline spec's assertions are all about the parser and the
//! database. Its only configuration call, `setLogLevel('trace')`, has no assertion.
//!
//! The tests after the ported ones pin this crate's adaptations: the span every error
//! carries, the two end-of-input tokens, and the `diagrams` entry point.

use super::{Ast, Direction, parse};

fn ast(body: &str) -> Ast {
    parse(body).unwrap_or_else(|error| panic!("expected a parse, got {error}"))
}

// timeline.spec.js: `describe('when parsing a timeline')`

#[test]
fn should_default_to_lr_direction_when_none_is_provided() {
    let ast = ast("timeline\n    section abc-123");
    assert_eq!(ast.direction, Direction::Lr);
    assert_eq!(ast.direction.as_str(), "LR");
}

#[test]
fn should_parse_td_direction() {
    let ast = ast("timeline TD\n    section abc-123");
    assert_eq!(ast.direction, Direction::Td);
    assert_eq!(ast.direction.as_str(), "TD");
}

#[test]
fn should_parse_lr_direction() {
    let ast = ast("timeline LR\n    section abc-123");
    assert_eq!(ast.direction, Direction::Lr);
}

#[test]
fn should_handle_a_simple_section_definition_abc_123() {
    let ast = ast("timeline\n    section abc-123");
    assert_eq!(ast.sections, ["abc-123"]);
}

#[test]
fn should_handle_a_simple_section_and_only_two_tasks() {
    let ast = ast("timeline\n    section abc-123\n    task1\n    task2");
    assert_eq!(ast.tasks.len(), 2);
    for task in &ast.tasks {
        assert_eq!(task.section, "abc-123");
        assert!(
            matches!(task.task.as_str(), "task1" | "task2"),
            "unexpected task {:?}",
            task.task
        );
    }
}

#[test]
fn should_handle_a_two_section_and_two_corresponding_tasks() {
    let ast = ast(
        "timeline\n    section abc-123\n    task1\n    task2\n    section abc-456\n    task3\n    task4",
    );
    assert_eq!(ast.sections, ["abc-123", "abc-456"]);
    for task in &ast.tasks {
        assert!(
            matches!(task.section.as_str(), "abc-123" | "abc-456"),
            "unexpected section {:?}",
            task.section
        );
        assert!(
            matches!(task.task.as_str(), "task1" | "task2" | "task3" | "task4"),
            "unexpected task {:?}",
            task.task
        );
        if task.section == "abc-123" {
            assert!(
                matches!(task.task.as_str(), "task1" | "task2"),
                "unexpected task {:?} in abc-123",
                task.task
            );
        } else {
            assert!(
                matches!(task.task.as_str(), "task3" | "task4"),
                "unexpected task {:?} in abc-456",
                task.task
            );
        }
    }
}

#[test]
fn should_handle_a_section_and_a_task_and_its_events() {
    let ast =
        ast("timeline\n    section abc-123\n      task1: event1\n      task2: event2: event3\n   ");
    assert_eq!(ast.sections[0], "abc-123");
    for task in &ast.tasks {
        match task.task.trim() {
            "task1" => assert_eq!(task.events, ["event1"]),
            "task2" => assert_eq!(task.events, ["event2", "event3"]),
            _ => {}
        }
    }
}

#[test]
fn should_handle_a_section_and_a_task_and_its_events_including_a_markdown_link() {
    let ast = ast(
        "timeline\n    section abc-123\n      task1: [event1](http://example.com)\n      task2: event2: event3\n   ",
    );
    assert_eq!(ast.sections[0], "abc-123");
    for task in &ast.tasks {
        match task.task.trim() {
            "task1" => assert_eq!(task.events, ["[event1](http://example.com)"]),
            "task2" => assert_eq!(task.events, ["event2", "event3"]),
            _ => {}
        }
    }
}

#[test]
fn should_handle_a_section_and_a_task_and_its_multi_line_events() {
    let ast = ast(
        "timeline\n    section abc-123\n      task1: event1\n      task2: event2: event3\n           : event4: event5\n   ",
    );
    assert_eq!(ast.sections[0], "abc-123");
    for task in &ast.tasks {
        match task.task.trim() {
            "task1" => assert_eq!(task.events, ["event1"]),
            "task2" => assert_eq!(task.events, ["event2", "event3", "event4", "event5"]),
            _ => {}
        }
    }
}

#[test]
fn should_handle_a_title_section_task_and_events_with_semicolons() {
    let ast = ast(
        "timeline\n      title ;my;title;\n      section ;a;bc-123;\n      ;ta;sk1;: ;ev;ent1; : ;ev;ent2; : ;ev;ent3;\n   ",
    );
    assert_eq!(ast.diagram_title, ";my;title;");
    assert_eq!(ast.sections, [";a;bc-123;"]);
    assert_eq!(
        ast.tasks[0].events,
        [";ev;ent1; ", ";ev;ent2; ", ";ev;ent3;"]
    );
}

#[test]
fn should_handle_a_title_section_task_and_events_with_hashtags() {
    let ast = ast(
        "timeline\n      title #my#title#\n      section #a#bc-123#\n      task1: #ev#ent1# : #ev#ent2# : #ev#ent3#\n   ",
    );
    assert_eq!(ast.diagram_title, "#my#title#");
    assert_eq!(ast.sections, ["#a#bc-123#"]);
    assert_eq!(ast.tasks[0].task, "task1");
    assert_eq!(
        ast.tasks[0].events,
        ["#ev#ent1# ", "#ev#ent2# ", "#ev#ent3#"]
    );
}

// Adaptations: the database's other getters, comments, and errors with a span.

#[test]
fn reads_the_title_and_the_accessibility_texts() {
    let ast = ast(
        "timeline\n  title A title\n  accTitle: my acc title\n  accDescr: my acc descr\n  section s\n  t",
    );
    assert_eq!(ast.diagram_title, "A title");
    assert_eq!(ast.acc_title, "my acc title");
    assert_eq!(ast.acc_description, "my acc descr");
    assert_eq!(ast.sections, ["s"]);
    assert_eq!(ast.tasks.len(), 1);
    assert_eq!(ast.tasks[0].task, "t");
}

#[test]
fn reads_a_multiline_accessibility_description() {
    let inline = ast("timeline\n  accDescr { multi line descr }\n  section s\n  t");
    assert_eq!(inline.acc_description, "multi line descr");
    assert_eq!(inline.sections, ["s"]);

    let multi_line = ast("timeline\n  section s\n  accDescr {\n  a\n  b  c }");
    assert_eq!(multi_line.acc_description, "a\nb  c");
}

#[test]
fn a_title_keyword_at_the_end_of_a_line_takes_the_next_line() {
    // `"title"\s[^\n]+`: the one whitespace character may be the line break, and
    // `substr(6)` then drops the indentation with the keyword.
    let ast = ast("timeline\n  title\n  section s");
    assert_eq!(ast.diagram_title, "  section s");
    assert!(ast.sections.is_empty());
}

#[test]
fn the_header_keyword_stops_at_a_word_character() {
    // jison-lex appends `\b` to the rule: `timelineTD` is a period, not a header.
    let error = parse("timelineTD\n  task").expect_err("the header is missing");
    assert_eq!(error.span.expect("a span").line, 1);

    // `LRx` fails its own rule (`LR\b`), so the bare header matches and `LRx` is a task.
    let ast = ast("timeline LRx\n  t");
    assert_eq!(ast.direction, Direction::Lr);
    assert_eq!(ast.tasks[0].task, "LRx");
}

#[test]
fn a_period_keeps_the_spaces_before_its_colon() {
    let ast = ast("timeline\n  t : e");
    assert_eq!(ast.tasks[0].task, "t ");
    assert_eq!(ast.tasks[0].events, ["e"]);
}

#[test]
fn the_period_text_ends_at_a_hash_comment() {
    let ast = ast("timeline\n  section s\n  t#: a");
    assert_eq!(ast.tasks[0].task, "t");
    assert!(ast.tasks[0].events.is_empty());
}

#[test]
fn skips_percent_comments() {
    // The generated regex is one `%` not followed by `{`, so a `%%{...}%%` line is a comment
    // too, while `%{` starts a period.
    let commented = ast("timeline\n  %% a comment\n  %%{init: {}}%%\n  a%%b\n  c");
    assert_eq!(commented.tasks.len(), 1);
    assert_eq!(commented.tasks[0].task, "c");

    let percent_brace = ast("timeline\n  %{not a comment}: e");
    assert_eq!(percent_brace.tasks[0].task, "%{not a comment}");
    assert_eq!(percent_brace.tasks[0].events, ["e"]);
}

#[test]
fn an_event_without_a_task_is_an_error_with_its_span() {
    let error = parse("timeline\n  section a\n  : orphan event").expect_err("there is no task");
    let span = error.span.expect("a span");
    assert_eq!((span.line, span.column), (3, 3));
    assert_eq!((span.start, span.end), (23, 37));
}

#[test]
fn a_colon_that_cannot_start_an_event_is_an_error_with_its_span() {
    let error = parse("timeline\n  section s\n  t:").expect_err("the colon has no event");
    assert_eq!(error.message, "unexpected ':'");
    let span = error.span.expect("a span");
    assert_eq!((span.line, span.column), (3, 4));
    assert_eq!((span.start, span.end), (24, 25));
}

#[test]
fn an_empty_accessibility_value_at_the_end_of_the_input_is_an_error() {
    // Upstream's lexer answers the parser's `$end` instead of the grammar's `EOF` token when
    // the last token is the zero-width value, so the parse fails.
    for body in [
        "timeline\naccTitle:",
        "timeline\naccDescr:",
        "timeline\naccDescr {",
    ] {
        let error = parse(body).expect_err("no value follows");
        assert_eq!(error.message, "unexpected end of input", "body: {body:?}");
        assert_eq!(error.span.expect("a span").line, 2, "body: {body:?}");
    }
}

#[test]
fn skips_leading_blank_lines() {
    // `preprocess` trims them, but a body handed to the parser directly may carry them.
    let ast = ast("\n\n\ntimeline\n  section s\n  t");
    assert_eq!(ast.sections, ["s"]);
    assert_eq!(ast.tasks.len(), 1);
}

#[test]
fn parses_through_the_crate_entry_point() {
    let body = "---\ntitle: Release\n---\ntimeline\n    section Build\n    t: e\n";
    match crate::parse(body).expect("the crate parses the diagram") {
        crate::Diagram::Timeline(ast) => {
            assert_eq!(ast.sections, ["Build"]);
            assert_eq!(ast.tasks.len(), 1);
            assert_eq!(ast.tasks[0].task, "t");
            assert_eq!(ast.tasks[0].events, ["e"]);
        }
        other => panic!("expected a timeline diagram, got {other:?}"),
    }
}
