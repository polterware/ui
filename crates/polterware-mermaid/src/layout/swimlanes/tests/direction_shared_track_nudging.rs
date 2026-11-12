//! Port of `__tests__/direction.sharedTrackNudging.spec.ts`.

use crate::layout::swimlanes::direction::shared_track_nudging::nudge_shared_interior_subpaths;
use crate::render::pipeline::types::{Edge, Node};
use crate::scene::Point;

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn edge(id: &str, points: &[Point]) -> Edge {
    Edge {
        id: id.to_string(),
        points: Some(points.to_vec()),
        ..Default::default()
    }
}

fn with_endpoints(mut edge: Edge, start: &str, end: &str) -> Edge {
    edge.start = Some(start.to_string());
    edge.end = Some(end.to_string());
    edge
}

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

fn vertical_segment_x(points: &[Point], min_overlap: f64) -> Option<f64> {
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() >= min_overlap {
            return Some(a.x);
        }
    }
    None
}

fn vertical_segment_x_overlapping_y(points: &[Point], y1: f64, y2: f64) -> Option<f64> {
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if (a.x - b.x).abs() >= 1e-3 {
            continue;
        }
        let overlap = f64::max(0.0, f64::min(a.y.max(b.y), y2) - f64::max(a.y.min(b.y), y1));
        if overlap > 0.0 {
            return Some(a.x);
        }
    }
    None
}

#[test]
fn separates_near_parallel_interior_rails_with_long_projected_overlap() {
    let mut edges = vec![
        edge(
            "detoured",
            &[p(0.0, 0.0), p(50.0, 0.0), p(50.0, 100.0), p(80.0, 100.0)],
        ),
        edge("nearby", &[p(52.0, 0.0), p(52.0, 100.0)]),
    ];

    nudge_shared_interior_subpaths(&mut edges, &[]);

    let detoured_x = vertical_segment_x(edges[0].points.as_ref().unwrap(), 80.0)
        .expect("a long vertical segment");
    assert!((detoured_x - 52.0).abs() >= 7.0);
}

#[test]
fn separates_the_close_b_and_i_exit_rails_from_fixture_12() {
    let mut edges = vec![
        with_endpoints(
            edge(
                "L_I_exit_0",
                &[
                    p(-113.64374732971191, 955.0),
                    p(220.83124923706055, 955.0),
                    p(220.83124923706055, 1083.5643920898438),
                    p(240.546875, 1083.5643920898438),
                ],
            ),
            "I",
            "exit",
        ),
        edge(
            "L_H_I_0",
            &[p(243.83124923706055, 973.5), p(-113.64374732971191, 973.5)],
        ),
        with_endpoints(
            edge(
                "L_B_exit_0",
                &[
                    p(-131.5906219482422, 192.0),
                    p(-131.5906219482422, 928.0),
                    p(222.77187538146973, 928.0),
                    p(222.77187538146973, 1038.0643920898438),
                    p(258.328125, 1038.0643920898438),
                    p(258.328125, 1058.0643920898438),
                ],
            ),
            "B",
            "exit",
        ),
    ];
    let nodes = vec![
        node("I", -131.5906219482422, 973.5, 35.89374923706055, 45.0),
        node("B", -131.5906219482422, 146.5, 121.3125, 91.0),
        node("exit", 267.21875, 1083.5643920898438, 53.34375, 51.0),
    ];

    nudge_shared_interior_subpaths(&mut edges, &nodes);

    let i_exit_x = vertical_segment_x_overlapping_y(
        edges[0].points.as_ref().unwrap(),
        1016.0,
        1038.0643920898438,
    );
    let b_exit_x = vertical_segment_x_overlapping_y(
        edges[2].points.as_ref().unwrap(),
        1016.0,
        1038.0643920898438,
    );
    let (Some(i_exit_x), Some(b_exit_x)) = (i_exit_x, b_exit_x) else {
        panic!("both exit rails keep a vertical segment");
    };
    assert!((i_exit_x - b_exit_x).abs() >= 7.0);
    assert_eq!(
        edges[0].points.as_ref().unwrap()[0],
        p(-131.5906219482422, 996.0)
    );
}
