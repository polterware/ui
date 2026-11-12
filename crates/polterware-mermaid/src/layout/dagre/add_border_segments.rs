//! Port of `dagre-d3-es/src/dagre/add-border-segments.js` (MIT, Copyright (c) 2012-2014 Chris
//! Pettitt).
//!
//! Gives every rank of a subgraph a left and a right border dummy, chained rank by
//! rank, so ordering and positioning can treat a subgraph as a box.

use super::util::add_dummy_node;
use super::{EdgeLabel, LayeredGraph, NodeLabel};

/// Adds border dummies for every subgraph that spans at least one rank.
pub fn run(graph: &mut LayeredGraph) {
    fn dfs(graph: &mut LayeredGraph, v: &str) {
        let children = graph.children(v);
        let node = graph.node(v).cloned();
        if !children.is_empty() {
            for child in children {
                dfs(graph, &child);
            }
        }

        if let Some(node) = node
            && node.min_rank.is_some()
        {
            let min_rank = node.min_rank.unwrap_or(0.0) as isize;
            let max_rank = node.max_rank.unwrap_or(0.0) as isize;
            if let Some(label) = graph.node_mut(v) {
                label.border_left = Vec::new();
                label.border_right = Vec::new();
            }
            for rank in min_rank..max_rank + 1 {
                add_border_node(graph, "borderLeft", "_bl", v, rank as f64);
                add_border_node(graph, "borderRight", "_br", v, rank as f64);
            }
        }
    }

    for v in graph.root_children() {
        dfs(graph, &v);
    }
}

fn add_border_node(graph: &mut LayeredGraph, prop: &str, prefix: &str, sg: &str, rank: f64) {
    let index = rank.max(0.0) as usize;
    let previous = graph.node(sg).and_then(|node| {
        if prop == "borderLeft" {
            node.border_left
                .get(index.wrapping_sub(1))
                .cloned()
                .flatten()
        } else {
            node.border_right
                .get(index.wrapping_sub(1))
                .cloned()
                .flatten()
        }
    });

    let label = NodeLabel {
        width: 0.0,
        height: 0.0,
        rank: Some(rank),
        border_type: Some(prop.to_string()),
        ..NodeLabel::default()
    };
    let current = add_dummy_node(graph, "border", label, prefix);

    if let Some(node) = graph.node_mut(sg) {
        if prop == "borderLeft" {
            grow(&mut node.border_left, index + 1);
            node.border_left[index] = Some(current.clone());
        } else {
            grow(&mut node.border_right, index + 1);
            node.border_right[index] = Some(current.clone());
        }
    }

    graph
        .set_parent(&current, Some(sg))
        .expect("border nodes are added to compound graphs");

    if let Some(previous) = previous {
        graph.set_edge_with(
            &previous,
            &current,
            EdgeLabel {
                weight: Some(1.0),
                ..EdgeLabel::default()
            },
        );
    }
}

fn grow(values: &mut Vec<Option<String>>, len: usize) {
    if values.len() < len {
        values.resize(len, None);
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::compound;
    use super::super::{LayeredGraph, NodeLabel};

    fn subgraph(min_rank: f64, max_rank: f64) -> NodeLabel {
        NodeLabel {
            min_rank: Some(min_rank),
            max_rank: Some(max_rank),
            ..NodeLabel::default()
        }
    }

    fn border_at(graph: &LayeredGraph, sg: &str, side: &str, rank: usize) -> String {
        let node = graph.node(sg).unwrap();
        let chain = if side == "left" {
            &node.border_left
        } else {
            &node.border_right
        };
        chain
            .get(rank)
            .cloned()
            .flatten()
            .expect("the border node exists")
    }

    #[test]
    fn does_not_add_border_nodes_for_a_non_compound_graph() {
        let mut graph = super::super::super::graphlib::Graph::<
            super::super::GraphLabel,
            NodeLabel,
            super::super::EdgeLabel,
        >::new();
        graph.set_node_with(
            "a",
            NodeLabel {
                rank: Some(0.0),
                ..NodeLabel::default()
            },
        );
        super::run(&mut graph);
        assert_eq!(graph.node_count(), 1);
        assert_eq!(graph.node("a").unwrap().rank, Some(0.0));
    }

    #[test]
    fn does_not_add_border_nodes_without_clusters() {
        let mut graph = compound();
        graph.set_node_with(
            "a",
            NodeLabel {
                rank: Some(0.0),
                ..NodeLabel::default()
            },
        );
        super::run(&mut graph);
        assert_eq!(graph.node_count(), 1);
        assert_eq!(graph.node("a").unwrap().rank, Some(0.0));
    }

    #[test]
    fn adds_a_border_for_a_single_rank_subgraph() {
        let mut graph = compound();
        graph.set_node_with("sg", subgraph(1.0, 1.0));
        super::run(&mut graph);
        let left = border_at(&graph, "sg", "left", 1);
        let right = border_at(&graph, "sg", "right", 1);
        let left_label = graph.node(&left).unwrap();
        assert_eq!(left_label.dummy.as_deref(), Some("border"));
        assert_eq!(left_label.border_type.as_deref(), Some("borderLeft"));
        assert_eq!(left_label.rank, Some(1.0));
        assert_eq!(left_label.width, 0.0);
        assert_eq!(left_label.height, 0.0);
        assert_eq!(graph.parent(&left), Some("sg"));
        assert_eq!(
            graph.node(&right).unwrap().border_type.as_deref(),
            Some("borderRight")
        );
        assert_eq!(graph.parent(&right), Some("sg"));
    }

    #[test]
    fn adds_a_border_for_a_multi_rank_subgraph() {
        let mut graph = compound();
        graph.set_node_with("sg", subgraph(1.0, 2.0));
        super::run(&mut graph);
        let left_one = border_at(&graph, "sg", "left", 1);
        let left_two = border_at(&graph, "sg", "left", 2);
        let right_one = border_at(&graph, "sg", "right", 1);
        let right_two = border_at(&graph, "sg", "right", 2);
        assert_eq!(graph.node(&left_two).unwrap().rank, Some(2.0));
        assert_eq!(graph.node(&right_two).unwrap().rank, Some(2.0));
        assert!(graph.has_edge(&left_one, &left_two));
        assert!(graph.has_edge(&right_one, &right_two));
    }

    #[test]
    fn adds_borders_for_nested_subgraphs() {
        let mut graph = compound();
        graph.set_node_with("sg1", subgraph(1.0, 1.0));
        graph.set_node_with("sg2", subgraph(1.0, 1.0));
        graph.set_parent("sg2", Some("sg1")).unwrap();
        super::run(&mut graph);
        for sg in ["sg1", "sg2"] {
            let left = border_at(&graph, sg, "left", 1);
            let right = border_at(&graph, sg, "right", 1);
            assert_eq!(graph.parent(&left), Some(sg));
            assert_eq!(graph.parent(&right), Some(sg));
            assert_eq!(graph.node(&left).unwrap().rank, Some(1.0));
            assert_eq!(graph.node(&right).unwrap().rank, Some(1.0));
        }
    }
}
