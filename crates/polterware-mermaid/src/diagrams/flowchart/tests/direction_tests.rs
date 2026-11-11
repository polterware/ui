//! Port of `parser/flow-direction.spec.js`.

use super::{ast, text};

#[test]
fn should_use_default_direction_from_top_level() {
    let ast = ast("flowchart TB\n    subgraph A\n      a --> b\n    end");
    assert_eq!(ast.sub_graphs.len(), 1);
    let sub_graph = &ast.sub_graphs[0];
    assert_eq!(sub_graph.nodes.len(), 2);
    assert_eq!(sub_graph.nodes, vec!["b", "a"]);
    assert_eq!(sub_graph.id, "A");
    assert_eq!(sub_graph.dir, None);
}

#[test]
fn should_handle_a_subgraph_with_a_direction() {
    let ast = ast("flowchart TB\n    subgraph A\n      direction BT\n      a --> b\n    end");
    assert_eq!(ast.sub_graphs.len(), 1);
    let sub_graph = &ast.sub_graphs[0];
    assert_eq!(sub_graph.nodes, vec!["b", "a"]);
    assert_eq!(sub_graph.id, "A");
    assert_eq!(sub_graph.dir.as_deref(), Some("BT"));
}

#[test]
fn should_use_the_last_defined_direction() {
    let ast = ast(
        "flowchart TB\n    subgraph A\n      direction BT\n      a --> b\n      direction RL\n    end",
    );
    assert_eq!(ast.sub_graphs.len(), 1);
    let sub_graph = &ast.sub_graphs[0];
    assert_eq!(sub_graph.id, "A");
    assert_eq!(sub_graph.dir.as_deref(), Some("RL"));
}

#[test]
fn should_handle_nested_subgraphs_1() {
    let ast = ast(
        "flowchart TB\n    subgraph A\n      direction RL\n      b-->B\n      a\n    end\n    a-->c\n    subgraph B\n      direction LR\n      c\n    end",
    );
    assert_eq!(ast.sub_graphs.len(), 2);
    let sub_graph_a = ast.sub_graph("A").expect("A");
    let sub_graph_b = ast.sub_graph("B").expect("B");
    assert_eq!(sub_graph_b.nodes[0], "c");
    assert_eq!(sub_graph_b.dir.as_deref(), Some("LR"));
    assert!(sub_graph_a.nodes.contains(&"B".to_string()));
    assert!(sub_graph_a.nodes.contains(&"b".to_string()));
    assert!(sub_graph_a.nodes.contains(&"a".to_string()));
    assert!(!sub_graph_a.nodes.contains(&"c".to_string()));
    assert_eq!(sub_graph_a.dir.as_deref(), Some("RL"));
    // The vertex named `c` still exists; only the membership was deduped.
    assert_eq!(text(&ast, "c"), "c");
}
