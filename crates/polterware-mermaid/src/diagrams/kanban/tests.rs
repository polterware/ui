//! Tests ported from `packages/mermaid/src/diagrams/kanban/kanban.spec.ts` (the grammar and
//! the database). Inputs and expectations are upstream's.
//!
//! Skipped, and why:
//! - `setLogLevel('trace')` and the logger calls are not ported; there is no logger.
//!
//! Adapted:
//! - The `data.nodes.filter(...)` steps are [`Ast::children`], the same filter the spec uses.
//! - `kanban.spec.ts` has no config assertion, but the configuration fields `getData` copies
//!   (`look`, `padding`, `width`) are absent from [`super::DataNode`] because this crate owns
//!   no configuration. There is no kanban renderer spec to port.

use serde_json::json;

use super::{Ast, Node, NodeType, parse};
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

fn children<'a>(ast: &'a Ast, section: &'a Node) -> Vec<&'a Node> {
    ast.children(&section.id).collect()
}

fn labels<'a>(nodes: &[&'a Node]) -> Vec<&'a str> {
    nodes.iter().map(|node| node.label.as_str()).collect()
}

// kanban.spec.ts, describe('hierarchy')

#[test]
fn knbn_1_handles_a_simple_root_definition() {
    let ast = ast("kanban\n    root");
    assert_eq!(ast.sections.len(), 1);
    assert_eq!(ast.sections[0].label, "root");
}

#[test]
fn knbn_2_handles_a_hierarchical_kanban_definition() {
    let ast = ast("kanban\n    root\n      child1\n      child2\n ");
    let section = &ast.sections[0];
    let children = children(&ast, section);
    assert_eq!(ast.sections.len(), 1);
    assert_eq!(section.label, "root");
    assert_eq!(children.len(), 2);
    assert_eq!(labels(&children), ["child1", "child2"]);
}

#[test]
fn knbn_3_handles_a_root_with_a_shape_and_without_an_id() {
    let ast = ast("kanban\n    (root)");
    assert_eq!(ast.sections[0].label, "root");
}

#[test]
fn knbn_4_does_not_distinguish_deeper_levels() {
    let ast = ast("kanban\n    root\n      child1\n        leaf1\n      child2");
    let section = &ast.sections[0];
    assert_eq!(ast.sections.len(), 1);
    assert_eq!(children(&ast, section).len(), 3);
}

#[test]
fn knbn_5_multiple_sections_are_ok() {
    let ast = ast("kanban\n    section1\n    section2");
    assert_eq!(ast.sections.len(), 2);
    assert_eq!(ast.sections[0].label, "section1");
    assert_eq!(ast.sections[1].label, "section2");
}

#[test]
fn knbn_6_real_root_in_wrong_place_fails() {
    let error = error("kanban\n          root\n        fakeRoot\n    realRootWrongPlace");
    assert_eq!(
        error.message,
        "Items without section detected, found section (\"fakeRoot\")"
    );
}

// kanban.spec.ts, describe('nodes')

#[test]
fn knbn_7_handles_an_id_and_type_for_a_section() {
    let ast = ast("kanban\n    root[The root]\n      ");
    let section = &ast.sections[0];
    assert_eq!(section.id, "root");
    assert_eq!(section.label, "The root");
    assert_eq!(section.node_type, NodeType::Rect);
}

#[test]
fn knbn_8_handles_an_id_and_type_for_a_child() {
    let ast = ast("kanban\n    root\n      theId(child1)");
    let section = &ast.sections[0];
    let children = children(&ast, section);
    assert_eq!(section.label, "root");
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].label, "child1");
    assert_eq!(children[0].id, "theId");
}

#[test]
fn knbn_9_handles_an_unindented_root_and_a_child() {
    let ast = ast("kanban\nroot\n      theId(child1)");
    let section = &ast.sections[0];
    let children = children(&ast, section);
    assert_eq!(section.label, "root");
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].label, "child1");
    assert_eq!(children[0].id, "theId");
}

// kanban.spec.ts, describe('decorations')

#[test]
fn knbn_13_sets_an_icon_for_the_node() {
    let ast = ast("kanban\n    root[The root]\n    ::icon(bomb)\n    ");
    let section = &ast.sections[0];
    assert_eq!(section.id, "root");
    assert_eq!(section.label, "The root");
    assert_eq!(section.icon.as_deref(), Some("bomb"));
}

#[test]
fn knbn_14_sets_classes_for_the_node() {
    let ast = ast("kanban\n    root[The root]\n    :::m-4 p-8\n    ");
    let section = &ast.sections[0];
    assert_eq!(section.id, "root");
    assert_eq!(section.label, "The root");
    assert_eq!(section.css_classes.as_deref(), Some("m-4 p-8"));
}

#[test]
fn knbn_15_sets_both_classes_and_icon() {
    let ast = ast("kanban\n    root[The root]\n    :::m-4 p-8\n    ::icon(bomb)\n    ");
    let section = &ast.sections[0];
    assert_eq!(section.css_classes.as_deref(), Some("m-4 p-8"));
    assert_eq!(section.icon.as_deref(), Some("bomb"));
}

#[test]
fn knbn_16_sets_both_classes_and_icon_in_the_other_order() {
    let ast = ast("kanban\n    root[The root]\n    ::icon(bomb)\n    :::m-4 p-8\n    ");
    let section = &ast.sections[0];
    assert_eq!(section.css_classes.as_deref(), Some("m-4 p-8"));
    assert_eq!(section.icon.as_deref(), Some("bomb"));
}

// kanban.spec.ts, describe('descriptions')

#[test]
fn knbn_17_accepts_brackets_in_a_quoted_description() {
    let ast = ast("kanban\n    root[\"String containing []\"]\n");
    assert_eq!(ast.sections[0].id, "root");
    assert_eq!(ast.sections[0].label, "String containing []");
}

#[test]
fn knbn_18_accepts_node_syntax_in_child_descriptions() {
    let ast =
        ast("kanban\n    root[\"String containing []\"]\n      child1[\"String containing ()\"]\n");
    let section = &ast.sections[0];
    let children = children(&ast, section);
    assert_eq!(section.label, "String containing []");
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].label, "String containing ()");
}

#[test]
fn knbn_19_accepts_a_child_after_a_class_assignment() {
    let ast =
        ast("kanban\n  root(Root)\n    Child(Child)\n    :::hot\n      a(a)\n      b[New Stuff]");
    let section = &ast.sections[0];
    let children = children(&ast, section);
    assert_eq!(section.id, "root");
    assert_eq!(section.label, "Root");
    assert_eq!(children.len(), 3);
    assert_eq!(children[0].id, "Child");
    assert_eq!(children[1].id, "a");
    assert_eq!(children[2].id, "b");
}

#[test]
fn knbn_20_accepts_meaningless_empty_rows() {
    let ast = ast("kanban\n  root(Root)\n    Child(Child)\n      a(a)\n\n      b[New Stuff]");
    let section = &ast.sections[0];
    let children = children(&ast, section);
    assert_eq!(section.label, "Root");
    assert_eq!(children.len(), 3);
    assert_eq!(children[0].id, "Child");
    assert_eq!(children[1].id, "a");
    assert_eq!(children[2].id, "b");
}

#[test]
fn knbn_21_accepts_comment_lines() {
    let ast = ast(
        "kanban\n  root(Root)\n    Child(Child)\n      a(a)\n\n      %% This is a comment\n      b[New Stuff]",
    );
    let section = &ast.sections[0];
    let children = children(&ast, section);
    assert_eq!(section.label, "Root");
    assert_eq!(children[0].id, "Child");
    assert_eq!(children[1].id, "a");
    assert_eq!(children[2].id, "b");
    assert_eq!(children.len(), 3);
}

#[test]
fn knbn_22_accepts_comments_at_the_end_of_a_line() {
    let ast = ast(
        "kanban\n  root(Root)\n    Child(Child)\n      a(a) %% This is a comment\n      b[New Stuff]",
    );
    let section = &ast.sections[0];
    let children = children(&ast, section);
    assert_eq!(section.label, "Root");
    assert_eq!(children.len(), 3);
    assert_eq!(children[0].id, "Child");
    assert_eq!(children[1].id, "a");
    assert_eq!(children[2].id, "b");
}

#[test]
fn knbn_23_rows_with_only_spaces_do_not_interfere() {
    let ast = ast("kanban\nroot\n A\n \n\n B");
    let section = &ast.sections[0];
    let children = children(&ast, section);
    assert_eq!(section.id, "root");
    assert_eq!(children.len(), 2);
    assert_eq!(children[0].id, "A");
    assert_eq!(children[1].id, "B");
}

#[test]
fn knbn_24_handles_rows_above_the_declaration() {
    let ast = ast("\n \nkanban\nroot\n A\n \n\n B");
    let section = &ast.sections[0];
    let children = children(&ast, section);
    assert_eq!(section.id, "root");
    assert_eq!(children.len(), 2);
    assert_eq!(children[0].id, "A");
    assert_eq!(children[1].id, "B");
}

#[test]
fn knbn_25_handles_rows_above_the_declaration_without_spaces() {
    let ast = ast("\n\n\nkanban\nroot\n A\n \n\n B");
    let section = &ast.sections[0];
    let children = children(&ast, section);
    assert_eq!(section.id, "root");
    assert_eq!(children.len(), 2);
    assert_eq!(children[0].id, "A");
    assert_eq!(children[1].id, "B");
}

// kanban.spec.ts, describe('item data data')

#[test]
fn knbn_30_sets_the_priority() {
    let ast = ast("kanban\n        root@{ priority: high }\n    ");
    assert_eq!(ast.sections[0].id, "root");
    assert_eq!(ast.sections[0].priority, Some(json!("high")));
}

#[test]
fn knbn_31_sets_the_assignment() {
    let ast = ast("kanban\n        root@{ assigned: knsv }\n    ");
    assert_eq!(ast.sections[0].id, "root");
    assert_eq!(ast.sections[0].assigned.as_deref(), Some("knsv"));
}

#[test]
fn knbn_32_sets_the_icon_from_metadata() {
    let ast = ast("kanban\n        root@{ icon: star }\n    ");
    assert_eq!(ast.sections[0].id, "root");
    assert_eq!(ast.sections[0].icon.as_deref(), Some("star"));
}

#[test]
fn knbn_33_sets_the_icon_from_metadata_again() {
    let ast = ast("kanban\n        root@{ icon: star }\n    ");
    assert_eq!(ast.sections[0].id, "root");
    assert_eq!(ast.sections[0].icon.as_deref(), Some("star"));
}

#[test]
fn knbn_34_sets_metadata_over_multiple_lines() {
    let ast = ast(
        "kanban\n        root@{\n          icon: star\n          assigned: knsv\n        }\n    ",
    );
    assert_eq!(ast.sections[0].id, "root");
    assert_eq!(ast.sections[0].icon.as_deref(), Some("star"));
    assert_eq!(ast.sections[0].assigned.as_deref(), Some("knsv"));
}

#[test]
fn knbn_35_sets_metadata_on_one_line() {
    let ast = ast("kanban\n        root@{ icon: star, assigned: knsv }\n    ");
    assert_eq!(ast.sections[0].id, "root");
    assert_eq!(ast.sections[0].icon.as_deref(), Some("star"));
    assert_eq!(ast.sections[0].assigned.as_deref(), Some("knsv"));
}

#[test]
fn knbn_36_sets_the_label_from_metadata() {
    let ast = ast("kanban\n        root@{ icon: star, label: 'fix things' }\n    ");
    assert_eq!(ast.sections[0].label, "fix things");
}

#[test]
fn knbn_37_sets_the_external_id() {
    let ast = ast("kanban\n        root@{ ticket: MC-1234 }\n    ");
    assert_eq!(ast.sections[0].id, "root");
    assert_eq!(ast.sections[0].ticket.as_deref(), Some("MC-1234"));
}

// Behavior of the lexer, the grammar, and the database the specs above do not pin down.

#[test]
fn get_type_maps_every_delimiter() {
    let cases = [
        ("root[Rect]", NodeType::Rect),
        ("root(Round)", NodeType::RoundedRect),
        ("root((Circle))", NodeType::Circle),
        ("root{{Hex}}", NodeType::Hexagon),
        ("root(cloud(-", NodeType::Cloud),
        ("root))bang))", NodeType::Bang),
        ("(root)", NodeType::RoundedRect),
        ("root", NodeType::NoBorder),
    ];
    for (body, expected) in cases {
        let ast = ast(&format!("kanban\n  {body}"));
        assert_eq!(ast.sections[0].node_type, expected, "body: {body:?}");
    }
    assert_eq!(NodeType::Hexagon.as_str(), "hexgon");
    assert_eq!(NodeType::Rect.number(), 2);
}

#[test]
fn an_empty_sanitized_id_falls_back_to_a_generated_one() {
    let ast = ast("kanban\n  <script></script>");
    assert_eq!(ast.sections[0].id, "kbn0");
}

#[test]
fn a_markdown_description_reads_backticks_and_quotes() {
    let ast = ast("kanban\n  root[\"`a b`\"]");
    assert_eq!(ast.sections[0].label, "a b");
}

#[test]
fn a_quoted_description_may_span_lines() {
    let ast = ast("kanban\n  root[\"one\ntwo\"]");
    assert_eq!(ast.sections[0].label, "one\ntwo");
}

#[test]
fn a_metadata_string_quote_replaces_line_breaks_with_breaks() {
    let ast = ast("kanban\n  root@{ label: \"one\ntwo\" }");
    assert_eq!(ast.sections[0].label, "one<br/>two");
}

#[test]
fn a_metadata_block_scalar_keeps_its_lines() {
    let ast = ast("kanban\n  root@{\n    label: |\n      first\n      second\n  }");
    assert_eq!(ast.sections[0].label, "first\nsecond\n");
}

#[test]
fn a_non_string_label_is_stringified() {
    let ast = ast("kanban\n  root@{ label: 5 }");
    assert_eq!(ast.sections[0].label, "5");
}

#[test]
fn a_numeric_priority_stays_a_number() {
    let ast = ast("kanban\n  root@{ priority: 5 }");
    assert_eq!(ast.sections[0].priority, Some(json!(5)));
}

#[test]
fn an_unknown_lowercase_shape_is_accepted_and_not_applied() {
    let ast = ast("kanban\n  root@{ shape: rect }");
    assert_eq!(ast.sections[0].shape, None);
}

#[test]
fn an_uppercase_shape_is_rejected() {
    let error = error("kanban\n  root@{ shape: Rect }");
    assert_eq!(
        error.message,
        "No such shape: Rect. Shape names should be lowercase."
    );
}

#[test]
fn a_shape_with_an_underscore_is_rejected() {
    let error = error("kanban\n  root@{ shape: rounded_rect }");
    assert_eq!(
        error.message,
        "No such shape: rounded_rect. Shape names should be lowercase."
    );
}

#[test]
fn a_decoration_before_any_item_is_an_error() {
    let error = error("kanban\n  ::icon(bomb)");
    assert!(error.message.contains("cannot decorate"));
    assert!(error.span.is_some());
}

#[test]
fn an_unparsable_metadata_block_is_an_error() {
    let error = error("kanban\n  root@{ icon: [ }");
    assert!(error.span.is_some());
}

#[test]
fn a_decoration_applies_to_the_last_node() {
    let ast = ast("kanban\n  root\n    :::hot\n  other");
    assert_eq!(ast.sections[0].css_classes.as_deref(), Some("hot"));
    assert_eq!(ast.sections[1].css_classes, None);
}

#[test]
fn an_empty_icon_decoration_adds_nothing() {
    let ast = ast("kanban\n  root\n  ::icon()");
    assert_eq!(ast.sections[0].icon, None);
}

#[test]
fn the_data_projection_lists_sections_before_their_items() {
    let ast = ast("kanban\n  first\n    a\n  second\n    b");
    let data = ast.data();
    let ids: Vec<&str> = data.iter().map(|node| node.id.as_str()).collect();
    assert_eq!(ids, ["first", "a", "second", "b"]);
    assert!(data[0].is_group);
    assert_eq!(data[1].parent_id.as_deref(), Some("first"));
    assert_eq!(data[1].shape.as_str(), "kanbanItem");
    assert_eq!(data[1].rx, Some(5.0));
    assert_eq!(data[1].css_styles, ["text-align: left"]);
    assert_eq!(data[2].shape.as_str(), "kanbanSection");
}

#[test]
fn a_bare_header_is_rejected() {
    assert!(parse("kanban").is_err());
    assert!(parse("kanban\n").is_err());
}

#[test]
fn trailing_spaces_after_the_header_are_a_blank_statement() {
    // The whitespace run after `kanban` carries its line break, so it is a `SPACELINE`, not
    // the `NL` that `mindMap: KANBAN NL document` needs; as a statement it then has no
    // `stop` before the first node.
    assert!(parse("kanban  \n  root").is_err());
}

#[test]
fn an_indented_header_is_rejected() {
    // `start` only skips `SPACELINE`/`NL`; the lexer turns the indent into a `SPACELIST`.
    assert!(parse("  kanban\n  root").is_err());
}

#[test]
fn a_blank_line_right_after_the_header_needs_its_own_stop() {
    // `kanban`, `SPACELINE`, `SPACELIST root` is not a `document`: a `SPACELINE` statement
    // must be followed by a `stop`, so a node on the next line cannot follow it directly.
    // Both spellings of the blank line produce a `SPACELINE` after the header.
    assert!(parse("kanban\n \n root").is_err());
    assert!(parse("kanban\n\n root").is_err());
}

#[test]
fn an_error_carries_the_span_of_the_item() {
    // The check runs on the nodes already stored: `lonely` is below the section level but no
    // earlier node is, so it is attached; the next node then finds `lonely`.
    let error = error("kanban\n     root\n   lonely\n  orphan");
    assert_eq!(
        error.message,
        "Items without section detected, found section (\"lonely\")"
    );
    let span = error.span.expect("the error carries a span");
    assert_eq!((span.line, span.column), (4, 3));
}
