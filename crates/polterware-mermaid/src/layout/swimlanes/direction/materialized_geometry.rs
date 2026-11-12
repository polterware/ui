//! Port of `direction/materializedGeometry.ts`: the cleanup passes that run on the rendered
//! swimlane geometry. They separate terminal rails that land on one node face, collapse
//! dog-legs and jogs proven redundant, lift same-side rails out of a node's border, swap two
//! destination tails, and re-channel external rail bundles until the visible routes no longer
//! cross. Every pass keeps upstream's `f64` evaluation order.

use std::collections::{BTreeSet, HashMap};
use std::fmt::Write as _;

use crate::render::pipeline::types::{Edge, Node};
use crate::scene::Point;

use super::super::config::EDGE_ROUTING;
use super::super::helpers::metadata_value;
use super::super::lanes::build_lane_model;
use super::geometry::{
    OrthogonalSegment, RectBounds, RectSide, collect_node_rect_entries, count_orthogonal_bends,
    dedupe_consecutive_points, is_horizontal_segment, is_vertical_segment,
    orthogonal_segments_for_points, orthogonal_segments_strictly_cross, overlap_length,
    rect_of_node_bounds, routes_touch_at_bend, same_axis_segment_overlap_length, same_point,
    same_x, same_y, segment_hits_any_rect, simplify_polyline,
};

const EPS_LOCAL: f64 = 1e-3;
const MIN_SHARED: f64 = 8.0;

type Path = Vec<Point>;
type Candidates = Vec<Path>;
type EdgeReplacementMap = HashMap<usize, Path>;

fn js_sign(value: f64) -> f64 {
    if value > 0.0 {
        1.0
    } else if value < 0.0 {
        -1.0
    } else {
        0.0
    }
}

fn orthogonally_aligned(a: Point, b: Point) -> bool {
    same_x(a, b, EPS_LOCAL) || same_y(a, b, EPS_LOCAL)
}

fn segments_for(points: &[Point]) -> Vec<OrthogonalSegment> {
    orthogonal_segments_for_points(points, EPS_LOCAL)
}

fn upper_case_direction(raw: Option<&str>) -> String {
    raw.map(str::to_uppercase).unwrap_or_default()
}

fn node_direction<'a>(node: &'a Node, layout_direction: Option<&'a str>) -> &'a str {
    metadata_value(node, "direction")
        .and_then(|value| value.as_str())
        .or(layout_direction)
        .unwrap_or("")
}

fn edge_points(edge: &Edge) -> &[Point] {
    edge.points.as_deref().unwrap_or(&[])
}

fn deduped_edge_points(edge: &Edge) -> Path {
    dedupe_consecutive_points(edge_points(edge), EPS_LOCAL)
}

fn replacement_points_for(
    edges: &[Edge],
    edge_index: usize,
    replacements: &EdgeReplacementMap,
) -> Path {
    let points = replacements
        .get(&edge_index)
        .map(Vec::as_slice)
        .unwrap_or_else(|| edge_points(&edges[edge_index]));
    dedupe_consecutive_points(points, EPS_LOCAL)
}

fn endpoint_ids(edge: &Edge) -> Vec<&str> {
    [edge.start.as_deref(), edge.end.as_deref()]
        .into_iter()
        .flatten()
        .collect()
}

fn visible_edge_indices(edges: &[Edge]) -> Vec<usize> {
    edges
        .iter()
        .enumerate()
        .filter(|(_, edge)| !edge.is_layout_only.unwrap_or(false))
        .map(|(index, _)| index)
        .collect()
}

fn point_key(candidate: &[Point]) -> String {
    let mut key = String::new();
    for (index, point) in candidate.iter().enumerate() {
        if index > 0 {
            key.push('|');
        }
        let _ = write!(key, "{:.3},{:.3}", point.x, point.y);
    }
    key
}

// ---- separateSharedRenderedTerminalLanes ----

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LaneOrientation {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug)]
struct TerminalLane {
    edge_index: usize,
    node_id: String,
    at_start: bool,
    orientation: LaneOrientation,
    coord: f64,
    min: f64,
    max: f64,
    boundary: Point,
    rail_end: Point,
    rect: RectBounds,
}

fn usable_face(low: f64, high: f64) -> (f64, f64) {
    let inset = 8.0_f64.min((high - low) / 4.0);
    (low + inset, high - inset)
}

fn rect_intersect(node: &Node, point: Point) -> Point {
    let x = node.x.unwrap_or(0.0);
    let y = node.y.unwrap_or(0.0);
    let dx = point.x - x;
    let dy = point.y - y;
    let mut w = node.width.unwrap_or(0.0) / 2.0;
    let mut h = node.height.unwrap_or(0.0) / 2.0;

    if dy.abs() * w > dx.abs() * h {
        if dy < 0.0 {
            h = -h;
        }
        return Point::new(x + if dy == 0.0 { 0.0 } else { (h * dx) / dy }, y + h);
    }

    if dx < 0.0 {
        w = -w;
    }
    Point::new(x + w, y + if dx == 0.0 { 0.0 } else { (w * dy) / dx })
}

fn terminal_lane_for_points(
    edge: &Edge,
    edge_index: usize,
    at_start: bool,
    points: &[Point],
    node_by_id: &HashMap<&str, &Node>,
) -> Option<TerminalLane> {
    let points = dedupe_consecutive_points(points, EPS_LOCAL);
    if points.len() < 2 {
        return None;
    }

    let node_id = if at_start {
        edge.start.as_deref()
    } else {
        edge.end.as_deref()
    };
    let node = node_id.and_then(|id| node_by_id.get(id).copied());
    let rect = node.and_then(rect_of_node_bounds);
    let (node, node_id, rect) = match (node, node_id, rect) {
        (Some(node), Some(node_id), Some(rect)) => (node, node_id, rect),
        _ => return None,
    };

    let endpoint = if at_start {
        points[0]
    } else {
        points[points.len() - 1]
    };
    let adjacent = if at_start {
        points[1]
    } else {
        points[points.len() - 2]
    };
    let boundary = rect_intersect(node, endpoint);
    let mut rail_end = endpoint;
    if orthogonally_aligned(adjacent, boundary) {
        rail_end = adjacent;
    }

    if same_x(boundary, rail_end, EPS_LOCAL) {
        return Some(TerminalLane {
            edge_index,
            node_id: node_id.to_string(),
            at_start,
            orientation: LaneOrientation::Vertical,
            coord: boundary.x,
            min: boundary.y.min(rail_end.y),
            max: boundary.y.max(rail_end.y),
            boundary,
            rail_end,
            rect,
        });
    }
    if same_y(boundary, rail_end, EPS_LOCAL) {
        return Some(TerminalLane {
            edge_index,
            node_id: node_id.to_string(),
            at_start,
            orientation: LaneOrientation::Horizontal,
            coord: boundary.y,
            min: boundary.x.min(rail_end.x),
            max: boundary.x.max(rail_end.x),
            boundary,
            rail_end,
            rect,
        });
    }
    None
}

fn terminal_lane_for_edge(
    edges: &[Edge],
    edge_index: usize,
    at_start: bool,
    node_by_id: &HashMap<&str, &Node>,
) -> Option<TerminalLane> {
    terminal_lane_for_points(
        &edges[edge_index],
        edge_index,
        at_start,
        edge_points(&edges[edge_index]),
        node_by_id,
    )
}

fn projected_overlap_length(a: &TerminalLane, b: &TerminalLane) -> f64 {
    f64::max(0.0, a.max.min(b.max) - a.min.max(b.min))
}

fn same_terminal_face(a: &TerminalLane, b: &TerminalLane) -> bool {
    if a.node_id != b.node_id || a.orientation != b.orientation {
        return false;
    }

    if a.orientation == LaneOrientation::Horizontal {
        let on_horizontal_face =
            (a.boundary.x - a.rect.left).abs() < 1.0 || (a.boundary.x - a.rect.right).abs() < 1.0;
        return on_horizontal_face && same_x(a.boundary, b.boundary, 1.0);
    }

    let on_vertical_face =
        (a.boundary.y - a.rect.top).abs() < 1.0 || (a.boundary.y - a.rect.bottom).abs() < 1.0;
    on_vertical_face && same_y(a.boundary, b.boundary, 1.0)
}

fn exact_terminal_lane_conflict(a: &TerminalLane, b: &TerminalLane) -> bool {
    if a.node_id != b.node_id || a.orientation != b.orientation {
        return false;
    }
    let shared = projected_overlap_length(a, b);
    shared >= MIN_SHARED && (a.coord - b.coord).abs() < 0.5
}

fn near_terminal_lane_conflict(a: &TerminalLane, b: &TerminalLane) -> bool {
    if a.node_id != b.node_id
        || a.orientation != b.orientation
        || a.orientation != LaneOrientation::Horizontal
        || a.at_start == b.at_start
    {
        return false;
    }

    let shared = projected_overlap_length(a, b);
    if shared < MIN_SHARED {
        return false;
    }
    let face_span = a.rect.bottom - a.rect.top;
    if shared < face_span || shared > 2.0 * face_span {
        return false;
    }

    // Wybrow-style nudging keeps connector topology fixed while preserving ordering
    // constraints; rendered terminal tracks on the same object face need the same treatment
    // before endpoint duplication pins them in place.
    same_terminal_face(a, b) && (a.coord - b.coord).abs() < 16.0
}

fn shifted_candidate(lane: &TerminalLane, edges: &[Edge], shift: f64) -> Option<Path> {
    let points = deduped_edge_points(&edges[lane.edge_index]);
    if points.len() < 2 {
        return None;
    }

    let shifted_boundary = if lane.orientation == LaneOrientation::Vertical {
        Point::new(lane.boundary.x + shift, lane.boundary.y)
    } else {
        Point::new(lane.boundary.x, lane.boundary.y + shift)
    };
    let shifted_rail_end = if lane.orientation == LaneOrientation::Vertical {
        Point::new(lane.rail_end.x + shift, lane.rail_end.y)
    } else {
        Point::new(lane.rail_end.x, lane.rail_end.y + shift)
    };

    let boundary_stays_on_same_face = || {
        if (lane.boundary.y - lane.rect.top).abs() < 1.0
            || (lane.boundary.y - lane.rect.bottom).abs() < 1.0
        {
            let (low, high) = usable_face(lane.rect.left, lane.rect.right);
            return same_y(shifted_boundary, lane.boundary, EPS_LOCAL)
                && shifted_boundary.x >= low
                && shifted_boundary.x <= high;
        }

        if (lane.boundary.x - lane.rect.left).abs() < 1.0
            || (lane.boundary.x - lane.rect.right).abs() < 1.0
        {
            let (low, high) = usable_face(lane.rect.top, lane.rect.bottom);
            return same_x(shifted_boundary, lane.boundary, EPS_LOCAL)
                && shifted_boundary.y >= low
                && shifted_boundary.y <= high;
        }

        false
    };

    if !boundary_stays_on_same_face() {
        return None;
    }

    if lane.at_start {
        let rail_end_is_adjacent =
            points.len() > 1 && same_point(points[1], lane.rail_end, EPS_LOCAL);
        let rest = &points[if rail_end_is_adjacent { 2 } else { 1 }..];
        if let Some(next) = rest.first()
            && !orthogonally_aligned(*next, shifted_rail_end)
        {
            return None;
        }
        let mut result = Vec::with_capacity(rest.len() + 2);
        result.push(shifted_boundary);
        result.push(shifted_rail_end);
        result.extend_from_slice(rest);
        return Some(result);
    }

    let rail_end_is_adjacent =
        points.len() > 1 && same_point(points[points.len() - 2], lane.rail_end, EPS_LOCAL);
    let before = &points[..if rail_end_is_adjacent {
        points.len() - 2
    } else {
        points.len() - 1
    }];
    if let Some(previous) = before.last()
        && !orthogonally_aligned(*previous, shifted_rail_end)
    {
        return None;
    }
    let mut result = Vec::with_capacity(before.len() + 2);
    result.extend_from_slice(before);
    result.push(shifted_rail_end);
    result.push(shifted_boundary);
    Some(result)
}

fn lane_is_straight_collinear_connector(
    lane: &TerminalLane,
    edges: &[Edge],
    node_by_id: &HashMap<&str, &Node>,
) -> bool {
    let edge = &edges[lane.edge_index];
    let points = deduped_edge_points(edge);
    if points.len() != 2 {
        return false;
    }
    let start = edge
        .start
        .as_deref()
        .and_then(|id| node_by_id.get(id).copied());
    let end = edge
        .end
        .as_deref()
        .and_then(|id| node_by_id.get(id).copied());
    let (Some(start), Some(end)) = (start, end) else {
        return false;
    };

    let start_x = start.x.unwrap_or(0.0);
    let start_y = start.y.unwrap_or(0.0);
    let end_x = end.x.unwrap_or(0.0);
    let end_y = end.y.unwrap_or(0.0);
    let [a, b] = [points[0], points[1]];

    (same_y(a, b, EPS_LOCAL) && (start_y - end_y).abs() < 1.0 && (start_x - end_x).abs() > 1.0)
        || (same_x(a, b, EPS_LOCAL)
            && (start_x - end_x).abs() < 1.0
            && (start_y - end_y).abs() > 1.0)
}

fn usable_span_of(lane: &TerminalLane) -> (f64, f64) {
    if lane.orientation == LaneOrientation::Horizontal {
        usable_face(lane.rect.top, lane.rect.bottom)
    } else {
        usable_face(lane.rect.left, lane.rect.right)
    }
}

fn shift_to_face_limit(lane: &TerminalLane, other: &TerminalLane) -> f64 {
    let (low, high) = usable_span_of(lane);
    let away = if lane.coord < other.coord {
        low - lane.coord
    } else {
        high - lane.coord
    };
    if js_sign(away) == js_sign(lane.coord - other.coord) {
        away
    } else {
        0.0
    }
}

fn shifts_to_face_ends(lane: &TerminalLane) -> [f64; 2] {
    let (low, high) = usable_span_of(lane);
    [low - lane.coord, high - lane.coord]
}

fn closest_track_gap(lanes: &[TerminalLane], lane: &TerminalLane, at: &TerminalLane) -> f64 {
    let mut gap = f64::INFINITY;
    for other in lanes {
        if other.edge_index == lane.edge_index
            || other.node_id != at.node_id
            || other.orientation != at.orientation
            || projected_overlap_length(at, other) < MIN_SHARED
        {
            continue;
        }
        gap = gap.min((at.coord - other.coord).abs());
    }
    gap
}

fn conflicts_with(
    lanes: &[TerminalLane],
    lane: &TerminalLane,
    next_lane: &TerminalLane,
    near: bool,
) -> bool {
    lanes.iter().any(|other| {
        other.edge_index != lane.edge_index
            && (exact_terminal_lane_conflict(next_lane, other)
                || (near && near_terminal_lane_conflict(next_lane, other)))
    })
}

fn accept_terminal_lane(
    edges: &mut [Edge],
    lanes: &[TerminalLane],
    lane: &TerminalLane,
    candidate: Option<Path>,
    near: bool,
    keep_closest_gap: bool,
    node_by_id: &HashMap<&str, &Node>,
) -> bool {
    let next_lane = match &candidate {
        Some(candidate) => terminal_lane_for_points(
            &edges[lane.edge_index],
            lane.edge_index,
            lane.at_start,
            candidate,
            node_by_id,
        ),
        None => None,
    };
    let (Some(candidate), Some(next_lane)) = (candidate, next_lane) else {
        return false;
    };
    if conflicts_with(lanes, lane, &next_lane, near) {
        return false;
    }
    if keep_closest_gap
        && closest_track_gap(lanes, lane, &next_lane) < closest_track_gap(lanes, lane, lane)
    {
        return false;
    }
    edges[lane.edge_index].points = Some(candidate);
    true
}

/// `separateSharedRenderedTerminalLanes`: moves one of two first/last stubs that land on the
/// same node face just far enough apart for the renderer to draw both.
pub fn separate_shared_rendered_terminal_lanes(edges: &mut [Edge], nodes: &[Node]) {
    const TRACK_SHIFT: f64 = 7.0;

    let node_by_id: HashMap<&str, &Node> =
        nodes.iter().map(|node| (node.id.as_str(), node)).collect();

    let shifts = [
        -TRACK_SHIFT,
        TRACK_SHIFT,
        -2.0 * TRACK_SHIFT,
        2.0 * TRACK_SHIFT,
        -3.0 * TRACK_SHIFT,
        3.0 * TRACK_SHIFT,
    ];

    for _ in 0..8 {
        let mut lanes: Vec<TerminalLane> = Vec::new();
        for edge_index in visible_edge_indices(edges) {
            for at_start in [true, false] {
                if let Some(lane) = terminal_lane_for_edge(edges, edge_index, at_start, &node_by_id)
                {
                    lanes.push(lane);
                }
            }
        }

        let mut fixed = false;
        for i in 0..lanes.len() {
            if fixed {
                break;
            }
            for j in (i + 1)..lanes.len() {
                if fixed {
                    break;
                }
                let first = lanes[i].clone();
                let second = lanes[j].clone();
                if first.edge_index == second.edge_index
                    || !(exact_terminal_lane_conflict(&first, &second)
                        || near_terminal_lane_conflict(&first, &second))
                {
                    continue;
                }

                let fixing_near_conflict = !exact_terminal_lane_conflict(&first, &second);
                let mut candidates = [first.clone(), second.clone()];
                candidates.sort_by(|a, b| {
                    let a_preserves_straight =
                        lane_is_straight_collinear_connector(a, edges, &node_by_id);
                    let b_preserves_straight =
                        lane_is_straight_collinear_connector(b, edges, &node_by_id);
                    if a_preserves_straight != b_preserves_straight {
                        return a_preserves_straight.cmp(&b_preserves_straight);
                    }
                    (!b.at_start).cmp(&(!a.at_start))
                });

                // Two tracks that are merely close leave a straight connector between aligned
                // nodes where it is; only two tracks drawn on top of each other may move it.
                let movable: Vec<TerminalLane> = if fixing_near_conflict {
                    candidates
                        .into_iter()
                        .filter(|lane| {
                            !lane_is_straight_collinear_connector(lane, edges, &node_by_id)
                        })
                        .collect()
                } else {
                    candidates.to_vec()
                };

                for lane in &movable {
                    for shift in shifts {
                        let candidate = shifted_candidate(lane, edges, shift);
                        let accepted = accept_terminal_lane(
                            edges,
                            &lanes,
                            lane,
                            candidate,
                            fixing_near_conflict,
                            false,
                            &node_by_id,
                        );
                        if accepted {
                            fixed = true;
                            break;
                        }
                    }
                    if fixed {
                        break;
                    }
                }

                // When no whole step fits on the face, a near conflict moves the lane as far
                // from the other as the face allows, so the gap widens and the lane stays on
                // the face; two tracks on top of each other take either end of the face.
                if !fixed {
                    for lane in &movable {
                        let other = if lane.edge_index == first.edge_index
                            && lane.at_start == first.at_start
                        {
                            &second
                        } else {
                            &first
                        };
                        let face_shifts = if fixing_near_conflict {
                            vec![shift_to_face_limit(lane, other)]
                        } else {
                            shifts_to_face_ends(lane).to_vec()
                        };
                        for shift in face_shifts {
                            if shift.abs() <= EPS_LOCAL {
                                continue;
                            }
                            let candidate = shifted_candidate(lane, edges, shift);
                            let accepted = accept_terminal_lane(
                                edges,
                                &lanes,
                                lane,
                                candidate,
                                false,
                                true,
                                &node_by_id,
                            );
                            if accepted {
                                fixed = true;
                                break;
                            }
                        }
                        if fixed {
                            break;
                        }
                    }
                }
            }
        }

        if !fixed {
            return;
        }
    }
}

// ---- collapseRedundantRectangularDoglegs ----

fn without_dogleg(points: &[Point], index: usize) -> Option<Path> {
    if index + 4 >= points.len() {
        return None;
    }
    let p0 = points[index];
    let p1 = points[index + 1];
    let p2 = points[index + 2];
    let p3 = points[index + 3];
    let p4 = points[index + 4];

    let terminal_vertical_dogleg = is_horizontal_segment(p0, p1, EPS_LOCAL)
        && is_vertical_segment(p1, p2, EPS_LOCAL)
        && is_horizontal_segment(p2, p3, EPS_LOCAL)
        && is_vertical_segment(p3, p4, EPS_LOCAL)
        && same_x(p0, p3, EPS_LOCAL)
        && same_x(p0, p4, EPS_LOCAL)
        && same_x(p1, p2, EPS_LOCAL)
        && (p1.x - p0.x) * (p3.x - p2.x) < 0.0;

    let terminal_horizontal_dogleg = is_vertical_segment(p0, p1, EPS_LOCAL)
        && is_horizontal_segment(p1, p2, EPS_LOCAL)
        && is_vertical_segment(p2, p3, EPS_LOCAL)
        && is_horizontal_segment(p3, p4, EPS_LOCAL)
        && same_y(p0, p3, EPS_LOCAL)
        && same_y(p0, p4, EPS_LOCAL)
        && same_y(p1, p2, EPS_LOCAL)
        && (p1.y - p0.y) * (p3.y - p2.y) < 0.0;

    if terminal_vertical_dogleg || terminal_horizontal_dogleg {
        let mut result = Vec::with_capacity(points.len());
        result.extend_from_slice(&points[..=index]);
        result.push(p4);
        result.extend_from_slice(&points[index + 5..]);
        return Some(dedupe_consecutive_points(&result, EPS_LOCAL));
    }

    if index + 5 >= points.len() {
        return None;
    }
    let p5 = points[index + 5];

    let vertical_dogleg = is_vertical_segment(p0, p1, EPS_LOCAL)
        && is_horizontal_segment(p1, p2, EPS_LOCAL)
        && is_vertical_segment(p2, p3, EPS_LOCAL)
        && is_horizontal_segment(p3, p4, EPS_LOCAL)
        && is_vertical_segment(p4, p5, EPS_LOCAL)
        && same_x(p0, p4, EPS_LOCAL)
        && same_x(p0, p5, EPS_LOCAL)
        && same_x(p2, p3, EPS_LOCAL)
        && (p2.x - p1.x) * (p4.x - p3.x) < 0.0;

    let horizontal_dogleg = is_horizontal_segment(p0, p1, EPS_LOCAL)
        && is_vertical_segment(p1, p2, EPS_LOCAL)
        && is_horizontal_segment(p2, p3, EPS_LOCAL)
        && is_vertical_segment(p3, p4, EPS_LOCAL)
        && is_horizontal_segment(p4, p5, EPS_LOCAL)
        && same_y(p0, p4, EPS_LOCAL)
        && same_y(p0, p5, EPS_LOCAL)
        && same_y(p2, p3, EPS_LOCAL)
        && (p2.y - p1.y) * (p4.y - p3.y) < 0.0;

    if !vertical_dogleg && !horizontal_dogleg {
        return None;
    }

    let mut result = Vec::with_capacity(points.len());
    result.extend_from_slice(&points[..=index]);
    result.push(p5);
    result.extend_from_slice(&points[index + 6..]);
    Some(dedupe_consecutive_points(&result, EPS_LOCAL))
}

fn candidate_is_safe_for_dogleg(
    edges: &[Edge],
    edge_index: usize,
    candidate: &[Point],
    obstacles: &super::geometry::NodeRectEntries,
) -> bool {
    let edge = &edges[edge_index];
    let endpoint_ids = endpoint_ids(edge);
    let candidate_segments = segments_for(candidate);
    if candidate_segments.len() != candidate.len() - 1 {
        return false;
    }

    for segment in &candidate_segments {
        if segment_hits_any_rect(
            segment.a,
            segment.b,
            &obstacles.real_node_rects,
            &endpoint_ids,
            -2.0,
        ) {
            return false;
        }
        if segment_hits_any_rect(segment.a, segment.b, &obstacles.label_node_rects, &[], -2.0) {
            return false;
        }
    }

    for (other_index, other) in edges.iter().enumerate() {
        if other_index == edge_index || other.is_layout_only.unwrap_or(false) {
            continue;
        }
        let Some(other_points) = other.points.as_deref() else {
            continue;
        };
        if other_points.len() < 2 {
            continue;
        }
        for candidate_segment in &candidate_segments {
            for other_segment in segments_for(&dedupe_consecutive_points(other_points, EPS_LOCAL)) {
                if same_axis_segment_overlap_length(candidate_segment, &other_segment, 0.5)
                    >= MIN_SHARED
                {
                    return false;
                }
                if orthogonal_segments_strictly_cross(
                    candidate_segment.a,
                    candidate_segment.b,
                    other_segment.a,
                    other_segment.b,
                    EPS_LOCAL,
                ) {
                    return false;
                }
            }
        }
    }

    true
}

/// `collapseRedundantRectangularDoglegs`: removes a terminal or interior dog-leg when the
/// shorter route stays clear of every node, label, and other edge.
pub fn collapse_redundant_rectangular_doglegs(edges: &mut [Edge], nodes: &[Node]) {
    let obstacles = collect_node_rect_entries(nodes);

    for _ in 0..8 {
        let mut fixed = false;
        for edge_index in 0..edges.len() {
            if edges[edge_index].is_layout_only.unwrap_or(false) {
                continue;
            }
            let points = deduped_edge_points(&edges[edge_index]);
            for index in 0..points.len().saturating_sub(4) {
                let candidate = without_dogleg(&points, index);
                let Some(candidate) = candidate else {
                    continue;
                };
                if !candidate_is_safe_for_dogleg(edges, edge_index, &candidate, &obstacles) {
                    continue;
                }
                edges[edge_index].points = Some(candidate);
                fixed = true;
                break;
            }
            if fixed {
                break;
            }
        }
        if !fixed {
            return;
        }
    }
}

// ---- liftObstacleHuggingSameSideRails ----

fn points_for_edge(
    edges: &[Edge],
    edge_index: usize,
    replacement: Option<(usize, &[Point])>,
) -> Path {
    let points = match replacement {
        Some((replacement_index, points)) if replacement_index == edge_index => points,
        _ => edge_points(&edges[edge_index]),
    };
    dedupe_consecutive_points(points, EPS_LOCAL)
}

fn strict_crossing_count(
    edges: &[Edge],
    visible: &[usize],
    replacement: Option<(usize, &[Point])>,
) -> usize {
    let mut count = 0;
    for (i, first) in visible.iter().enumerate() {
        let first_segments = segments_for(&points_for_edge(edges, *first, replacement));
        for second in &visible[i + 1..] {
            let second_segments = segments_for(&points_for_edge(edges, *second, replacement));
            for first_segment in &first_segments {
                for second_segment in &second_segments {
                    if orthogonal_segments_strictly_cross(
                        first_segment.a,
                        first_segment.b,
                        second_segment.a,
                        second_segment.b,
                        EPS_LOCAL,
                    ) {
                        count += 1;
                    }
                }
            }
        }
    }
    count
}

fn middle_rail(points: &[Point]) -> Option<OrthogonalSegment> {
    let segments = segments_for(points);
    if segments.len() != 3 {
        return None;
    }
    let middle = segments[1];
    if segments[0].horizontal == middle.horizontal || segments[2].horizontal == middle.horizontal {
        return None;
    }
    Some(middle)
}

fn candidate_by_moving_rail(
    points: &[Point],
    rail: &OrthogonalSegment,
    coord: f64,
) -> Option<Path> {
    let mut candidate = points.to_vec();
    if rail.horizontal {
        candidate[rail.index].y = coord;
        candidate[rail.index + 1].y = coord;
    } else if rail.vertical {
        candidate[rail.index].x = coord;
        candidate[rail.index + 1].x = coord;
    } else {
        return None;
    }
    let simplified = simplify_polyline(&dedupe_consecutive_points(&candidate, EPS_LOCAL));
    if segments_for(&simplified).len() == simplified.len() - 1 {
        Some(simplified)
    } else {
        None
    }
}

fn candidate_is_safe_for_rail(
    edges: &[Edge],
    visible: &[usize],
    edge_index: usize,
    candidate: &[Point],
    current_crossings: usize,
    obstacles: &super::geometry::NodeRectEntries,
) -> bool {
    let endpoint_ids = endpoint_ids(&edges[edge_index]);
    let candidate_segments = segments_for(candidate);
    if candidate_segments.len() != candidate.len() - 1 {
        return false;
    }

    for segment in &candidate_segments {
        if segment_hits_any_rect(
            segment.a,
            segment.b,
            &obstacles.real_node_rects,
            &endpoint_ids,
            -2.0,
        ) {
            return false;
        }
        if segment_hits_any_rect(segment.a, segment.b, &obstacles.label_node_rects, &[], -2.0) {
            return false;
        }
    }

    for other_index in visible {
        if *other_index == edge_index {
            continue;
        }
        for candidate_segment in &candidate_segments {
            for other_segment in segments_for(&points_for_edge(edges, *other_index, None)) {
                if same_axis_segment_overlap_length(candidate_segment, &other_segment, 0.5)
                    >= MIN_SHARED
                {
                    return false;
                }
            }
        }
    }

    strict_crossing_count(edges, visible, Some((edge_index, candidate))) <= current_crossings
}

/// `liftObstacleHuggingSameSideRails`: moves the middle rail of a three-segment route outside
/// a taller intervening node it currently hugs.
pub fn lift_obstacle_hugging_same_side_rails(edges: &mut [Edge], nodes: &[Node]) {
    const BUFFER: f64 = 2.0;
    const CLEARANCE: f64 = 20.0;

    let obstacles = collect_node_rect_entries(nodes);
    let visible = visible_edge_indices(edges);

    for _ in 0..8 {
        let current_crossings = strict_crossing_count(edges, &visible, None);
        let mut fixed = false;

        for edge_index in &visible {
            let points = points_for_edge(edges, *edge_index, None);
            let Some(rail) = middle_rail(&points) else {
                continue;
            };
            let edge = &edges[*edge_index];
            let endpoint_ids = endpoint_ids(edge);
            let blockers: Vec<&super::geometry::RectEntry> = obstacles
                .real_node_rects
                .iter()
                .filter(|entry| {
                    if endpoint_ids.contains(&entry.id.as_str()) {
                        return false;
                    }
                    let rect = entry.rect;
                    if rail.horizontal {
                        let x_overlap = overlap_length(rail.a.x, rail.b.x, rect.left, rect.right);
                        return x_overlap >= MIN_SHARED
                            && rail.a.y >= rect.top - BUFFER
                            && rail.a.y <= rect.bottom + BUFFER;
                    }
                    let y_overlap = overlap_length(rail.a.y, rail.b.y, rect.top, rect.bottom);
                    y_overlap >= MIN_SHARED
                        && rail.a.x >= rect.left - BUFFER
                        && rail.a.x <= rect.right + BUFFER
                })
                .collect();
            if blockers.is_empty() {
                continue;
            }

            let coords = if rail.horizontal {
                [
                    blockers
                        .iter()
                        .map(|entry| entry.rect.top)
                        .fold(f64::INFINITY, f64::min)
                        - CLEARANCE,
                    blockers
                        .iter()
                        .map(|entry| entry.rect.bottom)
                        .fold(f64::NEG_INFINITY, f64::max)
                        + CLEARANCE,
                ]
            } else {
                [
                    blockers
                        .iter()
                        .map(|entry| entry.rect.left)
                        .fold(f64::INFINITY, f64::min)
                        - CLEARANCE,
                    blockers
                        .iter()
                        .map(|entry| entry.rect.right)
                        .fold(f64::NEG_INFINITY, f64::max)
                        + CLEARANCE,
                ]
            };

            for coord in coords {
                let Some(candidate) = candidate_by_moving_rail(&points, &rail, coord) else {
                    continue;
                };
                if !candidate_is_safe_for_rail(
                    edges,
                    &visible,
                    *edge_index,
                    &candidate,
                    current_crossings,
                    &obstacles,
                ) {
                    continue;
                }
                edges[*edge_index].points = Some(candidate);
                fixed = true;
                break;
            }
            if fixed {
                break;
            }
        }

        if !fixed {
            return;
        }
    }
}

// ---- lane title bands ----

fn valid_title_rect(node: &Node) -> Option<RectBounds> {
    let rect = node.group_title_rect?;
    if !rect.left.is_finite()
        || !rect.right.is_finite()
        || !rect.top.is_finite()
        || !rect.bottom.is_finite()
        || rect.right <= rect.left
        || rect.bottom <= rect.top
    {
        return None;
    }
    Some(RectBounds {
        left: rect.left,
        right: rect.right,
        top: rect.top,
        bottom: rect.bottom,
    })
}

fn horizontal_segment_intersects_title(segment: &OrthogonalSegment, rect: RectBounds) -> bool {
    if !segment.horizontal {
        return false;
    }
    let y = segment.a.y;
    // A rail along the band's edge counts, as it does for validateLayout.
    if y < rect.top - EPS_LOCAL || y > rect.bottom + EPS_LOCAL {
        return false;
    }
    overlap_length(segment.a.x, segment.b.x, rect.left, rect.right) >= MIN_SHARED
}

fn vertical_segment_intersects_title(segment: &OrthogonalSegment, rect: RectBounds) -> bool {
    if !segment.vertical {
        return false;
    }
    let x = segment.a.x;
    // A rail along the band's edge counts, as it does for validateLayout.
    if x < rect.left - EPS_LOCAL || x > rect.right + EPS_LOCAL {
        return false;
    }
    overlap_length(segment.a.y, segment.b.y, rect.top, rect.bottom) >= MIN_SHARED
}

/// `liftTopLaneTitleBandsAboveRails`: raises a top-lane title band above any rail crossing it.
/// A lane with no `direction` of its own takes the layout's `direction`.
pub fn lift_top_lane_title_bands_above_rails(
    edges: &[Edge],
    nodes: &mut [Node],
    layout_direction: Option<&str>,
) {
    const CLEARANCE: f64 = 4.0;

    let lane_model = build_lane_model(nodes);
    let mut lanes: Vec<(usize, RectBounds)> = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        if !lane_model.is_lane(&node.id) {
            continue;
        }
        let direction = upper_case_direction(Some(node_direction(node, layout_direction)));
        if direction == "LR" || direction == "RL" || direction == "BT" {
            continue;
        }
        let rect = match valid_title_rect(node) {
            Some(rect) => rect,
            None => continue,
        };
        let (Some(y), Some(height)) = (node.y, node.height) else {
            continue;
        };
        if !y.is_finite() || !height.is_finite() || height <= 0.0 {
            continue;
        }
        let title_width = rect.right - rect.left;
        let title_height = rect.bottom - rect.top;
        if title_height <= 0.0 || title_width < title_height {
            continue;
        }
        lanes.push((index, rect));
    }
    if lanes.is_empty() {
        return;
    }

    let mut top_delta = 0.0_f64;
    for edge in edges {
        if edge.is_layout_only.unwrap_or(false) {
            continue;
        }
        let points = deduped_edge_points(edge);
        for segment in segments_for(&points) {
            for (_, rect) in &lanes {
                if !horizontal_segment_intersects_title(&segment, *rect) {
                    continue;
                }
                top_delta = top_delta.max(rect.bottom - segment.a.y + CLEARANCE);
            }
        }
    }

    if top_delta <= EPS_LOCAL {
        return;
    }

    for (index, rect) in lanes {
        let node = &mut nodes[index];
        let (Some(y), Some(height)) = (node.y, node.height) else {
            continue;
        };
        if !y.is_finite() || !height.is_finite() || height <= 0.0 {
            continue;
        }
        node.y = Some(y - top_delta / 2.0);
        node.height = Some(height + top_delta);
        node.group_title_rect = Some(crate::render::pipeline::types::GroupTitleRect {
            left: rect.left,
            right: rect.right,
            top: rect.top - top_delta,
            bottom: rect.bottom - top_delta,
        });
    }
}

/// `shiftLeftLaneTitleBandsLeftOfRails`: moves a left-lane title band of an LR layout left of
/// any rail crossing it. A lane with no `direction` of its own takes the layout's `direction`.
pub fn shift_left_lane_title_bands_left_of_rails(
    edges: &[Edge],
    nodes: &mut [Node],
    layout_direction: Option<&str>,
) {
    const CLEARANCE: f64 = 4.0;

    let lane_model = build_lane_model(nodes);
    let mut lanes: Vec<(usize, RectBounds)> = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        if !lane_model.is_lane(&node.id) {
            continue;
        }
        let direction = upper_case_direction(Some(node_direction(node, layout_direction)));
        if direction != "LR" {
            continue;
        }
        let rect = match valid_title_rect(node) {
            Some(rect) => rect,
            None => continue,
        };
        let (Some(x), Some(width)) = (node.x, node.width) else {
            continue;
        };
        if !x.is_finite() || !width.is_finite() || width <= 0.0 {
            continue;
        }
        let title_width = rect.right - rect.left;
        let title_height = rect.bottom - rect.top;
        if title_width <= 0.0 || title_height < title_width {
            continue;
        }
        lanes.push((index, rect));
    }
    if lanes.is_empty() {
        return;
    }

    let mut left_delta = 0.0_f64;
    for edge in edges {
        if edge.is_layout_only.unwrap_or(false) {
            continue;
        }
        let points = deduped_edge_points(edge);
        for segment in segments_for(&points) {
            for (_, rect) in &lanes {
                if vertical_segment_intersects_title(&segment, *rect) {
                    left_delta = left_delta.max(rect.right - segment.a.x + CLEARANCE);
                } else if horizontal_segment_intersects_title(&segment, *rect) {
                    let segment_left = segment.a.x.min(segment.b.x);
                    left_delta = left_delta.max(rect.right - segment_left + CLEARANCE);
                }
            }
        }
    }

    if left_delta <= EPS_LOCAL {
        return;
    }

    for (index, rect) in lanes {
        let node = &mut nodes[index];
        let (Some(x), Some(width)) = (node.x, node.width) else {
            continue;
        };
        if !x.is_finite() || !width.is_finite() || width <= 0.0 {
            continue;
        }
        node.x = Some(x - left_delta / 2.0);
        node.width = Some(width + left_delta);
        node.group_title_rect = Some(crate::render::pipeline::types::GroupTitleRect {
            left: rect.left - left_delta,
            right: rect.right - left_delta,
            top: rect.top,
            bottom: rect.bottom,
        });
    }
}

// ---- swapDestinationTerminalTailsToReduceCrossings ----

#[derive(Clone, Copy, Debug)]
struct TerminalTail {
    tail_start: Point,
    terminal: Point,
}

fn crossing_count(edges: &[Edge], visible: &[usize], replacements: &EdgeReplacementMap) -> usize {
    let mut count = 0;
    for (i, first) in visible.iter().enumerate() {
        let first_segments = segments_for(&replacement_points_for(edges, *first, replacements));
        for second in &visible[i + 1..] {
            let second_segments =
                segments_for(&replacement_points_for(edges, *second, replacements));
            for first_segment in &first_segments {
                for second_segment in &second_segments {
                    if orthogonal_segments_strictly_cross(
                        first_segment.a,
                        first_segment.b,
                        second_segment.a,
                        second_segment.b,
                        EPS_LOCAL,
                    ) {
                        count += 1;
                    }
                }
            }
        }
    }
    count
}

fn total_bends(edges: &[Edge], visible: &[usize], replacements: &EdgeReplacementMap) -> usize {
    visible
        .iter()
        .map(|index| {
            count_orthogonal_bends(
                &replacement_points_for(edges, *index, replacements),
                EPS_LOCAL,
            )
        })
        .sum()
}

fn terminal_tail_for(edges: &[Edge], edge_index: usize) -> Option<TerminalTail> {
    let points = replacement_points_for(edges, edge_index, &EdgeReplacementMap::new());
    if points.len() < 4 {
        return None;
    }
    let tail_start = points[points.len() - 2];
    let terminal = points[points.len() - 1];
    if !is_horizontal_segment(tail_start, terminal, EPS_LOCAL)
        && !is_vertical_segment(tail_start, terminal, EPS_LOCAL)
    {
        return None;
    }
    Some(TerminalTail {
        tail_start,
        terminal,
    })
}

fn candidate_with_destination_tail(
    edges: &[Edge],
    edge_index: usize,
    tail: TerminalTail,
) -> Option<Path> {
    let points = replacement_points_for(edges, edge_index, &EdgeReplacementMap::new());
    if points.len() < 3 {
        return None;
    }
    let start = points[0];
    let first_turn = points[1];

    let connector = if is_horizontal_segment(start, first_turn, EPS_LOCAL) {
        Point::new(first_turn.x, tail.tail_start.y)
    } else if is_vertical_segment(start, first_turn, EPS_LOCAL) {
        Point::new(tail.tail_start.x, first_turn.y)
    } else {
        return None;
    };

    let candidate = simplify_polyline(&dedupe_consecutive_points(
        &[start, first_turn, connector, tail.tail_start, tail.terminal],
        EPS_LOCAL,
    ));
    if segments_for(&candidate).len() == candidate.len() - 1 {
        Some(candidate)
    } else {
        None
    }
}

fn path_has_node_hit(
    edges: &[Edge],
    edge_index: usize,
    path: &[Point],
    obstacles: &super::geometry::NodeRectEntries,
) -> bool {
    let endpoint_ids = endpoint_ids(&edges[edge_index]);
    for segment in segments_for(path) {
        if segment_hits_any_rect(
            segment.a,
            segment.b,
            &obstacles.real_node_rects,
            &endpoint_ids,
            -2.0,
        ) {
            return true;
        }
    }
    false
}

fn path_has_shared_track(
    edges: &[Edge],
    visible: &[usize],
    edge_index: usize,
    path: &[Point],
    replacements: &EdgeReplacementMap,
) -> bool {
    for other in visible {
        if *other == edge_index {
            continue;
        }
        for candidate_segment in segments_for(path) {
            for other_segment in segments_for(&replacement_points_for(edges, *other, replacements))
            {
                if same_axis_segment_overlap_length(&candidate_segment, &other_segment, 0.5)
                    >= MIN_SHARED
                {
                    return true;
                }
            }
        }
    }
    false
}

fn swap_candidate_is_safe(
    edges: &[Edge],
    visible: &[usize],
    edge_index: usize,
    path: &[Point],
    replacements: &EdgeReplacementMap,
    obstacles: &super::geometry::NodeRectEntries,
) -> bool {
    !path_has_node_hit(edges, edge_index, path, obstacles)
        && !path_has_shared_track(edges, visible, edge_index, path, replacements)
}

fn edges_by_destination(
    edges: &[Edge],
    visible: &[usize],
    nodes: &[Node],
) -> Vec<(String, Vec<usize>)> {
    let node_ids: BTreeSet<&str> = nodes.iter().map(|node| node.id.as_str()).collect();
    let mut result: Vec<(String, Vec<usize>)> = Vec::new();
    for index in visible {
        let Some(dst_id) = edges[*index].end.as_deref() else {
            continue;
        };
        if !node_ids.contains(dst_id) {
            continue;
        }
        let points = replacement_points_for(edges, *index, &EdgeReplacementMap::new());
        if points.len() < 4 {
            continue;
        }
        match result.iter_mut().find(|(id, _)| id == dst_id) {
            Some((_, bucket)) => bucket.push(*index),
            None => result.push((dst_id.to_string(), vec![*index])),
        }
    }
    result
}

/// `swapDestinationTerminalTailsToReduceCrossings`: exchanges the terminal tails of two edges
/// that share a destination when doing so removes at least one crossing.
pub fn swap_destination_terminal_tails_to_reduce_crossings(edges: &mut [Edge], nodes: &[Node]) {
    let obstacles = collect_node_rect_entries(nodes);
    let visible = visible_edge_indices(edges);

    for _ in 0..4 {
        let current_crossings = crossing_count(edges, &visible, &EdgeReplacementMap::new());
        if current_crossings == 0 {
            return;
        }
        let current_bends = total_bends(edges, &visible, &EdgeReplacementMap::new());

        let mut best_replacements: Option<EdgeReplacementMap> = None;
        let mut best_crossings = current_crossings;
        let mut best_bends = current_bends;

        for (_, destination_edges) in edges_by_destination(edges, &visible, nodes) {
            for i in 0..destination_edges.len() {
                for j in (i + 1)..destination_edges.len() {
                    let first = destination_edges[i];
                    let second = destination_edges[j];
                    let (Some(first_tail), Some(second_tail)) = (
                        terminal_tail_for(edges, first),
                        terminal_tail_for(edges, second),
                    ) else {
                        continue;
                    };

                    let (Some(first_candidate), Some(second_candidate)) = (
                        candidate_with_destination_tail(edges, first, second_tail),
                        candidate_with_destination_tail(edges, second, first_tail),
                    ) else {
                        continue;
                    };

                    let mut replacements = EdgeReplacementMap::new();
                    replacements.insert(first, first_candidate.clone());
                    replacements.insert(second, second_candidate.clone());
                    if !swap_candidate_is_safe(
                        edges,
                        &visible,
                        first,
                        &first_candidate,
                        &replacements,
                        &obstacles,
                    ) || !swap_candidate_is_safe(
                        edges,
                        &visible,
                        second,
                        &second_candidate,
                        &replacements,
                        &obstacles,
                    ) {
                        continue;
                    }

                    let candidate_crossings = crossing_count(edges, &visible, &replacements);
                    let candidate_bends = total_bends(edges, &visible, &replacements);
                    if candidate_crossings >= current_crossings {
                        continue;
                    }
                    if candidate_crossings > best_crossings
                        || (candidate_crossings == best_crossings && candidate_bends >= best_bends)
                    {
                        continue;
                    }
                    best_replacements = Some(replacements);
                    best_crossings = candidate_crossings;
                    best_bends = candidate_bends;
                }
            }
        }

        let Some(best_replacements) = best_replacements else {
            return;
        };
        for (edge_index, points) in best_replacements {
            edges[edge_index].points = Some(points);
        }
    }
}

// ---- reassignCrossingExternalRailChannels ----

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RailAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug)]
struct ExternalRail {
    edge_index: usize,
    points: Path,
    segment_index: usize,
    axis: RailAxis,
    side: RectSide,
    coord: f64,
    min: f64,
    max: f64,
}

fn endpoint_rects_for(
    edges: &[Edge],
    edge_index: usize,
    nodes: &[Node],
) -> Option<(RectBounds, RectBounds)> {
    let edge = &edges[edge_index];
    let src_node = edge
        .start
        .as_deref()
        .and_then(|id| nodes.iter().find(|node| node.id == id));
    let dst_node = edge
        .end
        .as_deref()
        .and_then(|id| nodes.iter().find(|node| node.id == id));
    let src = src_node.and_then(rect_of_node_bounds);
    let dst = dst_node.and_then(rect_of_node_bounds);
    match (src, dst) {
        (Some(src), Some(dst)) => Some((src, dst)),
        _ => None,
    }
}

fn external_rail_for_segment(
    edges: &[Edge],
    nodes: &[Node],
    edge_index: usize,
    points: &[Point],
    segment: &OrthogonalSegment,
) -> Option<ExternalRail> {
    if segment.index == 0 || segment.index + 1 >= points.len() - 1 {
        return None;
    }

    let (src, dst) = endpoint_rects_for(edges, edge_index, nodes)?;

    if segment.vertical {
        let coord = segment.a.x;
        let left_bound = src.left.min(dst.left);
        let right_bound = src.right.max(dst.right);
        let side = if coord < left_bound - EPS_LOCAL {
            RectSide::Left
        } else if coord > right_bound + EPS_LOCAL {
            RectSide::Right
        } else {
            return None;
        };
        return Some(ExternalRail {
            edge_index,
            points: points.to_vec(),
            segment_index: segment.index,
            axis: RailAxis::Vertical,
            side,
            coord,
            min: segment.a.y.min(segment.b.y),
            max: segment.a.y.max(segment.b.y),
        });
    }

    if segment.horizontal {
        let coord = segment.a.y;
        let top_bound = src.top.min(dst.top);
        let bottom_bound = src.bottom.max(dst.bottom);
        let side = if coord < top_bound - EPS_LOCAL {
            RectSide::Top
        } else if coord > bottom_bound + EPS_LOCAL {
            RectSide::Bottom
        } else {
            return None;
        };
        return Some(ExternalRail {
            edge_index,
            points: points.to_vec(),
            segment_index: segment.index,
            axis: RailAxis::Horizontal,
            side,
            coord,
            min: segment.a.x.min(segment.b.x),
            max: segment.a.x.max(segment.b.x),
        });
    }

    None
}

fn collect_external_rails(edges: &[Edge], nodes: &[Node], visible: &[usize]) -> Vec<ExternalRail> {
    let mut rails = Vec::new();
    for edge_index in visible {
        let points = replacement_points_for(edges, *edge_index, &EdgeReplacementMap::new());
        for segment in segments_for(&points) {
            if let Some(rail) =
                external_rail_for_segment(edges, nodes, *edge_index, &points, &segment)
            {
                rails.push(rail);
            }
        }
    }
    rails
}

fn rails_interact(a: &ExternalRail, b: &ExternalRail) -> bool {
    a.edge_index != b.edge_index
        && a.axis == b.axis
        && a.side == b.side
        && overlap_length(a.min, a.max, b.min, b.max) >= MIN_SHARED
}

fn connected_components(rails: &[ExternalRail]) -> Vec<Vec<usize>> {
    let mut result = Vec::new();
    let mut seen = vec![false; rails.len()];
    for start in 0..rails.len() {
        if seen[start] {
            continue;
        }
        let mut queue = vec![start];
        let mut component = Vec::new();
        seen[start] = true;
        while let Some(current) = queue.pop() {
            component.push(current);
            for next in 0..rails.len() {
                if !seen[next] && rails_interact(&rails[current], &rails[next]) {
                    seen[next] = true;
                    queue.push(next);
                }
            }
        }
        if component.len() > 1 {
            result.push(component);
        }
    }
    result
}

fn unique_coords_for(rails: &[ExternalRail], component: &[usize]) -> Vec<f64> {
    const RAIL_CHANNEL_GAP: f64 = 12.0;
    let mut coords: Vec<f64> = Vec::new();
    for index in component {
        let coord = rails[*index].coord;
        if !coords
            .iter()
            .any(|existing| (existing - coord).abs() < EPS_LOCAL)
        {
            coords.push(coord);
        }
    }

    while coords.len() < component.len() {
        let min = coords.iter().copied().fold(f64::INFINITY, f64::min);
        let max = coords.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let side = rails[component[0]].side;
        coords.push(if side == RectSide::Left || side == RectSide::Top {
            min - RAIL_CHANNEL_GAP * (component.len() - coords.len()) as f64
        } else {
            max + RAIL_CHANNEL_GAP * (component.len() - coords.len()) as f64
        });
    }
    coords
}

fn coordinate_assignments_for(rails: &[ExternalRail], component: &[usize]) -> Vec<Vec<f64>> {
    const MAX_EXHAUSTIVE_COMPONENT: usize = 6;
    let current: Vec<f64> = component.iter().map(|index| rails[*index].coord).collect();
    let coords = unique_coords_for(rails, component);
    let mut assignments = Vec::new();

    if component.len() <= MAX_EXHAUSTIVE_COMPONENT {
        let mut used = vec![false; coords.len()];
        let mut next: Vec<f64> = Vec::new();
        fn visit(
            coords: &[f64],
            used: &mut [bool],
            next: &mut Vec<f64>,
            length: usize,
            current: &[f64],
            assignments: &mut Vec<Vec<f64>>,
        ) {
            if next.len() == length {
                if next
                    .iter()
                    .enumerate()
                    .any(|(index, coord)| (coord - current[index]).abs() >= EPS_LOCAL)
                {
                    assignments.push(next.clone());
                }
                return;
            }
            for (i, coord) in coords.iter().enumerate() {
                if used[i] {
                    continue;
                }
                used[i] = true;
                next.push(*coord);
                visit(coords, used, next, length, current, assignments);
                next.pop();
                used[i] = false;
            }
        }
        visit(
            &coords,
            &mut used,
            &mut next,
            component.len(),
            &current,
            &mut assignments,
        );
        return assignments;
    }

    for i in 0..current.len() {
        for j in (i + 1)..current.len() {
            let mut assignment = current.clone();
            assignment.swap(i, j);
            assignments.push(assignment);
        }
    }
    assignments
}

fn replacements_for_assignment(
    rails: &[ExternalRail],
    component: &[usize],
    assignment: &[f64],
) -> Option<EdgeReplacementMap> {
    let mut draft: Vec<(usize, Path)> = Vec::new();
    for (i, rail_index) in component.iter().enumerate() {
        let rail = &rails[*rail_index];
        let coord = assignment[i];
        let position = match draft.iter().position(|(edge, _)| *edge == rail.edge_index) {
            Some(position) => position,
            None => {
                draft.push((rail.edge_index, rail.points.clone()));
                draft.len() - 1
            }
        };
        let points = &mut draft[position].1;
        if rail.axis == RailAxis::Vertical {
            points[rail.segment_index].x = coord;
            points[rail.segment_index + 1].x = coord;
        } else {
            points[rail.segment_index].y = coord;
            points[rail.segment_index + 1].y = coord;
        }
    }

    let mut replacements = EdgeReplacementMap::new();
    for (edge_index, points) in draft {
        let simplified = simplify_polyline(&dedupe_consecutive_points(&points, EPS_LOCAL));
        if segments_for(&simplified).len() != simplified.len() - 1 {
            return None;
        }
        replacements.insert(edge_index, simplified);
    }
    Some(replacements)
}

fn reassign_candidate_is_safe(
    edges: &[Edge],
    visible: &[usize],
    replacements: &EdgeReplacementMap,
    obstacles: &super::geometry::NodeRectEntries,
) -> bool {
    for (edge_index, points) in replacements {
        let endpoint_ids = endpoint_ids(&edges[*edge_index]);
        for segment in segments_for(points) {
            if segment_hits_any_rect(
                segment.a,
                segment.b,
                &obstacles.real_node_rects,
                &endpoint_ids,
                -2.0,
            ) {
                return false;
            }
            if segment_hits_any_rect(segment.a, segment.b, &obstacles.label_node_rects, &[], -2.0) {
                return false;
            }
        }
    }

    for (i, first) in visible.iter().enumerate() {
        let first_changed = replacements.contains_key(first);
        let first_segments = segments_for(&replacement_points_for(edges, *first, replacements));
        for second in &visible[i + 1..] {
            if !first_changed && !replacements.contains_key(second) {
                continue;
            }
            let second_segments =
                segments_for(&replacement_points_for(edges, *second, replacements));
            for first_segment in &first_segments {
                for second_segment in &second_segments {
                    if same_axis_segment_overlap_length(first_segment, second_segment, 0.5)
                        >= MIN_SHARED
                    {
                        return false;
                    }
                }
            }
        }
    }

    true
}

/// `reassignCrossingExternalRailChannels`: moves a bundle of overlapping external rails
/// together, keeping crossings as the first acceptance criterion.
pub fn reassign_crossing_external_rail_channels(edges: &mut [Edge], nodes: &[Node]) {
    let obstacles = collect_node_rect_entries(nodes);
    let visible = visible_edge_indices(edges);

    for _ in 0..4 {
        let current_crossings = crossing_count(edges, &visible, &EdgeReplacementMap::new());
        if current_crossings == 0 {
            return;
        }

        let rails = collect_external_rails(edges, nodes, &visible);
        let mut best_replacements: Option<EdgeReplacementMap> = None;
        let mut best_crossings = current_crossings;
        let mut best_bends = total_bends(edges, &visible, &EdgeReplacementMap::new());
        let mut best_displacement = f64::INFINITY;

        for component in connected_components(&rails) {
            for assignment in coordinate_assignments_for(&rails, &component) {
                let Some(replacements) =
                    replacements_for_assignment(&rails, &component, &assignment)
                else {
                    continue;
                };
                if !reassign_candidate_is_safe(edges, &visible, &replacements, &obstacles) {
                    continue;
                }

                let candidate_crossings = crossing_count(edges, &visible, &replacements);
                if candidate_crossings >= current_crossings {
                    continue;
                }
                let candidate_bends = total_bends(edges, &visible, &replacements);
                let candidate_displacement: f64 = component
                    .iter()
                    .enumerate()
                    .map(|(index, rail_index)| (assignment[index] - rails[*rail_index].coord).abs())
                    .sum();

                if candidate_crossings > best_crossings
                    || (candidate_crossings == best_crossings
                        && (candidate_bends > best_bends
                            || (candidate_bends == best_bends
                                && candidate_displacement >= best_displacement)))
                {
                    continue;
                }

                best_replacements = Some(replacements);
                best_crossings = candidate_crossings;
                best_bends = candidate_bends;
                best_displacement = candidate_displacement;
            }
        }

        let Some(best_replacements) = best_replacements else {
            return;
        };
        for (edge_index, points) in best_replacements {
            edges[edge_index].points = Some(points);
        }
    }
}

// ---- shortcutRedundantOrthogonalJogs ----

fn path_length(points: &[Point]) -> f64 {
    segments_for(points)
        .iter()
        .map(|segment| (segment.a.x - segment.b.x).hypot(segment.a.y - segment.b.y))
        .sum()
}

fn segment_runs_along_rect_border(segment: &OrthogonalSegment, rect: RectBounds) -> bool {
    if segment.horizontal {
        let y = segment.a.y;
        let on_border = (y - rect.top).abs() < 1.0 || (y - rect.bottom).abs() < 1.0;
        return on_border
            && overlap_length(segment.a.x, segment.b.x, rect.left, rect.right) >= MIN_SHARED;
    }

    if segment.vertical {
        let x = segment.a.x;
        let on_border = (x - rect.left).abs() < 1.0 || (x - rect.right).abs() < 1.0;
        return on_border
            && overlap_length(segment.a.y, segment.b.y, rect.top, rect.bottom) >= MIN_SHARED;
    }

    false
}

fn shortcut_candidates_at(points: &[Point], index: usize) -> Candidates {
    if index + 3 >= points.len() {
        return Vec::new();
    }

    let p0 = points[index];
    let p1 = points[index + 1];
    let p2 = points[index + 2];
    let p3 = points[index + 3];
    let is_hvh = is_horizontal_segment(p0, p1, EPS_LOCAL)
        && is_vertical_segment(p1, p2, EPS_LOCAL)
        && is_horizontal_segment(p2, p3, EPS_LOCAL);
    let is_vhv = is_vertical_segment(p0, p1, EPS_LOCAL)
        && is_horizontal_segment(p1, p2, EPS_LOCAL)
        && is_vertical_segment(p2, p3, EPS_LOCAL);
    if !is_hvh && !is_vhv {
        return Vec::new();
    }
    let outer_segments_oppose = if is_hvh {
        js_sign(p1.x - p0.x) != js_sign(p3.x - p2.x)
    } else {
        js_sign(p1.y - p0.y) != js_sign(p3.y - p2.y)
    };
    if !outer_segments_oppose {
        return Vec::new();
    }

    let corners = if same_x(p0, p3, EPS_LOCAL) || same_y(p0, p3, EPS_LOCAL) {
        Vec::new()
    } else {
        vec![Point::new(p0.x, p3.y), Point::new(p3.x, p0.y)]
    };
    let raw_candidates: Candidates = if corners.is_empty() {
        let mut candidate = Vec::with_capacity(points.len() - 2);
        candidate.extend_from_slice(&points[..=index]);
        candidate.extend_from_slice(&points[index + 3..]);
        vec![candidate]
    } else {
        corners
            .into_iter()
            .map(|corner| {
                let mut candidate = Vec::with_capacity(points.len() - 1);
                candidate.extend_from_slice(&points[..=index]);
                candidate.push(corner);
                candidate.extend_from_slice(&points[index + 3..]);
                candidate
            })
            .collect()
    };

    let mut seen = BTreeSet::new();
    raw_candidates
        .into_iter()
        .map(|candidate| simplify_polyline(&dedupe_consecutive_points(&candidate, EPS_LOCAL)))
        .filter(|candidate| {
            if segments_for(candidate).len() != candidate.len() - 1 {
                return false;
            }
            if !candidate
                .iter()
                .any(|point| same_point(*point, p3, EPS_LOCAL))
            {
                return false;
            }
            if !seen.insert(point_key(candidate)) {
                return false;
            }
            true
        })
        .collect()
}

fn shortcut_candidate_is_safe(
    edges: &[Edge],
    visible: &[usize],
    edge_index: usize,
    candidate: &[Point],
    current_crossings: usize,
    obstacles: &super::geometry::NodeRectEntries,
    nodes: &[Node],
) -> bool {
    let endpoint_ids = endpoint_ids(&edges[edge_index]);
    let mut endpoint_rects = Vec::new();
    for id in &endpoint_ids {
        if let Some(node) = nodes.iter().find(|node| node.id == *id)
            && let Some(rect) = rect_of_node_bounds(node)
        {
            endpoint_rects.push(rect);
        }
    }

    for segment in segments_for(candidate) {
        if segment_hits_any_rect(
            segment.a,
            segment.b,
            &obstacles.real_node_rects,
            &endpoint_ids,
            -2.0,
        ) {
            return false;
        }
        if segment_hits_any_rect(segment.a, segment.b, &obstacles.label_node_rects, &[], -2.0) {
            return false;
        }
        if endpoint_rects
            .iter()
            .any(|rect| segment_runs_along_rect_border(&segment, *rect))
        {
            return false;
        }
    }

    for other_index in visible {
        if *other_index == edge_index {
            continue;
        }
        for candidate_segment in segments_for(candidate) {
            for other_segment in segments_for(&points_for_edge(edges, *other_index, None)) {
                if same_axis_segment_overlap_length(&candidate_segment, &other_segment, 0.5)
                    >= MIN_SHARED
                {
                    return false;
                }
            }
        }
    }

    strict_crossing_count(edges, visible, Some((edge_index, candidate))) <= current_crossings
}

/// `shortcutRedundantOrthogonalJogs`: replaces a pair of opposing corners with the straight
/// route when that has fewer bends or a shorter equal-bend route and stays clear.
pub fn shortcut_redundant_orthogonal_jogs(edges: &mut [Edge], nodes: &[Node]) {
    let obstacles = collect_node_rect_entries(nodes);
    let visible = visible_edge_indices(edges);

    for _ in 0..8 {
        let current_crossings = strict_crossing_count(edges, &visible, None);
        let mut best_edge: Option<usize> = None;
        let mut best_path: Option<Path> = None;
        let mut best_crossings = current_crossings;
        let mut best_bends = usize::MAX;
        let mut best_length = f64::INFINITY;

        for edge_index in &visible {
            let current_points = points_for_edge(edges, *edge_index, None);
            let current_bends = count_orthogonal_bends(&current_points, EPS_LOCAL);
            let current_length = path_length(&current_points);
            for index in 0..current_points.len().saturating_sub(3) {
                for candidate in shortcut_candidates_at(&current_points, index) {
                    let candidate_bends = count_orthogonal_bends(&candidate, EPS_LOCAL);
                    let candidate_length = path_length(&candidate);
                    let improves_shape = candidate_bends < current_bends
                        || (candidate_bends == current_bends
                            && candidate_length < current_length - EPS_LOCAL);
                    if !improves_shape
                        || !shortcut_candidate_is_safe(
                            edges,
                            &visible,
                            *edge_index,
                            &candidate,
                            current_crossings,
                            &obstacles,
                            nodes,
                        )
                    {
                        continue;
                    }

                    let candidate_crossings =
                        strict_crossing_count(edges, &visible, Some((*edge_index, &candidate)));
                    if candidate_crossings > best_crossings
                        || (candidate_crossings == best_crossings
                            && (candidate_bends > best_bends
                                || (candidate_bends == best_bends
                                    && candidate_length >= best_length)))
                    {
                        continue;
                    }

                    best_edge = Some(*edge_index);
                    best_path = Some(candidate);
                    best_crossings = candidate_crossings;
                    best_bends = candidate_bends;
                    best_length = candidate_length;
                }
            }
        }

        let (Some(best_edge), Some(best_path)) = (best_edge, best_path) else {
            return;
        };
        edges[best_edge].points = Some(best_path);
    }
}

// ---- resolveRenderedOrthogonalCrossings ----

#[derive(Clone, Debug)]
struct ResolveNodeInfo {
    id: String,
    cx: f64,
    cy: f64,
    rect: RectBounds,
}

fn port_for_resolve_node(node: &ResolveNodeInfo, side: RectSide) -> Point {
    match side {
        RectSide::Top => Point::new(node.cx, node.rect.top),
        RectSide::Bottom => Point::new(node.cx, node.rect.bottom),
        RectSide::Left => Point::new(node.rect.left, node.cy),
        RectSide::Right => Point::new(node.rect.right, node.cy),
    }
}

#[derive(Clone, Debug)]
struct CrossingPair {
    first: usize,
    second: usize,
    count: usize,
}

#[derive(Clone, Debug, Default)]
struct CrossingSnapshot {
    count: usize,
    pairs: Vec<CrossingPair>,
    edge_set: BTreeSet<usize>,
    edges: Vec<usize>,
}

#[derive(Clone, Debug)]
struct PairCandidate {
    path: Path,
    segments: Vec<OrthogonalSegment>,
    shared_track_conflicts: BTreeSet<usize>,
    total_bends: usize,
    length: f64,
}

#[derive(Clone, Debug)]
struct PairOption {
    edge: usize,
    candidates: Vec<PairCandidate>,
}

#[derive(Clone, Debug)]
struct PairReplacementScore {
    replacements: EdgeReplacementMap,
    crossings: usize,
    bends: usize,
    length: f64,
}

struct PairScoringContext<'a> {
    current: &'a CrossingSnapshot,
    current_bends: usize,
    current_length: f64,
    base_bends_by_edge: &'a HashMap<usize, usize>,
    base_length_by_edge: &'a HashMap<usize, f64>,
    base_segments: &'a HashMap<usize, Vec<OrthogonalSegment>>,
}

fn crossing_count_between_segments(
    first_segments: &[OrthogonalSegment],
    second_segments: &[OrthogonalSegment],
) -> usize {
    let mut count = 0;
    for first in first_segments {
        for second in second_segments {
            if orthogonal_segments_strictly_cross(first.a, first.b, second.a, second.b, EPS_LOCAL) {
                count += 1;
            }
        }
    }
    count
}

fn crossing_count_between_paths(first: &[Point], second: &[Point]) -> usize {
    crossing_count_between_segments(&segments_for(first), &segments_for(second))
}

fn resolve_crossing_snapshot(
    edges: &[Edge],
    visible: &[usize],
    replacements: &EdgeReplacementMap,
) -> CrossingSnapshot {
    let mut snapshot = CrossingSnapshot::default();
    let edge_position: HashMap<usize, usize> = visible
        .iter()
        .enumerate()
        .map(|(position, edge)| (*edge, position))
        .collect();
    let mut count = 0;
    let mut pairs = Vec::new();
    let mut edge_set = BTreeSet::new();
    let mut edge_order = Vec::new();
    let add_edge = |edge: usize, edge_set: &mut BTreeSet<usize>, edge_order: &mut Vec<usize>| {
        if edge_set.insert(edge) {
            edge_order.push(edge);
        }
    };

    for (i, first) in visible.iter().enumerate() {
        let first_points = replacement_points_for(edges, *first, replacements);
        for second in &visible[i + 1..] {
            let pair_count = crossing_count_between_paths(
                &first_points,
                &replacement_points_for(edges, *second, replacements),
            );
            if pair_count > 0 {
                count += pair_count;
                pairs.push(CrossingPair {
                    first: *first,
                    second: *second,
                    count: pair_count,
                });
                add_edge(*first, &mut edge_set, &mut edge_order);
                add_edge(*second, &mut edge_set, &mut edge_order);
            }
        }
    }

    edge_order.sort_by_key(|edge| edge_position.get(edge).copied().unwrap_or(0));
    snapshot.count = count;
    snapshot.pairs = pairs;
    snapshot.edge_set = edge_set;
    snapshot.edges = edge_order;
    snapshot
}

fn crossing_count_with_replacements(
    edges: &[Edge],
    visible: &[usize],
    current: &CrossingSnapshot,
    replacements: &EdgeReplacementMap,
) -> usize {
    if replacements.is_empty() {
        return current.count;
    }

    let mut current_affected = 0;
    for pair in &current.pairs {
        if replacements.contains_key(&pair.first) || replacements.contains_key(&pair.second) {
            current_affected += pair.count;
        }
    }

    let mut replacement_affected = 0;
    for (i, first) in visible.iter().enumerate() {
        let first_changed = replacements.contains_key(first);
        let first_points = replacement_points_for(edges, *first, replacements);
        for second in &visible[i + 1..] {
            if !first_changed && !replacements.contains_key(second) {
                continue;
            }
            replacement_affected += crossing_count_between_paths(
                &first_points,
                &replacement_points_for(edges, *second, replacements),
            );
        }
    }

    current.count - current_affected + replacement_affected
}

fn crossing_components(snapshot: &CrossingSnapshot) -> Vec<Vec<usize>> {
    let mut neighbors: Vec<(usize, Vec<usize>)> = Vec::new();
    for pair in &snapshot.pairs {
        for (edge, neighbor) in [(pair.first, pair.second), (pair.second, pair.first)] {
            match neighbors.iter_mut().find(|(node, _)| *node == edge) {
                Some((_, list)) => {
                    if !list.contains(&neighbor) {
                        list.push(neighbor);
                    }
                }
                None => neighbors.push((edge, vec![neighbor])),
            }
        }
    }

    let mut components = Vec::new();
    let mut seen = BTreeSet::new();
    for edge in &snapshot.edges {
        if seen.contains(edge) {
            continue;
        }
        let mut queue = vec![*edge];
        let mut component = Vec::new();
        seen.insert(*edge);
        while let Some(current) = queue.pop() {
            component.push(current);
            if let Some((_, list)) = neighbors.iter().find(|(node, _)| *node == current) {
                for next in list {
                    if !seen.contains(next) {
                        seen.insert(*next);
                        queue.push(*next);
                    }
                }
            }
        }
        component.sort();
        if component.len() > 1 {
            components.push(component);
        }
    }

    components
}

fn pair_search_groups(
    edges: &[Edge],
    visible: &[usize],
    snapshot: &CrossingSnapshot,
) -> Vec<Vec<usize>> {
    let mut groups = Vec::new();
    for component in crossing_components(snapshot) {
        let component_set: BTreeSet<usize> = component.iter().copied().collect();
        let mut component_endpoint_ids: Vec<&str> = Vec::new();
        for edge_index in &component {
            for id in endpoint_ids(&edges[*edge_index]) {
                if !component_endpoint_ids.contains(&id) {
                    component_endpoint_ids.push(id);
                }
            }
        }
        let mut group = component.clone();
        for edge_index in visible {
            if component_set.contains(edge_index) {
                continue;
            }
            if endpoint_ids(&edges[*edge_index])
                .iter()
                .any(|id| component_endpoint_ids.contains(id))
            {
                group.push(*edge_index);
            }
        }
        group.sort();
        groups.push(group);
    }
    groups
}

fn path_has_segment_conflict(
    edges: &[Edge],
    visible: &[usize],
    edge_index: usize,
    path: &[Point],
    replacements: &EdgeReplacementMap,
) -> bool {
    let path_segments = segments_for(path);
    for other_index in visible {
        if *other_index == edge_index {
            continue;
        }
        for candidate_segment in &path_segments {
            for other_segment in
                segments_for(&replacement_points_for(edges, *other_index, replacements))
            {
                if same_axis_segment_overlap_length(candidate_segment, &other_segment, 0.5)
                    >= MIN_SHARED
                {
                    return true;
                }
            }
        }
        if routes_touch_at_bend(
            path,
            &replacement_points_for(edges, *other_index, replacements),
            EDGE_ROUTING.route_touch_tolerance,
        ) {
            return true;
        }
    }
    false
}

fn resolve_path_hits_node(
    edges: &[Edge],
    edge_index: usize,
    path: &[Point],
    nodes: &[ResolveNodeInfo],
) -> bool {
    let endpoint_ids = endpoint_ids(&edges[edge_index]);
    let rects: Vec<super::geometry::RectEntry> = nodes
        .iter()
        .map(|node| super::geometry::RectEntry {
            id: node.id.clone(),
            rect: node.rect,
        })
        .collect();
    for segment in segments_for(path) {
        if segment_hits_any_rect(segment.a, segment.b, &rects, &endpoint_ids, -2.0) {
            return true;
        }
    }
    false
}

fn push_orthogonal_candidate(candidates: &mut Candidates, points: Vec<Point>) {
    let candidate = simplify_polyline(&dedupe_consecutive_points(&points, EPS_LOCAL));
    if segments_for(&candidate).len() == candidate.len() - 1 && !candidate.is_empty() {
        candidates.push(candidate);
    }
}

fn side_is_horizontal(side: RectSide) -> bool {
    matches!(side, RectSide::Left | RectSide::Right)
}

fn local_track_for_same_side(src: Point, side: RectSide, dst: Point) -> f64 {
    match side {
        RectSide::Left => src.x.min(dst.x) - 20.0,
        RectSide::Right => src.x.max(dst.x) + 20.0,
        RectSide::Top => src.y.min(dst.y) - 20.0,
        RectSide::Bottom => src.y.max(dst.y) + 20.0,
    }
}

fn outward_tracks_for_side(side: RectSide, outside: &OutsideTracksValue) -> Vec<f64> {
    let outward = if matches!(side, RectSide::Left | RectSide::Top) {
        -1.0
    } else {
        1.0
    };
    let seed = track_for_side_value(outside, side);
    (0..=2)
        .map(|channel| seed + outward * 20.0 * channel as f64)
        .collect()
}

#[derive(Clone, Copy, Debug, Default)]
struct OutsideTracksValue {
    top: f64,
    bottom: f64,
    left: f64,
    right: f64,
}

fn track_for_side_value(tracks: &OutsideTracksValue, side: RectSide) -> f64 {
    match side {
        RectSide::Top => tracks.top,
        RectSide::Bottom => tracks.bottom,
        RectSide::Left => tracks.left,
        RectSide::Right => tracks.right,
    }
}

fn add_same_side_candidates(
    candidates: &mut Candidates,
    src: Point,
    src_side: RectSide,
    dst: Point,
    outside: &OutsideTracksValue,
) {
    let outward = if matches!(src_side, RectSide::Left | RectSide::Top) {
        -1.0
    } else {
        1.0
    };
    let track_seeds = [
        local_track_for_same_side(src, src_side, dst),
        track_for_side_value(outside, src_side),
    ];
    for seed in track_seeds {
        for channel in 0..=2 {
            push_orthogonal_candidate(
                candidates,
                super::geometry::build_same_side_track_path(
                    src,
                    src_side,
                    dst,
                    seed + outward * 20.0 * channel as f64,
                ),
            );
        }
    }
}

fn add_horizontal_to_vertical_candidates(
    candidates: &mut Candidates,
    src: Point,
    src_side: RectSide,
    dst: Point,
    dst_side: RectSide,
    outside: &OutsideTracksValue,
) {
    for x_track in outward_tracks_for_side(src_side, outside) {
        for y_track in outward_tracks_for_side(dst_side, outside) {
            push_orthogonal_candidate(
                candidates,
                vec![
                    src,
                    Point::new(x_track, src.y),
                    Point::new(x_track, y_track),
                    Point::new(dst.x, y_track),
                    dst,
                ],
            );
        }
    }
}

fn add_vertical_to_horizontal_candidates(
    candidates: &mut Candidates,
    src: Point,
    src_side: RectSide,
    dst: Point,
    dst_side: RectSide,
    outside: &OutsideTracksValue,
) {
    for y_track in outward_tracks_for_side(src_side, outside) {
        for x_track in outward_tracks_for_side(dst_side, outside) {
            push_orthogonal_candidate(
                candidates,
                vec![
                    src,
                    Point::new(src.x, y_track),
                    Point::new(x_track, y_track),
                    Point::new(x_track, dst.y),
                    dst,
                ],
            );
        }
    }
}

fn add_horizontal_pair_candidates(
    candidates: &mut Candidates,
    src: Point,
    src_side: RectSide,
    dst: Point,
    dst_side: RectSide,
    outside: &OutsideTracksValue,
) {
    let mut y_tracks = outward_tracks_for_side(RectSide::Top, outside);
    y_tracks.extend(outward_tracks_for_side(RectSide::Bottom, outside));
    for src_track in outward_tracks_for_side(src_side, outside) {
        for dst_track in outward_tracks_for_side(dst_side, outside) {
            for y_track in &y_tracks {
                push_orthogonal_candidate(
                    candidates,
                    vec![
                        src,
                        Point::new(src_track, src.y),
                        Point::new(src_track, *y_track),
                        Point::new(dst_track, *y_track),
                        Point::new(dst_track, dst.y),
                        dst,
                    ],
                );
            }
        }
    }
}

fn add_vertical_pair_candidates(
    candidates: &mut Candidates,
    src: Point,
    src_side: RectSide,
    dst: Point,
    dst_side: RectSide,
    outside: &OutsideTracksValue,
) {
    let mut x_tracks = outward_tracks_for_side(RectSide::Left, outside);
    x_tracks.extend(outward_tracks_for_side(RectSide::Right, outside));
    for src_track in outward_tracks_for_side(src_side, outside) {
        for dst_track in outward_tracks_for_side(dst_side, outside) {
            for x_track in &x_tracks {
                push_orthogonal_candidate(
                    candidates,
                    vec![
                        src,
                        Point::new(src.x, src_track),
                        Point::new(*x_track, src_track),
                        Point::new(*x_track, dst_track),
                        Point::new(dst.x, dst_track),
                        dst,
                    ],
                );
            }
        }
    }
}

fn dedupe_candidate_paths(candidates: Candidates) -> Candidates {
    let mut seen = BTreeSet::new();
    candidates
        .into_iter()
        .map(|candidate| dedupe_consecutive_points(&candidate, EPS_LOCAL))
        .filter(|candidate| {
            let key = point_key(candidate);
            if seen.contains(&key) || candidate.len() < 2 {
                return false;
            }
            seen.insert(key);
            true
        })
        .collect()
}

fn build_candidates_for_sides(
    src: Point,
    src_side: RectSide,
    dst: Point,
    dst_side: RectSide,
    outside: &OutsideTracksValue,
) -> Candidates {
    let mut candidates = Candidates::new();
    if let Some(base) =
        super::geometry::build_orthogonal_port_path(src, src_side, dst, dst_side, 20.0, EPS_LOCAL)
    {
        push_orthogonal_candidate(&mut candidates, base);
    }
    if src_side == dst_side {
        add_same_side_candidates(&mut candidates, src, src_side, dst, outside);
    }

    let src_horizontal = side_is_horizontal(src_side);
    let dst_horizontal = side_is_horizontal(dst_side);
    if src_horizontal && !dst_horizontal {
        add_horizontal_to_vertical_candidates(
            &mut candidates,
            src,
            src_side,
            dst,
            dst_side,
            outside,
        );
    } else if !src_horizontal && dst_horizontal {
        add_vertical_to_horizontal_candidates(
            &mut candidates,
            src,
            src_side,
            dst,
            dst_side,
            outside,
        );
    } else if src_horizontal {
        add_horizontal_pair_candidates(&mut candidates, src, src_side, dst, dst_side, outside);
    } else {
        add_vertical_pair_candidates(&mut candidates, src, src_side, dst, dst_side, outside);
    }

    dedupe_candidate_paths(candidates)
}

fn add_vertical_departure_outer_track_candidates(
    candidates: &mut Candidates,
    first: Point,
    departure: Point,
    dst_node: &ResolveNodeInfo,
    outside: &OutsideTracksValue,
) {
    let mut external_x_tracks = outward_tracks_for_side(RectSide::Left, outside);
    external_x_tracks.extend(outward_tracks_for_side(RectSide::Right, outside));
    let mut external_y_tracks = outward_tracks_for_side(RectSide::Top, outside);
    external_y_tracks.extend(outward_tracks_for_side(RectSide::Bottom, outside));
    const SIDES: [RectSide; 4] = [
        RectSide::Top,
        RectSide::Bottom,
        RectSide::Left,
        RectSide::Right,
    ];
    for side in SIDES {
        let dst = port_for_resolve_node(dst_node, side);
        let target_y_tracks = if matches!(side, RectSide::Top | RectSide::Bottom) {
            outward_tracks_for_side(side, outside)
        } else {
            external_y_tracks.clone()
        };
        for track in &external_x_tracks {
            if side_is_horizontal(side) {
                push_orthogonal_candidate(
                    candidates,
                    vec![
                        first,
                        departure,
                        Point::new(*track, departure.y),
                        Point::new(*track, dst.y),
                        dst,
                    ],
                );
            }
            for target_track in &target_y_tracks {
                push_orthogonal_candidate(
                    candidates,
                    vec![
                        first,
                        departure,
                        Point::new(*track, departure.y),
                        Point::new(*track, *target_track),
                        Point::new(dst.x, *target_track),
                        dst,
                    ],
                );
            }
        }
    }
}

fn add_horizontal_departure_outer_track_candidates(
    candidates: &mut Candidates,
    first: Point,
    departure: Point,
    dst_node: &ResolveNodeInfo,
    outside: &OutsideTracksValue,
) {
    let mut external_x_tracks = outward_tracks_for_side(RectSide::Left, outside);
    external_x_tracks.extend(outward_tracks_for_side(RectSide::Right, outside));
    let mut external_y_tracks = outward_tracks_for_side(RectSide::Top, outside);
    external_y_tracks.extend(outward_tracks_for_side(RectSide::Bottom, outside));
    const SIDES: [RectSide; 4] = [
        RectSide::Top,
        RectSide::Bottom,
        RectSide::Left,
        RectSide::Right,
    ];
    for side in SIDES {
        let dst = port_for_resolve_node(dst_node, side);
        let target_x_tracks = if matches!(side, RectSide::Left | RectSide::Right) {
            outward_tracks_for_side(side, outside)
        } else {
            external_x_tracks.clone()
        };
        for track in &external_y_tracks {
            if !side_is_horizontal(side) {
                push_orthogonal_candidate(
                    candidates,
                    vec![
                        first,
                        departure,
                        Point::new(departure.x, *track),
                        Point::new(dst.x, *track),
                        dst,
                    ],
                );
            }
            for target_track in &target_x_tracks {
                push_orthogonal_candidate(
                    candidates,
                    vec![
                        first,
                        departure,
                        Point::new(departure.x, *track),
                        Point::new(*target_track, *track),
                        Point::new(*target_track, dst.y),
                        dst,
                    ],
                );
            }
        }
    }
}

fn terminal_preserving_outer_track_candidates(
    edges: &[Edge],
    edge_index: usize,
    node_info_by_id: &HashMap<&str, ResolveNodeInfo>,
    outside: &OutsideTracksValue,
) -> Candidates {
    let edge = &edges[edge_index];
    if edge.start.is_none() {
        return Vec::new();
    }
    let Some(dst_node) = edge.end.as_deref().and_then(|id| node_info_by_id.get(id)) else {
        return Vec::new();
    };

    let points = deduped_edge_points(edge);
    if points.len() < 4 {
        return Vec::new();
    }

    let first = points[0];
    let departure = points[1];
    let mut candidates = Candidates::new();
    if is_vertical_segment(first, departure, EPS_LOCAL) {
        add_vertical_departure_outer_track_candidates(
            &mut candidates,
            first,
            departure,
            dst_node,
            outside,
        );
    } else if is_horizontal_segment(first, departure, EPS_LOCAL) {
        add_horizontal_departure_outer_track_candidates(
            &mut candidates,
            first,
            departure,
            dst_node,
            outside,
        );
    }

    candidates
}

fn candidate_paths_for(
    edges: &[Edge],
    edge_index: usize,
    node_info_by_id: &HashMap<&str, ResolveNodeInfo>,
    outside: &OutsideTracksValue,
) -> Candidates {
    const SIDES: [RectSide; 4] = [
        RectSide::Top,
        RectSide::Bottom,
        RectSide::Left,
        RectSide::Right,
    ];
    let edge = &edges[edge_index];
    let src_node = edge.start.as_deref().and_then(|id| node_info_by_id.get(id));
    let dst_node = edge.end.as_deref().and_then(|id| node_info_by_id.get(id));
    let (Some(src_node), Some(dst_node)) = (src_node, dst_node) else {
        return Vec::new();
    };

    let mut candidates = Candidates::new();
    for src_side in SIDES {
        let src_port = port_for_resolve_node(src_node, src_side);
        for dst_side in SIDES {
            candidates.extend(build_candidates_for_sides(
                src_port,
                src_side,
                port_for_resolve_node(dst_node, dst_side),
                dst_side,
                outside,
            ));
        }
    }
    candidates.extend(terminal_preserving_outer_track_candidates(
        edges,
        edge_index,
        node_info_by_id,
        outside,
    ));
    candidates
}

fn current_segments_by_edge(
    edges: &[Edge],
    visible: &[usize],
) -> HashMap<usize, Vec<OrthogonalSegment>> {
    visible
        .iter()
        .map(|index| {
            (
                *index,
                segments_for(&replacement_points_for(
                    edges,
                    *index,
                    &EdgeReplacementMap::new(),
                )),
            )
        })
        .collect()
}

fn shared_track_conflicts_for(
    edges: &[Edge],
    visible: &[usize],
    edge_index: usize,
    candidate_path: &[Point],
    candidate_segments: &[OrthogonalSegment],
    base_segments: &HashMap<usize, Vec<OrthogonalSegment>>,
) -> BTreeSet<usize> {
    let mut conflicts = BTreeSet::new();
    for other_index in visible {
        if *other_index == edge_index {
            continue;
        }
        let other_segments = base_segments.get(other_index).cloned().unwrap_or_else(|| {
            segments_for(&replacement_points_for(
                edges,
                *other_index,
                &EdgeReplacementMap::new(),
            ))
        });
        if candidate_segments.iter().any(|candidate_segment| {
            other_segments.iter().any(|other_segment| {
                same_axis_segment_overlap_length(candidate_segment, other_segment, 0.5)
                    >= MIN_SHARED
            })
        }) || routes_touch_at_bend(
            candidate_path,
            &replacement_points_for(edges, *other_index, &EdgeReplacementMap::new()),
            EDGE_ROUTING.route_touch_tolerance,
        ) {
            conflicts.insert(*other_index);
        }
    }
    conflicts
}

#[allow(clippy::too_many_arguments)]
fn pair_candidates_for(
    edges: &[Edge],
    visible: &[usize],
    edge_index: usize,
    current: &CrossingSnapshot,
    base_segments: &HashMap<usize, Vec<OrthogonalSegment>>,
    crossing_count_by_edge: &HashMap<usize, usize>,
    node_info_by_id: &HashMap<&str, ResolveNodeInfo>,
    outside: &OutsideTracksValue,
    nodes: &[ResolveNodeInfo],
) -> Vec<PairCandidate> {
    const MAX_PAIR_CANDIDATES_PER_EDGE: usize = 48;
    let mut seen = BTreeSet::new();
    let mut scored: Vec<(Path, usize, usize, f64)> = Vec::new();
    for candidate in candidate_paths_for(edges, edge_index, node_info_by_id, outside) {
        let candidate = simplify_polyline(&dedupe_consecutive_points(&candidate, EPS_LOCAL));
        if resolve_path_hits_node(edges, edge_index, &candidate, nodes) {
            continue;
        }
        let key = point_key(&candidate);
        if seen.contains(&key) || candidate.len() < 2 {
            continue;
        }
        seen.insert(key);

        let candidate_segments = segments_for(&candidate);
        let mut replacement_affected = 0;
        for other_index in visible {
            if *other_index == edge_index {
                continue;
            }
            replacement_affected += crossing_count_between_segments(
                &candidate_segments,
                base_segments
                    .get(other_index)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]),
            );
        }
        let bends = count_orthogonal_bends(&candidate, EPS_LOCAL);
        let length = resolve_path_length(&candidate);
        let crossings = current.count
            - crossing_count_by_edge
                .get(&edge_index)
                .copied()
                .unwrap_or(0)
            + replacement_affected;
        scored.push((candidate, crossings, bends, length));
    }

    scored.retain(|(_, crossings, _, _)| *crossings <= current.count);
    scored.sort_by(|a, b| {
        a.1.cmp(&b.1)
            .then(a.2.cmp(&b.2))
            .then(a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal))
    });

    scored
        .into_iter()
        .take(MAX_PAIR_CANDIDATES_PER_EDGE)
        .map(|(path, _, _, length)| {
            let segments = segments_for(&path);
            let shared_track_conflicts = shared_track_conflicts_for(
                edges,
                visible,
                edge_index,
                &path,
                &segments,
                base_segments,
            );
            PairCandidate {
                total_bends: count_orthogonal_bends(&path, EPS_LOCAL),
                path,
                segments,
                shared_track_conflicts,
                length,
            }
        })
        .collect()
}

fn resolve_path_length(points: &[Point]) -> f64 {
    let mut sum = 0.0;
    for index in 1..points.len() {
        let point = points[index];
        let previous = points[index - 1];
        sum += (point.x - previous.x).abs() + (point.y - previous.y).abs();
    }
    sum
}

#[allow(clippy::too_many_arguments)]
fn pair_crossing_count(
    visible: &[usize],
    current: &CrossingSnapshot,
    first_edge: usize,
    first_candidate: &PairCandidate,
    second_edge: usize,
    second_candidate: &PairCandidate,
    base_segments: &HashMap<usize, Vec<OrthogonalSegment>>,
) -> usize {
    let mut current_affected = 0;
    for pair in &current.pairs {
        if pair.first == first_edge
            || pair.second == first_edge
            || pair.first == second_edge
            || pair.second == second_edge
        {
            current_affected += pair.count;
        }
    }

    let mut replacement_affected =
        crossing_count_between_segments(&first_candidate.segments, &second_candidate.segments);
    for other_index in visible {
        if *other_index == first_edge || *other_index == second_edge {
            continue;
        }
        let other_segments = base_segments
            .get(other_index)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        replacement_affected +=
            crossing_count_between_segments(&first_candidate.segments, other_segments)
                + crossing_count_between_segments(&second_candidate.segments, other_segments);
    }
    current.count - current_affected + replacement_affected
}

fn conflicts_only_with(candidate: &PairCandidate, edge: usize) -> bool {
    candidate
        .shared_track_conflicts
        .iter()
        .all(|conflict| *conflict == edge)
}

fn candidates_share_track(first: &PairCandidate, second: &PairCandidate) -> bool {
    first.segments.iter().any(|first_segment| {
        second.segments.iter().any(|second_segment| {
            same_axis_segment_overlap_length(first_segment, second_segment, 0.5) >= MIN_SHARED
        })
    }) || routes_touch_at_bend(
        &first.path,
        &second.path,
        EDGE_ROUTING.route_touch_tolerance,
    )
}

fn pair_candidates_are_compatible(
    first: &PairOption,
    first_candidate: &PairCandidate,
    second: &PairOption,
    second_candidate: &PairCandidate,
) -> bool {
    conflicts_only_with(first_candidate, second.edge)
        && conflicts_only_with(second_candidate, first.edge)
        && !candidates_share_track(first_candidate, second_candidate)
}

fn score_pair_replacement(
    context: &PairScoringContext<'_>,
    visible: &[usize],
    first: &PairOption,
    first_candidate: &PairCandidate,
    second: &PairOption,
    second_candidate: &PairCandidate,
) -> Option<PairReplacementScore> {
    let crossings = pair_crossing_count(
        visible,
        context.current,
        first.edge,
        first_candidate,
        second.edge,
        second_candidate,
        context.base_segments,
    );
    if crossings >= context.current.count {
        return None;
    }

    let mut replacements = EdgeReplacementMap::new();
    replacements.insert(first.edge, first_candidate.path.clone());
    replacements.insert(second.edge, second_candidate.path.clone());
    Some(PairReplacementScore {
        replacements,
        crossings,
        bends: context.current_bends
            - context
                .base_bends_by_edge
                .get(&first.edge)
                .copied()
                .unwrap_or(0)
            - context
                .base_bends_by_edge
                .get(&second.edge)
                .copied()
                .unwrap_or(0)
            + first_candidate.total_bends
            + second_candidate.total_bends,
        length: context.current_length
            - context
                .base_length_by_edge
                .get(&first.edge)
                .copied()
                .unwrap_or(0.0)
            - context
                .base_length_by_edge
                .get(&second.edge)
                .copied()
                .unwrap_or(0.0)
            + first_candidate.length
            + second_candidate.length,
    })
}

fn pair_score_is_better(candidate: &PairReplacementScore, best: &PairReplacementScore) -> bool {
    candidate.crossings < best.crossings
        || (candidate.crossings == best.crossings
            && (candidate.bends < best.bends
                || (candidate.bends == best.bends && candidate.length < best.length)))
}

fn best_score_for_option_pair(
    context: &PairScoringContext<'_>,
    visible: &[usize],
    first: &PairOption,
    second: &PairOption,
    best: PairReplacementScore,
) -> PairReplacementScore {
    let mut pair_best = best;
    for first_candidate in &first.candidates {
        for second_candidate in &second.candidates {
            if !pair_candidates_are_compatible(first, first_candidate, second, second_candidate) {
                continue;
            }
            if let Some(score) = score_pair_replacement(
                context,
                visible,
                first,
                first_candidate,
                second,
                second_candidate,
            ) && pair_score_is_better(&score, &pair_best)
            {
                pair_best = score;
            }
        }
    }
    pair_best
}

#[allow(clippy::too_many_arguments)]
fn best_paired_replacement(
    edges: &[Edge],
    visible: &[usize],
    current: &CrossingSnapshot,
    node_info_by_id: &HashMap<&str, ResolveNodeInfo>,
    outside: &OutsideTracksValue,
    nodes: &[ResolveNodeInfo],
    current_bends: usize,
    current_length: f64,
) -> Option<EdgeReplacementMap> {
    let base_segments = current_segments_by_edge(edges, visible);
    let crossing_count_by_edge = current_crossings_by_edge(current);
    let base_bends_by_edge: HashMap<usize, usize> = visible
        .iter()
        .map(|index| {
            (
                *index,
                count_orthogonal_bends(
                    &replacement_points_for(edges, *index, &EdgeReplacementMap::new()),
                    EPS_LOCAL,
                ),
            )
        })
        .collect();
    let base_length_by_edge: HashMap<usize, f64> = visible
        .iter()
        .map(|index| {
            (
                *index,
                resolve_path_length(&replacement_points_for(
                    edges,
                    *index,
                    &EdgeReplacementMap::new(),
                )),
            )
        })
        .collect();
    let mut options_by_edge: HashMap<usize, PairOption> = HashMap::new();
    let groups = pair_search_groups(edges, visible, current);
    for group in &groups {
        for edge_index in group {
            if options_by_edge.contains_key(edge_index) {
                continue;
            }
            let candidates = pair_candidates_for(
                edges,
                visible,
                *edge_index,
                current,
                &base_segments,
                &crossing_count_by_edge,
                node_info_by_id,
                outside,
                nodes,
            );
            if !candidates.is_empty() {
                options_by_edge.insert(
                    *edge_index,
                    PairOption {
                        edge: *edge_index,
                        candidates,
                    },
                );
            }
        }
    }

    let mut best = PairReplacementScore {
        replacements: EdgeReplacementMap::new(),
        crossings: current.count,
        bends: current_bends,
        length: current_length,
    };
    let context = PairScoringContext {
        current,
        current_bends,
        current_length,
        base_bends_by_edge: &base_bends_by_edge,
        base_length_by_edge: &base_length_by_edge,
        base_segments: &base_segments,
    };

    for group in &groups {
        let crossing_edge_set: BTreeSet<usize> = group
            .iter()
            .filter(|edge| current.edge_set.contains(edge))
            .copied()
            .collect();
        let options: Vec<&PairOption> = group
            .iter()
            .filter_map(|edge| options_by_edge.get(edge))
            .collect();
        for (i, first) in options.iter().enumerate() {
            for second in &options[i + 1..] {
                if !crossing_edge_set.contains(&first.edge)
                    && !crossing_edge_set.contains(&second.edge)
                {
                    continue;
                }
                best = best_score_for_option_pair(&context, visible, first, second, best);
            }
        }
    }

    if best.replacements.is_empty() {
        None
    } else {
        Some(best.replacements)
    }
}

fn current_crossings_by_edge(current: &CrossingSnapshot) -> HashMap<usize, usize> {
    let mut result = HashMap::new();
    for pair in &current.pairs {
        *result.entry(pair.first).or_insert(0) += pair.count;
        *result.entry(pair.second).or_insert(0) += pair.count;
    }
    result
}

/// `resolveRenderedOrthogonalCrossings`: rewrites a single edge onto a shorter crossing-free
/// port pair, or a pair of edges together, while keeping the visible crossing count down.
pub fn resolve_rendered_orthogonal_crossings(edges: &mut [Edge], nodes: &[Node]) {
    let mut real_nodes: Vec<ResolveNodeInfo> = Vec::new();
    for node in nodes {
        if node.is_group.unwrap_or(false) || node.is_edge_label.unwrap_or(false) {
            continue;
        }
        let cx = node.x.unwrap_or(0.0);
        let cy = node.y.unwrap_or(0.0);
        let Some(rect) = rect_of_node_bounds(node) else {
            continue;
        };
        real_nodes.push(ResolveNodeInfo {
            id: node.id.clone(),
            cx,
            cy,
            rect,
        });
    }
    if real_nodes.is_empty() {
        return;
    }

    let node_info_by_id: HashMap<&str, ResolveNodeInfo> = real_nodes
        .iter()
        .map(|node| (node.id.as_str(), node.clone()))
        .collect();
    let outside = OutsideTracksValue {
        top: real_nodes
            .iter()
            .map(|node| node.rect.top)
            .fold(f64::INFINITY, f64::min)
            - 20.0,
        bottom: real_nodes
            .iter()
            .map(|node| node.rect.bottom)
            .fold(f64::NEG_INFINITY, f64::max)
            + 20.0,
        left: real_nodes
            .iter()
            .map(|node| node.rect.left)
            .fold(f64::INFINITY, f64::min)
            - 20.0,
        right: real_nodes
            .iter()
            .map(|node| node.rect.right)
            .fold(f64::NEG_INFINITY, f64::max)
            + 20.0,
    };
    let visible = visible_edge_indices(edges);

    for _ in 0..4 {
        let current = resolve_crossing_snapshot(edges, &visible, &EdgeReplacementMap::new());
        if current.count == 0 {
            return;
        }

        let mut best_edge: Option<usize> = None;
        let mut best_path: Option<Path> = None;
        let mut best_crossings = current.count;
        let mut best_bends = usize::MAX;

        for edge_index in &current.edges {
            let current_edge_bends = count_orthogonal_bends(
                &replacement_points_for(edges, *edge_index, &EdgeReplacementMap::new()),
                EPS_LOCAL,
            );
            for candidate in candidate_paths_for(edges, *edge_index, &node_info_by_id, &outside) {
                let candidate_hits_node =
                    resolve_path_hits_node(edges, *edge_index, &candidate, &real_nodes);
                let candidate_has_segment_conflict = !candidate_hits_node
                    && path_has_segment_conflict(
                        edges,
                        &visible,
                        *edge_index,
                        &candidate,
                        &EdgeReplacementMap::new(),
                    );
                let mut replacement = EdgeReplacementMap::new();
                replacement.insert(*edge_index, candidate.clone());
                let candidate_crossings =
                    crossing_count_with_replacements(edges, &visible, &current, &replacement);
                let candidate_bends = count_orthogonal_bends(&candidate, EPS_LOCAL);
                if candidate_hits_node || candidate_has_segment_conflict {
                    continue;
                }
                let improves_current_edge = candidate_crossings < current.count
                    || (candidate_crossings == current.count
                        && candidate_bends < current_edge_bends);
                if !improves_current_edge {
                    continue;
                }
                if candidate_crossings > best_crossings
                    || (candidate_crossings == best_crossings && candidate_bends >= best_bends)
                {
                    continue;
                }
                best_edge = Some(*edge_index);
                best_path = Some(candidate);
                best_crossings = candidate_crossings;
                best_bends = candidate_bends;
            }
        }

        if let (Some(best_edge), Some(best_path)) = (best_edge, best_path) {
            edges[best_edge].points = Some(best_path);
            continue;
        }

        let current_bends = visible
            .iter()
            .map(|index| {
                count_orthogonal_bends(
                    &replacement_points_for(edges, *index, &EdgeReplacementMap::new()),
                    EPS_LOCAL,
                )
            })
            .sum();
        let current_length = visible
            .iter()
            .map(|index| {
                resolve_path_length(&replacement_points_for(
                    edges,
                    *index,
                    &EdgeReplacementMap::new(),
                ))
            })
            .sum();
        let paired_replacement = best_paired_replacement(
            edges,
            &visible,
            &current,
            &node_info_by_id,
            &outside,
            &real_nodes,
            current_bends,
            current_length,
        );
        let Some(paired_replacement) = paired_replacement else {
            return;
        };
        for (edge_index, points) in paired_replacement {
            edges[edge_index].points = Some(points);
        }
    }
}
