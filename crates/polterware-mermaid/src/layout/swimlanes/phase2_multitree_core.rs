//! Port of `phase2.multitree.core.ts`: the shared tree-ordering utilities.

use std::collections::{HashMap, HashSet};

use super::helpers::{NodeId, compare_by_rank_then_id};

pub fn annotate_minimum_layers(
    nodes: &[NodeId],
    children: &HashMap<NodeId, Vec<NodeId>>,
    rank_of: &HashMap<NodeId, i64>,
) -> HashMap<NodeId, i64> {
    let mut min_layer: HashMap<NodeId, i64> = HashMap::new();

    fn annotate(
        node: &str,
        children: &HashMap<NodeId, Vec<NodeId>>,
        rank_of: &HashMap<NodeId, i64>,
        min_layer: &mut HashMap<NodeId, i64>,
    ) {
        let mut min_l = rank_of.get(node).copied().unwrap_or(0);
        let mut child_list = children.get(node).cloned().unwrap_or_default();
        child_list.sort_by(compare_by_rank_then_id(rank_of));
        for child in child_list {
            annotate(&child, children, rank_of, min_layer);
            if let Some(child_min) = min_layer.get(&child) {
                min_l = min_l.min(*child_min);
            }
        }
        min_layer.insert(node.to_string(), min_l);
    }

    for node in nodes {
        annotate(node, children, rank_of, &mut min_layer);
    }

    min_layer
}

/// Emits nodes into layers in tree traversal order, using `order_children` for the order of a
/// node's children.
pub fn emit_nodes_in_tree_order(
    roots: &[NodeId],
    all_nodes: &[NodeId],
    rank_of: &HashMap<NodeId, i64>,
    order_children: impl Fn(&str) -> Vec<NodeId>,
) -> Vec<Vec<NodeId>> {
    let mut max_rank: i64 = 0;
    for node in all_nodes {
        let rank = rank_of.get(node).copied().unwrap_or(0);
        if rank > max_rank {
            max_rank = rank;
        }
    }

    let mut layers: Vec<Vec<NodeId>> = vec![Vec::new(); max_rank.max(0) as usize + 1];
    let mut emitted: HashSet<NodeId> = HashSet::new();

    fn emit(
        node: &str,
        layers: &mut Vec<Vec<NodeId>>,
        emitted: &mut HashSet<NodeId>,
        rank_of: &HashMap<NodeId, i64>,
        order_children: &impl Fn(&str) -> Vec<NodeId>,
    ) {
        if !emitted.insert(node.to_string()) {
            return;
        }
        let layer = rank_of.get(node).copied().unwrap_or(0).max(0) as usize;
        if layer >= layers.len() {
            layers.resize(layer + 1, Vec::new());
        }
        layers[layer].push(node.to_string());
        for child in order_children(node) {
            emit(&child, layers, emitted, rank_of, order_children);
        }
    }

    for root in roots {
        emit(root, &mut layers, &mut emitted, rank_of, &order_children);
    }

    // Fallback for isolated nodes (if any were not connected through the spanning forest).
    for node in all_nodes {
        if !emitted.contains(node) {
            let layer = rank_of.get(node).copied().unwrap_or(0).max(0) as usize;
            if layer >= layers.len() {
                layers.resize(layer + 1, Vec::new());
            }
            layers[layer].push(node.clone());
            emitted.insert(node.clone());
        }
    }

    layers
}

/// Removes duplicate nodes from each layer while preserving order.
pub fn deduplicate_layers(layers: &[Vec<NodeId>]) -> Vec<Vec<NodeId>> {
    let mut result = Vec::with_capacity(layers.len());
    for layer in layers {
        let mut seen = HashSet::new();
        let mut deduped = Vec::new();
        for id in layer {
            if seen.insert(id.clone()) {
                deduped.push(id.clone());
            }
        }
        result.push(deduped);
    }
    result
}
