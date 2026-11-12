//! Port of `phase2.options.ts`: the shared layering options and the top-lane resolvers.

use std::collections::{HashMap, HashSet};

use super::helpers::{Direction, Graph, NodeId};
use super::lanes::{build_lane_model, read_lane_index};

/// Options controlling how layers are assigned in the Sugiyama pipeline.
#[derive(Clone, Copy, Debug, Default)]
pub struct LayeringOptions {
    /// If true, a node with exactly one incoming edge inherits its predecessor's layer.
    pub compact_single_input: Option<bool>,
    /// If true, ignore edges from other lanes when calculating layer positions.
    pub ignore_cross_lane_edges: Option<bool>,
    /// If true, try to lift nodes to reduce crossings between layers.
    pub optimize_ranks_by_crossings: Option<bool>,
    /// Diagram direction, used by lane-aware rank heuristics.
    pub direction: Option<Direction>,
}

/// Computes the "top lane" (outermost group container) for each node; `None` when the node
/// does not belong to any lane.
#[derive(Clone, Debug, Default)]
pub struct TopLaneResolver {
    lanes: HashMap<NodeId, Option<String>>,
}

impl TopLaneResolver {
    pub fn lane_of(&self, id: &str) -> Option<String> {
        self.lanes.get(id).cloned().flatten()
    }
}

pub fn build_top_lane_map(g: &Graph) -> HashMap<NodeId, Option<String>> {
    let model = build_lane_model(&g.layout.nodes);
    let mut cache = HashMap::new();
    for id in &g.nodes {
        cache.insert(id.clone(), model.lane_id_of(id));
    }
    cache
}

pub fn create_top_lane_resolver(g: &Graph) -> TopLaneResolver {
    TopLaneResolver {
        lanes: build_top_lane_map(g),
    }
}

fn unique_reversed(ids: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut unique: Vec<String> = Vec::new();
    for id in ids {
        if seen.insert(id.clone()) {
            unique.push(id);
        }
    }
    unique.reverse();
    unique
}

pub fn build_top_lane_order(g: &Graph) -> Vec<String> {
    let nodes = &g.layout.nodes;
    let model = build_lane_model(nodes);
    let lane_nodes: Vec<&NodeId> = nodes
        .iter()
        .filter(|node| model.is_lane(&node.id))
        .map(|node| &node.id)
        .collect();

    let indexed: Vec<&NodeId> = nodes
        .iter()
        .filter(|node| model.is_lane(&node.id) && read_lane_index(Some(node)).is_some())
        .map(|node| &node.id)
        .collect();
    if !indexed.is_empty() && indexed.len() == lane_nodes.len() {
        let mut indexed_lanes: Vec<&crate::render::pipeline::types::Node> = nodes
            .iter()
            .filter(|node| model.is_lane(&node.id) && read_lane_index(Some(node)).is_some())
            .collect();
        indexed_lanes.sort_by(|a, b| {
            read_lane_index(Some(a))
                .unwrap_or(0.0)
                .partial_cmp(&read_lane_index(Some(b)).unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        return indexed_lanes
            .into_iter()
            .map(|node| node.id.clone())
            .collect();
    }

    if !model.has_pools {
        return unique_reversed(lane_nodes.into_iter().cloned());
    }

    let mut lanes: Vec<String> = Vec::new();
    for node in nodes {
        if node.parent_id.is_some() {
            continue;
        }
        if model.is_pool(&node.id) {
            if let Some(pool_lanes) = model.lanes_by_pool.get(&node.id) {
                lanes.extend(pool_lanes.iter().cloned());
            }
        } else if model.is_lane(&node.id) {
            lanes.push(node.id.clone());
        }
    }
    unique_reversed(lanes)
}

pub fn resolve_top_lane_order(g: &Graph, preferred_order: Option<&[String]>) -> Vec<String> {
    let source_order = build_top_lane_order(g);
    let Some(preferred_order) = preferred_order.filter(|order| !order.is_empty()) else {
        return source_order;
    };

    let source_lane_ids: HashSet<&str> = source_order.iter().map(String::as_str).collect();
    let mut seen: HashSet<String> = HashSet::new();
    let mut resolved: Vec<String> = Vec::new();
    for lane_id in preferred_order {
        if !source_lane_ids.contains(lane_id.as_str()) || !seen.insert(lane_id.clone()) {
            continue;
        }
        resolved.push(lane_id.clone());
    }
    for lane_id in source_order {
        if seen.contains(&lane_id) {
            continue;
        }
        resolved.push(lane_id);
    }
    resolved
}
