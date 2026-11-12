//! Port of `lanes.ts`: which groups are lanes and pools, and which lane a node sits in.

use std::collections::{HashMap, HashSet};

use crate::render::pipeline::types::Node;

use super::helpers::metadata_value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaneRole {
    Pool,
    Lane,
}

pub fn read_lane_role(node: Option<&Node>) -> Option<LaneRole> {
    let role = node.and_then(|node| metadata_value(node, "laneRole"))?;
    match role.as_str() {
        Some("pool") => Some(LaneRole::Pool),
        Some("lane") => Some(LaneRole::Lane),
        _ => None,
    }
}

pub fn read_lane_index(node: Option<&Node>) -> Option<f64> {
    let index = node.and_then(|node| metadata_value(node, "laneIndex"))?;
    index.as_f64().filter(|index| index.is_finite())
}

/// The lane model `buildLaneModel` returns: pools, the lanes each pool holds, and the walk
/// from a node up to its lane.
#[derive(Clone, Debug, Default)]
pub struct LaneModel {
    pools: HashSet<String>,
    lanes: HashSet<String>,
    parents: HashMap<String, String>,
    pub lanes_by_pool: HashMap<String, Vec<String>>,
    pub has_pools: bool,
}

impl LaneModel {
    pub fn is_pool(&self, id: &str) -> bool {
        self.pools.contains(id)
    }

    pub fn is_lane(&self, id: &str) -> bool {
        self.lanes.contains(id)
    }

    fn walk(&self, id: &str, stop_at: &HashSet<String>) -> Option<String> {
        let mut seen: HashSet<&str> = HashSet::from([id]);
        let mut current = self.parents.get(id).map(String::as_str);
        while let Some(parent) = current {
            if seen.contains(parent) {
                break;
            }
            if stop_at.contains(parent) {
                return Some(parent.to_string());
            }
            seen.insert(parent);
            current = self.parents.get(parent).map(String::as_str);
        }
        None
    }

    pub fn lane_id_of(&self, id: &str) -> Option<String> {
        self.walk(id, &self.lanes)
    }

    pub fn pool_id_of(&self, id: &str) -> Option<String> {
        if self.pools.contains(id) {
            Some(id.to_string())
        } else {
            self.walk(id, &self.pools)
        }
    }
}

pub fn build_lane_model(nodes: &[Node]) -> LaneModel {
    let mut parents: HashMap<String, String> = HashMap::new();
    let mut group_child_count: HashMap<String, usize> = HashMap::new();
    for node in nodes {
        if let Some(parent) = &node.parent_id {
            parents.insert(node.id.clone(), parent.clone());
        }
        if node.is_group.unwrap_or(false)
            && let Some(parent) = &node.parent_id
        {
            *group_child_count.entry(parent.clone()).or_insert(0) += 1;
        }
    }

    let mut pools: HashSet<String> = HashSet::new();
    for node in nodes {
        let children = group_child_count.get(&node.id).copied().unwrap_or(0);
        if node.is_group.unwrap_or(false)
            && read_lane_role(Some(node)) == Some(LaneRole::Pool)
            && children > 0
        {
            pools.insert(node.id.clone());
        }
    }

    let mut lanes: HashSet<String> = HashSet::new();
    let mut lanes_by_pool: HashMap<String, Vec<String>> = HashMap::new();
    for node in nodes {
        if !node.is_group.unwrap_or(false) || pools.contains(&node.id) {
            continue;
        }
        if let Some(parent) = &node.parent_id {
            if pools.contains(parent) {
                lanes.insert(node.id.clone());
                lanes_by_pool
                    .entry(parent.clone())
                    .or_default()
                    .push(node.id.clone());
            }
        } else {
            lanes.insert(node.id.clone());
        }
    }

    let has_pools = !pools.is_empty();
    LaneModel {
        pools,
        lanes,
        parents,
        lanes_by_pool,
        has_pools,
    }
}

/// For callers that hold things in a lane: whether a group is a lane, as `buildLaneModel`
/// decides it.
pub fn is_lane_in(nodes: &[Node]) -> impl Fn(&Node) -> bool {
    let model = build_lane_model(nodes);
    move |group: &Node| model.is_lane(&group.id)
}
