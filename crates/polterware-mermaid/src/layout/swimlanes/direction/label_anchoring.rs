//! Port of `direction/labelAnchoring.ts`: placing each edge's label on one of the edge's middle
//! segments (Helmers, diss.pdf §118), inside a lane, clear of foreign nodes, foreign edges, and
//! the edge's own arrowhead clearance, then rating a lane's border-anchored fallback.

use std::collections::HashMap;

use crate::render::pipeline::types::{LayoutData, Node};
use crate::scene::Point;

use super::super::lanes::build_lane_model;
use super::geometry::{
    EPS, RectBounds, dedupe_consecutive_points, inflate_rect, rect_contains_rect,
    rect_from_center_size, rect_of_node_bounds, rects_overlap, segment_bounds_overlap_rect,
};

const MARKER_CLEARANCE_LENGTH: f64 = 10.0;
const MARKER_CLEARANCE_HALF_WIDTH: f64 = 7.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Orientation {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug)]
struct SegmentCandidate {
    idx: usize,
    length: f64,
    orientation: Orientation,
    mid_x: f64,
    mid_y: f64,
}

#[derive(Clone, Debug)]
struct Placement {
    lane_id: String,
    anchor: Point,
    rect: RectBounds,
}

fn js_sign(value: f64) -> f64 {
    if value > 0.0 {
        1.0
    } else if value < 0.0 {
        -1.0
    } else {
        0.0
    }
}

fn marker_clearance_rect_for(points: &[Point], at_start: bool) -> Option<RectBounds> {
    let terminal_index = if at_start { 0 } else { points.len() - 1 };
    let step: isize = if at_start { 1 } else { -1 };
    let tip = *points.get(terminal_index)?;
    let inner = *points.get((terminal_index as isize + step) as usize)?;

    let dx = inner.x - tip.x;
    let dy = inner.y - tip.y;
    let len = dx.abs() + dy.abs();
    if len < EPS {
        return None;
    }

    if dy.abs() <= EPS {
        let x2 = tip.x + js_sign(dx) * MARKER_CLEARANCE_LENGTH;
        return Some(RectBounds {
            left: tip.x.min(x2),
            right: tip.x.max(x2),
            top: tip.y - MARKER_CLEARANCE_HALF_WIDTH,
            bottom: tip.y + MARKER_CLEARANCE_HALF_WIDTH,
        });
    }

    if dx.abs() <= EPS {
        let y2 = tip.y + js_sign(dy) * MARKER_CLEARANCE_LENGTH;
        return Some(RectBounds {
            left: tip.x - MARKER_CLEARANCE_HALF_WIDTH,
            right: tip.x + MARKER_CLEARANCE_HALF_WIDTH,
            top: tip.y.min(y2),
            bottom: tip.y.max(y2),
        });
    }

    Some(RectBounds {
        left: tip.x.min(inner.x),
        right: tip.x.max(inner.x),
        top: tip.y.min(inner.y),
        bottom: tip.y.max(inner.y),
    })
}

fn normalize_rect(rect: RectBounds) -> RectBounds {
    RectBounds {
        left: rect.left.min(rect.right),
        right: rect.left.max(rect.right),
        top: rect.top.min(rect.bottom),
        bottom: rect.top.max(rect.bottom),
    }
}

fn label_overlaps_own_marker(rect: RectBounds, points: &[Point]) -> bool {
    let visible = dedupe_consecutive_points(points, EPS);
    let start_marker = marker_clearance_rect_for(&visible, true);
    let end_marker = marker_clearance_rect_for(&visible, false);
    [start_marker, end_marker]
        .into_iter()
        .flatten()
        .any(|marker| rects_overlap(rect, normalize_rect(marker)))
}

fn clamp(value: f64, min: f64, max: f64) -> f64 {
    value.min(max).max(min)
}

fn point_inside_rect_inclusive(point: Point, rect: RectBounds) -> bool {
    point.x >= rect.left - EPS
        && point.x <= rect.right + EPS
        && point.y >= rect.top - EPS
        && point.y <= rect.bottom + EPS
}

/// The foreign geometry and the placements already chosen, shared by one label's candidates.
struct LabelPlacer<'a> {
    all_edge_segments: &'a [(String, Point, Point)],
    foreign_node_rects: &'a [(String, RectBounds)],
    lane_groups: &'a [(String, RectBounds)],
    placed: Vec<(String, RectBounds)>,
}

impl<'a> LabelPlacer<'a> {
    fn overlaps_foreign_node(&self, label_id: &str, rect: RectBounds) -> bool {
        let buffered = inflate_rect(rect, LABEL_PLACEMENT_BUFFER);
        self.foreign_node_rects.iter().any(|(node_id, node_rect)| {
            node_id.as_str() != label_id && rects_overlap(buffered, *node_rect)
        })
    }

    fn overlaps_foreign_edge(&self, edge_id: &str, rect: RectBounds) -> bool {
        let buffered = inflate_rect(rect, LABEL_PLACEMENT_BUFFER);
        self.all_edge_segments.iter().any(|(id, p1, p2)| {
            id.as_str() != edge_id && segment_bounds_overlap_rect(*p1, *p2, buffered, 0.0)
        })
    }

    fn overlaps_anything(&self, label_id: &str, edge_id: &str, rect: RectBounds) -> bool {
        self.overlaps_foreign_node(label_id, rect) || self.overlaps_foreign_edge(edge_id, rect)
    }

    fn find_containing_lane(&self, rect: RectBounds) -> Option<String> {
        self.lane_groups
            .iter()
            .find(|(_, lane_rect)| rect_contains_rect(*lane_rect, rect))
            .map(|(id, _)| id.clone())
    }

    fn overlaps_placed_label(&self, label_id: &str, rect: RectBounds) -> bool {
        self.placed.iter().any(|(placed_id, placed_rect)| {
            placed_id.as_str() != label_id && rects_overlap(rect, *placed_rect)
        })
    }

    fn placement_for_anchor(&self, anchor: Point, lw: f64, lh: f64) -> Option<Placement> {
        let centered_rect = rect_from_center_size(anchor.x, anchor.y, lw, lh);
        if let Some(lane_id) = self.find_containing_lane(centered_rect) {
            return Some(Placement {
                lane_id,
                anchor,
                rect: centered_rect,
            });
        }

        // A segment close to a lane border keeps the label box inside the lane while requiring
        // the original segment point to stay inside that box, so the validator still sees the
        // edge pass through its label.
        let containing_lane = self
            .lane_groups
            .iter()
            .find(|(_, lane_rect)| point_inside_rect_inclusive(anchor, *lane_rect))?;

        let min_x = containing_lane.1.left + lw / 2.0 + LABEL_LANE_MARGIN;
        let max_x = containing_lane.1.right - lw / 2.0 - LABEL_LANE_MARGIN;
        let min_y = containing_lane.1.top + lh / 2.0 + LABEL_LANE_MARGIN;
        let max_y = containing_lane.1.bottom - lh / 2.0 - LABEL_LANE_MARGIN;
        if min_x > max_x || min_y > max_y {
            return None;
        }

        let clamped_anchor =
            Point::new(clamp(anchor.x, min_x, max_x), clamp(anchor.y, min_y, max_y));
        let clamped_rect = rect_from_center_size(clamped_anchor.x, clamped_anchor.y, lw, lh);
        if point_inside_rect_inclusive(anchor, clamped_rect) {
            Some(Placement {
                lane_id: containing_lane.0.clone(),
                anchor: clamped_anchor,
                rect: clamped_rect,
            })
        } else {
            None
        }
    }
}

// Inflation margin for foreign-edge and foreign-node proximity: the validator's
// `edge-border-hugging` check fires when a polyline runs within about 2u of a label's visual
// border, so candidates are tested with a little more than that.
const LABEL_PLACEMENT_BUFFER: f64 = 3.0;
const LABEL_LANE_MARGIN: f64 = 1.0;
/// Mermaid's point marker occupies roughly 10u at the edge endpoint; keep labels a little
/// farther away so the arrowhead stays readable.
const LABEL_ENDPOINT_CLEARANCE: f64 = 12.0;
const ALONG_SEGMENT_TS: [f64; 9] = [0.5, 0.25, 0.75, 0.05, 0.95, 0.15, 0.85, 0.1, 0.9];

fn rank_segments(
    pool: &[SegmentCandidate],
    label_long_axis: Orientation,
    lw: f64,
    lh: f64,
) -> Vec<SegmentCandidate> {
    let mut ranked = pool.to_vec();
    ranked.sort_by(|a, b| {
        let a_long = a.orientation == label_long_axis;
        let b_long = b.orientation == label_long_axis;
        if a_long != b_long {
            return if a_long {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }
        let a_fits = a.length
            >= if a.orientation == Orientation::Horizontal {
                lw
            } else {
                lh
            } + 2.0;
        let b_fits = b.length
            >= if b.orientation == Orientation::Horizontal {
                lw
            } else {
                lh
            } + 2.0;
        if a_fits != b_fits {
            return if a_fits {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }
        b.length
            .partial_cmp(&a.length)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    ranked
}

struct LabelCandidate<'a> {
    placer: &'a LabelPlacer<'a>,
    label_id: &'a str,
    edge_id: &'a str,
    points: &'a [Point],
    lw: f64,
    lh: f64,
    first_segment: Option<usize>,
    last_segment: Option<usize>,
}

impl LabelCandidate<'_> {
    fn clears_terminal_endpoints(&self, seg: SegmentCandidate, anchor: Point) -> bool {
        let label_half_extent = if seg.orientation == Orientation::Horizontal {
            self.lw / 2.0
        } else {
            self.lh / 2.0
        };
        let required_distance = label_half_extent + LABEL_ENDPOINT_CLEARANCE;
        let distance = |anchor: Point, endpoint: Point| -> f64 {
            if seg.orientation == Orientation::Horizontal {
                (anchor.x - endpoint.x).abs()
            } else {
                (anchor.y - endpoint.y).abs()
            }
        };
        if self.first_segment == Some(seg.idx) {
            let start = self.points[seg.idx];
            if distance(anchor, start) + EPS < required_distance {
                return false;
            }
        }
        if self.last_segment == Some(seg.idx) {
            let end = self.points[seg.idx + 1];
            if distance(anchor, end) + EPS < required_distance {
                return false;
            }
        }
        true
    }

    fn anchor_at_t(&self, seg: SegmentCandidate, t: f64) -> Point {
        let a = self.points[seg.idx];
        let b = self.points[seg.idx + 1];
        Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
    }

    fn try_pool(
        &self,
        pool: &[SegmentCandidate],
        label_long_axis: Orientation,
    ) -> Option<(String, Point)> {
        for seg in rank_segments(pool, label_long_axis, self.lw, self.lh) {
            for t in ALONG_SEGMENT_TS {
                let anchor = self.anchor_at_t(seg, t);
                if !self.clears_terminal_endpoints(seg, anchor) {
                    continue;
                }
                let Some(placement) = self.placer.placement_for_anchor(anchor, self.lw, self.lh)
                else {
                    continue;
                };
                if label_overlaps_own_marker(placement.rect, self.points) {
                    continue;
                }
                if self
                    .placer
                    .overlaps_placed_label(self.label_id, placement.rect)
                {
                    continue;
                }
                if !self
                    .placer
                    .overlaps_anything(self.label_id, self.edge_id, placement.rect)
                {
                    return Some((placement.lane_id, placement.anchor));
                }
            }
        }
        None
    }

    fn find_lane_containing_fallback(
        &self,
        pool: &[SegmentCandidate],
        label_long_axis: Orientation,
        require_endpoint_clearance: bool,
        allow_foreign_edge_overlap: bool,
    ) -> Option<(String, Point)> {
        for seg in rank_segments(pool, label_long_axis, self.lw, self.lh) {
            let anchor = Point::new(seg.mid_x, seg.mid_y);
            if require_endpoint_clearance && !self.clears_terminal_endpoints(seg, anchor) {
                continue;
            }
            let Some(placement) = self.placer.placement_for_anchor(anchor, self.lw, self.lh) else {
                continue;
            };
            if label_overlaps_own_marker(placement.rect, self.points)
                || self
                    .placer
                    .overlaps_placed_label(self.label_id, placement.rect)
                || self
                    .placer
                    .overlaps_foreign_node(self.label_id, placement.rect)
            {
                continue;
            }
            if allow_foreign_edge_overlap
                || !self
                    .placer
                    .overlaps_foreign_edge(self.edge_id, placement.rect)
            {
                return Some((placement.lane_id, placement.anchor));
            }
        }
        None
    }
}

fn segments_of(points: &[Point]) -> Vec<SegmentCandidate> {
    let mut segments = Vec::new();
    for index in 0..points.len().saturating_sub(1) {
        let a = points[index];
        let b = points[index + 1];
        let dx = (a.x - b.x).abs();
        let dy = (a.y - b.y).abs();
        if dx < EPS && dy < EPS {
            continue;
        }
        if dx >= EPS && dy >= EPS {
            // Non-orthogonal; should not happen after orthogonalization.
            continue;
        }
        segments.push(SegmentCandidate {
            idx: index,
            length: dx + dy,
            orientation: if dx >= EPS {
                Orientation::Horizontal
            } else {
                Orientation::Vertical
            },
            mid_x: (a.x + b.x) / 2.0,
            mid_y: (a.y + b.y) / 2.0,
        });
    }
    segments
}

/// Places every labelled edge's label on its polyline, preferring a middle segment, and moves
/// the label into the lane that contains the chosen anchor.
pub fn anchor_labels_to_polyline(layout: &mut LayoutData) {
    let mut all_edge_segments: Vec<(String, Point, Point)> = Vec::new();
    for edge in &layout.edges {
        if edge.is_layout_only.unwrap_or(false) {
            continue;
        }
        let Some(points) = edge.points.as_deref() else {
            continue;
        };
        if points.len() < 2 {
            continue;
        }
        for index in 0..points.len() - 1 {
            all_edge_segments.push((edge.id.clone(), points[index], points[index + 1]));
        }
    }

    let lane_model = build_lane_model(&layout.nodes);
    let mut lane_groups: Vec<(String, RectBounds)> = Vec::new();
    let mut foreign_node_rects: Vec<(String, RectBounds)> = Vec::new();
    for node in &layout.nodes {
        if lane_model.is_lane(&node.id) {
            if let Some(rect) = rect_of_node_bounds(node) {
                lane_groups.push((node.id.clone(), rect));
            }
            continue;
        }
        if node.is_group.unwrap_or(false) {
            continue;
        }
        if node.is_edge_label.unwrap_or(false) {
            continue;
        }
        if let Some(rect) = rect_of_node_bounds(node) {
            foreign_node_rects.push((node.id.clone(), rect));
        }
    }

    let label_indices: HashMap<String, usize> = layout
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id.clone(), index))
        .collect();

    let mut placer = LabelPlacer {
        all_edge_segments: &all_edge_segments,
        foreign_node_rects: &foreign_node_rects,
        lane_groups: &lane_groups,
        placed: Vec::new(),
    };

    // Anchored label positions, applied after the pass so the candidate tests read the same
    // node boxes upstream reads.
    let mut moved: Vec<(usize, Point, String)> = Vec::new();

    for edge in &layout.edges {
        if edge.is_layout_only.unwrap_or(false) {
            continue;
        }
        let Some(label_id) = edge.label_node_id.as_deref() else {
            continue;
        };
        let Some(&label_index) = label_indices.get(label_id) else {
            continue;
        };
        let label_node: &Node = &layout.nodes[label_index];
        let Some(points) = edge.points.as_deref() else {
            continue;
        };
        if points.len() < 2 {
            continue;
        }
        let lw = label_node.width.unwrap_or(0.0);
        let lh = label_node.height.unwrap_or(0.0);
        if lw <= 0.0 || lh <= 0.0 {
            continue;
        }

        let segments = segments_of(points);
        if segments.is_empty() {
            continue;
        }

        // §118: middle segments only, falling back to any segment when the polyline has fewer
        // than three segments (Mermaid calibration for degenerate routes).
        let middle_segments: Vec<SegmentCandidate> = if segments.len() >= 3 {
            segments
                .iter()
                .filter(|segment| segment.idx > 0 && segment.idx < segments.len() - 1)
                .copied()
                .collect()
        } else {
            segments.clone()
        };
        let pool_base = if middle_segments.is_empty() {
            segments.clone()
        } else {
            middle_segments
        };

        // The label is drawn horizontally; its long axis only drives preference.
        let label_long_axis = if lw >= lh {
            Orientation::Horizontal
        } else {
            Orientation::Vertical
        };

        let first_segment = segments.first().map(|segment| segment.idx);
        let last_segment = segments.last().map(|segment| segment.idx);
        let candidate = LabelCandidate {
            placer: &placer,
            label_id,
            edge_id: &edge.id,
            points,
            lw,
            lh,
            first_segment,
            last_segment,
        };

        let chosen = candidate
            .try_pool(&pool_base, label_long_axis)
            .or_else(|| {
                if pool_base.len() < segments.len() {
                    candidate.try_pool(&segments, label_long_axis)
                } else {
                    None
                }
            })
            .or_else(|| {
                candidate.find_lane_containing_fallback(&segments, label_long_axis, true, false)
            })
            .or_else(|| {
                candidate.find_lane_containing_fallback(&segments, label_long_axis, false, false)
            })
            .or_else(|| {
                candidate.find_lane_containing_fallback(&segments, label_long_axis, false, true)
            });

        if let Some((lane_id, anchor)) = chosen {
            moved.push((label_index, anchor, lane_id.clone()));
            let chosen_rect = rect_from_center_size(anchor.x, anchor.y, lw, lh);
            if let Some(entry) = placer
                .placed
                .iter_mut()
                .find(|(placed_id, _)| placed_id == label_id)
            {
                entry.1 = chosen_rect;
            } else {
                placer.placed.push((label_id.to_string(), chosen_rect));
            }
        }
    }

    for (index, anchor, lane_id) in moved {
        layout.nodes[index].x = Some(anchor.x);
        layout.nodes[index].y = Some(anchor.y);
        layout.nodes[index].parent_id = Some(lane_id);
    }
}
