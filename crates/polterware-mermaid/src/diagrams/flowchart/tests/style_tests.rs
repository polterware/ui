//! Port of `parser/flow-style.spec.js`.

use super::{ast, edge, parse_error, text, vertex};

#[test]
fn should_handle_styles_for_vertices() {
    let ast = ast("graph TD;style Q background:#fff;");
    assert_eq!(vertex(&ast, "Q").styles.len(), 1);
    assert_eq!(vertex(&ast, "Q").styles[0], "background:#fff");
}

#[test]
fn should_handle_multiple_styles_for_a_vertex() {
    let ast = ast("graph TD;style R background:#fff,border:1px solid red;");
    assert_eq!(vertex(&ast, "R").styles.len(), 2);
    assert_eq!(vertex(&ast, "R").styles[0], "background:#fff");
    assert_eq!(vertex(&ast, "R").styles[1], "border:1px solid red");
}

#[test]
fn should_handle_multiple_styles_in_a_graph() {
    let ast =
        ast("graph TD;style S background:#aaa;\nstyle T background:#bbb,border:1px solid red;");
    assert_eq!(vertex(&ast, "S").styles.len(), 1);
    assert_eq!(vertex(&ast, "T").styles.len(), 2);
    assert_eq!(vertex(&ast, "S").styles[0], "background:#aaa");
    assert_eq!(vertex(&ast, "T").styles[0], "background:#bbb");
    assert_eq!(vertex(&ast, "T").styles[1], "border:1px solid red");
}

#[test]
fn should_handle_styles_and_graph_definitions_in_a_graph() {
    let ast = ast(
        "graph TD;S-->T;\nstyle S background:#aaa;\nstyle T background:#bbb,border:1px solid red;",
    );
    assert_eq!(vertex(&ast, "S").styles.len(), 1);
    assert_eq!(vertex(&ast, "T").styles.len(), 2);
    assert_eq!(vertex(&ast, "S").styles[0], "background:#aaa");
    assert_eq!(vertex(&ast, "T").styles[0], "background:#bbb");
    assert_eq!(vertex(&ast, "T").styles[1], "border:1px solid red");
}

#[test]
fn should_keep_node_label_text_when_a_style_is_applied() {
    let ast =
        ast("graph TD;A(( ));B((Test));C;style A background:#fff;style D border:1px solid red;");
    assert_eq!(text(&ast, "A"), "");
    assert_eq!(text(&ast, "B"), "Test");
    assert_eq!(text(&ast, "C"), "C");
    assert_eq!(text(&ast, "D"), "D");
}

#[test]
fn should_be_possible_to_declare_a_class() {
    let ast = ast("graph TD;classDef exClass background:#bbb,border:1px solid red;");
    let class = ast.class("exClass").expect("exClass");
    assert_eq!(class.styles.len(), 2);
    assert_eq!(class.styles[0], "background:#bbb");
    assert_eq!(class.styles[1], "border:1px solid red");
}

#[test]
fn should_be_possible_to_declare_multiple_classes() {
    let ast = ast("graph TD;classDef firstClass,secondClass background:#bbb,border:1px solid red;");
    for id in ["firstClass", "secondClass"] {
        let class = ast.class(id).unwrap_or_else(|| panic!("{id}"));
        assert_eq!(class.styles.len(), 2);
        assert_eq!(class.styles[0], "background:#bbb");
        assert_eq!(class.styles[1], "border:1px solid red");
    }
}

#[test]
fn should_be_possible_to_declare_a_class_with_a_dot_in_the_style() {
    let ast = ast("graph TD;classDef exClass background:#bbb,border:1.5px solid red;");
    let class = ast.class("exClass").expect("exClass");
    assert_eq!(class.styles[0], "background:#bbb");
    assert_eq!(class.styles[1], "border:1.5px solid red");
}

#[test]
fn should_be_possible_to_declare_a_class_with_a_space_in_the_style() {
    let ast = ast("graph TD;classDef exClass background:  #bbb,border:1.5px solid red;");
    let class = ast.class("exClass").expect("exClass");
    assert_eq!(class.styles[0], "background:  #bbb");
    assert_eq!(class.styles[1], "border:1.5px solid red");
}

#[test]
fn should_be_possible_to_apply_a_class_to_a_vertex() {
    let ast = ast(
        "graph TD;\nclassDef exClass background:#bbb,border:1px solid red;\na-->b;\nclass a exClass;",
    );
    let class = ast.class("exClass").expect("exClass");
    assert_eq!(class.styles.len(), 2);
    assert_eq!(vertex(&ast, "a").classes, vec!["exClass"]);
}

#[test]
fn should_be_possible_to_apply_a_class_to_a_vertex_with_an_underscore() {
    let ast = ast(
        "graph TD;\nclassDef exClass background:#bbb,border:1px solid red;\na_a-->b_b;\nclass a_a exClass;",
    );
    assert_eq!(ast.class("exClass").expect("exClass").styles.len(), 2);
    assert_eq!(vertex(&ast, "a_a").classes, vec!["exClass"]);
}

#[test]
fn should_be_possible_to_apply_a_class_to_a_vertex_directly() {
    let ast = ast(
        "graph TD;\nclassDef exClass background:#bbb,border:1px solid red;\na-->b[test]:::exClass;\n",
    );
    assert_eq!(ast.class("exClass").expect("exClass").styles.len(), 2);
    assert_eq!(vertex(&ast, "b").classes[0], "exClass");
}

#[test]
fn should_be_possible_to_apply_a_class_directly_to_a_vertex() {
    let ast = ast(
        "graph TD;\nclassDef exClass background:#bbb,border:1px solid red;\nb[test]:::exClass;\n",
    );
    assert_eq!(vertex(&ast, "b").classes[0], "exClass");
}

#[test]
fn should_be_possible_to_apply_a_class_directly_between_edges() {
    let ast = ast(
        "graph TD;\nclassDef exClass background:#bbb,border:1px solid red;\nA[test]:::exClass-->B[test2];\n",
    );
    assert_eq!(vertex(&ast, "A").classes[0], "exClass");
}

#[test]
fn should_be_possible_to_apply_a_class_directly_to_a_vertex_with_text() {
    let ast = ast(
        "graph TD;\nclassDef exClass background:#bbb,border:1px solid red;\na-->b[1 a a text!.]:::exClass;\n",
    );
    assert_eq!(vertex(&ast, "b").classes[0], "exClass");
}

#[test]
fn should_be_possible_to_apply_a_class_to_a_comma_separated_list_of_vertices() {
    let ast = ast(
        "graph TD;\nclassDef exClass background:#bbb,border:1px solid red;\na-->b;\nclass a,b exClass;",
    );
    assert_eq!(vertex(&ast, "a").classes[0], "exClass");
    assert_eq!(vertex(&ast, "b").classes[0], "exClass");
}

#[test]
fn should_handle_style_definitions_with_more_than_one_digit() {
    let body = "graph TD\nA-->B1\nA-->B2\nA-->B3\nA-->B4\nA-->B5\nA-->B6\nA-->B7\nA-->B8\nA-->B9\nA-->B10\nA-->B11\nlinkStyle 10 stroke-width:1px;";
    let ast = ast(body);
    assert_eq!(
        edge(&ast, 0).type_name.unwrap(),
        super::super::ArrowType::Point
    );
}

#[test]
fn should_reject_a_link_style_index_out_of_bounds() {
    let error = parse_error("graph TD\n    A-->B\n    linkStyle 1 stroke-width:1px;");
    assert!(error.to_string().contains("out of bounds"), "{error}");
}

#[test]
fn should_handle_style_definitions_within_number_of_edges() {
    let ast = ast("graph TD\n    A-->B\n    linkStyle 0 stroke-width:1px;");
    assert_eq!(
        edge(&ast, 0).style.as_ref().expect("style")[0],
        "stroke-width:1px"
    );
}

#[test]
fn should_handle_multi_numbered_style_definitions() {
    let body = "graph TD\nA-->B1\nA-->B2\nA-->B3\nA-->B4\nA-->B5\nA-->B6\nA-->B7\nA-->B8\nA-->B9\nA-->B10\nA-->B11\nA-->B12\nlinkStyle 10,11 stroke-width:1px;";
    let ast = ast(body);
    assert_eq!(
        edge(&ast, 0).type_name.unwrap(),
        super::super::ArrowType::Point
    );
}

#[test]
fn should_handle_class_defs_with_style_in_classes() {
    let ast = ast("graph TD\nA-->B\nclassDef exClass font-style:bold;");
    assert_eq!(
        edge(&ast, 0).type_name.unwrap(),
        super::super::ArrowType::Point
    );
}

#[test]
fn should_handle_class_defs_with_percent_in_classes() {
    let ast = ast(
        "graph TD\nA-->B\nclassDef exClass fill:#f96,stroke:#333,stroke-width:4px,font-size:50%,font-style:bold;",
    );
    assert_eq!(
        edge(&ast, 0).type_name.unwrap(),
        super::super::ArrowType::Point
    );
}

#[test]
fn should_create_a_node_when_a_style_targets_none() {
    // Upstream logs a warning and still creates the node for backward compatibility.
    let ast = ast("graph TD;\nAA-->BB;\nstyle A fill:#f00;");
    assert_eq!(vertex(&ast, "A").styles[0], "fill:#f00");
    assert!(ast.vertex("AA").is_some());
    assert!(ast.vertex("BB").is_some());
}

#[test]
fn should_handle_multiple_vertices_with_style() {
    let ast = ast(
        "\n    graph TD\n      classDef C1 stroke-dasharray:4\n      classDef C2 stroke-dasharray:6\n      A & B:::C1 & D:::C1 --> E:::C2\n    ",
    );
    assert!(vertex(&ast, "A").classes.is_empty());
    assert_eq!(vertex(&ast, "B").classes[0], "C1");
    assert_eq!(vertex(&ast, "D").classes[0], "C1");
    assert_eq!(vertex(&ast, "E").classes[0], "C2");
}
