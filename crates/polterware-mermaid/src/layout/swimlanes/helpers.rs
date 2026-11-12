//! Port of `helpers.ts`: the graph view, the layout types the phases share, and the write-back
//! from Sugiyama coordinates to [`LayoutData`].

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::render::pipeline::types::{Edge as MermaidEdge, LayoutData, Node};

use super::anchored_nodes::{collect_anchored_ids, resolve_anchor_host_id};
use super::lanes::{LaneModel, build_lane_model};

pub type Layout = LayoutData;
pub type NodeId = String;
pub type EdgeId = String;

/// The diagram direction; upstream's `'TB' | 'LR' | 'BT' | 'RL'`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    #[default]
    Tb,
    Lr,
    Bt,
    Rl,
}

impl Direction {
    /// Reads a layout's direction string; anything else is TB, as upstream's cast behaves.
    pub fn parse(value: Option<&str>) -> Direction {
        match value {
            Some("LR") => Direction::Lr,
            Some("BT") => Direction::Bt,
            Some("RL") => Direction::Rl,
            _ => Direction::Tb,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Direction::Tb => "TB",
            Direction::Lr => "LR",
            Direction::Bt => "BT",
            Direction::Rl => "RL",
        }
    }

    /// Whether ranks advance along X (`LR` and `RL`).
    pub fn is_horizontal(self) -> bool {
        matches!(self, Direction::Lr | Direction::Rl)
    }
}

/// `EdgeRef`: an edge of the graph view, with the layout edge it came from.
#[derive(Clone, Debug, PartialEq)]
pub struct EdgeRef {
    pub id: EdgeId,
    pub src: NodeId,
    pub dst: NodeId,
    pub weight: Option<f64>,
    pub reference: MermaidEdge,
}

/// Upstream's `type Edge = EdgeRef`.
pub type Edge = EdgeRef;

/// The graph the Sugiyama phases lay out. `layout` is the [`LayoutData`] the graph was read
/// from; the caller copies it back after [`write_back_to_layout_data`], because upstream shares
/// the node objects with the layout data.
#[derive(Clone, Debug, Default)]
pub struct Graph {
    pub nodes: Vec<NodeId>,
    pub edges: Vec<EdgeRef>,
    pub layout: LayoutData,
    pub node_by_id: HashMap<NodeId, Node>,
}

impl Graph {
    /// A graph over every node and edge of `layout`, in declaration order.
    pub fn from_layout(layout: &LayoutData) -> Self {
        let nodes = layout.nodes.iter().map(|node| node.id.clone()).collect();
        let node_by_id = layout
            .nodes
            .iter()
            .map(|node| (node.id.clone(), node.clone()))
            .collect();
        let edges = layout
            .edges
            .iter()
            .filter_map(|edge| {
                let src = edge.start.clone()?;
                let dst = edge.end.clone()?;
                Some(EdgeRef {
                    id: edge.id.clone(),
                    src,
                    dst,
                    weight: None,
                    reference: edge.clone(),
                })
            })
            .collect();
        Self {
            nodes,
            edges,
            layout: layout.clone(),
            node_by_id,
        }
    }

    pub fn node(&self, id: &str) -> Option<&Node> {
        self.node_by_id.get(id)
    }

    pub fn node_mut(&mut self, id: &str) -> Option<&mut Node> {
        self.node_by_id.get_mut(id)
    }

    pub fn layout_node(&self, id: &str) -> Option<&Node> {
        self.layout.nodes.iter().find(|node| node.id == id)
    }

    pub fn layout_node_mut(&mut self, id: &str) -> Option<&mut Node> {
        self.layout.nodes.iter_mut().find(|node| node.id == id)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layering {
    pub layers: Vec<Vec<NodeId>>,
    pub rank_of: HashMap<NodeId, i64>,
    pub dummy: HashSet<NodeId>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OrderedLayers {
    pub layers: Vec<Vec<NodeId>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Coordinates {
    pub x: HashMap<NodeId, f64>,
    pub y: HashMap<NodeId, f64>,
}

pub const DEFAULT_SWIMLANE_ID: &str = "__swimlane_default__";

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WriteBackOptions {
    pub layer_gap: Option<f64>,
    pub node_gap: Option<f64>,
}

/// The captured fixtures' stable single-line label height.
const TOP_LANE_TITLE_BAND_HEIGHT: f64 = 21.0;
const MIN_TOP_LANE_HORIZONTAL_PADDING: f64 = 20.0;

/// `Math.round`: rounds `.5` towards positive infinity, where `f64::round` rounds away from zero.
pub fn js_round(value: f64) -> f64 {
    (value + 0.5).floor()
}

/// `String.prototype.localeCompare` for the identifiers a diagram carries: letters compare
/// case-insensitively, a lowercase letter before its uppercase form, and an uppercase letter
/// before the next lowercase letter. Punctuation and digits compare by code point.
pub fn locale_compare(a: &str, b: &str) -> Ordering {
    let left: Vec<char> = a.chars().collect();
    let right: Vec<char> = b.chars().collect();
    let mut index = 0;
    while index < left.len() && index < right.len() {
        let la = left[index].to_lowercase().next().unwrap_or(left[index]);
        let lb = right[index].to_lowercase().next().unwrap_or(right[index]);
        match la.cmp(&lb) {
            Ordering::Equal => {}
            other => return other,
        }
        index += 1;
    }
    match left.len().cmp(&right.len()) {
        Ordering::Equal => {}
        other => return other,
    }
    // Equal ignoring case: compare the raw characters, putting lowercase first, as ICU does.
    for (ca, cb) in a.chars().zip(b.chars()) {
        match (ca.is_uppercase(), cb.is_uppercase()) {
            (false, true) => return Ordering::Less,
            (true, false) => return Ordering::Greater,
            _ => match ca.cmp(&cb) {
                Ordering::Equal => {}
                other => return other,
            },
        }
    }
    Ordering::Equal
}

/// `compareByRankThenId` of `phase2.multitree.core.ts`.
pub fn compare_by_rank_then_id(
    rank_of: &HashMap<NodeId, i64>,
) -> impl FnMut(&NodeId, &NodeId) -> Ordering {
    move |a, b| {
        let ra = rank_of.get(a).copied().unwrap_or(0);
        let rb = rank_of.get(b).copied().unwrap_or(0);
        if ra == rb {
            locale_compare(a, b)
        } else {
            ra.cmp(&rb)
        }
    }
}

pub fn node_is_group(node: &Node) -> bool {
    node.is_group.unwrap_or(false)
}

pub fn node_is_edge_label(node: &Node) -> bool {
    node.is_edge_label.unwrap_or(false)
}

pub fn node_is_dummy(node: &Node) -> bool {
    node.is_dummy.unwrap_or(false)
}

/// The `metadata` object of a node, if it has one.
pub fn metadata(node: &Node) -> Option<&serde_json::Map<String, Value>> {
    node.metadata.as_ref().and_then(Value::as_object)
}

pub fn metadata_value<'a>(node: &'a Node, key: &str) -> Option<&'a Value> {
    metadata(node).and_then(|map| map.get(key))
}

/// Stores a number under a node's metadata; upstream sets ad-hoc properties such as
/// `swimlaneContentTop` and `direction` on the node object itself, which the shared
/// [`Node`] type has no field for.
pub fn set_metadata_number(node: &mut Node, key: &str, value: f64) {
    if !node.metadata.as_ref().is_some_and(Value::is_object) {
        node.metadata = Some(Value::Object(serde_json::Map::new()));
    }
    if let Some(Value::Object(map)) = node.metadata.as_mut() {
        map.insert(key.to_string(), Value::from(value));
    }
}

pub fn set_metadata_string(node: &mut Node, key: &str, value: &str) {
    if !node.metadata.as_ref().is_some_and(Value::is_object) {
        node.metadata = Some(Value::Object(serde_json::Map::new()));
    }
    if let Some(Value::Object(map)) = node.metadata.as_mut() {
        map.insert(key.to_string(), Value::from(value));
    }
}

pub fn metadata_number(node: &Node, key: &str) -> Option<f64> {
    metadata_value(node, key).and_then(Value::as_f64)
}

fn top_lane_horizontal_padding(lane: &Node) -> f64 {
    lane.padding
        .unwrap_or(MIN_TOP_LANE_HORIZONTAL_PADDING)
        .max(MIN_TOP_LANE_HORIZONTAL_PADDING)
}

fn assign_top_lane_title_rect(lane: &mut Node) {
    let (Some(x), Some(y), Some(width), Some(height)) = (lane.x, lane.y, lane.width, lane.height)
    else {
        lane.group_title_rect = None;
        return;
    };
    let Some(content_top) = metadata_number(lane, "swimlaneContentTop") else {
        lane.group_title_rect = None;
        return;
    };
    if !x.is_finite()
        || !y.is_finite()
        || !width.is_finite()
        || !height.is_finite()
        || !content_top.is_finite()
        || width <= 0.0
        || height <= 0.0
    {
        lane.group_title_rect = None;
        return;
    }

    let top = y - height / 2.0;
    let header_bottom = content_top.min(y + height / 2.0);
    let title_height = TOP_LANE_TITLE_BAND_HEIGHT.min((header_bottom - top).max(0.0));
    let bottom = top + title_height;
    if bottom <= top {
        lane.group_title_rect = None;
        return;
    }

    lane.group_title_rect = Some(crate::render::pipeline::types::GroupTitleRect {
        left: x - width / 2.0,
        right: x + width / 2.0,
        top,
        bottom,
    });
}

/// Mirrors `DroppedSubGraphMember` in the flowchart types, which a rendering-util module must
/// not import. [`shared_members`] reads them back from the layout's open-ended `other` bag.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SharedMember {
    pub node: String,
    pub kept_by: String,
    pub dropped_from: String,
}

/// `sharedMembers`: `layout.other?.droppedSubGraphMembers`, or nothing when absent.
pub fn shared_members(layout: &LayoutData) -> Vec<SharedMember> {
    layout
        .other
        .as_ref()
        .and_then(|other| other.get("droppedSubGraphMembers"))
        .and_then(serde_json::Value::as_array)
        .map(|members| {
            members
                .iter()
                .filter_map(|member| {
                    Some(SharedMember {
                        node: member.get("node")?.as_str()?.to_string(),
                        kept_by: member.get("keptBy")?.as_str()?.to_string(),
                        dropped_from: member.get("droppedFrom")?.as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn is_empty_lane(layout: &LayoutData, nodes: &[Node], lane_id: &str) -> bool {
    !nodes
        .iter()
        .any(|node| node.parent_id.as_deref() == Some(lane_id))
        && !layout.edges.iter().any(|edge| {
            edge.start.as_deref() == Some(lane_id) || edge.end.as_deref() == Some(lane_id)
        })
}

fn lane_id_or_self(model: &LaneModel, id: &str) -> Option<String> {
    if model.is_lane(id) {
        Some(id.to_string())
    } else {
        model.lane_id_of(id)
    }
}

fn remove_empty_lanes(layout: &LayoutData, nodes: &mut Vec<Node>, lane_ids: &HashSet<String>) {
    for lane_id in lane_ids {
        if let Some(index) = nodes.iter().position(|node| &node.id == lane_id)
            && is_empty_lane(layout, nodes, lane_id)
        {
            nodes.remove(index);
        }
    }
}

/// A node listed in two lanes stays in the first, so the second can end up with nothing. The
/// lanes it emptied are removed; a lane the author left empty lost no member and stays, and so
/// does one an edge points at. The lane that kept the node can be the one around a subgraph
/// nested in it, so `kept_by` is resolved up to its lane.
///
/// Upstream logs a warning for each shared node; this crate has no logger, so the lint only
/// changes the layout.
fn drop_lanes_emptied_by_shared_nodes(
    layout: &LayoutData,
    nodes: &mut Vec<Node>,
    members: &[SharedMember],
) {
    let model = build_lane_model(nodes);
    let lane_ids: HashSet<String> = nodes
        .iter()
        .filter(|node| model.is_lane(&node.id))
        .map(|lane| lane.id.clone())
        .collect();
    let mut emptied = HashSet::new();
    for member in members {
        let kept_by_lane = lane_id_or_self(&model, &member.kept_by);
        if let Some(kept_by_lane) = kept_by_lane
            && kept_by_lane != member.dropped_from
            && lane_ids.contains(&member.dropped_from)
        {
            emptied.insert(member.dropped_from.clone());
        }
    }
    remove_empty_lanes(layout, nodes, &emptied);
}

/// Upstream warns about a group nested in a lane that is drawn as a plain box; this crate has
/// no logger, so the walk exists only to keep the module shape.
fn warn_about_nested_groups(_nodes: &[Node], _model: &LaneModel) {}

pub fn prepare_layout_for_swimlanes(layout: &mut LayoutData) {
    let direction = layout.direction.clone();
    let mut nodes = std::mem::take(&mut layout.nodes);
    let dropped_members = shared_members(layout);
    drop_lanes_emptied_by_shared_nodes(layout, &mut nodes, &dropped_members);
    let lane_model = build_lane_model(&nodes);
    warn_about_nested_groups(&nodes, &lane_model);
    for node in &mut nodes {
        let is_lane = lane_model.is_lane(&node.id);
        if !is_lane && !lane_model.is_pool(&node.id) {
            continue;
        }
        node.shape = Some(if is_lane { "swimlane" } else { "pool" }.to_string());
        if let Some(direction) = direction.as_deref() {
            set_metadata_string(node, "direction", direction);
        }
    }

    let loose_ids: Vec<String> = nodes
        .iter()
        .filter(|node| !node_is_group(node) && node.parent_id.is_none())
        .map(|node| node.id.clone())
        .collect();
    if loose_ids.is_empty() {
        layout.nodes = nodes;
        return;
    }

    let existing = nodes.iter().position(|node| node.id == DEFAULT_SWIMLANE_ID);
    match existing {
        None => {
            let max_color = nodes
                .iter()
                .map(|node| node.color_index.map(|index| index as f64).unwrap_or(-1.0))
                .fold(-1.0_f64, f64::max);
            let look = if layout.config.look.is_empty() {
                None
            } else {
                Some(layout.config.look.clone())
            };
            let mut default_lane = Node {
                id: DEFAULT_SWIMLANE_ID.to_string(),
                label: Some(String::new()),
                is_group: Some(true),
                shape: Some("swimlane".to_string()),
                padding: Some(20.0),
                look,
                color_index: Some((max_color + 1.0) as usize),
                ..Default::default()
            };
            if let Some(direction) = direction.as_deref() {
                set_metadata_string(&mut default_lane, "direction", direction);
            }
            nodes.push(default_lane);
        }
        Some(index) => {
            if node_is_group(&nodes[index]) {
                nodes[index].shape = Some("swimlane".to_string());
                if let Some(direction) = direction.as_deref() {
                    set_metadata_string(&mut nodes[index], "direction", direction);
                }
            }
        }
    }

    for node in &mut nodes {
        if loose_ids.contains(&node.id) {
            node.parent_id = Some(DEFAULT_SWIMLANE_ID.to_string());
        }
    }
    layout.nodes = nodes;
}

/// The graph the Sugiyama phases lay out. Edge-label nodes and their layout-only edges are
/// left out, and a labelled edge takes part as itself, so a label never changes a node's rank.
pub fn to_graph_view(layout: &LayoutData) -> Graph {
    let all_nodes: Vec<&Node> = layout
        .nodes
        .iter()
        .filter(|node| !node_is_edge_label(node))
        .collect();
    let by_id: HashMap<NodeId, Node> = all_nodes
        .iter()
        .map(|node| (node.id.clone(), (*node).clone()))
        .collect();

    let anchored_ids = collect_anchored_ids(layout.nodes.as_slice());
    let host_of = |id: &str| -> NodeId {
        if anchored_ids.contains(id) {
            resolve_anchor_host_id(id, &by_id).unwrap_or_else(|| id.to_string())
        } else {
            id.to_string()
        }
    };

    let mut node_by_id = HashMap::new();
    for node in &all_nodes {
        if !anchored_ids.contains(&node.id) {
            node_by_id.insert(node.id.clone(), (*node).clone());
        }
    }

    let mut edges = Vec::new();
    for edge in &layout.edges {
        let (Some(raw_src), Some(raw_dst)) = (edge.start.as_deref(), edge.end.as_deref()) else {
            continue;
        };
        if edge.is_layout_only.unwrap_or(false) {
            continue;
        }
        let src = host_of(raw_src);
        let dst = host_of(raw_dst);
        if src == dst {
            continue;
        }
        edges.push(EdgeRef {
            id: edge.id.clone(),
            src,
            dst,
            weight: None,
            reference: edge.clone(),
        });
    }

    let group_nodes: Vec<&Node> = all_nodes
        .iter()
        .copied()
        .filter(|node| node_is_group(node) && !anchored_ids.contains(&node.id))
        .rev()
        .collect();
    let non_group_nodes: Vec<&Node> = all_nodes
        .iter()
        .copied()
        .filter(|node| !node_is_group(node) && !anchored_ids.contains(&node.id))
        .collect();
    let nodes = group_nodes
        .into_iter()
        .chain(non_group_nodes)
        .map(|node| node.id.clone())
        .collect();

    Graph {
        nodes,
        edges,
        layout: layout.clone(),
        node_by_id,
    }
}

fn groups_deepest_first(all_nodes: &[Node]) -> Vec<String> {
    let by_id: HashMap<&str, &Node> = all_nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let depth_of = |node: &Node| {
        let mut depth = 0_i64;
        let mut parent_id = node.parent_id.as_deref();
        let mut seen: HashSet<&str> = HashSet::from([node.id.as_str()]);
        while let Some(parent) = parent_id {
            if seen.contains(parent) {
                break;
            }
            seen.insert(parent);
            depth += 1;
            parent_id = by_id.get(parent).and_then(|node| node.parent_id.as_deref());
        }
        depth
    };
    let mut groups: Vec<(String, i64)> = all_nodes
        .iter()
        .filter(|node| node_is_group(node))
        .map(|node| (node.id.clone(), depth_of(node)))
        .collect();
    groups.sort_by(|a, b| b.1.cmp(&a.1));
    groups.into_iter().map(|entry| entry.0).collect()
}

/// Writes the phases' coordinates into the layout: every placed node, each label's seed
/// position, every group's frame, and the lanes' shared header band.
pub fn write_back_to_layout_data(
    g: &mut Graph,
    ordered: &OrderedLayers,
    coords: &Coordinates,
    opts: Option<WriteBackOptions>,
) {
    let layer_gap = opts.and_then(|opts| opts.layer_gap).unwrap_or(100.0);
    let node_gap = opts.and_then(|opts| opts.node_gap).unwrap_or(40.0);

    for (layer_index, layer) in ordered.layers.iter().enumerate() {
        for (order_index, id) in layer.iter().enumerate() {
            let x = coords
                .x
                .get(id)
                .copied()
                .unwrap_or(order_index as f64 * node_gap);
            let y = coords
                .y
                .get(id)
                .copied()
                .unwrap_or(layer_index as f64 * layer_gap);
            if let Some(node) = g.node_by_id.get_mut(id) {
                node.layer = Some(layer_index as f64);
                node.order = Some(order_index as f64);
                node.x = Some(x);
                node.y = Some(y);
            }
            if let Some(node) = g.layout_node_mut(id) {
                node.layer = Some(layer_index as f64);
                node.order = Some(order_index as f64);
                node.x = Some(x);
                node.y = Some(y);
            }
        }
    }

    let all_ids: Vec<String> = g.layout.nodes.iter().map(|node| node.id.clone()).collect();
    let anchored_ids = collect_anchored_ids(g.layout.nodes.as_slice());
    let is_edge_label: HashSet<String> = g
        .layout
        .nodes
        .iter()
        .filter(|node| node_is_edge_label(node))
        .map(|node| node.id.clone())
        .collect();
    for id in &all_ids {
        if is_edge_label.contains(id)
            && let (Some(x), Some(y)) = (coords.x.get(id), coords.y.get(id))
            && let Some(node) = g.layout_node_mut(id)
        {
            node.x = Some(*x);
            node.y = Some(*y);
        }
    }

    let lane_model = build_lane_model(&g.layout.nodes);
    let mut group_bounds: HashMap<NodeId, (f64, f64, f64, f64)> = HashMap::new();
    let mut top_level_groups: Vec<NodeId> = Vec::new();
    for group_id in groups_deepest_first(&g.layout.nodes) {
        if lane_model.is_lane(&group_id) {
            top_level_groups.push(group_id.clone());
        }
        let Some(group_index) = g.layout.nodes.iter().position(|node| node.id == group_id) else {
            continue;
        };
        let child_indices: Vec<usize> = g
            .layout
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, child)| {
                child.parent_id.as_deref() == Some(group_id.as_str())
                    && !anchored_ids.contains(&child.id)
            })
            .map(|(index, _)| index)
            .collect();
        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for child_index in child_indices {
            let child = &g.layout.nodes[child_index];
            let cx = child.x.or_else(|| coords.x.get(&child.id).copied());
            let cy = child.y.or_else(|| coords.y.get(&child.id).copied());
            let cw = child.width.unwrap_or(0.0);
            let ch = child.height.unwrap_or(0.0);
            if let (Some(cx), Some(cy)) = (cx, cy) {
                if !node_is_edge_label(child) {
                    min_x = min_x.min(cx - cw / 2.0);
                    max_x = max_x.max(cx + cw / 2.0);
                }
                min_y = min_y.min(cy - ch / 2.0);
                max_y = max_y.max(cy + ch / 2.0);
            }
        }
        if min_x == f64::INFINITY || min_y == f64::INFINITY {
            let group = &mut g.layout.nodes[group_index];
            let x = group.x.unwrap_or(0.0);
            let y = group.y.unwrap_or(0.0);
            let width = group.width.unwrap_or(0.0);
            let height = group.height.unwrap_or(0.0);
            group.x = Some(x);
            group.y = Some(y);
            group.width = Some(width);
            group.height = Some(height);
        } else {
            let group = &g.layout.nodes[group_index];
            let pad = group.padding.unwrap_or(20.0);
            let horizontal_pad = if group.parent_id.is_some() {
                pad
            } else {
                2.0 * top_lane_horizontal_padding(group)
            };
            let vertical_pad = pad;
            let width = (max_x - min_x).max(0.0) + horizontal_pad;
            let height = (max_y - min_y).max(0.0) + vertical_pad;
            let cx = (min_x + max_x) / 2.0;
            let cy = (min_y + max_y) / 2.0;
            let group = &mut g.layout.nodes[group_index];
            group.x = Some(cx);
            group.y = Some(cy);
            group.width = Some(width);
            group.height = Some(height);
            group_bounds.insert(group_id.clone(), (min_x, max_x, min_y, max_y));
        }
    }

    if top_level_groups.is_empty() || group_bounds.is_empty() {
        return;
    }

    let mut global_min_y = f64::INFINITY;
    let mut global_max_y = f64::NEG_INFINITY;
    let mut max_pad: f64 = 0.0;
    for lane_id in &top_level_groups {
        let Some(group) = g.layout.nodes.iter().find(|node| &node.id == lane_id) else {
            continue;
        };
        let pad = group.padding.unwrap_or(20.0);
        if pad > max_pad {
            max_pad = pad;
        }
        let Some(&(_, _, min_y, max_y)) = group_bounds.get(lane_id) else {
            continue;
        };
        global_min_y = global_min_y.min(min_y);
        global_max_y = global_max_y.max(max_y);
    }
    if global_min_y == f64::INFINITY || global_max_y == f64::NEG_INFINITY {
        return;
    }

    let content_height = (global_max_y - global_min_y).max(0.0);
    let min_header_margin = 36.0;
    let vertical_margin = max_pad.max(min_header_margin);
    let lane_height = content_height + 2.0 * vertical_margin;
    let center_y = (global_min_y + global_max_y) / 2.0;
    for lane_id in &top_level_groups {
        if let Some(lane) = g.layout_node_mut(lane_id) {
            lane.y = Some(center_y);
            lane.height = Some(lane_height);
            set_metadata_number(lane, "swimlaneContentTop", global_min_y);
        }
    }

    let mut sorted_lanes: Vec<NodeId> = top_level_groups.clone();
    sorted_lanes.sort_by(|a, b| {
        let ax = g
            .layout
            .nodes
            .iter()
            .find(|node| &node.id == a)
            .and_then(|node| node.x)
            .unwrap_or(0.0);
        let bx = g
            .layout
            .nodes
            .iter()
            .find(|node| &node.id == b)
            .and_then(|node| node.x)
            .unwrap_or(0.0);
        ax.partial_cmp(&bx).unwrap_or(Ordering::Equal)
    });

    let mut lane_ids: Vec<NodeId> = Vec::new();
    let mut centers: Vec<f64> = Vec::new();
    let mut base_widths: Vec<f64> = Vec::new();
    for lane_id in &sorted_lanes {
        let Some(&(min_x, max_x, _, _)) = group_bounds.get(lane_id) else {
            continue;
        };
        let Some(lane) = g.layout.nodes.iter().find(|node| &node.id == lane_id) else {
            continue;
        };
        let content_width = (max_x - min_x).max(0.0) + 2.0 * top_lane_horizontal_padding(lane);
        lane_ids.push(lane_id.clone());
        centers.push((min_x + max_x) / 2.0);
        base_widths.push(content_width);
    }

    let count = lane_ids.len();
    if count == 0 {
        return;
    }
    let mut lane_widths: HashMap<NodeId, f64> = HashMap::new();
    if count == 1 {
        lane_widths.insert(lane_ids[0].clone(), base_widths[0]);
    } else {
        let mut d = Vec::with_capacity(count - 1);
        for i in 0..count - 1 {
            d.push(centers[i + 1] - centers[i]);
        }
        let mut u = vec![0.0; count];
        for i in 0..count - 1 {
            u[i + 1] = 2.0 * d[i] - u[i];
        }
        let mut lower_bound: f64 = 0.0;
        let mut upper_bound: f64 = f64::INFINITY;
        for i in 0..count {
            let base_w = base_widths[i];
            if i % 2 == 0 {
                lower_bound = lower_bound.max(base_w - u[i]);
            } else {
                upper_bound = upper_bound.min(u[i] - base_w);
            }
        }
        let x = if lower_bound <= upper_bound {
            (lower_bound + upper_bound) / 2.0
        } else {
            lower_bound
        };
        for i in 0..count {
            let w = u[i] + if i % 2 == 0 { x } else { -x };
            let final_width = base_widths[i].max(w);
            lane_widths.insert(lane_ids[i].clone(), final_width);
        }
    }

    for lane_id in &top_level_groups {
        if let Some(width) = lane_widths.get(lane_id)
            && let Some(lane) = g.layout_node_mut(lane_id)
        {
            lane.width = Some(*width);
        }
        if let Some(lane) = g.layout_node_mut(lane_id) {
            assign_top_lane_title_rect(lane);
        }
    }
}
