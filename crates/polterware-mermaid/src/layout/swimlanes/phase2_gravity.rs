//! Port of `phase2.gravity.ts`: gravity-based layering that minimizes edge lengths while
//! keeping the graph acyclic.

use std::collections::HashMap;

use super::config::LAYERING;
use super::helpers::{Graph, Layering, NodeId, js_round};
use super::phase0_helpers::{
    build_layers_from_ranks, build_predecessor_successor_maps, normalize_graph,
    topo_sort_if_acyclic,
};
use super::phase2_longest_path::assign_layers_longest_path;
use super::phase2_options::{LayeringOptions, create_top_lane_resolver};

fn clamp_feasible(
    v: &str,
    desired: i64,
    rank_of: &HashMap<NodeId, i64>,
    preds: &HashMap<NodeId, Vec<NodeId>>,
    succs: &HashMap<NodeId, Vec<NodeId>>,
) -> i64 {
    let mut lower_bound = 0;
    if let Some(predecessors) = preds.get(v) {
        for u in predecessors {
            lower_bound = lower_bound.max(rank_of.get(u).copied().unwrap_or(0) + 1);
        }
    }
    let successors = succs.get(v);
    let mut upper_bound = f64::INFINITY;
    if let Some(successors) = successors.filter(|successors| !successors.is_empty()) {
        upper_bound = super::phase0_helpers::min_of(
            successors
                .iter()
                .map(|w| rank_of.get(w).copied().unwrap_or(0) as f64 - 1.0),
        );
    }
    if !upper_bound.is_finite() {
        return lower_bound.max(desired);
    }
    desired.max(lower_bound).min(upper_bound as i64)
}

fn relax_order(
    node_order: &[NodeId],
    rank_of: &mut HashMap<NodeId, i64>,
    preds: &HashMap<NodeId, Vec<NodeId>>,
    succs: &HashMap<NodeId, Vec<NodeId>>,
) -> bool {
    let mut changed = false;
    for v in node_order {
        let empty: Vec<NodeId> = Vec::new();
        let ps = preds.get(v).unwrap_or(&empty);
        let ss = succs.get(v).unwrap_or(&empty);
        if ps.is_empty() && ss.is_empty() {
            continue;
        }
        let pred_avg = if ps.is_empty() {
            rank_of.get(v).copied().unwrap_or(0) as f64
        } else {
            ps.iter()
                .map(|u| rank_of.get(u).copied().unwrap_or(0) as f64 + 1.0)
                .sum::<f64>()
                / ps.len() as f64
        };
        let succ_avg = if ss.is_empty() {
            rank_of.get(v).copied().unwrap_or(0) as f64
        } else {
            ss.iter()
                .map(|w| rank_of.get(w).copied().unwrap_or(0) as f64 - 1.0)
                .sum::<f64>()
                / ss.len() as f64
        };
        let desired = js_round((pred_avg + succ_avg) / 2.0) as i64;
        let clamped = clamp_feasible(v, desired, rank_of, preds, succs);
        if clamped != rank_of.get(v).copied().unwrap_or(0) {
            rank_of.insert(v.clone(), clamped);
            changed = true;
        }
    }
    changed
}

pub fn assign_layers_gravity(g_acyclic: &Graph, opts: LayeringOptions) -> Layering {
    let g = normalize_graph(g_acyclic);
    let base = assign_layers_longest_path(
        &g,
        LayeringOptions {
            compact_single_input: opts.compact_single_input,
            ignore_cross_lane_edges: opts.ignore_cross_lane_edges,
            optimize_ranks_by_crossings: opts.optimize_ranks_by_crossings,
            direction: opts.direction,
        },
    );
    let mut rank_of = base.rank_of.clone();

    let top_lane_of = create_top_lane_resolver(&g);

    let (preds, succs) = build_predecessor_successor_maps(&g, |edge| {
        if opts.ignore_cross_lane_edges.unwrap_or(false)
            && let (Some(lane_src), Some(lane_dst)) = (
                top_lane_of.lane_of(&edge.src),
                top_lane_of.lane_of(&edge.dst),
            )
            && lane_src != lane_dst
        {
            return false;
        }
        true
    });

    let order = topo_sort_if_acyclic(&g).unwrap_or_else(|| g.nodes.clone());
    let rev_order: Vec<NodeId> = order.iter().rev().cloned().collect();

    let iterations = LAYERING.gravity_iterations;

    for _ in 0..iterations {
        let forward_changed = relax_order(&order, &mut rank_of, &preds, &succs);
        let backward_changed = relax_order(&rev_order, &mut rank_of, &preds, &succs);
        if !forward_changed && !backward_changed {
            break;
        }
    }

    // Final feasibility fix-ups (ensure r(v) >= r(u) + 1).
    for v in &order {
        let mut lower_bound = 0;
        if let Some(predecessors) = preds.get(v) {
            for u in predecessors {
                lower_bound = lower_bound.max(rank_of.get(u).copied().unwrap_or(0) + 1);
            }
        }
        if rank_of.get(v).copied().unwrap_or(0) < lower_bound {
            rank_of.insert(v.clone(), lower_bound);
        }
    }
    for v in &rev_order {
        if let Some(successors) = succs.get(v).filter(|successors| !successors.is_empty()) {
            let upper_bound = successors
                .iter()
                .map(|w| rank_of.get(w).copied().unwrap_or(0) - 1)
                .min()
                .unwrap_or(0);
            if rank_of.get(v).copied().unwrap_or(0) > upper_bound {
                rank_of.insert(v.clone(), upper_bound);
            }
        }
    }

    let layers = build_layers_from_ranks(&g, &order, &rank_of, false);

    Layering {
        layers,
        rank_of,
        dummy: Default::default(),
    }
}
