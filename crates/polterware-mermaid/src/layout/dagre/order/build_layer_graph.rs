//! Port of `dagre/lib/order/build-layer-graph.ts` (MIT, Copyright (c) 2012-2014
//! Chris Pettitt).
//!
//! Builds the graph for one rank: movable nodes keep their subgraph hierarchy under
//! a synthetic root, and their neighbours one rank away are copied without
//! hierarchy so barycenters have orders to read.

use super::super::super::graphlib::Graph;
use super::super::util;
use super::super::{EdgeLabel, GraphLabel, LayeredGraph, NodeLabel};
use super::Relationship;

/// Builds the ordering graph for `rank`.
///
/// `nodes_with_rank` restricts the movable nodes to those of the rank (and the
/// clusters spanning it); `None` uses every node of `graph`.
pub fn build_layer_graph(
    graph: &LayeredGraph,
    rank: f64,
    relationship: Relationship,
    nodes_with_rank: Option<&[String]>,
) -> LayeredGraph {
    let all_nodes = graph.nodes();
    let nodes_with_rank = nodes_with_rank.unwrap_or(&all_nodes);
    let root = create_root_node(graph);
    let mut result: LayeredGraph =
        Graph::with_options(super::super::super::graphlib::GraphOptions {
            compound: true,
            ..Default::default()
        });
    result.set_graph(GraphLabel {
        root: Some(root.clone()),
        ..GraphLabel::default()
    });

    for v in nodes_with_rank {
        let node = graph.node(v).cloned().unwrap_or_default();
        let parent = graph.parent(v).map(str::to_string);

        let in_rank = node.rank == Some(rank)
            || match (node.min_rank, node.max_rank) {
                (Some(min_rank), Some(max_rank)) => min_rank <= rank && rank <= max_rank,
                _ => false,
            };
        if in_rank {
            result.set_node_with(v.clone(), node.clone());
            let parent = parent
                .filter(|parent| !parent.is_empty())
                .unwrap_or(root.clone());
            result
                .set_parent(v, Some(parent.as_str()))
                .expect("the layer graph is compound");

            let edges = match relationship {
                Relationship::InEdges => graph.in_edges(v, None),
                Relationship::OutEdges => graph.out_edges(v, None),
            };
            for edge in edges.unwrap_or_default() {
                let u = if edge.v == *v { &edge.w } else { &edge.v };
                if !result.has_node(u) {
                    result.set_node_label(u.clone(), graph.node(u).cloned());
                }
                let previous_weight = result
                    .edge(u, v)
                    .map_or(0.0, |edge: &EdgeLabel| edge.weight());
                let weight = graph.edge_object(&edge).map_or(0.0, EdgeLabel::weight);
                result.set_edge_with(
                    u,
                    v,
                    EdgeLabel {
                        weight: Some(weight + previous_weight),
                        ..EdgeLabel::default()
                    },
                );
            }

            if node.min_rank.is_some() {
                let rank_index = rank.max(0.0) as usize;
                result.set_node_with(
                    v.clone(),
                    NodeLabel {
                        border_left_node: node.border_left.get(rank_index).cloned().flatten(),
                        border_right_node: node.border_right.get(rank_index).cloned().flatten(),
                        ..NodeLabel::default()
                    },
                );
            }
        }
    }

    result
}

fn create_root_node(graph: &LayeredGraph) -> String {
    loop {
        let v = util::unique_id("_root");
        if !graph.has_node(&v) {
            return v;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::test_support::{layering, weighted};
    use super::super::super::{NodeLabel, util};
    use super::super::Relationship;

    fn ranked(width: f64, rank: f64) -> NodeLabel {
        NodeLabel {
            width,
            rank: Some(rank),
            ..NodeLabel::default()
        }
    }

    fn rank_only(rank: f64) -> NodeLabel {
        NodeLabel {
            rank: Some(rank),
            ..NodeLabel::default()
        }
    }

    #[test]
    fn places_movable_nodes_without_parents_under_the_root() {
        let mut graph = layering();
        graph.set_node_with("a", rank_only(1.0));
        graph.set_node_with("b", rank_only(1.0));
        graph.set_node_with("c", rank_only(2.0));
        graph.set_node_with("d", rank_only(3.0));
        let layer = super::build_layer_graph(&graph, 1.0, Relationship::InEdges, None);
        let root = layer.graph().unwrap().root.clone().unwrap();
        assert!(layer.has_node(&root));
        assert_eq!(layer.root_children(), vec![root.clone()]);
        assert_eq!(
            layer.children(&root),
            vec!["a".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn copies_flat_nodes_from_the_layer() {
        let mut graph = layering();
        graph.set_node_with("a", rank_only(1.0));
        graph.set_node_with("b", rank_only(1.0));
        graph.set_node_with("c", rank_only(2.0));
        graph.set_node_with("d", rank_only(3.0));
        assert!(super::build_layer_graph(&graph, 1.0, Relationship::InEdges, None).has_node("a"));
        assert!(super::build_layer_graph(&graph, 1.0, Relationship::InEdges, None).has_node("b"));
        assert!(super::build_layer_graph(&graph, 2.0, Relationship::InEdges, None).has_node("c"));
        assert!(super::build_layer_graph(&graph, 3.0, Relationship::InEdges, None).has_node("d"));
    }

    #[test]
    fn copies_the_node_label_at_construction() {
        let mut graph = layering();
        graph.set_node_with("a", ranked(42.0, 1.0));
        graph.set_node_with("b", rank_only(2.0));
        graph.set_edge("a", "b");
        let layer = super::build_layer_graph(&graph, 2.0, Relationship::InEdges, None);
        assert_eq!(layer.node("a").unwrap().width, 42.0);
        graph.node_mut("a").unwrap().width = 24.0;
        // Unlike upstream's shared labels, the layer graph keeps the label it copied.
        assert_eq!(layer.node("a").unwrap().width, 42.0);
    }

    #[test]
    fn copies_edges_incident_on_rank_nodes_for_in_edges() {
        let mut graph = layering();
        graph.set_node_with("a", rank_only(1.0));
        graph.set_node_with("b", rank_only(1.0));
        graph.set_node_with("c", rank_only(2.0));
        graph.set_node_with("d", rank_only(3.0));
        graph.set_edge_with("a", "c", weighted(2.0, 1.0));
        graph.set_edge_with("b", "c", weighted(3.0, 1.0));
        graph.set_edge_with("c", "d", weighted(4.0, 1.0));
        assert_eq!(
            super::build_layer_graph(&graph, 1.0, Relationship::InEdges, None).edge_count(),
            0
        );
        let layer = super::build_layer_graph(&graph, 2.0, Relationship::InEdges, None);
        assert_eq!(layer.edge_count(), 2);
        assert_eq!(layer.edge("a", "c").unwrap().weight(), 2.0);
        assert_eq!(layer.edge("b", "c").unwrap().weight(), 3.0);
        let layer = super::build_layer_graph(&graph, 3.0, Relationship::InEdges, None);
        assert_eq!(layer.edge_count(), 1);
        assert_eq!(layer.edge("c", "d").unwrap().weight(), 4.0);
    }

    #[test]
    fn copies_edges_incident_on_rank_nodes_for_out_edges() {
        let mut graph = layering();
        graph.set_node_with("a", rank_only(1.0));
        graph.set_node_with("b", rank_only(1.0));
        graph.set_node_with("c", rank_only(2.0));
        graph.set_node_with("d", rank_only(3.0));
        graph.set_edge_with("a", "c", weighted(2.0, 1.0));
        graph.set_edge_with("b", "c", weighted(3.0, 1.0));
        graph.set_edge_with("c", "d", weighted(4.0, 1.0));
        let layer = super::build_layer_graph(&graph, 1.0, Relationship::OutEdges, None);
        assert_eq!(layer.edge_count(), 2);
        assert_eq!(layer.edge("c", "a").unwrap().weight(), 2.0);
        assert_eq!(layer.edge("c", "b").unwrap().weight(), 3.0);
        let layer = super::build_layer_graph(&graph, 2.0, Relationship::OutEdges, None);
        assert_eq!(layer.edge_count(), 1);
        assert_eq!(layer.edge("d", "c").unwrap().weight(), 4.0);
        assert_eq!(
            super::build_layer_graph(&graph, 3.0, Relationship::OutEdges, None).edge_count(),
            0
        );
    }

    #[test]
    fn collapses_multi_edges() {
        let mut graph = layering();
        graph.set_node_with("a", rank_only(1.0));
        graph.set_node_with("b", rank_only(2.0));
        graph.set_edge_with("a", "b", weighted(2.0, 1.0));
        graph
            .set_edge_full("a", "b", Some(weighted(3.0, 1.0)), Some("multi"))
            .unwrap();
        let layer = super::build_layer_graph(&graph, 2.0, Relationship::InEdges, None);
        assert_eq!(layer.edge("a", "b").unwrap().weight(), 5.0);
    }

    #[test]
    fn preserves_hierarchy_for_the_movable_layer() {
        let mut graph = layering();
        graph.set_node_with("a", rank_only(0.0));
        graph.set_node_with("b", rank_only(0.0));
        graph.set_node_with("c", rank_only(0.0));
        graph.set_node_with(
            "sg",
            NodeLabel {
                min_rank: Some(0.0),
                max_rank: Some(0.0),
                border_left: vec![Some("bl".to_string())],
                border_right: vec![Some("br".to_string())],
                ..NodeLabel::default()
            },
        );
        graph.set_parent("a", Some("sg")).unwrap();
        graph.set_parent("b", Some("sg")).unwrap();
        let layer = super::build_layer_graph(&graph, 0.0, Relationship::InEdges, None);
        let root = layer.graph().unwrap().root.clone().unwrap();
        let mut root_children = layer.children(&root);
        root_children.sort();
        assert_eq!(root_children, vec!["c".to_string(), "sg".to_string()]);
        assert_eq!(layer.parent("a"), Some("sg"));
        assert_eq!(layer.parent("b"), Some("sg"));
        assert_eq!(
            layer.node("sg").unwrap().border_left_node.as_deref(),
            Some("bl")
        );
    }

    #[test]
    fn only_includes_the_requested_rank_when_given() {
        let mut graph = layering();
        graph.set_node_with("a", rank_only(1.0));
        graph.set_node_with("b", rank_only(1.0));
        let layer =
            super::build_layer_graph(&graph, 1.0, Relationship::InEdges, Some(&["a".to_string()]));
        assert!(layer.has_node("a"));
        assert!(!layer.has_node("b"));
    }

    #[test]
    fn unique_roots_do_not_collide_with_graph_nodes() {
        let mut graph = layering();
        graph.set_node_with("_root1", rank_only(1.0));
        graph.set_node_with("a", rank_only(1.0));
        let root = graph.graph().unwrap().root.clone();
        let _ = root;
        let layer = super::build_layer_graph(&graph, 1.0, Relationship::InEdges, None);
        let root = layer.graph().unwrap().root.clone().unwrap();
        assert_ne!(root, "_root1");
        assert!(layer.has_node("_root1"));
        assert_eq!(util::max_rank(&graph), 1.0);
    }
}
