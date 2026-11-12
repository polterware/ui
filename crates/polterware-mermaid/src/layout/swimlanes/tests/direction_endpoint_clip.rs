//! Port of `__tests__/direction.endpointClip.spec.ts`.

use crate::layout::swimlanes::direction::endpoint_clip::{
    clip_edge_endpoints_to_node_boundaries, prepare_edge_endpoints_for_renderer,
};
use crate::render::pipeline::types::{Edge, LayoutData, Node};
use crate::scene::Point;

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn node(id: &str, x: f64, y: f64, width: f64, height: f64) -> Node {
    Node {
        id: id.to_string(),
        x: Some(x),
        y: Some(y),
        width: Some(width),
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

#[test]
fn clips_buried_endpoints_to_source_and_destination_node_boundaries() {
    let mut layout = layout(
        vec![
            node("A", 0.0, 0.0, 10.0, 10.0),
            node("B", 20.0, 0.0, 10.0, 10.0),
        ],
        vec![edge("A_B", "A", "B", &[p(0.0, 0.0), p(20.0, 0.0)])],
    );

    clip_edge_endpoints_to_node_boundaries(&mut layout);

    assert_eq!(
        layout.edges[0].points,
        Some(vec![p(5.0, 0.0), p(15.0, 0.0)])
    );
}

#[test]
fn moves_straight_side_to_side_endpoints_away_from_node_corners() {
    let mut layout = layout(
        vec![
            node("A", 0.0, 0.0, 20.0, 20.0),
            node("B", 40.0, 0.0, 20.0, 40.0),
        ],
        vec![edge("A_B", "A", "B", &[p(10.0, 8.0), p(30.0, 8.0)])],
    );

    clip_edge_endpoints_to_node_boundaries(&mut layout);

    assert_eq!(
        layout.edges[0].points,
        Some(vec![p(10.0, 6.0), p(30.0, 6.0)])
    );
}

#[test]
fn duplicates_snapped_endpoints_so_renderer_clipping_becomes_a_no_op() {
    let mut layout = layout(
        vec![
            node("A", 0.0, 0.0, 10.0, 10.0),
            node("B", 20.0, 0.0, 10.0, 10.0),
        ],
        vec![edge(
            "A_B",
            "A",
            "B",
            &[p(-5.0, 0.0), p(10.0, 0.0), p(15.0, 0.0)],
        )],
    );

    prepare_edge_endpoints_for_renderer(&mut layout);

    assert_eq!(
        layout.edges[0].points,
        Some(vec![
            p(-5.0, 0.0),
            p(-5.0, 0.0),
            p(10.0, 0.0),
            p(15.0, 0.0),
            p(15.0, 0.0),
        ])
    );
}

#[test]
fn is_idempotent_when_renderer_endpoints_are_already_duplicated() {
    let mut layout = layout(
        vec![
            node("A", 0.0, 0.0, 10.0, 10.0),
            node("B", 20.0, 0.0, 10.0, 10.0),
        ],
        vec![edge(
            "A_B",
            "A",
            "B",
            &[p(-5.0, 0.0), p(10.0, 0.0), p(15.0, 0.0)],
        )],
    );

    prepare_edge_endpoints_for_renderer(&mut layout);
    let once = layout.edges[0].points.clone();
    prepare_edge_endpoints_for_renderer(&mut layout);

    assert_eq!(layout.edges[0].points, once);
}

#[test]
fn keeps_straight_renderer_edges_two_point_while_clearing_corner_ports() {
    let mut layout = layout(
        vec![
            node("A", 0.0, 0.0, 20.0, 20.0),
            node("B", 40.0, 0.0, 20.0, 40.0),
        ],
        vec![edge("A_B", "A", "B", &[p(10.0, 8.0), p(30.0, 8.0)])],
    );

    prepare_edge_endpoints_for_renderer(&mut layout);

    assert_eq!(
        layout.edges[0].points,
        Some(vec![p(10.0, 6.0), p(30.0, 6.0)])
    );
}

#[test]
fn snaps_renderer_endpoints_to_the_boundary_entered_by_the_approach_segment() {
    let mut layout = layout(
        vec![
            node("A", 0.0, 0.0, 10.0, 10.0),
            node("B", 40.0, 40.0, 20.0, 20.0),
        ],
        vec![edge(
            "A_B",
            "A",
            "B",
            &[p(5.0, 0.0), p(40.0, 0.0), p(40.0, 50.0)],
        )],
    );

    prepare_edge_endpoints_for_renderer(&mut layout);

    assert_eq!(
        layout.edges[0].points,
        Some(vec![
            p(5.0, 0.0),
            p(5.0, 0.0),
            p(40.0, 0.0),
            p(40.0, 30.0),
            p(40.0, 30.0),
        ])
    );
}
