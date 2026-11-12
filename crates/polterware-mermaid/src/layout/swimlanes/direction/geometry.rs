//! Port of `direction/geometry.ts`: the shared orthogonal-geometry helpers of the swimlane
//! passes. Every predicate and measurement keeps upstream's epsilon and evaluation order.

use std::collections::HashMap;

use crate::render::pipeline::types::{Edge, Node};
use crate::scene::Point;

pub const EPS: f64 = 1e-3;

/// One side of a node's bounding box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RectSide {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RectBounds {
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RectEntry {
    pub id: String,
    pub rect: RectBounds,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NodeBoundsInfo {
    pub id: String,
    pub cx: f64,
    pub cy: f64,
    pub rect: RectBounds,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NodePairGeometry {
    pub src_id: String,
    pub dst_id: String,
    pub src_info: NodeBoundsInfo,
    pub dst_info: NodeBoundsInfo,
    pub collinear_x: bool,
    pub collinear_y: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayoutNodeRect {
    pub node_id: String,
    pub rect: RectBounds,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreeSegmentKind {
    Hvh,
    Vhv,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ThreeSegmentRoute {
    pub kind: ThreeSegmentKind,
    pub p0: Point,
    pub p1: Point,
    pub p2: Point,
    pub p3: Point,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrthogonalSegment {
    pub index: usize,
    pub a: Point,
    pub b: Point,
    pub horizontal: bool,
    pub vertical: bool,
}

/// The node bounds and rect lists two passes share.
#[derive(Clone, Debug, Default)]
pub struct RealNodeBounds {
    pub node_info_by_id: HashMap<String, NodeBoundsInfo>,
    pub real_node_rects: Vec<RectEntry>,
}

#[derive(Clone, Debug, Default)]
pub struct NodeRectEntries {
    pub real_node_rects: Vec<RectEntry>,
    pub label_node_rects: Vec<RectEntry>,
}

/// `measuredNodeRect`: a node's centre and rect, when the diagram measured it.
pub fn measured_node_rect(node: &Node) -> Option<(f64, f64, RectBounds)> {
    let cx = node.x.unwrap_or(0.0);
    let cy = node.y.unwrap_or(0.0);
    let width = node.width.unwrap_or(0.0);
    let height = node.height.unwrap_or(0.0);
    if width > 0.0 && height > 0.0 {
        Some((cx, cy, rect_from_center_size(cx, cy, width, height)))
    } else {
        None
    }
}

pub fn node_bounds_info_for(node: &Node) -> Option<NodeBoundsInfo> {
    if node.is_group.unwrap_or(false) {
        return None;
    }
    let (cx, cy, rect) = measured_node_rect(node)?;
    Some(NodeBoundsInfo {
        id: node.id.clone(),
        cx,
        cy,
        rect,
    })
}

pub fn same_point(a: Point, b: Point, epsilon: f64) -> bool {
    (a.x - b.x).abs() < epsilon && (a.y - b.y).abs() < epsilon
}

pub fn same_x(a: Point, b: Point, epsilon: f64) -> bool {
    (a.x - b.x).abs() < epsilon
}

pub fn same_y(a: Point, b: Point, epsilon: f64) -> bool {
    (a.y - b.y).abs() < epsilon
}

pub fn is_horizontal_segment(a: Point, b: Point, epsilon: f64) -> bool {
    same_y(a, b, epsilon) && (a.x - b.x).abs() > epsilon
}

pub fn is_vertical_segment(a: Point, b: Point, epsilon: f64) -> bool {
    same_x(a, b, epsilon) && (a.y - b.y).abs() > epsilon
}

pub fn overlap_length(a1: f64, a2: f64, b1: f64, b2: f64) -> f64 {
    f64::max(
        0.0,
        f64::min(f64::max(a1, a2), f64::max(b1, b2)) - f64::max(f64::min(a1, a2), f64::min(b1, b2)),
    )
}

pub fn same_axis_segment_overlap_length(
    a: &OrthogonalSegment,
    b: &OrthogonalSegment,
    epsilon: f64,
) -> f64 {
    if a.horizontal && b.horizontal && same_y(a.a, b.a, epsilon) {
        return overlap_length(a.a.x, a.b.x, b.a.x, b.b.x);
    }
    if a.vertical && b.vertical && same_x(a.a, b.a, epsilon) {
        return overlap_length(a.a.y, a.b.y, b.a.y, b.b.y);
    }
    0.0
}

pub fn orthogonal_segments_for_points(points: &[Point], epsilon: f64) -> Vec<OrthogonalSegment> {
    let mut result = Vec::new();
    for index in 0..points.len().saturating_sub(1) {
        let a = points[index];
        let b = points[index + 1];
        let horizontal = is_horizontal_segment(a, b, epsilon);
        let vertical = is_vertical_segment(a, b, epsilon);
        if horizontal || vertical {
            result.push(OrthogonalSegment {
                index,
                a,
                b,
                horizontal,
                vertical,
            });
        }
    }
    result
}

pub fn count_orthogonal_bends(points: &[Point], epsilon: f64) -> usize {
    let segments = orthogonal_segments_for_points(points, epsilon);
    let mut bends = 0;
    for index in 1..segments.len() {
        if segments[index - 1].horizontal != segments[index].horizontal {
            bends += 1;
        }
    }
    bends
}

fn bend_points(points: &[Point]) -> Vec<Point> {
    let segments = orthogonal_segments_for_points(&dedupe_consecutive_points(points, EPS), EPS);
    let mut bends = Vec::new();
    for (offset, segment) in segments.iter().enumerate().skip(1) {
        if segments[offset - 1].horizontal != segment.horizontal {
            bends.push(segment.a);
        }
    }
    bends
}

fn distance_to_segment(p: Point, segment: &OrthogonalSegment) -> f64 {
    let dx = f64::max(
        f64::min(segment.a.x, segment.b.x) - p.x,
        f64::max(0.0, p.x - f64::max(segment.a.x, segment.b.x)),
    );
    let dy = f64::max(
        f64::min(segment.a.y, segment.b.y) - p.y,
        f64::max(0.0, p.y - f64::max(segment.a.y, segment.b.y)),
    );
    dx.hypot(dy)
}

/// True when a bend of one route lies within `tolerance` of the other route.
pub fn routes_touch_at_bend(first: &[Point], second: &[Point], tolerance: f64) -> bool {
    let touches = |bends_of: &[Point], path: &[Point]| -> bool {
        let segments = orthogonal_segments_for_points(&dedupe_consecutive_points(path, EPS), EPS);
        bend_points(bends_of).iter().any(|bend| {
            segments
                .iter()
                .any(|segment| distance_to_segment(*bend, segment) < tolerance)
        })
    };
    touches(first, second) || touches(second, first)
}

pub fn dedupe_consecutive_points(points: &[Point], epsilon: f64) -> Vec<Point> {
    let mut result: Vec<Point> = Vec::new();
    for point in points {
        let keep = match result.last() {
            Some(last) => !same_point(*last, *point, epsilon),
            None => true,
        };
        if keep {
            result.push(*point);
        }
    }
    result
}

pub fn classify_three_segment_route(
    points: Option<&[Point]>,
    epsilon: f64,
) -> Option<ThreeSegmentRoute> {
    let points = points?;
    if points.len() != 4 {
        return None;
    }
    let [p0, p1, p2, p3] = [points[0], points[1], points[2], points[3]];
    if is_horizontal_segment(p0, p1, epsilon)
        && is_vertical_segment(p1, p2, epsilon)
        && is_horizontal_segment(p2, p3, epsilon)
    {
        return Some(ThreeSegmentRoute {
            kind: ThreeSegmentKind::Hvh,
            p0,
            p1,
            p2,
            p3,
        });
    }
    let is_vhv = is_vertical_segment(p0, p1, epsilon)
        && is_horizontal_segment(p1, p2, epsilon)
        && is_vertical_segment(p2, p3, epsilon);
    is_vhv.then_some(ThreeSegmentRoute {
        kind: ThreeSegmentKind::Vhv,
        p0,
        p1,
        p2,
        p3,
    })
}

pub fn segment_bounds_overlap_rect(a: Point, b: Point, rect: RectBounds, buffer: f64) -> bool {
    let seg_min_x = f64::min(a.x, b.x);
    let seg_max_x = f64::max(a.x, b.x);
    let seg_min_y = f64::min(a.y, b.y);
    let seg_max_y = f64::max(a.y, b.y);
    seg_max_x > rect.left - buffer
        && seg_min_x < rect.right + buffer
        && seg_max_y > rect.top - buffer
        && seg_min_y < rect.bottom + buffer
}

pub fn point_inside_rect(point: Point, rect: RectBounds, buffer: f64) -> bool {
    point.x > rect.left + buffer
        && point.x < rect.right - buffer
        && point.y > rect.top + buffer
        && point.y < rect.bottom - buffer
}

pub fn rect_contains_rect(outer: RectBounds, inner: RectBounds) -> bool {
    outer.left <= inner.left
        && outer.right >= inner.right
        && outer.top <= inner.top
        && outer.bottom >= inner.bottom
}

pub fn rects_overlap(a: RectBounds, b: RectBounds) -> bool {
    a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top
}

pub fn inflate_rect(rect: RectBounds, margin: f64) -> RectBounds {
    RectBounds {
        left: rect.left - margin,
        right: rect.right + margin,
        top: rect.top - margin,
        bottom: rect.bottom + margin,
    }
}

pub fn rect_from_center_size(cx: f64, cy: f64, width: f64, height: f64) -> RectBounds {
    RectBounds {
        left: cx - width / 2.0,
        right: cx + width / 2.0,
        top: cy - height / 2.0,
        bottom: cy + height / 2.0,
    }
}

pub fn rect_of_node_bounds(node: &Node) -> Option<RectBounds> {
    measured_node_rect(node).map(|(_, _, rect)| rect)
}

pub fn port_for_rect_side(node: &NodeBoundsInfo, side: RectSide) -> Point {
    match side {
        RectSide::Top => Point::new(node.cx, node.rect.top),
        RectSide::Bottom => Point::new(node.cx, node.rect.bottom),
        RectSide::Left => Point::new(node.rect.left, node.cy),
        RectSide::Right => Point::new(node.rect.right, node.cy),
    }
}

pub fn build_orthogonal_port_path(
    src: Point,
    src_side: RectSide,
    dst: Point,
    dst_side: RectSide,
    anchor: f64,
    epsilon: f64,
) -> Option<Vec<Point>> {
    let src_h = matches!(src_side, RectSide::Left | RectSide::Right);
    let dst_h = matches!(dst_side, RectSide::Left | RectSide::Right);

    if src_h && dst_h {
        let opposing_dir =
            (src_side == RectSide::Right && dst_side == RectSide::Left && src.x < dst.x)
                || (src_side == RectSide::Left && dst_side == RectSide::Right && src.x > dst.x);
        if opposing_dir {
            if same_y(src, dst, epsilon) {
                return Some(vec![src, dst]);
            }
            let mid_x = (src.x + dst.x) / 2.0;
            return Some(vec![
                src,
                Point::new(mid_x, src.y),
                Point::new(mid_x, dst.y),
                dst,
            ]);
        }
        if src_side == dst_side {
            if same_y(src, dst, epsilon) {
                return None;
            }
            let int_x = if src_side == RectSide::Left {
                f64::min(src.x, dst.x) - anchor
            } else {
                f64::max(src.x, dst.x) + anchor
            };
            return Some(vec![
                src,
                Point::new(int_x, src.y),
                Point::new(int_x, dst.y),
                dst,
            ]);
        }
        return None;
    }

    if !src_h && !dst_h {
        if src_side == dst_side {
            if same_x(src, dst, epsilon) {
                return None;
            }
            let int_y = if src_side == RectSide::Top {
                f64::min(src.y, dst.y) - anchor
            } else {
                f64::max(src.y, dst.y) + anchor
            };
            return Some(vec![
                src,
                Point::new(src.x, int_y),
                Point::new(dst.x, int_y),
                dst,
            ]);
        }
        let same_dir = (src_side == RectSide::Bottom && dst_side == RectSide::Top && src.y < dst.y)
            || (src_side == RectSide::Top && dst_side == RectSide::Bottom && src.y > dst.y);
        if !same_dir {
            return None;
        }
        if same_x(src, dst, epsilon) {
            return Some(vec![src, dst]);
        }
        let mid_y = (src.y + dst.y) / 2.0;
        return Some(vec![
            src,
            Point::new(src.x, mid_y),
            Point::new(dst.x, mid_y),
            dst,
        ]);
    }

    if src_h && !dst_h {
        let same_dir_src = (src_side == RectSide::Right && dst.x > src.x)
            || (src_side == RectSide::Left && dst.x < src.x);
        let same_dir_dst = (dst_side == RectSide::Top && src.y < dst.y)
            || (dst_side == RectSide::Bottom && src.y > dst.y);
        return (same_dir_src && same_dir_dst).then(|| vec![src, Point::new(dst.x, src.y), dst]);
    }

    let same_dir_src = (src_side == RectSide::Bottom && dst.y > src.y)
        || (src_side == RectSide::Top && dst.y < src.y);
    let same_dir_dst = (dst_side == RectSide::Left && src.x < dst.x)
        || (dst_side == RectSide::Right && src.x > dst.x);
    (same_dir_src && same_dir_dst).then(|| vec![src, Point::new(src.x, dst.y), dst])
}

pub fn build_same_side_track_path(
    src: Point,
    side: RectSide,
    dst: Point,
    track: f64,
) -> Vec<Point> {
    if matches!(side, RectSide::Left | RectSide::Right) {
        vec![src, Point::new(track, src.y), Point::new(track, dst.y), dst]
    } else {
        vec![src, Point::new(src.x, track), Point::new(dst.x, track), dst]
    }
}

pub fn collect_real_node_bounds(nodes: &[Node]) -> RealNodeBounds {
    let mut node_info_by_id = HashMap::new();
    let mut real_node_rects = Vec::new();
    for node in nodes {
        if node.is_edge_label.unwrap_or(false) {
            continue;
        }
        let Some(info) = node_bounds_info_for(node) else {
            continue;
        };
        real_node_rects.push(RectEntry {
            id: info.id.clone(),
            rect: info.rect,
        });
        node_info_by_id.insert(info.id.clone(), info);
    }
    RealNodeBounds {
        node_info_by_id,
        real_node_rects,
    }
}

pub fn collect_node_rect_entries(nodes: &[Node]) -> NodeRectEntries {
    let mut real_node_rects = Vec::new();
    let mut label_node_rects = Vec::new();
    for node in nodes {
        let Some(info) = node_bounds_info_for(node) else {
            continue;
        };
        let entry = RectEntry {
            id: info.id,
            rect: info.rect,
        };
        if node.is_edge_label.unwrap_or(false) {
            label_node_rects.push(entry);
        } else {
            real_node_rects.push(entry);
        }
    }
    NodeRectEntries {
        real_node_rects,
        label_node_rects,
    }
}

pub fn collect_layout_node_rects(nodes: &[Node], include_edge_labels: bool) -> Vec<LayoutNodeRect> {
    let mut result = Vec::new();
    for node in nodes {
        if node.is_group.unwrap_or(false)
            || (!include_edge_labels && node.is_edge_label.unwrap_or(false))
        {
            continue;
        }
        let cx = node.x.unwrap_or(0.0);
        let cy = node.y.unwrap_or(0.0);
        let width = node.width.unwrap_or(0.0);
        let height = node.height.unwrap_or(0.0);
        result.push(LayoutNodeRect {
            node_id: node.id.clone(),
            rect: rect_from_center_size(cx, cy, width, height),
        });
    }
    result
}

pub fn get_node_pair_geometry(
    edge: &Edge,
    node_info_by_id: &HashMap<String, NodeBoundsInfo>,
    epsilon: f64,
) -> Option<NodePairGeometry> {
    let src_id = edge.start.clone()?;
    let dst_id = edge.end.clone()?;
    let src_info = node_info_by_id.get(&src_id)?.clone();
    let dst_info = node_info_by_id.get(&dst_id)?.clone();
    Some(NodePairGeometry {
        src_id,
        dst_id,
        collinear_x: (src_info.cx - dst_info.cx).abs() < epsilon,
        collinear_y: (src_info.cy - dst_info.cy).abs() < epsilon,
        src_info,
        dst_info,
    })
}

pub fn segment_hits_any_rect(
    a: Point,
    b: Point,
    rects: &[RectEntry],
    exclude_ids: &[&str],
    shrink: f64,
) -> bool {
    for entry in rects {
        if exclude_ids.contains(&entry.id.as_str()) {
            continue;
        }
        if segment_bounds_overlap_rect(a, b, entry.rect, -shrink) {
            return true;
        }
    }
    false
}

pub fn orthogonal_segments_cross(
    a1: Point,
    b1: Point,
    a2: Point,
    b2: Point,
    epsilon: f64,
    endpoint_tolerance: f64,
) -> bool {
    let s1_h = same_y(a1, b1, epsilon);
    let s1_v = same_x(a1, b1, epsilon);
    let s2_h = same_y(a2, b2, epsilon);
    let s2_v = same_x(a2, b2, epsilon);
    if (s1_h && s2_h) || (s1_v && s2_v) {
        return false;
    }
    if !(s1_h || s1_v) || !(s2_h || s2_v) {
        return false;
    }

    let horiz = if s1_h { (a1, b1) } else { (a2, b2) };
    let vert = if s1_v { (a1, b1) } else { (a2, b2) };
    let h_y = horiz.0.y;
    let h_x1 = f64::min(horiz.0.x, horiz.1.x);
    let h_x2 = f64::max(horiz.0.x, horiz.1.x);
    let v_x = vert.0.x;
    let v_y1 = f64::min(vert.0.y, vert.1.y);
    let v_y2 = f64::max(vert.0.y, vert.1.y);
    if v_x < h_x1 || v_x > h_x2 || h_y < v_y1 || h_y > v_y2 {
        return false;
    }

    let matches_horiz_endpoint = ((v_x - horiz.0.x).abs() < endpoint_tolerance
        && (h_y - horiz.0.y).abs() < endpoint_tolerance)
        || ((v_x - horiz.1.x).abs() < endpoint_tolerance
            && (h_y - horiz.1.y).abs() < endpoint_tolerance);
    let matches_vert_endpoint = ((v_x - vert.0.x).abs() < endpoint_tolerance
        && (h_y - vert.0.y).abs() < endpoint_tolerance)
        || ((v_x - vert.1.x).abs() < endpoint_tolerance
            && (h_y - vert.1.y).abs() < endpoint_tolerance);
    !(matches_horiz_endpoint && matches_vert_endpoint)
}

pub fn same_axis_segments_overlap(
    a1: Point,
    b1: Point,
    a2: Point,
    b2: Point,
    epsilon: f64,
) -> bool {
    let s1_h = same_y(a1, b1, epsilon);
    let s1_v = same_x(a1, b1, epsilon);
    let s2_h = same_y(a2, b2, epsilon);
    let s2_v = same_x(a2, b2, epsilon);
    if s1_v && s2_v && same_x(a1, a2, epsilon) {
        return overlap_length(a1.y, b1.y, a2.y, b2.y) > epsilon;
    }
    if s1_h && s2_h && same_y(a1, a2, epsilon) {
        return overlap_length(a1.x, b1.x, a2.x, b2.x) > epsilon;
    }
    false
}

pub fn segment_conflicts_with_any_edge(
    a: Point,
    b: Point,
    edges: &[Edge],
    exclude_edge: Option<usize>,
    epsilon: f64,
    skip_degenerate_other: bool,
) -> bool {
    for (index, other) in edges.iter().enumerate() {
        if exclude_edge == Some(index) || other.is_layout_only.unwrap_or(false) {
            continue;
        }
        let Some(points) = other.points.as_deref() else {
            continue;
        };
        if points.len() < 2 {
            continue;
        }
        for pair in points.windows(2) {
            let (oa, ob) = (pair[0], pair[1]);
            if skip_degenerate_other && same_point(oa, ob, epsilon) {
                continue;
            }
            if orthogonal_segments_cross(a, b, oa, ob, epsilon, 1e-6)
                || same_axis_segments_overlap(a, b, oa, ob, epsilon)
            {
                return true;
            }
        }
    }
    false
}

pub fn orthogonal_segments_strictly_cross(
    a1: Point,
    b1: Point,
    a2: Point,
    b2: Point,
    epsilon: f64,
) -> bool {
    let a_horiz = same_y(a1, b1, epsilon);
    let a_vert = same_x(a1, b1, epsilon);
    let b_horiz = same_y(a2, b2, epsilon);
    let b_vert = same_x(a2, b2, epsilon);
    if !((a_horiz && b_vert) || (a_vert && b_horiz)) {
        return false;
    }

    let horiz = if a_horiz { (a1, b1) } else { (a2, b2) };
    let vert = if a_horiz { (a2, b2) } else { (a1, b1) };
    let h_y = horiz.0.y;
    let h_x_min = f64::min(horiz.0.x, horiz.1.x);
    let h_x_max = f64::max(horiz.0.x, horiz.1.x);
    let v_x = vert.0.x;
    let v_y_min = f64::min(vert.0.y, vert.1.y);
    let v_y_max = f64::max(vert.0.y, vert.1.y);
    v_x > h_x_min + epsilon
        && v_x < h_x_max - epsilon
        && h_y > v_y_min + epsilon
        && h_y < v_y_max - epsilon
}

fn strictly_between(value: f64, a: f64, b: f64) -> bool {
    let lo = f64::min(a, b);
    let hi = f64::max(a, b);
    value > lo + EPS && value < hi - EPS
}

fn is_collinear_intermediate(prev: Point, cur: Point, next: Point) -> bool {
    if same_x(prev, cur, EPS) && same_x(cur, next, EPS) {
        return strictly_between(cur.y, prev.y, next.y);
    }
    if same_y(prev, cur, EPS) && same_y(cur, next, EPS) {
        return strictly_between(cur.x, prev.x, next.x);
    }
    false
}

fn simplify_polyline_once(points: &[Point]) -> (Vec<Point>, bool) {
    let mut changed = false;
    let mut out: Vec<Point> = Vec::new();

    let mut index = 0;
    while index < points.len() {
        let prev = out.last().copied();
        let cur = points[index];
        let next = points.get(index + 1).copied();
        if let (Some(prev), Some(next)) = (prev, next) {
            if same_point(prev, next, EPS) {
                changed = true;
                index += 2;
                continue;
            }
            if is_collinear_intermediate(prev, cur, next) {
                changed = true;
                index += 1;
                continue;
            }
        }
        out.push(cur);
        index += 1;
    }

    (out, changed)
}

/// Inserts orthogonal L-bends and removes consecutive duplicate points.
pub fn orthogonalize_polyline(points: &[Point]) -> Vec<Point> {
    let Some(first) = points.first() else {
        return Vec::new();
    };
    let mut cleaned: Vec<Point> = vec![*first];
    for curr in points.iter().skip(1) {
        let prev = cleaned[cleaned.len() - 1];
        if !same_x(prev, *curr, EPS) && !same_y(prev, *curr, EPS) {
            let prev_prev = cleaned.len().checked_sub(2).map(|at| cleaned[at]);
            let incoming_vertical = prev_prev.is_some_and(|prev_prev| same_x(prev_prev, prev, EPS));
            let corner = if incoming_vertical {
                Point::new(prev.x, curr.y)
            } else {
                Point::new(curr.x, prev.y)
            };
            cleaned.push(corner);
        }
        cleaned.push(*curr);
    }
    let mut deduped: Vec<Point> = Vec::new();
    for point in cleaned {
        let keep = match deduped.last() {
            Some(last) => !same_point(*last, point, EPS),
            None => true,
        };
        if keep {
            deduped.push(point);
        }
    }
    deduped
}

pub fn simplify_polyline(points: &[Point]) -> Vec<Point> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut work = points.to_vec();
    for _ in 0..32 {
        let (next, changed) = simplify_polyline_once(&work);
        work = next;
        if !changed {
            break;
        }
    }
    work
}
