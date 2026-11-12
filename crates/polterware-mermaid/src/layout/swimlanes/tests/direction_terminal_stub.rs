//! Port of `__tests__/direction.terminalStub.spec.ts`.

use crate::layout::swimlanes::direction::terminal_stub::collapse_short_terminal_stub;
use crate::render::pipeline::types::{Edge, LayoutData, Node};
use crate::scene::Point;

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

#[test]
fn retargets_a_short_perpendicular_destination_stub_to_the_destination_face_center() {
    let mut layout = LayoutData {
        nodes: vec![Node {
            id: "B".to_string(),
            x: Some(100.0),
            y: Some(110.0),
            width: Some(40.0),
            height: Some(40.0),
            ..Default::default()
        }],
        edges: vec![Edge {
            id: "A_B".to_string(),
            start: Some("A".to_string()),
            end: Some("B".to_string()),
            points: Some(vec![
                p(0.0, 120.0),
                p(70.0, 120.0),
                p(70.0, 100.0),
                p(78.0, 100.0),
            ]),
            ..Default::default()
        }],
        ..Default::default()
    };

    collapse_short_terminal_stub(&mut layout);

    assert_eq!(
        layout.edges[0].points,
        Some(vec![p(0.0, 120.0), p(100.0, 120.0), p(100.0, 130.0)])
    );
}

/// The pass reads its nodes through a map, as upstream does; a node that is not a group and has
/// a measured box is a destination.
#[test]
fn leaves_an_edge_alone_when_its_destination_is_not_measured() {
    let mut layout = LayoutData {
        nodes: vec![Node {
            id: "B".to_string(),
            x: Some(100.0),
            y: Some(110.0),
            ..Default::default()
        }],
        edges: vec![Edge {
            id: "A_B".to_string(),
            start: Some("A".to_string()),
            end: Some("B".to_string()),
            points: Some(vec![
                p(0.0, 120.0),
                p(70.0, 120.0),
                p(70.0, 100.0),
                p(78.0, 100.0),
            ]),
            ..Default::default()
        }],
        ..Default::default()
    };
    let before = layout.edges[0].points.clone();

    collapse_short_terminal_stub(&mut layout);

    assert_eq!(layout.edges[0].points, before);
}
