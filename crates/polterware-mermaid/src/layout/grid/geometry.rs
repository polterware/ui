//! Port of Mermaid 12.3.0's `rendering-util/layout-algorithms/layout-utils/geometry.ts`, plus the
//! geometry subset of `layout-utils/helpers.ts`.
//!
//! The routing phases measure and normalize orthogonal polylines with these helpers, so the
//! tolerance (`PIXEL_EPSILON`) and the merge order are the upstream ones. `GridOrientation` has no
//! zero-length member; [`SegmentOrientation::Z`] carries that third case locally.

use crate::layout::grid::types::{GridOrientation, Node, RouterPoint};

/// Canonical one-pixel tolerance shared by routing geometry and final layout validation.
pub const PIXEL_EPSILON: f64 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentOrientation {
    H,
    V,
    Z,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub a: RouterPoint,
    pub b: RouterPoint,
    pub orientation: SegmentOrientation,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NormalizedPolyline {
    pub points: Vec<RouterPoint>,
    pub segments: Vec<Segment>,
    pub bends: usize,
}

pub fn segment_orientation(a: RouterPoint, b: RouterPoint) -> SegmentOrientation {
    let dx = (b.x - a.x).abs();
    let dy = (b.y - a.y).abs();
    if dx <= PIXEL_EPSILON && dy <= PIXEL_EPSILON {
        return SegmentOrientation::Z;
    }
    if dy <= PIXEL_EPSILON {
        return SegmentOrientation::H;
    }
    SegmentOrientation::V
}

pub fn segments_from_points(points: &[RouterPoint]) -> Vec<Segment> {
    let mut segments = Vec::new();
    for index in 0..points.len().saturating_sub(1) {
        let a = points[index];
        let b = points[index + 1];
        segments.push(Segment {
            a,
            b,
            orientation: segment_orientation(a, b),
        });
    }
    segments
}

pub fn merge_collinear(segments: &[Segment]) -> Vec<Segment> {
    if segments.is_empty() {
        return Vec::new();
    }
    let mut result = Vec::new();
    let mut current = segments[0];

    for next in &segments[1..] {
        // Skip zero-length segments.
        if next.orientation == SegmentOrientation::Z {
            continue;
        }
        if current.orientation == SegmentOrientation::Z {
            current = *next;
            continue;
        }
        // Merge if same orientation and collinear.
        if current.orientation == next.orientation {
            if current.orientation == SegmentOrientation::H
                && (current.b.y - next.a.y).abs() <= PIXEL_EPSILON
                && (current.a.y - next.a.y).abs() <= PIXEL_EPSILON
            {
                current.b = next.b;
                continue;
            }
            if current.orientation == SegmentOrientation::V
                && (current.b.x - next.a.x).abs() <= PIXEL_EPSILON
                && (current.a.x - next.a.x).abs() <= PIXEL_EPSILON
            {
                current.b = next.b;
                continue;
            }
        }
        if current.orientation != SegmentOrientation::Z {
            result.push(current);
        }
        current = *next;
    }
    if current.orientation != SegmentOrientation::Z {
        result.push(current);
    }
    result
}

pub fn normalize_polyline(points: &[RouterPoint]) -> NormalizedPolyline {
    if points.len() < 2 {
        return NormalizedPolyline {
            points: points.to_vec(),
            segments: Vec::new(),
            bends: 0,
        };
    }

    let raw_segments: Vec<Segment> = segments_from_points(points)
        .into_iter()
        .filter(|segment| segment.orientation != SegmentOrientation::Z)
        .collect();
    let merged = merge_collinear(&raw_segments);

    let mut new_points = Vec::new();
    for (index, segment) in merged.iter().enumerate() {
        if index == 0 {
            new_points.push(segment.a);
        }
        new_points.push(segment.b);
    }

    let mut bends = 0usize;
    for index in 1..merged.len() {
        if merged[index - 1].orientation != merged[index].orientation {
            bends += 1;
        }
    }

    NormalizedPolyline {
        points: if new_points.is_empty() {
            points.to_vec()
        } else {
            new_points
        },
        segments: merged,
        bends,
    }
}

/// A rectangle defined by its center point and boundaries, from `layout-utils/types.ts`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub cx: f64,
    pub cy: f64,
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
}

/// Creates a `Rect` from a node's `x`, `y`, `width`, and `height`; `x` and `y` are centers.
pub fn rect_for_node(node: &Node) -> Rect {
    let cx = node.x.unwrap_or(0.0);
    let cy = node.y.unwrap_or(0.0);
    let w = node.width.unwrap_or(40.0);
    let h = node.height.unwrap_or(40.0);
    Rect {
        cx,
        cy,
        left: cx - w / 2.0,
        right: cx + w / 2.0,
        top: cy - h / 2.0,
        bottom: cy + h / 2.0,
    }
}

pub fn manhattan_length(points: &[RouterPoint]) -> f64 {
    let mut total = 0.0;
    for index in 1..points.len() {
        let a = points[index - 1];
        let b = points[index];
        total += (b.x - a.x).abs() + (b.y - a.y).abs();
    }
    total
}

pub fn manhattan_distance(a: RouterPoint, b: RouterPoint) -> f64 {
    (a.x - b.x).abs() + (a.y - b.y).abs()
}

/// Reports whether an axis-aligned segment passes through a rectangle's interior. A segment that
/// runs exactly along a boundary counts as a collision, so edges never run under a node.
pub fn segment_intersects_rect_interior(a: RouterPoint, b: RouterPoint, rect: Rect) -> bool {
    if a.x == b.x && a.y == b.y {
        return false;
    }

    if a.y == b.y {
        let y = a.y;
        let x1 = a.x.min(b.x);
        let x2 = a.x.max(b.x);
        let crosses_vertically = y >= rect.top && y <= rect.bottom;
        let overlaps_horizontally = x1.max(rect.left) < x2.min(rect.right);
        return crosses_vertically && overlaps_horizontally;
    }

    if a.x == b.x {
        let x = a.x;
        let y1 = a.y.min(b.y);
        let y2 = a.y.max(b.y);
        let crosses_horizontally = x >= rect.left && x <= rect.right;
        let overlaps_vertically = y1.max(rect.top) < y2.min(rect.bottom);
        return crosses_horizontally && overlaps_vertically;
    }

    false
}

pub fn polyline_intersects_rect(points: &[RouterPoint], rect: Rect) -> bool {
    for index in 1..points.len() {
        if segment_intersects_rect_interior(points[index - 1], points[index], rect) {
            return true;
        }
    }
    false
}

pub fn clamp(value: f64, low: f64, high: f64) -> f64 {
    low.max(high.min(value))
}

pub fn to_router_orientation(orientation: SegmentOrientation) -> Option<GridOrientation> {
    match orientation {
        SegmentOrientation::H => Some(GridOrientation::H),
        SegmentOrientation::V => Some(GridOrientation::V),
        SegmentOrientation::Z => None,
    }
}
