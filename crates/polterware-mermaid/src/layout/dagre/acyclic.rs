//! Port of `dagre/lib/acyclic.ts` (MIT, Copyright (c) 2012-2014 Chris Pettitt).
//!
//! Breaks cycles by reversing a feedback arc set. With `acyclicer: "greedy"` the
//! set comes from [`super::greedy_fas`]; otherwise from a depth-first search, which
//! can also prefer to break edges that point backwards in the previous layout.

use std::collections::{HashMap, HashSet};

use super::super::graphlib::Edge;
use super::greedy_fas::{WeightFn, greedy_fas};
use super::util::unique_id;
use super::{EdgeLabel, LayeredGraph};

/// Ranks of the previous layout graph, when one is available.
pub type OldRanks = HashMap<String, Option<f64>>;

/// Reverses a feedback arc set of `graph` and marks those edges as reversed.
pub fn run(graph: &mut LayeredGraph, old_ranks: Option<&OldRanks>) {
    let weight = |edge: &Edge| graph.edge_object(edge).map_or(0.0, EdgeLabel::weight);
    let weight_fn: &WeightFn<'_> = &weight;
    let fas = if graph.graph().and_then(|graph| graph.acyclicer.as_deref()) == Some("greedy") {
        greedy_fas(graph, Some(weight_fn))
    } else {
        dfs_fas(graph, old_ranks)
    };

    for edge in fas {
        let Some(mut label) = graph.edge_object(&edge).cloned() else {
            continue;
        };
        graph.remove_edge_object(&edge);
        label.forward_name = edge.name.clone();
        label.reversed = true;
        graph
            .set_edge_full(&edge.w, &edge.v, Some(label), Some(&unique_id("rev")))
            .expect("a reversal stays inside the multigraph");
    }
}

fn dfs_fas(graph: &LayeredGraph, old_ranks: Option<&OldRanks>) -> Vec<Edge> {
    let mut fas = Vec::new();
    let mut stack = HashSet::new();
    let mut visited = HashSet::new();

    if let Some(old_ranks) = old_ranks {
        for v in graph.sources() {
            dfs_dynamic(graph, &v, old_ranks, &mut fas, &mut stack, &mut visited);
        }
        for v in graph.nodes() {
            dfs_dynamic(graph, &v, old_ranks, &mut fas, &mut stack, &mut visited);
        }
    } else {
        for v in graph.sources() {
            dfs(graph, &v, &mut fas, &mut stack, &mut visited);
        }
        for v in graph.nodes() {
            dfs(graph, &v, &mut fas, &mut stack, &mut visited);
        }
    }

    fas
}

fn dfs(
    graph: &LayeredGraph,
    v: &str,
    fas: &mut Vec<Edge>,
    stack: &mut HashSet<String>,
    visited: &mut HashSet<String>,
) {
    if !visited.insert(v.to_string()) {
        return;
    }
    stack.insert(v.to_string());
    for edge in graph.out_edges(v, None).unwrap_or_default() {
        if stack.contains(&edge.w) {
            fas.push(edge);
        } else {
            dfs(graph, &edge.w, fas, stack, visited);
        }
    }
    stack.remove(v);
}

fn dfs_dynamic(
    graph: &LayeredGraph,
    v: &str,
    old_ranks: &OldRanks,
    fas: &mut Vec<Edge>,
    stack: &mut HashSet<String>,
    visited: &mut HashSet<String>,
) {
    if !visited.insert(v.to_string()) {
        return;
    }
    stack.insert(v.to_string());
    for edge in graph.out_edges(v, None).unwrap_or_default() {
        let points_back = match (old_ranks.get(v), old_ranks.get(&edge.w)) {
            (Some(Some(from)), Some(Some(to))) => from > to,
            _ => false,
        };
        if stack.contains(&edge.w)
            || (points_back && is_reached_from_start_without_edge(graph, &edge.w, &edge))
        {
            fas.push(edge);
        } else {
            dfs_dynamic(graph, &edge.w, old_ranks, fas, stack, visited);
        }
    }
    stack.remove(v);
}

/// Whether `node` can reach a source using only edges other than `ignore_edge`.
fn is_reached_from_start_without_edge(
    graph: &LayeredGraph,
    node: &str,
    ignore_edge: &Edge,
) -> bool {
    let mut visited = HashSet::new();
    reverse_dfs(graph, node, ignore_edge, &mut visited)
}

fn reverse_dfs(
    graph: &LayeredGraph,
    v: &str,
    ignore_edge: &Edge,
    visited: &mut HashSet<String>,
) -> bool {
    if graph.sources().iter().any(|source| source == v) {
        return true;
    }
    visited.insert(v.to_string());
    for edge in graph.in_edges(v, None).unwrap_or_default() {
        let ignored = edge.v == ignore_edge.v && edge.w == ignore_edge.w;
        if !ignored
            && !visited.contains(&edge.v)
            && reverse_dfs(graph, &edge.v, ignore_edge, visited)
        {
            return true;
        }
    }
    false
}

/// Restores the edges [`run`] reversed.
pub fn undo(graph: &mut LayeredGraph) {
    for edge in graph.edges() {
        let Some(label) = graph.edge_object(&edge).cloned() else {
            continue;
        };
        if !label.reversed {
            continue;
        }
        graph.remove_edge_object(&edge);
        let mut label = label;
        let forward_name = label.forward_name.clone();
        label.reversed = false;
        label.forward_name = None;
        graph
            .set_edge_full(&edge.w, &edge.v, Some(label), forward_name.as_deref())
            .expect("a restored edge stays inside the multigraph");
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::graphlib::alg::find_cycles;
    use super::super::test_support::{multigraph, weighted};
    use super::super::{EdgeLabel, GraphLabel, LayeredGraph};

    fn default_edge() -> EdgeLabel {
        weighted(1.0, 1.0)
    }

    fn build(acyclicer: &str) -> LayeredGraph {
        let mut graph = multigraph();
        graph.set_graph(GraphLabel {
            acyclicer: Some(acyclicer.to_string()),
            ..GraphLabel::default()
        });
        graph
    }

    fn path(graph: &mut LayeredGraph, nodes: &[&str]) {
        graph.set_path_with(nodes, default_edge());
    }

    #[test]
    fn run_does_not_change_an_already_acyclic_graph() {
        for acyclicer in ["greedy", "dfs", "unknown-should-still-work"] {
            let mut graph = build(acyclicer);
            path(&mut graph, &["a", "b", "d"]);
            path(&mut graph, &["a", "c", "d"]);
            super::run(&mut graph, None);
            let mut edges: Vec<(String, String)> = graph
                .edges()
                .into_iter()
                .map(|edge| (edge.v, edge.w))
                .collect();
            edges.sort();
            assert_eq!(
                edges,
                vec![
                    ("a".to_string(), "b".to_string()),
                    ("a".to_string(), "c".to_string()),
                    ("b".to_string(), "d".to_string()),
                    ("c".to_string(), "d".to_string()),
                ]
            );
        }
    }

    #[test]
    fn run_breaks_cycles_in_the_input_graph() {
        for acyclicer in ["greedy", "dfs", "unknown-should-still-work"] {
            let mut graph = build(acyclicer);
            path(&mut graph, &["a", "b", "c", "d", "a"]);
            super::run(&mut graph, None);
            assert!(find_cycles(&graph).is_empty());
        }
    }

    #[test]
    fn run_creates_a_multi_edge_where_necessary() {
        for acyclicer in ["greedy", "dfs", "unknown-should-still-work"] {
            let mut graph = build(acyclicer);
            path(&mut graph, &["a", "b", "a"]);
            super::run(&mut graph, None);
            assert!(find_cycles(&graph).is_empty());
            if graph.has_edge("a", "b") {
                assert_eq!(graph.out_edges("a", Some("b")).unwrap().len(), 2);
            } else {
                assert_eq!(graph.out_edges("b", Some("a")).unwrap().len(), 2);
            }
            assert_eq!(graph.edge_count(), 2);
        }
    }

    #[test]
    fn undo_does_not_change_an_acyclic_graph() {
        for acyclicer in ["greedy", "dfs", "unknown-should-still-work"] {
            let mut graph = build(acyclicer);
            graph.set_edge_with("a", "b", weighted(3.0, 2.0));
            super::run(&mut graph, None);
            super::undo(&mut graph);
            let label = graph.edge("a", "b").unwrap();
            assert_eq!(label.weight(), 3.0);
            assert_eq!(label.minlen(), 2.0);
            assert_eq!(graph.edge_count(), 1);
        }
    }

    #[test]
    fn undo_restores_previously_reversed_edges() {
        for acyclicer in ["greedy", "dfs", "unknown-should-still-work"] {
            let mut graph = build(acyclicer);
            graph.set_edge_with("a", "b", weighted(3.0, 2.0));
            graph.set_edge_with("b", "a", weighted(4.0, 3.0));
            super::run(&mut graph, None);
            super::undo(&mut graph);
            let forward = graph.edge("a", "b").unwrap();
            assert_eq!(forward.weight(), 3.0);
            assert_eq!(forward.minlen(), 2.0);
            let backward = graph.edge("b", "a").unwrap();
            assert_eq!(backward.weight(), 4.0);
            assert_eq!(backward.minlen(), 3.0);
            assert_eq!(graph.edge_count(), 2);
        }
    }

    #[test]
    fn greedy_prefers_to_break_cycles_at_low_weight_edges() {
        let mut graph = build("greedy");
        graph.set_path_with(&["a", "b", "c", "d", "a"], weighted(2.0, 1.0));
        graph.set_edge_with("c", "d", weighted(1.0, 1.0));
        super::run(&mut graph, None);
        assert!(find_cycles(&graph).is_empty());
        assert!(!graph.has_edge("c", "d"));
    }
}
