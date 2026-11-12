//! Port of `dagre-d3-es/src/dagre/rank/network-simplex.js` (MIT, Copyright (c) 2012-2014 Chris
//! Pettitt).
//!
//! Starts from longest-path ranks, builds a feasible tight tree, and exchanges tree
//! edges while any cut value is negative, following Gansner, et al.

use super::super::super::graphlib::Edge;
use super::super::super::graphlib::alg;
use super::super::util::simplify;
use super::super::{EdgeLabel, LayeredGraph, LayoutError};
use super::feasible_tree::{FeasibleTree, TreeNode, feasible_tree};
use super::util::{copy_ranks, longest_path, slack};

/// Assigns optimized ranks to `graph`.
pub fn network_simplex(graph: &mut LayeredGraph) -> Result<(), LayoutError> {
    let mut simplified = simplify(graph);
    longest_path(&mut simplified);

    let mut tree = feasible_tree(&mut simplified)?;
    init_low_lim_values(&mut tree, None);
    init_cut_values(&mut tree, &simplified);

    while let Some(edge) = leave_edge(&tree) {
        let new_edge = enter_edge(&tree, &simplified, &edge);
        exchange_edges(&mut tree, &mut simplified, &edge, &new_edge);
    }

    copy_ranks(&simplified, graph);
    Ok(())
}

/// Initializes the low and lim postorder numbers of `tree`.
pub fn init_low_lim_values(tree: &mut FeasibleTree, root: Option<&str>) {
    let root = root
        .map(str::to_string)
        .or_else(|| tree.nodes().first().cloned());
    if let Some(root) = root {
        let mut visited = std::collections::HashSet::new();
        dfs_assign_low_lim(tree, &mut visited, 1, &root, None);
    }
}

fn dfs_assign_low_lim(
    tree: &mut FeasibleTree,
    visited: &mut std::collections::HashSet<String>,
    next_lim: usize,
    v: &str,
    parent: Option<&str>,
) -> usize {
    let low = next_lim;
    visited.insert(v.to_string());
    let mut next_lim = next_lim;
    for w in tree.neighbors(v).unwrap_or_default() {
        if !visited.contains(&w) {
            next_lim = dfs_assign_low_lim(tree, visited, next_lim, &w, Some(v));
        }
    }
    if let Some(node) = tree.node_mut(v) {
        node.low = Some(low);
        node.lim = Some(next_lim);
        node.parent = parent.map(str::to_string);
    }
    next_lim + 1
}

/// Initializes the cut value of every tree edge.
pub fn init_cut_values(tree: &mut FeasibleTree, graph: &LayeredGraph) {
    let roots = tree.nodes();
    let root_refs: Vec<&str> = roots.iter().map(String::as_str).collect();
    let mut visited = alg::postorder(tree, &root_refs).unwrap_or_default();
    if !visited.is_empty() {
        visited.truncate(visited.len() - 1);
    }
    for v in visited {
        assign_cut_value(tree, graph, &v);
    }
}

fn assign_cut_value(tree: &mut FeasibleTree, graph: &LayeredGraph, child: &str) {
    let parent = tree
        .node(child)
        .and_then(|node| node.parent.clone())
        .unwrap_or_default();
    let cutvalue = calc_cut_value(tree, graph, child);
    if let Some(edge) = tree.edge_mut(child, &parent) {
        edge.cutvalue = Some(cutvalue);
    }
}

/// The cut value of the tree edge between `child` and its parent.
pub fn calc_cut_value(tree: &FeasibleTree, graph: &LayeredGraph, child: &str) -> f64 {
    let parent = tree
        .node(child)
        .and_then(|node| node.parent.clone())
        .unwrap_or_default();
    let mut child_is_tail = true;
    let mut graph_edge = graph.edge(child, &parent).cloned();
    if graph_edge.is_none() {
        child_is_tail = false;
        graph_edge = graph.edge(&parent, child).cloned();
    }
    let mut cut_value = graph_edge.as_ref().map_or(0.0, |edge| edge.weight());

    for edge in graph.node_edges(child, None).unwrap_or_default() {
        let is_out_edge = edge.v == child;
        let other = if is_out_edge { &edge.w } else { &edge.v };
        if other != &parent {
            let points_to_head = is_out_edge == child_is_tail;
            let other_weight = graph.edge_object(&edge).map_or(0.0, EdgeLabel::weight);
            cut_value += if points_to_head {
                other_weight
            } else {
                -other_weight
            };
            if tree.has_edge(child, other) {
                let other_cut_value = tree
                    .edge(child, other)
                    .and_then(|edge| edge.cutvalue)
                    .unwrap_or(0.0);
                cut_value += if points_to_head {
                    -other_cut_value
                } else {
                    other_cut_value
                };
            }
        }
    }

    cut_value
}

/// The first tree edge with a negative cut value.
pub fn leave_edge(tree: &FeasibleTree) -> Option<Edge> {
    tree.edges().into_iter().find(|edge| {
        tree.edge_object(edge)
            .and_then(|edge| edge.cutvalue)
            .unwrap_or(0.0)
            < 0.0
    })
}

/// The graph edge that replaces `edge` when it leaves the tree.
pub fn enter_edge(tree: &FeasibleTree, graph: &LayeredGraph, edge: &Edge) -> Edge {
    let mut v = edge.v.clone();
    let mut w = edge.w.clone();
    if !graph.has_edge(&v, &w) {
        std::mem::swap(&mut v, &mut w);
    }

    let v_label = tree.node(&v).cloned().unwrap_or_default();
    let w_label = tree.node(&w).cloned().unwrap_or_default();
    let mut tail_label = v_label;
    let mut flip = false;
    if tail_label.lim.unwrap_or(0) > w_label.lim.unwrap_or(0) {
        tail_label = w_label;
        flip = true;
    }

    let candidates: Vec<Edge> = graph
        .edges()
        .into_iter()
        .filter(|edge| {
            let source = tree.node(&edge.v).cloned().unwrap_or_default();
            let target = tree.node(&edge.w).cloned().unwrap_or_default();
            flip == is_descendant(&source, &tail_label)
                && flip != is_descendant(&target, &tail_label)
        })
        .collect();

    let mut candidates = candidates.into_iter();
    let first = candidates
        .next()
        .expect("entering an edge always has a candidate");
    candidates.fold(first, |accumulator, edge| {
        if slack(graph, &edge) < slack(graph, &accumulator) {
            edge
        } else {
            accumulator
        }
    })
}

fn is_descendant(node: &TreeNode, root: &TreeNode) -> bool {
    let low = root.low.unwrap_or(0);
    let limit = root.lim.unwrap_or(0);
    let node_lim = node.lim.unwrap_or(0);
    low <= node_lim && node_lim <= limit
}

/// Replaces tree edge `e` with graph edge `f` and updates ranks.
pub fn exchange_edges(tree: &mut FeasibleTree, graph: &mut LayeredGraph, e: &Edge, f: &Edge) {
    tree.remove_edge(&e.v, &e.w);
    tree.set_edge_with(&f.v, &f.w, super::feasible_tree::TreeEdge::default());
    init_low_lim_values(tree, None);
    init_cut_values(tree, graph);
    update_ranks(tree, graph);
}

fn update_ranks(tree: &FeasibleTree, graph: &mut LayeredGraph) {
    let Some(root) = tree
        .nodes()
        .into_iter()
        .find(|v| tree.node(v).is_some_and(|node| node.parent.is_none()))
    else {
        return;
    };
    let root_refs = [root.as_str()];
    let mut vs = alg::preorder(tree, &root_refs).unwrap_or_default();
    if !vs.is_empty() {
        vs.remove(0);
    }
    for v in vs {
        let Some(parent) = tree.node(&v).and_then(|node| node.parent.clone()) else {
            continue;
        };
        let mut edge = graph.edge(&v, &parent).cloned();
        let mut flipped = false;
        if edge.is_none() {
            edge = graph.edge(&parent, &v).cloned();
            flipped = true;
        }
        let parent_rank = graph
            .node(&parent)
            .and_then(|node| node.rank)
            .unwrap_or(0.0);
        let minlen = edge.as_ref().map_or(0.0, EdgeLabel::minlen);
        if let Some(node) = graph.node_mut(&v) {
            node.rank = Some(parent_rank + if flipped { minlen } else { -minlen });
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::layout::dagre::test_support::{graph, multigraph, weighted};
    use crate::layout::dagre::util::normalize_ranks;
    use crate::layout::dagre::{LayeredGraph, NodeLabel};
    use crate::layout::graphlib::{Edge, Graph, GraphOptions};

    use super::super::feasible_tree::{FeasibleTree, TreeEdge, TreeNode};
    use super::super::util::longest_path;

    fn tree() -> FeasibleTree {
        let mut tree = Graph::with_options(GraphOptions::undirected());
        tree.set_default_node_label(TreeNode::default());
        tree.set_default_edge_label(TreeEdge::default());
        tree
    }

    fn gansner_graph() -> LayeredGraph {
        let mut graph = graph();
        let edge = weighted(1.0, 1.0);
        graph.set_path_with(&["a", "b", "c", "d", "h"], edge.clone());
        graph.set_path_with(&["a", "e", "g", "h"], edge.clone());
        graph.set_path_with(&["a", "f", "g"], edge);
        graph
    }

    fn gansner_tree() -> FeasibleTree {
        let mut tree = tree();
        tree.set_path(&["a", "b", "c", "d", "h", "g", "e"]);
        tree.set_edge("g", "f");
        tree
    }

    fn ns(graph: &mut LayeredGraph) {
        super::network_simplex(graph).unwrap();
        normalize_ranks(graph);
    }

    fn rank(graph: &LayeredGraph, v: &str) -> f64 {
        graph.node(v).unwrap().rank.unwrap()
    }

    fn undirected_edge(edge: &Edge) -> (String, String) {
        if edge.v < edge.w {
            (edge.v.clone(), edge.w.clone())
        } else {
            (edge.w.clone(), edge.v.clone())
        }
    }

    #[test]
    fn can_assign_a_rank_to_a_single_node() {
        let mut graph = multigraph();
        graph.set_node("a");
        ns(&mut graph);
        assert_eq!(rank(&graph, "a"), 0.0);
    }

    #[test]
    fn can_assign_ranks_to_a_two_node_graph() {
        let mut graph = multigraph();
        graph.set_edge("a", "b");
        ns(&mut graph);
        assert_eq!(rank(&graph, "a"), 0.0);
        assert_eq!(rank(&graph, "b"), 1.0);
    }

    #[test]
    fn can_assign_ranks_for_a_diamond() {
        let mut graph = multigraph();
        graph.set_path(&["a", "b", "d"]);
        graph.set_path(&["a", "c", "d"]);
        ns(&mut graph);
        assert_eq!(rank(&graph, "a"), 0.0);
        assert_eq!(rank(&graph, "b"), 1.0);
        assert_eq!(rank(&graph, "c"), 1.0);
        assert_eq!(rank(&graph, "d"), 2.0);
    }

    #[test]
    fn uses_the_minlen_attribute() {
        let mut graph = multigraph();
        graph.set_path(&["a", "b", "d"]);
        graph.set_edge("a", "c");
        graph.set_edge_with("c", "d", weighted(1.0, 2.0));
        ns(&mut graph);
        assert_eq!(rank(&graph, "a"), 0.0);
        assert_eq!(rank(&graph, "b"), 2.0);
        assert_eq!(rank(&graph, "c"), 1.0);
        assert_eq!(rank(&graph, "d"), 3.0);
    }

    #[test]
    fn can_rank_the_gansner_graph() {
        let mut graph = gansner_graph();
        ns(&mut graph);
        assert_eq!(rank(&graph, "a"), 0.0);
        assert_eq!(rank(&graph, "b"), 1.0);
        assert_eq!(rank(&graph, "c"), 2.0);
        assert_eq!(rank(&graph, "d"), 3.0);
        assert_eq!(rank(&graph, "h"), 4.0);
        assert_eq!(rank(&graph, "e"), 1.0);
        assert_eq!(rank(&graph, "f"), 1.0);
        assert_eq!(rank(&graph, "g"), 2.0);
    }

    #[test]
    fn can_handle_multi_edges() {
        let mut graph = multigraph();
        graph.set_path(&["a", "b", "c", "d"]);
        graph.set_edge_with("a", "e", weighted(2.0, 1.0));
        graph.set_edge("e", "d");
        graph
            .set_edge_full("b", "c", Some(weighted(1.0, 2.0)), Some("multi"))
            .unwrap();
        ns(&mut graph);
        assert_eq!(rank(&graph, "a"), 0.0);
        assert_eq!(rank(&graph, "b"), 1.0);
        assert_eq!(rank(&graph, "c"), 3.0);
        assert_eq!(rank(&graph, "d"), 4.0);
        assert_eq!(rank(&graph, "e"), 1.0);
    }

    #[test]
    fn leave_edge_returns_none_without_a_negative_cutvalue() {
        let mut tree = tree();
        tree.set_edge_with(
            "a",
            "b",
            TreeEdge {
                cutvalue: Some(1.0),
            },
        );
        tree.set_edge_with(
            "b",
            "c",
            TreeEdge {
                cutvalue: Some(1.0),
            },
        );
        assert_eq!(super::leave_edge(&tree), None);
    }

    #[test]
    fn leave_edge_returns_a_negative_edge() {
        let mut tree = tree();
        tree.set_edge_with(
            "a",
            "b",
            TreeEdge {
                cutvalue: Some(1.0),
            },
        );
        tree.set_edge_with(
            "b",
            "c",
            TreeEdge {
                cutvalue: Some(-1.0),
            },
        );
        assert_eq!(super::leave_edge(&tree), Some(Edge::new("b", "c")));
    }

    #[test]
    fn enter_edge_finds_an_edge_from_head_to_tail() {
        let mut graph = multigraph();
        graph.set_node_with(
            "a",
            NodeLabel {
                rank: Some(0.0),
                ..NodeLabel::default()
            },
        );
        graph.set_node_with(
            "b",
            NodeLabel {
                rank: Some(2.0),
                ..NodeLabel::default()
            },
        );
        graph.set_node_with(
            "c",
            NodeLabel {
                rank: Some(3.0),
                ..NodeLabel::default()
            },
        );
        graph.set_path(&["a", "b", "c"]);
        graph.set_edge("a", "c");
        let mut tree = tree();
        tree.set_path(&["b", "c", "a"]);
        super::init_low_lim_values(&mut tree, Some("c"));
        let edge = super::enter_edge(&tree, &graph, &Edge::new("b", "c"));
        assert_eq!(
            undirected_edge(&edge),
            undirected_edge(&Edge::new("a", "b"))
        );
    }

    #[test]
    fn enter_edge_works_when_the_root_is_in_the_tail() {
        let mut graph = multigraph();
        graph.set_node_with(
            "a",
            NodeLabel {
                rank: Some(0.0),
                ..NodeLabel::default()
            },
        );
        graph.set_node_with(
            "b",
            NodeLabel {
                rank: Some(2.0),
                ..NodeLabel::default()
            },
        );
        graph.set_node_with(
            "c",
            NodeLabel {
                rank: Some(3.0),
                ..NodeLabel::default()
            },
        );
        graph.set_path(&["a", "b", "c"]);
        graph.set_edge("a", "c");
        let mut tree = tree();
        tree.set_path(&["b", "c", "a"]);
        super::init_low_lim_values(&mut tree, Some("b"));
        let edge = super::enter_edge(&tree, &graph, &Edge::new("b", "c"));
        assert_eq!(
            undirected_edge(&edge),
            undirected_edge(&Edge::new("a", "b"))
        );
    }

    #[test]
    fn enter_edge_finds_the_least_slack() {
        let mut graph = multigraph();
        graph.set_node_with(
            "a",
            NodeLabel {
                rank: Some(0.0),
                ..NodeLabel::default()
            },
        );
        graph.set_node_with(
            "b",
            NodeLabel {
                rank: Some(1.0),
                ..NodeLabel::default()
            },
        );
        graph.set_node_with(
            "c",
            NodeLabel {
                rank: Some(3.0),
                ..NodeLabel::default()
            },
        );
        graph.set_node_with(
            "d",
            NodeLabel {
                rank: Some(4.0),
                ..NodeLabel::default()
            },
        );
        graph.set_edge("a", "d");
        graph.set_path(&["a", "c", "d"]);
        graph.set_edge("b", "c");
        let mut tree = tree();
        tree.set_path(&["c", "d", "a", "b"]);
        super::init_low_lim_values(&mut tree, Some("a"));
        let edge = super::enter_edge(&tree, &graph, &Edge::new("c", "d"));
        assert_eq!(
            undirected_edge(&edge),
            undirected_edge(&Edge::new("b", "c"))
        );
    }

    #[test]
    fn enter_edge_finds_a_candidate_for_gansner_1() {
        let mut graph = gansner_graph();
        let mut tree = gansner_tree();
        longest_path(&mut graph);
        super::init_low_lim_values(&mut tree, Some("a"));
        let edge = super::enter_edge(&tree, &graph, &Edge::new("g", "h"));
        let undirected = undirected_edge(&edge);
        assert_eq!(undirected.0, "a");
        assert!(["e", "f"].contains(&undirected.1.as_str()));
    }

    #[test]
    fn enter_edge_finds_a_candidate_for_gansner_2() {
        let mut graph = gansner_graph();
        let mut tree = gansner_tree();
        longest_path(&mut graph);
        super::init_low_lim_values(&mut tree, Some("e"));
        let edge = super::enter_edge(&tree, &graph, &Edge::new("g", "h"));
        let undirected = undirected_edge(&edge);
        assert_eq!(undirected.0, "a");
        assert!(["e", "f"].contains(&undirected.1.as_str()));
    }

    #[test]
    fn enter_edge_finds_a_candidate_for_gansner_3() {
        let mut graph = gansner_graph();
        let mut tree = gansner_tree();
        longest_path(&mut graph);
        super::init_low_lim_values(&mut tree, Some("a"));
        let edge = super::enter_edge(&tree, &graph, &Edge::new("h", "g"));
        let undirected = undirected_edge(&edge);
        assert_eq!(undirected.0, "a");
        assert!(["e", "f"].contains(&undirected.1.as_str()));
    }

    #[test]
    fn enter_edge_finds_a_candidate_for_gansner_4() {
        let mut graph = gansner_graph();
        let mut tree = gansner_tree();
        longest_path(&mut graph);
        super::init_low_lim_values(&mut tree, Some("e"));
        let edge = super::enter_edge(&tree, &graph, &Edge::new("h", "g"));
        let undirected = undirected_edge(&edge);
        assert_eq!(undirected.0, "a");
        assert!(["e", "f"].contains(&undirected.1.as_str()));
    }

    #[test]
    fn init_low_lim_values_assigns_low_lim_and_parent() {
        let mut tree = tree();
        tree.set_nodes(&["a", "b", "c", "d", "e"]);
        tree.set_path(&["a", "b", "a", "c", "d", "c", "e"]);
        super::init_low_lim_values(&mut tree, Some("a"));
        let a = tree.node("a").unwrap().clone();
        let b = tree.node("b").unwrap().clone();
        let c = tree.node("c").unwrap().clone();
        let d = tree.node("d").unwrap().clone();
        let e = tree.node("e").unwrap().clone();
        let mut lims: Vec<usize> = tree
            .nodes()
            .iter()
            .map(|v| tree.node(v).unwrap().lim.unwrap())
            .collect();
        lims.sort();
        assert_eq!(lims, vec![1, 2, 3, 4, 5]);
        assert_eq!((a.low, a.lim), (Some(1), Some(5)));
        assert_eq!(b.parent.as_deref(), Some("a"));
        assert_eq!(c.parent.as_deref(), Some("a"));
        assert_ne!(b.lim, c.lim);
        assert_eq!(d.parent.as_deref(), Some("c"));
        assert_eq!(e.parent.as_deref(), Some("c"));
        assert_ne!(d.lim, e.lim);
    }

    #[test]
    fn exchange_edges_updates_cut_values_and_low_lim() {
        let mut graph = gansner_graph();
        let mut tree = gansner_tree();
        longest_path(&mut graph);
        super::init_low_lim_values(&mut tree, None);
        super::exchange_edges(
            &mut tree,
            &mut graph,
            &Edge::new("g", "h"),
            &Edge::new("a", "e"),
        );
        assert_eq!(tree.edge("a", "b").unwrap().cutvalue, Some(2.0));
        assert_eq!(tree.edge("b", "c").unwrap().cutvalue, Some(2.0));
        assert_eq!(tree.edge("c", "d").unwrap().cutvalue, Some(2.0));
        assert_eq!(tree.edge("d", "h").unwrap().cutvalue, Some(2.0));
        assert_eq!(tree.edge("a", "e").unwrap().cutvalue, Some(1.0));
        assert_eq!(tree.edge("e", "g").unwrap().cutvalue, Some(1.0));
        assert_eq!(tree.edge("g", "f").unwrap().cutvalue, Some(0.0));
        let mut lims: Vec<usize> = tree
            .nodes()
            .iter()
            .map(|v| tree.node(v).unwrap().lim.unwrap())
            .collect();
        lims.sort();
        assert_eq!(lims, vec![1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn exchange_edges_updates_ranks() {
        let mut graph = gansner_graph();
        let mut tree = gansner_tree();
        longest_path(&mut graph);
        super::init_low_lim_values(&mut tree, None);
        super::exchange_edges(
            &mut tree,
            &mut graph,
            &Edge::new("g", "h"),
            &Edge::new("a", "e"),
        );
        normalize_ranks(&mut graph);
        assert_eq!(rank(&graph, "a"), 0.0);
        assert_eq!(rank(&graph, "b"), 1.0);
        assert_eq!(rank(&graph, "c"), 2.0);
        assert_eq!(rank(&graph, "d"), 3.0);
        assert_eq!(rank(&graph, "e"), 1.0);
        assert_eq!(rank(&graph, "f"), 1.0);
        assert_eq!(rank(&graph, "g"), 2.0);
        assert_eq!(rank(&graph, "h"), 4.0);
    }

    fn cut_value(graph: &mut LayeredGraph, tree: &mut FeasibleTree, child: &str) -> f64 {
        super::init_low_lim_values(tree, Some("p"));
        super::calc_cut_value(tree, graph, child)
    }

    #[test]
    fn calc_cut_value_two_node_tree_with_child_to_parent() {
        let mut graph = multigraph();
        graph.set_path(&["c", "p"]);
        let mut tree = tree();
        tree.set_path(&["p", "c"]);
        assert_eq!(cut_value(&mut graph, &mut tree, "c"), 1.0);
    }

    #[test]
    fn calc_cut_value_two_node_tree_with_parent_to_child() {
        let mut graph = multigraph();
        graph.set_path(&["p", "c"]);
        let mut tree = tree();
        tree.set_path(&["p", "c"]);
        assert_eq!(cut_value(&mut graph, &mut tree, "c"), 1.0);
    }

    #[test]
    fn calc_cut_value_grandchild_child_parent() {
        let mut graph = multigraph();
        graph.set_path(&["gc", "c", "p"]);
        let mut tree = tree();
        tree.set_edge_with(
            "gc",
            "c",
            TreeEdge {
                cutvalue: Some(3.0),
            },
        );
        tree.set_edge("p", "c");
        assert_eq!(cut_value(&mut graph, &mut tree, "c"), 3.0);
    }

    #[test]
    fn calc_cut_value_grandchild_to_child_from_parent() {
        let mut graph = multigraph();
        graph.set_edge("p", "c");
        graph.set_edge("gc", "c");
        let mut tree = tree();
        tree.set_edge_with(
            "gc",
            "c",
            TreeEdge {
                cutvalue: Some(3.0),
            },
        );
        tree.set_edge("p", "c");
        assert_eq!(cut_value(&mut graph, &mut tree, "c"), -1.0);
    }

    #[test]
    fn calc_cut_value_from_child_to_grandchild_and_parent() {
        let mut graph = multigraph();
        graph.set_edge("c", "p");
        graph.set_edge("c", "gc");
        let mut tree = tree();
        tree.set_edge_with(
            "gc",
            "c",
            TreeEdge {
                cutvalue: Some(3.0),
            },
        );
        tree.set_edge("p", "c");
        assert_eq!(cut_value(&mut graph, &mut tree, "c"), -1.0);
    }

    #[test]
    fn calc_cut_value_from_parent_to_child_to_grandchild() {
        let mut graph = multigraph();
        graph.set_path(&["p", "c", "gc"]);
        let mut tree = tree();
        tree.set_edge_with(
            "gc",
            "c",
            TreeEdge {
                cutvalue: Some(3.0),
            },
        );
        tree.set_edge("p", "c");
        assert_eq!(cut_value(&mut graph, &mut tree, "c"), 3.0);
    }

    #[test]
    fn calc_cut_value_other_to_child() {
        let mut graph = multigraph();
        graph.set_edge_with("o", "c", weighted(7.0, 1.0));
        graph.set_path(&["gc", "c", "p", "o"]);
        let mut tree = tree();
        tree.set_edge_with(
            "gc",
            "c",
            TreeEdge {
                cutvalue: Some(3.0),
            },
        );
        tree.set_path(&["c", "p", "o"]);
        assert_eq!(cut_value(&mut graph, &mut tree, "c"), -4.0);
    }

    #[test]
    fn calc_cut_value_child_to_other() {
        let mut graph = multigraph();
        graph.set_edge_with("c", "o", weighted(7.0, 1.0));
        graph.set_path(&["gc", "c", "p", "o"]);
        let mut tree = tree();
        tree.set_edge_with(
            "gc",
            "c",
            TreeEdge {
                cutvalue: Some(3.0),
            },
        );
        tree.set_path(&["c", "p", "o"]);
        assert_eq!(cut_value(&mut graph, &mut tree, "c"), 10.0);
    }

    #[test]
    fn calc_cut_value_other_to_grandchild_to_child() {
        let mut graph = multigraph();
        graph.set_edge_with("o", "c", weighted(7.0, 1.0));
        graph.set_path(&["o", "gc", "c", "p"]);
        let mut tree = tree();
        tree.set_edge("o", "gc");
        tree.set_edge_with(
            "gc",
            "c",
            TreeEdge {
                cutvalue: Some(3.0),
            },
        );
        tree.set_edge("c", "p");
        assert_eq!(cut_value(&mut graph, &mut tree, "c"), -4.0);
    }

    #[test]
    fn calc_cut_value_child_to_other_through_grandchild() {
        let mut graph = multigraph();
        graph.set_edge_with("c", "o", weighted(7.0, 1.0));
        graph.set_path(&["o", "gc", "c", "p"]);
        let mut tree = tree();
        tree.set_edge("o", "gc");
        tree.set_edge_with(
            "gc",
            "c",
            TreeEdge {
                cutvalue: Some(3.0),
            },
        );
        tree.set_edge("c", "p");
        assert_eq!(cut_value(&mut graph, &mut tree, "c"), 10.0);
    }

    #[test]
    fn calc_cut_value_incoming_from_grandchild_and_parent() {
        let mut graph = multigraph();
        graph.set_edge("gc", "c");
        graph.set_edge("p", "c");
        graph.set_edge("p", "o");
        graph.set_edge_with("o", "c", weighted(7.0, 1.0));
        let mut tree = tree();
        tree.set_edge("o", "gc");
        tree.set_edge_with(
            "gc",
            "c",
            TreeEdge {
                cutvalue: Some(3.0),
            },
        );
        tree.set_edge("c", "p");
        assert_eq!(cut_value(&mut graph, &mut tree, "c"), 6.0);
    }

    #[test]
    fn calc_cut_value_outgoing_from_child_and_parent() {
        let mut graph = multigraph();
        graph.set_edge("gc", "c");
        graph.set_edge("p", "c");
        graph.set_edge("p", "o");
        graph.set_edge_with("c", "o", weighted(7.0, 1.0));
        let mut tree = tree();
        tree.set_edge("o", "gc");
        tree.set_edge_with(
            "gc",
            "c",
            TreeEdge {
                cutvalue: Some(3.0),
            },
        );
        tree.set_edge("c", "p");
        assert_eq!(cut_value(&mut graph, &mut tree, "c"), -8.0);
    }

    #[test]
    fn calc_cut_value_other_to_grandchild_to_child_incoming() {
        let mut graph = multigraph();
        graph.set_edge_with("o", "c", weighted(7.0, 1.0));
        graph.set_path(&["o", "gc", "c"]);
        graph.set_edge("p", "c");
        let mut tree = tree();
        tree.set_edge("o", "gc");
        tree.set_edge_with(
            "gc",
            "c",
            TreeEdge {
                cutvalue: Some(3.0),
            },
        );
        tree.set_edge("c", "p");
        assert_eq!(cut_value(&mut graph, &mut tree, "c"), 6.0);
    }

    #[test]
    fn calc_cut_value_child_to_other_through_grandchild_incoming() {
        let mut graph = multigraph();
        graph.set_edge_with("c", "o", weighted(7.0, 1.0));
        graph.set_path(&["o", "gc", "c"]);
        graph.set_edge("p", "c");
        let mut tree = tree();
        tree.set_edge("o", "gc");
        tree.set_edge_with(
            "gc",
            "c",
            TreeEdge {
                cutvalue: Some(3.0),
            },
        );
        tree.set_edge("c", "p");
        assert_eq!(cut_value(&mut graph, &mut tree, "c"), -8.0);
    }

    #[test]
    fn init_cut_values_works_for_the_gansner_graph() {
        let graph = gansner_graph();
        let mut tree = gansner_tree();
        super::init_low_lim_values(&mut tree, None);
        super::init_cut_values(&mut tree, &graph);
        assert_eq!(tree.edge("a", "b").unwrap().cutvalue, Some(3.0));
        assert_eq!(tree.edge("b", "c").unwrap().cutvalue, Some(3.0));
        assert_eq!(tree.edge("c", "d").unwrap().cutvalue, Some(3.0));
        assert_eq!(tree.edge("d", "h").unwrap().cutvalue, Some(3.0));
        assert_eq!(tree.edge("g", "h").unwrap().cutvalue, Some(-1.0));
        assert_eq!(tree.edge("e", "g").unwrap().cutvalue, Some(0.0));
        assert_eq!(tree.edge("f", "g").unwrap().cutvalue, Some(0.0));
    }

    #[test]
    fn init_cut_values_works_for_the_updated_gansner_graph() {
        let graph = gansner_graph();
        let mut tree = gansner_tree();
        tree.remove_edge("g", "h");
        tree.set_edge("a", "e");
        super::init_low_lim_values(&mut tree, None);
        super::init_cut_values(&mut tree, &graph);
        assert_eq!(tree.edge("a", "b").unwrap().cutvalue, Some(2.0));
        assert_eq!(tree.edge("b", "c").unwrap().cutvalue, Some(2.0));
        assert_eq!(tree.edge("c", "d").unwrap().cutvalue, Some(2.0));
        assert_eq!(tree.edge("d", "h").unwrap().cutvalue, Some(2.0));
        assert_eq!(tree.edge("a", "e").unwrap().cutvalue, Some(1.0));
        assert_eq!(tree.edge("e", "g").unwrap().cutvalue, Some(1.0));
        assert_eq!(tree.edge("f", "g").unwrap().cutvalue, Some(0.0));
    }
}
