//! Port of `phase2.crossOptimization.ts`: greedily lifts nodes to reduce crossings while
//! keeping every edge's ranks ordered.

use std::collections::{HashMap, HashSet};

use super::config::LAYERING;
use super::helpers::{EdgeRef, Graph, NodeId};
use super::phase0_helpers::{
    build_layer_index, build_predecessor_successor_maps, count_inversions,
};
use super::phase2_multitree_order::build_multitree_layer_order;
use super::phase2_options::create_top_lane_resolver;

fn count_crossings_between_adjacent(
    upper: &[NodeId],
    lower: &[NodeId],
    edges: &[EdgeRef],
) -> usize {
    let upper_set: HashSet<&str> = upper.iter().map(String::as_str).collect();
    let lower_set: HashSet<&str> = lower.iter().map(String::as_str).collect();
    let lower_index = build_layer_index(lower);
    let mut values: Vec<f64> = Vec::new();
    for edge in edges {
        if upper_set.contains(edge.src.as_str())
            && lower_set.contains(edge.dst.as_str())
            && let Some(index) = lower_index.get(&edge.dst)
        {
            values.push(*index as f64);
        }
    }
    count_inversions(&mut values)
}

fn total_crossings(
    layers: &[Vec<NodeId>],
    edges: &[EdgeRef],
    rank_of: &HashMap<NodeId, i64>,
) -> usize {
    let mut expanded: Vec<EdgeRef> = Vec::new();
    for edge in edges {
        let (Some(&ru), Some(&rv)) = (rank_of.get(&edge.src), rank_of.get(&edge.dst)) else {
            continue;
        };
        if ru == rv {
            continue;
        }
        let (upper, lower, r_upper, r_lower) = if ru > rv {
            (&edge.dst, &edge.src, rv, ru)
        } else {
            (&edge.src, &edge.dst, ru, rv)
        };
        for layer in r_upper..r_lower {
            expanded.push(EdgeRef {
                id: format!("{}@{}", edge.id, layer),
                src: upper.clone(),
                dst: lower.clone(),
                weight: edge.weight,
                reference: edge.reference.clone(),
            });
        }
    }
    let mut sum = 0;
    for index in 0..layers.len().saturating_sub(1) {
        sum += count_crossings_between_adjacent(&layers[index], &layers[index + 1], &expanded);
    }
    sum
}

pub fn optimize_ranks_by_crossings(
    g: &Graph,
    initial_rank: &HashMap<NodeId, i64>,
) -> HashMap<NodeId, i64> {
    let mut rank_of = initial_rank.clone();
    let (preds, _) = build_predecessor_successor_maps(g, |_| true);
    let lane_of = create_top_lane_resolver(g);

    let mut layers = build_multitree_layer_order(g, &rank_of, &lane_of);
    let mut best = total_crossings(&layers, &g.edges, &rank_of);
    let max_passes = LAYERING.max_crossing_optimization_passes;
    for _ in 0..max_passes {
        let mut changed = false;
        let mut nodes_by_rank = g.nodes.clone();
        nodes_by_rank.sort_by(|a, b| {
            let ra = rank_of.get(a).copied().unwrap_or(0);
            let rb = rank_of.get(b).copied().unwrap_or(0);
            rb.cmp(&ra)
        });
        for v in nodes_by_rank {
            let rank = rank_of.get(&v).copied().unwrap_or(0);
            if rank == 0 {
                continue;
            }
            let mut lower_bound = 0;
            if let Some(predecessors) = preds.get(&v) {
                for u in predecessors {
                    lower_bound = lower_bound.max(rank_of.get(u).copied().unwrap_or(0) + 1);
                }
            }
            if lower_bound >= rank {
                continue;
            }
            let old = rank;
            rank_of.insert(v.clone(), lower_bound);
            layers = build_multitree_layer_order(g, &rank_of, &lane_of);
            let score = total_crossings(&layers, &g.edges, &rank_of);
            if score < best {
                best = score;
                changed = true;
            } else {
                rank_of.insert(v, old);
            }
        }
        if !changed {
            break;
        }
    }
    rank_of
}
