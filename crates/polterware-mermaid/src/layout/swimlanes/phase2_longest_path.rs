//! Port of `phase2.longestPath.ts`: classic longest-path layering with optional compaction
//! and lane awareness.

use std::collections::HashMap;

use super::helpers::{Graph, Layering, NodeId};
use super::phase0_helpers::{incoming, topo_sort_if_acyclic};
use super::phase2_cross_lane_adjust::adjust_cross_lane_sources;
use super::phase2_cross_optimization::optimize_ranks_by_crossings;
use super::phase2_multitree_order::build_multitree_layer_order;
use super::phase2_options::{LayeringOptions, create_top_lane_resolver};

pub fn assign_layers_longest_path(g_acyclic: &Graph, opts: LayeringOptions) -> Layering {
    let g = super::phase0_helpers::normalize_graph(g_acyclic);
    let order = topo_sort_if_acyclic(&g).unwrap_or_else(|| {
        let mut nodes = g.nodes.clone();
        nodes.sort();
        nodes
    });
    let compact = opts.compact_single_input.unwrap_or(false);

    let top_lane_of = create_top_lane_resolver(&g);

    let mut rank_of: HashMap<NodeId, i64> = HashMap::new();
    for v in &order {
        let inc_all = incoming(&g, v);
        let inc: Vec<_> = if opts.ignore_cross_lane_edges.unwrap_or(false) {
            inc_all
                .into_iter()
                .filter(|edge| {
                    let lane_src = top_lane_of.lane_of(&edge.src);
                    let lane_dst = top_lane_of.lane_of(v);
                    match (lane_src, lane_dst) {
                        (Some(lane_src), Some(lane_dst)) => lane_src == lane_dst,
                        _ => true,
                    }
                })
                .collect()
        } else {
            inc_all
        };
        let rank = if inc.is_empty() {
            0
        } else if compact && inc.len() == 1 {
            let u = &inc[0].src;
            let lane_u = top_lane_of.lane_of(u);
            let lane_v = top_lane_of.lane_of(v);
            if lane_u != lane_v {
                rank_of.get(u).copied().unwrap_or(0)
            } else {
                rank_of.get(u).copied().unwrap_or(0) + 1
            }
        } else {
            let mut max = f64::NEG_INFINITY;
            for edge in &inc {
                max = max.max(rank_of.get(&edge.src).copied().unwrap_or(0) as f64 + 1.0);
            }
            if max == f64::NEG_INFINITY {
                0
            } else {
                max as i64
            }
        };
        rank_of.insert(v.clone(), rank);
    }

    if opts.optimize_ranks_by_crossings.unwrap_or(false) {
        rank_of = optimize_ranks_by_crossings(&g, &rank_of);
    }

    if opts.ignore_cross_lane_edges.unwrap_or(false) {
        adjust_cross_lane_sources(&g, &mut rank_of);
    }

    let layers = build_multitree_layer_order(&g, &rank_of, &top_lane_of);

    Layering {
        layers,
        rank_of,
        dummy: Default::default(),
    }
}
