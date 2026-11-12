//! Port of `dagre-d3-es/src/dagre/coordinate-system.js` (MIT, Copyright (c) 2012-2014
//! Chris Pettitt).
//!
//! Layout always works top to bottom; this module rotates the graph in and out of
//! that frame for `LR` and `RL`, and flips it for `BT`. Upstream calls
//! `rankdir.toLowerCase()` unconditionally, so a graph without a direction throws
//! there; the layout graph always carries one (`tb` by default), and this port treats
//! an absent direction as top to bottom instead of failing.

use super::{EdgeLabel, LayeredGraph, NodeLabel};

/// Rotates the graph into layout coordinates.
pub fn adjust(graph: &mut LayeredGraph) {
    let rank_dir = graph
        .graph()
        .and_then(|graph| graph.rankdir.as_deref())
        .map(str::to_lowercase);
    if matches!(rank_dir.as_deref(), Some("lr" | "rl")) {
        swap_width_height(graph);
    }
}

/// Rotates the graph back into its own coordinates.
pub fn undo(graph: &mut LayeredGraph) {
    let rank_dir = graph
        .graph()
        .and_then(|graph| graph.rankdir.as_deref())
        .map(str::to_lowercase);
    if matches!(rank_dir.as_deref(), Some("bt" | "rl")) {
        reverse_y(graph);
    }
    if matches!(rank_dir.as_deref(), Some("lr" | "rl")) {
        swap_xy(graph);
        swap_width_height(graph);
    }
}

fn swap_width_height(graph: &mut LayeredGraph) {
    for v in graph.nodes() {
        if let Some(node) = graph.node_mut(&v) {
            swap_node_dimensions(node);
        }
    }
    for edge in graph.edges() {
        if let Some(label) = graph.edge_object_mut(&edge) {
            swap_edge_dimensions(label);
        }
    }
}

fn swap_node_dimensions(node: &mut NodeLabel) {
    std::mem::swap(&mut node.width, &mut node.height);
}

fn swap_edge_dimensions(label: &mut EdgeLabel) {
    std::mem::swap(&mut label.width, &mut label.height);
}

fn reverse_y(graph: &mut LayeredGraph) {
    for v in graph.nodes() {
        if let Some(node) = graph.node_mut(&v)
            && let Some(y) = node.y
        {
            node.y = Some(-y);
        }
    }
    for edge in graph.edges() {
        if let Some(label) = graph.edge_object_mut(&edge) {
            if let Some(points) = &mut label.points {
                for point in points {
                    point.y = -point.y;
                }
            }
            if let Some(y) = label.y {
                label.y = Some(-y);
            }
        }
    }
}

fn swap_xy(graph: &mut LayeredGraph) {
    for v in graph.nodes() {
        if let Some(node) = graph.node_mut(&v) {
            std::mem::swap(&mut node.x, &mut node.y);
        }
    }
    for edge in graph.edges() {
        if let Some(label) = graph.edge_object_mut(&edge) {
            if let Some(points) = &mut label.points {
                for point in points {
                    std::mem::swap(&mut point.x, &mut point.y);
                }
            }
            if label.x.is_some() {
                std::mem::swap(&mut label.x, &mut label.y);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::graph;
    use super::super::{GraphLabel, LayeredGraph, NodeLabel};

    fn graph_with(rankdir: &str, node: NodeLabel) -> LayeredGraph {
        let mut graph = graph();
        graph.set_graph(GraphLabel {
            rankdir: Some(rankdir.to_string()),
            ..GraphLabel::default()
        });
        graph.set_node_with("a", node);
        graph
    }

    #[test]
    fn adjust_keeps_dimensions_for_tb() {
        let mut graph = graph_with(
            "TB",
            NodeLabel {
                width: 100.0,
                height: 200.0,
                ..NodeLabel::default()
            },
        );
        super::adjust(&mut graph);
        assert_eq!(graph.node("a").unwrap().width, 100.0);
        assert_eq!(graph.node("a").unwrap().height, 200.0);
    }

    #[test]
    fn adjust_keeps_dimensions_for_bt() {
        let mut graph = graph_with(
            "BT",
            NodeLabel {
                width: 100.0,
                height: 200.0,
                ..NodeLabel::default()
            },
        );
        super::adjust(&mut graph);
        assert_eq!(graph.node("a").unwrap().width, 100.0);
        assert_eq!(graph.node("a").unwrap().height, 200.0);
    }

    #[test]
    fn adjust_swaps_dimensions_for_lr() {
        let mut graph = graph_with(
            "LR",
            NodeLabel {
                width: 100.0,
                height: 200.0,
                ..NodeLabel::default()
            },
        );
        super::adjust(&mut graph);
        assert_eq!(graph.node("a").unwrap().width, 200.0);
        assert_eq!(graph.node("a").unwrap().height, 100.0);
    }

    #[test]
    fn adjust_swaps_dimensions_for_rl() {
        let mut graph = graph_with(
            "RL",
            NodeLabel {
                width: 100.0,
                height: 200.0,
                ..NodeLabel::default()
            },
        );
        super::adjust(&mut graph);
        assert_eq!(graph.node("a").unwrap().width, 200.0);
        assert_eq!(graph.node("a").unwrap().height, 100.0);
    }

    fn positioned(rankdir: &str) -> LayeredGraph {
        graph_with(
            rankdir,
            NodeLabel {
                width: 100.0,
                height: 200.0,
                x: Some(20.0),
                y: Some(40.0),
                ..NodeLabel::default()
            },
        )
    }

    #[test]
    fn undo_keeps_points_for_tb() {
        let mut graph = positioned("TB");
        super::undo(&mut graph);
        let node = graph.node("a").unwrap();
        assert_eq!((node.x, node.y), (Some(20.0), Some(40.0)));
        assert_eq!((node.width, node.height), (100.0, 200.0));
    }

    #[test]
    fn undo_flips_y_for_bt() {
        let mut graph = positioned("BT");
        super::undo(&mut graph);
        let node = graph.node("a").unwrap();
        assert_eq!((node.x, node.y), (Some(20.0), Some(-40.0)));
    }

    #[test]
    fn undo_swaps_dimensions_and_coordinates_for_lr() {
        let mut graph = positioned("LR");
        super::undo(&mut graph);
        let node = graph.node("a").unwrap();
        assert_eq!((node.x, node.y), (Some(40.0), Some(20.0)));
        assert_eq!((node.width, node.height), (200.0, 100.0));
    }

    #[test]
    fn undo_swaps_and_flips_for_rl() {
        let mut graph = positioned("RL");
        super::undo(&mut graph);
        let node = graph.node("a").unwrap();
        assert_eq!((node.x, node.y), (Some(-40.0), Some(20.0)));
        assert_eq!((node.width, node.height), (200.0, 100.0));
    }
}
