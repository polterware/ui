//! Port of `__tests__/alignedEdgeStraightening.spec.ts`.

use crate::layout::swimlanes::direction::aligned_edge_straightening::{
    ALIGNED_CENTRE_TOLERANCE, straighten_aligned_cross_lane_edges,
};
use crate::layout::swimlanes::post_processing::post_process_swimlane_layout;
use crate::render::pipeline::types::{Edge, LayoutData, Node};
use crate::scene::Point;

const SIZE: f64 = 100.0;
const HALF: f64 = SIZE / 2.0;
const GAP: f64 = 200.0;
const DETOUR: f64 = 150.0;
const LABEL: f64 = 20.0;

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn boxed(id: &str, shape: &str, x: f64, y: f64, parent_id: &str, width: f64, height: f64) -> Node {
    Node {
        id: id.to_string(),
        shape: Some(shape.to_string()),
        is_group: Some(false),
        parent_id: Some(parent_id.to_string()),
        x: Some(x),
        y: Some(y),
        width: Some(width),
        height: Some(height),
        ..Default::default()
    }
}

fn box_node(id: &str, shape: &str, x: f64, y: f64, parent_id: &str) -> Node {
    boxed(id, shape, x, y, parent_id, SIZE, SIZE)
}

fn edge(id: &str, start: &str, end: &str, points: Vec<Point>) -> Edge {
    Edge {
        id: id.to_string(),
        start: Some(start.to_string()),
        end: Some(end.to_string()),
        points: Some(points),
        ..Default::default()
    }
}

fn route_points(points: &[Point]) -> Vec<Point> {
    points.to_vec()
}

/// A diamond in lane `top` above a diamond in lane `low`, joined by a detour round the right.
fn stacked(dst_x: f64, dst_lane: &str) -> (Vec<Node>, Edge) {
    let nodes = vec![
        box_node("K", "diamond", 0.0, 0.0, "top"),
        box_node("S", "diamond", dst_x, GAP, dst_lane),
    ];
    let route = edge(
        "K-S",
        "K",
        "S",
        route_points(&[
            p(HALF, 0.0),
            p(DETOUR, 0.0),
            p(DETOUR, GAP - HALF - LABEL),
            p(dst_x, GAP - HALF - LABEL),
            p(dst_x, GAP - HALF),
        ]),
    );
    (nodes, route)
}

#[test]
fn joins_the_facing_vertices_of_two_diamonds_aligned_in_different_lanes() {
    let (nodes, mut route) = stacked(0.0, "low");
    assert!(straighten_aligned_cross_lane_edges(
        std::slice::from_mut(&mut route),
        &nodes
    ));
    assert_eq!(
        route.points.as_deref(),
        Some([p(0.0, HALF), p(0.0, GAP - HALF)].as_slice())
    );
}

#[test]
fn runs_upward_when_the_target_lies_above_the_source() {
    let nodes = vec![
        box_node("K", "diamond", 0.0, GAP, "low"),
        box_node("S", "diamond", 0.0, 0.0, "top"),
    ];
    let mut route = edge(
        "K-S",
        "K",
        "S",
        route_points(&[
            p(HALF, GAP),
            p(DETOUR, GAP),
            p(DETOUR, HALF + LABEL),
            p(0.0, HALF + LABEL),
            p(0.0, HALF),
        ]),
    );
    assert!(straighten_aligned_cross_lane_edges(
        std::slice::from_mut(&mut route),
        &nodes
    ));
    assert_eq!(
        route.points.as_deref(),
        Some([p(0.0, GAP - HALF), p(0.0, HALF)].as_slice())
    );
}

#[test]
fn joins_the_facing_face_centres_of_two_rectangles_side_by_side_both_ways() {
    let nodes = vec![
        box_node("L", "roundedRect", 0.0, 0.0, "left"),
        box_node("R", "roundedRect", GAP, 0.0, "right"),
    ];
    let detour = |from: f64, to: f64| {
        route_points(&[p(from, HALF), p(from, DETOUR), p(to, DETOUR), p(to, HALF)])
    };
    let mut rightward = edge("L-R", "L", "R", detour(0.0, GAP));
    let mut leftward = edge("R-L", "R", "L", detour(GAP, 0.0));
    assert!(straighten_aligned_cross_lane_edges(
        std::slice::from_mut(&mut rightward),
        &nodes
    ));
    assert_eq!(
        rightward.points.as_deref(),
        Some([p(HALF, 0.0), p(GAP - HALF, 0.0)].as_slice())
    );
    assert!(straighten_aligned_cross_lane_edges(
        std::slice::from_mut(&mut leftward),
        &nodes
    ));
    assert_eq!(
        leftward.points.as_deref(),
        Some([p(GAP - HALF, 0.0), p(HALF, 0.0)].as_slice())
    );
}

#[test]
fn accepts_centres_within_the_tolerance_and_refuses_centres_beyond_it() {
    let (near_nodes, mut near_route) = stacked(ALIGNED_CENTRE_TOLERANCE, "low");
    assert!(straighten_aligned_cross_lane_edges(
        std::slice::from_mut(&mut near_route),
        &near_nodes
    ));
    let (far_nodes, mut far_route) = stacked(ALIGNED_CENTRE_TOLERANCE * 2.0, "low");
    assert!(!straighten_aligned_cross_lane_edges(
        std::slice::from_mut(&mut far_route),
        &far_nodes
    ));
}

#[test]
fn leaves_an_edge_between_two_nodes_of_the_same_lane_alone() {
    let (nodes, mut route) = stacked(0.0, "top");
    assert!(!straighten_aligned_cross_lane_edges(
        std::slice::from_mut(&mut route),
        &nodes
    ));
}

#[test]
fn leaves_an_edge_alone_when_another_edge_uses_either_facing_face() {
    let (from_source_nodes, from_source_route) = stacked(0.0, "low");
    let sibling = edge(
        "K-T",
        "K",
        "T",
        vec![p(LABEL, HALF + LABEL / 2.0), p(GAP, HALF + LABEL / 2.0)],
    );
    assert!(!straighten_aligned_cross_lane_edges(
        &mut [from_source_route, sibling],
        &from_source_nodes
    ));

    let (into_target_nodes, into_target_route) = stacked(0.0, "low");
    let other = edge(
        "U-S",
        "U",
        "S",
        vec![
            p(GAP, GAP - HALF - LABEL / 2.0),
            p(LABEL, GAP - HALF - LABEL / 2.0),
        ],
    );
    assert!(!straighten_aligned_cross_lane_edges(
        &mut [into_target_route, other],
        &into_target_nodes
    ));
}

#[test]
fn leaves_an_edge_alone_when_a_node_or_a_label_sits_between_the_two() {
    let (mut blocked_nodes, mut blocked_route) = stacked(0.0, "low");
    blocked_nodes.push(boxed(
        "X",
        "roundedRect",
        0.0,
        GAP / 2.0,
        "mid",
        LABEL,
        LABEL,
    ));
    assert!(!straighten_aligned_cross_lane_edges(
        std::slice::from_mut(&mut blocked_route),
        &blocked_nodes
    ));

    let (mut labelled_nodes, mut labelled_route) = stacked(0.0, "low");
    let mut label = boxed("L", "roundedRect", 0.0, GAP / 2.0, "mid", LABEL, LABEL);
    label.is_edge_label = Some(true);
    labelled_nodes.push(label);
    assert!(!straighten_aligned_cross_lane_edges(
        std::slice::from_mut(&mut labelled_route),
        &labelled_nodes
    ));
}

#[test]
fn leaves_an_edge_alone_when_the_line_would_cross_another_edge() {
    let (nodes, route) = stacked(0.0, "low");
    let across = edge(
        "P-Q",
        "P",
        "Q",
        vec![
            p(-DETOUR, GAP / 2.0),
            p(-LABEL, GAP / 2.0),
            p(-LABEL, GAP / 2.0 + LABEL),
            p(LABEL, GAP / 2.0 + LABEL),
        ],
    );
    assert!(!straighten_aligned_cross_lane_edges(
        &mut [route, across],
        &nodes
    ));
}

#[test]
fn leaves_two_overlapping_nodes_and_an_already_straight_edge_alone() {
    let (mut overlapping_nodes, mut overlapping_route) = stacked(0.0, "low");
    let index = overlapping_nodes
        .iter()
        .position(|node| node.id == "S")
        .unwrap();
    overlapping_nodes[index] = box_node("S", "diamond", 0.0, HALF, "low");
    assert!(!straighten_aligned_cross_lane_edges(
        std::slice::from_mut(&mut overlapping_route),
        &overlapping_nodes
    ));

    let (nodes, mut route) = stacked(0.0, "low");
    assert!(straighten_aligned_cross_lane_edges(
        std::slice::from_mut(&mut route),
        &nodes
    ));
    assert!(!straighten_aligned_cross_lane_edges(
        std::slice::from_mut(&mut route),
        &nodes
    ));
}

#[test]
fn reroutes_a_route_that_starts_on_the_line_but_turns_off_it() {
    let (nodes, mut route) = stacked(0.0, "low");
    route.points = Some(vec![
        p(0.0, HALF),
        p(0.0, GAP - HALF - LABEL),
        p(LABEL, GAP - HALF - LABEL),
    ]);
    assert!(straighten_aligned_cross_lane_edges(
        std::slice::from_mut(&mut route),
        &nodes
    ));
    assert_eq!(
        route.points.as_deref(),
        Some([p(0.0, HALF), p(0.0, GAP - HALF)].as_slice())
    );
}

#[test]
fn completes_a_straight_route_that_stops_short_of_the_target() {
    let (nodes, mut route) = stacked(0.0, "low");
    route.points = Some(vec![p(0.0, HALF), p(0.0, GAP - HALF - LABEL)]);
    assert!(straighten_aligned_cross_lane_edges(
        std::slice::from_mut(&mut route),
        &nodes
    ));
}

#[test]
fn leaves_layout_only_edges_edges_without_a_route_and_edges_into_a_group_alone() {
    let (nodes, mut route) = stacked(0.0, "low");
    route.is_layout_only = Some(true);
    assert!(!straighten_aligned_cross_lane_edges(
        std::slice::from_mut(&mut route),
        &nodes
    ));

    let (nodes, mut route) = stacked(0.0, "low");
    route.points = Some(Vec::new());
    assert!(!straighten_aligned_cross_lane_edges(
        std::slice::from_mut(&mut route),
        &nodes
    ));

    let (mut nodes, mut route) = stacked(0.0, "low");
    let index = nodes.iter().position(|node| node.id == "S").unwrap();
    nodes[index] = Node {
        id: "S".to_string(),
        is_group: Some(true),
        parent_id: Some("low".to_string()),
        x: Some(0.0),
        y: Some(GAP),
        width: Some(SIZE),
        height: Some(SIZE),
        ..Default::default()
    };
    assert!(!straighten_aligned_cross_lane_edges(
        std::slice::from_mut(&mut route),
        &nodes
    ));
}

#[test]
fn uses_the_faces_that_face_each_other_when_checking_that_they_are_free() {
    let nodes = vec![
        box_node("K", "diamond", 0.0, GAP, "low"),
        box_node("S", "diamond", 0.0, 0.0, "top"),
    ];
    let route = edge(
        "K-S",
        "K",
        "S",
        vec![
            p(HALF, GAP),
            p(DETOUR, GAP),
            p(DETOUR, HALF + LABEL),
            p(0.0, HALF + LABEL),
            p(0.0, HALF),
        ],
    );
    let sibling = edge(
        "K-T",
        "K",
        "T",
        vec![
            p(LABEL, GAP - HALF - LABEL / 2.0),
            p(GAP, GAP - HALF - LABEL / 2.0),
        ],
    );
    assert!(!straighten_aligned_cross_lane_edges(
        &mut [route, sibling],
        &nodes
    ));

    let side = vec![
        box_node("L", "roundedRect", 0.0, 0.0, "left"),
        box_node("R", "roundedRect", GAP, 0.0, "right"),
    ];
    let leftward = edge(
        "R-L",
        "R",
        "L",
        vec![p(GAP, HALF), p(GAP, DETOUR), p(0.0, DETOUR), p(0.0, HALF)],
    );
    let into = edge(
        "U-L",
        "U",
        "L",
        vec![p(HALF + LABEL / 2.0, -GAP), p(HALF + LABEL / 2.0, -LABEL)],
    );
    assert!(!straighten_aligned_cross_lane_edges(
        &mut [leftward, into],
        &side
    ));
}

#[test]
fn leaves_side_by_side_nodes_that_overlap_or_are_not_level_alone() {
    let overlapping = vec![
        box_node("L", "roundedRect", 0.0, 0.0, "left"),
        box_node("R", "roundedRect", HALF, 0.0, "right"),
    ];
    let not_level = vec![
        box_node("L", "roundedRect", 0.0, 0.0, "left"),
        box_node(
            "R",
            "roundedRect",
            GAP,
            ALIGNED_CENTRE_TOLERANCE * 2.0,
            "right",
        ),
    ];
    let detour = || {
        edge(
            "L-R",
            "L",
            "R",
            vec![p(0.0, HALF), p(0.0, DETOUR), p(GAP, DETOUR), p(GAP, HALF)],
        )
    };
    assert!(!straighten_aligned_cross_lane_edges(
        std::slice::from_mut(&mut detour()),
        &overlapping
    ));
    assert!(!straighten_aligned_cross_lane_edges(
        std::slice::from_mut(&mut detour()),
        &not_level
    ));
}

#[test]
fn reanchors_the_label_of_a_straightened_edge_onto_the_line() {
    let (nodes, route) = stacked(0.0, "low");
    let mut label = boxed(
        "K-S-label",
        "roundedRect",
        DETOUR,
        GAP / 2.0,
        "top",
        LABEL,
        LABEL,
    );
    label.is_edge_label = Some(true);
    let lane = |id: &str, y: f64| Node {
        id: id.to_string(),
        is_group: Some(true),
        x: Some(0.0),
        y: Some(y),
        width: Some(GAP * 2.0),
        height: Some(GAP),
        ..Default::default()
    };
    let mut edge = route;
    edge.label_node_id = Some(label.id.clone());
    let mut layout = LayoutData {
        nodes: vec![
            lane("top", 0.0),
            lane("low", GAP),
            nodes[0].clone(),
            nodes[1].clone(),
            label,
        ],
        edges: vec![edge],
        ..Default::default()
    };

    post_process_swimlane_layout(&mut layout, None);

    assert_eq!(
        layout.edges[0].points.as_deref(),
        Some([p(0.0, HALF), p(0.0, GAP - HALF)].as_slice())
    );
    assert_eq!(
        layout
            .nodes
            .iter()
            .find(|node| node.id == "K-S-label")
            .unwrap()
            .x,
        Some(0.0)
    );
}
