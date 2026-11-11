//! Port of `parser/flow-arrows.spec.js`.

use super::{ast, edge, vertex};

#[test]
fn should_handle_a_nodes_and_edges() {
    let ast = ast("graph TD;\nA-->B;");
    assert_eq!(vertex(&ast, "A").id, "A");
    assert_eq!(vertex(&ast, "B").id, "B");
    assert_eq!(ast.edges.len(), 1);
    assert_eq!(edge(&ast, 0).start, "A");
    assert_eq!(edge(&ast, 0).end, "B");
    assert_eq!(
        edge(&ast, 0).type_name.unwrap(),
        super::super::ArrowType::Point
    );
    assert_eq!(edge(&ast, 0).text, "");
    assert_eq!(edge(&ast, 0).stroke.unwrap(), super::super::Stroke::Normal);
    assert_eq!(edge(&ast, 0).length, Some(1));
}

fn direction_case(body: &str, direction: &str) {
    let ast = ast(body);
    assert_eq!(ast.direction.as_deref(), Some(direction));
    assert_eq!(vertex(&ast, "A").id, "A");
    assert_eq!(vertex(&ast, "B").id, "B");
    assert_eq!(ast.edges.len(), 1);
    assert_eq!(edge(&ast, 0).start, "A");
    assert_eq!(edge(&ast, 0).end, "B");
    assert_eq!(
        edge(&ast, 0).type_name.unwrap(),
        super::super::ArrowType::Point
    );
    assert_eq!(edge(&ast, 0).text, "");
    assert_eq!(edge(&ast, 0).stroke.unwrap(), super::super::Stroke::Normal);
    assert_eq!(edge(&ast, 0).length, Some(1));
}

#[test]
fn should_handle_angle_bracket_gt_as_direction_lr() {
    direction_case("graph >;A-->B;", "LR");
}

#[test]
fn should_handle_angle_bracket_lt_as_direction_rl() {
    direction_case("graph <;A-->B;", "RL");
}

#[test]
fn should_handle_caret_as_direction_bt() {
    direction_case("graph ^;A-->B;", "BT");
}

#[test]
fn should_handle_lowercase_v_as_direction_tb() {
    direction_case("graph v;A-->B;", "TB");
}

#[test]
fn should_handle_a_nodes_and_edges_and_a_space_between_link_and_node() {
    let ast = ast("graph TD;A --> B;");
    assert_eq!(vertex(&ast, "A").id, "A");
    assert_eq!(vertex(&ast, "B").id, "B");
    assert_eq!(ast.edges.len(), 1);
    assert_eq!(edge(&ast, 0).start, "A");
    assert_eq!(edge(&ast, 0).end, "B");
    assert_eq!(
        edge(&ast, 0).type_name.unwrap(),
        super::super::ArrowType::Point
    );
    assert_eq!(edge(&ast, 0).text, "");
    assert_eq!(edge(&ast, 0).stroke.unwrap(), super::super::Stroke::Normal);
    assert_eq!(edge(&ast, 0).length, Some(1));
}

#[test]
fn should_handle_a_nodes_and_edges_with_a_style_line_after() {
    let ast = ast("graph TD\nA --> B\n style e red");
    assert_eq!(vertex(&ast, "A").id, "A");
    assert_eq!(vertex(&ast, "B").id, "B");
    assert_eq!(ast.edges.len(), 1);
    assert_eq!(edge(&ast, 0).start, "A");
    assert_eq!(edge(&ast, 0).end, "B");
    assert_eq!(
        edge(&ast, 0).type_name.unwrap(),
        super::super::ArrowType::Point
    );
    assert_eq!(edge(&ast, 0).text, "");
    assert_eq!(edge(&ast, 0).stroke.unwrap(), super::super::Stroke::Normal);
    assert_eq!(edge(&ast, 0).length, Some(1));
}

#[test]
fn should_handle_statements_ending_without_semicolon() {
    let ast = ast("graph TD\nA-->B\nB-->C");
    assert_eq!(vertex(&ast, "A").id, "A");
    assert_eq!(vertex(&ast, "B").id, "B");
    assert_eq!(ast.edges.len(), 2);
    assert_eq!(edge(&ast, 1).start, "B");
    assert_eq!(edge(&ast, 1).end, "C");
    assert_eq!(
        edge(&ast, 0).type_name.unwrap(),
        super::super::ArrowType::Point
    );
    assert_eq!(edge(&ast, 0).text, "");
    assert_eq!(edge(&ast, 0).stroke.unwrap(), super::super::Stroke::Normal);
    assert_eq!(edge(&ast, 0).length, Some(1));
}

fn double_edge_case(
    body: &str,
    arrow: super::super::ArrowType,
    stroke: super::super::Stroke,
    text: &str,
) {
    let ast = ast(body);
    assert_eq!(vertex(&ast, "A").id, "A");
    assert_eq!(vertex(&ast, "B").id, "B");
    assert_eq!(ast.edges.len(), 1);
    assert_eq!(edge(&ast, 0).start, "A");
    assert_eq!(edge(&ast, 0).end, "B");
    assert_eq!(edge(&ast, 0).type_name.unwrap(), arrow);
    assert_eq!(edge(&ast, 0).text, text);
    assert_eq!(edge(&ast, 0).stroke.unwrap(), stroke);
    assert_eq!(edge(&ast, 0).length, Some(1));
}

#[test]
fn should_handle_double_edged_nodes_and_edges() {
    double_edge_case(
        "graph TD;\nA<-->B;",
        super::super::ArrowType::DoublePoint,
        super::super::Stroke::Normal,
        "",
    );
}

#[test]
fn should_handle_double_edged_nodes_with_text() {
    double_edge_case(
        "graph TD;\nA<-- text -->B;",
        super::super::ArrowType::DoublePoint,
        super::super::Stroke::Normal,
        "text",
    );
}

#[test]
fn should_handle_double_edged_nodes_and_edges_on_thick_arrows() {
    double_edge_case(
        "graph TD;\nA<==>B;",
        super::super::ArrowType::DoublePoint,
        super::super::Stroke::Thick,
        "",
    );
}

#[test]
fn should_handle_double_edged_nodes_with_text_on_thick_arrows() {
    double_edge_case(
        "graph TD;\nA<== text ==>B;",
        super::super::ArrowType::DoublePoint,
        super::super::Stroke::Thick,
        "text",
    );
}

#[test]
fn should_handle_double_edged_nodes_and_edges_on_dotted_arrows() {
    double_edge_case(
        "graph TD;\nA<-.->B;",
        super::super::ArrowType::DoublePoint,
        super::super::Stroke::Dotted,
        "",
    );
}

#[test]
fn should_handle_double_edged_nodes_with_text_on_dotted_arrows() {
    double_edge_case(
        "graph TD;\nA<-. text .->B;",
        super::super::ArrowType::DoublePoint,
        super::super::Stroke::Dotted,
        "text",
    );
}
