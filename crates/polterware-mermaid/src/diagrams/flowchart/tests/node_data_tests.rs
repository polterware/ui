//! Port of `parser/flow-node-data.spec.js`.
//!
//! The spec's `getData()` assertions map to [`super::super::Ast::nodes`] and `layout_edges`.
//! The `stepDuration`/`stepNumbers` cases that change configuration are skipped (they are config
//! behavior); the default duration and the default absence of a number badge are kept.

use super::{ast, parse_error};

fn node<'a>(ast: &'a super::super::Ast, id: &str) -> &'a super::super::Node {
    ast.node(id)
        .unwrap_or_else(|| panic!("no node {id:?} in {:?}", ast.nodes))
}

#[test]
fn should_handle_basic_shape_data_statements() {
    for body in [
        "flowchart TB\n      D@{ shape: rounded}",
        "flowchart TB\n      D@{ shape: rounded }",
    ] {
        let ast = ast(body);
        assert_eq!(ast.nodes.len(), 1, "{body}");
        assert_eq!(ast.nodes[0].shape, "rounded", "{body}");
        assert_eq!(ast.nodes[0].label, "D", "{body}");
    }
}

#[test]
fn should_handle_basic_shape_data_statements_with_amp() {
    let ast = ast("flowchart TB\n      D@{ shape: rounded } & E");
    assert_eq!(ast.nodes.len(), 2);
    assert_eq!(ast.nodes[0].shape, "rounded");
    assert_eq!(ast.nodes[0].label, "D");
    assert_eq!(ast.nodes[1].label, "E");
}

#[test]
fn should_handle_shape_data_statements_with_edges() {
    let ast = ast("flowchart TB\n      D@{ shape: rounded } --> E");
    assert_eq!(ast.nodes.len(), 2);
    assert_eq!(ast.nodes[0].shape, "rounded");
    assert_eq!(ast.nodes[1].label, "E");
}

#[test]
fn should_handle_shape_data_with_amp_and_edges() {
    let cases = [
        ("flowchart TB\n      D@{ shape: rounded } & E --> F", 3),
        (
            "flowchart TB\n      D@{ shape: rounded } & E@{ shape: rounded } --> F",
            3,
        ),
        (
            "flowchart TB\n      D@{ shape: rounded } & E@{ shape: rounded } --> F & G@{ shape: rounded }",
            4,
        ),
        (
            "flowchart TB\n      D@{ shape: rounded } & E@{ shape: rounded } --> F@{ shape: rounded } & G@{ shape: rounded }",
            4,
        ),
        (
            "flowchart TB\n         D@{ shape: rounded } & E@{ shape: rounded } --> F{ shape: rounded } & G{ shape: rounded }    ",
            4,
        ),
    ];
    for (body, count) in cases {
        let ast = ast(body);
        assert_eq!(ast.nodes.len(), count, "{body}");
        assert_eq!(ast.nodes[0].shape, "rounded", "{body}");
        assert_eq!(ast.nodes[0].label, "D", "{body}");
        assert_eq!(ast.nodes[1].label, "E", "{body}");
    }
}

#[test]
fn should_not_care_about_leading_spaces_in_metadata() {
    for body in [
        "flowchart TB\n      D@{shape: rounded}",
        "flowchart TB\n      D@{       shape: rounded}",
        "flowchart TB\n      D@{ shape: rounded         }",
    ] {
        let ast = ast(body);
        assert_eq!(ast.nodes.len(), 1, "{body}");
        assert_eq!(ast.nodes[0].shape, "rounded", "{body}");
        assert_eq!(ast.nodes[0].label, "D", "{body}");
    }
}

#[test]
fn should_handle_multiple_properties_on_the_same_line() {
    let ast = ast("flowchart TB\n      D@{ shape: rounded , label: \"DD\"}");
    assert_eq!(ast.nodes.len(), 1);
    assert_eq!(ast.nodes[0].shape, "rounded");
    assert_eq!(ast.nodes[0].label, "DD");
}

#[test]
fn should_be_possible_to_link_to_a_node_with_more_data() {
    let ast = ast(
        "flowchart TB\n      A --> D@{\n        shape: circle\n        other: \"clock\"\n     }\n\n      ",
    );
    assert_eq!(ast.nodes.len(), 2);
    assert_eq!(ast.nodes[0].shape, "squareRect");
    assert_eq!(ast.nodes[0].label, "A");
    assert_eq!(ast.nodes[1].label, "D");
    assert_eq!(ast.nodes[1].shape, "circle");
    assert_eq!(ast.layout_edges.len(), 1);
}

#[test]
fn should_not_disturb_adding_multiple_nodes_after_each_other() {
    let ast = ast(
        "flowchart TB\n      A[hello]\n      B@{\n        shape: circle\n        other: \"clock\"\n     }\n      C[Hello]@{\n        shape: circle\n        other: \"clock\"\n     }\n      ",
    );
    assert_eq!(ast.nodes.len(), 3);
    assert_eq!(ast.nodes[0].shape, "squareRect");
    assert_eq!(ast.nodes[0].label, "hello");
    assert_eq!(ast.nodes[1].shape, "circle");
    assert_eq!(ast.nodes[1].label, "B");
    assert_eq!(ast.nodes[2].shape, "circle");
    assert_eq!(ast.nodes[2].label, "Hello");
}

#[test]
fn should_use_a_closing_brace_inside_shape_data() {
    let ast = ast(
        "flowchart TB\n      A@{\n        label: \"This is }\"\n        other: \"clock\"\n     }\n      ",
    );
    assert_eq!(ast.nodes.len(), 1);
    assert_eq!(ast.nodes[0].shape, "squareRect");
    assert_eq!(ast.nodes[0].label, "This is }");
}

#[test]
fn should_error_on_a_nonexistent_shape() {
    let error = parse_error("flowchart TB\n      A@{ shape: this-shape-does-not-exist }\n      ");
    assert!(
        error
            .to_string()
            .contains("No such shape: this-shape-does-not-exist.")
    );
}

#[test]
fn should_error_on_an_internal_only_shape() {
    let error = parse_error("flowchart TB\n      A@{ shape: rect_left_inv_arrow }\n      ");
    assert!(
        error
            .to_string()
            .contains("No such shape: rect_left_inv_arrow. Shape names should be lowercase.")
    );
}

#[test]
fn diamond_shapes_still_work() {
    let ast = ast("flowchart TB\n      A{This is a label}\n");
    assert_eq!(ast.nodes.len(), 1);
    assert_eq!(ast.nodes[0].shape, "diamond");
    assert_eq!(ast.nodes[0].label, "This is a label");
}

#[test]
fn should_support_multiline_block_strings() {
    let ast = ast(
        "flowchart TB\n      A@{\n        label: |\n          This is a\n          multiline string\n        other: \"clock\"\n     }\n      ",
    );
    assert_eq!(ast.nodes.len(), 1);
    assert_eq!(ast.nodes[0].shape, "squareRect");
    assert_eq!(ast.nodes[0].label, "This is a\nmultiline string\n");
}

#[test]
fn should_support_multiline_quoted_strings() {
    let ast = ast(
        "flowchart TB\n      A@{\n        label: \"This is a\n          multiline string\"\n        other: \"clock\"\n     }\n      ",
    );
    assert_eq!(ast.nodes.len(), 1);
    assert_eq!(ast.nodes[0].shape, "squareRect");
    assert_eq!(ast.nodes[0].label, "This is a<br/>multiline string");
}

#[test]
fn should_allow_braces_and_at_in_metadata_strings() {
    let cases = [
        ("This is a string with }", "This is a string with }"),
        ("This is a string with @", "This is a string with @"),
        ("This is a string with}", "This is a string with}"),
    ];
    for (label, expected) in cases {
        let body = format!(
            "flowchart TB\n      A@{{\n        label: \"{label}\"\n        other: \"clock\"\n     }}\n      "
        );
        let ast = ast(&body);
        assert_eq!(ast.nodes.len(), 1);
        assert_eq!(ast.nodes[0].shape, "squareRect");
        assert_eq!(ast.nodes[0].label, expected);
    }
}

#[test]
fn should_use_at_syntax_for_labels_on_multiple_nodes() {
    let ast = ast(
        "flowchart TB\n       n2[\"label for n2\"] &   n4@{ label: \"label for n4\"}   & n5@{ label: \"label for n5\"}\n      ",
    );
    assert_eq!(ast.nodes.len(), 3);
    assert_eq!(ast.nodes[0].label, "label for n2");
    assert_eq!(ast.nodes[1].label, "label for n4");
    assert_eq!(ast.nodes[2].label, "label for n5");
}

#[test]
fn should_use_at_syntax_for_labels_with_edges() {
    let ast = ast(
        "flowchart TD\n    A[\"A\"] --> B[\"for B\"] &    C@{ label: \"for c\"} & E@{label : \"for E\"}\n    D@{label: \"for D\"}\n      ",
    );
    assert_eq!(ast.nodes.len(), 5);
    assert_eq!(ast.nodes[0].label, "A");
    assert_eq!(ast.nodes[1].label, "for B");
    assert_eq!(ast.nodes[2].label, "for c");
    assert_eq!(ast.nodes[3].label, "for E");
    assert_eq!(ast.nodes[4].label, "for D");
}

#[test]
fn should_use_at_syntax_in_labels() {
    let ast = ast(
        "flowchart TD\n    A[\"@A@\"] --> B[\"@for@ B@\"] & C@{ label: \"@for@ c@\"} & E{\"`@for@ E@`\"} & D((\"@for@ D@\"))\n      H1{{\"@for@ H@\"}}\n      H2{{\"`@for@ H@`\"}}\n      Q1{\"@for@ Q@\"}\n      Q2{\"`@for@ Q@`\"}\n      AS1>\"@for@ AS@\"]\n      AS2>\"`@for@ AS@`\"]\n      ",
    );
    assert_eq!(ast.nodes.len(), 11);
    let labels = [
        "@A@",
        "@for@ B@",
        "@for@ c@",
        "@for@ E@",
        "@for@ D@",
        "@for@ H@",
        "@for@ H@",
        "@for@ Q@",
        "@for@ Q@",
        "@for@ AS@",
        "@for@ AS@",
    ];
    for (index, label) in labels.into_iter().enumerate() {
        assert_eq!(ast.nodes[index].label, label, "node {index}");
    }
}

#[test]
fn should_create_unique_edge_ids_with_at_and_amp() {
    let ast = ast("flowchart TD\n     A & B e1@--> C & D\n        A1 e2@--> C1 & D1\n      ");
    assert_eq!(ast.nodes.len(), 7);
    assert_eq!(ast.layout_edges.len(), 6);
    let ids: Vec<&str> = ast
        .layout_edges
        .iter()
        .map(|edge| edge.id.as_str())
        .collect();
    assert_eq!(
        ids,
        vec!["L_A_C_0", "L_A_D_0", "e1", "L_B_D_0", "e2", "L_A1_D1_0"]
    );
}

#[test]
fn should_redefine_the_same_edge_id() {
    let ast = ast("flowchart TD\n     A & B e1@--> C & D\n        A1 e1@--> C1 & D1\n      ");
    assert_eq!(ast.layout_edges.len(), 6);
    let ids: Vec<&str> = ast
        .layout_edges
        .iter()
        .map(|edge| edge.id.as_str())
        .collect();
    assert_eq!(
        ids,
        vec![
            "L_A_C_0",
            "L_A_D_0",
            "e1",
            "L_B_D_0",
            "L_A1_C1_0",
            "L_A1_D1_0"
        ]
    );
}

#[test]
fn should_override_edge_animate() {
    let ast = ast(
        "flowchart TD\n     A e1@--> B\n     C e2@--> D\n     E e3@--> F\n    e1@{ animate: true }\n    e2@{ animate: false }\n    e3@{ animate: true }\n    e3@{ animate: false }\n      ",
    );
    assert_eq!(ast.nodes.len(), 6);
    assert_eq!(ast.layout_edges.len(), 3);
    assert_eq!(ast.layout_edges[0].id, "e1");
    assert_eq!(ast.layout_edges[0].animate, Some(true));
    assert_eq!(ast.layout_edges[1].id, "e2");
    assert_eq!(ast.layout_edges[1].animate, Some(false));
    assert_eq!(ast.layout_edges[2].id, "e3");
    assert_eq!(ast.layout_edges[2].animate, Some(false));
}

#[test]
fn should_order_edges_for_the_step_animation() {
    let ast = ast(
        "flowchart LR\n     A e1@--> B\n     B e2@--> C\n     B e3@--> D\n     C e4@--> E\n    e1@{ step: 1 }\n    e2@{ step: 2 }\n    e3@{ step: 2 }\n      ",
    );
    let steps: Vec<(&str, Option<u32>, Option<u32>)> = ast
        .layout_edges
        .iter()
        .map(|edge| (edge.id.as_str(), edge.step, edge.step_count))
        .collect();
    assert_eq!(
        steps,
        vec![
            ("e1", Some(1), Some(2)),
            ("e2", Some(2), Some(2)),
            ("e3", Some(2), Some(2)),
            ("e4", None, None)
        ]
    );
}

#[test]
fn should_give_step_edges_the_default_step_duration() {
    let ast = ast("flowchart LR\n     A e1@--> B\n     B e2@--> C\n    e1@{ step: 1 }\n      ");
    assert_eq!(ast.layout_edges[0].step_duration, Some(1.0));
    assert_eq!(ast.layout_edges[1].step_duration, None);
}

#[test]
fn should_leave_step_edges_without_a_number_badge_by_default() {
    let ast = ast("flowchart LR\n     A e1@--> B\n    e1@{ step: 1 }\n      ");
    assert_eq!(ast.layout_edges[0].show_step_number, None);
}

#[test]
fn should_ignore_a_step_that_is_not_a_positive_integer() {
    let ast = ast(
        "flowchart LR\n     A e1@--> B\n     B e2@--> C\n    e1@{ step: 0 }\n    e2@{ step: 1.5 }\n      ",
    );
    assert!(
        ast.layout_edges
            .iter()
            .all(|edge| edge.step.is_none() && edge.step_count.is_none())
    );
}

#[test]
fn should_leave_edges_without_a_step_free_of_step_data() {
    let ast = ast("flowchart LR\n     A e1@--> B\n    e1@{ animate: true }\n      ");
    assert!(ast.layout_edges[0].step.is_none());
    assert!(ast.layout_edges[0].step_count.is_none());
}

#[test]
fn node_lookup_helper_is_usable() {
    let ast = ast("flowchart TB\nD");
    assert_eq!(node(&ast, "D").label, "D");
}
