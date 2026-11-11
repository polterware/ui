//! Port of `parser/flow-singlenode.spec.js`.

use super::{ast, parse_error, text, vertex};

#[test]
fn should_handle_a_single_node() {
    let ast = ast("graph TD;A;");
    assert!(ast.edges.is_empty());
    assert!(vertex(&ast, "A").styles.is_empty());
}

#[test]
fn should_handle_a_single_node_with_white_space_after_it() {
    let ast = ast("graph TD;A ;");
    assert!(ast.edges.is_empty());
    assert!(vertex(&ast, "A").styles.is_empty());
}

fn single_shape_case(body: &str, id: &str, type_name: &str) {
    let ast = ast(body);
    assert!(ast.edges.is_empty(), "{body}");
    let vertex = vertex(&ast, id);
    assert_eq!(vertex.type_name.as_deref(), Some(type_name), "{body}");
}

#[test]
fn should_handle_single_shapes() {
    single_shape_case("graph TD;a[A];", "a", "square");
    single_shape_case("graph TD;a((A));", "a", "circle");
    single_shape_case("graph TD;a(A);", "a", "round");
    single_shape_case("graph TD;a>A];", "a", "odd");
    single_shape_case("graph TD;a{A};", "a", "diamond");
    single_shape_case("graph TD;a{{A}};", "a", "hexagon");
    single_shape_case("graph TD;a(((A)));", "a", "doublecircle");
    single_shape_case("graph TD;a{A <br> end};", "a", "diamond");
    single_shape_case("graph TD;a{{A <br> end}};", "a", "hexagon");
    single_shape_case("graph TD;a(A <br> end);", "a", "round");
    single_shape_case("graph TD;a(((A <br> end)));", "a", "doublecircle");
}

#[test]
fn should_handle_shape_text() {
    assert_eq!(text(&ast("graph TD;a{A <br> end};"), "a"), "A <br> end");
    assert_eq!(text(&ast("graph TD;a{{A <br> end}};"), "a"), "A <br> end");
    assert_eq!(text(&ast("graph TD;a(A <br> end);"), "a"), "A <br> end");
    assert_eq!(text(&ast("graph TD;a(((A <br> end)));"), "a"), "A <br> end");
}

#[test]
fn should_handle_a_single_diamond_node_with_whitespace_after_it() {
    single_shape_case("graph TD;a{A}   ;", "a", "diamond");
}

#[test]
fn should_handle_a_single_double_circle_node_with_whitespace_after_it() {
    single_shape_case("graph TD;a(((A)))   ;", "a", "doublecircle");
}

#[test]
fn should_handle_single_node_ids() {
    assert!(vertex(&ast("graph TD;id1;"), "id1").styles.is_empty());
    assert_eq!(text(&ast("graph TD;1;"), "1"), "1");
    assert_eq!(text(&ast("graph TD;subgraph \"hello\";1;end;"), "1"), "1");
    assert!(vertex(&ast("graph TD;1id;"), "1id").styles.is_empty());
    assert!(vertex(&ast("graph TD;i-d;"), "i-d").styles.is_empty());
    assert!(vertex(&ast("graph TD;i_d;"), "i_d").styles.is_empty());
}

const KEYWORDS: [&str; 18] = [
    "graph",
    "flowchart",
    "flowchart-elk",
    "style",
    "default",
    "linkStyle",
    "interpolate",
    "classDef",
    "class",
    "href",
    "call",
    "click",
    "_self",
    "_blank",
    "_parent",
    "_top",
    "end",
    "subgraph",
];

#[test]
fn should_handle_keywords_between_dashes() {
    for keyword in KEYWORDS {
        let id = format!("a-{keyword}-node");
        let ast = ast(&format!("graph TD;a-{keyword}-node;"));
        assert_eq!(text(&ast, &id), id, "{keyword}");
    }
}

#[test]
fn should_handle_keywords_between_periods() {
    for keyword in KEYWORDS {
        let id = format!("a.{keyword}.node");
        let ast = ast(&format!("graph TD;a.{keyword}.node;"));
        assert_eq!(text(&ast, &id), id, "{keyword}");
    }
}

#[test]
fn should_handle_keywords_between_underscores() {
    for keyword in KEYWORDS {
        let id = format!("a_{keyword}_node");
        let ast = ast(&format!("graph TD;a_{keyword}_node;"));
        assert_eq!(text(&ast, &id), id, "{keyword}");
    }
}

#[test]
fn should_handle_nodes_ending_in_keywords() {
    for keyword in KEYWORDS {
        let ast = ast(&format!(
            "graph TD;node_{keyword};node.{keyword};node-{keyword};"
        ));
        for id in [
            format!("node_{keyword}"),
            format!("node.{keyword}"),
            format!("node-{keyword}"),
        ] {
            assert_eq!(text(&ast, &id), id, "{keyword}");
        }
    }
}

const ERROR_KEYWORDS: [&str; 14] = [
    "graph",
    "flowchart",
    "flowchart-elk",
    "style",
    "linkStyle",
    "interpolate",
    "classDef",
    "class",
    "_self",
    "_blank",
    "_parent",
    "_top",
    "end",
    "subgraph",
];

#[test]
fn should_throw_error_at_nodes_beginning_with_a_keyword() {
    for keyword in ERROR_KEYWORDS {
        let body = format!("graph TD;{keyword}.node;{keyword}-node;{keyword}/node");
        let error = parse_error(&body);
        assert!(!error.to_string().is_empty(), "{keyword}");
    }
}

#[test]
fn should_parse_nodes_beginning_with_a_working_keyword() {
    for keyword in ["default", "href", "click", "call"] {
        let ast = ast(&format!(
            "graph TD; {keyword}.node;{keyword}-node;{keyword}/node;"
        ));
        for id in [
            format!("{keyword}.node"),
            format!("{keyword}-node"),
            format!("{keyword}/node"),
        ] {
            assert_eq!(text(&ast, &id), id, "{keyword}");
        }
    }
}

const SPECIAL_CHARS: [&str; 12] = ["#", ":", "0", "&", ",", "*", ".", "\\", "v", "-", "/", "_"];

#[test]
fn should_allow_node_ids_of_single_special_characters() {
    for special in SPECIAL_CHARS {
        let ast = ast(&format!("graph TD; {special} --> A"));
        assert_eq!(text(&ast, special), special, "{special}");
    }
}

#[test]
fn should_allow_node_ids_with_special_characters_at_the_start() {
    for special in SPECIAL_CHARS {
        let id = format!("{special}node");
        let ast = ast(&format!("graph TD; {special}node --> A"));
        assert_eq!(text(&ast, &id), id, "{special}");
    }
}

#[test]
fn should_allow_node_ids_with_special_characters_at_the_end() {
    for special in SPECIAL_CHARS {
        let id = format!("node{special}");
        let ast = ast(&format!("graph TD; node{special} --> A"));
        assert_eq!(text(&ast, &id), id, "{special}");
    }
}
