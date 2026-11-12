//! Port of `dagre-d3-es/src/dagre/rank/` (MIT, Copyright (c) 2012-2014 Chris Pettitt).
//!
//! Assigns a rank to every node. The default `network-simplex` ranker comes from
//! Gansner, et al., "A Technique for Drawing Directed Graphs." There is no `none`
//! ranker: dagre-d3-es falls through to `network-simplex` for unknown names.

pub mod feasible_tree;
pub mod network_simplex;
pub mod util;

use super::{LayeredGraph, LayoutError};

/// Ranks `graph` with the ranker named by its graph label.
pub fn rank(graph: &mut LayeredGraph) -> Result<(), LayoutError> {
    let ranker = graph.graph().and_then(|label| label.ranker.clone());
    match ranker.as_deref() {
        Some("network-simplex") => network_simplex::network_simplex(graph),
        Some("tight-tree") => {
            util::longest_path(graph);
            feasible_tree::feasible_tree(graph)?;
            Ok(())
        }
        Some("longest-path") => {
            util::longest_path(graph);
            Ok(())
        }
        _ => network_simplex::network_simplex(graph),
    }
}

#[cfg(test)]
mod tests {
    use super::super::GraphLabel;
    use super::super::LayeredGraph;
    use super::super::test_support::{graph, weighted};
    use super::super::util::normalize_ranks;

    fn ranker_graph(ranker: &str) -> LayeredGraph {
        let mut graph = graph();
        graph.set_graph(GraphLabel {
            ranker: Some(ranker.to_string()),
            ..GraphLabel::default()
        });
        let edge = weighted(1.0, 1.0);
        graph.set_path_with(&["a", "b", "c", "d", "h"], edge.clone());
        graph.set_path_with(&["a", "e", "g", "h"], edge.clone());
        graph.set_path_with(&["a", "f", "g"], edge);
        graph
    }

    #[test]
    fn respects_the_minlen_attribute() {
        for ranker in [
            "longest-path",
            "tight-tree",
            "network-simplex",
            "unknown-should-still-work",
        ] {
            let mut graph = ranker_graph(ranker);
            super::rank(&mut graph).unwrap();
            for edge in graph.edges() {
                let v_rank = graph.node(&edge.v).unwrap().rank.unwrap();
                let w_rank = graph.node(&edge.w).unwrap().rank.unwrap();
                let minlen = graph.edge_object(&edge).unwrap().minlen();
                assert!(w_rank - v_rank >= minlen, "{ranker}: {edge:?}");
            }
        }
    }

    #[test]
    fn can_rank_a_single_node_graph() {
        for ranker in [
            "longest-path",
            "tight-tree",
            "network-simplex",
            "unknown-should-still-work",
        ] {
            let mut graph = graph();
            graph.set_graph(GraphLabel {
                ranker: Some(ranker.to_string()),
                ..GraphLabel::default()
            });
            graph.set_node("a");
            super::rank(&mut graph).unwrap();
            normalize_ranks(&mut graph);
            assert_eq!(graph.node("a").unwrap().rank, Some(0.0));
        }
    }
}
