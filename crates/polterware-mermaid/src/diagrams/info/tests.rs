//! Tests ported from the upstream info specs: `packages/parser/tests/info.test.ts` (the
//! Langium grammar) and `packages/mermaid/src/diagrams/info/info.spec.ts` (the diagram
//! parser). Inputs and expectations are upstream's.
//!
//! Skipped, and why:
//! - `expectNoErrorsOrAlternatives` checks Langium's lexer/parser error arrays and that no
//!   `console.log` ran. Here `parse` returning `Ok` is the error-free assertion, and there is
//!   no console.
//! - `result.value.$type` is Langium's generated AST type marker (`Info`), not parsed data.
//! - `info.spec.ts`'s unsupported input asserts the exact Langium message, including its
//!   character-skipping recovery (`skipped 11 characters`). This crate reports the same
//!   offending character with a [`Span`](crate::error::Span) instead; the assertion below
//!   checks the position and the character.
//! - `info.spec.ts` declares the unsupported-input test twice; it is ported once.
//! - `infoRenderer.ts` draws the version text; nothing here renders.

use super::Ast;
use super::parse;
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

// packages/parser/tests/info.test.ts

#[test]
fn handles_empty_info() {
    for body in ["info", "\n    info", "info\n    ", "\n    info\n    "] {
        assert_eq!(ast(body), Ast::default(), "{body:?}");
    }
}

#[test]
fn handles_show_info() {
    for body in [
        "info showInfo",
        "info showInfo\n    ",
        "\n    info showInfo",
        "info\n    showInfo",
        "info\n    showInfo\n    ",
        "\n    info\n    showInfo\n    ",
        "\n    info\n    showInfo",
        "\n    info showInfo\n    ",
    ] {
        assert_eq!(ast(body), Ast::default(), "{body:?}");
    }
}

// packages/mermaid/src/diagrams/info/info.spec.ts

#[test]
fn handles_an_info_definition() {
    assert_eq!(ast("info"), Ast::default());
}

#[test]
fn handles_an_info_definition_with_show_info() {
    assert_eq!(ast("info showInfo"), Ast::default());
}

#[test]
fn rejects_an_unsupported_info_statement() {
    let error = error("info unsupported");
    assert!(error.message.contains("unexpected character 'u'"));
    let span = error.span.expect("the error carries a span");
    assert_eq!((span.line, span.column), (1, 6));
    assert_eq!(span.start, 5);
}

// Behavior of the grammar, the lexer, and the value conversion the specs above do not pin
// down.

#[test]
fn parses_a_title_line() {
    assert_eq!(
        ast("info\ntitle Sample title").title.as_deref(),
        Some("Sample title")
    );
    assert_eq!(
        ast("info title Sample title").title.as_deref(),
        Some("Sample title")
    );
}

#[test]
fn parses_an_acc_title_line() {
    assert_eq!(
        ast("info\naccTitle: sample title").acc_title.as_deref(),
        Some("sample title")
    );
}

#[test]
fn parses_a_single_line_acc_description() {
    assert_eq!(
        ast("info\naccDescr: sample description")
            .acc_description
            .as_deref(),
        Some("sample description")
    );
}

#[test]
fn parses_a_multi_line_acc_description() {
    let ast = ast("info\naccDescr {\n   line one\n   line two\n}");
    assert_eq!(ast.acc_description.as_deref(), Some("line one\nline two"));
}

#[test]
fn parses_title_lines_after_show_info() {
    let ast = ast("info\nshowInfo\ntitle A\naccTitle: B\naccDescr: C");
    assert_eq!(ast.title.as_deref(), Some("A"));
    assert_eq!(ast.acc_title.as_deref(), Some("B"));
    assert_eq!(ast.acc_description.as_deref(), Some("C"));
}

#[test]
fn a_later_title_replaces_an_earlier_one() {
    let ast = ast("info\ntitle first\ntitle second");
    assert_eq!(ast.title.as_deref(), Some("second"));
}

#[test]
fn a_title_without_text_is_empty() {
    assert_eq!(ast("info\ntitle").title.as_deref(), Some(""));
}

#[test]
fn title_text_collapses_runs_of_spaces() {
    assert_eq!(
        ast("info\ntitle spaced   out\t\ttitle").title.as_deref(),
        Some("spaced out title")
    );
}

#[test]
fn skips_comment_lines() {
    let parsed = ast("info\n%% a comment\ntitle Sample");
    assert_eq!(parsed.title.as_deref(), Some("Sample"));
    assert_eq!(ast("info\n%% only a comment"), Ast::default());
}

#[test]
fn rejects_content_after_the_title_lines() {
    let error = error("info\ntitle A\nshowInfo");
    assert!(
        error
            .message
            .contains("expected end of input, got 'showInfo'"),
        "{}",
        error.message
    );
}

#[test]
fn rejects_a_word_where_the_keyword_is_expected() {
    let error = error("infox");
    let span = error.span.expect("the error carries a span");
    assert_eq!((span.line, span.column), (1, 1));
    assert!(error.message.contains("unexpected character 'i'"));
}

#[test]
fn a_stray_character_reports_its_span() {
    let error = error("info\n  @");
    let span = error.span.expect("the error carries a span");
    assert_eq!((span.line, span.column), (2, 3));
    assert!(error.message.contains('@'));
}

#[test]
fn parses_crlf_line_endings() {
    let ast = ast("info\r\ntitle Sample\r\n");
    assert_eq!(ast.title.as_deref(), Some("Sample"));
}
