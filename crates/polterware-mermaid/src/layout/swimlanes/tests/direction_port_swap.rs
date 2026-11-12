//! Port of `__tests__/direction.portSwap.spec.ts`.

use crate::layout::swimlanes::direction::port_swap::port_swap_to_l_shape;
use crate::render::pipeline::types::{Edge, LayoutData, Node};
use crate::scene::Point;

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

#[test]
fn rewrites_a_non_collinear_hvh_detour_to_a_three_point_l_shape_from_the_source_face() {
    let mut layout = LayoutData {
        nodes: vec![
            Node {
                id: "A".to_string(),
                x: Some(0.0),
                y: Some(0.0),
                width: Some(40.0),
                height: Some(40.0),
                ..Default::default()
            },
            Node {
                id: "B".to_string(),
                x: Some(100.0),
                y: Some(100.0),
                width: Some(40.0),
                height: Some(40.0),
                ..Default::default()
            },
        ],
        edges: vec![Edge {
            id: "A_B".to_string(),
            start: Some("A".to_string()),
            end: Some("B".to_string()),
            points: Some(vec![
                p(20.0, 0.0),
                p(60.0, 0.0),
                p(60.0, 80.0),
                p(100.0, 80.0),
            ]),
            ..Default::default()
        }],
        ..Default::default()
    };

    port_swap_to_l_shape(&mut layout);

    assert_eq!(
        layout.edges[0].points,
        Some(vec![p(0.0, 20.0), p(0.0, 80.0), p(100.0, 80.0)])
    );
}
