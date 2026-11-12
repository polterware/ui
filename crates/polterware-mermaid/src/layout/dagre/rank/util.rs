//! Port of `dagre-d3-es/src/dagre/rank/util.js` (MIT, Copyright (c) 2012-2014 Chris Pettitt).

use std::collections::HashSet;

use super::super::super::graphlib::Edge;
use super::super::{EdgeLabel, LayeredGraph};

/// Assigns ranks by longest path, leaving the bottom ranks wide.
pub fn longest_path(graph: &mut LayeredGraph) {
    fn dfs(graph: &mut LayeredGraph, v: &str, visited: &mut HashSet<String>) -> f64 {
        let label = graph.node(v).cloned().unwrap_or_default();
        if !visited.insert(v.to_string()) {
            return label.rank.unwrap_or(0.0);
        }

        let mut rank = f64::INFINITY;
        for edge in graph.out_edges(v, None).unwrap_or_default() {
            let minlen = graph.edge_object(&edge).map_or(0.0, EdgeLabel::minlen);
            rank = rank.min(dfs(graph, &edge.w, visited) - minlen);
        }
        if rank == f64::INFINITY {
            rank = 0.0;
        }

        if let Some(node) = graph.node_mut(v) {
            node.rank = Some(rank);
        }
        rank
    }

    let mut visited = HashSet::new();
    for source in graph.sources() {
        dfs(graph, &source, &mut visited);
    }
}

/// The difference between an edge's length and its minimum length.
pub fn slack(graph: &LayeredGraph, edge: &Edge) -> f64 {
    let w_rank = graph
        .node(&edge.w)
        .and_then(|node| node.rank)
        .unwrap_or(0.0);
    let v_rank = graph
        .node(&edge.v)
        .and_then(|node| node.rank)
        .unwrap_or(0.0);
    w_rank - v_rank - graph.edge_object(edge).map_or(0.0, EdgeLabel::minlen)
}

/// Copies the ranks of `from` into `to` for every node both have.
pub fn copy_ranks(from: &LayeredGraph, to: &mut LayeredGraph) {
    for v in to.nodes() {
        if let Some(rank) = from.node(&v).and_then(|node| node.rank)
            && let Some(node) = to.node_mut(&v)
        {
            node.rank = Some(rank);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::test_support::{graph, weighted};
    use super::super::super::util::normalize_ranks;
    use super::{longest_path, slack};
    use crate::layout::graphlib::Edge;

    #[test]
    fn longest_path_ranks_a_single_node() {
        let mut graph = graph();
        graph.set_node("a");
        longest_path(&mut graph);
        normalize_ranks(&mut graph);
        assert_eq!(graph.node("a").unwrap().rank, Some(0.0));
    }

    #[test]
    fn longest_path_ranks_unconnected_nodes() {
        let mut graph = graph();
        graph.set_node("a");
        graph.set_node("b");
        longest_path(&mut graph);
        normalize_ranks(&mut graph);
        assert_eq!(graph.node("a").unwrap().rank, Some(0.0));
        assert_eq!(graph.node("b").unwrap().rank, Some(0.0));
    }

    #[test]
    fn longest_path_ranks_connected_nodes() {
        let mut graph = graph();
        graph.set_edge_with("a", "b", weighted(1.0, 1.0));
        longest_path(&mut graph);
        normalize_ranks(&mut graph);
        assert_eq!(graph.node("a").unwrap().rank, Some(0.0));
        assert_eq!(graph.node("b").unwrap().rank, Some(1.0));
    }

    #[test]
    fn longest_path_ranks_a_diamond() {
        let mut graph = graph();
        graph.set_path_with(&["a", "b", "d"], weighted(1.0, 1.0));
        graph.set_path_with(&["a", "c", "d"], weighted(1.0, 1.0));
        longest_path(&mut graph);
        normalize_ranks(&mut graph);
        assert_eq!(graph.node("a").unwrap().rank, Some(0.0));
        assert_eq!(graph.node("b").unwrap().rank, Some(1.0));
        assert_eq!(graph.node("c").unwrap().rank, Some(1.0));
        assert_eq!(graph.node("d").unwrap().rank, Some(2.0));
    }

    #[test]
    fn longest_path_uses_the_minlen_attribute() {
        let mut graph = graph();
        graph.set_path_with(&["a", "b", "d"], weighted(1.0, 1.0));
        graph.set_edge_with("a", "c", weighted(1.0, 1.0));
        graph.set_edge_with("c", "d", weighted(1.0, 2.0));
        longest_path(&mut graph);
        normalize_ranks(&mut graph);
        assert_eq!(graph.node("a").unwrap().rank, Some(0.0));
        assert_eq!(graph.node("b").unwrap().rank, Some(2.0));
        assert_eq!(graph.node("c").unwrap().rank, Some(1.0));
        assert_eq!(graph.node("d").unwrap().rank, Some(3.0));
    }

    #[test]
    fn slack_is_the_difference_from_minlen() {
        let mut graph = graph();
        graph.set_node_with(
            "a",
            super::super::super::NodeLabel {
                rank: Some(0.0),
                ..Default::default()
            },
        );
        graph.set_node_with(
            "b",
            super::super::super::NodeLabel {
                rank: Some(3.0),
                ..Default::default()
            },
        );
        graph.set_edge_with("a", "b", weighted(1.0, 1.0));
        assert_eq!(slack(&graph, &Edge::new("a", "b")), 2.0);
    }
}
