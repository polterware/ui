use std::collections::HashMap;

use crate::layout::swimlanes::helpers::{EdgeRef, Graph};
use crate::layout::swimlanes::lane_ordering::{
    build_weighted_lane_edges, lane_arrangement_cost, optimize_top_lane_order,
};
use crate::layout::swimlanes::phase2_options::build_top_lane_order;
use crate::render::pipeline::types::{Edge, LayoutData, Node};

fn mk_graph(lanes: &[&str], pairs: &[(&str, &str)]) -> Graph {
    let mut nodes: Vec<Node> = Vec::new();
    let mut node_by_id: HashMap<String, Node> = HashMap::new();
    for lane_id in lanes.iter().rev() {
        let lane = Node {
            id: (*lane_id).to_string(),
            is_group: Some(true),
            ..Default::default()
        };
        nodes.push(lane.clone());
        node_by_id.insert(lane.id.clone(), lane);
    }
    for lane_id in lanes {
        let node = Node {
            id: format!("{lane_id}-node"),
            is_group: Some(false),
            parent_id: Some((*lane_id).to_string()),
            ..Default::default()
        };
        nodes.push(node.clone());
        node_by_id.insert(node.id.clone(), node);
    }
    let edges: Vec<Edge> = pairs
        .iter()
        .enumerate()
        .map(|(index, (src, dst))| Edge {
            id: format!("e{index}"),
            start: Some(format!("{src}-node")),
            end: Some(format!("{dst}-node")),
            r#type: Some("normal".to_string()),
            ..Default::default()
        })
        .collect();
    let mut layout = LayoutData {
        nodes,
        edges: edges.clone(),
        ..Default::default()
    };
    layout.edges = edges;
    let graph_edges: Vec<EdgeRef> = layout
        .edges
        .iter()
        .map(|edge| EdgeRef {
            id: edge.id.clone(),
            src: edge.start.clone().unwrap(),
            dst: edge.end.clone().unwrap(),
            weight: None,
            reference: edge.clone(),
        })
        .collect();
    let nodes = node_by_id.keys().cloned().collect();
    Graph {
        nodes,
        edges: graph_edges,
        layout,
        node_by_id,
    }
}

#[test]
fn counts_visible_interlane_edges_as_undirected_weights() {
    let mut g = mk_graph(
        &["lane1", "lane2", "lane3"],
        &[
            ("lane1", "lane2"),
            ("lane2", "lane1"),
            ("lane1", "lane3"),
            ("lane1", "lane1"),
        ],
    );
    g.layout.edges.push(Edge {
        id: "layout-only-label".to_string(),
        start: Some("lane1-node".to_string()),
        end: Some("lane2-node".to_string()),
        is_layout_only: Some(true),
        ..Default::default()
    });

    let weights = build_weighted_lane_edges(&g);
    assert_eq!(weights.len(), 2);
    assert_eq!(weights[0].a, "lane1");
    assert_eq!(weights[0].b, "lane2");
    assert_eq!(weights[0].weight, 2);
    assert_eq!(weights[1].a, "lane1");
    assert_eq!(weights[1].b, "lane3");
    assert_eq!(weights[1].weight, 1);
}

#[test]
fn reduces_weighted_linear_arrangement_cost() {
    let g = mk_graph(
        &["laneA", "laneB", "laneC", "laneD"],
        &[
            ("laneA", "laneD"),
            ("laneA", "laneD"),
            ("laneA", "laneD"),
            ("laneA", "laneD"),
        ],
    );
    let weights = build_weighted_lane_edges(&g);
    let source_order = build_top_lane_order(&g);
    let optimized = optimize_top_lane_order(&g, Some(8));
    assert!(
        lane_arrangement_cost(&optimized, &weights)
            < lane_arrangement_cost(&source_order, &weights)
    );
    assert_eq!(optimize_top_lane_order(&g, Some(8)), optimized);
}

#[test]
fn preserves_source_order_when_every_order_has_the_same_cost() {
    let g = mk_graph(
        &["laneA", "laneB", "laneC"],
        &[("laneA", "laneB"), ("laneA", "laneC"), ("laneB", "laneC")],
    );
    assert_eq!(
        optimize_top_lane_order(&g, Some(8)),
        vec!["laneA", "laneB", "laneC"]
    );
}
