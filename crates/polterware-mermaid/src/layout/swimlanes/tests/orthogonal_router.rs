//! The parts of upstream
//! `orthogonalRouter/__tests__/*.spec.ts` that exercise the router ported here.
//!
//! The fixtures are the specs' own: the same node boxes, the same edge declarations, and the
//! same assertions, with `mkNode`'s defaults spelled out per test group.

use crate::layout::swimlanes::helpers::Direction;
use crate::layout::swimlanes::orthogonal_router::line_jumps::line_jump_paths;
use crate::layout::swimlanes::orthogonal_router::route_edges_orthogonal;
use crate::render::pipeline::types::{Edge, LayoutData, Node};
use crate::scene::{PathCommand, Point};

fn node(id: &str, x: f64, y: f64, width: f64, height: f64) -> Node {
    Node {
        id: id.to_string(),
        x: Some(x),
        y: Some(y),
        width: Some(width),
        height: Some(height),
        is_group: Some(false),
        ..Default::default()
    }
}

fn lane(id: &str, x: f64, y: f64, width: f64, height: f64) -> Node {
    Node {
        is_group: Some(true),
        ..node(id, x, y, width, height)
    }
}

fn with_parent(node: Node, parent: &str) -> Node {
    Node {
        parent_id: Some(parent.to_string()),
        ..node
    }
}

fn edge(id: &str, start: &str, end: &str) -> Edge {
    Edge::new(id, start, end)
}

fn route(nodes: Vec<Node>, edges: Vec<Edge>) -> LayoutData {
    let mut data = LayoutData {
        nodes,
        edges,
        ..Default::default()
    };
    route_edges_orthogonal(&mut data, Direction::Tb).expect("the router never fails");
    data
}

fn points(data: &LayoutData, id: &str) -> Vec<Point> {
    data.edges
        .iter()
        .find(|edge| edge.id == id)
        .and_then(|edge| edge.points.clone())
        .unwrap_or_else(|| panic!("edge {id} has points"))
}

fn is_on_node_boundary(point: Point, node: &Node, tolerance: f64) -> bool {
    let (x, y) = (node.x.unwrap_or(0.0), node.y.unwrap_or(0.0));
    let width = node.width.unwrap_or(0.0);
    let height = node.height.unwrap_or(0.0);
    let left = x - width / 2.0;
    let right = x + width / 2.0;
    let top = y - height / 2.0;
    let bottom = y + height / 2.0;

    let on_left_edge = (point.x - left).abs() < tolerance
        && point.y >= top - tolerance
        && point.y <= bottom + tolerance;
    let on_right_edge = (point.x - right).abs() < tolerance
        && point.y >= top - tolerance
        && point.y <= bottom + tolerance;
    let on_top_edge = (point.y - top).abs() < tolerance
        && point.x >= left - tolerance
        && point.x <= right + tolerance;
    let on_bottom_edge = (point.y - bottom).abs() < tolerance
        && point.x >= left - tolerance
        && point.x <= right + tolerance;
    on_left_edge || on_right_edge || on_top_edge || on_bottom_edge
}

const CROSS_EPS: f64 = 1e-6;

fn is_vertical(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() < CROSS_EPS
}

fn is_horizontal(a: Point, b: Point) -> bool {
    (a.y - b.y).abs() < CROSS_EPS
}

/// `segmentsCross` from `raykov.repro_crossing.spec.ts`: shared endpoints are not crossings.
fn segments_cross(a1: Point, a2: Point, b1: Point, b2: Point) -> bool {
    if is_vertical(a1, a2) && is_horizontal(b1, b2) {
        let x = a1.x;
        let y = b1.y;
        let within_x = x >= b1.x.min(b2.x) - CROSS_EPS && x <= b1.x.max(b2.x) + CROSS_EPS;
        let within_y = y >= a1.y.min(a2.y) - CROSS_EPS && y <= a1.y.max(a2.y) + CROSS_EPS;
        if within_x && within_y {
            let x = a1.x;
            let y = b1.y;
            let touches_endpoint = (x - b1.x).abs() < CROSS_EPS && (y - b1.y).abs() < CROSS_EPS
                || (x - b2.x).abs() < CROSS_EPS && (y - b2.y).abs() < CROSS_EPS
                || (x - a1.x).abs() < CROSS_EPS && (y - a1.y).abs() < CROSS_EPS
                || (x - a2.x).abs() < CROSS_EPS && (y - a2.y).abs() < CROSS_EPS;
            return !touches_endpoint;
        }
    }
    if is_horizontal(a1, a2) && is_vertical(b1, b2) {
        return segments_cross(b1, b2, a1, a2);
    }
    false
}

fn polylines_cross(first: &[Point], second: &[Point]) -> bool {
    for window in first.windows(2) {
        for other in second.windows(2) {
            if segments_cross(window[0], window[1], other[0], other[1]) {
                return true;
            }
        }
    }
    false
}

/// `edgesCross` from `raykov.crossing.spec.ts`: crossings at the shared source port are fine.
fn edges_cross(first: &[Point], second: &[Point]) -> bool {
    let mut shared = Point::default();
    if let Some(point) = first.first() {
        shared = *point;
    }
    for a in first.windows(2) {
        for b in second.windows(2) {
            let a_vertical = is_vertical(a[0], a[1]);
            let b_vertical = is_vertical(b[0], b[1]);
            if a_vertical == b_vertical {
                continue;
            }
            let (vertical, horizontal) = if a_vertical { (a, b) } else { (b, a) };
            let vert_coord = vertical[0].x;
            let horiz_coord = horizontal[0].y;
            let within_x = horizontal[0].x.min(horizontal[1].x) - CROSS_EPS <= vert_coord
                && vert_coord <= horizontal[0].x.max(horizontal[1].x) + CROSS_EPS;
            let within_y = vertical[0].y.min(vertical[1].y) - CROSS_EPS <= horiz_coord
                && horiz_coord <= vertical[0].y.max(vertical[1].y) + CROSS_EPS;
            if within_x
                && within_y
                && !((vert_coord - shared.x).abs() < CROSS_EPS
                    && (horiz_coord - shared.y).abs() < CROSS_EPS)
            {
                return true;
            }
        }
    }
    false
}

fn segments_through_rect(points: &[Point], node: &Node, margin: f64) -> bool {
    let half_width = node.width.unwrap_or(0.0) / 2.0;
    let half_height = node.height.unwrap_or(0.0) / 2.0;
    let (cx, cy) = (node.x.unwrap_or(0.0), node.y.unwrap_or(0.0));
    let min_x = cx - half_width + margin;
    let max_x = cx + half_width - margin;
    let min_y = cy - half_height + margin;
    let max_y = cy + half_height - margin;

    points.windows(2).any(|window| {
        let (p1, p2) = (window[0], window[1]);
        if is_vertical(p1, p2) {
            let x = p1.x;
            let y_min = p1.y.min(p2.y);
            let y_max = p1.y.max(p2.y);
            x > min_x && x < max_x && y_min < max_y && y_max > min_y
        } else if is_horizontal(p1, p2) {
            let y = p1.y;
            let x_min = p1.x.min(p2.x);
            let x_max = p1.x.max(p2.x);
            y > min_y && y < max_y && x_min < max_x && x_max > min_x
        } else {
            false
        }
    })
}

#[test]
fn places_the_start_port_on_the_right_edge_when_the_target_is_to_the_right() {
    let a = node("A", 100.0, 150.0, 100.0, 80.0);
    let b = node("B", 300.0, 150.0, 100.0, 80.0);
    let data = route(vec![a.clone(), b.clone()], vec![edge("e1", "A", "B")]);

    let routed = points(&data, "e1");
    assert!(routed.len() >= 2);
    assert!(is_on_node_boundary(routed[0], &a, 1.0));
    assert!(routed[0].x > a.x.unwrap());
    assert!(is_on_node_boundary(routed[routed.len() - 1], &b, 1.0));
    assert!(routed[routed.len() - 1].x < b.x.unwrap());
}

#[test]
fn places_ports_on_the_top_and_bottom_edges_for_vertical_routing() {
    let a = node("A", 150.0, 100.0, 100.0, 80.0);
    let b = node("B", 150.0, 300.0, 100.0, 80.0);
    let data = route(vec![a.clone(), b.clone()], vec![edge("e1", "A", "B")]);

    let routed = points(&data, "e1");
    assert!(is_on_node_boundary(routed[0], &a, 1.0));
    assert!(routed[0].y > a.y.unwrap());
    assert!(is_on_node_boundary(routed[routed.len() - 1], &b, 1.0));
    assert!(routed[routed.len() - 1].y < b.y.unwrap());
}

#[test]
fn a_straight_edge_keeps_a_constant_y() {
    let data = route(
        vec![
            node("A", 100.0, 150.0, 100.0, 80.0),
            node("B", 300.0, 150.0, 100.0, 80.0),
        ],
        vec![edge("e1", "A", "B")],
    );

    let routed = points(&data, "e1");
    assert!((routed[0].y - routed[routed.len() - 1].y).abs() < 1.0);
}

#[test]
fn includes_anchor_points_offset_from_the_ports() {
    let data = route(
        vec![
            node("A", 100.0, 150.0, 100.0, 80.0),
            node("B", 400.0, 150.0, 100.0, 80.0),
        ],
        vec![edge("e1", "A", "B")],
    );

    let routed = points(&data, "e1");
    assert!(routed.len() >= 3);
    let start_offset = (routed[1].x - routed[0].x).hypot(routed[1].y - routed[0].y);
    assert!(start_offset > 15.0, "anchor offset was {start_offset}");
}

fn longest_horizontal_y(routed: &[Point]) -> f64 {
    let mut longest = -1.0;
    let mut longest_y = -1.0;
    for window in routed.windows(2) {
        if (window[0].y - window[1].y).abs() < 0.1 {
            let length = (window[0].x - window[1].x).abs();
            if length > longest {
                longest = length;
                longest_y = window[0].y;
            }
        }
    }
    longest_y
}

#[test]
fn parallel_edges_get_distinct_tracks() {
    let data = route(
        vec![
            node("A", 100.0, 150.0, 100.0, 80.0),
            node("B", 300.0, 150.0, 100.0, 80.0),
        ],
        vec![
            edge("e1", "A", "B"),
            edge("e2", "A", "B"),
            edge("e3", "A", "B"),
        ],
    );

    let ys: Vec<f64> = ["e1", "e2", "e3"]
        .iter()
        .map(|id| longest_horizontal_y(&points(&data, id)))
        .collect();
    let mut unique: Vec<f64> = ys.iter().map(|y| (y / 0.5).round() * 0.5).collect();
    unique.sort_by(|a, b| a.partial_cmp(b).unwrap());
    unique.dedup();
    assert!(unique.len() > 1, "all parallel edges stayed on {ys:?}");
}

#[test]
fn converging_edges_keep_their_final_points_on_the_target_boundary() {
    let start1 = node("Start1", -165.078125, 22.5, 69.3515625, 45.0);
    let start2 = node("Start2", 4.2734375, 22.5, 69.3515625, 45.0);
    let stop = node("Stop", 169.3515625, 22.5, 60.8046875, 45.0);
    let data = route(
        vec![start1, start2, stop.clone()],
        vec![edge("e1", "Start1", "Stop"), edge("e2", "Start2", "Stop")],
    );

    let first = points(&data, "e1");
    let second = points(&data, "e2");
    let first_end = first[first.len() - 1];
    let second_end = second[second.len() - 1];
    let boundary_x = stop.x.unwrap() - stop.width.unwrap() / 2.0;

    assert!((first_end.x - boundary_x).abs() < 1.0);
    assert!((second_end.x - boundary_x).abs() < 1.0);

    let half_height = stop.height.unwrap() / 2.0;
    for end in [first_end, second_end] {
        assert!(end.y >= stop.y.unwrap() - half_height);
        assert!(end.y <= stop.y.unwrap() + half_height);
    }
    assert!((first_end.y - second_end.y).abs() > 0.1);
    let off_first = (first_end.y - stop.y.unwrap()).abs();
    let off_second = (second_end.y - stop.y.unwrap()).abs();
    assert!((off_first - off_second).abs() < 0.1);
}

#[test]
fn an_obstacle_between_the_endpoints_is_not_pierced() {
    let start1 = node("Start1", -165.078125, 22.5, 69.3515625, 45.0);
    let obstacle = node("Start2", 4.2734375, 22.5, 69.3515625, 45.0);
    let stop = node("Stop", 169.3515625, 22.5, 60.8046875, 45.0);
    let data = route(
        vec![start1, obstacle.clone(), stop],
        vec![edge("e1", "Start1", "Stop")],
    );

    let routed = points(&data, "e1");
    let half = (
        obstacle.width.unwrap() / 2.0,
        obstacle.height.unwrap() / 2.0,
    );
    let min_x = obstacle.x.unwrap() - half.0;
    let max_x = obstacle.x.unwrap() + half.0;
    let min_y = obstacle.y.unwrap() - half.1;
    let max_y = obstacle.y.unwrap() + half.1;
    let epsilon = 1e-3;
    for window in routed.windows(2) {
        let (p1, p2) = (window[0], window[1]);
        let seg_min_x = p1.x.min(p2.x);
        let seg_max_x = p1.x.max(p2.x);
        let seg_min_y = p1.y.min(p2.y);
        let seg_max_y = p1.y.max(p2.y);
        let inside_x = seg_min_x.max(min_x + epsilon) < seg_max_x.min(max_x - epsilon);
        let inside_y = seg_min_y.max(min_y + epsilon) < seg_max_y.min(max_y - epsilon);
        assert!(
            !(inside_x && inside_y),
            "segment {p1:?}->{p2:?} pierces the obstacle"
        );
    }
}

#[test]
fn spreads_ports_on_the_drawn_side_of_a_smaller_shape() {
    let drawn_side = 36.0;
    let mut target = node("T", 400.0, 150.0, 100.0, 120.0);
    target.metadata = Some(serde_json::json!({
        "drawnExtent": { "width": drawn_side, "height": drawn_side }
    }));
    let sources: Vec<Node> = [100.0, 150.0, 200.0]
        .iter()
        .enumerate()
        .map(|(index, y)| node(&format!("S{index}"), 100.0, *y, 100.0, 80.0))
        .collect();
    let edges: Vec<Edge> = sources
        .iter()
        .enumerate()
        .map(|(index, source)| edge(&format!("e{index}"), &source.id, "T"))
        .collect();
    let data = route(sources.into_iter().chain([target.clone()]).collect(), edges);

    for index in 0..3 {
        let routed = points(&data, &format!("e{index}"));
        let end = routed[routed.len() - 1];
        assert!((end.y - target.y.unwrap()).abs() <= drawn_side / 2.0);
    }
}

#[test]
fn keeps_a_full_side_of_ports_inside_the_drawn_side_and_distinct() {
    for count in [6usize, 7] {
        let drawn_side = 36.0;
        let mut target = node("T", 400.0, 300.0, 100.0, 120.0);
        target.metadata = Some(serde_json::json!({
            "drawnExtent": { "width": drawn_side, "height": drawn_side }
        }));
        let source_y =
            |index: usize| target.y.unwrap() + (index as f64 - (count as f64 - 1.0) / 2.0) * 12.0;
        let sources: Vec<Node> = (0..count)
            .map(|index| node(&format!("S{index}"), 0.0, source_y(index), 20.0, 10.0))
            .collect();
        let edges: Vec<Edge> = sources
            .iter()
            .enumerate()
            .map(|(index, source)| edge(&format!("e{index}"), &source.id, "T"))
            .collect();
        let data = route(sources.into_iter().chain([target.clone()]).collect(), edges);

        let ends: Vec<f64> = (0..count)
            .map(|index| {
                let routed = points(&data, &format!("e{index}"));
                routed[routed.len() - 1].y
            })
            .collect();
        for y in &ends {
            assert!((*y - target.y.unwrap()).abs() <= drawn_side / 2.0);
        }
        let mut distinct: Vec<String> = ends.iter().map(|y| format!("{y:.3}")).collect();
        distinct.sort();
        distinct.dedup();
        assert_eq!(distinct.len(), count);
    }
}

#[test]
fn ignores_a_drawn_extent_that_cannot_be_drawn() {
    let route_to = |metadata: Option<serde_json::Value>| -> Vec<Option<Vec<Point>>> {
        let mut target = node("T", 400.0, 150.0, 100.0, 120.0);
        target.metadata = metadata;
        let sources: Vec<Node> = [100.0, 150.0, 200.0]
            .iter()
            .enumerate()
            .map(|(index, y)| node(&format!("S{index}"), 100.0, *y, 100.0, 80.0))
            .collect();
        let edges: Vec<Edge> = sources
            .iter()
            .enumerate()
            .map(|(index, source)| edge(&format!("e{index}"), &source.id, "T"))
            .collect();
        let data = route(sources.into_iter().chain([target]).collect(), edges);
        (0..3)
            .map(|index| {
                data.edges
                    .iter()
                    .find(|edge| edge.id == format!("e{index}"))
                    .and_then(|edge| edge.points.clone())
            })
            .collect()
    };

    for bad in [0.0, -5.0, f64::NAN, f64::INFINITY] {
        let routed = route_to(Some(serde_json::json!({
            "drawnExtent": { "width": bad, "height": bad }
        })));
        assert_eq!(
            routed,
            route_to(None),
            "bad drawn extent {bad} changed the route"
        );
    }
}

#[test]
fn routes_same_row_edges_without_a_u_shaped_detour() {
    let constr = lane("Constr", -139.77734375, 70.0, 100.0, 190.0);
    let legal = lane("Legal", 0.78125, 70.0, 100.0, 190.0);
    let fun = lane("Fun", 140.55859375, 70.0, 100.0, 190.0);
    let h = with_parent(node("H", -139.77734375, 22.5, 41.34375, 45.0), "Constr");
    let i = with_parent(node("I", 0.78125, 22.5, 39.0, 45.0), "Legal");
    let j = with_parent(node("J", 0.78125, 117.5, 39.0, 45.0), "Legal");
    let e = with_parent(node("E", -139.77734375, 117.5, 41.34375, 45.0), "Constr");
    let f = with_parent(node("F", 140.55859375, 117.5, 40.5546875, 45.0), "Fun");
    let data = route(
        vec![constr, legal, fun, h, i, j, e, f],
        vec![
            edge("eHI", "H", "I"),
            edge("eIJ", "I", "J"),
            edge("eJE", "J", "E"),
            edge("eEF", "E", "F"),
        ],
    );

    let je = points(&data, "eJE");
    let je_min = je.iter().map(|point| point.y).fold(f64::INFINITY, f64::min);
    let je_max = je
        .iter()
        .map(|point| point.y)
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(
        je_max - je_min < 50.0,
        "J->E y range was {}",
        je_max - je_min
    );

    let ef = points(&data, "eEF");
    let ef_min = ef.iter().map(|point| point.y).fold(f64::INFINITY, f64::min);
    let ef_max = ef
        .iter()
        .map(|point| point.y)
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(ef_max - ef_min < 100.0);
}

#[test]
fn sibling_edges_leaving_one_node_do_not_cross() {
    let d = node("D", -19.5, 22.5, 42.11, 45.0);
    let e = node("E", -19.5, 117.5, 42.11, 45.0);
    let h = node("H", -19.5, 212.5, 42.11, 45.0);
    let data = route(
        vec![d, e, h],
        vec![edge("eDE", "D", "E"), edge("eDH", "D", "H")],
    );

    let de = points(&data, "eDE");
    let dh = points(&data, "eDH");
    assert!(!edges_cross(&de, &dh), "D->E and D->H cross");
}

#[test]
fn keeps_a_centered_edge_centered_and_its_sibling_distinct() {
    let d = node("D", 50.0, 22.5, 42.11, 45.0);
    let e = node("E", 50.0, 117.5, 41.34375, 45.0);
    let h = node("H", 50.0, 212.5, 42.11, 45.0);
    let data = route(
        vec![d, e, h],
        vec![edge("eDE", "D", "E"), edge("eDH", "D", "H")],
    );

    let de = points(&data, "eDE");
    assert!((de[0].x - 50.0).abs() < 1e-3);
    assert!((de[de.len() - 1].x - 50.0).abs() < 1e-3);
    assert_ne!(points(&data, "eDE"), points(&data, "eDH"));
}

fn dep_lanes() -> (Node, Node, Node) {
    (
        lane(
            "Dep1",
            -249.75390625 + 169.3515625 / 2.0,
            -6.0 + 87.0 / 2.0,
            169.3515625,
            87.0,
        ),
        lane(
            "Dep2",
            -80.40234375 + 169.3515625 / 2.0,
            -6.0 + 87.0 / 2.0,
            169.3515625,
            87.0,
        ),
        lane(
            "Dep3",
            88.94921875 + 160.8046875 / 2.0,
            -6.0 + 87.0 / 2.0,
            160.8046875,
            87.0,
        ),
    )
}

fn lane_nodes() -> (Node, Node, Node) {
    (
        with_parent(node("Start1", -165.078125, 22.5, 69.3515625, 45.0), "Dep1"),
        with_parent(node("Start2", 4.2734375, 22.5, 69.3515625, 45.0), "Dep2"),
        with_parent(node("Stop", 169.3515625, 22.5, 60.8046875, 45.0), "Dep3"),
    )
}

#[test]
fn edges_do_not_share_a_track_where_they_overlap() {
    let (dep1, dep2, dep3) = dep_lanes();
    let (start1, start2, stop) = lane_nodes();
    let data = route(
        vec![dep1, dep2, dep3, start1, start2, stop],
        vec![edge("e1", "Start1", "Stop"), edge("e2", "Start2", "Stop")],
    );

    let horizontal_segments = |routed: &[Point]| -> Vec<(f64, f64, f64)> {
        routed
            .windows(2)
            .filter(|window| (window[0].y - window[1].y).abs() < 0.01)
            .map(|window| {
                (
                    window[0].y,
                    window[0].x.min(window[1].x),
                    window[0].x.max(window[1].x),
                )
            })
            .collect()
    };
    let first = horizontal_segments(&points(&data, "e1"));
    let second = horizontal_segments(&points(&data, "e2"));
    for (y1, min1, max1) in &first {
        for (y2, min2, max2) in &second {
            if (y1 - y2).abs() < 1.0 {
                let overlap_start = min1.max(*min2);
                let overlap_end = max1.min(*max2);
                assert!(
                    overlap_start >= overlap_end - 1.0,
                    "overlap at y={y1}: [{overlap_start}, {overlap_end}]"
                );
            }
        }
    }
}

#[test]
fn cross_lane_edges_between_adjacent_lanes_do_not_backtrack() {
    let (_, dep2, dep3) = dep_lanes();
    let start2 = with_parent(node("Start2", 4.2734375, 22.5, 69.3515625, 45.0), "Dep2");
    let stop = with_parent(node("Stop", 169.3515625, 22.5, 60.8046875, 45.0), "Dep3");
    let data = route(
        vec![dep2, dep3, start2, stop],
        vec![edge("e2", "Start2", "Stop")],
    );

    let routed = points(&data, "e2");
    let mut max_backtrack = 0.0_f64;
    for window in routed.windows(2) {
        if window[1].x < window[0].x {
            max_backtrack = max_backtrack.max(window[0].x - window[1].x);
        }
    }
    assert!(max_backtrack < 5.0, "backtracked {max_backtrack}");
}

#[test]
fn an_edge_does_not_hook_back_at_its_target() {
    let (dep1, dep2, dep3) = dep_lanes();
    let (start1, start2, stop) = lane_nodes();
    let stop_x = stop.x.unwrap();
    let stop_width = stop.width.unwrap();
    let data = route(
        vec![dep1, dep2, dep3, start1, start2, stop],
        vec![edge("e1", "Start1", "Stop"), edge("e2", "Start2", "Stop")],
    );

    let routed = points(&data, "e1");
    let last = routed[routed.len() - 1];
    let previous = routed[routed.len() - 2];
    assert!((last.x - (stop_x - stop_width / 2.0)).abs() < 1.0);
    assert!(last.x - previous.x >= 0.0);
}

#[test]
fn an_edge_does_not_go_up_when_the_target_is_below() {
    let constr = lane("Constr", 100.0, 200.0, 150.0, 350.0);
    let legal = lane("Legal", 300.0, 200.0, 200.0, 350.0);
    let h = with_parent(node("H", 100.0, 50.0, 60.0, 40.0), "Constr");
    let e = with_parent(node("E", 100.0, 150.0, 60.0, 40.0), "Constr");
    let l = with_parent(node("L", 100.0, 320.0, 60.0, 40.0), "Constr");
    let i = with_parent(node("I", 280.0, 50.0, 80.0, 50.0), "Legal");
    let j = with_parent(node("J", 280.0, 150.0, 150.0, 100.0), "Legal");
    let k = with_parent(node("K", 280.0, 280.0, 60.0, 40.0), "Legal");
    let data = route(
        vec![constr, legal, h, e, l, i.clone(), j, k],
        vec![edge("I-K", "I", "K")],
    );

    let routed = points(&data, "I-K");
    assert!(routed[0].y >= i.y.unwrap());
    if routed.len() > 1 {
        assert!(routed[1].y >= routed[0].y - 5.0);
    }
}

#[test]
fn routes_around_a_wide_obstacle() {
    let i = node("I", 150.0, 0.0, 50.0, 90.0);
    let j = node("J", 150.0, 120.0, 300.0, 90.0);
    let k = node("K", 150.0, 240.0, 50.0, 90.0);
    let data = route(
        vec![i, j.clone(), k],
        vec![edge("eIJ", "I", "J"), edge("eIK", "I", "K")],
    );

    let ik = points(&data, "eIK");
    assert!(
        !segments_through_rect(&ik, &j, 2.0),
        "I->K passes through J"
    );
}

#[test]
fn an_edge_ends_on_its_target_boundary() {
    let j = node("J", 150.0, 120.0, 300.0, 90.0);
    let data = route(
        vec![node("I", 150.0, 0.0, 50.0, 90.0), j.clone()],
        vec![edge("eIJ", "I", "J")],
    );

    let routed = points(&data, "eIJ");
    assert!(is_on_node_boundary(routed[routed.len() - 1], &j, 1.0));
}

#[test]
fn routes_between_two_nodes_around_a_wide_middle_obstacle() {
    let b = node("B", 100.0, 100.0, 250.0, 50.0);
    let data = route(
        vec![
            node("A", 100.0, 0.0, 50.0, 50.0),
            b.clone(),
            node("C", 100.0, 200.0, 50.0, 50.0),
        ],
        vec![edge("eAC", "A", "C")],
    );

    let ac = points(&data, "eAC");
    assert!(
        !segments_through_rect(&ac, &b, 2.0),
        "A->C passes through B"
    );
}

#[test]
fn layout_only_edges_are_not_routed_or_kept() {
    let mut virtual_edge = edge("virtual", "A", "B");
    virtual_edge.is_layout_only = Some(true);
    let data = route(
        vec![
            node("A", 100.0, 150.0, 100.0, 80.0),
            node("B", 300.0, 150.0, 100.0, 80.0),
        ],
        vec![edge("real", "A", "B"), virtual_edge],
    );

    assert_eq!(data.edges.len(), 1);
    assert_eq!(data.edges[0].id, "real");
    assert!(data.edges[0].points.is_some());
}

#[test]
fn self_loops_are_left_untouched() {
    let mut loop_edge = edge("loop", "A", "A");
    loop_edge.points = Some(vec![Point::new(1.0, 2.0), Point::new(3.0, 4.0)]);
    let data = route(vec![node("A", 100.0, 150.0, 100.0, 80.0)], vec![loop_edge]);

    assert_eq!(
        data.edges[0].points,
        Some(vec![Point::new(1.0, 2.0), Point::new(3.0, 4.0)])
    );
}

fn geometry(id: &str, points: Vec<Point>) -> Edge {
    let mut edge = edge(id, "", "");
    edge.points = Some(points);
    edge
}

#[test]
fn line_jumps_arc_over_a_crossing() {
    let mut layout = LayoutData {
        edges: vec![
            geometry(
                "horizontal",
                vec![Point::new(0.0, 0.0), Point::new(100.0, 0.0)],
            ),
            geometry(
                "vertical",
                vec![Point::new(50.0, -50.0), Point::new(50.0, 50.0)],
            ),
        ],
        ..Default::default()
    };
    layout.direction = Some("TB".to_string());

    let paths = line_jump_paths(&layout, "arc");
    let horizontal = paths
        .iter()
        .find(|(id, _)| id == "horizontal")
        .map(|(_, path)| path)
        .expect("the horizontal edge has a path");
    assert!(
        horizontal
            .commands
            .iter()
            .any(|command| matches!(command, PathCommand::ArcTo { .. })),
        "the horizontal edge should hop over the vertical one"
    );
    assert!(
        !paths.iter().any(|(id, _)| id == "vertical"),
        "only the hopping edge is rewritten"
    );
}

#[test]
fn line_jumps_can_break_the_line_instead_of_arcing() {
    let layout = LayoutData {
        edges: vec![
            geometry(
                "horizontal",
                vec![Point::new(0.0, 0.0), Point::new(100.0, 0.0)],
            ),
            geometry(
                "vertical",
                vec![Point::new(50.0, -50.0), Point::new(50.0, 50.0)],
            ),
        ],
        ..Default::default()
    };

    let arc = line_jump_paths(&layout, "arc");
    let gap = line_jump_paths(&layout, "gap");
    assert_ne!(arc, gap);
    let gap_moves = gap
        .iter()
        .find(|(id, _)| id == "horizontal")
        .map(|(_, path)| {
            path.commands
                .iter()
                .filter(|command| matches!(command, PathCommand::MoveTo { .. }))
                .count()
        })
        .unwrap();
    assert_eq!(gap_moves, 2);
}

#[test]
fn parallel_edges_share_no_line_jump() {
    let layout = LayoutData {
        edges: vec![
            geometry("a", vec![Point::new(0.0, 0.0), Point::new(100.0, 0.0)]),
            geometry("b", vec![Point::new(0.0, 20.0), Point::new(100.0, 20.0)]),
        ],
        ..Default::default()
    };

    let paths = line_jump_paths(&layout, "arc");
    for (_, path) in paths {
        assert!(
            !path
                .commands
                .iter()
                .any(|command| matches!(command, PathCommand::ArcTo { .. }))
        );
    }
    assert!(!polylines_cross(
        &[Point::new(0.0, 0.0), Point::new(100.0, 0.0)],
        &[Point::new(0.0, 20.0), Point::new(100.0, 20.0)]
    ));
}

#[test]
fn the_layout_core_routes_its_edges_through_the_router() {
    use crate::layout::swimlanes::layout_core::run_swimlane_layout_core;

    let mut data = LayoutData {
        nodes: vec![
            lane("Lane1", 0.0, 0.0, 300.0, 200.0),
            lane("Lane2", 0.0, 200.0, 300.0, 200.0),
            with_parent(node("A", 0.0, 0.0, 80.0, 40.0), "Lane1"),
            with_parent(node("B", 0.0, 0.0, 80.0, 40.0), "Lane2"),
        ],
        edges: vec![edge("e1", "A", "B")],
        direction: Some("TB".to_string()),
        ..Default::default()
    };

    run_swimlane_layout_core(&mut data).expect("a lane-to-lane edge routes");
    assert!(
        data.edges[0]
            .points
            .as_ref()
            .is_some_and(|points| points.len() >= 2)
    );
    assert_eq!(data.edges[0].curve.as_deref(), Some("rounded"));
}

#[test]
fn the_lane_order_keeps_a_cross_lane_edge_to_the_right_of_its_lane() {
    let constr_x = 0.0;
    let legal_x = 150.0;
    let row_gap = 95.0;
    let h = node("H", constr_x, 0.0, 42.11, 45.0);
    let i = node("I", legal_x, 0.0, 42.11, 45.0);
    let j = node("J", legal_x, row_gap, 42.11, 45.0);
    let k = node("K", legal_x, row_gap * 2.0, 42.11, 45.0);
    let e = node("E", constr_x, row_gap, 42.11, 45.0);
    let l = node("L", constr_x, row_gap * 2.0, 42.11, 45.0);
    let data = route(
        vec![h, i, j, k, e, l],
        vec![
            edge("eHI", "H", "I"),
            edge("eIK", "I", "K"),
            edge("eIJ", "I", "J"),
            edge("eJE", "J", "E"),
            edge("eKL", "K", "L"),
        ],
    );

    let ik = points(&data, "eIK");
    let je = points(&data, "eJE");
    let min_x = ik.iter().map(|point| point.x).fold(f64::INFINITY, f64::min);
    assert!(min_x >= legal_x - 1.0, "I->K swung left to {min_x}");
    assert!(!polylines_cross(&ik, &je), "I->K crosses J->E");
}

#[test]
fn an_edge_routes_around_a_tall_node_instead_of_over_it() {
    let constr = lane("Constr", 100.0, 200.0, 150.0, 400.0);
    let legal = lane("Legal", 320.0, 200.0, 250.0, 400.0);
    let i = with_parent(node("I", 280.0, 30.0, 80.0, 50.0), "Legal");
    let j = with_parent(node("J", 280.0, 130.0, 200.0, 120.0), "Legal");
    let k = with_parent(node("K", 350.0, 280.0, 60.0, 40.0), "Legal");
    let l = with_parent(node("L", 100.0, 280.0, 60.0, 40.0), "Constr");
    let data = route(
        vec![constr, legal, i.clone(), j.clone(), k, l],
        vec![edge("I-K", "I", "K")],
    );

    let routed = points(&data, "I-K");
    let min_y = routed
        .iter()
        .map(|point| point.y)
        .fold(f64::INFINITY, f64::min);
    assert!(min_y >= i.y.unwrap() - 10.0, "the edge went up to {min_y}");

    let j_min_y = j.y.unwrap() - j.height.unwrap() / 2.0;
    let j_max_y = j.y.unwrap() + j.height.unwrap() / 2.0;
    let j_min_x = j.x.unwrap() - j.width.unwrap() / 2.0;
    let j_max_x = j.x.unwrap() + j.width.unwrap() / 2.0;
    for window in routed.windows(2) {
        let (p1, p2) = (window[0], window[1]);
        if (p1.x - p2.x).abs() < 1.0 && p1.x > j_min_x && p1.x < j_max_x {
            let seg_min_y = p1.y.min(p2.y);
            let seg_max_y = p1.y.max(p2.y);
            assert!(seg_min_y >= j_max_y || seg_max_y <= j_min_y);
        }
    }
}

#[test]
fn an_edge_leaves_from_the_bottom_center_when_the_target_is_directly_below() {
    let constr = lane("Constr", 50.0, 200.0, 100.0, 350.0);
    let legal = lane("Legal", 121.06, 200.0, 250.0, 350.0);
    let i = with_parent(node("I", 121.06, 22.5, 35.89, 45.0), "Legal");
    let j = with_parent(node("J", 121.06, 170.0, 232.0, 150.0), "Legal");
    let k = with_parent(node("K", 121.06, 280.0, 40.0, 40.0), "Legal");
    let data = route(
        vec![constr, legal, i.clone(), j, k],
        vec![edge("I-K", "I", "K")],
    );

    let routed = points(&data, "I-K");
    let first = routed[0];
    assert!((first.x - i.x.unwrap()).abs() < 1.0);
    assert!((first.y - (i.y.unwrap() + i.height.unwrap() / 2.0)).abs() < 1.0);
    let min_y = routed
        .iter()
        .map(|point| point.y)
        .fold(f64::INFINITY, f64::min);
    assert!(min_y >= i.y.unwrap() - i.height.unwrap() / 2.0 - 5.0);
}

#[test]
fn an_edge_ends_at_the_center_of_the_cardinal_side() {
    let (dep1, dep2, dep3) = dep_lanes();
    let start1 = with_parent(node("Start1", -165.078125, 22.5, 69.3515625, 45.0), "Dep1");
    let start2 = with_parent(node("Start2", 4.2734375, 22.5, 69.3515625, 45.0), "Dep2");
    let stop = with_parent(node("Stop", 169.3515625, 22.5, 60.8046875, 45.0), "Dep3");
    let data = route(
        vec![dep1, dep2, dep3, start1, start2, stop.clone()],
        vec![edge("e1", "Start1", "Stop")],
    );

    let routed = points(&data, "e1");
    let last = routed[routed.len() - 1];
    let previous = routed[routed.len() - 2];
    assert!((last.x - 138.94921875).abs() < 1e-3);
    assert!((last.y - 22.5).abs() < 1e-3);
    assert!(previous.x <= last.x + 0.1);
    assert_eq!(stop.x, Some(169.3515625));
}
