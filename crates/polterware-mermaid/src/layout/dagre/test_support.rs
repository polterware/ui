//! Shared helpers for the dagre test modules.

use super::super::graphlib::GraphOptions;
use super::{EdgeLabel, GraphLabel, LayeredGraph, NodeLabel};

/// A simple directed layout graph.
pub fn graph() -> LayeredGraph {
    let mut graph = LayeredGraph::new();
    graph.set_graph(GraphLabel::default());
    graph.set_default_node_label(NodeLabel::default());
    graph.set_default_edge_label(weighted(1.0, 1.0));
    graph
}

/// A multigraph layout graph.
pub fn multigraph() -> LayeredGraph {
    let mut graph = LayeredGraph::with_options(GraphOptions::multigraph());
    graph.set_graph(GraphLabel::default());
    graph.set_default_node_label(NodeLabel::default());
    graph.set_default_edge_label(weighted(1.0, 1.0));
    graph
}

/// A compound layout graph.
pub fn compound() -> LayeredGraph {
    let mut graph = LayeredGraph::with_options(GraphOptions::compound());
    graph.set_graph(GraphLabel::default());
    graph.set_default_node_label(NodeLabel::default());
    graph.set_default_edge_label(weighted(1.0, 1.0));
    graph
}

/// A multigraph with compound nodes, what Mermaid lays out.
pub fn layering() -> LayeredGraph {
    let mut graph = LayeredGraph::with_options(GraphOptions {
        multigraph: true,
        compound: true,
        ..GraphOptions::default()
    });
    graph.set_graph(GraphLabel::default());
    graph.set_default_node_label(NodeLabel::default());
    graph.set_default_edge_label(weighted(1.0, 1.0));
    graph
}

/// A node label with the given size.
pub fn sized(width: f64, height: f64) -> NodeLabel {
    NodeLabel {
        width,
        height,
        ..NodeLabel::default()
    }
}

/// A node label with a rank.
pub fn ranked(rank: f64) -> NodeLabel {
    NodeLabel {
        rank: Some(rank),
        ..NodeLabel::default()
    }
}

/// An edge label with a weight and a minimum length.
pub fn weighted(weight: f64, minlen: f64) -> EdgeLabel {
    EdgeLabel {
        weight: Some(weight),
        minlen: Some(minlen),
        ..EdgeLabel::default()
    }
}

/// Asserts two coordinates are within the layout test tolerance.
pub fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 1e-9,
        "expected {expected}, got {actual}"
    );
}

/// Asserts two optional coordinates are within the layout test tolerance.
pub fn close_option(actual: Option<f64>, expected: f64) {
    close(actual.expect("the coordinate is set"), expected);
}

/// The `(x, y)` of a node, in graph order.
pub fn coordinates(graph: &LayeredGraph) -> Vec<(String, f64, f64)> {
    graph
        .nodes()
        .into_iter()
        .map(|v| {
            let node = graph.node(&v).expect("the node exists");
            (
                v,
                node.x.expect("the node has an x"),
                node.y.expect("the node has a y"),
            )
        })
        .collect()
}

/// The endpoints and name of every edge, in graph order.
pub fn edge_ids(graph: &LayeredGraph) -> Vec<(String, String, Option<String>)> {
    graph
        .edges()
        .into_iter()
        .map(|edge| (edge.v, edge.w, edge.name))
        .collect()
}

/// Sets a node with an order and a rank.
pub fn ordered(graph: &mut LayeredGraph, v: &str, rank: f64, order: usize) {
    graph.set_node_with(
        v,
        NodeLabel {
            rank: Some(rank),
            order: Some(order),
            ..NodeLabel::default()
        },
    );
}
