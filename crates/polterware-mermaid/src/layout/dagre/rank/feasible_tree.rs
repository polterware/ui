//! Port of `dagre-d3-es/src/dagre/rank/feasible-tree.js` (MIT, Copyright (c) 2012-2014 Chris
//! Pettitt).
//!
//! Builds a spanning tree of tight edges (an edge is tight when its length equals
//! its `minlen`) and shifts ranks until every node is in the tree.

use super::super::super::graphlib::{Edge, Graph, GraphOptions};
use super::super::{LayeredGraph, LayoutError};
use super::util::slack;

/// A node of the feasible tree: its postorder interval and its parent.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TreeNode {
    /// First postorder number below this node.
    pub low: Option<usize>,
    /// Last postorder number below this node.
    pub lim: Option<usize>,
    /// The node one step closer to the root.
    pub parent: Option<String>,
}

/// An edge of the feasible tree: its cut value.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TreeEdge {
    /// Net weight of the cut this edge represents.
    pub cutvalue: Option<f64>,
}

/// The undirected tree [`feasible_tree`] returns.
pub type FeasibleTree = Graph<(), TreeNode, TreeEdge>;

/// Builds a feasible tree and adjusts `graph`'s ranks so all its edges are tight.
pub fn feasible_tree(graph: &mut LayeredGraph) -> Result<FeasibleTree, LayoutError> {
    let mut tree = Graph::with_options(GraphOptions::undirected());
    let nodes = graph.nodes();
    let Some(start) = nodes.first() else {
        return Err(LayoutError::GraphMustHaveAtLeastOneNode);
    };
    let size = graph.node_count();
    tree.set_node_with(start.clone(), TreeNode::default());

    while tight_tree(&mut tree, graph) < size {
        let Some(edge) = find_min_slack_edge(&tree, graph) else {
            break;
        };
        let delta = if tree.has_node(&edge.v) {
            slack(graph, &edge)
        } else {
            -slack(graph, &edge)
        };
        shift_ranks(&tree, graph, delta);
    }

    Ok(tree)
}

/// Finds a maximal tree of tight edges and returns its node count.
fn tight_tree(tree: &mut FeasibleTree, graph: &LayeredGraph) -> usize {
    fn dfs(tree: &mut FeasibleTree, graph: &LayeredGraph, v: &str) {
        let node_edges = graph.node_edges(v, None).unwrap_or_default();
        for edge in node_edges {
            let w = if v == edge.v { &edge.w } else { &edge.v };
            if !tree.has_node(w) && slack(graph, &edge) == 0.0 {
                tree.set_node_with(w.clone(), TreeNode::default());
                tree.set_edge_with(v, w, TreeEdge::default());
                dfs(tree, graph, w);
            }
        }
    }

    let nodes = tree.nodes();
    for v in nodes {
        dfs(tree, graph, &v);
    }
    tree.node_count()
}

/// Finds the tightest edge with exactly one endpoint in the tree.
fn find_min_slack_edge(tree: &FeasibleTree, graph: &LayeredGraph) -> Option<Edge> {
    graph
        .edges()
        .into_iter()
        .fold((f64::INFINITY, None), |(min_slack, min_edge), edge| {
            let mut edge_slack = f64::INFINITY;
            if tree.has_node(&edge.v) != tree.has_node(&edge.w) {
                edge_slack = slack(graph, &edge);
            }
            if edge_slack < min_slack {
                (edge_slack, Some(edge))
            } else {
                (min_slack, min_edge)
            }
        })
        .1
}

fn shift_ranks(tree: &FeasibleTree, graph: &mut LayeredGraph, delta: f64) {
    for v in tree.nodes() {
        if let Some(node) = graph.node_mut(&v)
            && let Some(rank) = node.rank
        {
            node.rank = Some(rank + delta);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::NodeLabel;
    use super::super::super::test_support::{graph, weighted};

    fn ranked_node(rank: f64) -> NodeLabel {
        NodeLabel {
            rank: Some(rank),
            ..NodeLabel::default()
        }
    }

    #[test]
    fn rejects_an_empty_graph() {
        let mut graph = graph();
        assert!(matches!(
            super::feasible_tree(&mut graph),
            Err(super::super::super::LayoutError::GraphMustHaveAtLeastOneNode)
        ));
    }

    #[test]
    fn creates_a_tree_for_a_trivial_graph() {
        let mut graph = graph();
        graph.set_node_with("a", ranked_node(0.0));
        graph.set_node_with("b", ranked_node(1.0));
        graph.set_edge_with("a", "b", weighted(1.0, 1.0));
        let tree = super::feasible_tree(&mut graph).unwrap();
        assert_eq!(
            graph.node("b").unwrap().rank,
            Some(graph.node("a").unwrap().rank.unwrap() + 1.0)
        );
        assert_eq!(tree.neighbors("a").unwrap(), vec!["b".to_string()]);
    }

    #[test]
    fn shortens_slack_by_pulling_a_node_up() {
        let mut graph = graph();
        graph.set_node_with("a", ranked_node(0.0));
        graph.set_node_with("b", ranked_node(1.0));
        graph.set_node_with("c", ranked_node(2.0));
        graph.set_node_with("d", ranked_node(2.0));
        graph.set_path_with(&["a", "b", "c"], weighted(1.0, 1.0));
        graph.set_edge_with("a", "d", weighted(1.0, 1.0));
        let tree = super::feasible_tree(&mut graph).unwrap();
        assert_eq!(
            graph.node("b").unwrap().rank,
            graph.node("a").unwrap().rank.map(|rank| rank + 1.0)
        );
        assert_eq!(
            graph.node("c").unwrap().rank,
            graph.node("b").unwrap().rank.map(|rank| rank + 1.0)
        );
        assert_eq!(
            graph.node("d").unwrap().rank,
            graph.node("a").unwrap().rank.map(|rank| rank + 1.0)
        );
        let mut a_neighbors = tree.neighbors("a").unwrap();
        a_neighbors.sort();
        assert_eq!(a_neighbors, vec!["b".to_string(), "d".to_string()]);
        let mut b_neighbors = tree.neighbors("b").unwrap();
        b_neighbors.sort();
        assert_eq!(b_neighbors, vec!["a".to_string(), "c".to_string()]);
        assert_eq!(tree.neighbors("c").unwrap(), vec!["b".to_string()]);
        assert_eq!(tree.neighbors("d").unwrap(), vec!["a".to_string()]);
    }

    #[test]
    fn shortens_slack_by_pulling_a_node_down() {
        let mut graph = graph();
        graph.set_node_with("a", ranked_node(2.0));
        graph.set_node_with("b", ranked_node(0.0));
        graph.set_node_with("c", ranked_node(2.0));
        graph.set_edge_with("b", "a", weighted(1.0, 1.0));
        graph.set_edge_with("b", "c", weighted(1.0, 1.0));
        let tree = super::feasible_tree(&mut graph).unwrap();
        assert_eq!(
            graph.node("a").unwrap().rank,
            graph.node("b").unwrap().rank.map(|rank| rank + 1.0)
        );
        assert_eq!(
            graph.node("c").unwrap().rank,
            graph.node("b").unwrap().rank.map(|rank| rank + 1.0)
        );
        assert_eq!(tree.neighbors("a").unwrap(), vec!["b".to_string()]);
        let mut b_neighbors = tree.neighbors("b").unwrap();
        b_neighbors.sort();
        assert_eq!(b_neighbors, vec!["a".to_string(), "c".to_string()]);
        assert_eq!(tree.neighbors("c").unwrap(), vec!["b".to_string()]);
    }
}
