//! Port of `dagre-d3-es/src/dagre/position/index.js` (MIT, Copyright (c) 2012-2014
//! Chris Pettitt).
//!
//! Assigns y by rank and x by the Brandes-Köpf alignment in [`bk`].

pub mod bk;

use super::LayeredGraph;
use super::util;

/// Assigns `x` and `y` to every non-compound node of `graph`.
pub fn position(graph: &mut LayeredGraph) {
    let mut simple = util::as_non_compound_graph(graph);
    position_y(&mut simple);
    let xs = bk::position_x(&simple);
    for (v, x) in xs {
        if let Some(node) = simple.node_mut(&v) {
            node.x = Some(x);
        }
    }

    for v in simple.nodes() {
        let (x, y) = simple
            .node(&v)
            .map(|node| (node.x, node.y))
            .unwrap_or((None, None));
        if let Some(node) = graph.node_mut(&v) {
            node.x = x;
            node.y = y;
        }
    }
}

fn position_y(graph: &mut LayeredGraph) {
    let layering = util::build_layer_matrix(graph);
    let graph_label = graph.graph().cloned().unwrap_or_default();
    let rank_sep = graph_label.ranksep.unwrap_or(0.0);
    let mut previous_y = 0.0;

    for layer in layering {
        let max_height = layer.iter().fold(0.0, |accumulator, v| {
            let height = graph.node(v).map_or(0.0, |node| node.height);
            if accumulator > height {
                accumulator
            } else {
                height
            }
        });
        for v in &layer {
            if let Some(node) = graph.node_mut(v) {
                node.y = Some(previous_y + max_height / 2.0);
            }
        }
        previous_y += max_height + rank_sep;
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{close, compound, sized};
    use super::super::{GraphLabel, LayeredGraph, NodeLabel};

    fn setup() -> LayeredGraph {
        let mut graph = compound();
        graph.set_graph(GraphLabel {
            ranksep: Some(50.0),
            nodesep: Some(50.0),
            edgesep: Some(10.0),
            ..GraphLabel::default()
        });
        graph
    }

    fn node(width: f64, height: f64, rank: f64, order: usize) -> NodeLabel {
        NodeLabel {
            width,
            height,
            rank: Some(rank),
            order: Some(order),
            ..NodeLabel::default()
        }
    }

    #[test]
    fn respects_ranksep() {
        let mut graph = setup();
        graph.set_graph(GraphLabel {
            ranksep: Some(1000.0),
            ..graph.graph().unwrap().clone()
        });
        graph.set_node_with("a", node(50.0, 100.0, 0.0, 0));
        graph.set_node_with("b", node(50.0, 80.0, 1.0, 0));
        graph.set_edge("a", "b");
        super::position(&mut graph);
        close(graph.node("b").unwrap().y.unwrap(), 100.0 + 1000.0 + 40.0);
    }

    #[test]
    fn uses_the_largest_height_in_each_rank() {
        let mut graph = setup();
        graph.set_graph(GraphLabel {
            ranksep: Some(1000.0),
            ..graph.graph().unwrap().clone()
        });
        graph.set_node_with("a", node(50.0, 100.0, 0.0, 0));
        graph.set_node_with("b", node(50.0, 80.0, 0.0, 1));
        graph.set_node_with("c", node(50.0, 90.0, 1.0, 0));
        graph.set_edge("a", "c");
        super::position(&mut graph);
        close(graph.node("a").unwrap().y.unwrap(), 50.0);
        close(graph.node("b").unwrap().y.unwrap(), 50.0);
        close(graph.node("c").unwrap().y.unwrap(), 100.0 + 1000.0 + 45.0);
    }

    #[test]
    fn respects_nodesep() {
        let mut graph = setup();
        graph.set_graph(GraphLabel {
            nodesep: Some(1000.0),
            ..graph.graph().unwrap().clone()
        });
        graph.set_node_with("a", node(50.0, 100.0, 0.0, 0));
        graph.set_node_with("b", node(70.0, 80.0, 0.0, 1));
        super::position(&mut graph);
        close(
            graph.node("b").unwrap().x.unwrap(),
            graph.node("a").unwrap().x.unwrap() + 25.0 + 1000.0 + 35.0,
        );
    }

    #[test]
    fn does_not_position_the_subgraph_node_itself() {
        let mut graph = setup();
        graph.set_node_with("a", node(50.0, 50.0, 0.0, 0));
        graph.set_node_with("sg1", NodeLabel::default());
        graph.set_parent("a", Some("sg1")).unwrap();
        super::position(&mut graph);
        assert_eq!(graph.node("sg1").unwrap().x, None);
        assert_eq!(graph.node("sg1").unwrap().y, None);
    }

    #[test]
    fn leaves_compound_nodes_untouched() {
        let mut graph = setup();
        graph.set_node_with("a", sized(50.0, 50.0));
        graph.set_node_with("sg", NodeLabel::default());
        graph.set_parent("a", Some("sg")).unwrap();
        super::position(&mut graph);
        assert_eq!(graph.node("sg").unwrap().x, None);
    }
}
