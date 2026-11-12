//! Port of `phase2.multitree.order.ts`: emits layers by traversing the driving tree in an
//! order that tries to minimize crossings.

use std::collections::HashMap;

use super::driving_tree::{DrivingTreeBuildOptions, build_driving_tree};
use super::helpers::{Graph, NodeId, compare_by_rank_then_id, locale_compare};
use super::phase2_cross_counts::compute_subtree_cross_counts;
use super::phase2_multitree_core::{
    annotate_minimum_layers, deduplicate_layers, emit_nodes_in_tree_order,
};
use super::phase2_options::TopLaneResolver;

/// Orders children by crossing counts and minimum layers: children in future layers by their
/// minimum layer, children in the current layer by crossing counts.
fn create_child_orderer<'a>(
    children: &'a HashMap<NodeId, Vec<NodeId>>,
    rank_of: &'a HashMap<NodeId, i64>,
    cross_counts: &'a HashMap<NodeId, HashMap<NodeId, i64>>,
    min_layer: &'a HashMap<NodeId, i64>,
) -> impl Fn(&str) -> Vec<NodeId> + 'a {
    move |node: &str| -> Vec<NodeId> {
        let raw = children.get(node).cloned().unwrap_or_default();
        if raw.is_empty() {
            return Vec::new();
        }
        let layer = rank_of.get(node).copied().unwrap_or(0);
        let mut future: Vec<(NodeId, i64)> = Vec::new();
        let mut present: Vec<NodeId> = Vec::new();
        let cross_map = cross_counts.get(node);

        for child in raw {
            let min_l = min_layer.get(&child).copied().unwrap_or(layer);
            if min_l > layer {
                future.push((child, min_l));
            } else {
                present.push(child);
            }
        }

        future.sort_by(|a, b| {
            if a.1 == b.1 {
                locale_compare(&a.0, &b.0)
            } else {
                a.1.cmp(&b.1)
            }
        });

        present.sort_by(|a, b| {
            let ca = cross_map.and_then(|map| map.get(a)).copied().unwrap_or(0);
            let cb = cross_map.and_then(|map| map.get(b)).copied().unwrap_or(0);
            if ca != cb {
                return ca.cmp(&cb);
            }
            let ma = min_layer.get(a).copied().unwrap_or(layer);
            let mb = min_layer.get(b).copied().unwrap_or(layer);
            if ma != mb {
                return ma.cmp(&mb);
            }
            locale_compare(a, b)
        });

        future
            .into_iter()
            .map(|item| item.0)
            .chain(present)
            .collect()
    }
}

pub fn build_multitree_layer_order(
    g: &Graph,
    rank_of: &HashMap<NodeId, i64>,
    lane_of: &TopLaneResolver,
) -> Vec<Vec<NodeId>> {
    let tree = build_driving_tree(
        g,
        DrivingTreeBuildOptions {
            rank_hint: Some(rank_of.clone()),
            lane_resolver: Some(lane_of.clone()),
        },
    );
    let mut children = tree.children.clone();
    let roots = &tree.roots;

    // Ensure all nodes have a children entry.
    for node in &g.nodes {
        children.entry(node.clone()).or_default();
    }

    let cross_counts = compute_subtree_cross_counts(g, rank_of, &tree);

    let mut roots_sorted = roots.clone();
    roots_sorted.sort_by(compare_by_rank_then_id(rank_of));

    let min_layer = annotate_minimum_layers(&roots_sorted, &children, rank_of);
    let order_children = create_child_orderer(&children, rank_of, &cross_counts, &min_layer);
    let layers = emit_nodes_in_tree_order(&roots_sorted, &g.nodes, rank_of, order_children);

    deduplicate_layers(&layers)
}
