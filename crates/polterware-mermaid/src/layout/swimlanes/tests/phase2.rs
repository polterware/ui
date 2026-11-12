use std::collections::HashMap;

use crate::layout::swimlanes::helpers::Graph;
use crate::render::pipeline::types::Node;

fn mk(nodes: &[&str], pairs: &[(&str, &str)]) -> Graph {
    super::mk_graph(nodes, pairs)
}

#[test]
fn compact_does_not_compact_chain_within_same_lane() {
    let g = mk(&["A", "B", "C"], &[("A", "B"), ("B", "C")]);
    let layering = crate::layout::swimlanes::phase2_longest_path::assign_layers_longest_path(
        &g,
        crate::layout::swimlanes::phase2_options::LayeringOptions {
            compact_single_input: Some(true),
            ..Default::default()
        },
    );
    assert_eq!(layering.layers.len(), 3);
    assert_eq!(layering.layers[0], vec!["A"]);
    assert_eq!(layering.layers[1], vec!["B"]);
    assert_eq!(layering.layers[2], vec!["C"]);
}

#[test]
fn compact_only_applies_across_lanes() {
    let mut layout = crate::render::pipeline::types::LayoutData::default();
    let mut node_by_id = HashMap::new();
    let lane1 = Node {
        id: "lane1".to_string(),
        is_group: Some(true),
        ..Default::default()
    };
    let lane2 = Node {
        id: "lane2".to_string(),
        is_group: Some(true),
        ..Default::default()
    };
    layout.nodes.push(lane1.clone());
    layout.nodes.push(lane2.clone());
    node_by_id.insert(lane1.id.clone(), lane1);
    node_by_id.insert(lane2.id.clone(), lane2);
    for (id, parent) in [("A", "lane1"), ("B", "lane1"), ("C", "lane2")] {
        let node = Node {
            id: id.to_string(),
            parent_id: Some(parent.to_string()),
            ..Default::default()
        };
        layout.nodes.push(node.clone());
        node_by_id.insert(node.id.clone(), node);
    }
    layout.edges = vec![
        crate::render::pipeline::types::Edge {
            id: "e0".to_string(),
            start: Some("A".to_string()),
            end: Some("B".to_string()),
            ..Default::default()
        },
        crate::render::pipeline::types::Edge {
            id: "e1".to_string(),
            start: Some("A".to_string()),
            end: Some("C".to_string()),
            ..Default::default()
        },
    ];
    let g = Graph {
        nodes: vec!["A".to_string(), "B".to_string(), "C".to_string()],
        edges: layout
            .edges
            .iter()
            .map(|edge| crate::layout::swimlanes::helpers::EdgeRef {
                id: edge.id.clone(),
                src: edge.start.clone().unwrap(),
                dst: edge.end.clone().unwrap(),
                weight: None,
                reference: edge.clone(),
            })
            .collect(),
        layout,
        node_by_id,
    };

    let layering = crate::layout::swimlanes::phase2_longest_path::assign_layers_longest_path(
        &g,
        crate::layout::swimlanes::phase2_options::LayeringOptions {
            compact_single_input: Some(true),
            ..Default::default()
        },
    );
    assert_eq!(layering.rank_of["A"], 0);
    assert_eq!(layering.rank_of["B"], 1);
    assert_eq!(layering.rank_of["C"], 0);
}

fn group(id: &str) -> Node {
    Node {
        id: id.to_string(),
        is_group: Some(true),
        ..Default::default()
    }
}

fn node(id: &str, lane: &str) -> Node {
    Node {
        id: id.to_string(),
        is_group: Some(false),
        parent_id: Some(lane.to_string()),
        ..Default::default()
    }
}

fn edge(id: &str, start: &str, end: &str) -> crate::render::pipeline::types::Edge {
    crate::render::pipeline::types::Edge {
        id: id.to_string(),
        start: Some(start.to_string()),
        end: Some(end.to_string()),
        r#type: Some("arrow".to_string()),
        ..Default::default()
    }
}

fn layout_with(nodes: Vec<Node>, edges: Vec<crate::render::pipeline::types::Edge>) -> Graph {
    let layout = crate::render::pipeline::types::LayoutData {
        nodes,
        edges,
        ..Default::default()
    };
    crate::layout::swimlanes::helpers::to_graph_view(&layout)
}

#[test]
fn lane_aware_compact_keeps_sibling_cross_lane_branches_before_downstream_branches() {
    let lanes = ["S", "T", "Tester", "D"].map(group);
    let nodes: Vec<Node> = lanes
        .into_iter()
        .chain([
            node("T5", "S"),
            node("T4", "T"),
            node("T6", "T"),
            node("Te2", "Tester"),
            node("D4", "Tester"),
            node("D1", "D"),
            node("D2", "D"),
            node("D3", "D"),
        ])
        .collect();
    let edges = [
        edge("e1", "Te2", "D1"),
        edge("e3", "D1", "D2"),
        edge("e4", "D2", "D3"),
        edge("e5", "D3", "D4"),
        edge("e6", "Te2", "T4"),
        edge("e8", "D4", "T6"),
        edge("e10", "T6", "T5"),
    ]
    .to_vec();
    let g = layout_with(nodes, edges);
    let layering =
        crate::layout::swimlanes::phase2_lane_aware_compact::assign_layers_lane_aware_compact(
            &g,
            crate::layout::swimlanes::phase2_options::LayeringOptions {
                ignore_cross_lane_edges: Some(true),
                direction: Some(crate::layout::swimlanes::helpers::Direction::Lr),
                ..Default::default()
            },
        );
    assert!(layering.rank_of["T4"] < layering.rank_of["T6"]);
}

#[test]
fn lane_aware_compact_assigns_expected_layers() {
    let nodes: Vec<Node> = [
        group("Car"),
        group("Sales"),
        group("Legal"),
        group("Constr"),
        group("Fun"),
        node("A", "Car"),
        node("B", "Sales"),
        node("C", "Constr"),
        node("D", "Constr"),
        node("E", "Constr"),
        node("F", "Fun"),
        node("G", "Fun"),
        node("H", "Constr"),
        node("I", "Legal"),
        node("J", "Legal"),
        node("K", "Legal"),
        node("L", "Constr"),
        node("M", "Sales"),
        node("N", "Constr"),
    ]
    .into_iter()
    .collect();
    let edges = [
        edge("e1", "A", "B"),
        edge("e2", "B", "C"),
        edge("e3", "C", "D"),
        edge("e4", "D", "E"),
        edge("e5", "E", "F"),
        edge("e6", "F", "G"),
        edge("e7", "D", "H"),
        edge("e8", "H", "I"),
        edge("e9", "I", "J"),
        edge("e10", "J", "E"),
        edge("e11", "I", "K"),
        edge("e12", "K", "L"),
        edge("e13", "L", "M"),
        edge("e14", "L", "N"),
    ]
    .to_vec();
    let g = layout_with(nodes, edges);
    let layering =
        crate::layout::swimlanes::phase2_lane_aware_compact::assign_layers_lane_aware_compact(
            &g,
            crate::layout::swimlanes::phase2_options::LayeringOptions {
                ignore_cross_lane_edges: Some(true),
                ..Default::default()
            },
        );
    let expected = [
        ("A", 0),
        ("B", 0),
        ("C", 0),
        ("D", 1),
        ("E", 3),
        ("F", 3),
        ("G", 4),
        ("H", 2),
        ("I", 2),
        ("J", 3),
        ("L", 4),
        ("M", 4),
        ("N", 5),
    ];
    for (id, layer) in expected {
        assert_eq!(layering.rank_of[id], layer, "node {id}");
    }
}
