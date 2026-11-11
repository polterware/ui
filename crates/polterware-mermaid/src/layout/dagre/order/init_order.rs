//! Port of `dagre/lib/order/init-order.ts` (MIT, Copyright (c) 2012-2014 Chris
//! Pettitt).
//!
//! Assigns an initial order by a depth-first walk from the first rank, in rank
//! order and, among successors, in the previous layout's order when there is one.

use std::collections::HashSet;

use super::super::util::{self, LayerMatrix};
use super::super::{LayeredGraph, NodeLabel};

/// The initial layering of `graph`.
pub fn init_order(graph: &LayeredGraph, old_nodes: Option<&[(String, NodeLabel)]>) -> LayerMatrix {
    let mut visited: HashSet<String> = HashSet::new();
    let simple_nodes: Vec<String> = graph
        .nodes()
        .into_iter()
        .filter(|v| graph.children(v).is_empty())
        .collect();
    let ranks: Vec<f64> = simple_nodes
        .iter()
        .map(|v| graph.node(v).and_then(|node| node.rank).unwrap_or(f64::NAN))
        .collect();
    let max_rank = ranks.into_iter().fold(f64::NEG_INFINITY, f64::max);
    let layer_count = if max_rank.is_finite() && max_rank >= 0.0 {
        max_rank.floor() as usize + 1
    } else {
        0
    };
    let mut layers: LayerMatrix = vec![Vec::new(); layer_count];

    fn dfs(
        graph: &LayeredGraph,
        v: &str,
        old_nodes: Option<&[(String, NodeLabel)]>,
        visited: &mut HashSet<String>,
        layers: &mut LayerMatrix,
    ) {
        if !visited.insert(v.to_string()) {
            return;
        }
        let node = graph.node(v).cloned().unwrap_or_default();
        let rank = node.rank.unwrap_or(0.0) as usize;
        if rank >= layers.len() {
            layers.resize(rank + 1, Vec::new());
        }
        layers[rank].push(v.to_string());

        let mut successors = graph.successors(v).unwrap_or_default();
        successors.sort_by(|a, b| {
            let node_a = graph.node(a).cloned().unwrap_or_default();
            let node_b = graph.node(b).cloned().unwrap_or_default();
            util::cmp_f64(util::compare_by_old_order(old_nodes, &node_a, &node_b), 0.0)
        });
        for successor in successors {
            dfs(graph, &successor, old_nodes, visited, layers);
        }
    }

    let mut ordered = simple_nodes;
    ordered.sort_by(|a, b| {
        let rank_a = graph.node(a).and_then(|node| node.rank).unwrap_or(0.0);
        let rank_b = graph.node(b).and_then(|node| node.rank).unwrap_or(0.0);
        util::cmp_f64(rank_a, rank_b)
    });
    for v in ordered {
        dfs(graph, &v, old_nodes, &mut visited, &mut layers);
    }

    layers
}

#[cfg(test)]
mod tests {
    use super::super::super::NodeLabel;
    use super::super::super::test_support::{compound, ordered};

    fn sorted(layer: &[String]) -> Vec<String> {
        let mut layer = layer.to_vec();
        layer.sort();
        layer
    }

    #[test]
    fn assigns_non_overlapping_orders_for_each_rank_in_a_tree() {
        let mut graph = compound();
        ordered(&mut graph, "a", 0.0, 0);
        ordered(&mut graph, "b", 1.0, 0);
        ordered(&mut graph, "c", 2.0, 0);
        ordered(&mut graph, "d", 2.0, 0);
        ordered(&mut graph, "e", 1.0, 0);
        graph.set_path(&["a", "b", "c"]);
        graph.set_edge("b", "d");
        graph.set_edge("a", "e");
        let layering = super::init_order(&graph, None);
        assert_eq!(layering[0], vec!["a".to_string()]);
        assert_eq!(sorted(&layering[1]), vec!["b".to_string(), "e".to_string()]);
        assert_eq!(sorted(&layering[2]), vec!["c".to_string(), "d".to_string()]);
    }

    #[test]
    fn assigns_non_overlapping_orders_for_each_rank_in_a_dag() {
        let mut graph = compound();
        ordered(&mut graph, "a", 0.0, 0);
        ordered(&mut graph, "b", 1.0, 0);
        ordered(&mut graph, "c", 1.0, 0);
        ordered(&mut graph, "d", 2.0, 0);
        graph.set_path(&["a", "b", "d"]);
        graph.set_path(&["a", "c", "d"]);
        let layering = super::init_order(&graph, None);
        assert_eq!(layering[0], vec!["a".to_string()]);
        assert_eq!(sorted(&layering[1]), vec!["b".to_string(), "c".to_string()]);
        assert_eq!(layering[2], vec!["d".to_string()]);
    }

    #[test]
    fn does_not_assign_an_order_to_subgraph_nodes() {
        let mut graph = compound();
        ordered(&mut graph, "a", 0.0, 0);
        graph.set_node_with("sg1", NodeLabel::default());
        graph.set_parent("a", Some("sg1")).unwrap();
        let layering = super::init_order(&graph, None);
        assert_eq!(layering, vec![vec!["a".to_string()]]);
    }
}
