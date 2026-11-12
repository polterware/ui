//! Port of `dagre-d3-es/src/dagre/nesting-graph.js` (MIT, Copyright (c) 2012-2014 Chris Pettitt).
//!
//! Adds border nodes to every subgraph and a root node that connects the graph, so
//! ranking can keep subgraphs compact. Follows Sander, "Layout of Compound Directed
//! Graphs."

use std::collections::HashMap;

use super::util::{add_border_node, add_dummy_node};
use super::{EdgeLabel, LayeredGraph, NodeLabel};

/// Adds the nesting graph to `graph`.
pub fn run(graph: &mut LayeredGraph) {
    let root = add_dummy_node(graph, "root", NodeLabel::default(), "_root");
    let depths = tree_depths(graph);
    let height = depths.values().copied().fold(f64::NEG_INFINITY, f64::max) - 1.0;
    let node_sep = 2.0 * height + 1.0;

    if let Some(label) = graph.graph_mut() {
        label.nesting_root = Some(root.clone());
    }

    for edge in graph.edges() {
        if let Some(label) = graph.edge_object_mut(&edge) {
            label.minlen = Some(label.minlen() * node_sep);
        }
    }

    let weight = sum_weights(graph) + 1.0;

    for child in graph.root_children() {
        dfs(graph, &root, node_sep, weight, height, &depths, &child);
    }

    if let Some(label) = graph.graph_mut() {
        label.node_rank_factor = Some(node_sep);
    }
}

fn dfs(
    graph: &mut LayeredGraph,
    root: &str,
    node_sep: f64,
    weight: f64,
    height: f64,
    depths: &HashMap<String, f64>,
    v: &str,
) {
    let children = graph.children(v);
    if children.is_empty() {
        if v != root {
            graph.set_edge_with(
                root,
                v,
                EdgeLabel {
                    weight: Some(0.0),
                    minlen: Some(node_sep),
                    ..EdgeLabel::default()
                },
            );
        }
        return;
    }

    let top = add_border_node(graph, "_bt", None, None);
    let bottom = add_border_node(graph, "_bb", None, None);

    graph
        .set_parent(&top, Some(v))
        .expect("the nesting graph is compound");
    if let Some(node) = graph.node_mut(v) {
        node.border_top = Some(top.clone());
    }
    graph
        .set_parent(&bottom, Some(v))
        .expect("the nesting graph is compound");
    if let Some(node) = graph.node_mut(v) {
        node.border_bottom = Some(bottom.clone());
    }

    for child in children {
        dfs(graph, root, node_sep, weight, height, depths, &child);

        let child_node = graph.node(&child).cloned().unwrap_or_default();
        let child_top = child_node
            .border_top
            .clone()
            .unwrap_or_else(|| child.clone());
        let child_bottom = child_node
            .border_bottom
            .clone()
            .unwrap_or_else(|| child.clone());
        let this_weight = if child_node.border_top.is_some() {
            weight
        } else {
            2.0 * weight
        };
        let minlen = if child_top != child_bottom {
            1.0
        } else {
            height - depths.get(v).copied().unwrap_or(0.0) + 1.0
        };

        graph.set_edge_with(
            &top,
            &child_top,
            EdgeLabel {
                weight: Some(this_weight),
                minlen: Some(minlen),
                nesting_edge: true,
                ..EdgeLabel::default()
            },
        );
        graph.set_edge_with(
            &child_bottom,
            &bottom,
            EdgeLabel {
                weight: Some(this_weight),
                minlen: Some(minlen),
                nesting_edge: true,
                ..EdgeLabel::default()
            },
        );
    }

    if graph.parent(v).is_none() {
        graph.set_edge_with(
            root,
            &top,
            EdgeLabel {
                weight: Some(0.0),
                minlen: Some(height + depths.get(v).copied().unwrap_or(0.0)),
                ..EdgeLabel::default()
            },
        );
    }
}

fn tree_depths(graph: &LayeredGraph) -> HashMap<String, f64> {
    fn dfs(graph: &LayeredGraph, v: &str, depth: f64, depths: &mut HashMap<String, f64>) {
        let children = graph.children(v);
        if !children.is_empty() {
            for child in children {
                dfs(graph, &child, depth + 1.0, depths);
            }
        }
        depths.insert(v.to_string(), depth);
    }

    let mut depths = HashMap::new();
    for v in graph.root_children() {
        dfs(graph, &v, 1.0, &mut depths);
    }
    depths
}

fn sum_weights(graph: &LayeredGraph) -> f64 {
    graph.edges().into_iter().fold(0.0, |sum, edge| {
        sum + graph.edge_object(&edge).map_or(0.0, EdgeLabel::weight)
    })
}

/// Removes the root, border edges, and the nesting attributes [`run`] added.
pub fn cleanup(graph: &mut LayeredGraph) {
    let root = graph.graph().and_then(|graph| graph.nesting_root.clone());
    if let Some(root) = root {
        graph.remove_node(&root);
    }
    if let Some(label) = graph.graph_mut() {
        label.nesting_root = None;
    }
    for edge in graph.edges() {
        if graph
            .edge_object(&edge)
            .is_some_and(|label| label.nesting_edge)
        {
            graph.remove_edge_object(&edge);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::graphlib::alg::components;
    use super::super::test_support::{compound, weighted};
    use super::super::{LayeredGraph, NodeLabel};

    fn graph() -> LayeredGraph {
        compound()
    }

    fn add_node(graph: &mut LayeredGraph, v: &str) {
        graph.set_node_with(v, NodeLabel::default());
    }

    #[test]
    fn connects_a_disconnected_graph() {
        let mut graph = graph();
        add_node(&mut graph, "a");
        add_node(&mut graph, "b");
        assert_eq!(components(&graph).len(), 2);
        super::run(&mut graph);
        assert_eq!(components(&graph).len(), 1);
        assert!(graph.has_node("a"));
        assert!(graph.has_node("b"));
    }

    #[test]
    fn adds_border_nodes_to_a_subgraph() {
        let mut graph = graph();
        add_node(&mut graph, "a");
        graph.set_parent("a", Some("sg1")).unwrap();
        super::run(&mut graph);
        let node = graph.node("sg1").unwrap().clone();
        let top = node.border_top.clone().unwrap();
        let bottom = node.border_bottom.clone().unwrap();
        assert_eq!(graph.parent(&top), Some("sg1"));
        assert_eq!(graph.parent(&bottom), Some("sg1"));
        assert_eq!(graph.out_edges(&top, Some("a")).unwrap().len(), 1);
        assert_eq!(graph.edge(&top, "a").unwrap().minlen(), 1.0);
        assert_eq!(graph.out_edges("a", Some(&bottom)).unwrap().len(), 1);
        assert_eq!(graph.edge("a", &bottom).unwrap().minlen(), 1.0);
        let top_label = graph.node(&top).unwrap();
        assert_eq!(top_label.width, 0.0);
        assert_eq!(top_label.height, 0.0);
        assert_eq!(top_label.dummy.as_deref(), Some("border"));
    }

    #[test]
    fn adds_edges_between_borders_of_nested_subgraphs() {
        let mut graph = graph();
        add_node(&mut graph, "sg2");
        add_node(&mut graph, "a");
        graph.set_parent("a", Some("sg2")).unwrap();
        graph.set_parent("sg2", Some("sg1")).unwrap();
        super::run(&mut graph);
        let sg1 = graph.node("sg1").unwrap().clone();
        let sg2 = graph.node("sg2").unwrap().clone();
        assert!(sg1.border_top.is_some());
        assert!(sg1.border_bottom.is_some());
        assert!(sg2.border_top.is_some());
        assert!(sg2.border_bottom.is_some());
        let sg1_top = sg1.border_top.unwrap();
        let sg1_bottom = sg1.border_bottom.unwrap();
        let sg2_top = sg2.border_top.unwrap();
        let sg2_bottom = sg2.border_bottom.unwrap();
        assert_eq!(graph.out_edges(&sg1_top, Some(&sg2_top)).unwrap().len(), 1);
        assert_eq!(graph.edge(&sg1_top, &sg2_top).unwrap().minlen(), 1.0);
        assert_eq!(
            graph
                .out_edges(&sg2_bottom, Some(&sg1_bottom))
                .unwrap()
                .len(),
            1
        );
        assert_eq!(graph.edge(&sg2_bottom, &sg1_bottom).unwrap().minlen(), 1.0);
    }

    #[test]
    fn adds_enough_weight_to_border_to_node_edges() {
        let mut graph = graph();
        add_node(&mut graph, "x");
        graph.set_parent("x", Some("sg")).unwrap();
        add_node(&mut graph, "a");
        add_node(&mut graph, "b");
        graph.set_edge_with("a", "x", weighted(100.0, 1.0));
        graph.set_edge_with("x", "b", weighted(200.0, 1.0));
        super::run(&mut graph);
        let node = graph.node("sg").unwrap();
        let top = node.border_top.clone().unwrap();
        let bottom = node.border_bottom.clone().unwrap();
        assert!(graph.edge(&top, "x").unwrap().weight() > 300.0);
        assert!(graph.edge("x", &bottom).unwrap().weight() > 300.0);
    }

    #[test]
    fn adds_an_edge_from_the_root_to_top_level_subgraph_tops() {
        let mut graph = graph();
        add_node(&mut graph, "a");
        graph.set_parent("a", Some("sg1")).unwrap();
        super::run(&mut graph);
        let root = graph.graph().unwrap().nesting_root.clone().unwrap();
        let top = graph.node("sg1").unwrap().border_top.clone().unwrap();
        assert_eq!(graph.out_edges(&root, Some(&top)).unwrap().len(), 1);
    }

    #[test]
    fn adds_a_root_edge_with_minlen_1() {
        let mut graph = graph();
        add_node(&mut graph, "a");
        super::run(&mut graph);
        let root = graph.graph().unwrap().nesting_root.clone().unwrap();
        assert_eq!(graph.out_edges(&root, Some("a")).unwrap().len(), 1);
        let label = graph.edge(&root, "a").unwrap();
        assert_eq!(label.weight(), 0.0);
        assert_eq!(label.minlen(), 1.0);
    }

    #[test]
    fn adds_a_root_edge_with_minlen_3() {
        let mut graph = graph();
        add_node(&mut graph, "a");
        graph.set_parent("a", Some("sg1")).unwrap();
        super::run(&mut graph);
        let root = graph.graph().unwrap().nesting_root.clone().unwrap();
        assert_eq!(graph.edge(&root, "a").unwrap().minlen(), 3.0);
    }

    #[test]
    fn adds_a_root_edge_with_minlen_5() {
        let mut graph = graph();
        add_node(&mut graph, "sg2");
        add_node(&mut graph, "a");
        graph.set_parent("a", Some("sg2")).unwrap();
        graph.set_parent("sg2", Some("sg1")).unwrap();
        super::run(&mut graph);
        let root = graph.graph().unwrap().nesting_root.clone().unwrap();
        assert_eq!(graph.edge(&root, "a").unwrap().minlen(), 5.0);
    }

    #[test]
    fn does_not_add_an_edge_from_the_root_to_itself() {
        let mut graph = graph();
        add_node(&mut graph, "a");
        super::run(&mut graph);
        let root = graph.graph().unwrap().nesting_root.clone().unwrap();
        assert_eq!(
            graph.out_edges(&root, Some(&root)).unwrap_or_default(),
            vec![]
        );
    }

    #[test]
    fn keeps_an_inter_node_edge_without_subgraphs() {
        let mut graph = graph();
        add_node(&mut graph, "a");
        add_node(&mut graph, "b");
        graph.set_edge_with("a", "b", weighted(1.0, 1.0));
        super::run(&mut graph);
        assert_eq!(graph.edge("a", "b").unwrap().minlen(), 1.0);
    }

    #[test]
    fn expands_inter_node_edges_for_a_subgraph() {
        let mut graph = graph();
        add_node(&mut graph, "a");
        graph.set_parent("a", Some("sg1")).unwrap();
        add_node(&mut graph, "b");
        graph.set_edge_with("a", "b", weighted(1.0, 1.0));
        super::run(&mut graph);
        assert_eq!(graph.edge("a", "b").unwrap().minlen(), 3.0);
    }

    #[test]
    fn expands_inter_node_edges_for_nested_subgraphs() {
        let mut graph = graph();
        add_node(&mut graph, "sg2");
        add_node(&mut graph, "a");
        graph.set_parent("a", Some("sg2")).unwrap();
        graph.set_parent("sg2", Some("sg1")).unwrap();
        add_node(&mut graph, "b");
        graph.set_edge_with("a", "b", weighted(1.0, 1.0));
        super::run(&mut graph);
        assert_eq!(graph.edge("a", "b").unwrap().minlen(), 5.0);
    }

    #[test]
    fn sets_minlen_for_nested_border_to_children() {
        let mut graph = graph();
        add_node(&mut graph, "a");
        graph.set_parent("a", Some("sg1")).unwrap();
        add_node(&mut graph, "sg2");
        add_node(&mut graph, "b");
        graph.set_parent("sg2", Some("sg1")).unwrap();
        graph.set_parent("b", Some("sg2")).unwrap();
        super::run(&mut graph);
        let root = graph.graph().unwrap().nesting_root.clone().unwrap();
        let sg1 = graph.node("sg1").unwrap().clone();
        let sg2 = graph.node("sg2").unwrap().clone();
        let sg1_top = sg1.border_top.unwrap();
        let sg1_bot = sg1.border_bottom.unwrap();
        let sg2_top = sg2.border_top.unwrap();
        let sg2_bot = sg2.border_bottom.unwrap();
        assert_eq!(graph.edge(&root, &sg1_top).unwrap().minlen(), 3.0);
        assert_eq!(graph.edge(&sg1_top, &sg2_top).unwrap().minlen(), 1.0);
        assert_eq!(graph.edge(&sg1_top, "a").unwrap().minlen(), 2.0);
        assert_eq!(graph.edge("a", &sg1_bot).unwrap().minlen(), 2.0);
        assert_eq!(graph.edge(&sg2_top, "b").unwrap().minlen(), 1.0);
        assert_eq!(graph.edge("b", &sg2_bot).unwrap().minlen(), 1.0);
        assert_eq!(graph.edge(&sg2_bot, &sg1_bot).unwrap().minlen(), 1.0);
    }

    #[test]
    fn cleanup_removes_nesting_edges() {
        let mut graph = graph();
        add_node(&mut graph, "a");
        graph.set_parent("a", Some("sg1")).unwrap();
        add_node(&mut graph, "b");
        graph.set_edge_with("a", "b", weighted(1.0, 1.0));
        super::run(&mut graph);
        super::cleanup(&mut graph);
        assert_eq!(graph.successors("a").unwrap(), vec!["b".to_string()]);
    }

    #[test]
    fn cleanup_removes_the_root_node() {
        let mut graph = graph();
        add_node(&mut graph, "a");
        graph.set_parent("a", Some("sg1")).unwrap();
        super::run(&mut graph);
        super::cleanup(&mut graph);
        assert_eq!(graph.node_count(), 4);
    }
}
