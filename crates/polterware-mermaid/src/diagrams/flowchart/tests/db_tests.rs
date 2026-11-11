//! Port of `flowDb.spec.ts`, `flowDb-dropped-subgraph-members.spec.ts`, and the database parts of
//! `flowDb-elk-duplicate-subgraph.spec.ts`.
//!
//! Skipped from the upstream files: the `flowDiagram.spec.ts` cases (they only assert that `init`
//! does not force a layout, which is configuration), the ELK layout assertions (layout work), the
//! `clear()` lifecycle case (the parser builds a fresh database per call), and the JS-only check
//! that the JISON actions are own properties of the database object.

use serde_json::{Map, Value};

use super::super::db::{Db, FlowText, StatementValue, exists, make_uniq};
use super::super::{Ast, LabelType, SubGraph};
use super::{ast, vertex};
use crate::error::Span;

fn sub_graph(nodes: &[&str]) -> SubGraph {
    SubGraph {
        nodes: nodes.iter().map(|node| node.to_string()).collect(),
        ..SubGraph::default()
    }
}

#[test]
fn exists_finds_ids_in_subgraphs() {
    let subgraphs = [
        sub_graph(&["a", "b", "c", "e"]),
        sub_graph(&["f", "g", "h"]),
        sub_graph(&["i", "j"]),
        sub_graph(&["k"]),
    ];
    assert!(exists(&subgraphs, "a"));
    assert!(exists(&subgraphs, "h"));
    assert!(exists(&subgraphs, "j"));
    assert!(exists(&subgraphs, "k"));
    assert!(!exists(&subgraphs, "a2"));
    assert!(!exists(&subgraphs, "l"));
}

#[test]
fn make_uniq_removes_ids_owned_by_another_subgraph() {
    let subgraphs = [
        sub_graph(&["a", "b", "c", "e"]),
        sub_graph(&["f", "g", "h"]),
        sub_graph(&["i", "j"]),
        sub_graph(&["k"]),
    ];
    assert!(make_uniq(&["i".to_string(), "j".to_string()], &subgraphs).is_empty());
    assert_eq!(
        make_uniq(
            &["i".to_string(), "j".to_string(), "o".to_string()],
            &subgraphs
        ),
        vec!["o"]
    );
    assert_eq!(
        make_uniq(
            &["q".to_string(), "r".to_string(), "s".to_string()],
            &subgraphs
        ),
        vec!["q", "r", "s"]
    );
}

#[test]
fn add_class_detects_many_classes() {
    let mut db = Db::new();
    db.add_class("a,b", &["stroke-width: 8px".to_string()]);
    let class_a = db.class("a").expect("a");
    let class_b = db.class("b").expect("b");
    assert_eq!(class_a.styles, vec!["stroke-width: 8px"]);
    assert_eq!(class_b.styles, vec!["stroke-width: 8px"]);
}

#[test]
fn add_class_detects_a_single_class() {
    let mut db = Db::new();
    db.add_class("a", &["stroke-width: 8px".to_string()]);
    assert_eq!(db.class("a").expect("a").styles, vec!["stroke-width: 8px"]);
}

#[test]
fn get_data_uses_default_interpolate_for_edges_without_one() {
    let ast = ast("graph TD\nA-->B\nlinkStyle default interpolate stepBefore");
    assert_eq!(ast.layout_edges[0].curve.as_deref(), Some("stepBefore"));
}

#[test]
fn get_data_prioritizes_edge_specific_interpolate_over_the_default() {
    let ast = ast(
        "graph TD\nA-->B\nlinkStyle default interpolate stepBefore\nlinkStyle 0 interpolate basis",
    );
    assert_eq!(ast.layout_edges[0].curve.as_deref(), Some("basis"));
}

#[test]
fn get_data_supports_modifying_interpolate_using_the_edge_id_syntax() {
    let ast = ast(
        "graph TD\nA-->B\nA e2@-->C\nB e3@-->D\nC-->D\nlinkStyle default interpolate stepBefore\nlinkStyle 0 interpolate basis\ne2@{ curve: monotoneX }\ne3@{ curve: catmullRom }",
    );
    assert_eq!(ast.layout_edges[0].curve.as_deref(), Some("basis"));
    assert_eq!(ast.layout_edges[1].curve.as_deref(), Some("monotoneX"));
    assert_eq!(ast.layout_edges[2].curve.as_deref(), Some("catmullRom"));
    assert_eq!(ast.layout_edges[3].curve.as_deref(), Some("stepBefore"));
}

#[test]
fn renders_a_subgraph_as_a_group_when_no_collapse_metadata_is_set() {
    let ast = ast("flowchart LR\nA\nB\nC\nA-->B\nC-->A\nsubgraph sub1 [My Group]\nA\nB\nend");
    let sub = ast.node("sub1").expect("sub1");
    assert!(sub.is_group);
    let ids: Vec<&str> = ast.nodes.iter().map(|node| node.id.as_str()).collect();
    for id in ["A", "B", "C", "sub1"] {
        assert!(ids.contains(&id), "{id}");
    }
    assert!(
        ast.layout_edges
            .iter()
            .any(|edge| edge.start == "C" && edge.end == "A")
    );
}

#[test]
fn forwards_subgraph_metadata_to_the_group_node() {
    let ast =
        ast("flowchart LR\nA\nB\nsubgraph sub1 [My Group]\nA\nB\nend\nsub1@{ algorithm: elk.box }");
    let sub = ast.node("sub1").expect("sub1");
    assert!(sub.is_group);
    assert_eq!(
        sub.metadata
            .as_ref()
            .and_then(|metadata| metadata.get("algorithm")),
        Some(&Value::String("elk.box".to_string()))
    );
}

#[test]
fn renders_a_collapsed_subgraph_as_a_single_node_and_hides_members() {
    let ast = ast(
        "flowchart LR\nA\nB\nC\nA-->B\nC-->A\nsubgraph sub1 [My Group]\nA\nB\nend\nsub1@{ view: collapsed }",
    );
    let sub = ast.node("sub1").expect("sub1");
    assert_eq!(sub.shape, "collapsedGroup");
    assert!(!sub.is_group);
    assert_eq!(sub.label, "My Group");
    assert!(ast.node("A").is_none());
    assert!(ast.node("B").is_none());
    assert!(ast.node("C").is_some());
}

#[test]
fn redirects_edges_crossing_into_a_collapsed_subgraph() {
    let ast = ast(
        "flowchart LR\nA\nB\nC\nC-->A\nsubgraph sub1 [My Group]\nA\nB\nend\nsub1@{ view: collapsed }",
    );
    assert!(
        ast.layout_edges
            .iter()
            .any(|edge| edge.start == "C" && edge.end == "sub1")
    );
    assert!(!ast.layout_edges.iter().any(|edge| edge.end == "A"));
}

#[test]
fn drops_edges_fully_internal_to_a_collapsed_subgraph() {
    let ast = ast(
        "flowchart LR\nA\nB\nA-->B\nsubgraph sub1 [My Group]\nA\nB\nend\nsub1@{ view: collapsed }",
    );
    assert!(ast.layout_edges.is_empty());
}

#[test]
fn redirects_edges_to_the_outermost_collapsed_ancestor() {
    let ast = ast(
        "flowchart LR\nA\nB\nC\nC-->A\nsubgraph inner [Inner]\nA\nB\nend\nsubgraph outer [Outer]\ninner\nend\ninner@{ view: collapsed }\nouter@{ view: collapsed }",
    );
    assert_eq!(ast.node("outer").expect("outer").shape, "collapsedGroup");
    assert!(ast.node("inner").is_none());
    assert!(ast.node("A").is_none());
    assert!(
        ast.layout_edges
            .iter()
            .any(|edge| edge.start == "C" && edge.end == "outer")
    );
}

#[test]
fn direction_normalizes_td_and_leading_spaces() {
    let mut db = Db::new();
    db.set_direction("TD");
    assert_eq!(db.direction(), Some("TB"));
    db.set_direction(" TD");
    assert_eq!(db.direction(), Some("TB"));
}

fn colour_slot(ast: &Ast, id: &str) -> Option<usize> {
    ast.node(id).and_then(|node| node.color_index)
}

#[test]
fn numbers_flat_subgraphs_in_source_order() {
    let ast = ast(
        "flowchart LR\nA\nB\nC\nsubgraph first [First]\nA\nend\nsubgraph second [Second]\nB\nend\nsubgraph third [Third]\nC\nend",
    );
    assert_eq!(
        [
            colour_slot(&ast, "first"),
            colour_slot(&ast, "second"),
            colour_slot(&ast, "third")
        ],
        [Some(0), Some(1), Some(2)]
    );
}

#[test]
fn numbers_nested_subgraphs_in_source_order_parent_first() {
    let ast = ast(
        "flowchart LR\nA\nB\nC\nD\nsubgraph InnerOne [InnerOne]\nA\nend\nsubgraph InnerTwo [InnerTwo]\nB\nend\nsubgraph Outer [Outer]\nInnerOne\nInnerTwo\nend\nsubgraph Sibling [Sibling]\nD\nend",
    );
    assert_eq!(
        [
            colour_slot(&ast, "Outer"),
            colour_slot(&ast, "InnerOne"),
            colour_slot(&ast, "InnerTwo"),
            colour_slot(&ast, "Sibling"),
        ],
        [Some(0), Some(1), Some(2), Some(3)]
    );
}

#[test]
fn keeps_a_collapsed_subgraph_on_its_own_slot() {
    let ast = ast(
        "flowchart LR\nA\nB\nC\nsubgraph first [First]\nA\nend\nsubgraph second [Second]\nB\nend\nsubgraph third [Third]\nC\nend\nsecond@{ view: collapsed }",
    );
    assert_eq!(colour_slot(&ast, "first"), Some(0));
    assert_eq!(colour_slot(&ast, "second"), Some(1));
    assert_eq!(colour_slot(&ast, "third"), Some(2));
}

#[test]
fn leaves_plain_vertices_without_a_colour_slot() {
    let ast = ast("flowchart LR\nA\nsubgraph only [Only]\nA\nend");
    assert_eq!(ast.node("A").expect("A").color_index, None);
    assert_eq!(colour_slot(&ast, "only"), Some(0));
}

#[test]
fn merges_a_repeated_subgraph_id() {
    let ast = ast("flowchart LR\nsubgraph S\n  x\nend\nsubgraph S\n  y\nend");
    let ids: Vec<&str> = ast
        .sub_graphs
        .iter()
        .map(|sub_graph| sub_graph.id.as_str())
        .collect();
    assert_eq!(ids, vec!["S"]);
    assert_eq!(ast.sub_graph("S").expect("S").nodes, vec!["x", "y"]);
}

#[test]
fn keeps_the_title_of_the_first_declaration_of_a_repeated_subgraph() {
    let ast = ast("flowchart LR\nsubgraph S [First]\n  x\nend\nsubgraph S [Second]\n  y\nend");
    assert_eq!(ast.sub_graph("S").expect("S").title, "First");
}

#[test]
fn produces_a_single_group_node_for_a_repeated_subgraph() {
    let ast = ast("flowchart LR\nsubgraph S\n  x\nend\nsubgraph S\n  y\nend");
    assert_eq!(ast.nodes.iter().filter(|node| node.id == "S").count(), 1);
    assert!(ast.node("S").expect("S").is_group);
    assert_eq!(ast.node("x").expect("x").parent_id.as_deref(), Some("S"));
    assert_eq!(ast.node("y").expect("y").parent_id.as_deref(), Some("S"));
    let mut ids: Vec<&str> = ast.nodes.iter().map(|node| node.id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), ast.nodes.len());
}

#[test]
fn applies_a_class_set_after_a_repeated_declaration() {
    let ast = ast(
        "flowchart LR\nsubgraph S\n  x\nend\nsubgraph S\n  y\nend\nclassDef hot fill:#f00\nclass S hot",
    );
    assert!(
        ast.sub_graph("S")
            .expect("S")
            .classes
            .contains(&"hot".to_string())
    );
    assert!(ast.node("S").expect("S").css_classes.contains("hot"));
}

#[test]
fn collapses_a_repeated_subgraph_when_metadata_comes_after() {
    let ast =
        ast("flowchart LR\nsubgraph S\n  x\nend\nsubgraph S\n  y\nend\nS@{ view: collapsed }");
    assert_eq!(
        ast.sub_graph("S")
            .expect("S")
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("view")),
        Some(&Value::String("collapsed".to_string()))
    );
    assert_eq!(ast.nodes.iter().filter(|node| node.id == "S").count(), 1);
    assert_eq!(ast.node("S").expect("S").shape, "collapsedGroup");
    assert!(ast.node("x").is_none());
    assert!(ast.node("y").is_none());
}

#[test]
fn collapses_a_repeated_subgraph_when_metadata_comes_between() {
    let ast =
        ast("flowchart LR\nsubgraph S\n  x\nend\nS@{ view: collapsed }\nsubgraph S\n  y\nend");
    assert_eq!(
        ast.sub_graph("S")
            .expect("S")
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("view")),
        Some(&Value::String("collapsed".to_string()))
    );
    assert_eq!(ast.node("S").expect("S").shape, "collapsedGroup");
    assert!(ast.node("x").is_none());
    assert!(ast.node("y").is_none());
}

#[test]
fn flattens_a_subgraph_redeclared_inside_its_own_body() {
    let ast = ast("flowchart LR\nsubgraph S\n  subgraph S\n    x\n  end\n  y\nend");
    assert_eq!(ast.sub_graphs.len(), 1);
    let members = &ast.sub_graph("S").expect("S").nodes;
    assert!(!members.contains(&"S".to_string()));
    assert!(members.contains(&"x".to_string()));
    assert!(members.contains(&"y".to_string()));
    assert_eq!(ast.nodes.iter().filter(|node| node.id == "S").count(), 1);
    assert!(
        ast.nodes
            .iter()
            .all(|node| node.parent_id.as_deref() != Some(node.id.as_str()))
    );
    assert_eq!(ast.node("x").expect("x").parent_id.as_deref(), Some("S"));
    assert_eq!(ast.node("y").expect("y").parent_id.as_deref(), Some("S"));
}

#[test]
fn merges_an_empty_first_declaration_with_a_later_nested_subgraph() {
    let ast = ast("flowchart LR\nsubgraph S\nend\nsubgraph S\n  subgraph T\n    y\n  end\nend");
    let mut ids: Vec<&str> = ast
        .sub_graphs
        .iter()
        .map(|sub_graph| sub_graph.id.as_str())
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, vec!["S", "T"]);
    assert_eq!(ast.sub_graph("S").expect("S").nodes, vec!["T"]);
    assert_eq!(ast.sub_graph("T").expect("T").nodes, vec!["y"]);
    assert_eq!(ast.node("T").expect("T").parent_id.as_deref(), Some("S"));
    assert_eq!(ast.node("y").expect("y").parent_id.as_deref(), Some("T"));
}

#[test]
fn keeps_distinct_and_anonymous_subgraphs_separate() {
    let ast = ast(
        "flowchart LR\nsubgraph A\n  a\nend\nsubgraph B\n  b\nend\nsubgraph \"Same Title\"\n  c\nend\nsubgraph \"Same Title\"\n  d\nend",
    );
    assert_eq!(ast.sub_graphs.len(), 4);
    assert_eq!(ast.sub_graph("A").expect("A").nodes, vec!["a"]);
    assert_eq!(ast.sub_graph("B").expect("B").nodes, vec!["b"]);
    assert_eq!(
        ast.sub_graph("subGraph2").expect("subGraph2").nodes,
        vec!["c"]
    );
    assert_eq!(
        ast.sub_graph("subGraph3").expect("subGraph3").nodes,
        vec!["d"]
    );
}

fn pool_diagram(metadata: Option<&str>) -> Vec<super::super::Node> {
    let text = |value: &str| FlowText {
        text: value.to_string(),
        label_type: LabelType::Text,
    };
    let mut db = Db::new();
    for id in ["A", "B", "C"] {
        db.add_vertex(
            id,
            Some(text(id)),
            None,
            Some(&[]),
            Some(&[]),
            Some(""),
            Some(Map::new()),
            None,
            Span::default(),
        )
        .expect("vertex");
    }
    db.add_sub_graph(
        Some(&text("P")),
        Some(&text("Company")),
        false,
        &[
            StatementValue::Text("L1".to_string()),
            StatementValue::Text("L2".to_string()),
        ],
    );
    db.add_sub_graph(
        Some(&text("L1")),
        Some(&text("Sales")),
        false,
        &[StatementValue::Nodes(vec![
            "A".to_string(),
            "B".to_string(),
        ])],
    );
    db.add_sub_graph(
        Some(&text("L2")),
        Some(&text("Ops")),
        false,
        &[StatementValue::Nodes(vec!["C".to_string()])],
    );
    if let Some(metadata) = metadata {
        let (id, rest) = metadata.split_once('|').expect("id|metadata");
        db.add_vertex(
            id,
            None,
            None,
            Some(&[]),
            Some(&[]),
            Some(""),
            Some(Map::new()),
            Some(rest),
            Span::default(),
        )
        .expect("metadata");
    }
    db.get_data().0
}

#[test]
fn drops_keys_only_a_pool_diagram_may_set() {
    for metadata in [
        "P|laneRole: pool",
        "L1|laneIndex: 1",
        "L1|drawnExtent: { width: 5, height: 5 }",
    ] {
        assert_eq!(
            pool_diagram(Some(metadata)),
            pool_diagram(None),
            "{metadata}"
        );
    }
}

#[test]
fn keeps_the_other_keys_a_subgraph_is_given() {
    let nodes = pool_diagram(Some("L1|laneRole: pool\nalgorithm: elk.box"));
    let l1 = nodes.iter().find(|node| node.id == "L1").expect("L1");
    assert_eq!(
        l1.metadata
            .as_ref()
            .and_then(|metadata| metadata.get("algorithm")),
        Some(&Value::String("elk.box".to_string()))
    );
    assert!(
        l1.metadata
            .as_ref()
            .is_some_and(|metadata| !metadata.contains_key("laneRole"))
    );
}

fn dropped(body: &str) -> Vec<super::super::DroppedSubGraphMember> {
    ast(body).dropped_sub_graph_members
}

#[test]
fn records_a_node_listed_in_two_unrelated_subgraphs() {
    let dropped = dropped("flowchart LR\nsubgraph lane1\n  a\nend\nsubgraph phase1\n  a\nend");
    assert_eq!(
        dropped,
        vec![super::super::DroppedSubGraphMember {
            node: "a".to_string(),
            kept_by: "lane1".to_string(),
            dropped_from: "phase1".to_string(),
        }]
    );
}

#[test]
fn records_every_shared_node_of_a_subgraph_in_source_order() {
    let dropped =
        dropped("flowchart LR\nsubgraph one\n  a\n  b\nend\nsubgraph two\n  b\n  a\n  c\nend");
    assert_eq!(
        dropped,
        vec![
            super::super::DroppedSubGraphMember {
                node: "b".to_string(),
                kept_by: "one".to_string(),
                dropped_from: "two".to_string(),
            },
            super::super::DroppedSubGraphMember {
                node: "a".to_string(),
                kept_by: "one".to_string(),
                dropped_from: "two".to_string(),
            },
        ]
    );
}

#[test]
fn records_nothing_for_subgraphs_that_share_no_node() {
    assert!(dropped("flowchart LR\nsubgraph one\n  a\nend\nsubgraph two\n  b\nend").is_empty());
}

#[test]
fn records_nothing_for_a_subgraph_nested_in_another_with_the_same_node() {
    assert!(
        dropped("flowchart LR\nsubgraph outer\n  subgraph inner\n    a\n  end\nend").is_empty()
    );
}

#[test]
fn records_nothing_when_the_keeping_subgraph_is_nested_deeper() {
    assert!(dropped(
        "flowchart LR\nsubgraph L1\n  subgraph M\n    subgraph I\n      a\n    end\n  end\n  a\nend"
    )
    .is_empty());
}

#[test]
fn records_nothing_when_a_repeated_subgraph_id_lists_a_node_again() {
    assert!(dropped("flowchart LR\nsubgraph S\n  x\nend\nsubgraph S\n  x\n  y\nend").is_empty());
}

#[test]
fn keeps_vertex_ids_for_collapsed_subgraphs() {
    let ast = ast("flowchart LR\nsubgraph S\n  x\nend\nS@{ view: collapsed }");
    assert_eq!(vertex(&ast, "x").id, "x");
}
