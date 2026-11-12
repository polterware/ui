use std::collections::HashMap;

use crate::layout::swimlanes::helpers::{Graph, OrderedLayers};
use crate::layout::swimlanes::phase4_coordinates::{CoordOptions, assign_coordinates};
use crate::render::pipeline::types::Node;

fn mk(nodes: &[&str], pairs: &[(&str, &str)]) -> Graph {
    super::mk_graph(nodes, pairs)
}

fn opts(node_gap: f64, layer_gap: f64) -> Option<CoordOptions> {
    Some(CoordOptions {
        node_gap: Some(node_gap),
        layer_gap: Some(layer_gap),
        ..Default::default()
    })
}

#[test]
fn single_layer_is_monotone_with_node_gap_and_constant_y() {
    let g = mk(&["A", "B", "C"], &[]);
    let ordered = OrderedLayers {
        layers: vec![vec!["A".to_string(), "B".to_string(), "C".to_string()]],
    };
    let coords = assign_coordinates(&ordered, &g, opts(50.0, 80.0));
    assert_eq!(coords.y["A"], 0.0);
    assert_eq!(coords.y["B"], 0.0);
    assert_eq!(coords.y["C"], 0.0);
    assert_eq!(coords.x["A"], -50.0);
    assert_eq!(coords.x["B"], 0.0);
    assert_eq!(coords.x["C"], 50.0);
}

fn graph_with_lanes(nodes: Vec<Node>, node_ids: &[&str]) -> Graph {
    let layout = crate::render::pipeline::types::LayoutData {
        nodes: nodes.clone(),
        ..Default::default()
    };
    let node_by_id: HashMap<String, Node> = nodes
        .iter()
        .map(|node| (node.id.clone(), node.clone()))
        .collect();
    Graph {
        nodes: node_ids.iter().map(|id| (*id).to_string()).collect(),
        edges: Vec::new(),
        layout,
        node_by_id,
    }
}

#[test]
fn separates_nodes_of_different_lanes_with_lane_gap() {
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
    let a = Node {
        id: "A".to_string(),
        is_group: Some(false),
        width: Some(100.0),
        height: Some(50.0),
        ..Default::default()
    };
    let b = Node {
        id: "B".to_string(),
        is_group: Some(false),
        width: Some(120.0),
        height: Some(50.0),
        parent_id: Some("lane1".to_string()),
        ..Default::default()
    };
    let c = Node {
        id: "C".to_string(),
        is_group: Some(false),
        width: Some(180.0),
        height: Some(50.0),
        parent_id: Some("lane2".to_string()),
        ..Default::default()
    };
    let d = Node {
        id: "D".to_string(),
        is_group: Some(false),
        width: Some(80.0),
        height: Some(50.0),
        ..Default::default()
    };
    let g = graph_with_lanes(vec![lane2, lane1, a, b, c, d], &["A", "B", "C", "D"]);
    let ordered = OrderedLayers {
        layers: vec![vec!["B".to_string(), "C".to_string()]],
    };
    let coords = assign_coordinates(
        &ordered,
        &g,
        Some(CoordOptions {
            node_gap: Some(40.0),
            lane_gap: Some(100.0),
            layer_gap: Some(80.0),
            ..Default::default()
        }),
    );
    let expected_min = 100.0 + (120.0 + 180.0) / 2.0;
    assert!(coords.x["C"] - coords.x["B"] >= expected_min);
}

#[test]
fn keeps_same_x_for_nodes_in_the_same_lane_across_layers() {
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
    let a = Node {
        id: "A".to_string(),
        is_group: Some(false),
        width: Some(120.0),
        height: Some(50.0),
        parent_id: Some("lane1".to_string()),
        ..Default::default()
    };
    let b = Node {
        id: "B".to_string(),
        is_group: Some(false),
        width: Some(160.0),
        height: Some(50.0),
        parent_id: Some("lane1".to_string()),
        ..Default::default()
    };
    let c = Node {
        id: "C".to_string(),
        is_group: Some(false),
        width: Some(200.0),
        height: Some(50.0),
        parent_id: Some("lane2".to_string()),
        ..Default::default()
    };
    let d = Node {
        id: "D".to_string(),
        is_group: Some(false),
        width: Some(100.0),
        height: Some(50.0),
        parent_id: Some("lane1".to_string()),
        ..Default::default()
    };
    let g = graph_with_lanes(vec![lane2, lane1, a, b, c, d], &["A", "B", "C", "D"]);
    let ordered = OrderedLayers {
        layers: vec![
            vec!["A".to_string()],
            vec!["B".to_string(), "C".to_string()],
            vec!["D".to_string()],
        ],
    };
    let coords = assign_coordinates(
        &ordered,
        &g,
        Some(CoordOptions {
            node_gap: Some(40.0),
            lane_gap: Some(120.0),
            layer_gap: Some(100.0),
            ..Default::default()
        }),
    );
    assert_eq!(coords.x["A"], coords.x["B"]);
    assert_eq!(coords.x["A"], coords.x["D"]);
    assert!(coords.x["C"] > coords.x["B"]);
}
