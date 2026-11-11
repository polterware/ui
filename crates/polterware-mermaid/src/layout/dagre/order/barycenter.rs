//! Port of `dagre/lib/order/barycenter.ts` (MIT, Copyright (c) 2012-2014 Chris
//! Pettitt).

use super::super::LayeredGraph;

/// One node's barycenter: the weighted mean order of its predecessors.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BarycenterEntry {
    /// The movable node.
    pub v: String,
    /// The weighted mean order, absent when the node has no in-edges.
    pub barycenter: Option<f64>,
    /// The sum of the in-edge weights.
    pub weight: Option<f64>,
}

/// Computes the barycenter of every node in `movable`.
pub fn barycenter(graph: &LayeredGraph, movable: &[String]) -> Vec<BarycenterEntry> {
    movable
        .iter()
        .map(|v| {
            let in_edges = graph.in_edges(v, None).unwrap_or_default();
            if in_edges.is_empty() {
                return BarycenterEntry {
                    v: v.clone(),
                    ..BarycenterEntry::default()
                };
            }
            let mut sum = 0.0;
            let mut weight = 0.0;
            for edge in in_edges {
                let edge_label = graph.edge_object(&edge).cloned().unwrap_or_default();
                let node_u = graph.node(&edge.v).cloned().unwrap_or_default();
                sum += edge_label.weight() * node_u.order.unwrap_or(0) as f64;
                weight += edge_label.weight();
            }
            BarycenterEntry {
                v: v.clone(),
                barycenter: Some(sum / weight),
                weight: Some(weight),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::super::NodeLabel;
    use super::super::super::test_support::{graph, weighted};

    fn ordered_node(order: usize) -> NodeLabel {
        NodeLabel {
            order: Some(order),
            ..NodeLabel::default()
        }
    }

    #[test]
    fn assigns_no_barycenter_without_predecessors() {
        let mut graph = graph();
        graph.set_node_with("x", NodeLabel::default());
        let results = super::barycenter(&graph, &["x".to_string()]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].v, "x");
        assert_eq!(results[0].barycenter, None);
        assert_eq!(results[0].weight, None);
    }

    #[test]
    fn assigns_the_position_of_the_sole_predecessor() {
        let mut graph = graph();
        graph.set_node_with("a", ordered_node(2));
        graph.set_edge("a", "x");
        let results = super::barycenter(&graph, &["x".to_string()]);
        assert_eq!(results[0].barycenter, Some(2.0));
        assert_eq!(results[0].weight, Some(1.0));
    }

    #[test]
    fn assigns_the_average_of_multiple_predecessors() {
        let mut graph = graph();
        graph.set_node_with("a", ordered_node(2));
        graph.set_node_with("b", ordered_node(4));
        graph.set_edge("a", "x");
        graph.set_edge("b", "x");
        let results = super::barycenter(&graph, &["x".to_string()]);
        assert_eq!(results[0].barycenter, Some(3.0));
        assert_eq!(results[0].weight, Some(2.0));
    }

    #[test]
    fn takes_edge_weights_into_account() {
        let mut graph = graph();
        graph.set_node_with("a", ordered_node(2));
        graph.set_node_with("b", ordered_node(4));
        graph.set_edge_with("a", "x", weighted(3.0, 1.0));
        graph.set_edge("b", "x");
        let results = super::barycenter(&graph, &["x".to_string()]);
        assert_eq!(results[0].barycenter, Some(2.5));
        assert_eq!(results[0].weight, Some(4.0));
    }

    #[test]
    fn calculates_barycenters_for_all_movable_nodes() {
        let mut graph = graph();
        graph.set_node_with("a", ordered_node(1));
        graph.set_node_with("b", ordered_node(2));
        graph.set_node_with("c", ordered_node(4));
        graph.set_edge("a", "x");
        graph.set_edge("b", "x");
        graph.set_node_with("y", NodeLabel::default());
        graph.set_edge_with("a", "z", weighted(2.0, 1.0));
        graph.set_edge("c", "z");
        let results =
            super::barycenter(&graph, &["x".to_string(), "y".to_string(), "z".to_string()]);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].barycenter, Some(1.5));
        assert_eq!(results[0].weight, Some(2.0));
        assert_eq!(results[1].barycenter, None);
        assert_eq!(results[2].barycenter, Some(2.0));
        assert_eq!(results[2].weight, Some(3.0));
    }

    #[test]
    fn keeps_the_movable_order() {
        let mut graph = graph();
        graph.set_node_with("a", ordered_node(1));
        graph.set_edge("a", "z");
        let results = super::barycenter(&graph, &["z".to_string(), "y".to_string()]);
        assert_eq!(results[0].v, "z");
        assert_eq!(results[1].v, "y");
    }
}
