//! Ports of `direction/geometry.spec.ts` and `__tests__/direction.geometry.spec.ts`.

use crate::layout::swimlanes::direction::geometry::{RectBounds, RectEntry};
use crate::layout::swimlanes::direction::geometry::{
    ThreeSegmentKind, classify_three_segment_route, collect_real_node_bounds,
    dedupe_consecutive_points, orthogonal_segments_cross, orthogonal_segments_strictly_cross,
    orthogonalize_polyline, same_axis_segments_overlap, segment_conflicts_with_any_edge,
    segment_hits_any_rect, simplify_polyline,
};
use crate::render::pipeline::types::{Edge, Node};
use crate::scene::Point;

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn rect_entry(id: &str, left: f64, right: f64, top: f64, bottom: f64) -> RectEntry {
    RectEntry {
        id: id.to_string(),
        rect: RectBounds {
            left,
            right,
            top,
            bottom,
        },
    }
}

fn edge_with_points(id: &str, points: &[Point]) -> Edge {
    Edge {
        id: id.to_string(),
        points: Some(points.to_vec()),
        ..Default::default()
    }
}

#[test]
fn orthogonal_segments_cross_counts_strict_perpendicular_crossings() {
    assert!(orthogonal_segments_cross(
        p(-10.0, 0.0),
        p(10.0, 0.0),
        p(0.0, -10.0),
        p(0.0, 10.0),
        1e-3,
        1e-6
    ));
}

#[test]
fn orthogonal_segments_cross_counts_t_junctions() {
    assert!(orthogonal_segments_cross(
        p(-10.0, 0.0),
        p(10.0, 0.0),
        p(0.0, 0.0),
        p(0.0, 10.0),
        1e-3,
        1e-6
    ));
}

#[test]
fn orthogonal_segments_cross_ignores_a_shared_endpoint() {
    assert!(!orthogonal_segments_cross(
        p(-10.0, 0.0),
        p(0.0, 0.0),
        p(0.0, 0.0),
        p(0.0, 10.0),
        1e-3,
        1e-6
    ));
}

#[test]
fn orthogonal_segments_cross_ignores_collinear_and_non_orthogonal_segments() {
    assert!(!orthogonal_segments_cross(
        p(-10.0, 0.0),
        p(10.0, 0.0),
        p(-5.0, 0.0),
        p(5.0, 0.0),
        1e-3,
        1e-6
    ));
    assert!(!orthogonal_segments_cross(
        p(-10.0, 0.0),
        p(10.0, 10.0),
        p(0.0, -10.0),
        p(0.0, 10.0),
        1e-3,
        1e-6
    ));
}

#[test]
fn orthogonal_segments_cross_honors_the_caller_epsilon() {
    assert!(!orthogonal_segments_cross(
        p(-10.0, 0.0),
        p(10.0, 0.00001),
        p(0.0, -10.0),
        p(0.0, 10.0),
        1e-6,
        1e-6
    ));
}

#[test]
fn strictly_crossing_counts_only_interior_perpendicular_crossings() {
    assert!(orthogonal_segments_strictly_cross(
        p(-10.0, 0.0),
        p(10.0, 0.0),
        p(0.0, -10.0),
        p(0.0, 10.0),
        1e-3
    ));
    assert!(!orthogonal_segments_strictly_cross(
        p(-10.0, 0.0),
        p(10.0, 0.0),
        p(0.0, 0.0),
        p(0.0, 10.0),
        1e-3
    ));
    assert!(!orthogonal_segments_strictly_cross(
        p(-10.0, 0.0),
        p(0.0, 0.0),
        p(0.0, 0.0),
        p(0.0, 10.0),
        1e-3
    ));
}

#[test]
fn strictly_crossing_ignores_collinear_or_nearly_endpoint_touches() {
    assert!(!orthogonal_segments_strictly_cross(
        p(-10.0, 0.0),
        p(10.0, 0.0),
        p(-5.0, 0.0),
        p(5.0, 0.0),
        1e-3
    ));
    assert!(!orthogonal_segments_strictly_cross(
        p(-10.0, 0.0),
        p(10.0, 0.0),
        p(9.9995, -10.0),
        p(9.9995, 10.0),
        1e-3
    ));
}

#[test]
fn collect_real_node_bounds_keeps_only_real_visible_nodes() {
    let nodes = vec![
        Node {
            id: "A".to_string(),
            x: Some(10.0),
            y: Some(20.0),
            width: Some(40.0),
            height: Some(20.0),
            ..Default::default()
        },
        Node {
            id: "group".to_string(),
            is_group: Some(true),
            x: Some(10.0),
            y: Some(20.0),
            width: Some(40.0),
            height: Some(20.0),
            ..Default::default()
        },
        Node {
            id: "label".to_string(),
            is_edge_label: Some(true),
            x: Some(10.0),
            y: Some(20.0),
            width: Some(40.0),
            height: Some(20.0),
            ..Default::default()
        },
        Node {
            id: "empty".to_string(),
            x: Some(10.0),
            y: Some(20.0),
            width: Some(0.0),
            height: Some(20.0),
            ..Default::default()
        },
    ];

    let bounds = collect_real_node_bounds(&nodes);

    let mut keys: Vec<String> = bounds.node_info_by_id.keys().cloned().collect();
    keys.sort();
    assert_eq!(keys, vec!["A".to_string()]);
    assert_eq!(
        bounds.real_node_rects,
        vec![rect_entry("A", -10.0, 30.0, 10.0, 30.0)]
    );
}

#[test]
fn segment_hits_any_rect_checks_exclusions_and_shrink() {
    let rects = vec![rect_entry("A", 0.0, 10.0, 0.0, 10.0)];

    assert!(segment_hits_any_rect(
        p(-5.0, 5.0),
        p(15.0, 5.0),
        &rects,
        &[],
        0.0
    ));
    assert!(!segment_hits_any_rect(
        p(-5.0, 5.0),
        p(15.0, 5.0),
        &rects,
        &["A"],
        0.0
    ));
    assert!(!segment_hits_any_rect(
        p(-5.0, 0.5),
        p(15.0, 0.5),
        &rects,
        &[],
        1.0
    ));
}

#[test]
fn classify_three_segment_route_reads_hvh_and_vhv() {
    let hvh = classify_three_segment_route(
        Some(&[p(0.0, 0.0), p(10.0, 0.0), p(10.0, 20.0), p(30.0, 20.0)]),
        1e-3,
    );
    assert_eq!(hvh.map(|route| route.kind), Some(ThreeSegmentKind::Hvh));

    let vhv = classify_three_segment_route(
        Some(&[p(0.0, 0.0), p(0.0, 10.0), p(20.0, 10.0), p(20.0, 30.0)]),
        1e-3,
    );
    assert_eq!(vhv.map(|route| route.kind), Some(ThreeSegmentKind::Vhv));

    assert!(
        classify_three_segment_route(
            Some(&[p(0.0, 0.0), p(10.0, 10.0), p(20.0, 10.0), p(20.0, 30.0)]),
            1e-3
        )
        .is_none()
    );
}

#[test]
fn same_axis_segments_overlap_ignores_touching_endpoints() {
    assert!(same_axis_segments_overlap(
        p(0.0, 0.0),
        p(10.0, 0.0),
        p(5.0, 0.0),
        p(15.0, 0.0),
        1e-3
    ));
    assert!(!same_axis_segments_overlap(
        p(0.0, 0.0),
        p(10.0, 0.0),
        p(10.0, 0.0),
        p(15.0, 0.0),
        1e-3
    ));
    assert!(!same_axis_segments_overlap(
        p(0.0, 0.0),
        p(10.0, 0.0),
        p(5.0, 1.0),
        p(15.0, 1.0),
        1e-3
    ));
}

#[test]
fn candidate_segments_conflict_with_other_visible_edges_only() {
    let own = edge_with_points("self", &[p(0.0, 0.0), p(10.0, 0.0)]);
    let crossing = edge_with_points("crossing", &[p(5.0, -5.0), p(5.0, 5.0)]);
    let overlap = edge_with_points("overlap", &[p(20.0, 0.0), p(30.0, 0.0)]);
    let mut layout_only = edge_with_points("layout", &[p(0.0, -5.0), p(0.0, 5.0)]);
    layout_only.is_layout_only = Some(true);

    assert!(segment_conflicts_with_any_edge(
        p(0.0, 0.0),
        p(10.0, 0.0),
        &[own.clone(), crossing],
        Some(0),
        1e-3,
        false
    ));
    assert!(segment_conflicts_with_any_edge(
        p(21.0, 0.0),
        p(25.0, 0.0),
        &[own.clone(), overlap],
        Some(0),
        1e-3,
        false
    ));
    assert!(!segment_conflicts_with_any_edge(
        p(0.0, 0.0),
        p(10.0, 0.0),
        &[own, layout_only],
        Some(0),
        1e-3,
        false
    ));
}

#[test]
fn orthogonalize_inserts_an_l_bend_for_a_diagonal() {
    assert_eq!(
        orthogonalize_polyline(&[p(0.0, 0.0), p(10.0, 10.0)]),
        vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0)]
    );
}

#[test]
fn orthogonalize_preserves_incoming_vertical_orientation() {
    assert_eq!(
        orthogonalize_polyline(&[p(0.0, 0.0), p(0.0, 10.0), p(10.0, 20.0)]),
        vec![p(0.0, 0.0), p(0.0, 10.0), p(0.0, 20.0), p(10.0, 20.0)]
    );
}

#[test]
fn orthogonalize_dedupes_consecutive_coincident_points() {
    assert_eq!(
        orthogonalize_polyline(&[p(0.0, 0.0), p(0.0, 0.0), p(0.0, 10.0)]),
        vec![p(0.0, 0.0), p(0.0, 10.0)]
    );
}

#[test]
fn simplify_removes_out_and_back_spikes() {
    assert_eq!(
        simplify_polyline(&[p(0.0, 0.0), p(10.0, 0.0), p(0.0, 0.0), p(0.0, 10.0)]),
        vec![p(0.0, 0.0), p(0.0, 10.0)]
    );
}

#[test]
fn simplify_removes_collinear_intermediates() {
    assert_eq!(
        simplify_polyline(&[p(0.0, 0.0), p(5.0, 0.0), p(10.0, 0.0), p(10.0, 5.0)]),
        vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 5.0)]
    );
}

#[test]
fn dedupe_keeps_only_distinct_consecutive_points() {
    assert_eq!(
        dedupe_consecutive_points(&[p(0.0, 0.0), p(0.0, 0.0), p(0.0, 10.0)], 1e-3),
        vec![p(0.0, 0.0), p(0.0, 10.0)]
    );
}
