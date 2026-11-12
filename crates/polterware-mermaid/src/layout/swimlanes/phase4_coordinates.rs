//! Port of `phase4.coordinates.ts`: positions every node and seeds each edge label where the
//! layout keeps room for it.

use std::collections::HashMap;

use crate::render::pipeline::types::Node;

use super::config::COORDINATES;
use super::helpers::{
    Coordinates, Direction, Graph, NodeId, OrderedLayers, js_round, node_is_dummy,
    node_is_edge_label,
};
use super::phase2_options::{TopLaneResolver, create_top_lane_resolver, resolve_top_lane_order};

#[derive(Clone, Debug, Default)]
pub struct CoordOptions {
    /// Vertical distance between layers.
    pub layer_gap: Option<f64>,
    /// Horizontal gap between siblings inside a lane.
    pub node_gap: Option<f64>,
    /// Horizontal gap between lanes (clusters).
    pub lane_gap: Option<f64>,
    /// Layout direction for proper spacing.
    pub direction: Option<Direction>,
    pub lane_order: Option<Vec<String>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum RowSide {
    Before,
    After,
}

impl RowSide {
    fn as_str(self) -> &'static str {
        match self {
            RowSide::Before => "before",
            RowSide::After => "after",
        }
    }
}

/// A labelled edge within one rank: its label sits in the target's row.
#[derive(Clone, Debug)]
struct RowLabel {
    label_id: NodeId,
    rank: usize,
    lane: Option<String>,
    side: RowSide,
}

/// A labelled edge whose label spans the gap between two ranks.
#[derive(Clone, Debug)]
struct GapLabel {
    label_id: NodeId,
    start: NodeId,
    end: NodeId,
    gap: i64,
    crosses_lanes: bool,
}

type GapLabelGroup = ((i64, Option<String>), Vec<GapLabel>);

fn max_of(values: impl Iterator<Item = f64>) -> f64 {
    let mut max = f64::NEG_INFINITY;
    for value in values {
        max = max.max(value);
    }
    max
}

fn min_of(values: impl Iterator<Item = f64>) -> f64 {
    let mut min = f64::INFINITY;
    for value in values {
        min = min.min(value);
    }
    min
}

fn lane_of_label(
    label_lane: &HashMap<NodeId, Option<String>>,
    resolver: &TopLaneResolver,
    id: &str,
) -> Option<String> {
    match label_lane.get(id) {
        Some(lane) => lane.clone(),
        None => resolver.lane_of(id),
    }
}

pub fn assign_coordinates(
    ordered: &OrderedLayers,
    g_with_dummies: &Graph,
    opts: Option<CoordOptions>,
) -> Coordinates {
    let opts = opts.unwrap_or_default();
    let layer_gap = opts.layer_gap.unwrap_or(COORDINATES.default_layer_gap);
    let node_gap = opts.node_gap.unwrap_or(COORDINATES.default_node_gap);
    let lane_gap = opts.lane_gap.unwrap_or(node_gap * 2.0);
    let direction = opts.direction.unwrap_or_default();
    let is_horizontal = direction.is_horizontal();

    let mut x: HashMap<NodeId, f64> = HashMap::new();
    let mut y: HashMap<NodeId, f64> = HashMap::new();

    let label_by_id: HashMap<NodeId, Node> = g_with_dummies
        .layout
        .nodes
        .iter()
        .filter(|node| node_is_edge_label(node))
        .map(|node| (node.id.clone(), node.clone()))
        .collect();
    let size_of = |id: &str, width: bool| -> f64 {
        let node = label_by_id
            .get(id)
            .or_else(|| g_with_dummies.node_by_id.get(id));
        node.map(|node| {
            if width {
                node.width.unwrap_or(0.0)
            } else {
                node.height.unwrap_or(0.0)
            }
        })
        .unwrap_or(0.0)
    };
    let get_width = |id: &str| size_of(id, true);
    let get_height = |id: &str| size_of(id, false);

    let top_lane_of_node = create_top_lane_resolver(g_with_dummies);
    let mut label_lane: HashMap<NodeId, Option<String>> = HashMap::new();
    let lane_order_global = resolve_top_lane_order(g_with_dummies, opts.lane_order.as_deref());

    let mut lanes_used_set: std::collections::HashSet<Option<String>> =
        std::collections::HashSet::new();
    for layer in &ordered.layers {
        for id in layer {
            lanes_used_set.insert(top_lane_of_node.lane_of(id));
        }
    }
    let has_null_lane = lanes_used_set.contains(&None);
    let lanes_used: Vec<String> = lane_order_global
        .into_iter()
        .filter(|lane| lanes_used_set.contains(&Some(lane.clone())))
        .collect();
    let mut lane_order_columns: Vec<Option<String>> = Vec::new();
    if has_null_lane {
        lane_order_columns.push(None);
    }
    for lane in lanes_used {
        lane_order_columns.push(Some(lane));
    }

    let layers = &ordered.layers;
    let mut rank_of: HashMap<NodeId, usize> = HashMap::new();
    for (layer_index, layer) in layers.iter().enumerate() {
        for id in layer {
            rank_of.insert(id.clone(), layer_index);
        }
    }
    let mut row_labels: Vec<RowLabel> = Vec::new();
    let mut gap_labels: Vec<GapLabel> = Vec::new();
    let mut unranked_labels: Vec<(NodeId, NodeId)> = Vec::new();
    for edge in &g_with_dummies.layout.edges {
        let Some(label_id) = edge.label_node_id.clone() else {
            continue;
        };
        if !label_by_id.contains_key(&label_id) {
            continue;
        }
        let (Some(start), Some(end)) = (edge.start.clone(), edge.end.clone()) else {
            continue;
        };
        let rs = rank_of.get(&start).copied();
        let rt = rank_of.get(&end).copied();
        let (Some(rs), Some(rt)) = (rs, rt) else {
            // An endpoint without a rank leaves the label nothing to sit between.
            let near = if rs.is_none() { end } else { start };
            unranked_labels.push((label_id, near));
            continue;
        };
        label_lane.insert(label_id.clone(), top_lane_of_node.lane_of(&end));
        if rs == rt {
            let column = |id: &str| -> i64 {
                lane_order_columns
                    .iter()
                    .position(|lane| *lane == top_lane_of_node.lane_of(id))
                    .map(|index| index as i64)
                    .unwrap_or(-1)
            };
            row_labels.push(RowLabel {
                label_id,
                rank: rt,
                lane: top_lane_of_node.lane_of(&end),
                side: if start != end && column(&start) < column(&end) {
                    RowSide::Before
                } else {
                    RowSide::After
                },
            });
            continue;
        }
        let lo = rs.min(rt);
        let hi = rs.max(rt);
        gap_labels.push(GapLabel {
            label_id,
            start: start.clone(),
            end: end.clone(),
            gap: lo as i64 + ((hi - lo - 1) as i64).div_euclid(2),
            crosses_lanes: top_lane_of_node.lane_of(&start) != top_lane_of_node.lane_of(&end),
        });
    }

    let mut layer_heights: Vec<f64> = layers
        .iter()
        .map(|layer| max_of(layer.iter().map(|id| get_height(id))).max(0.0))
        .collect();
    for label in &row_labels {
        if let Some(height) = layer_heights.get_mut(label.rank) {
            *height = height.max(get_height(&label.label_id));
        }
    }

    // LR/RL transforms turn width into horizontal span, so widen layer gaps up front.
    let mut extra_layer_gaps: Vec<f64> = vec![0.0; layers.len()];
    if is_horizontal {
        for index in 0..layers.len().saturating_sub(1) {
            let this_layer_max_width =
                max_of(layers[index].iter().map(|id| get_width(id))).max(0.0);
            let next_layer_max_width =
                max_of(layers[index + 1].iter().map(|id| get_width(id))).max(0.0);
            let this_layer_max_height = layer_heights[index];
            let next_layer_max_height = layer_heights[index + 1];

            let normal_spacing = this_layer_max_height / 2.0 + next_layer_max_height / 2.0;
            let required_spacing = (this_layer_max_width + next_layer_max_width) / 2.0;
            extra_layer_gaps[index] = (required_spacing - normal_spacing - layer_gap).max(0.0);
        }
    }

    let flow_stretch = if is_horizontal {
        super::stubs::flow_axis_stretch(&g_with_dummies.layout.nodes)
    } else {
        1.0
    };
    let mut label_gap_need: Vec<f64> = vec![0.0; layers.len()];
    for label in &gap_labels {
        let extent = if is_horizontal {
            get_width(&label.label_id)
        } else {
            get_height(&label.label_id)
        };
        let need = (extent + 2.0 * COORDINATES.edge_label_clearance) / flow_stretch;
        if let Some(slot) = label_gap_need.get_mut(label.gap.max(0) as usize) {
            *slot = slot.max(need);
        }
    }
    for (gap, need) in label_gap_need.iter().enumerate() {
        if let Some(extra) = extra_layer_gaps.get_mut(gap) {
            *extra += need;
        }
    }

    let mut lane_width: HashMap<Option<String>, f64> = HashMap::new();
    for lane in &lane_order_columns {
        lane_width.insert(lane.clone(), 0.0);
    }
    for layer in layers {
        let mut per_lane: Vec<(Option<String>, Vec<NodeId>)> = Vec::new();
        let mut lane_index: HashMap<Option<String>, usize> = HashMap::new();
        let mut null_ids: Vec<NodeId> = Vec::new();
        for id in layer {
            match lane_of_label(&label_lane, &top_lane_of_node, id) {
                None => null_ids.push(id.clone()),
                Some(lane) => {
                    let index = *lane_index.entry(Some(lane.clone())).or_insert_with(|| {
                        per_lane.push((Some(lane.clone()), Vec::new()));
                        per_lane.len() - 1
                    });
                    per_lane[index].1.push(id.clone());
                }
            }
        }
        for (lane, ids) in &per_lane {
            let total = ids.iter().map(|id| get_width(id)).sum::<f64>()
                + node_gap * ids.len().saturating_sub(1) as f64;
            let entry = lane_width.entry(lane.clone()).or_insert(0.0);
            *entry = entry.max(total);
        }
        if has_null_lane && !null_ids.is_empty() {
            let total_null = null_ids.iter().map(|id| get_width(id)).sum::<f64>()
                + node_gap * null_ids.len().saturating_sub(1) as f64;
            let entry = lane_width.entry(None).or_insert(0.0);
            *entry = entry.max(total_null);
        }
    }

    // Labels between ranks are anchored inside their lane, so the lane must hold them.
    let mut gap_label_groups: Vec<GapLabelGroup> = Vec::new();
    let mut gap_label_index: HashMap<(i64, Option<String>), usize> = HashMap::new();
    for label in &gap_labels {
        let key = (
            label.gap,
            lane_of_label(&label_lane, &top_lane_of_node, &label.label_id),
        );
        let index = *gap_label_index.entry(key.clone()).or_insert_with(|| {
            gap_label_groups.push((key.clone(), Vec::new()));
            gap_label_groups.len() - 1
        });
        gap_label_groups[index].1.push(label.clone());
    }
    for (_, group) in &gap_label_groups {
        let lane = lane_of_label(&label_lane, &top_lane_of_node, &group[0].label_id);
        if lane.is_some() {
            let mut span = 0.0;
            for (index, label) in group.iter().enumerate() {
                span += get_width(&label.label_id) + if index > 0 { node_gap } else { 0.0 };
            }
            let entry = lane_width.entry(lane).or_insert(0.0);
            *entry = entry.max(span);
        }
    }

    // Labels in a row stack outward from the row's nodes on their side.
    let mut row_label_spans: HashMap<(usize, Option<String>, &'static str), f64> = HashMap::new();
    for label in &row_labels {
        let key = (label.rank, label.lane.clone(), label.side.as_str());
        *row_label_spans.entry(key).or_insert(0.0) += node_gap + get_width(&label.label_id);
    }
    let row_label_span = |rank: usize, lane: &Option<String>, side: RowSide| -> f64 {
        row_label_spans
            .get(&(rank, lane.clone(), side.as_str()))
            .copied()
            .unwrap_or(0.0)
    };
    for label in &row_labels {
        let nodes_width = layers[label.rank]
            .iter()
            .filter(|id| lane_of_label(&label_lane, &top_lane_of_node, id) == label.lane)
            .enumerate()
            .map(|(index, id)| get_width(id) + if index > 0 { node_gap } else { 0.0 })
            .sum::<f64>();
        let side = row_label_span(label.rank, &label.lane, RowSide::Before).max(row_label_span(
            label.rank,
            &label.lane,
            RowSide::After,
        ));
        let entry = lane_width.entry(label.lane.clone()).or_insert(0.0);
        *entry = entry.max(nodes_width + 2.0 * side);
    }

    let mut center_x: HashMap<Option<String>, f64> = HashMap::new();
    {
        let widths: Vec<f64> = lane_order_columns
            .iter()
            .map(|lane| lane_width.get(lane).copied().unwrap_or(0.0))
            .collect();
        let total_width = widths.iter().sum::<f64>()
            + lane_gap * lane_order_columns.len().saturating_sub(1) as f64;
        let mut cursor = -total_width / 2.0;
        for (index, lane) in lane_order_columns.iter().enumerate() {
            let width = widths[index];
            center_x.insert(lane.clone(), cursor + width / 2.0);
            cursor += width;
            if index < lane_order_columns.len() - 1 {
                cursor += lane_gap;
            }
        }
    }

    let mut gap_center_y: Vec<f64> = Vec::new();
    let mut y_offset = 0.0;
    for (layer_index, layer) in layers.iter().enumerate() {
        let layer_height = layer_heights[layer_index];

        let mut by_lane: Vec<(Option<String>, Vec<NodeId>)> = Vec::new();
        let mut lane_index: HashMap<Option<String>, usize> = HashMap::new();
        for id in layer {
            let lane = lane_of_label(&label_lane, &top_lane_of_node, id);
            let index = *lane_index.entry(lane.clone()).or_insert_with(|| {
                by_lane.push((lane.clone(), Vec::new()));
                by_lane.len() - 1
            });
            by_lane[index].1.push(id.clone());
        }
        let by_lane_map: HashMap<Option<String>, Vec<NodeId>> = by_lane.into_iter().collect();

        for lane in &lane_order_columns {
            let nodes_in_lane = by_lane_map.get(lane).cloned().unwrap_or_default();
            if nodes_in_lane.is_empty() {
                continue;
            }
            let cx = center_x.get(lane).copied().unwrap_or(0.0);
            if nodes_in_lane.len() == 1 {
                let id = &nodes_in_lane[0];
                x.insert(id.clone(), cx);
                y.insert(id.clone(), y_offset + layer_height / 2.0);
            } else {
                let widths: Vec<f64> = nodes_in_lane.iter().map(|id| get_width(id)).collect();
                let total =
                    widths.iter().sum::<f64>() + node_gap * (nodes_in_lane.len() - 1) as f64;
                let mut start = cx - total / 2.0;
                for (index, id) in nodes_in_lane.iter().enumerate() {
                    let width = widths[index];
                    x.insert(id.clone(), start + width / 2.0);
                    y.insert(id.clone(), y_offset + layer_height / 2.0);
                    start += width + node_gap;
                }
            }
        }

        let gap_size = layer_gap + extra_layer_gaps.get(layer_index).copied().unwrap_or(0.0);
        gap_center_y.push(y_offset + layer_height + gap_size / 2.0);
        y_offset += layer_height + gap_size;
    }

    // A row label goes beyond the row's nodes in its lane, on the side the edge comes in from.
    let mut row_edge: HashMap<(usize, Option<String>, &'static str), f64> = HashMap::new();
    for label in &row_labels {
        let key = (label.rank, label.lane.clone(), label.side.as_str());
        if !row_edge.contains_key(&key) {
            let in_row = layers[label.rank]
                .iter()
                .filter(|id| lane_of_label(&label_lane, &top_lane_of_node, id) == label.lane);
            let edges = in_row.map(|id| {
                let id_x = x.get(id).copied().unwrap_or(0.0);
                if label.side == RowSide::Before {
                    id_x - get_width(id) / 2.0
                } else {
                    id_x + get_width(id) / 2.0
                }
            });
            let edge = if label.side == RowSide::Before {
                min_of(edges)
            } else {
                max_of(edges)
            };
            row_edge.insert(key.clone(), edge);
        }
        let width = get_width(&label.label_id);
        let sign = if label.side == RowSide::Before {
            -1.0
        } else {
            1.0
        };
        let at = row_edge.get(&key).copied().unwrap_or(0.0) + sign * (node_gap + width / 2.0);
        x.insert(label.label_id.clone(), at);
        if let Some(anchor) = layers[label.rank]
            .iter()
            .find(|id| lane_of_label(&label_lane, &top_lane_of_node, id) == label.lane)
            && let Some(anchor_y) = y.get(anchor).copied()
        {
            y.insert(label.label_id.clone(), anchor_y);
        }
        row_edge.insert(key, at + sign * (width / 2.0));
    }

    // A label between ranks starts in the middle of its gap; labels sharing a gap and lane
    // are spread side by side around where they would have started.
    for (_, group) in &gap_label_groups {
        let wanted: Vec<f64> = group
            .iter()
            .map(|label| {
                if label.crosses_lanes {
                    x.get(&label.end).copied().unwrap_or(0.0)
                } else {
                    (x.get(&label.start).copied().unwrap_or(0.0)
                        + x.get(&label.end).copied().unwrap_or(0.0))
                        / 2.0
                }
            })
            .collect();
        let mut order: Vec<usize> = (0..group.len()).collect();
        order.sort_by(|a, b| {
            wanted[*a]
                .partial_cmp(&wanted[*b])
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.cmp(b))
        });
        let mut placed = vec![0.0; group.len()];
        let mut right = f64::NEG_INFINITY;
        for index in order {
            let width = get_width(&group[index].label_id);
            placed[index] = wanted[index].max(right + node_gap + width / 2.0);
            right = placed[index] + width / 2.0;
        }
        let mean = |values: &[f64]| values.iter().sum::<f64>() / values.len() as f64;
        let shift = mean(&wanted) - mean(&placed);
        for (index, label) in group.iter().enumerate() {
            x.insert(label.label_id.clone(), placed[index] + shift);
            if let Some(gap_y) = gap_center_y.get(label.gap.max(0) as usize) {
                y.insert(label.label_id.clone(), *gap_y);
            }
        }
    }

    // A label with no ranked endpoint to follow stays on the other endpoint, or at the origin.
    for (label_id, near) in &unranked_labels {
        x.insert(label_id.clone(), x.get(near).copied().unwrap_or(0.0));
        y.insert(label_id.clone(), y.get(near).copied().unwrap_or(0.0));
    }

    // Align dummy chains for each original edge: each dummy x sits midway between the two ends.
    let mut by_ref: Vec<(String, Vec<&super::helpers::EdgeRef>)> = Vec::new();
    let mut by_ref_index: HashMap<String, usize> = HashMap::new();
    for edge in &g_with_dummies.edges {
        let index = *by_ref_index
            .entry(edge.reference.id.clone())
            .or_insert_with(|| {
                by_ref.push((edge.reference.id.clone(), Vec::new()));
                by_ref.len() - 1
            });
        by_ref[index].1.push(edge);
    }
    for (_, chain_edges) in &by_ref {
        let Some(first) = chain_edges.first() else {
            continue;
        };
        let reference = &first.reference;
        let (Some(src), Some(dst)) = (reference.start.clone(), reference.end.clone()) else {
            continue;
        };
        let mid_x = js_round(
            (x.get(&src).copied().unwrap_or(0.0) + x.get(&dst).copied().unwrap_or(0.0)) / 2.0,
        );
        let mut involved: Vec<NodeId> = Vec::new();
        for edge in chain_edges {
            if !involved.contains(&edge.src) {
                involved.push(edge.src.clone());
            }
            if !involved.contains(&edge.dst) {
                involved.push(edge.dst.clone());
            }
        }
        for id in involved {
            if id == src || id == dst {
                continue;
            }
            if g_with_dummies
                .node_by_id
                .get(&id)
                .is_some_and(node_is_dummy)
            {
                x.insert(id, mid_x);
            }
        }
    }

    Coordinates { x, y }
}
