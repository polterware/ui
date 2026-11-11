//! Port of `parser/flow-md-string.spec.js`.

use super::{ast, edge, text, vertex};

#[test]
fn markdown_formatting_in_nodes_and_labels() {
    let ast = ast(
        "flowchart\nA[\"`The cat in **the** hat`\"]-- \"`The *bat* in the chat`\" -->B[\"The dog in the hog\"] -- \"The rat in the mat\" -->C;",
    );
    assert_eq!(vertex(&ast, "A").id, "A");
    assert_eq!(text(&ast, "A"), "The cat in **the** hat");
    assert_eq!(
        vertex(&ast, "A").label_type,
        super::super::LabelType::Markdown
    );
    assert_eq!(vertex(&ast, "B").id, "B");
    assert_eq!(text(&ast, "B"), "The dog in the hog");
    assert_eq!(
        vertex(&ast, "B").label_type,
        super::super::LabelType::String
    );
    assert_eq!(ast.edges.len(), 2);
    assert_eq!(edge(&ast, 0).start, "A");
    assert_eq!(edge(&ast, 0).end, "B");
    assert_eq!(
        edge(&ast, 0).type_name.unwrap(),
        super::super::ArrowType::Point
    );
    assert_eq!(edge(&ast, 0).text, "The *bat* in the chat");
    assert_eq!(edge(&ast, 0).label_type, super::super::LabelType::Markdown);
    assert_eq!(edge(&ast, 1).start, "B");
    assert_eq!(edge(&ast, 1).end, "C");
    assert_eq!(
        edge(&ast, 1).type_name.unwrap(),
        super::super::ArrowType::Point
    );
    assert_eq!(edge(&ast, 1).text, "The rat in the mat");
    assert_eq!(edge(&ast, 1).label_type, super::super::LabelType::String);
}

#[test]
fn markdown_formatting_in_subgraphs() {
    let ast = ast(
        "flowchart LR\nsubgraph \"One\"\n  a(\"`The **cat**\n  in the hat`\") -- \"1o\" --> b{{\"`The **dog** in the hog`\"}}\nend\nsubgraph \"`**Two**`\"\n  c(\"`The **cat**\n  in the hat`\") -- \"`1o **ipa**`\" --> d(\"The dog in the hog\")\nend",
    );
    assert_eq!(ast.sub_graphs.len(), 2);
    let sub_graph = &ast.sub_graphs[0];
    assert_eq!(sub_graph.nodes.len(), 2);
    assert_eq!(sub_graph.title, "One");
    assert_eq!(sub_graph.label_type, super::super::LabelType::Text);
    let sub_graph2 = &ast.sub_graphs[1];
    assert_eq!(sub_graph2.nodes.len(), 2);
    assert_eq!(sub_graph2.title, "**Two**");
    assert_eq!(sub_graph2.label_type, super::super::LabelType::Markdown);
}
