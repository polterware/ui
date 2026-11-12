//! Port of `phase2.laneAwareCompact.ts`: one node per (layer, lane); inter-lane edges can stay
//! on the same layer.

use std::collections::{HashMap, HashSet};

use super::helpers::{Graph, Layering, NodeId};
use super::phase0_helpers::{
    build_in_degree_map, build_layers_from_ranks, build_sorted_successor_map, incoming,
    normalize_graph, sorted_zero_in_degree_nodes, topo_sort_if_acyclic,
};
use super::phase2_options::{LayeringOptions, create_top_lane_resolver};

fn topo_sort_by_generation_if_acyclic(g: &Graph) -> Option<Vec<NodeId>> {
    let mut in_degree = build_in_degree_map(g);
    let adj = build_sorted_successor_map(g);
    let mut frontier = sorted_zero_in_degree_nodes(g, &in_degree);
    let mut order: Vec<NodeId> = Vec::new();

    while !frontier.is_empty() {
        let mut next_frontier: Vec<NodeId> = Vec::new();
        for u in frontier {
            order.push(u.clone());
            if let Some(neighbors) = adj.get(&u) {
                for v in neighbors {
                    let degree = in_degree.entry(v.clone()).or_insert(0);
                    *degree = degree.saturating_sub(1);
                    if *degree == 0 {
                        next_frontier.push(v.clone());
                    }
                }
            }
        }
        next_frontier.sort_by(|a, b| super::helpers::locale_compare(a, b));
        frontier = next_frontier;
    }

    (order.len() == g.nodes.len()).then_some(order)
}

pub fn assign_layers_lane_aware_compact(g_acyclic: &Graph, opts: LayeringOptions) -> Layering {
    let g = normalize_graph(g_acyclic);
    let mut fallback = g.nodes.clone();
    fallback.sort();
    let order = if opts.direction == Some(super::helpers::Direction::Lr) {
        topo_sort_by_generation_if_acyclic(&g).unwrap_or(fallback)
    } else {
        topo_sort_if_acyclic(&g).unwrap_or(fallback)
    };

    // The lane id of a node: its top-level parent, or the node id when it has none.
    let top_lane_of = create_top_lane_resolver(&g);
    let lane_of = |id: &str| top_lane_of.lane_of(id).unwrap_or_else(|| id.to_string());

    let mut rank_of: HashMap<NodeId, i64> = HashMap::new();
    let mut next_free: HashMap<String, i64> = HashMap::new();
    let ignore_cross_lane = opts.ignore_cross_lane_edges.unwrap_or(true);
    let edge_weight = |u: &str, v: &str| -> i64 {
        if ignore_cross_lane {
            if lane_of(u) == lane_of(v) { 1 } else { 0 }
        } else {
            1
        }
    };

    for v in &order {
        let Some(node) = g.node_by_id.get(v) else {
            continue;
        };
        if node.is_group.unwrap_or(false) {
            continue; // do not assign ranks/capacity to lane/group containers
        }
        let predecessors = incoming(&g, v);
        let mut base = 0;
        for edge in predecessors {
            let u = &edge.src;
            let rank_u = rank_of.get(u).copied().unwrap_or(0);
            base = base.max(rank_u + edge_weight(u, v));
        }
        let lane = lane_of(v);
        let next = next_free.get(&lane).copied().unwrap_or(0);
        let layer = base.max(next);
        rank_of.insert(v.clone(), layer);
        next_free.insert(lane, layer + 1);
    }

    let layers = build_layers_from_ranks(&g, &order, &rank_of, true);

    Layering {
        layers,
        rank_of,
        dummy: HashSet::new(),
    }
}
