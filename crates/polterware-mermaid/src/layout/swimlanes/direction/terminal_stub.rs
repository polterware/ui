//! Port of `direction/terminalStub.ts`: collapsing a short terminal stub at an edge's
//! destination by retargeting the destination face and dropping the corner (Siebenhaller; the
//! stale-port straightener of Hegemann–Wolff is the precedent).

use std::collections::{HashMap, HashSet};

use crate::render::pipeline::types::{Edge, LayoutData, Node};
use crate::scene::Point;

use super::geometry::{
    collect_node_rect_entries, dedupe_consecutive_points, is_horizontal_segment,
    is_vertical_segment, orthogonal_segments_strictly_cross, point_inside_rect,
    rect_of_node_bounds, same_x, same_y, segment_hits_any_rect,
};

const MIN_STUB: f64 = 10.0;
const EPS_LOCAL: f64 = 1e-3;
const BUFFER: f64 = 2.0;

fn own_segment_key(a: Point, b: Point) -> String {
    let normalise = |value: f64| if value == 0.0 { 0.0 } else { value };
    format!(
        "{:.3},{:.3}|{:.3},{:.3}",
        normalise(a.x),
        normalise(a.y),
        normalise(b.x),
        normalise(b.y)
    )
}

fn crosses_other_edge(
    from: Point,
    to: Point,
    edge_index: usize,
    edges: &[Edge],
    self_segments: &HashSet<String>,
) -> bool {
    for (index, other) in edges.iter().enumerate() {
        if index == edge_index || other.is_layout_only.unwrap_or(false) {
            continue;
        }
        let Some(points) = other.points.as_deref() else {
            continue;
        };
        if points.len() < 2 {
            continue;
        }
        for pair in points.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if self_segments.contains(&own_segment_key(a, b)) {
                continue;
            }
            if orthogonal_segments_strictly_cross(from, to, a, b, EPS_LOCAL) {
                return true;
            }
        }
    }
    false
}

fn reanchor_label(
    points: &[Point],
    label_id: Option<&str>,
    node_by_id: &HashMap<String, Node>,
) -> Option<(f64, f64)> {
    let label_node = node_by_id.get(label_id?)?;
    let lw = label_node.width.unwrap_or(0.0);
    let lh = label_node.height.unwrap_or(0.0);
    if lw <= 0.0 || lh <= 0.0 {
        return None;
    }
    let mut best_mid_x: Option<f64> = None;
    let mut best_mid_y: Option<f64> = None;
    let mut best_len = -1.0_f64;
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let seg_len = (b.x - a.x).hypot(b.y - a.y);
        let is_horiz = same_y(a, b, EPS_LOCAL);
        let is_vert = same_x(a, b, EPS_LOCAL);
        let fits = (is_horiz && seg_len >= lw + 2.0) || (is_vert && seg_len >= lh + 2.0);
        if !fits {
            continue;
        }
        if seg_len > best_len {
            best_len = seg_len;
            best_mid_x = Some((a.x + b.x) / 2.0);
            best_mid_y = Some((a.y + b.y) / 2.0);
        }
    }
    match (best_mid_x, best_mid_y) {
        (Some(x), Some(y)) => Some((x, y)),
        _ => None,
    }
}

/// Collapses a destination stub shorter than the minimum into a single face-centred arrival.
pub fn collapse_short_terminal_stub(layout: &mut LayoutData) {
    let node_by_id: HashMap<String, Node> = layout
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node.clone()))
        .collect();
    let entries = collect_node_rect_entries(&layout.nodes);
    let real_node_rects = entries.real_node_rects;
    let label_rects = entries.label_node_rects;

    for edge_index in 0..layout.edges.len() {
        if layout.edges[edge_index].is_layout_only.unwrap_or(false) {
            continue;
        }
        let Some(raw_points) = layout.edges[edge_index].points.clone() else {
            continue;
        };
        if raw_points.len() < 4 {
            continue;
        }

        // Dedupe consecutive equal points so we measure the real last segment.
        let pts = dedupe_consecutive_points(&raw_points, EPS_LOCAL);
        if pts.len() < 4 {
            continue;
        }

        let n_last = pts.len() - 1;
        let end_pt = pts[n_last];
        let penult_pt = pts[n_last - 1];
        let prev_pt = pts[n_last - 2];

        // Last segment (penult to end): must be short.
        let last_len = (end_pt.x - penult_pt.x).hypot(end_pt.y - penult_pt.y);
        if !(EPS_LOCAL..MIN_STUB).contains(&last_len) {
            continue;
        }

        // Penult segment (prev to penult): must be non-degenerate and perpendicular to last.
        let penult_dx = penult_pt.x - prev_pt.x;
        let penult_dy = penult_pt.y - prev_pt.y;
        let penult_len = penult_dx.hypot(penult_dy);
        if penult_len < EPS_LOCAL {
            continue;
        }

        let last_is_horiz = is_horizontal_segment(penult_pt, end_pt, EPS_LOCAL);
        let last_is_vert = is_vertical_segment(penult_pt, end_pt, EPS_LOCAL);
        let penult_is_horiz = is_horizontal_segment(prev_pt, penult_pt, EPS_LOCAL);
        let penult_is_vert = is_vertical_segment(prev_pt, penult_pt, EPS_LOCAL);
        if !((last_is_horiz && penult_is_vert) || (last_is_vert && penult_is_horiz)) {
            continue;
        }

        let dst_id = layout.edges[edge_index].end.clone();
        let src_id = layout.edges[edge_index].start.clone();
        let Some(dst) = dst_id.as_ref().and_then(|id| node_by_id.get(id)) else {
            continue;
        };
        let dst_cx = dst.x.unwrap_or(0.0);
        let dst_cy = dst.y.unwrap_or(0.0);
        let Some(dst_rect) = rect_of_node_bounds(dst) else {
            continue;
        };

        // The axis of approach is the penult segment's axis; the new face is on the
        // perpendicular to the approach, opposite to the approach direction.
        let (new_prev, new_end) = if penult_is_vert {
            let approach_from_below = penult_dy < 0.0;
            (
                Point::new(dst_cx, prev_pt.y),
                Point::new(
                    dst_cx,
                    if approach_from_below {
                        dst_rect.bottom
                    } else {
                        dst_rect.top
                    },
                ),
            )
        } else {
            let approach_from_left = penult_dx > 0.0;
            (
                Point::new(prev_pt.x, dst_cy),
                Point::new(
                    if approach_from_left {
                        dst_rect.right
                    } else {
                        dst_rect.left
                    },
                    dst_cy,
                ),
            )
        };

        // Reject when the new approach segment would cross a real node (dst excepted) or a
        // label rect.
        let dst_excluded: Vec<&str> = dst_id.as_deref().into_iter().collect();
        if segment_hits_any_rect(new_prev, new_end, &real_node_rects, &dst_excluded, -BUFFER) {
            continue;
        }
        if segment_hits_any_rect(new_prev, new_end, &label_rects, &[], -BUFFER) {
            continue;
        }

        // Reject when new prev' lies inside the source node's rect (pathological).
        if let Some(src) = src_id.as_ref().and_then(|id| node_by_id.get(id))
            && let Some(src_rect) = rect_of_node_bounds(src)
            && point_inside_rect(new_prev, src_rect, BUFFER)
        {
            continue;
        }

        // Reject when the new approach segment crosses another edge's segment.
        let mut self_segments: HashSet<String> = HashSet::new();
        for pair in pts.windows(2) {
            self_segments.insert(own_segment_key(pair[0], pair[1]));
        }
        if crosses_other_edge(new_prev, new_end, edge_index, &layout.edges, &self_segments) {
            continue;
        }

        // The segment before the original prev shifts its far end when the axis moves, so
        // re-check the extended segment for obstacles.
        if n_last >= 3 {
            let before_prev = pts[n_last - 3];
            let endpoint_ids: Vec<&str> = [src_id.as_deref(), dst_id.as_deref()]
                .into_iter()
                .flatten()
                .collect();
            if segment_hits_any_rect(
                before_prev,
                new_prev,
                &real_node_rects,
                &endpoint_ids,
                -BUFFER,
            ) {
                continue;
            }
            if crosses_other_edge(
                before_prev,
                new_prev,
                edge_index,
                &layout.edges,
                &self_segments,
            ) {
                continue;
            }
        }

        // Build the rewritten polyline: the head, then newPrev and newEnd (one fewer bend).
        let mut new_points: Vec<Point> = pts[..n_last - 2].to_vec();
        new_points.push(new_prev);
        new_points.push(new_end);
        layout.edges[edge_index].points = Some(new_points.clone());

        // Re-anchor the edge's label onto the new polyline: the original pass ran earlier and
        // placed the label against the old geometry.
        let label_id = layout.edges[edge_index].label_node_id.clone();
        if let Some((x, y)) = reanchor_label(&new_points, label_id.as_deref(), &node_by_id)
            && let Some(label_id) = label_id.as_deref()
            && let Some(at) = layout.nodes.iter().position(|node| node.id == label_id)
        {
            layout.nodes[at].x = Some(x);
            layout.nodes[at].y = Some(y);
        }
    }
}
