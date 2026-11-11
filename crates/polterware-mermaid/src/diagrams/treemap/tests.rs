//! Tests ported from the upstream treemap specs:
//! `packages/parser/tests/treemap.test.ts` (the Langium grammar) and
//! `packages/mermaid/src/diagrams/treemap/utils.test.ts` (`buildHierarchy`). Inputs and
//! expectations are upstream's; the hierarchy cases use the equivalent indentation because
//! they call `buildHierarchy` directly upstream.
//!
//! Skipped, and why:
//! - `expectNoErrorsOrAlternatives` checks Langium's lexer/parser error arrays and that no
//!   `console.log` ran. Here `parse` returning `Ok` is the error-free assertion, and there is
//!   no console.
//! - `result.value.$type` is Langium's generated AST type marker, not database data. The
//!   `ClassDefStatement` row type is checked through `Ast::classes`, and a row's `Section`
//!   versus `Leaf` through `children` (`Some` versus `None`).
//! - `treemap.test.ts`'s two `class` assignment cases ("class myNode myClass") ignore the
//!   parser errors and only assert that a value exists (Langium's error recovery). The
//!   grammar has no `class` statement, so this port asserts the parse error instead.
//! - The accessibility cases say the title, accTitle, and accDescr "can't" be asserted
//!   upstream because they are not exposed through the parser result; this port checks the
//!   database getters, which do return them.
//! - `utils.test.ts` builds `buildHierarchy`'s flat item list by hand; the equivalent
//!   `treemap` text goes through the whole parser here.
//! - `treemap-validator.ts`'s single-root check runs in document validation, not in the
//!   parser, so multiple roots parse (upstream's validation test expects exactly that).

use super::{Ast, TreemapNode, parse};
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

fn node<'a>(ast: &'a Ast, name: &str) -> &'a TreemapNode {
    ast.nodes
        .iter()
        .find(|node| node.name == name)
        .unwrap_or_else(|| panic!("node {name:?} not found in {:?}", names(ast)))
}

fn names(ast: &Ast) -> Vec<&str> {
    ast.nodes.iter().map(|node| node.name.as_str()).collect()
}

fn child_names(node: &TreemapNode) -> Vec<&str> {
    node.children
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|child| child.name.as_str())
        .collect()
}

// packages/parser/tests/treemap.test.ts

#[test]
fn parses_an_empty_treemap() {
    for body in ["treemap", "treemap-beta"] {
        let ast = ast(body);
        assert!(ast.nodes.is_empty(), "{body:?}");
        assert!(ast.levels.is_empty(), "{body:?}");
        assert_eq!(
            ast.root,
            TreemapNode {
                name: String::new(),
                children: Some(Vec::new()),
                ..TreemapNode::default()
            },
            "{body:?}"
        );
        assert!(ast.classes.is_empty(), "{body:?}");
        assert_eq!(ast.diagram_title, "", "{body:?}");
    }
}

#[test]
fn parses_a_section_node() {
    let ast = ast("treemap\n\"Root\"");
    assert_eq!(names(&ast), ["Root"]);
    let root = node(&ast, "Root");
    assert_eq!(root.children, Some(Vec::new()));
    assert_eq!(root.value, None);
    assert_eq!(ast.levels, [0]);
    assert_eq!(child_names(&ast.root), ["Root"]);
}

#[test]
fn parses_a_section_with_leaf_nodes() {
    let ast = ast("treemap\n\"Root\"\n  \"Child1\" , 100\n  \"Child2\" : 200\n");
    assert_eq!(names(&ast), ["Root", "Child1", "Child2"]);
    assert_eq!(ast.levels, [0, 1, 1]);
    assert_eq!(child_names(node(&ast, "Root")), ["Child1", "Child2"]);
    assert_eq!(node(&ast, "Child1").value, Some(100.0));
    assert_eq!(node(&ast, "Child1").children, None);
    assert_eq!(node(&ast, "Child2").value, Some(200.0));
    assert_eq!(node(&ast, "Child2").children, None);
}

#[test]
fn parses_string_names() {
    let ast = ast("treemap\n\"My Section\"");
    assert_eq!(node(&ast, "My Section").children, Some(Vec::new()));
}

#[test]
fn parses_number_values() {
    let ast = ast("treemap\n\"Item\" : 123.45");
    let item = node(&ast, "Item");
    assert_eq!(item.children, None);
    assert_eq!(item.value, Some(123.45));
}

#[test]
fn parses_multiple_root_nodes() {
    let ast = ast("treemap\n\"Root1\"\n\"Root2\"");
    assert_eq!(names(&ast), ["Root1", "Root2"]);
    assert_eq!(ast.levels, [0, 0]);
    assert_eq!(child_names(&ast.root), ["Root1", "Root2"]);
}

#[test]
fn parses_a_treemap_with_a_title() {
    let ast = ast("treemap\ntitle My Treemap Diagram\n\"Root\"\n  \"Child\": 100");
    assert_eq!(ast.diagram_title, "My Treemap Diagram");
    assert_eq!(names(&ast), ["Root", "Child"]);
}

#[test]
fn parses_a_treemap_with_an_acc_title() {
    let ast = ast("treemap\naccTitle: Accessible Title\n\"Root\"\n  \"Child\": 100");
    assert_eq!(ast.acc_title, "Accessible Title");
    assert_eq!(names(&ast), ["Root", "Child"]);
}

#[test]
fn parses_a_treemap_with_an_acc_description() {
    let ast =
        ast("treemap\naccDescr: This is an accessible description\n\"Root\"\n  \"Child\": 100");
    assert_eq!(ast.acc_description, "This is an accessible description");
    assert_eq!(names(&ast), ["Root", "Child"]);
}

#[test]
fn parses_a_treemap_with_multiple_accessibility_attributes() {
    let ast = ast(
        "treemap\ntitle My Treemap Diagram\naccTitle: Accessible Title\naccDescr: This is an accessible description\n\"Root\"\n  \"Child\": 100",
    );
    assert_eq!(ast.diagram_title, "My Treemap Diagram");
    assert_eq!(ast.acc_title, "Accessible Title");
    assert_eq!(ast.acc_description, "This is an accessible description");
    assert_eq!(names(&ast), ["Root", "Child"]);
}

#[test]
fn parses_a_class_def_statement() {
    let ast = ast("treemap\nclassDef myClass fill:red;");
    assert!(ast.nodes.is_empty());
    let class = ast.class("myClass").expect("the class is defined");
    assert_eq!(class.styles, ["fill:red"]);
    assert!(class.text_styles.is_empty());
}

#[test]
fn parses_a_class_def_statement_without_a_semicolon() {
    let ast = ast("treemap\nclassDef myClass fill:red");
    assert_eq!(
        ast.class("myClass").expect("the class is defined").styles,
        ["fill:red"]
    );
}

#[test]
fn parses_a_class_def_statement_with_multiple_style_properties() {
    let ast = ast("treemap\nclassDef complexClass fill:blue stroke:#ff0000 stroke-width:2px");
    assert_eq!(
        ast.class("complexClass")
            .expect("the class is defined")
            .styles,
        ["fill:blue stroke:#ff0000 stroke-width:2px"]
    );
}

#[test]
fn rejects_a_class_assignment_statement() {
    for body in [
        "treemap\nclass myNode myClass",
        "treemap\nclass myNode myClass;",
    ] {
        assert!(parse(body).is_err(), "{body:?}");
    }
}

#[test]
fn parses_a_section_with_an_inline_class() {
    let ast = ast("treemap\n\"My Section\":::sectionClass");
    let section = node(&ast, "My Section");
    assert_eq!(section.children, Some(Vec::new()));
    assert_eq!(section.class_selector.as_deref(), Some("sectionClass"));
    assert_eq!(section.css_compiled_styles, None);
}

#[test]
fn parses_a_leaf_with_an_inline_class() {
    let ast = ast("treemap\n\"My Leaf\" : 100:::leafClass");
    let leaf = node(&ast, "My Leaf");
    assert_eq!(leaf.children, None);
    assert_eq!(leaf.value, Some(100.0));
    assert_eq!(leaf.class_selector.as_deref(), Some("leafClass"));
}

// packages/mermaid/src/diagrams/treemap/utils.test.ts

#[test]
fn builds_a_hierarchy_from_a_flat_structure() {
    let ast = ast(
        "treemap\n\"Root\"\n    \"Branch 1\"\n        \"Leaf 1.1\": 10\n        \"Leaf 1.2\": 15\n    \"Branch 2\"\n        \"Leaf 2.1\": 20\n        \"Leaf 2.2\": 25\n        \"Leaf 2.3\": 30",
    );
    assert_eq!(
        names(&ast),
        [
            "Root", "Branch 1", "Leaf 1.1", "Leaf 1.2", "Branch 2", "Leaf 2.1", "Leaf 2.2",
            "Leaf 2.3",
        ]
    );
    assert_eq!(ast.levels, [0, 1, 2, 2, 1, 2, 2, 2]);
    let root = node(&ast, "Root");
    assert_eq!(child_names(root), ["Branch 1", "Branch 2"]);
    assert_eq!(
        child_names(node(&ast, "Branch 1")),
        ["Leaf 1.1", "Leaf 1.2"]
    );
    assert_eq!(
        child_names(node(&ast, "Branch 2")),
        ["Leaf 2.1", "Leaf 2.2", "Leaf 2.3"]
    );
    assert_eq!(node(&ast, "Leaf 1.1").value, Some(10.0));
    assert_eq!(node(&ast, "Leaf 2.3").value, Some(30.0));
}

#[test]
fn handles_empty_input_in_the_hierarchy() {
    let ast = ast("treemap");
    assert!(ast.nodes.is_empty());
    assert_eq!(child_names(&ast.root), Vec::<&str>::new());
}

#[test]
fn handles_only_root_nodes_in_the_hierarchy() {
    let ast = ast("treemap\n\"Root 1\"\n\"Root 2\"");
    assert_eq!(nodes_children(&ast), [Some(0), Some(0)]);
    assert_eq!(child_names(&ast.root), ["Root 1", "Root 2"]);
}

#[test]
fn handles_complex_nesting_levels() {
    let ast = ast(
        "treemap\n\"Root\"\n  \"Level 1\"\n    \"Level 2\"\n      \"Leaf 1\": 10\n    \"Level 2 again\"\n      \"Leaf 2\": 20",
    );
    assert_eq!(
        names(&ast),
        [
            "Root",
            "Level 1",
            "Level 2",
            "Leaf 1",
            "Level 2 again",
            "Leaf 2"
        ]
    );
    assert_eq!(ast.levels, [0, 1, 2, 3, 2, 3]);
    assert_eq!(
        child_names(node(&ast, "Level 1")),
        ["Level 2", "Level 2 again"]
    );
    assert_eq!(child_names(node(&ast, "Level 2")), ["Leaf 1"]);
    assert_eq!(child_names(node(&ast, "Level 2 again")), ["Leaf 2"]);
}

fn nodes_children(ast: &Ast) -> Vec<Option<usize>> {
    ast.nodes
        .iter()
        .map(|node| node.children.as_ref().map(Vec::len))
        .collect()
}

// Behavior of the lexer, parser, and database the specs above do not pin down.

#[test]
fn ignores_whole_line_comments_including_their_indentation() {
    let ast = ast(
        "treemap-beta\n  %% This is a comment\n  \"Category A\"\n    \"Item A1\": 10\n    \"Item A2\": 20\n  %% Another comment\n  \"Category B\"\n    \"Item B1\": 15\n    \"Item B2\": 25",
    );
    assert_eq!(
        names(&ast),
        [
            "Category A",
            "Item A1",
            "Item A2",
            "Category B",
            "Item B1",
            "Item B2",
        ]
    );
    assert_eq!(ast.levels, [0, 1, 1, 0, 1, 1]);
}

#[test]
fn ignores_a_comment_line_before_the_header() {
    let ast = ast("  %% leading comment\n  treemap\n\"Root\"");
    assert_eq!(names(&ast), ["Root"]);
}

#[test]
fn ignores_leading_whitespace_before_the_header() {
    for body in ["  treemap", "\n\ttreemap", " \n  treemap\n\"Root\""] {
        let parsed = ast(body);
        assert!(parsed.nodes.len() <= 1, "{body:?}");
    }
}

#[test]
fn trailing_whitespace_after_the_header_is_an_indentation_row() {
    // `INDENTATION` matches before the dead `WS` terminal, so the spaces are a row prefix
    // without a node after them.
    assert!(parse("treemap  ").is_err());
    assert!(parse("treemap \n").is_err());
}

#[test]
fn a_later_title_replaces_an_earlier_one() {
    let ast = ast("treemap\ntitle first\ntitle second");
    assert_eq!(ast.diagram_title, "second");
}

#[test]
fn a_title_stops_at_a_comment_marker() {
    let ast = ast("treemap\ntitle Foo %% bar\n\"Root\"");
    assert_eq!(ast.diagram_title, "Foo");
    assert_eq!(names(&ast), ["Root"]);
}

#[test]
fn parses_a_multiline_accessible_description() {
    let ast = ast("treemap\naccDescr {\n  multi\n  line\n}\n\"Root\"");
    assert_eq!(ast.acc_description, "multi\nline");
}

#[test]
fn class_definitions_apply_to_nodes_declared_before_them() {
    let ast = ast(
        "treemap-beta\n\"Main\"\n    \"A\": 20\n    \"B\":::important\n        \"B1\": 10\n        \"B2\": 15\n    \"C\": 5\n\nclassDef important fill:#f96,stroke:#333,stroke-width:2px;",
    );
    let styles = node(&ast, "B")
        .css_compiled_styles
        .as_deref()
        .expect("the class styles are compiled");
    assert_eq!(styles, ["fill:#f96", "stroke:#333", "stroke-width:2px"]);
    assert_eq!(child_names(node(&ast, "B")), ["B1", "B2"]);
}

#[test]
fn class_styles_split_on_commas_and_semicolons() {
    let ast =
        ast("treemap\n\"Leaf\": 5:::class1\nclassDef class1 fill:red,color:blue,stroke:#FFD600;");
    assert_eq!(
        ast.class("class1").expect("the class is defined").styles,
        ["fill:red", "color:blue", "stroke:#FFD600"]
    );
    assert_eq!(
        node(&ast, "Leaf").css_compiled_styles.as_deref(),
        Some(
            [
                "fill:red".to_string(),
                "color:blue".to_string(),
                "stroke:#FFD600".to_string()
            ]
            .as_slice()
        )
    );
}

#[test]
fn class_styles_keep_an_escaped_comma() {
    let ast = ast("treemap\nclassDef class1 fill:rgb(1\\,2\\,3),color:red;");
    assert_eq!(
        ast.class("class1").expect("the class is defined").styles,
        ["fill:rgb(1,2,3)", "color:red"]
    );
}

#[test]
fn class_definitions_accumulate_for_the_same_class() {
    let ast = ast("treemap\nclassDef class1 fill:red;\nclassDef class1 color:blue;");
    assert_eq!(ast.classes.len(), 1);
    assert_eq!(
        ast.class("class1").expect("the class is defined").styles,
        ["fill:red", "color:blue"]
    );
}

#[test]
fn a_class_def_can_span_lines_after_its_keyword() {
    // `CLASS_DEF`'s `\s+` is any JavaScript whitespace, newlines included.
    let ast = ast("treemap\nclassDef\n  class1 fill:red;");
    assert_eq!(
        ast.class("class1").expect("the class is defined").styles,
        ["fill:red"]
    );
}

#[test]
fn a_label_style_keyword_is_recorded_as_a_text_style() {
    let ast = ast("treemap\nclassDef class1 color\nclassDef class2 font-size:12px");
    assert_eq!(ast.class("class1").expect("defined").styles, ["color"]);
    assert_eq!(ast.class("class1").expect("defined").text_styles, ["color"]);
    assert_eq!(
        ast.class("class2").expect("defined").styles,
        ["font-size:12px"]
    );
    assert!(ast.class("class2").expect("defined").text_styles.is_empty());
}

#[test]
fn an_unknown_class_selector_compiles_no_styles() {
    let ast = ast("treemap\n\"Leaf\": 1:::missing");
    let leaf = node(&ast, "Leaf");
    assert_eq!(leaf.class_selector.as_deref(), Some("missing"));
    assert_eq!(leaf.css_compiled_styles, None);
}

#[test]
fn an_empty_class_def_style_stays_one_empty_style() {
    let ast = ast("treemap\nclassDef ab;");
    assert_eq!(ast.class("ab").expect("the class is defined").styles, [""]);
}

#[test]
fn a_class_def_name_needs_two_characters() {
    assert!(parse("treemap\nclassDef a fill:red").is_err());
}

#[test]
fn leaf_values_follow_javascript_parse_float() {
    let ast = ast("treemap\n\"Commas\": 1,234\n\"Underscore\": 1_0\n\"Dots\": 1.2.3");
    assert_eq!(node(&ast, "Commas").value, Some(1234.0));
    assert_eq!(node(&ast, "Underscore").value, Some(1.0));
    assert_eq!(node(&ast, "Dots").value, Some(1.2));
}

#[test]
fn a_value_without_digits_is_nan() {
    let ast = ast("treemap\n\"Empty\": .");
    assert!(node(&ast, "Empty").value.expect("a value").is_nan());
}

#[test]
fn a_leaf_requires_a_value() {
    assert!(parse("treemap\n\"Leaf\":").is_err());
    assert!(parse("treemap\n\"Leaf\": :").is_err());
}

#[test]
fn a_name_without_a_separator_is_a_section() {
    let ast = ast("treemap\n\"Section\"\n  \"Leaf\": 1");
    assert_eq!(
        node(&ast, "Section").children.as_ref().map(Vec::len),
        Some(1)
    );
    assert!(parse("treemap\n\"Section\" 5").is_err());
}

#[test]
fn a_row_can_follow_the_header_on_the_same_line() {
    // Newlines are hidden, so the space is the row's indentation.
    let ast = ast("treemap \"Root\"");
    assert_eq!(names(&ast), ["Root"]);
    assert_eq!(ast.levels, [0]);
}

#[test]
fn parses_crlf_line_endings() {
    let ast = ast("treemap\r\n\"Root\"\r\n  \"Child\": 1\r\n");
    assert_eq!(names(&ast), ["Root", "Child"]);
    assert_eq!(ast.levels, [0, 1]);
}

#[test]
fn an_unterminated_string_reports_its_span() {
    let error = error("treemap\n\"Root");
    let span = error.span.expect("the error carries a span");
    assert_eq!((span.line, span.column), (2, 1));
    assert!(error.message.contains('"'));
}

#[test]
fn a_bad_header_reports_the_first_token() {
    let error = error("graph TD");
    let span = error.span.expect("the error carries a span");
    assert_eq!((span.line, span.column), (1, 1));
}

#[test]
fn an_empty_name_is_kept_empty() {
    let ast = ast("treemap\n\"\"");
    assert_eq!(names(&ast), [""]);
    assert_eq!(node(&ast, "").children, Some(Vec::new()));
}

#[test]
fn styles_for_class_returns_an_empty_slice_for_an_unknown_class() {
    let ast = ast("treemap\nclassDef class1 fill:red");
    assert_eq!(ast.styles_for_class("class1"), ["fill:red"]);
    assert!(ast.styles_for_class("missing").is_empty());
}
