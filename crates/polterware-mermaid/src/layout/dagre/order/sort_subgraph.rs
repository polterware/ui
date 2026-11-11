//! Port of `dagre/lib/order/sort-subgraph.ts` (MIT, Copyright (c) 2012-2014 Chris
//! Pettitt).
//!
//! Sorts one movable subgraph (or the root of a layer graph) recursively, expanding
//! nested subgraphs into their sorted members and keeping border nodes at the ends.

use super::super::super::graphlib::Graph;
use super::super::{LayeredGraph, NodeLabel};
use super::barycenter::{BarycenterEntry, barycenter};
use super::resolve_conflicts::{ResolvedEntry, resolve_conflicts};
use super::sort::{SortResult, sort_with};

/// The sorted members of a subgraph.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SubgraphResult {
    /// The nodes in order.
    pub vs: Vec<String>,
    /// The weighted mean barycenter, when there are barycenters to merge.
    pub barycenter: Option<f64>,
    /// The sum of the weights, when there are weights to merge.
    pub weight: Option<f64>,
    /// Whether the sort had to break a tie with the bias, upstream's `usedBias`.
    pub used_bias: bool,
}

/// Sorts the children of `v` inside `graph`.
pub fn sort_subgraph(
    graph: &LayeredGraph,
    v: &str,
    constraint_graph: &Graph<(), (), ()>,
    old_nodes: Option<&[(String, NodeLabel)]>,
    bias_right: bool,
) -> SubgraphResult {
    let mut movable = graph.children(v);
    let node = graph.node(v).cloned();
    let border_left = node.as_ref().and_then(|node| node.border_left_node.clone());
    let border_right = node
        .as_ref()
        .and_then(|node| node.border_right_node.clone());
    let mut subgraphs: Vec<(String, SubgraphResult)> = Vec::new();

    if let Some(border_left) = &border_left {
        movable.retain(|w| Some(w) != Some(border_left) && Some(w) != border_right.as_ref());
    }

    let mut barycenters = barycenter(graph, &movable);
    for entry in &mut barycenters {
        if !graph.children(&entry.v).is_empty() {
            let subgraph_result =
                sort_subgraph(graph, &entry.v, constraint_graph, old_nodes, bias_right);
            if subgraph_result.barycenter.is_some() {
                merge_barycenters(entry, &subgraph_result);
            }
            subgraphs.push((entry.v.clone(), subgraph_result));
        }
    }

    let mut entries = resolve_conflicts(&barycenters, constraint_graph);
    expand_subgraphs(&mut entries, &subgraphs);

    let mut reversed_pairs: Vec<(String, ResolvedEntry)> = Vec::new();
    let mut used_bias = false;
    let mut i = 0usize;
    while i < entries.len() {
        let mut restart_outer = false;
        let mut j = i + 1;
        while j < entries.len() {
            let (Some(barycenter_i), Some(barycenter_j)) =
                (entries[i].barycenter, entries[j].barycenter)
            else {
                j += 1;
                continue;
            };
            if barycenter_i == 0.0 || barycenter_j == 0.0 {
                j += 1;
                continue;
            }
            if barycenter_i == barycenter_j {
                let name_i = entries[i].vs.first().cloned().unwrap_or_default();
                let name_j = entries[j].vs.first().cloned().unwrap_or_default();
                let node_i = graph.node(&name_i).cloned().unwrap_or_default();
                let node_j = graph.node(&name_j).cloned().unwrap_or_default();

                let same_edge = node_i.dummy.as_deref() == Some("edge")
                    && node_j.dummy.as_deref() == Some("edge")
                    && node_i.edge_obj.as_ref().is_some_and(|edge| {
                        node_j
                            .edge_obj
                            .as_ref()
                            .is_some_and(|other| edge.v == other.v && edge.w == other.w)
                    });
                if same_edge {
                    if node_i
                        .edge_label
                        .as_ref()
                        .is_some_and(|label| label.reversed)
                    {
                        reversed_pairs.push((name_j, entries[i].clone()));
                        entries.remove(i);
                        restart_outer = true;
                        break;
                    } else if !reversed_pairs.iter().any(|(key, _)| key == &name_i) {
                        reversed_pairs.push((name_i, entries[j].clone()));
                        entries.remove(j);
                        j = j.saturating_sub(1);
                    }
                } else {
                    used_bias = true;
                }
            }
            j += 1;
        }
        if restart_outer {
            i = i.saturating_sub(1);
            continue;
        }
        i += 1;
    }

    let entries = entries.into_iter().map(Into::into).collect();
    let mut result = sort_with(entries, &reversed_pairs, old_nodes, Some(graph), bias_right);

    if let (Some(border_left), Some(border_right)) = (border_left, border_right) {
        let mut vs = vec![border_left.clone()];
        vs.extend(result.vs);
        vs.push(border_right.clone());
        result.vs = vs;

        let left_predecessors = graph.predecessors(&border_left).unwrap_or_default();
        if let Some(left_predecessor) = left_predecessors.first() {
            let left_node = graph.node(left_predecessor).cloned().unwrap_or_default();
            let right_predecessors = graph.predecessors(&border_right).unwrap_or_default();
            let right_node = right_predecessors
                .first()
                .and_then(|right| graph.node(right))
                .cloned()
                .unwrap_or_default();
            if result.barycenter.is_none() {
                result.barycenter = Some(0.0);
                result.weight = Some(0.0);
            }
            let barycenter = result.barycenter.unwrap_or(0.0);
            let weight = result.weight.unwrap_or(0.0);
            result.barycenter = Some(
                (barycenter * weight
                    + left_node.order.unwrap_or(0) as f64
                    + right_node.order.unwrap_or(0) as f64)
                    / (weight + 2.0),
            );
            result.weight = Some(weight + 2.0);
        }
    }

    SubgraphResult {
        vs: result.vs,
        barycenter: result.barycenter,
        weight: result.weight,
        used_bias,
    }
}

fn expand_subgraphs(entries: &mut [ResolvedEntry], subgraphs: &[(String, SubgraphResult)]) {
    for entry in entries {
        entry.vs = entry
            .vs
            .iter()
            .flat_map(|v| match subgraphs.iter().find(|(id, _)| id == v) {
                Some((_, subgraph)) => subgraph.vs.clone(),
                None => vec![v.clone()],
            })
            .collect();
    }
}

fn merge_barycenters(target: &mut BarycenterEntry, other: &SubgraphResult) {
    merge_barycenter_values(
        &mut target.barycenter,
        &mut target.weight,
        other.barycenter,
        other.weight,
    );
}

fn merge_barycenter_values(
    target_barycenter: &mut Option<f64>,
    target_weight: &mut Option<f64>,
    other_barycenter: Option<f64>,
    other_weight: Option<f64>,
) {
    if let Some(barycenter) = *target_barycenter {
        let target_weight_value = target_weight.unwrap_or(0.0);
        let other_weight_value = other_weight.unwrap_or(0.0);
        *target_barycenter = Some(
            (barycenter * target_weight_value
                + other_barycenter.unwrap_or(0.0) * other_weight_value)
                / (target_weight_value + other_weight_value),
        );
        *target_weight = Some(target_weight_value + other_weight_value);
    } else {
        *target_barycenter = other_barycenter;
        *target_weight = other_weight;
    }
}

/// The sort result type is re-exported for callers that build subgraph expectations.
pub type SubgraphSortResult = SortResult;

#[cfg(test)]
mod tests {
    use super::super::super::super::graphlib::Graph;
    use super::super::super::NodeLabel;
    use super::super::super::test_support::{compound, weighted};

    fn ordered_node(order: usize) -> NodeLabel {
        NodeLabel {
            order: Some(order),
            ..NodeLabel::default()
        }
    }

    fn setup() -> (super::super::super::LayeredGraph, Graph<(), (), ()>) {
        let mut graph = compound();
        for (i, v) in ["0", "1", "2", "3", "4"].iter().enumerate() {
            graph.set_node_with(*v, ordered_node(i));
        }
        (graph, Graph::new())
    }

    #[test]
    fn sorts_a_flat_subgraph_by_barycenter() {
        let (mut graph, constraints) = setup();
        graph.set_edge("3", "x");
        graph.set_edge_with("1", "y", weighted(2.0, 1.0));
        graph.set_edge("4", "y");
        graph.set_parent("x", Some("movable")).unwrap();
        graph.set_parent("y", Some("movable")).unwrap();
        assert_eq!(
            super::sort_subgraph(&graph, "movable", &constraints, None, false).vs,
            vec!["y".to_string(), "x".to_string()]
        );
    }

    #[test]
    fn preserves_the_position_of_a_node_without_neighbors() {
        let (mut graph, constraints) = setup();
        graph.set_edge("3", "x");
        graph.set_node_with("y", NodeLabel::default());
        graph.set_edge_with("1", "z", weighted(2.0, 1.0));
        graph.set_edge("4", "z");
        for v in ["x", "y", "z"] {
            graph.set_parent(v, Some("movable")).unwrap();
        }
        assert_eq!(
            super::sort_subgraph(&graph, "movable", &constraints, None, false).vs,
            vec!["z".to_string(), "y".to_string(), "x".to_string()]
        );
    }

    #[test]
    fn biases_left_without_reverse_bias() {
        let (mut graph, constraints) = setup();
        graph.set_edge("1", "x");
        graph.set_edge("1", "y");
        graph.set_parent("x", Some("movable")).unwrap();
        graph.set_parent("y", Some("movable")).unwrap();
        assert_eq!(
            super::sort_subgraph(&graph, "movable", &constraints, None, false).vs,
            vec!["x".to_string(), "y".to_string()]
        );
    }

    #[test]
    fn biases_right_with_reverse_bias() {
        let (mut graph, constraints) = setup();
        graph.set_edge("1", "x");
        graph.set_edge("1", "y");
        graph.set_parent("x", Some("movable")).unwrap();
        graph.set_parent("y", Some("movable")).unwrap();
        assert_eq!(
            super::sort_subgraph(&graph, "movable", &constraints, None, true).vs,
            vec!["y".to_string(), "x".to_string()]
        );
    }

    #[test]
    fn aggregates_stats_about_the_subgraph() {
        let (mut graph, constraints) = setup();
        graph.set_edge("3", "x");
        graph.set_edge_with("1", "y", weighted(2.0, 1.0));
        graph.set_edge("4", "y");
        graph.set_parent("x", Some("movable")).unwrap();
        graph.set_parent("y", Some("movable")).unwrap();
        let result = super::sort_subgraph(&graph, "movable", &constraints, None, false);
        assert_eq!(result.barycenter, Some(2.25));
        assert_eq!(result.weight, Some(4.0));
    }

    #[test]
    fn can_sort_a_nested_subgraph_without_a_barycenter() {
        let (mut graph, constraints) = setup();
        for v in ["a", "b", "c"] {
            graph.set_node_with(v, NodeLabel::default());
            graph.set_parent(v, Some("y")).unwrap();
        }
        graph.set_edge("0", "x");
        graph.set_edge("1", "z");
        graph.set_edge("2", "y");
        for v in ["x", "y", "z"] {
            graph.set_parent(v, Some("movable")).unwrap();
        }
        assert_eq!(
            super::sort_subgraph(&graph, "movable", &constraints, None, false).vs,
            vec![
                "x".to_string(),
                "z".to_string(),
                "a".to_string(),
                "b".to_string(),
                "c".to_string(),
            ]
        );
    }

    #[test]
    fn can_sort_a_nested_subgraph_with_a_barycenter() {
        let (mut graph, constraints) = setup();
        for v in ["a", "b", "c"] {
            graph.set_node_with(v, NodeLabel::default());
            graph.set_parent(v, Some("y")).unwrap();
        }
        graph.set_edge_with("0", "a", weighted(3.0, 1.0));
        graph.set_edge("0", "x");
        graph.set_edge("1", "z");
        graph.set_edge("2", "y");
        for v in ["x", "y", "z"] {
            graph.set_parent(v, Some("movable")).unwrap();
        }
        assert_eq!(
            super::sort_subgraph(&graph, "movable", &constraints, None, false).vs,
            vec![
                "x".to_string(),
                "a".to_string(),
                "b".to_string(),
                "c".to_string(),
                "z".to_string(),
            ]
        );
    }

    #[test]
    fn can_sort_a_nested_subgraph_without_in_edges() {
        let (mut graph, constraints) = setup();
        for v in ["a", "b", "c"] {
            graph.set_node_with(v, NodeLabel::default());
            graph.set_parent(v, Some("y")).unwrap();
        }
        graph.set_edge("0", "a");
        graph.set_edge("1", "b");
        graph.set_edge("0", "x");
        graph.set_edge("1", "z");
        for v in ["x", "y", "z"] {
            graph.set_parent(v, Some("movable")).unwrap();
        }
        assert_eq!(
            super::sort_subgraph(&graph, "movable", &constraints, None, false).vs,
            vec![
                "x".to_string(),
                "a".to_string(),
                "b".to_string(),
                "c".to_string(),
                "z".to_string(),
            ]
        );
    }

    #[test]
    fn sorts_border_nodes_to_the_extremes() {
        let (mut graph, constraints) = setup();
        graph.set_edge("0", "x");
        graph.set_edge("1", "y");
        graph.set_edge("2", "z");
        graph.set_node_with(
            "sg1",
            NodeLabel {
                border_left_node: Some("bl".to_string()),
                border_right_node: Some("br".to_string()),
                ..NodeLabel::default()
            },
        );
        for v in ["x", "y", "z", "bl", "br"] {
            graph.set_parent(v, Some("sg1")).unwrap();
        }
        assert_eq!(
            super::sort_subgraph(&graph, "sg1", &constraints, None, false).vs,
            vec![
                "bl".to_string(),
                "x".to_string(),
                "y".to_string(),
                "z".to_string(),
                "br".to_string(),
            ]
        );
    }

    #[test]
    fn assigns_a_barycenter_from_previous_border_nodes() {
        let (mut graph, constraints) = setup();
        graph.set_node_with("bl1", ordered_node(0));
        graph.set_node_with("br1", ordered_node(1));
        graph.set_edge("bl1", "bl2");
        graph.set_edge("br1", "br2");
        graph.set_parent("bl2", Some("sg")).unwrap();
        graph.set_parent("br2", Some("sg")).unwrap();
        graph.set_node_with(
            "sg",
            NodeLabel {
                border_left_node: Some("bl2".to_string()),
                border_right_node: Some("br2".to_string()),
                ..NodeLabel::default()
            },
        );
        let result = super::sort_subgraph(&graph, "sg", &constraints, None, false);
        assert_eq!(result.barycenter, Some(0.5));
        assert_eq!(result.weight, Some(2.0));
        assert_eq!(result.vs, vec!["bl2".to_string(), "br2".to_string()]);
    }
}
