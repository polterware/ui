//! Port of `phase2.dummies.ts`: inserts a dummy node on every intermediate layer of a long
//! edge, so the proper layering has only edges between adjacent layers.

use std::collections::HashSet;

use super::helpers::{EdgeRef, Graph, Layering, NodeId, locale_compare};
use super::phase0_helpers::normalize_graph;

pub fn make_proper_layering(layering: &Layering, g_acyclic: &Graph) -> (Layering, Graph) {
    let g = normalize_graph(g_acyclic);
    let mut layers = layering.layers.clone();
    let mut dummy: HashSet<NodeId> = layering.dummy.clone();
    let mut dummy_order: Vec<NodeId> = layering.dummy.iter().cloned().collect();

    let mut dummy_seq = 0_usize;
    let mut node_by_id = g.node_by_id.clone();
    let mut new_edges: Vec<EdgeRef> = Vec::new();
    let mut rank_of = layering.rank_of.clone();

    // Sort edges deterministically to stabilize dummy creation order.
    let mut edges_sorted = g.edges.clone();
    edges_sorted.sort_by(|a, b| {
        if a.id == b.id {
            if a.src == b.src {
                locale_compare(&a.dst, &b.dst)
            } else {
                locale_compare(&a.src, &b.src)
            }
        } else {
            locale_compare(&a.id, &b.id)
        }
    });

    for edge in edges_sorted {
        let r_u = rank_of.get(&edge.src).copied().unwrap_or(0);
        let r_v = rank_of.get(&edge.dst).copied().unwrap_or(0);
        if r_v - r_u <= 1 {
            new_edges.push(edge);
            continue;
        }
        let mut prev = edge.src.clone();
        for (k, layer) in ((r_u + 1)..r_v).enumerate() {
            let dummy_id = format!("placeholder-{}", dummy_seq);
            dummy_seq += 1;
            let node = crate::render::pipeline::types::Node {
                id: dummy_id.clone(),
                is_group: Some(false),
                is_dummy: Some(true),
                width: Some(0.0),
                height: Some(0.0),
                ..Default::default()
            };
            node_by_id.insert(dummy_id.clone(), node);
            dummy.insert(dummy_id.clone());
            dummy_order.push(dummy_id.clone());
            while layers.len() <= layer as usize {
                layers.push(Vec::new());
            }
            layers[layer as usize].push(dummy_id.clone());
            rank_of.insert(dummy_id.clone(), layer);

            new_edges.push(EdgeRef {
                id: format!("{}#{}", edge.id, k),
                src: prev.clone(),
                dst: dummy_id.clone(),
                weight: edge.weight,
                reference: edge.reference.clone(),
            });
            prev = dummy_id;
        }
        let last_index = r_v - r_u - 2;
        new_edges.push(EdgeRef {
            id: format!("{}#{}", edge.id, (last_index + 1).max(0)),
            src: prev,
            dst: edge.dst.clone(),
            weight: edge.weight,
            reference: edge.reference.clone(),
        });
    }

    // Preserve the original node order, then dummies by creation order.
    let mut nodes = g.nodes.clone();
    for id in &dummy_order {
        if !nodes.contains(id) {
            nodes.push(id.clone());
        }
    }

    let graph_with_dummies = Graph {
        nodes,
        edges: new_edges,
        layout: g.layout.clone(),
        node_by_id,
    };

    (
        Layering {
            layers,
            rank_of,
            dummy,
        },
        graph_with_dummies,
    )
}
