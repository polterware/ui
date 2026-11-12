//! Port of `anchoredNodes.ts`: nodes pinned to another node's border, their stand-off room,
//! overlap clearing, and the square line drawn to the host.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::render::pipeline::types::{LayoutData, Node};
use crate::scene::Point;

use super::helpers::{Direction, metadata, node_is_group};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AnchorSide {
    Top,
    Right,
    Bottom,
    Left,
}

impl AnchorSide {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "top" => Some(Self::Top),
            "right" => Some(Self::Right),
            "bottom" => Some(Self::Bottom),
            "left" => Some(Self::Left),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Right => "right",
            Self::Bottom => "bottom",
            Self::Left => "left",
        }
    }
}

const ANCHOR_GAP: f64 = 8.0;

/// Far beyond any lane a diagram draws, and small enough that a stand-off keeps every
/// coordinate it feeds finite.
pub const MAX_ANCHOR_GAP: f64 = 10_000.0;

fn outward(side: AnchorSide) -> Point {
    match side {
        AnchorSide::Top => Point::new(0.0, -1.0),
        AnchorSide::Bottom => Point::new(0.0, 1.0),
        AnchorSide::Left => Point::new(-1.0, 0.0),
        AnchorSide::Right => Point::new(1.0, 0.0),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NodeAnchor {
    pub host_id: String,
    pub side: Option<AnchorSide>,
    pub slot: Option<f64>,
    pub gap: Option<f64>,
}

pub fn read_anchor(node: Option<&Node>) -> Option<NodeAnchor> {
    let node = node?;
    let raw = metadata(node)?.get("anchorTo")?;
    let object = raw.as_object()?;
    let host_id = object.get("hostId")?.as_str()?;
    if host_id.is_empty() || host_id == node.id {
        return None;
    }
    Some(NodeAnchor {
        host_id: host_id.to_string(),
        side: object
            .get("side")
            .and_then(Value::as_str)
            .and_then(AnchorSide::parse),
        slot: object
            .get("slot")
            .and_then(Value::as_f64)
            .filter(|slot| slot.is_finite()),
        gap: object
            .get("gap")
            .and_then(Value::as_f64)
            .filter(|gap| gap.is_finite() && *gap > 0.0)
            .map(|gap| gap.min(MAX_ANCHOR_GAP)),
    })
}

pub fn resolve_anchor_host_id(id: &str, by_id: &HashMap<String, Node>) -> Option<String> {
    let mut seen: HashSet<String> = HashSet::from([id.to_string()]);
    let mut host_id = read_anchor(by_id.get(id)).map(|anchor| anchor.host_id);
    while let Some(current) = host_id {
        if seen.contains(&current) {
            return None;
        }
        let host = by_id.get(&current)?;
        let next = read_anchor(Some(host)).map(|anchor| anchor.host_id);
        match next {
            None => return Some(current),
            Some(next) => {
                seen.insert(current);
                host_id = Some(next);
            }
        }
    }
    None
}

/// For a validator's `isAttached`: whether `node_id` is pinned to `to_id` through its anchor
/// chain.
pub fn is_anchored_to(nodes: &[Node]) -> impl Fn(&str, &str) -> bool {
    let by_id: HashMap<String, Node> = nodes
        .iter()
        .map(|node| (node.id.clone(), node.clone()))
        .collect();
    move |node_id: &str, to_id: &str| {
        resolve_anchor_host_id(node_id, &by_id).as_deref() == Some(to_id)
    }
}

pub fn collect_anchored_ids(nodes: &[Node]) -> HashSet<String> {
    let by_id: HashMap<String, Node> = nodes
        .iter()
        .map(|node| (node.id.clone(), node.clone()))
        .collect();
    let mut ids = HashSet::new();
    for node in nodes {
        if read_anchor(Some(node)).is_some() && resolve_anchor_host_id(&node.id, &by_id).is_some() {
            ids.insert(node.id.clone());
        }
    }
    ids
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnchorSpace {
    Canonical,
    Final,
}

pub fn default_anchor_side(space: AnchorSpace, direction: Direction) -> AnchorSide {
    match space {
        AnchorSpace::Canonical => AnchorSide::Right,
        AnchorSpace::Final => {
            if direction.is_horizontal() {
                AnchorSide::Bottom
            } else {
                AnchorSide::Right
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnchorPin {
    pub node_id: String,
    pub host_id: String,
    pub side: AnchorSide,
    pub x: f64,
    pub y: f64,
    pub outward: Point,
}

#[derive(Clone, Copy, Debug)]
struct BucketEntry {
    index: usize,
    side: AnchorSide,
    host_id: usize,
    anchor_slot: Option<f64>,
}

fn compare_bucket_entries(a: &BucketEntry, b: &BucketEntry) -> Ordering {
    match (a.anchor_slot, b.anchor_slot) {
        (Some(slot_a), Some(slot_b)) if slot_a != slot_b => {
            slot_a.partial_cmp(&slot_b).unwrap_or(Ordering::Equal)
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        _ => a.index.cmp(&b.index),
    }
}

fn pack_along_border(spans: &[f64]) -> (Vec<f64>, f64) {
    let total = spans.iter().sum::<f64>() + ANCHOR_GAP * (spans.len().saturating_sub(1)) as f64;
    let mut offsets = Vec::with_capacity(spans.len());
    let mut cursor = -total / 2.0;
    for span in spans {
        offsets.push(cursor + span / 2.0);
        cursor += span + ANCHOR_GAP;
    }
    (offsets, total)
}

fn has_usable_size(node: &Node) -> bool {
    [node.width, node.height]
        .iter()
        .all(|size| size.is_none_or(f64::is_finite))
}

fn resolve_host_index(id: &str, nodes: &[Node], by_id: &HashMap<String, usize>) -> Option<usize> {
    let mut seen: HashSet<String> = HashSet::from([id.to_string()]);
    let mut host_id = read_anchor(nodes.get(*by_id.get(id)?)).map(|anchor| anchor.host_id);
    while let Some(current) = host_id {
        if seen.contains(&current) {
            return None;
        }
        let host_index = *by_id.get(&current)?;
        let next = read_anchor(nodes.get(host_index)).map(|anchor| anchor.host_id);
        match next {
            None => return Some(host_index),
            Some(next) => {
                seen.insert(current);
                host_id = Some(next);
            }
        }
    }
    None
}

fn bucket_by_host_and_side(
    nodes: &[Node],
    side_of: impl Fn(&NodeAnchor) -> AnchorSide,
) -> Vec<Vec<BucketEntry>> {
    let by_id: HashMap<String, usize> = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id.clone(), index))
        .collect();
    let mut keys: Vec<(AnchorSide, usize)> = Vec::new();
    let mut buckets: HashMap<(AnchorSide, usize), Vec<BucketEntry>> = HashMap::new();
    for (index, node) in nodes.iter().enumerate() {
        let Some(anchor) = read_anchor(Some(node)) else {
            continue;
        };
        if !has_usable_size(node) {
            continue;
        }
        let Some(host_index) = resolve_host_index(&node.id, nodes, &by_id) else {
            continue;
        };
        let side = side_of(&anchor);
        let key = (side, host_index);
        let bucket = buckets.entry(key).or_insert_with(|| {
            keys.push(key);
            Vec::new()
        });
        bucket.push(BucketEntry {
            index,
            side,
            host_id: host_index,
            anchor_slot: anchor.slot,
        });
    }
    for bucket in buckets.values_mut() {
        bucket.sort_by(compare_bucket_entries);
    }
    keys.into_iter()
        .filter_map(|key| buckets.remove(&key))
        .collect()
}

/// Where a node's centre lies along its host's border, measured from the host's centre.
fn centre_along_border(offset: f64, horizontal: bool, clearance: f64, along_half: f64) -> f64 {
    let clear_of_flow = if !horizontal && clearance > 0.0 {
        -(along_half + ANCHOR_GAP)
    } else {
        0.0
    };
    offset + clear_of_flow
}

/// The room a run of stood-off nodes needs on one side of its host.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AnchorFootprint {
    /// Room needed on the start side of the host's centre, along its border.
    pub before: f64,
    /// Room needed on the end side of the host's centre, along its border.
    pub after: f64,
    /// Room needed outward from the host's border.
    pub beyond: f64,
}

fn footprint_of(nodes: &[Node], bucket: &[BucketEntry], side: AnchorSide) -> AnchorFootprint {
    let horizontal = matches!(side, AnchorSide::Top | AnchorSide::Bottom);
    let along_border = |node: &Node| {
        if horizontal {
            node.width.unwrap_or(0.0)
        } else {
            node.height.unwrap_or(0.0)
        }
    };
    let reach = |node: &Node| {
        if horizontal {
            node.height.unwrap_or(0.0)
        } else {
            node.width.unwrap_or(0.0)
        }
    };
    let spans: Vec<f64> = bucket
        .iter()
        .map(|entry| along_border(&nodes[entry.index]))
        .collect();
    let (offsets, _) = pack_along_border(&spans);
    let mut before: f64 = 0.0;
    let mut after: f64 = 0.0;
    let mut beyond: f64 = 0.0;
    for (position, entry) in bucket.iter().enumerate() {
        let node = &nodes[entry.index];
        let anchor = read_anchor(Some(node));
        let clearance = anchor.and_then(|anchor| anchor.gap).unwrap_or(0.0);
        let half = along_border(node) / 2.0;
        let centre = centre_along_border(offsets[position], horizontal, clearance, half);
        before = before.max(half - centre);
        after = after.max(centre + half);
        beyond = beyond.max(if clearance > 0.0 {
            clearance + reach(node)
        } else {
            0.0
        });
    }
    AnchorFootprint {
        before,
        after,
        beyond,
    }
}

/// The room each run of stood-off nodes needs, per host and per side the run stands on: a
/// node's own `side` when it has one, otherwise the side its direction defaults to.
pub fn anchor_footprints(
    nodes: &[Node],
    direction: Option<Direction>,
) -> HashMap<String, HashMap<AnchorSide, AnchorFootprint>> {
    let fallback = default_anchor_side(AnchorSpace::Final, direction.unwrap_or_default());
    let mut footprints: HashMap<String, HashMap<AnchorSide, AnchorFootprint>> = HashMap::new();
    for bucket in bucket_by_host_and_side(nodes, |anchor| anchor.side.unwrap_or(fallback)) {
        let has_gap = bucket.iter().any(|entry| {
            read_anchor(nodes.get(entry.index))
                .and_then(|a| a.gap)
                .is_some()
        });
        if !has_gap {
            continue;
        }
        let first = bucket[0];
        footprints
            .entry(nodes[first.host_id].id.clone())
            .or_default()
            .insert(first.side, footprint_of(nodes, &bucket, first.side));
    }
    footprints
}

#[derive(Clone, Copy, Debug)]
struct BoxRect {
    left: f64,
    right: f64,
    top: f64,
    bottom: f64,
}

fn box_of(node: &Node) -> BoxRect {
    BoxRect {
        left: node.x.unwrap_or(0.0) - node.width.unwrap_or(0.0) / 2.0,
        right: node.x.unwrap_or(0.0) + node.width.unwrap_or(0.0) / 2.0,
        top: node.y.unwrap_or(0.0) - node.height.unwrap_or(0.0) / 2.0,
        bottom: node.y.unwrap_or(0.0) + node.height.unwrap_or(0.0) / 2.0,
    }
}

fn shares_space(a: BoxRect, b: BoxRect) -> bool {
    a.left < b.right - 1.0
        && a.right > b.left + 1.0
        && a.top < b.bottom - 1.0
        && a.bottom > b.top + 1.0
}

fn band_for_node(
    nodes: &[Node],
    by_id: &HashMap<String, usize>,
    is_lane: &impl Fn(&Node) -> bool,
    host_id: &str,
) -> Option<BoxRect> {
    let mut at = *by_id.get(host_id)?;
    let mut seen: HashSet<String> = HashSet::new();
    loop {
        let parent_id = nodes[at].parent_id.clone()?;
        if seen.contains(&parent_id) {
            return None;
        }
        seen.insert(parent_id.clone());
        let parent = *by_id.get(&parent_id)?;
        if is_lane(&nodes[parent]) {
            return Some(box_of(&nodes[parent]));
        }
        at = parent;
    }
}

const CLEARING_STEP: f64 = 8.0;
const CLEARING_TRIES: usize = 24;

/// Whether a group is a lane or a pool, going by the `laneRole` its producer set.
pub fn has_lane_role(group: &Node) -> bool {
    matches!(
        metadata(group)
            .and_then(|map| map.get("laneRole"))
            .and_then(Value::as_str),
        Some("lane") | Some("pool")
    )
}

/// Moves a stood-off node clear of what it overlaps, never out of the lane its host sits in.
pub fn clear_anchored_overlaps(layout: &mut LayoutData, pins: &[AnchorPin]) -> Vec<AnchorPin> {
    clear_anchored_overlaps_with(layout, pins, has_lane_role)
}

pub fn clear_anchored_overlaps_with(
    layout: &mut LayoutData,
    pins: &[AnchorPin],
    is_lane: impl Fn(&Node) -> bool,
) -> Vec<AnchorPin> {
    let pinned: HashSet<&str> = pins.iter().map(|pin| pin.node_id.as_str()).collect();
    let settled: Vec<usize> = layout
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| !node_is_group(node) && !pinned.contains(node.id.as_str()))
        .map(|(index, _)| index)
        .collect();
    let by_id: HashMap<String, usize> = layout
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id.clone(), index))
        .collect();
    let mut placed: Vec<usize> = Vec::new();
    let mut cleared = Vec::new();

    for pin in pins {
        let Some(node_index) = by_id.get(&pin.node_id).copied() else {
            cleared.push(pin.clone());
            continue;
        };
        let stands_off = read_anchor(layout.nodes.get(node_index))
            .and_then(|anchor| anchor.gap)
            .is_some_and(|gap| gap > 0.0);
        if !stands_off {
            placed.push(node_index);
            cleared.push(pin.clone());
            continue;
        }

        let obstacles: Vec<usize> = settled
            .iter()
            .chain(placed.iter())
            .copied()
            .filter(|index| layout.nodes[*index].id != pin.host_id)
            .collect();
        let band = band_for_node(&layout.nodes, &by_id, &is_lane, &pin.host_id);
        let start_x = layout.nodes[node_index].x.unwrap_or(0.0);
        let start_y = layout.nodes[node_index].y.unwrap_or(0.0);
        let mut tries = 0;
        loop {
            let overlaps = if tries >= CLEARING_TRIES {
                false
            } else {
                let current = box_of(&layout.nodes[node_index]);
                obstacles
                    .iter()
                    .any(|index| shares_space(current, box_of(&layout.nodes[*index])))
            };
            if !overlaps {
                break;
            }
            let node = &mut layout.nodes[node_index];
            node.x = Some(node.x.unwrap_or(0.0) + pin.outward.x * CLEARING_STEP);
            node.y = Some(node.y.unwrap_or(0.0) + pin.outward.y * CLEARING_STEP);
            tries += 1;
            if let Some(band) = band {
                let moved = box_of(&layout.nodes[node_index]);
                if moved.left < band.left
                    || moved.right > band.right
                    || moved.top < band.top
                    || moved.bottom > band.bottom
                {
                    let node = &mut layout.nodes[node_index];
                    node.x = Some(start_x);
                    node.y = Some(start_y);
                    tries = 0;
                    break;
                }
            }
        }

        placed.push(node_index);
        if tries > 0 {
            let node = &layout.nodes[node_index];
            cleared.push(AnchorPin {
                x: node.x.unwrap_or(pin.x),
                y: node.y.unwrap_or(pin.y),
                ..pin.clone()
            });
        } else {
            cleared.push(pin.clone());
        }
    }

    cleared
}

#[derive(Clone, Copy, Debug)]
pub struct PinOptions {
    pub space: AnchorSpace,
    pub direction: Direction,
}

pub fn pin_anchored_nodes(layout: &mut LayoutData, opts: PinOptions) -> Vec<AnchorPin> {
    let nodes = std::mem::take(&mut layout.nodes);
    let buckets = {
        let side_of = |anchor: &NodeAnchor| match opts.space {
            AnchorSpace::Canonical => default_anchor_side(AnchorSpace::Canonical, opts.direction),
            AnchorSpace::Final => anchor
                .side
                .unwrap_or_else(|| default_anchor_side(AnchorSpace::Final, opts.direction)),
        };
        bucket_by_host_and_side(&nodes, side_of)
    };

    let mut pins: Vec<AnchorPin> = Vec::new();
    let mut nodes = nodes;
    for bucket in buckets {
        let first = bucket[0];
        let host = &nodes[first.host_id];
        let (Some(hx), Some(hy), Some(hw), Some(hh)) = (host.x, host.y, host.width, host.height)
        else {
            continue;
        };
        if !hx.is_finite() || !hy.is_finite() || !hw.is_finite() || !hh.is_finite() {
            continue;
        }
        let host_id = host.id.clone();

        let side = first.side;
        let horizontal = matches!(side, AnchorSide::Top | AnchorSide::Bottom);
        let spans: Vec<f64> = bucket
            .iter()
            .map(|entry| {
                let node = &nodes[entry.index];
                if horizontal {
                    node.width.unwrap_or(0.0)
                } else {
                    node.height.unwrap_or(0.0)
                }
            })
            .collect();
        let (offsets, _) = pack_along_border(&spans);

        for (position, entry) in bucket.iter().enumerate() {
            let node = &nodes[entry.index];
            let offset = offsets[position];
            let clearance = read_anchor(Some(node))
                .and_then(|anchor| anchor.gap)
                .unwrap_or(0.0);
            let span = if horizontal {
                node.height.unwrap_or(0.0)
            } else {
                node.width.unwrap_or(0.0)
            } / 2.0;
            let push = if clearance > 0.0 {
                clearance + span
            } else {
                0.0
            };
            let along_half = if horizontal {
                node.width.unwrap_or(0.0)
            } else {
                node.height.unwrap_or(0.0)
            } / 2.0;
            let along = centre_along_border(offset, horizontal, clearance, along_half);
            let x = if horizontal {
                hx + along
            } else if side == AnchorSide::Right {
                hx + hw / 2.0 + push
            } else {
                hx - hw / 2.0 - push
            };
            let y = if horizontal {
                if side == AnchorSide::Bottom {
                    hy + hh / 2.0 + push
                } else {
                    hy - hh / 2.0 - push
                }
            } else {
                hy + along
            };
            let node = &mut nodes[entry.index];
            node.x = Some(x);
            node.y = Some(y);
            pins.push(AnchorPin {
                node_id: node.id.clone(),
                host_id: host_id.clone(),
                side,
                x,
                y,
                outward: outward(side),
            });
        }
    }

    layout.nodes = nodes;
    pins
}

const STUB_REACH: f64 = 16.0;
const BORDER_INSET: f64 = 8.0;

fn clamp(value: f64, low: f64, high: f64) -> f64 {
    value.min(high).max(low)
}

fn attach_face_of(node: Option<&Node>) -> Option<AnchorSide> {
    node.and_then(|node| metadata(node))
        .and_then(|map| map.get("attachFace"))
        .and_then(Value::as_str)
        .and_then(AnchorSide::parse)
}

fn is_finite_number(value: Option<f64>) -> bool {
    value.is_some_and(f64::is_finite)
}

fn can_route(node: &Node, host: &Node) -> bool {
    has_usable_size(node)
        && is_finite_number(node.x)
        && is_finite_number(node.y)
        && is_finite_number(host.x)
        && is_finite_number(host.y)
}

fn point_at(along_x: bool, along: f64, across: f64) -> Point {
    if along_x {
        Point::new(along, across)
    } else {
        Point::new(across, along)
    }
}

fn size_along(node: &Node, along_x: bool) -> f64 {
    if along_x {
        node.width.unwrap_or(0.0)
    } else {
        node.height.unwrap_or(0.0)
    }
}

fn leaves_sideways(face: Option<AnchorSide>, along_x: bool) -> bool {
    if along_x {
        matches!(face, Some(AnchorSide::Left | AnchorSide::Right))
    } else {
        matches!(face, Some(AnchorSide::Top | AnchorSide::Bottom))
    }
}

fn exit_along(centre: f64, half: f64, face: Option<AnchorSide>, sideways: bool) -> f64 {
    if !sideways {
        return centre;
    }
    if matches!(face, Some(AnchorSide::Left | AnchorSide::Top)) {
        centre - (half + STUB_REACH)
    } else {
        centre + (half + STUB_REACH)
    }
}

fn stub_to_host(node: &Node, host: &Node, pin: &AnchorPin) -> Option<Vec<Point>> {
    if !can_route(node, host) {
        return None;
    }
    let along_x = matches!(pin.side, AnchorSide::Top | AnchorSide::Bottom);
    let (n_along, n_across) = if along_x {
        (node.x?, node.y?)
    } else {
        (node.y?, node.x?)
    };
    let (h_along, h_across) = if along_x {
        (host.x?, host.y?)
    } else {
        (host.y?, host.x?)
    };
    let reach = (size_along(host, along_x) / 2.0 - BORDER_INSET).max(0.0);

    let face = attach_face_of(Some(node));
    let sideways = leaves_sideways(face, along_x);
    let exit = exit_along(n_along, size_along(node, along_x) / 2.0, face, sideways);
    let enter = clamp(exit, h_along - reach, h_along + reach);
    let mid = (n_across + h_across) / 2.0;

    let mut path = vec![point_at(along_x, n_along, n_across)];
    if sideways {
        path.push(point_at(along_x, exit, n_across));
    }
    if (enter - exit).abs() >= 1.0 {
        path.push(point_at(along_x, exit, mid));
        path.push(point_at(along_x, enter, mid));
    }
    path.push(point_at(along_x, enter, h_across));
    Some(path)
}

pub fn square_anchored_edges(layout: &mut LayoutData, pins: &[AnchorPin]) {
    let by_id: HashMap<String, usize> = layout
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id.clone(), index))
        .collect();
    let pin_by_id: HashMap<&str, &AnchorPin> =
        pins.iter().map(|pin| (pin.node_id.as_str(), pin)).collect();
    for edge in &mut layout.edges {
        let (Some(start), Some(end)) = (edge.start.as_deref(), edge.end.as_deref()) else {
            continue;
        };
        let Some(pin) = pin_by_id
            .get(start)
            .copied()
            .or_else(|| pin_by_id.get(end).copied())
        else {
            continue;
        };
        let other = if pin.node_id == start { end } else { start };
        if other != pin.host_id {
            continue;
        }
        let node = by_id.get(&pin.node_id).map(|index| &layout.nodes[*index]);
        let host = by_id.get(&pin.host_id).map(|index| &layout.nodes[*index]);
        let path = match (node, host) {
            (Some(node), Some(host)) => stub_to_host(node, host, pin),
            _ => None,
        };
        if let Some(path) = path {
            edge.points = Some(if pin.node_id == start {
                path
            } else {
                let mut reversed = path;
                reversed.reverse();
                reversed
            });
        }
    }
}
