//! Port of `parser/flow-vertice-chaining.spec.js`.

use super::{ast, edge, vertex};

#[test]
fn should_handle_chaining_of_vertices() {
    let ast = ast("\n    graph TD\n      A-->B-->C;\n    ");
    assert_eq!(vertex(&ast, "A").id, "A");
    assert_eq!(vertex(&ast, "B").id, "B");
    assert_eq!(vertex(&ast, "C").id, "C");
    assert_eq!(ast.edges.len(), 2);
    assert_eq!(edge(&ast, 0).start, "A");
    assert_eq!(edge(&ast, 0).end, "B");
    assert_eq!(
        edge(&ast, 0).type_name.unwrap(),
        super::super::ArrowType::Point
    );
    assert_eq!(edge(&ast, 0).text, "");
    assert_eq!(edge(&ast, 1).start, "B");
    assert_eq!(edge(&ast, 1).end, "C");
    assert_eq!(
        edge(&ast, 1).type_name.unwrap(),
        super::super::ArrowType::Point
    );
    assert_eq!(edge(&ast, 1).text, "");
}

#[test]
fn should_handle_chaining_with_and_on_the_left() {
    let ast = ast("\n    graph TD\n      A & B --> C;\n    ");
    assert_eq!(ast.edges.len(), 2);
    assert_eq!(edge(&ast, 0).start, "A");
    assert_eq!(edge(&ast, 0).end, "C");
    assert_eq!(edge(&ast, 1).start, "B");
    assert_eq!(edge(&ast, 1).end, "C");
    assert_eq!(
        edge(&ast, 0).type_name.unwrap(),
        super::super::ArrowType::Point
    );
    assert_eq!(edge(&ast, 1).text, "");
}

#[test]
fn should_handle_multiple_vertices_in_link_statement_in_the_beginning() {
    let ast = ast("\n    graph TD\n      A-->B & C;\n    ");
    assert_eq!(ast.edges.len(), 2);
    assert_eq!(edge(&ast, 0).start, "A");
    assert_eq!(edge(&ast, 0).end, "B");
    assert_eq!(edge(&ast, 1).start, "A");
    assert_eq!(edge(&ast, 1).end, "C");
}

#[test]
fn should_handle_multiple_vertices_in_link_statement_at_the_end() {
    let ast = ast("\n    graph TD\n      A & B--> C & D;\n    ");
    assert_eq!(ast.edges.len(), 4);
    let expected = [("A", "C"), ("A", "D"), ("B", "C"), ("B", "D")];
    for (index, (start, end)) in expected.into_iter().enumerate() {
        assert_eq!(edge(&ast, index).start, start);
        assert_eq!(edge(&ast, index).end, end);
        assert_eq!(
            edge(&ast, index).type_name.unwrap(),
            super::super::ArrowType::Point
        );
        assert_eq!(edge(&ast, index).text, "");
    }
}

#[test]
fn should_handle_chaining_and_multiple_nodes_in_link_statement() {
    let ast = ast("\n    graph TD\n      A --> B & B2 & C --> D2;\n    ");
    let expected = [
        ("A", "B"),
        ("A", "B2"),
        ("A", "C"),
        ("B", "D2"),
        ("B2", "D2"),
        ("C", "D2"),
    ];
    assert_eq!(ast.edges.len(), 6);
    for (index, (start, end)) in expected.into_iter().enumerate() {
        assert_eq!(edge(&ast, index).start, start);
        assert_eq!(edge(&ast, index).end, end);
        assert_eq!(
            edge(&ast, index).type_name.unwrap(),
            super::super::ArrowType::Point
        );
        assert_eq!(edge(&ast, index).text, "");
    }
}

#[test]
fn should_handle_chaining_with_extra_info_in_statements() {
    let ast = ast(
        "\n    graph TD\n      A[ h ] -- hello --> B[\" test \"]:::exClass & C --> D;\n      classDef exClass background:#bbb,border:1px solid red;\n    ",
    );
    let class = ast.class("exClass").expect("exClass");
    assert_eq!(
        class.styles,
        vec!["background:#bbb", "border:1px solid red"]
    );
    assert_eq!(vertex(&ast, "A").id, "A");
    assert_eq!(vertex(&ast, "B").id, "B");
    assert_eq!(vertex(&ast, "B").classes[0], "exClass");
    assert_eq!(vertex(&ast, "C").id, "C");
    assert_eq!(vertex(&ast, "D").id, "D");
    assert_eq!(ast.edges.len(), 4);
    assert_eq!(edge(&ast, 0).start, "A");
    assert_eq!(edge(&ast, 0).end, "B");
    assert_eq!(edge(&ast, 0).text, "hello");
    assert_eq!(edge(&ast, 1).start, "A");
    assert_eq!(edge(&ast, 1).end, "C");
    assert_eq!(edge(&ast, 1).text, "hello");
    assert_eq!(edge(&ast, 2).start, "B");
    assert_eq!(edge(&ast, 2).end, "D");
    assert_eq!(edge(&ast, 2).text, "");
    assert_eq!(edge(&ast, 3).start, "C");
    assert_eq!(edge(&ast, 3).end, "D");
    assert_eq!(edge(&ast, 3).text, "");
}
