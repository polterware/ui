//! Tests ported from the upstream ishikawa spec:
//! `packages/mermaid/src/diagrams/ishikawa/ishikawa.spec.ts`. Inputs and expectations are
//! upstream's; the rest pin behavior the grammar and the database have but the spec does
//! not spell out.
//!
//! Skipped, and why:
//! - `ishikawa.yy.clear()` and `setLogLevel('trace')` are harness setup, not diagram data;
//!   [`parse`] returns a fresh [`Ast`] per document.
//! - `ishikawaRenderer.ts` and `ishikawaStyles.ts` render; nothing here renders.

use super::parse;
use super::{Ast, IshikawaNode};
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

fn root(ast: &Ast) -> &IshikawaNode {
    ast.root
        .as_ref()
        .unwrap_or_else(|| panic!("expected a root in {ast:?}"))
}

fn children(node: &IshikawaNode) -> Vec<&str> {
    node.children
        .iter()
        .map(|child| child.text.as_str())
        .collect()
}

fn place(error: &ParseError) -> (u32, u32) {
    let span = error.span.expect("the error carries a span");
    (span.line, span.column)
}

// packages/mermaid/src/diagrams/ishikawa/ishikawa.spec.ts

#[test]
fn should_parse_a_basic_ishikawa_hierarchy() {
    let ast = ast(
        "ishikawa-beta\n    Blurry Photo\n        Process\n            Out of focus\n        User\n            Shaky hands\n",
    );
    let root = root(&ast);
    assert_eq!(root.text, "Blurry Photo");
    assert_eq!(root.children.len(), 2);
    assert_eq!(root.children[0].text, "Process");
    assert_eq!(root.children[0].children[0].text, "Out of focus");
    assert_eq!(root.children[1].text, "User");
    assert_eq!(root.children[1].children[0].text, "Shaky hands");
    assert_eq!(ast.diagram_title, "Blurry Photo");
}

#[test]
fn should_support_an_unindented_root_with_nested_causes() {
    let ast = ast("ishikawa-beta\nProblem\nCause A\n  Subcause A1\nCause B\n");
    let root = root(&ast);
    assert_eq!(root.text, "Problem");
    assert_eq!(root.children.len(), 2);
    assert_eq!(root.children[0].text, "Cause A");
    assert_eq!(root.children[0].children[0].text, "Subcause A1");
    assert_eq!(root.children[1].text, "Cause B");
}

#[test]
fn should_handle_effect_indented_more_than_causes() {
    let ast = ast("ishikawa-beta\n    Problem\nCause A\n  Subcause A1\nCause B\n");
    let root = root(&ast);
    assert_eq!(root.text, "Problem");
    assert_eq!(root.children.len(), 2);
    assert_eq!(root.children[0].text, "Cause A");
    assert_eq!(root.children[0].children.len(), 1);
    assert_eq!(root.children[0].children[0].text, "Subcause A1");
    assert_eq!(root.children[1].text, "Cause B");
}

// Behavior of the grammar, the lexer, and the database the spec does not pin down.

#[test]
fn parses_a_root_with_no_causes() {
    let ast = ast("ishikawa-beta\n  Problem\n");
    let root = root(&ast);
    assert_eq!(root.text, "Problem");
    assert!(root.children.is_empty());
    assert_eq!(ast.diagram_title, "Problem");
}

#[test]
fn a_document_of_blank_and_comment_lines_has_no_root() {
    for body in [
        "ishikawa\n\n",
        "ishikawa\n%% only a comment\n\n",
        "ishikawa  ",
        "\n\n%% a comment\nishikawa\n\n",
    ] {
        let ast = ast(body);
        assert_eq!(ast.root, None, "{body:?}");
        assert_eq!(ast.diagram_title, "", "{body:?}");
    }
}

#[test]
fn rejects_a_header_with_no_statement() {
    // `ishikawa: ISHIKAWA document` and `document: statement stop` need a statement;
    // a single line break is not one (`ishikawa\n\n` is a `SPACELINE`, above).
    let failure = error("ishikawa\n");
    assert_eq!(place(&failure), (2, 1));
    assert!(failure.message.contains("the end of the input"));

    let failure = error("ishikawa");
    assert_eq!(place(&failure), (1, 9));
}

#[test]
fn rejects_an_indented_header() {
    // `start: spaceLines ishikawa`: a blank or comment line may precede the header, an
    // indentation may not.
    let error = error("  ishikawa\n a");
    assert_eq!(place(&error), (1, 1));
    assert!(error.message.contains("'ishikawa'"));
    assert!(error.message.contains("indentation"));
}

#[test]
fn rejects_an_empty_line_between_the_header_and_the_first_statement() {
    // `\n\n` is one `SPACELINE`, which is a statement that adds nothing; the indented line
    // after it then has no `stop` to follow it.
    let error = error("ishikawa\n\n a");
    assert_eq!(place(&error), (3, 1));
    assert!(
        error
            .message
            .contains("a line break or the end of the diagram")
    );
}

#[test]
fn skips_comment_lines_anywhere() {
    let ast = ast("ishikawa\n%% first\n    Blurry Photo\n    %% mid\n    Process\n");
    let root = root(&ast);
    assert_eq!(root.text, "Blurry Photo");
    assert_eq!(children(root), ["Process"]);
}

#[test]
fn counts_tabs_as_one_indentation_column() {
    let ast = ast("ishikawa\n\ta\n\t\tb\n");
    let root = root(&ast);
    assert_eq!(root.text, "a");
    assert_eq!(children(root), ["b"]);
}

#[test]
fn matches_the_header_case_insensitively() {
    for header in ["ISHIKAWA", "Ishikawa", "ishikawa-BETA", "ISHIKAWA-BETA"] {
        let body = format!("{header}\n A\n");
        let ast = ast(&body);
        assert_eq!(root(&ast).text, "A", "header: {header}");
    }
}

#[test]
fn requires_a_word_boundary_after_the_header() {
    // Jison appends `\b` to the quoted literals: `ishikawa-betaX` falls back to the
    // `ishikawa` rule and leaves `-betaX` on the header line, while `ishikawaX` is text.
    let ast = ast("ishikawa-betaX\n A\n");
    assert_eq!(root(&ast).text, "-betaX");
    assert_eq!(children(root(&ast)), ["A"]);

    let error = error("ishikawaX\n A\n");
    assert_eq!(place(&error), (1, 1));
    assert!(error.message.contains("'ishikawaX'"));
}

#[test]
fn a_text_on_the_header_line_becomes_the_first_node() {
    // `ishikawa: ISHIKAWA document`: no line break is required between the two.
    let ast = ast("ishikawa Blurry Photo\n");
    assert_eq!(root(&ast).text, "Blurry Photo");
}

#[test]
fn clamps_a_cause_indented_less_than_the_first_cause() {
    // `baseLevel` is the first cause's indentation, and a smaller one becomes level 1.
    let ast = ast("ishikawa\n    a\n  b\n c");
    let root = root(&ast);
    assert_eq!(root.text, "a");
    assert_eq!(children(root), ["b", "c"]);
}

#[test]
fn pops_to_the_parent_with_a_strictly_smaller_level() {
    let ast = ast("ishikawa\n a\n  b\n   c\n  d\n e");
    let root = root(&ast);
    assert_eq!(root.text, "a");
    assert_eq!(children(root), ["b", "d", "e"]);
    assert_eq!(children(&root.children[0]), ["c"]);
}

#[test]
fn nests_the_fishbone_as_the_docs_draw_it() {
    let ast = ast(
        "ishikawa-beta\n  Server Outage\n  Hardware\n      Disk\n          Bad sectors\n          Full capacity\n      Memory\n          Leak detected\n  Software\n      Bug\n          Race condition\n",
    );
    let root = root(&ast);
    assert_eq!(children(root), ["Hardware", "Software"]);
    assert_eq!(children(&root.children[0]), ["Disk", "Memory"]);
    assert_eq!(
        children(&root.children[0].children[0]),
        ["Bad sectors", "Full capacity"]
    );
    assert_eq!(children(&root.children[1].children[0]), ["Race condition"]);
}

#[test]
fn accepts_a_trailing_indented_line_with_nothing_after_it() {
    // `statement: SPACELIST` adds no node and is ended by `stop: EOF`.
    let ast = ast("ishikawa\n a\n b\n ");
    let root = root(&ast);
    assert_eq!(children(root), ["b"]);
}

#[test]
fn rejects_a_second_header_in_the_middle_of_the_document() {
    let error = error("ishikawa\n a\nishikawa\n b");
    assert_eq!(place(&error), (3, 1));
    assert!(error.message.contains("'ishikawa'"));
}

#[test]
fn rejects_an_indented_line_after_a_comment_line_ends_a_statement() {
    // `a` ends with a `SPACELINE` (the comment line), which no statement can follow unless
    // a line break comes first.
    let error = error("ishikawa\n a\n%% c\n\n b");
    assert_eq!(place(&error), (5, 1));
}

#[test]
fn keeps_a_carriage_return_that_a_caller_passed_in() {
    // `TEXT` is `[^\n]+`, so `\r` survives in a label; `crate::preprocess` normalizes line
    // endings before the parser sees them, so this only happens on a direct `parse` call.
    let ast = ast("ishikawa\n a\rb\n");
    assert_eq!(root(&ast).text, "a\rb");

    // A raw `\r\n` is a `SPACELINE` (whitespace with a line break before its end), so the
    // indented line after the header has no statement to belong to.
    let error = error("ishikawa\r\n a\r\n");
    assert_eq!(place(&error), (2, 1));
}

#[test]
fn sanitizes_labels() {
    // `addNode` runs `sanitizeText`, as `sanitize.rs` approximates it.
    assert_eq!(root(&ast("ishikawa\n a < b")).text, "a &lt; b");
    assert_eq!(root(&ast("ishikawa\n a<br/>b")).text, "a<br>b");
    assert_eq!(
        root(&ast("ishikawa\n <script>alert(1)</script>a")).text,
        "a"
    );
    assert_eq!(root(&ast("ishikawa\n <style>p{}</style>b")).text, "b");
}

#[test]
fn trims_the_label_but_not_its_inner_whitespace() {
    let ast = ast("ishikawa\n a  b\t\n");
    assert_eq!(root(&ast).text, "a  b");
}

#[test]
fn parses_every_line_ending_after_preprocessing() {
    // The crate's entry point normalizes `\r\n` and `\r`, which is what the diagrams get.
    let source = crate::preprocess::preprocess("ishikawa\r\n  a\r\n    b\r\n");
    let ast = ast(&source.body);
    let root = root(&ast);
    assert_eq!(root.text, "a");
    assert_eq!(children(root), ["b"]);
}
