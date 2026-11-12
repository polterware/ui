//! Port of `direction/endpointClip.ts`: clipping a route's endpoints to the borders of the
//! nodes it joins, clearing the corners they land beside, and the renderer hand-off that
//! duplicates the terminal points so the paint-time clip is a no-op.

use crate::render::pipeline::types::{Edge, LayoutData, Node};
use crate::scene::Point;

use super::geometry::{
    EPS, RectBounds, dedupe_consecutive_points, orthogonalize_polyline, point_inside_rect,
    rect_of_node_bounds, same_point, same_x, same_y, simplify_polyline,
};

const INSIDE_EPS: f64 = 0.5;

/// How far endpoint clipping keeps a terminal leg from a node's corner.
pub const CORNER_CLEARANCE: f64 = 4.0;

type BorderSide = super::geometry::RectSide;

struct EndpointContext {
    points: Vec<Point>,
    src_rect: Option<RectBounds>,
    dst_rect: Option<RectBounds>,
}

fn endpoint_context_for(
    edge: &Edge,
    node_by_id: &std::collections::HashMap<String, Node>,
    min_points: usize,
) -> Option<EndpointContext> {
    if edge.is_layout_only.unwrap_or(false) {
        return None;
    }
    let points = edge.points.clone()?;
    if points.len() < min_points {
        return None;
    }
    let src_rect = edge
        .start
        .as_ref()
        .and_then(|id| node_by_id.get(id))
        .and_then(rect_of_node_bounds);
    let dst_rect = edge
        .end
        .as_ref()
        .and_then(|id| node_by_id.get(id))
        .and_then(rect_of_node_bounds);
    Some(EndpointContext {
        points,
        src_rect,
        dst_rect,
    })
}

/// Given an axis-aligned segment from outside a rect to inside it, return the point where the
/// segment enters the rect boundary.
fn segment_enter_point(outside: Point, inside: Point, r: RectBounds) -> Point {
    if same_y(outside, inside, EPS) {
        let x = if outside.x < r.left { r.left } else { r.right };
        return Point::new(x, outside.y);
    }
    if same_x(outside, inside, EPS) {
        let y = if outside.y < r.top { r.top } else { r.bottom };
        return Point::new(outside.x, y);
    }
    Point::new(
        f64::min(r.right, f64::max(r.left, outside.x)),
        f64::min(r.bottom, f64::max(r.top, outside.y)),
    )
}

fn clip_endpoint(points: Vec<Point>, rect: RectBounds, at_start: bool) -> Vec<Point> {
    let step: isize = if at_start { 1 } else { -1 };
    let mut outside_index: isize = if at_start {
        0
    } else {
        points.len() as isize - 1
    };
    while outside_index >= 0
        && (outside_index as usize) < points.len()
        && point_inside_rect(points[outside_index as usize], rect, INSIDE_EPS)
    {
        outside_index += step;
    }
    if outside_index < 0 || outside_index as usize >= points.len() {
        return points;
    }

    let inside_index = outside_index - step;
    if inside_index < 0 || inside_index as usize >= points.len() {
        return points;
    }

    let outside = outside_index as usize;
    let inside = inside_index as usize;
    let entry = segment_enter_point(points[outside], points[inside], rect);
    if at_start {
        let mut next = vec![entry];
        next.extend_from_slice(&points[outside..]);
        next
    } else {
        let mut next: Vec<Point> = points[..=outside].to_vec();
        next.push(entry);
        next
    }
}

/// Clips every edge endpoint that sits inside the node it joins back to that node's border.
pub fn clip_edge_endpoints_to_node_boundaries(layout: &mut LayoutData) {
    let node_by_id: std::collections::HashMap<String, Node> = layout
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node.clone()))
        .collect();
    for edge in &mut layout.edges {
        let Some(context) = endpoint_context_for(edge, &node_by_id, 2) else {
            continue;
        };

        let mut next = context.points;
        if let Some(rect) = context.src_rect {
            next = clip_endpoint(next, rect, true);
        }
        if let Some(rect) = context.dst_rect {
            next = clip_endpoint(next, rect, false);
        }
        next = simplify_polyline(&orthogonalize_polyline(&next));
        next = clear_straight_endpoint_corner_connections(next, context.src_rect, context.dst_rect);
        edge.points = Some(simplify_polyline(&orthogonalize_polyline(&next)));
    }
}

fn snap_endpoint_to_boundary(
    inner: Point,
    endpoint: Point,
    r: RectBounds,
    use_approach_side: bool,
) -> Point {
    if same_y(inner, endpoint, EPS) {
        if endpoint.y < r.top - EPS || endpoint.y > r.bottom + EPS {
            return endpoint;
        }
        if use_approach_side {
            if inner.x < r.left - EPS {
                return Point::new(r.left, inner.y);
            }
            if inner.x > r.right + EPS {
                return Point::new(r.right, inner.y);
            }
        }
        let to_left = (endpoint.x - r.left).abs() <= (endpoint.x - r.right).abs();
        return Point::new(if to_left { r.left } else { r.right }, inner.y);
    }
    if same_x(inner, endpoint, EPS) {
        if endpoint.x < r.left - EPS || endpoint.x > r.right + EPS {
            return endpoint;
        }
        if use_approach_side {
            if inner.y < r.top - EPS {
                return Point::new(inner.x, r.top);
            }
            if inner.y > r.bottom + EPS {
                return Point::new(inner.x, r.bottom);
            }
        }
        let to_top = (endpoint.y - r.top).abs() <= (endpoint.y - r.bottom).abs();
        return Point::new(inner.x, if to_top { r.top } else { r.bottom });
    }
    endpoint
}

fn first_distinct_adjacent(points: &[Point], endpoint_index: usize, step: isize) -> Option<Point> {
    let endpoint = points[endpoint_index];
    let mut index = endpoint_index as isize + step;
    while index >= 0 && (index as usize) < points.len() {
        let candidate = points[index as usize];
        if !same_point(candidate, endpoint, EPS) {
            return Some(candidate);
        }
        index += step;
    }
    points
        .get((endpoint_index as isize + step) as usize)
        .copied()
}

fn corner_clearance_range(min: f64, max: f64) -> (f64, f64) {
    let lo = min + CORNER_CLEARANCE;
    let hi = max - CORNER_CLEARANCE;
    if lo <= hi {
        (lo, hi)
    } else {
        ((min + max) / 2.0, (min + max) / 2.0)
    }
}

fn clamp_to_corner_clearance(value: f64, min: f64, max: f64) -> f64 {
    let (lo, hi) = corner_clearance_range(min, max);
    f64::min(hi, f64::max(lo, value))
}

fn intersect_ranges(ranges: &[(f64, f64)]) -> Option<(f64, f64)> {
    let lo = ranges
        .iter()
        .map(|range| range.0)
        .fold(f64::NEG_INFINITY, f64::max);
    let hi = ranges
        .iter()
        .map(|range| range.1)
        .fold(f64::INFINITY, f64::min);
    if lo > hi {
        return None;
    }
    Some((lo, hi))
}

fn clearance_range_for_side(r: RectBounds, side: BorderSide) -> (f64, f64) {
    if matches!(side, BorderSide::Left | BorderSide::Right) {
        corner_clearance_range(r.top, r.bottom)
    } else {
        corner_clearance_range(r.left, r.right)
    }
}

fn terminal_side_for_segment(
    endpoint: Point,
    adjacent: Point,
    r: RectBounds,
) -> Option<BorderSide> {
    let y_within = endpoint.y >= r.top - EPS && endpoint.y <= r.bottom + EPS;
    let x_within = endpoint.x >= r.left - EPS && endpoint.x <= r.right + EPS;
    if same_y(endpoint, adjacent, EPS) && y_within {
        if (endpoint.x - r.left).abs() < EPS {
            return Some(BorderSide::Left);
        }
        if (endpoint.x - r.right).abs() < EPS {
            return Some(BorderSide::Right);
        }
    }
    if same_x(endpoint, adjacent, EPS) && x_within {
        if (endpoint.y - r.top).abs() < EPS {
            return Some(BorderSide::Top);
        }
        if (endpoint.y - r.bottom).abs() < EPS {
            return Some(BorderSide::Bottom);
        }
    }
    None
}

fn is_horizontal_side(side: BorderSide) -> bool {
    matches!(side, BorderSide::Left | BorderSide::Right)
}

fn straight_clearance_range(
    start: Point,
    end: Point,
    src_rect: Option<RectBounds>,
    dst_rect: Option<RectBounds>,
    horizontal: bool,
) -> Option<(f64, f64)> {
    let mut ranges: Vec<(f64, f64)> = Vec::new();
    let src_side = src_rect.and_then(|rect| terminal_side_for_segment(start, end, rect));
    let dst_side = dst_rect.and_then(|rect| terminal_side_for_segment(end, start, rect));

    if let (Some(rect), Some(side)) = (src_rect, src_side)
        && is_horizontal_side(side) == horizontal
    {
        ranges.push(clearance_range_for_side(rect, side));
    }
    if let (Some(rect), Some(side)) = (dst_rect, dst_side)
        && is_horizontal_side(side) == horizontal
    {
        ranges.push(clearance_range_for_side(rect, side));
    }

    if ranges.is_empty() {
        None
    } else {
        intersect_ranges(&ranges)
    }
}

fn clear_straight_endpoint_corner_axis(
    start: Point,
    end: Point,
    src_rect: Option<RectBounds>,
    dst_rect: Option<RectBounds>,
    horizontal: bool,
) -> Option<Vec<Point>> {
    let (lo, hi) = straight_clearance_range(start, end, src_rect, dst_rect, horizontal)?;

    let current = if horizontal { start.y } else { start.x };
    let next = f64::min(hi, f64::max(lo, current));
    if (next - current).abs() < EPS {
        return None;
    }

    Some(if horizontal {
        vec![Point::new(start.x, next), Point::new(end.x, next)]
    } else {
        vec![Point::new(next, start.y), Point::new(next, end.y)]
    })
}

/// Returns the points unchanged when nothing moved, so a caller can tell contact from a move.
fn clear_straight_endpoint_corner_connections(
    points: Vec<Point>,
    src_rect: Option<RectBounds>,
    dst_rect: Option<RectBounds>,
) -> Vec<Point> {
    if points.len() != 2 {
        return points;
    }

    let (start, end) = (points[0], points[1]);
    if same_y(start, end, EPS) {
        return clear_straight_endpoint_corner_axis(start, end, src_rect, dst_rect, true)
            .unwrap_or(points);
    }

    if same_x(start, end, EPS) {
        return clear_straight_endpoint_corner_axis(start, end, src_rect, dst_rect, false)
            .unwrap_or(points);
    }

    points
}

fn corner_cleared_endpoint(endpoint: Point, r: RectBounds, side: BorderSide) -> Point {
    if is_horizontal_side(side) {
        Point::new(
            endpoint.x,
            clamp_to_corner_clearance(endpoint.y, r.top, r.bottom),
        )
    } else {
        Point::new(
            clamp_to_corner_clearance(endpoint.x, r.left, r.right),
            endpoint.y,
        )
    }
}

fn move_collinear_endpoint_run(
    points: &[Point],
    endpoint_index: usize,
    step: isize,
    endpoint: Point,
    adjusted: Point,
    horizontal_terminal: bool,
) -> Vec<Point> {
    let mut next = points.to_vec();
    let mut index = endpoint_index as isize;
    while index >= 0 && (index as usize) < points.len() {
        let point = points[index as usize];
        if horizontal_terminal && !same_y(point, endpoint, EPS) {
            break;
        }
        if !horizontal_terminal && !same_x(point, endpoint, EPS) {
            break;
        }
        if horizontal_terminal {
            next[index as usize].y = adjusted.y;
        } else {
            next[index as usize].x = adjusted.x;
        }
        index += step;
    }
    next
}

fn clear_endpoint_corner_connection(
    points: Vec<Point>,
    r: RectBounds,
    at_start: bool,
) -> Vec<Point> {
    if points.len() < 2 {
        return points;
    }

    let endpoint_index = if at_start { 0 } else { points.len() - 1 };
    let step: isize = if at_start { 1 } else { -1 };
    let endpoint = points[endpoint_index];
    let Some(adjacent) = first_distinct_adjacent(&points, endpoint_index, step) else {
        return points;
    };

    let Some(side) = terminal_side_for_segment(endpoint, adjacent, r) else {
        return points;
    };

    let horizontal_terminal = is_horizontal_side(side);
    let adjusted = corner_cleared_endpoint(endpoint, r, side);
    if same_point(endpoint, adjusted, EPS) {
        return points;
    }

    move_collinear_endpoint_run(
        &points,
        endpoint_index,
        step,
        endpoint,
        adjusted,
        horizontal_terminal,
    )
}

fn border_side_for_segment(a: Point, b: Point, r: RectBounds) -> Option<BorderSide> {
    let x_within = f64::min(a.x, b.x) >= r.left - EPS && f64::max(a.x, b.x) <= r.right + EPS;
    let y_within = f64::min(a.y, b.y) >= r.top - EPS && f64::max(a.y, b.y) <= r.bottom + EPS;
    if (a.y - r.top).abs() < EPS && (b.y - r.top).abs() < EPS && x_within {
        return Some(BorderSide::Top);
    }
    if (a.y - r.bottom).abs() < EPS && (b.y - r.bottom).abs() < EPS && x_within {
        return Some(BorderSide::Bottom);
    }
    if (a.x - r.left).abs() < EPS && (b.x - r.left).abs() < EPS && y_within {
        return Some(BorderSide::Left);
    }
    if (a.x - r.right).abs() < EPS && (b.x - r.right).abs() < EPS && y_within {
        return Some(BorderSide::Right);
    }
    None
}

fn leaves_outward(side: BorderSide, from: Point, to: Point, r: RectBounds) -> bool {
    match side {
        BorderSide::Top => same_x(from, to, EPS) && to.y < r.top - EPS,
        BorderSide::Bottom => same_x(from, to, EPS) && to.y > r.bottom + EPS,
        BorderSide::Left => same_y(from, to, EPS) && to.x < r.left - EPS,
        BorderSide::Right => same_y(from, to, EPS) && to.x > r.right + EPS,
    }
}

fn collapse_own_border_stub(points: Vec<Point>, r: RectBounds, at_start: bool) -> Vec<Point> {
    if points.len() < 3 {
        return points;
    }
    if at_start {
        if let Some(side) = border_side_for_segment(points[0], points[1], r)
            && leaves_outward(side, points[1], points[2], r)
        {
            return points[1..].to_vec();
        }
        return points;
    }

    let last = points.len() - 1;
    if let Some(side) = border_side_for_segment(points[last - 1], points[last], r)
        && leaves_outward(side, points[last - 1], points[last - 2], r)
    {
        return points[..last].to_vec();
    }
    points
}

fn snap_and_collapse_endpoints(
    points: Vec<Point>,
    src_rect: Option<RectBounds>,
    dst_rect: Option<RectBounds>,
) -> Vec<Point> {
    let mut next = points;
    if let Some(rect) = src_rect {
        if let Some(adjacent) = first_distinct_adjacent(&next, 0, 1) {
            let snapped = snap_endpoint_to_boundary(adjacent, next[0], rect, false);
            if !same_point(snapped, next[0], 0.0) {
                next[0] = snapped;
            }
        }
        next = collapse_own_border_stub(next, rect, true);
    }
    if let Some(rect) = dst_rect {
        let last = next.len() - 1;
        if let Some(adjacent) = first_distinct_adjacent(&next, last, -1) {
            let snapped = snap_endpoint_to_boundary(adjacent, next[last], rect, true);
            if !same_point(snapped, next[last], 0.0) {
                next[last] = snapped;
            }
        }
        next = collapse_own_border_stub(next, rect, false);
    }

    let straightened = clear_straight_endpoint_corner_connections(next.clone(), src_rect, dst_rect);
    if straightened != next || next.len() == 2 {
        return straightened;
    }

    if let Some(rect) = src_rect {
        next = clear_endpoint_corner_connection(next, rect, true);
    }
    if let Some(rect) = dst_rect {
        next = clear_endpoint_corner_connection(next, rect, false);
    }
    next
}

/// Pins every edge's endpoints to the boundary it enters and duplicates them, so the renderer's
/// own clip does not move a terminal again.
pub fn prepare_edge_endpoints_for_renderer(layout: &mut LayoutData) {
    let node_by_id: std::collections::HashMap<String, Node> = layout
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node.clone()))
        .collect();
    for edge in &mut layout.edges {
        let Some(context) = endpoint_context_for(edge, &node_by_id, 2) else {
            continue;
        };

        let input = dedupe_consecutive_points(&context.points, EPS);
        let new_points = snap_and_collapse_endpoints(input, context.src_rect, context.dst_rect);
        if new_points.len() < 3 {
            edge.points = Some(new_points);
            continue;
        }
        let mut duplicated = Vec::with_capacity(new_points.len() + 2);
        duplicated.push(new_points[0]);
        duplicated.push(new_points[0]);
        duplicated.extend_from_slice(&new_points[1..new_points.len() - 1]);
        duplicated.push(new_points[new_points.len() - 1]);
        duplicated.push(new_points[new_points.len() - 1]);
        edge.points = Some(duplicated);
    }
}
