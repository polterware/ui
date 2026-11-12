//! Port of `__tests__/terminalLegAlignment.spec.ts`.

use crate::layout::swimlanes::direction::endpoint_clip::CORNER_CLEARANCE;
use crate::layout::swimlanes::direction::terminal_leg_alignment::align_terminal_legs_to_ports;
use crate::render::pipeline::types::{Edge, LayoutData, Node};
use crate::scene::Point;

const SIZE: f64 = 100.0;
const HALF: f64 = SIZE / 2.0;
const STUB: f64 = 20.0;
const RAIL_Y: f64 = HALF + STUB;
const TARGET_X: f64 = 300.0;
const TARGET_HEIGHT: f64 = 50.0;
const TARGET_Y: f64 = RAIL_Y + STUB + TARGET_HEIGHT / 2.0;
const TARGET_TOP: f64 = TARGET_Y - TARGET_HEIGHT / 2.0;
const OFF_VERTEX: f64 = 32.0;

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn box_node(id: &str, shape: &str, x: f64, y: f64, height: f64) -> Node {
    Node {
        id: id.to_string(),
        shape: Some(shape.to_string()),
        is_group: Some(false),
        x: Some(x),
        y: Some(y),
        width: Some(SIZE),
        height: Some(height),
        ..Default::default()
    }
}

fn edge(id: &str, start: &str, end: &str, points: &[Point]) -> Edge {
    Edge {
        id: id.to_string(),
        start: Some(start.to_string()),
        end: Some(end.to_string()),
        points: Some(points.to_vec()),
        ..Default::default()
    }
}

fn layout(nodes: Vec<Node>, edges: Vec<Edge>) -> LayoutData {
    LayoutData {
        nodes,
        edges,
        ..Default::default()
    }
}

/// A diamond at the origin with an edge leaving below it, then running right to a target.
fn below_diamond(start_x: f64, end_x: f64, source_shape: &str) -> LayoutData {
    layout(
        vec![
            box_node("D", source_shape, 0.0, 0.0, SIZE),
            box_node("T", "roundedRect", TARGET_X, TARGET_Y, TARGET_HEIGHT),
        ],
        vec![edge(
            "D-T",
            "D",
            "T",
            &[
                p(start_x, HALF),
                p(start_x, RAIL_Y),
                p(end_x, RAIL_Y),
                p(end_x, TARGET_TOP),
            ],
        )],
    )
}

fn route(layout: &LayoutData) -> &Vec<Point> {
    layout.edges[0]
        .points
        .as_ref()
        .expect("the edge keeps its points")
}

#[test]
fn moves_a_leg_that_leaves_beside_a_diamond_vertex_onto_the_vertex() {
    let mut layout = below_diamond(-OFF_VERTEX, TARGET_X, "diamond");
    assert!(align_terminal_legs_to_ports(&mut layout));
    assert_eq!(&route(&layout)[0..2], &[p(0.0, HALF), p(0.0, RAIL_Y)]);
}

#[test]
fn treats_every_diamond_name_as_a_diamond() {
    for shape in ["question", "diamond", "diam", "decision"] {
        let mut layout = below_diamond(-OFF_VERTEX, TARGET_X, shape);
        assert!(align_terminal_legs_to_ports(&mut layout));
        assert_eq!(route(&layout)[0], p(0.0, HALF));
    }
}

#[test]
fn centres_a_leg_that_endpoint_clipping_left_against_a_rectangle_corner() {
    let corner_x = TARGET_X - HALF + CORNER_CLEARANCE;
    let mut layout = below_diamond(0.0, corner_x, "diamond");
    assert!(align_terminal_legs_to_ports(&mut layout));
    let points = route(&layout);
    assert_eq!(
        &points[points.len() - 2..],
        &[p(TARGET_X, RAIL_Y), p(TARGET_X, TARGET_TOP)]
    );
}

#[test]
fn leaves_a_rectangle_leg_alone_when_it_is_off_centre_but_clear_of_the_corner() {
    let mut layout = below_diamond(0.0, TARGET_X - HALF + CORNER_CLEARANCE + 1.0, "diamond");
    let before = route(&layout).clone();
    assert!(!align_terminal_legs_to_ports(&mut layout));
    assert_eq!(route(&layout), &before);
}

#[test]
fn leaves_a_rectangle_source_off_centre_only_a_diamond_face_is_a_single_port() {
    let mut layout = below_diamond(-OFF_VERTEX, TARGET_X, "roundedRect");
    assert!(!align_terminal_legs_to_ports(&mut layout));
    assert_eq!(route(&layout)[0], p(-OFF_VERTEX, HALF));
}

#[test]
fn keeps_a_diamond_lane_the_pipeline_separated_while_another_edge_leaves_through_that_face() {
    let mut layout = below_diamond(-OFF_VERTEX, TARGET_X, "diamond");
    layout.edges.push(edge(
        "D-T2",
        "D",
        "T2",
        &[p(OFF_VERTEX, HALF + STUB), p(OFF_VERTEX, HALF + STUB * 2.0)],
    ));
    assert!(!align_terminal_legs_to_ports(&mut layout));
    assert_eq!(route(&layout)[0], p(-OFF_VERTEX, HALF));
}

#[test]
fn refuses_a_move_that_would_run_the_leg_through_another_node() {
    let mut layout = below_diamond(-OFF_VERTEX, TARGET_X, "diamond");
    let blocker_size = STUB / 2.0;
    layout.nodes.push(Node {
        id: "X".to_string(),
        shape: Some("roundedRect".to_string()),
        is_group: Some(false),
        x: Some(0.0),
        y: Some(HALF + STUB / 2.0),
        width: Some(blocker_size),
        height: Some(blocker_size),
        ..Default::default()
    });
    assert!(!align_terminal_legs_to_ports(&mut layout));
}

#[test]
fn refuses_a_move_that_would_cross_another_edge() {
    let mut layout = below_diamond(-OFF_VERTEX, TARGET_X, "diamond");
    layout.edges.push(edge(
        "P-Q",
        "P",
        "Q",
        &[
            p(-OFF_VERTEX / 2.0, HALF + STUB / 2.0),
            p(OFF_VERTEX, HALF + STUB / 2.0),
        ],
    ));
    assert!(!align_terminal_legs_to_ports(&mut layout));
}

#[test]
fn refuses_a_move_that_would_run_along_another_edge() {
    let mut layout = below_diamond(-OFF_VERTEX, TARGET_X, "diamond");
    layout.edges.push(edge(
        "P-Q",
        "P",
        "Q",
        &[p(0.0, HALF + STUB / 4.0), p(0.0, RAIL_Y - STUB / 4.0)],
    ));
    assert!(!align_terminal_legs_to_ports(&mut layout));
}

#[test]
fn moves_a_leg_beside_a_diamond_vertex_on_a_left_or_right_face() {
    let nodes = vec![
        box_node("D", "diamond", 0.0, 0.0, SIZE),
        box_node("T", "roundedRect", TARGET_X, -TARGET_X, TARGET_HEIGHT),
    ];
    let right = edge(
        "D-T",
        "D",
        "T",
        &[
            p(HALF, OFF_VERTEX),
            p(RAIL_Y, OFF_VERTEX),
            p(RAIL_Y, -TARGET_X),
            p(TARGET_X - HALF, -TARGET_X),
        ],
    );
    let left = edge(
        "T-D",
        "T",
        "D",
        &[
            p(TARGET_X - HALF, -TARGET_X - CORNER_CLEARANCE),
            p(-RAIL_Y, -TARGET_X - CORNER_CLEARANCE),
            p(-RAIL_Y, -OFF_VERTEX),
            p(-HALF, -OFF_VERTEX),
        ],
    );
    let mut layout = layout(nodes, vec![right, left]);

    assert!(align_terminal_legs_to_ports(&mut layout));

    let right_points = layout.edges[0].points.as_ref().unwrap();
    assert_eq!(&right_points[0..2], &[p(HALF, 0.0), p(RAIL_Y, 0.0)]);
    let left_points = layout.edges[1].points.as_ref().unwrap();
    assert_eq!(
        &left_points[left_points.len() - 2..],
        &[p(-RAIL_Y, 0.0), p(-HALF, 0.0)]
    );
}

#[test]
fn centres_a_leg_against_either_corner_of_the_face() {
    let mut layout = below_diamond(0.0, TARGET_X + HALF - CORNER_CLEARANCE, "diamond");
    assert!(align_terminal_legs_to_ports(&mut layout));
    let points = route(&layout);
    assert_eq!(points.last(), Some(&p(TARGET_X, TARGET_TOP)));
}

#[test]
fn straightens_the_route_when_the_leg_lands_on_the_rail_end() {
    let mut layout = layout(
        vec![
            box_node("D", "diamond", 0.0, 0.0, SIZE),
            box_node("T", "roundedRect", 0.0, TARGET_Y, TARGET_HEIGHT),
        ],
        vec![edge(
            "D-T",
            "D",
            "T",
            &[
                p(-OFF_VERTEX, HALF),
                p(-OFF_VERTEX, RAIL_Y),
                p(0.0, RAIL_Y),
                p(0.0, TARGET_TOP),
            ],
        )],
    );

    assert!(align_terminal_legs_to_ports(&mut layout));
    assert_eq!(route(&layout), &vec![p(0.0, HALF), p(0.0, TARGET_TOP)]);
}

#[test]
fn leaves_a_leg_whose_route_runs_on_straight_from_it() {
    let mut layout = below_diamond(-OFF_VERTEX, TARGET_X, "diamond");
    layout.edges[0].points = Some(vec![
        p(-OFF_VERTEX, HALF),
        p(-OFF_VERTEX, RAIL_Y),
        p(-OFF_VERTEX, RAIL_Y + STUB),
    ]);
    assert!(!align_terminal_legs_to_ports(&mut layout));
}

#[test]
fn moves_the_leg_when_the_other_edge_of_the_diamond_leaves_beyond_the_face() {
    let mut layout = below_diamond(-OFF_VERTEX, TARGET_X, "diamond");
    layout.edges.push(edge(
        "D-T2",
        "D",
        "T2",
        &[p(SIZE, RAIL_Y + STUB), p(SIZE, RAIL_Y + STUB * 2.0)],
    ));
    assert!(align_terminal_legs_to_ports(&mut layout));
}

#[test]
fn refuses_a_move_that_would_share_a_rail_with_another_edge() {
    let mut layout = below_diamond(-OFF_VERTEX, TARGET_X, "diamond");
    layout.edges.push(edge(
        "P-Q",
        "P",
        "Q",
        &[p(TARGET_X / 3.0, RAIL_Y), p((TARGET_X * 2.0) / 3.0, RAIL_Y)],
    ));
    assert!(!align_terminal_legs_to_ports(&mut layout));
}

#[test]
fn refuses_a_move_whose_route_runs_through_its_own_target() {
    let mut layout = layout(
        vec![
            box_node("D", "diamond", 0.0, 0.0, SIZE),
            box_node("T", "roundedRect", TARGET_X, RAIL_Y, TARGET_HEIGHT),
        ],
        vec![edge(
            "D-T",
            "D",
            "T",
            &[
                p(-OFF_VERTEX, HALF),
                p(-OFF_VERTEX, RAIL_Y),
                p(TARGET_X + SIZE, RAIL_Y),
                p(TARGET_X + SIZE, RAIL_Y - TARGET_HEIGHT),
                p(TARGET_X, RAIL_Y - TARGET_HEIGHT),
                p(TARGET_X, RAIL_Y - TARGET_HEIGHT / 2.0),
            ],
        )],
    );
    assert!(!align_terminal_legs_to_ports(&mut layout));
}
