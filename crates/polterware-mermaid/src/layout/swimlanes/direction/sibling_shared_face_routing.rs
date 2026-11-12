//! Port of `direction/siblingSharedFaceRouting.ts`: the co-route sibling straight-line rescue.
//! It fires only on the narrow four-point U-detour whose obvious straight line is geometrically
//! clear, shifting both attach points along the shared face so the line keeps a sibling's
//! arrowhead spacing.

use std::collections::HashMap;

use crate::render::pipeline::types::{Edge, Node};
use crate::scene::Point;

use super::super::config::EDGE_ROUTING;
use super::geometry::{
    NodeBoundsInfo, RectBounds, classify_three_segment_route, collect_real_node_bounds,
    get_node_pair_geometry, segment_conflicts_with_any_edge, segment_hits_any_rect,
};

const EPS: f64 = 1e-6;
const MIN_PORT_SPACING: f64 = 8.0;
const PORT_SHIFT: f64 = MIN_PORT_SPACING / 2.0;
const LABEL_CLEARANCE_BUFFER: f64 = 3.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShiftAxis {
    X,
    Y,
}

impl ShiftAxis {
    fn coordinate(self, point: Point) -> f64 {
        match self {
            ShiftAxis::X => point.x,
            ShiftAxis::Y => point.y,
        }
    }

    fn across(self, point: Point) -> f64 {
        match self {
            ShiftAxis::X => point.y,
            ShiftAxis::Y => point.x,
        }
    }
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

fn pair_key(a: &str, b: &str) -> String {
    if a < b {
        format!("{a}::{b}")
    } else {
        format!("{b}::{a}")
    }
}

/// True when another edge already ends or starts on the same face of `node_id` within an
/// arrowhead of `end`. `axis` runs along the face.
fn crowds_face_neighbour(
    edges: &[Edge],
    edge_index: usize,
    node_id: &str,
    centre: Point,
    end: Point,
    axis: ShiftAxis,
) -> bool {
    let face_side = js_sign(axis.across(end) - axis.across(centre));
    for (other_index, other) in edges.iter().enumerate() {
        if other_index == edge_index || other.is_layout_only.unwrap_or(false) {
            continue;
        }
        let Some(points) = other.points.as_deref() else {
            continue;
        };
        if points.is_empty() {
            continue;
        }
        let mut attached: Vec<Point> = Vec::new();
        if other.end.as_deref() == Some(node_id) {
            attached.push(points[points.len() - 1]);
        }
        if other.start.as_deref() == Some(node_id) {
            attached.push(points[0]);
        }
        if attached.iter().any(|point| {
            js_sign(axis.across(*point) - axis.across(centre)) == face_side
                && (axis.coordinate(*point) - axis.coordinate(end)).abs()
                    < EDGE_ROUTING.min_arrowhead_spacing
        }) {
            return true;
        }
    }
    false
}

fn label_clearance_for(
    edges: &[Edge],
    this_edge_index: usize,
    this_src_id: &str,
    this_dst_id: &str,
    axis: ShiftAxis,
    label_dim_by_id: &HashMap<String, (f64, f64)>,
) -> f64 {
    let target_pair = pair_key(this_src_id, this_dst_id);
    let mut max_half = 0.0_f64;
    let mut consider = |label_id: Option<&str>| {
        let Some(label_id) = label_id else {
            return;
        };
        let Some(&(width, height)) = label_dim_by_id.get(label_id) else {
            return;
        };
        let half = match axis {
            ShiftAxis::X => width / 2.0,
            ShiftAxis::Y => height / 2.0,
        };
        if half > max_half {
            max_half = half;
        }
    };
    consider(edges[this_edge_index].label_node_id.as_deref());
    for (other_index, other) in edges.iter().enumerate() {
        if other_index == this_edge_index || other.is_layout_only.unwrap_or(false) {
            continue;
        }
        let (Some(other_src), Some(other_dst)) = (other.start.as_deref(), other.end.as_deref())
        else {
            continue;
        };
        if pair_key(other_src, other_dst) != target_pair {
            continue;
        }
        consider(other.label_node_id.as_deref());
    }
    if max_half > 0.0 {
        max_half + LABEL_CLEARANCE_BUFFER
    } else {
        0.0
    }
}

fn target_points(
    src_info: &NodeBoundsInfo,
    dst_info: &NodeBoundsInfo,
    collinear_x: bool,
) -> (Point, Point) {
    if collinear_x {
        let dst_below = dst_info.cy > src_info.cy;
        (
            Point::new(
                src_info.cx,
                if dst_below {
                    src_info.rect.bottom
                } else {
                    src_info.rect.top
                },
            ),
            Point::new(
                dst_info.cx,
                if dst_below {
                    dst_info.rect.top
                } else {
                    dst_info.rect.bottom
                },
            ),
        )
    } else {
        let dst_east = dst_info.cx > src_info.cx;
        (
            Point::new(
                if dst_east {
                    src_info.rect.right
                } else {
                    src_info.rect.left
                },
                src_info.cy,
            ),
            Point::new(
                if dst_east {
                    dst_info.rect.left
                } else {
                    dst_info.rect.right
                },
                dst_info.cy,
            ),
        )
    }
}

fn bounds_allow_shift(shifted: Point, axis: ShiftAxis, rect: RectBounds) -> bool {
    let coordinate = axis.coordinate(shifted);
    match axis {
        ShiftAxis::X => coordinate > rect.left && coordinate < rect.right,
        ShiftAxis::Y => coordinate > rect.top && coordinate < rect.bottom,
    }
}

/// `straightenCollinearSiblingDetours`: replaces a four-point U-detour around a collinear
/// blocker with a straight line, shifted along the shared face until it clears labels and
/// siblings.
pub fn straighten_collinear_sibling_detours(edges: &mut [Edge], nodes: &[Node]) {
    let bounds = collect_real_node_bounds(nodes);
    let node_info_by_id = &bounds.node_info_by_id;

    let mut label_dim_by_id: HashMap<String, (f64, f64)> = HashMap::new();
    for node in nodes {
        if node.is_group.unwrap_or(false) {
            continue;
        }
        if node.is_edge_label.unwrap_or(false) {
            label_dim_by_id.insert(
                node.id.clone(),
                (node.width.unwrap_or(0.0), node.height.unwrap_or(0.0)),
            );
        }
    }

    for edge_index in 0..edges.len() {
        if edges[edge_index].is_layout_only.unwrap_or(false) {
            continue;
        }
        let Some(points) = edges[edge_index].points.as_deref() else {
            continue;
        };
        if classify_three_segment_route(Some(points), EPS).is_none() {
            continue;
        }

        let Some(node_pair) = get_node_pair_geometry(&edges[edge_index], node_info_by_id, EPS)
        else {
            continue;
        };
        if node_pair.collinear_x == node_pair.collinear_y {
            continue;
        }

        let (target_src, target_dst) = target_points(
            &node_pair.src_info,
            &node_pair.dst_info,
            node_pair.collinear_x,
        );
        if segment_hits_any_rect(
            target_src,
            target_dst,
            &bounds.real_node_rects,
            &[&node_pair.src_id, &node_pair.dst_id],
            1.0,
        ) {
            continue;
        }

        // The rescue moves the line perpendicular to its own direction: a horizontal rescued
        // line shifts in y (so the label height determines clearance), a vertical one shifts
        // in x (label width). `collinearX` means the rescued line is vertical.
        let shift_axis = if node_pair.collinear_x {
            ShiftAxis::X
        } else {
            ShiftAxis::Y
        };
        let label_shift = label_clearance_for(
            edges,
            edge_index,
            &node_pair.src_id,
            &node_pair.dst_id,
            shift_axis,
            &label_dim_by_id,
        );
        let effective_shift = if label_shift > PORT_SHIFT {
            label_shift
        } else {
            PORT_SHIFT
        };
        let deltas = [0.0, effective_shift, -effective_shift];
        let mut crowded: Option<Vec<Point>> = None;
        for delta in deltas {
            let mut shifted_src = target_src;
            let mut shifted_dst = target_dst;
            if node_pair.collinear_x {
                shifted_src.x += delta;
                shifted_dst.x += delta;
                if !bounds_allow_shift(shifted_src, ShiftAxis::X, node_pair.src_info.rect)
                    || !bounds_allow_shift(shifted_dst, ShiftAxis::X, node_pair.dst_info.rect)
                {
                    continue;
                }
            } else {
                shifted_src.y += delta;
                shifted_dst.y += delta;
                if !bounds_allow_shift(shifted_src, ShiftAxis::Y, node_pair.src_info.rect)
                    || !bounds_allow_shift(shifted_dst, ShiftAxis::Y, node_pair.dst_info.rect)
                {
                    continue;
                }
            }

            if segment_hits_any_rect(
                shifted_src,
                shifted_dst,
                &bounds.real_node_rects,
                &[&node_pair.src_id, &node_pair.dst_id],
                1.0,
            ) {
                continue;
            }

            if segment_conflicts_with_any_edge(
                shifted_src,
                shifted_dst,
                edges,
                Some(edge_index),
                EPS,
                false,
            ) {
                continue;
            }

            let rescued = vec![shifted_src, shifted_dst];
            if crowds_face_neighbour(
                edges,
                edge_index,
                &node_pair.src_id,
                Point::new(node_pair.src_info.cx, node_pair.src_info.cy),
                shifted_src,
                shift_axis,
            ) || crowds_face_neighbour(
                edges,
                edge_index,
                &node_pair.dst_id,
                Point::new(node_pair.dst_info.cx, node_pair.dst_info.cy),
                shifted_dst,
                shift_axis,
            ) {
                if crowded.is_none() {
                    crowded = Some(rescued);
                }
                continue;
            }

            edges[edge_index].points = Some(rescued);
            crowded = None;
            break;
        }
        if let Some(crowded) = crowded {
            edges[edge_index].points = Some(crowded);
        }
    }
}
