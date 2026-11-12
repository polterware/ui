//! Port of `phase2.crossLaneAdjust.ts`: pushes nodes whose outgoing edges all leave their
//! lane downward, so same-lane successors have room above them.

use std::collections::HashMap;

use super::helpers::{Graph, NodeId, locale_compare};
use super::phase2_options::create_top_lane_resolver;

pub fn adjust_cross_lane_sources(g: &Graph, rank_of: &mut HashMap<NodeId, i64>) {
    let top_lane_of = create_top_lane_resolver(g);

    let mut nodes_by_rank = g.nodes.clone();
    nodes_by_rank.sort_by(|a, b| {
        let ra = rank_of.get(a).copied().unwrap_or(0);
        let rb = rank_of.get(b).copied().unwrap_or(0);
        if ra != rb {
            ra.cmp(&rb)
        } else {
            locale_compare(a, b)
        }
    });
    for v in nodes_by_rank {
        let Some(lane_v) = top_lane_of.lane_of(&v) else {
            continue;
        };
        let out_edges: Vec<_> = g.edges.iter().filter(|edge| edge.src == v).collect();
        if out_edges.is_empty() {
            continue;
        }
        let mut has_same_lane_successor = false;
        let mut cross_lane_count = 0;
        for edge in out_edges {
            let lane_dst = top_lane_of.lane_of(&edge.dst);
            if lane_dst.is_none() || lane_dst.as_deref() == Some(lane_v.as_str()) {
                has_same_lane_successor = true;
            } else {
                cross_lane_count += 1;
            }
        }
        if cross_lane_count == 0 || has_same_lane_successor {
            continue;
        }

        let mut cross_lane_incoming = 0;
        let mut has_same_lane_predecessor = false;
        for edge in &g.edges {
            if edge.dst != v {
                continue;
            }
            let Some(lane_src) = top_lane_of.lane_of(&edge.src) else {
                continue;
            };
            if lane_src == lane_v {
                has_same_lane_predecessor = true;
            } else {
                cross_lane_incoming += 1;
            }
        }
        if cross_lane_incoming > 0 || !has_same_lane_predecessor {
            continue;
        }
        let current = rank_of.get(&v).copied().unwrap_or(0);
        let target = current + cross_lane_count as i64;
        // Still respect predecessor constraints.
        let mut lower_bound = 0;
        for edge in &g.edges {
            if edge.dst == v {
                lower_bound = lower_bound.max(rank_of.get(&edge.src).copied().unwrap_or(0) + 1);
            }
        }
        let new_rank = current.max(lower_bound).max(target);
        if new_rank != current {
            rank_of.insert(v, new_rank);
        }
    }
}
