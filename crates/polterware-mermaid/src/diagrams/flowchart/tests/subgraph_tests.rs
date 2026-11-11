//! Port of `parser/subgraph.spec.js`.

use super::{ast, edge};

#[test]
fn should_handle_subgraph_with_tab_indentation() {
    let ast = ast("graph TB\nsubgraph One\n\ta1-->a2\nend");
    assert_eq!(ast.sub_graphs.len(), 1);
    let sub_graph = &ast.sub_graphs[0];
    assert_eq!(sub_graph.nodes, vec!["a2", "a1"]);
    assert_eq!(sub_graph.title, "One");
    assert_eq!(sub_graph.id, "One");
}

#[test]
fn should_handle_subgraph_with_chaining_nodes() {
    let ast = ast("graph TB\nsubgraph One\n\ta1-->a2-->a3\nend");
    let sub_graph = &ast.sub_graphs[0];
    assert_eq!(sub_graph.nodes, vec!["a3", "a2", "a1"]);
    assert_eq!(sub_graph.title, "One");
    assert_eq!(sub_graph.id, "One");
}

#[test]
fn should_handle_subgraph_with_multiple_words_in_title() {
    let ast = ast("graph TB\nsubgraph \"Some Title\"\n\ta1-->a2\nend");
    let sub_graph = &ast.sub_graphs[0];
    assert_eq!(sub_graph.nodes, vec!["a2", "a1"]);
    assert_eq!(sub_graph.title, "Some Title");
    assert_eq!(sub_graph.id, "subGraph0");
}

#[test]
fn should_handle_subgraph_with_id_and_title_notation() {
    let ast = ast("graph TB\nsubgraph some-id[Some Title]\n\ta1-->a2\nend");
    let sub_graph = &ast.sub_graphs[0];
    assert_eq!(sub_graph.nodes, vec!["a2", "a1"]);
    assert_eq!(sub_graph.title, "Some Title");
    assert_eq!(sub_graph.id, "some-id");
}

#[test]
fn should_handle_subgraph_id_starting_with_a_number() {
    let ast = ast(
        "graph TD\n    A[Christmas] -->|Get money| B(Go shopping)\n    subgraph 1test\n    A\n    end",
    );
    assert_eq!(ast.sub_graphs.len(), 1);
    let sub_graph = &ast.sub_graphs[0];
    assert_eq!(sub_graph.nodes, vec!["A"]);
    assert_eq!(sub_graph.id, "1test");
}

#[test]
fn should_handle_subgraphs_with_a_title() {
    let ast = ast("graph TD;A-->B;subgraph myTitle;c-->d;end;");
    assert_eq!(
        edge(&ast, 0).type_name.unwrap(),
        super::super::ArrowType::Point
    );
    assert_eq!(ast.sub_graphs.len(), 1);
    assert_eq!(ast.sub_graphs[0].title, "myTitle");
    assert_eq!(ast.sub_graphs[0].id, "myTitle");
}

#[test]
fn should_handle_subgraphs_with_title_in_quotes() {
    let ast = ast("graph TD;A-->B;subgraph \"title in quotes\";c-->d;end;");
    let sub_graph = &ast.sub_graphs[0];
    assert_eq!(sub_graph.title, "title in quotes");
    assert_eq!(
        edge(&ast, 0).type_name.unwrap(),
        super::super::ArrowType::Point
    );
}

#[test]
fn should_handle_old_style_subgraphs() {
    let ast = ast("graph TD;A-->B;subgraph old style that is broken;c-->d;end;");
    let sub_graph = &ast.sub_graphs[0];
    assert_eq!(sub_graph.title, "old style that is broken");
    assert_eq!(
        edge(&ast, 0).type_name.unwrap(),
        super::super::ArrowType::Point
    );
}

#[test]
fn should_handle_subgraphs_with_dashes_in_the_title() {
    let ast = ast("graph TD;A-->B;subgraph a-b-c;c-->d;end;");
    assert_eq!(ast.sub_graphs[0].title, "a-b-c");
    assert_eq!(
        edge(&ast, 0).type_name.unwrap(),
        super::super::ArrowType::Point
    );
}

#[test]
fn should_handle_subgraphs_with_id_and_title_in_brackets() {
    let ast = ast("graph TD;A-->B;subgraph uid1[text of doom];c-->d;end;");
    let sub_graph = &ast.sub_graphs[0];
    assert_eq!(sub_graph.title, "text of doom");
    assert_eq!(sub_graph.id, "uid1");
    assert_eq!(
        edge(&ast, 0).type_name.unwrap(),
        super::super::ArrowType::Point
    );
}

#[test]
fn should_handle_subgraphs_with_id_and_title_in_brackets_and_quotes() {
    let ast = ast("graph TD;A-->B;subgraph uid2[\"text of doom\"];c-->d;end;");
    let sub_graph = &ast.sub_graphs[0];
    assert_eq!(sub_graph.title, "text of doom");
    assert_eq!(sub_graph.id, "uid2");
}

#[test]
fn should_handle_subgraphs_with_id_and_title_without_spaces() {
    let ast = ast("graph TD;A-->B;subgraph uid2[textofdoom];c-->d;end;");
    let sub_graph = &ast.sub_graphs[0];
    assert_eq!(sub_graph.title, "textofdoom");
    assert_eq!(sub_graph.id, "uid2");
}

#[test]
fn should_handle_subgraphs_with_blank_lines() {
    for body in [
        "graph TD\nA-->B\nsubgraph myTitle\n\n c-->d \nend\n",
        "graph TD\nA-->B\nsubgraph myTitle   \n\n    c-->d \nend\n",
        "graph TD\nA-->B\nsubgraph myTitle\nc-->d\nend;",
        "graph TD\nA-->B\nsubgraph myTitle\nc-- text -->d\nd-->e\n end;",
        "graph TD\nA-->B\nsubgraph myTitle\na & b --> c & e\n end;",
    ] {
        let ast = ast(body);
        assert_eq!(
            edge(&ast, 0).type_name.unwrap(),
            super::super::ArrowType::Point,
            "{body}"
        );
    }
}

#[test]
fn should_handle_nested_subgraphs() {
    let ast = ast(
        "graph TD\nA-->B\nsubgraph myTitle\n\n c-->d \n\n subgraph inner\n\n   e-->f \n end \n\n subgraph inner\n\n   h-->i \n end \n\nend\n",
    );
    assert_eq!(ast.sub_graphs.len(), 2);
}

#[test]
fn should_handle_nested_subgraphs_1() {
    let ast = ast(
        "flowchart TB\n    subgraph A\n    b-->B\n    a\n    end\n    a-->c\n    subgraph B\n      c\n    end",
    );
    assert_eq!(ast.sub_graphs.len(), 2);
    let sub_graph_a = ast.sub_graph("A").expect("A");
    let sub_graph_b = ast.sub_graph("B").expect("B");
    assert_eq!(sub_graph_b.nodes[0], "c");
    assert!(sub_graph_a.nodes.contains(&"B".to_string()));
    assert!(sub_graph_a.nodes.contains(&"b".to_string()));
    assert!(sub_graph_a.nodes.contains(&"a".to_string()));
    assert!(!sub_graph_a.nodes.contains(&"c".to_string()));
}

#[test]
fn should_handle_nested_subgraphs_2() {
    let ast = ast(
        "flowchart TB\n    b-->B\n    a-->c\n    subgraph B\n      c\n    end\n    subgraph A\n        a\n        b\n        B\n    end",
    );
    assert_eq!(ast.sub_graphs.len(), 2);
    let sub_graph_a = ast.sub_graph("A").expect("A");
    let sub_graph_b = ast.sub_graph("B").expect("B");
    assert_eq!(sub_graph_b.nodes[0], "c");
    assert!(sub_graph_a.nodes.contains(&"B".to_string()));
    assert!(sub_graph_a.nodes.contains(&"b".to_string()));
    assert!(sub_graph_a.nodes.contains(&"a".to_string()));
    assert!(!sub_graph_a.nodes.contains(&"c".to_string()));
}

#[test]
fn should_handle_nested_subgraphs_3() {
    let ast = ast(
        "flowchart TB\n    subgraph B\n      c\n    end\n    a-->c\n    subgraph A\n      b-->B\n      a\n    end",
    );
    assert_eq!(ast.sub_graphs.len(), 2);
    let sub_graph_a = ast.sub_graph("A").expect("A");
    let sub_graph_b = ast.sub_graph("B").expect("B");
    assert_eq!(sub_graph_b.nodes[0], "c");
    assert!(sub_graph_a.nodes.contains(&"B".to_string()));
    assert!(sub_graph_a.nodes.contains(&"b".to_string()));
    assert!(sub_graph_a.nodes.contains(&"a".to_string()));
    assert!(!sub_graph_a.nodes.contains(&"c".to_string()));
}

#[test]
fn should_parse_direction_td_inside_a_subgraph() {
    let ast = ast(
        "\n      graph LR\n        subgraph WithTD\n          direction TD\n          A1 --> A2\n        end\n    ",
    );
    let sub_graph = &ast.sub_graphs[0];
    assert_eq!(sub_graph.dir.as_deref(), Some("TD"));
    assert!(sub_graph.nodes.contains(&"A1".to_string()));
    assert!(sub_graph.nodes.contains(&"A2".to_string()));
}

#[test]
fn should_attach_collapsed_metadata_to_a_subgraph_and_collapse_it() {
    let ast = ast(
        "\n      graph TD\n        subgraph one [My Group]\n          A --> B\n        end\n        C --> A\n        one@{ view: collapsed }\n    ",
    );
    assert_eq!(ast.sub_graphs.len(), 1);
    assert_eq!(
        ast.sub_graphs[0]
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("view")),
        Some(&serde_json::Value::String("collapsed".to_string()))
    );
    let collapsed = ast.node("one").expect("collapsed node");
    assert_eq!(collapsed.shape, "collapsedGroup");
    assert!(!collapsed.is_group);
    assert!(ast.node("A").is_none());
    assert!(ast.node("B").is_none());
    assert!(
        ast.layout_edges
            .iter()
            .any(|edge| edge.start == "C" && edge.end == "one")
    );
    assert!(!ast.layout_edges.iter().any(|edge| edge.end == "A"));
}
