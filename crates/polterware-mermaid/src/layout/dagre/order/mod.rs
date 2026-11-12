//! Port of `dagre-d3-es/src/dagre/order/index.js` (MIT, Copyright (c) 2012-2014
//! Chris Pettitt).
//!
//! Minimizes edge crossings by sweeping the ranks up and down, ordering each rank
//! with barycenters and constraints, and keeping the layering with the fewest
//! crossings. dagre-d3-es accepts no options here: there is no `customOrder`,
//! `constraints`, or `disableOptimalOrderHeuristic`, and no previous layout.
//!
//! Upstream's layer graphs share node label objects with the layout graph, so a
//! sweep's `order` writes land on the main graph. This port copies labels, so
//! [`sweep_layer_graphs`] republishes the orders the shared object would have
//! carried: into the layer graph for every node, and back into the layout graph for
//! the nodes whose label the layer graph copied. A cluster's layer label is replaced
//! by its border references and therefore never carries an order, as upstream's.

pub mod add_subgraph_constraints;
pub mod barycenter;
pub mod build_layer_graph;
pub mod cross_count;
pub mod init_order;
pub mod resolve_conflicts;
pub mod sort;
pub mod sort_subgraph;

use super::super::graphlib::Graph;
use super::LayeredGraph;
use super::util::{self, LayerMatrix};

/// Which edges a layer graph copies from the layout graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relationship {
    /// Copy in-edges, so predecessors are on the previous layer (down sweeps).
    InEdges,
    /// Copy out-edges, so successors are on the previous layer (up sweeps).
    OutEdges,
}

/// Assigns `order` to every node to reduce edge crossings.
pub fn order(graph: &mut LayeredGraph) {
    let max_rank = util::max_rank(graph);
    let ranks: Vec<isize> = util::range(1, max_rank as isize + 1, 1);
    let reversed_ranks: Vec<isize> = util::range(max_rank as isize - 1, -1, -1);
    let mut down_layer_graphs = build_layer_graphs(graph, &ranks, Relationship::InEdges);
    let mut up_layer_graphs = build_layer_graphs(graph, &reversed_ranks, Relationship::OutEdges);

    let mut layering = init_order::init_order(graph);
    assign_order(graph, &layering);

    let mut best_cross_count = f64::INFINITY;
    let mut best: Option<LayerMatrix> = None;
    let mut i = 0usize;
    let mut last_best = 0usize;
    while last_best < 4 {
        let layer_graphs = if i % 2 == 1 {
            &mut down_layer_graphs
        } else {
            &mut up_layer_graphs
        };
        sweep_layer_graphs(layer_graphs, i % 4 >= 2, graph);

        layering = util::build_layer_matrix(graph);
        let cross_count = cross_count::cross_count(graph, &layering);
        if cross_count < best_cross_count {
            last_best = 0;
            best = Some(layering.clone());
            best_cross_count = cross_count;
        }
        i += 1;
        last_best += 1;
    }

    if let Some(best) = best {
        assign_order(graph, &best);
    }
}

fn build_layer_graphs(
    graph: &LayeredGraph,
    ranks: &[isize],
    relationship: Relationship,
) -> Vec<LayeredGraph> {
    ranks
        .iter()
        .map(|rank| build_layer_graph::build_layer_graph(graph, *rank as f64, relationship))
        .collect()
}

fn sweep_layer_graphs(
    layer_graphs: &mut [LayeredGraph],
    bias_right: bool,
    graph: &mut LayeredGraph,
) {
    let mut constraint_graph: Graph<(), (), ()> = Graph::new();
    for layer_graph in layer_graphs.iter_mut() {
        // Layer graph labels are copies: refresh the orders the shared labels would
        // have seen. A cluster's label was replaced by its border references, so it
        // carries no order in either graph.
        for v in layer_graph.nodes() {
            if is_cluster(graph, &v) {
                continue;
            }
            if let Some(order) = graph.node(&v).and_then(|node| node.order)
                && let Some(node) = layer_graph.node_mut(&v)
            {
                node.order = Some(order);
            }
        }

        let root = layer_graph
            .graph()
            .and_then(|label| label.root.clone())
            .unwrap_or_default();
        let sorted =
            sort_subgraph::sort_subgraph(layer_graph, &root, &constraint_graph, bias_right);
        for (index, v) in sorted.vs.iter().enumerate() {
            if let Some(node) = layer_graph.node_mut(v) {
                node.order = Some(index);
            }
            if !is_cluster(graph, v)
                && let Some(node) = graph.node_mut(v)
            {
                node.order = Some(index);
            }
        }
        add_subgraph_constraints::add_subgraph_constraints(
            layer_graph,
            &mut constraint_graph,
            &sorted.vs,
        );
    }
}

/// Whether `graph`'s label for `v` is the one a layer graph replaces: a cluster
/// label, which `buildLayerGraph` swaps for its border references.
fn is_cluster(graph: &LayeredGraph, v: &str) -> bool {
    graph.node(v).is_some_and(|node| node.min_rank.is_some())
}

fn assign_order(graph: &mut LayeredGraph, layering: &LayerMatrix) {
    for layer in layering {
        for (index, v) in layer.iter().enumerate() {
            if let Some(node) = graph.node_mut(v) {
                node.order = Some(index);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{graph, weighted};
    use super::super::util::{LayerMatrix, build_layer_matrix};
    use super::cross_count::cross_count;
    use super::order;

    fn ordered(graph: &mut super::super::LayeredGraph, v: &str, rank: f64) {
        graph.set_node_with(
            v,
            super::super::NodeLabel {
                rank: Some(rank),
                ..Default::default()
            },
        );
    }

    #[test]
    fn does_not_add_crossings_to_a_tree() {
        let mut graph = graph();
        ordered(&mut graph, "a", 1.0);
        for v in ["b", "e"] {
            ordered(&mut graph, v, 2.0);
        }
        for v in ["c", "d", "f"] {
            ordered(&mut graph, v, 3.0);
        }
        graph.set_path_with(&["a", "b", "c"], weighted(1.0, 1.0));
        graph.set_edge_with("b", "d", weighted(1.0, 1.0));
        graph.set_path_with(&["a", "e", "f"], weighted(1.0, 1.0));
        order(&mut graph);
        let layering: LayerMatrix = build_layer_matrix(&graph);
        assert_eq!(cross_count(&graph, &layering), 0.0);
    }

    #[test]
    fn can_solve_a_simple_graph() {
        let mut graph = graph();
        for v in ["a", "d"] {
            ordered(&mut graph, v, 1.0);
        }
        for v in ["b", "f", "e"] {
            ordered(&mut graph, v, 2.0);
        }
        for v in ["c", "g"] {
            ordered(&mut graph, v, 3.0);
        }
        order(&mut graph);
        let layering = build_layer_matrix(&graph);
        assert_eq!(cross_count(&graph, &layering), 0.0);
    }

    #[test]
    fn can_minimize_crossings() {
        let mut graph = graph();
        ordered(&mut graph, "a", 1.0);
        for v in ["b", "e", "g"] {
            ordered(&mut graph, v, 2.0);
        }
        for v in ["c", "f", "h"] {
            ordered(&mut graph, v, 3.0);
        }
        ordered(&mut graph, "d", 4.0);
        order(&mut graph);
        let layering = build_layer_matrix(&graph);
        assert!(cross_count(&graph, &layering) <= 1.0);
    }
}
