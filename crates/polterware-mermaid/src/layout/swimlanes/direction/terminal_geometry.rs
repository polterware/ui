//! Port of `direction/terminalGeometry.ts`: the faces and routes the terminal passes agree on,
//! that is whether a face is free and whether a moved route stays clear of other edges and of
//! the nodes and labels.

use crate::render::pipeline::types::Edge;
use crate::scene::Point;

use super::super::config::EDGE_ROUTING;
use super::geometry::{
    RectBounds, RectEntry, RectSide, dedupe_consecutive_points, orthogonal_segments_for_points,
    orthogonal_segments_strictly_cross, routes_touch_at_bend, same_axis_segments_overlap,
    segment_hits_any_rect,
};

/// How close two coordinates must be to count as equal.
pub const EPS: f64 = 1e-3;
const OBSTACLE_BUFFER: f64 = 2.0;

/// One face of a node's bounding box.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeFace {
    pub node_id: String,
    pub rect: RectBounds,
    pub side: RectSide,
}

#[derive(Clone, Debug, Default)]
pub struct Obstacles {
    pub real_node_rects: Vec<RectEntry>,
    pub label_node_rects: Vec<RectEntry>,
}

/// The axis a face runs along: the coordinate that stays put as the leg slides on it.
pub fn runs_along_face(side: RectSide) -> Axis {
    match side {
        RectSide::Top | RectSide::Bottom => Axis::X,
        RectSide::Left | RectSide::Right => Axis::Y,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
}

impl Axis {
    pub fn coordinate(self, point: Point) -> f64 {
        match self {
            Axis::X => point.x,
            Axis::Y => point.y,
        }
    }

    pub fn set(self, point: &mut Point, value: f64) {
        match self {
            Axis::X => point.x = value,
            Axis::Y => point.y = value,
        }
    }
}

/// Whether an endpoint meets the node through this face: on the face, or beyond it, where the
/// renderer joins the endpoint to the node through the face.
fn endpoint_through_face(point: Point, rect: RectBounds, side: RectSide) -> bool {
    let axis = runs_along_face(side);
    let (lo, hi) = if axis == Axis::X {
        (rect.left, rect.right)
    } else {
        (rect.top, rect.bottom)
    };
    let outward = match side {
        RectSide::Top => rect.top - point.y,
        RectSide::Bottom => point.y - rect.bottom,
        RectSide::Left => rect.left - point.x,
        RectSide::Right => point.x - rect.right,
    };
    outward > -EPS && axis.coordinate(point) >= lo - EPS && axis.coordinate(point) <= hi + EPS
}

/// No edge but `edge` meets the node through the face, so a route moved there lands on a free
/// port.
pub fn face_is_own(face: &NodeFace, edge_index: usize, edges: &[Edge]) -> bool {
    edges.iter().enumerate().all(|(index, other)| {
        if index == edge_index || other.is_layout_only.unwrap_or(false) {
            return true;
        }
        let points = other.points.clone().unwrap_or_default();
        let ends: [(Option<&String>, Option<Point>); 2] = [
            (other.start.as_ref(), points.first().copied()),
            (other.end.as_ref(), points.last().copied()),
        ];
        ends.iter().all(|(id, point)| {
            id.map(String::as_str) != Some(face.node_id.as_str())
                || point.is_none_or(|point| !endpoint_through_face(point, face.rect, face.side))
        })
    })
}

fn strict_crossings(route: &[Point], other: &[Point]) -> usize {
    let other_segments = orthogonal_segments_for_points(other, EPS);
    orthogonal_segments_for_points(route, EPS)
        .iter()
        .map(|segment| {
            other_segments
                .iter()
                .filter(|o| orthogonal_segments_strictly_cross(segment.a, segment.b, o.a, o.b, EPS))
                .count()
        })
        .sum()
}

fn overlaps_any(route: &[Point], other: &[Point]) -> bool {
    let other_segments = orthogonal_segments_for_points(other, EPS);
    orthogonal_segments_for_points(route, EPS)
        .iter()
        .any(|segment| {
            other_segments
                .iter()
                .any(|o| same_axis_segments_overlap(segment.a, segment.b, o.a, o.b, EPS))
        })
}

/// The moved route keeps clear of every other edge at least as well as the current one.
pub fn clear_of_other_edges(
    edge_index: usize,
    current: &[Point],
    moved: &[Point],
    edges: &[Edge],
) -> bool {
    let tolerance = EDGE_ROUTING.route_touch_tolerance;
    edges.iter().enumerate().all(|(index, other)| {
        let points = dedupe_consecutive_points(other.points.as_deref().unwrap_or(&[]), EPS);
        if index == edge_index || other.is_layout_only.unwrap_or(false) || points.len() < 2 {
            return true;
        }
        !overlaps_any(moved, &points)
            && strict_crossings(moved, &points) <= strict_crossings(current, &points)
            && (!routes_touch_at_bend(moved, &points, tolerance)
                || routes_touch_at_bend(current, &points, tolerance))
    })
}

/// The moved route passes no node but its own endpoints at its ends, and no label but its own.
pub fn clear_of_nodes(edge: &Edge, moved: &[Point], obstacles: &Obstacles) -> bool {
    let own_label = edge.label_node_id.clone();
    let endpoint_ids: Vec<&str> = [edge.start.as_deref(), edge.end.as_deref()]
        .into_iter()
        .flatten()
        .collect();
    let segments = orthogonal_segments_for_points(moved, EPS);
    let segment_count = segments.len();
    segments.iter().all(|segment| {
        let touches_endpoint = segment.index == 0 || segment.index + 1 == segment_count;
        let excluded: Vec<&str> = if touches_endpoint {
            endpoint_ids.clone()
        } else {
            Vec::new()
        };
        let label_excluded: Vec<&str> = own_label.as_deref().into_iter().collect();
        !segment_hits_any_rect(
            segment.a,
            segment.b,
            &obstacles.real_node_rects,
            &excluded,
            OBSTACLE_BUFFER,
        ) && !segment_hits_any_rect(
            segment.a,
            segment.b,
            &obstacles.label_node_rects,
            &label_excluded,
            OBSTACLE_BUFFER,
        )
    })
}
