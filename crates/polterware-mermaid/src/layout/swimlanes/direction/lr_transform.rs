//! Port of `direction/lrTransform.ts`: the LR/RL rotation of the canonical top-to-bottom layout
//! onto horizontal progression, and the BT mirror. Lane strips are sized here in the rotated
//! system, and anchored nodes reserve the room their stand-off needs.

use std::collections::{HashMap, HashSet};

use crate::render::pipeline::types::{GroupTitleRect, LayoutData, Node};

use super::super::anchored_nodes::{
    AnchorFootprint, AnchorSide, anchor_footprints, collect_anchored_ids,
};
use super::super::helpers::{Direction, set_metadata_number};
use super::super::lanes::build_lane_model;
use super::geometry::RectBounds;

/// `LANE_TITLE_BAND`, shared with [`super::geometry`]'s readers of the band.
pub const LANE_TITLE_BAND: f64 = 36.0;

fn build_node_map(nodes: &[Node]) -> HashMap<String, Node> {
    nodes
        .iter()
        .map(|node| (node.id.clone(), node.clone()))
        .collect()
}

fn group_depth(group: &Node, node_by_id: &HashMap<String, Node>) -> i64 {
    let mut seen: HashSet<String> = HashSet::from([group.id.clone()]);
    let mut depth = 0_i64;
    let mut parent_id = group.parent_id.clone();
    while let Some(parent) = parent_id {
        if seen.contains(&parent) {
            break;
        }
        let Some(parent_node) = node_by_id.get(&parent) else {
            break;
        };
        if !parent_node.is_group.unwrap_or(false) {
            break;
        }
        seen.insert(parent.clone());
        depth += 1;
        parent_id = parent_node.parent_id.clone();
    }
    depth
}

#[derive(Clone, Copy, Debug, Default)]
struct Bounds {
    min_x: f64,
    max_x: f64,
    min_y: f64,
    max_y: f64,
}

fn bounds_for_children(children: &[Node]) -> Option<Bounds> {
    let mut bounds = Bounds {
        min_x: f64::INFINITY,
        max_x: f64::NEG_INFINITY,
        min_y: f64::INFINITY,
        max_y: f64::NEG_INFINITY,
    };
    for child in children {
        let (Some(cx), Some(cy)) = (child.x, child.y) else {
            continue;
        };
        let w = child.width.unwrap_or(0.0);
        let h = child.height.unwrap_or(0.0);
        bounds.min_x = bounds.min_x.min(cx - w / 2.0);
        bounds.max_x = bounds.max_x.max(cx + w / 2.0);
        bounds.min_y = bounds.min_y.min(cy - h / 2.0);
        bounds.max_y = bounds.max_y.max(cy + h / 2.0);
    }
    if bounds.min_x == f64::INFINITY || bounds.min_y == f64::INFINITY {
        return None;
    }
    Some(bounds)
}

fn apply_group_bounds(group: &mut Node, bounds: Bounds) {
    let pad = group.padding.unwrap_or(20.0);
    group.x = Some((bounds.min_x + bounds.max_x) / 2.0);
    group.y = Some((bounds.min_y + bounds.max_y) / 2.0);
    group.width = Some((bounds.max_x - bounds.min_x).max(0.0) + pad);
    group.height = Some((bounds.max_y - bounds.min_y).max(0.0) + pad);
}

fn recompute_nested_group_bounds(nodes: &mut [Node]) {
    let node_by_id = build_node_map(nodes);
    let mut groups_by_depth: Vec<usize> = nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.is_group.unwrap_or(false) && node.parent_id.is_some())
        .map(|(index, _)| index)
        .collect();
    groups_by_depth.sort_by(|a, b| {
        group_depth(&nodes[*b], &node_by_id).cmp(&group_depth(&nodes[*a], &node_by_id))
    });

    for group_index in groups_by_depth {
        let group_id = nodes[group_index].id.clone();
        let children: Vec<Node> = nodes
            .iter()
            .filter(|node| node.parent_id.as_deref() == Some(group_id.as_str()))
            .cloned()
            .collect();
        if let Some(bounds) = bounds_for_children(&children) {
            apply_group_bounds(&mut nodes[group_index], bounds);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Axis {
    X,
    Y,
}

fn mirror_axis(layout: &mut LayoutData, axis: Axis) -> bool {
    let content_nodes: Vec<&Node> = layout
        .nodes
        .iter()
        .filter(|node| !node.is_group.unwrap_or(false))
        .collect();
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for node in content_nodes {
        let value = match axis {
            Axis::X => node.x,
            Axis::Y => node.y,
        };
        let Some(value) = value else {
            continue;
        };
        min = min.min(value);
        max = max.max(value);
    }
    if !min.is_finite() || !max.is_finite() {
        return false;
    }
    let mirror = |value: f64| min + max - value;
    for node in &mut layout.nodes {
        match axis {
            Axis::X => {
                if let Some(value) = node.x {
                    node.x = Some(mirror(value));
                }
            }
            Axis::Y => {
                if let Some(value) = node.y {
                    node.y = Some(mirror(value));
                }
            }
        }
        if let Some(title_rect) = node.group_title_rect {
            node.group_title_rect = Some(match axis {
                Axis::X => GroupTitleRect {
                    left: mirror(title_rect.right),
                    right: mirror(title_rect.left),
                    top: title_rect.top,
                    bottom: title_rect.bottom,
                },
                Axis::Y => GroupTitleRect {
                    left: title_rect.left,
                    right: title_rect.right,
                    top: mirror(title_rect.bottom),
                    bottom: mirror(title_rect.top),
                },
            });
        }
    }
    for edge in &mut layout.edges {
        if let Some(points) = edge.points.as_mut() {
            for point in points {
                match axis {
                    Axis::X => point.x = mirror(point.x),
                    Axis::Y => point.y = mirror(point.y),
                }
            }
        }
    }
    true
}

pub fn apply_bt_direction_transform(layout: &mut LayoutData) -> bool {
    if !layout
        .nodes
        .iter()
        .any(|node| !node.is_group.unwrap_or(false))
    {
        return true;
    }
    mirror_axis(layout, Axis::Y)
}

/// How far the LR/RL transform stretches the flow axis: the diagram's own nodes' average
/// width over their average height, never below 1. Edge labels and groups do not count, so a
/// label cannot stretch the flow axis. Anchored nodes ride on their host and do not count
/// either.
pub fn flow_axis_stretch(nodes: &[Node]) -> f64 {
    let anchored_ids = collect_anchored_ids(nodes);
    let scaled_nodes: Vec<&Node> = nodes
        .iter()
        .filter(|node| {
            !node.is_group.unwrap_or(false)
                && !node.is_edge_label.unwrap_or(false)
                && !anchored_ids.contains(&node.id)
        })
        .collect();
    if scaled_nodes.is_empty() {
        return 1.0;
    }
    let mut total_width = 0.0;
    let mut total_height = 0.0;
    for node in &scaled_nodes {
        total_width += node.width.unwrap_or(0.0);
        total_height += node.height.unwrap_or(0.0);
    }
    let count = scaled_nodes.len() as f64;
    let avg_width = total_width / count;
    let avg_height = total_height / count;
    if avg_height > 0.0 {
        f64::max(1.0, avg_width / avg_height)
    } else {
        1.0
    }
}

fn mirrored_side(side: AnchorSide) -> AnchorSide {
    match side {
        AnchorSide::Left => AnchorSide::Right,
        AnchorSide::Right => AnchorSide::Left,
        other => other,
    }
}

fn grow_along_border(
    bounds: &mut RectBounds,
    host: RectBounds,
    side: AnchorSide,
    footprint: AnchorFootprint,
    mirrored: bool,
) {
    if matches!(side, AnchorSide::Top | AnchorSide::Bottom) {
        let centre_x = (host.left + host.right) / 2.0;
        let (before, after) = if mirrored {
            (footprint.after, footprint.before)
        } else {
            (footprint.before, footprint.after)
        };
        bounds.left = bounds.left.min(centre_x - before);
        bounds.right = bounds.right.max(centre_x + after);
        return;
    }
    let centre_y = (host.top + host.bottom) / 2.0;
    bounds.top = bounds.top.min(centre_y - footprint.before);
    bounds.bottom = bounds.bottom.max(centre_y + footprint.after);
}

fn grow_outward(bounds: &mut RectBounds, host: RectBounds, side: AnchorSide, beyond: f64) {
    match side {
        AnchorSide::Top => bounds.top = bounds.top.min(host.top - beyond),
        AnchorSide::Bottom => bounds.bottom = bounds.bottom.max(host.bottom + beyond),
        AnchorSide::Left => bounds.left = bounds.left.min(host.left - beyond),
        AnchorSide::Right => bounds.right = bounds.right.max(host.right + beyond),
    }
}

fn reserve_on_side(
    bounds: &mut RectBounds,
    host: RectBounds,
    side: AnchorSide,
    footprint: AnchorFootprint,
    mirrored: bool,
) {
    grow_along_border(bounds, host, side, footprint, mirrored);
    grow_outward(bounds, host, side, footprint.beyond);
}

/// Grows a host to cover the room each run standing off it is placed in. The runs are keyed by
/// the side they stand on as drawn, so RL reserves them on the side the mirror will turn into
/// it.
fn with_anchor_footprints(
    host: &Node,
    footprints: &HashMap<AnchorSide, AnchorFootprint>,
    direction: Direction,
) -> Node {
    let x = host.x.unwrap_or(0.0);
    let y = host.y.unwrap_or(0.0);
    let half_width = host.width.unwrap_or(0.0) / 2.0;
    let half_height = host.height.unwrap_or(0.0) / 2.0;
    let own = RectBounds {
        left: x - half_width,
        right: x + half_width,
        top: y - half_height,
        bottom: y + half_height,
    };
    let mut grown = own;
    let mirrored = direction == Direction::Rl;
    for side in [
        AnchorSide::Top,
        AnchorSide::Right,
        AnchorSide::Bottom,
        AnchorSide::Left,
    ] {
        if let Some(footprint) = footprints.get(&side) {
            reserve_on_side(
                &mut grown,
                own,
                if mirrored { mirrored_side(side) } else { side },
                *footprint,
                mirrored,
            );
        }
    }
    Node {
        x: Some((grown.left + grown.right) / 2.0),
        y: Some((grown.top + grown.bottom) / 2.0),
        width: Some(grown.right - grown.left),
        height: Some(grown.bottom - grown.top),
        ..host.clone()
    }
}

pub fn apply_lr_direction_transform(layout: &mut LayoutData, direction: Direction) -> bool {
    let anchored_ids = collect_anchored_ids(&layout.nodes);
    let content_indices: Vec<usize> = layout
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| !node.is_group.unwrap_or(false) && !anchored_ids.contains(&node.id))
        .map(|(index, _)| index)
        .collect();

    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    for index in &content_indices {
        let x0 = layout.nodes[*index].x.unwrap_or(0.0);
        let y0 = layout.nodes[*index].y.unwrap_or(0.0);
        if x0 < min_x {
            min_x = x0;
        }
        if y0 < min_y {
            min_y = y0;
        }
    }

    if !min_x.is_finite() || !min_y.is_finite() {
        return false;
    }

    let title_band_size = LANE_TITLE_BAND;
    let horizontal_scale_factor = flow_axis_stretch(&layout.nodes);

    for index in &content_indices {
        let node = &mut layout.nodes[*index];
        let x0 = node.x.unwrap_or(0.0);
        let y0 = node.y.unwrap_or(0.0);
        node.x = Some((y0 - min_y) * horizontal_scale_factor + title_band_size);
        node.y = Some(x0 - min_x);
    }

    for edge in &mut layout.edges {
        let Some(points) = edge.points.as_mut() else {
            continue;
        };
        for point in points.iter_mut() {
            let x0 = point.x;
            let y0 = point.y;
            point.x = (y0 - min_y) * horizontal_scale_factor + title_band_size;
            point.y = x0 - min_x;
        }
    }

    recompute_nested_group_bounds(&mut layout.nodes);

    let lane_model = build_lane_model(&layout.nodes);
    let lane_indices: Vec<usize> = layout
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| lane_model.is_lane(&node.id))
        .map(|(index, _)| index)
        .collect();
    if lane_indices.is_empty() {
        if direction == Direction::Rl {
            mirror_axis(layout, Axis::X);
        }
        return true;
    }

    let mut children_by_lane: HashMap<String, Vec<Node>> = HashMap::new();
    let footprints = anchor_footprints(&layout.nodes, Some(direction));

    for node in &layout.nodes {
        if lane_model.is_lane(&node.id)
            || lane_model.is_pool(&node.id)
            || anchored_ids.contains(&node.id)
        {
            continue;
        }
        let Some(lane_id) = lane_model.lane_id_of(&node.id) else {
            continue;
        };
        let placed = match footprints.get(&node.id) {
            Some(footprint) => with_anchor_footprints(node, footprint, direction),
            None => node.clone(),
        };
        children_by_lane.entry(lane_id).or_default().push(placed);
    }

    let mut max_pad: f64 = 0.0;
    for index in &lane_indices {
        let pad = layout.nodes[*index].padding.unwrap_or(0.0);
        if pad > max_pad {
            max_pad = pad;
        }
    }

    struct LaneBounds {
        index: usize,
        content_top: f64,
        content_bottom: f64,
        center_y: f64,
    }

    let mut lane_bounds: Vec<LaneBounds> = Vec::new();
    let mut global_min_x_child = f64::INFINITY;
    let mut global_max_x_child = f64::NEG_INFINITY;

    for index in &lane_indices {
        let lane_id = layout.nodes[*index].id.clone();
        let children = children_by_lane.get(&lane_id);
        let Some(children) = children else {
            continue;
        };
        let Some(bounds) = bounds_for_children(children) else {
            continue;
        };
        global_min_x_child = global_min_x_child.min(bounds.min_x);
        global_max_x_child = global_max_x_child.max(bounds.max_x);

        lane_bounds.push(LaneBounds {
            index: *index,
            content_top: bounds.min_y,
            content_bottom: bounds.max_y,
            center_y: (bounds.min_y + bounds.max_y) / 2.0,
        });
    }

    if global_min_x_child == f64::INFINITY || global_max_x_child == f64::NEG_INFINITY {
        return true;
    }

    let full_content_width = (global_max_x_child - global_min_x_child).max(0.0);
    let horizontal_margin = f64::max(max_pad, 10.0);
    let body_width = full_content_width + 2.0 * horizontal_margin;
    let lane_width = title_band_size + body_width;
    let body_center = (global_min_x_child + global_max_x_child) / 2.0;
    let body_left = body_center - body_width / 2.0;
    let lane_left = body_left - title_band_size;
    let center_x = lane_left + lane_width / 2.0;
    let vertical_margin = f64::max(max_pad, title_band_size);

    lane_bounds.sort_by(|a, b| {
        a.center_y
            .partial_cmp(&b.center_y)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let lane_count = lane_bounds.len();
    for (position, bounds) in lane_bounds.iter().enumerate() {
        let lane_top = if position == 0 {
            bounds.content_top - vertical_margin
        } else {
            let prev = &lane_bounds[position - 1];
            (prev.content_bottom + bounds.content_top) / 2.0
        };
        let lane_bottom = if position == lane_count - 1 {
            bounds.content_bottom + vertical_margin
        } else {
            let next = &lane_bounds[position + 1];
            (bounds.content_bottom + next.content_top) / 2.0
        };

        let lane_height = (lane_bottom - lane_top).max(0.0);
        let center_y = (lane_top + lane_bottom) / 2.0;

        let lane = &mut layout.nodes[bounds.index];
        lane.x = Some(center_x);
        lane.y = Some(center_y);
        lane.width = Some(lane_width);
        lane.height = Some(lane_height);
        set_metadata_number(lane, "swimlaneContentTop", bounds.content_top);
        lane.group_title_rect = Some(GroupTitleRect {
            left: lane_left,
            right: lane_left + title_band_size,
            top: lane_top,
            bottom: lane_bottom,
        });
    }

    if direction == Direction::Rl {
        mirror_axis(layout, Axis::X);
    }

    true
}

/// The `swimlaneContentTop` a layout stores on a lane, which the title band and the lane
/// readers use.
pub fn swimlane_content_top(node: &Node) -> Option<f64> {
    super::super::helpers::metadata_number(node, "swimlaneContentTop")
}
