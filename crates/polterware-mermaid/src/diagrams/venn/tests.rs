//! Tests ported from the upstream venn spec,
//! `packages/mermaid/src/diagrams/venn/parser/venn.spec.ts` (the grammar and the database it
//! fills; upstream has no separate `vennDB.spec.ts`). Inputs and expectations are upstream's.
//!
//! Skipped, and why:
//! - `vennRenderer.spec.ts` and `vennPalette.spec.ts` render and read theme configuration;
//!   nothing here lays out, draws, or owns configuration. (The palette spec is also not a
//!   parser or database spec.)
//! - The spec's `beforeEach` assigns `venn.parser.yy = db` and calls `db.clear()`. Here each
//!   [`parse`] builds a fresh database and returns it, which is the same isolation.
//! - The spec's `expect(...).toThrow()` cases assert a thrown error. Here `parse` returns
//!   `Err`, and the message cases assert upstream's exact text.
//!
//! After the ported cases come tests for behavior the spec does not pin down: the header and
//! keyword case-insensitivity, comments, the `TEXT` statement and quoted ids, the indentation
//! rules, number forms, label and style normalization, title replacement and sanitizing, the
//! spans errors carry, and CRLF input.

use super::{Ast, StyleEntry, Subset, TextNode, parse};
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

fn subset(sets: &[&str], size: f64, label: Option<&str>) -> Subset {
    Subset {
        sets: sets.iter().map(|set| set.to_string()).collect(),
        size,
        label: label.map(str::to_string),
    }
}

fn text(sets: &[&str], id: &str, label: Option<&str>) -> TextNode {
    TextNode {
        sets: sets.iter().map(|set| set.to_string()).collect(),
        id: id.to_string(),
        label: label.map(str::to_string),
    }
}

fn style(targets: &[&str], styles: &[(&str, &str)]) -> StyleEntry {
    StyleEntry {
        targets: targets.iter().map(|target| target.to_string()).collect(),
        styles: styles
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect(),
    }
}

// packages/mermaid/src/diagrams/venn/parser/venn.spec.ts

#[test]
fn parses_a_simple_diagram() {
    let ast = ast(r#"venn-beta
          title foo bar
          set A
          set B
          union A,B
      "#);
    assert_eq!(ast.diagram_title, "foo bar");
    assert_eq!(
        ast.subsets,
        [
            subset(&["A"], 10.0, None),
            subset(&["B"], 10.0, None),
            subset(&["A", "B"], 2.5, None),
        ]
    );
}

#[test]
fn parses_bracket_labels() {
    let ast = ast(r#"venn-beta
          set A["Alpha"]
          set B["Beta"]
          union A,B["AB"]
      "#);
    assert_eq!(
        ast.subsets,
        [
            subset(&["A"], 10.0, Some("Alpha")),
            subset(&["B"], 10.0, Some("Beta")),
            subset(&["A", "B"], 2.5, Some("AB")),
        ]
    );
}

#[test]
fn parses_unquoted_bracket_labels() {
    let ast = ast(r#"venn-beta
          set A[Alpha]
          set B[Beta]
          union A,B[AB]
      "#);
    assert_eq!(
        ast.subsets,
        [
            subset(&["A"], 10.0, Some("Alpha")),
            subset(&["B"], 10.0, Some("Beta")),
            subset(&["A", "B"], 2.5, Some("AB")),
        ]
    );
}

#[test]
fn parses_sizes() {
    let ast = ast(r#"venn-beta
          set A:20
          set B:12
          union A,B:3
      "#);
    assert_eq!(
        ast.subsets,
        [
            subset(&["A"], 20.0, None),
            subset(&["B"], 12.0, None),
            subset(&["A", "B"], 3.0, None),
        ]
    );
}

#[test]
fn parses_bracket_labels_with_sizes() {
    let ast = ast(r#"venn-beta
          title foo bar
          set A["Alpha"]:20
          set B["Beta"]:12
          set C["Gamma"]:30
          union A,B["AB"]:5.3
          union C,A,B:1
      "#);
    assert_eq!(
        ast.subsets,
        [
            subset(&["A"], 20.0, Some("Alpha")),
            subset(&["B"], 12.0, Some("Beta")),
            subset(&["C"], 30.0, Some("Gamma")),
            subset(&["A", "B"], 5.3, Some("AB")),
            subset(&["A", "B", "C"], 1.0, None),
        ]
    );
}

#[test]
fn parses_text_nodes_with_bracket_labels() {
    let ast = ast(r#"venn-beta
          set A
            text A1["foo bar"]
          set B
            text B1["hello, world"]
          union A,B
            text AB1["shared note"]
      "#);
    assert_eq!(
        ast.texts,
        [
            text(&["A"], "A1", Some("foo bar")),
            text(&["B"], "B1", Some("hello, world")),
            text(&["A", "B"], "AB1", Some("shared note")),
        ]
    );
}

#[test]
fn parses_indented_text_nodes_without_a_label() {
    let ast = ast(r#"venn-beta
          set A["Frontend"]
            text A1
            text A2
          set B["Backend"]
            text B1
          union A,B["APIs"]
      "#);
    assert_eq!(
        ast.texts,
        [
            text(&["A"], "A1", None),
            text(&["A"], "A2", None),
            text(&["B"], "B1", None),
        ]
    );
}

#[test]
fn parses_a_style_statement_for_a_single_set() {
    let ast = ast(r#"venn-beta
          set A
          set B
          union A,B
          style A fill:#ff6b6b
      "#);
    assert_eq!(ast.styles, [style(&["A"], &[("fill", "#ff6b6b")])]);
}

#[test]
fn parses_a_style_statement_for_an_intersection() {
    let ast = ast(r#"venn-beta
          set A
          set B
          union A,B
          style A,B color:#333
      "#);
    assert_eq!(ast.styles, [style(&["A", "B"], &[("color", "#333")])]);
}

#[test]
fn parses_a_style_statement_for_a_text_node() {
    let ast = ast(r#"venn-beta
          set A
            text A1["React"]
          style A1 color:red
      "#);
    assert_eq!(ast.styles, [style(&["A1"], &[("color", "red")])]);
}

#[test]
fn parses_multiple_style_properties() {
    let ast = ast(r#"venn-beta
          set A
          set B
          union A,B
          style A fill:#ff6b6b, color:#333
      "#);
    assert_eq!(
        ast.styles,
        [style(&["A"], &[("fill", "#ff6b6b"), ("color", "#333")])]
    );
}

#[test]
fn parses_rgb_and_rgba_style_values() {
    let ast = ast(r#"venn-beta
          set A
          set B
          style A fill:rgb(255, 0, 128)
          style B fill:rgba(255, 0, 128, 0.5)
      "#);
    assert_eq!(
        ast.styles,
        [
            style(&["A"], &[("fill", "rgb(255, 0, 128)")]),
            style(&["B"], &[("fill", "rgba(255, 0, 128, 0.5)")]),
        ]
    );
}

#[test]
fn parses_indented_text_alongside_styles() {
    let ast = ast(r#"venn-beta
          set A["Frontend"]:20
            text A1["React"]
          set B["Backend"]:12
          union A,B["Shared"]:3
            text AB1["OpenAPI"]
          style A fill:#ff6b6b
          style A,B color:#333
          style A1 color:red
      "#);
    assert_eq!(
        ast.subsets,
        [
            subset(&["A"], 20.0, Some("Frontend")),
            subset(&["B"], 12.0, Some("Backend")),
            subset(&["A", "B"], 3.0, Some("Shared")),
        ]
    );
    assert_eq!(
        ast.texts,
        [
            text(&["A"], "A1", Some("React")),
            text(&["A", "B"], "AB1", Some("OpenAPI")),
        ]
    );
    assert_eq!(
        ast.styles,
        [
            style(&["A"], &[("fill", "#ff6b6b")]),
            style(&["A", "B"], &[("color", "#333")]),
            style(&["A1"], &[("color", "red")]),
        ]
    );
}

#[test]
fn rejects_a_set_with_multiple_identifiers() {
    assert!(parse("venn-beta\n        set A,B\n    ").is_err());
}

#[test]
fn rejects_a_union_with_a_single_identifier() {
    let error = error("venn-beta\n        union A\n    ");
    assert_eq!(error.message, "union requires multiple identifiers");
}

#[test]
fn rejects_a_union_with_an_unknown_identifier() {
    let error = error("venn-beta\n        set Foo\n        union Foo,Buz\n    ");
    assert_eq!(error.message, "unknown set identifier: Buz");
}

#[test]
fn parses_quoted_identifiers() {
    let ast = ast(r#"venn-beta
        set "Foo Bar"
        set Buz
        union "Foo Bar",Buz
    "#);
    assert_eq!(
        ast.subsets,
        [
            subset(&["Foo Bar"], 10.0, None),
            subset(&["Buz"], 10.0, None),
            subset(&["Buz", "Foo Bar"], 2.5, None),
        ]
    );
}

// Behavior the spec does not pin down.

#[test]
fn parses_the_header_alone() {
    let ast = ast("venn-beta");
    assert_eq!(ast, Ast::default());
}

#[test]
fn keywords_are_case_insensitive() {
    let ast = ast("VENN-BETA\n  SET A\n  Set B\n  Union A,B[AB]\n  STYLE A fill:RED\n");
    assert_eq!(ast.subsets.len(), 3);
    assert_eq!(ast.subsets[2].label.as_deref(), Some("AB"));
    assert_eq!(ast.styles, [style(&["A"], &[("fill", "RED")])]);
}

#[test]
fn statements_need_no_line_break_between_them() {
    let ast = ast("venn-beta set A set B union A,B");
    assert_eq!(
        ast.subsets,
        [
            subset(&["A"], 10.0, None),
            subset(&["B"], 10.0, None),
            subset(&["A", "B"], 2.5, None),
        ]
    );
}

#[test]
fn skips_comment_lines_and_inline_comments() {
    let ast = ast(r#"%% leading comment
venn-beta
  set A %% trailing comment
  %% a whole comment line
  set B
  union A,B
%% trailing comment
"#);
    assert_eq!(ast.subsets.len(), 3);
}

#[test]
fn accepts_a_tab_indentation() {
    let ast = ast("venn-beta\n\tset A\n\t\ttext A1\n");
    assert_eq!(ast.texts, [text(&["A"], "A1", None)]);
}

#[test]
fn a_statement_line_resets_indent_mode() {
    let ast = ast(r#"venn-beta
set A
  text A1
set B
  text B1
"#);
    assert_eq!(
        ast.texts,
        [text(&["A"], "A1", None), text(&["B"], "B1", None)]
    );
}

#[test]
fn parses_the_explicit_text_statement() {
    let ast = ast("venn-beta\nset A\nset B\ntext A,B note[\"label\"]\n");
    assert_eq!(ast.texts, [text(&["A", "B"], "note", Some("label"))]);
}

#[test]
fn parses_a_quoted_text_id() {
    let ast = ast("venn-beta\nset A\ntext A \"shared node\"\n");
    assert_eq!(ast.texts, [text(&["A"], "shared node", None)]);
}

#[test]
fn parses_a_numeric_text_id() {
    let ast = ast("venn-beta\nset A\ntext A 42\n");
    assert_eq!(ast.texts, [text(&["A"], "42", None)]);
}

#[test]
fn rejects_a_text_statement_without_a_value() {
    assert!(parse("venn-beta\nset A\ntext A\n").is_err());
}

#[test]
fn parses_decimal_and_signed_sizes() {
    let ast = ast("venn-beta\n  set A:.5\n  set B:+2\n  union A,B:-3.5\n");
    assert_eq!(
        ast.subsets,
        [
            subset(&["A"], 0.5, None),
            subset(&["B"], 2.0, None),
            subset(&["A", "B"], -3.5, None),
        ]
    );
}

#[test]
fn a_three_set_union_defaults_to_ten_thirds() {
    let ast = ast("venn-beta\nset A\nset B\nset C\nunion A,B,C\n");
    assert_eq!(ast.subsets[3], subset(&["A", "B", "C"], 10.0 / 9.0, None));
}

#[test]
fn labels_are_trimmed() {
    let ast = ast("venn-beta\nset A[  Alpha  ]\nset B[\"  Beta  \"]\n");
    assert_eq!(ast.subsets[0].label.as_deref(), Some("Alpha"));
    assert_eq!(ast.subsets[1].label.as_deref(), Some("Beta"));
}

#[test]
fn an_empty_bracket_label_is_no_label() {
    let ast = ast("venn-beta\nset A[\"\"]\n");
    assert_eq!(ast.subsets[0].label, None);
}

#[test]
fn style_targets_are_sorted_and_values_are_normalized() {
    let ast = ast(r##"venn-beta
set A
set B
union A,B
style B,A fill:"#fff", stroke:red dashed
"##);
    assert_eq!(
        ast.styles,
        [style(
            &["A", "B"],
            &[("fill", "#fff"), ("stroke", "red dashed")]
        )]
    );
}

#[test]
fn a_repeated_style_property_keeps_its_first_position() {
    let ast = ast("venn-beta\nset A\nstyle A fill:red, color:blue, fill:green\n");
    assert_eq!(
        ast.styles,
        [style(&["A"], &[("fill", "green"), ("color", "blue")])]
    );
    assert_eq!(ast.styles[0].style("fill"), Some("green"));
    assert_eq!(ast.styles[0].style("missing"), None);
}

#[test]
fn a_later_title_replaces_an_earlier_one() {
    let ast = ast("venn-beta\ntitle first\ntitle second\n");
    assert_eq!(ast.diagram_title, "second");
}

#[test]
fn a_title_drops_only_the_first_whitespace_character() {
    let ast = ast("venn-beta\n  title   spaced  \n");
    assert_eq!(ast.diagram_title, "  spaced  ");
}

#[test]
fn a_title_is_sanitized() {
    let ast = ast("venn-beta\ntitle <script>alert(1)</script>kept\n");
    assert_eq!(ast.diagram_title, "kept");
}

#[test]
fn parses_crlf_line_endings() {
    let ast = ast("venn-beta\r\n  set A\r\n  set B\r\n  union A,B\r\n");
    assert_eq!(ast.subsets.len(), 3);
}

#[test]
fn rejects_a_body_that_does_not_start_with_the_header() {
    assert!(parse("").is_err());
    assert!(parse("  \n  ").is_err());
    assert!(parse("set A").is_err());
}

#[test]
fn the_header_must_come_first() {
    assert!(parse("venn-beta\nvenn-beta\n").is_err());
}

#[test]
fn parses_the_syntax_documentation_example() {
    let ast = ast(r#"venn-beta
  title What makes a good feature
  set Desirable
  set Feasible
  set Viable
  union Desirable,Feasible["Buildable"]
  union Feasible,Viable["Sustainable"]
  union Desirable,Viable["Marketable"]
  union Desirable,Feasible,Viable["Ship it"]
"#);
    assert_eq!(ast.diagram_title, "What makes a good feature");
    assert_eq!(ast.subsets.len(), 7);
    assert_eq!(
        ast.subsets[6],
        subset(
            &["Desirable", "Feasible", "Viable"],
            10.0 / 9.0,
            Some("Ship it")
        )
    );
}

#[test]
fn an_unknown_union_lists_every_unknown_identifier_in_order() {
    let error = error("venn-beta\nset A\nunion A,Buz,Qux\n");
    assert_eq!(error.message, "unknown set identifier: Buz, Qux");
}

#[test]
fn an_unknown_union_reports_the_union_span() {
    let error = error("venn-beta\n  set Foo\n  union Foo,Buz\n");
    let span = error.span.expect("the error carries a span");
    assert_eq!((span.line, span.column), (3, 3));
}

#[test]
fn a_stray_character_reports_its_span() {
    let error = error("venn-beta\n  set A\n  ?");
    let span = error.span.expect("the error carries a span");
    assert_eq!((span.line, span.column), (3, 3));
    assert!(error.message.contains('?'));
}

#[test]
fn an_unterminated_bracket_reports_the_opening_bracket() {
    let error = error("venn-beta\nset A[\"Alpha\"\n");
    let span = error.span.expect("the error carries a span");
    assert_eq!((span.line, span.column), (2, 6));
}

#[test]
fn a_colon_without_a_size_is_an_error() {
    assert!(parse("venn-beta\nset A:\n").is_err());
}

#[test]
fn a_style_without_a_value_is_an_error() {
    assert!(parse("venn-beta\nset A\nstyle A fill:\n").is_err());
}

#[test]
fn a_style_without_a_field_is_an_error() {
    assert!(parse("venn-beta\nset A\nstyle A\n").is_err());
}

#[test]
fn rgb_needs_three_components_and_rgba_four() {
    assert!(parse("venn-beta\nset A\nstyle A fill:rgb(1, 2)\n").is_err());
    assert!(parse("venn-beta\nset A\nstyle A fill:rgba(1, 2, 3)\n").is_err());
}

#[test]
fn a_hex_color_needs_three_to_eight_digits() {
    let ast = ast("venn-beta\nset A\nstyle A fill:#abc, stroke:#abcdef01\n");
    assert_eq!(ast.styles[0].style("fill"), Some("#abc"));
    assert_eq!(ast.styles[0].style("stroke"), Some("#abcdef01"));
    assert!(parse("venn-beta\nset A\nstyle A fill:#ab\n").is_err());
}
