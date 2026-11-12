//! Behaviour of the label-anchoring and validation passes, which upstream covers only through
//! the full-layout DDLT suite: the middle-segment placement, the lane reassignment of the
//! label, and the final overlap/crossing scan.

use crate::layout::swimlanes::direction::label_anchoring::anchor_labels_to_polyline;
use crate::layout::swimlanes::direction::validation::{
    ValidationIssueType, collect_validation_issues,
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

#[test]
fn anchors_a_label_on_the_middle_segment_inside_its_lane() {
    let mut layout = LayoutData {
        nodes: vec![
            Node {
                id: "lane".to_string(),
                x: Some(50.0),
                y: Some(50.0),
                width: Some(220.0),
                height: Some(180.0),
                is_group: Some(true),
                ..Default::default()
            },
            node("A", 0.0, 0.0, 40.0, 40.0),
            node("B", 100.0, 100.0, 40.0, 40.0),
            Node {
                id: "edge-label-A-B".to_string(),
                x: Some(0.0),
                y: Some(0.0),
                width: Some(70.0),
                height: Some(10.0),
                is_edge_label: Some(true),
                ..Default::default()
            },
        ],
        edges: vec![Edge {
            id: "A_B".to_string(),
            start: Some("A".to_string()),
            end: Some("B".to_string()),
            label_node_id: Some("edge-label-A-B".to_string()),
            points: Some(vec![
                p(20.0, 0.0),
                p(50.0, 0.0),
                p(50.0, 100.0),
                p(80.0, 100.0),
            ]),
            ..Default::default()
        }],
        ..Default::default()
    };

    anchor_labels_to_polyline(&mut layout);

    let label = layout
        .nodes
        .iter()
        .find(|node| node.id == "edge-label-A-B")
        .expect("the label node is in the layout");
    assert_eq!(label.x, Some(50.0));
    assert_eq!(label.y, Some(50.0));
    assert_eq!(label.parent_id.as_deref(), Some("lane"));
}

#[test]
fn leaves_a_label_where_it_is_when_no_lane_contains_the_route() {
    let mut layout = LayoutData {
        nodes: vec![
            node("A", 0.0, 0.0, 40.0, 40.0),
            node("B", 100.0, 100.0, 40.0, 40.0),
            Node {
                id: "label".to_string(),
                x: Some(-7.0),
                y: Some(-9.0),
                width: Some(70.0),
                height: Some(10.0),
                is_edge_label: Some(true),
                ..Default::default()
            },
        ],
        edges: vec![Edge {
            id: "A_B".to_string(),
            start: Some("A".to_string()),
            end: Some("B".to_string()),
            label_node_id: Some("label".to_string()),
            points: Some(vec![
                p(20.0, 0.0),
                p(50.0, 0.0),
                p(50.0, 100.0),
                p(80.0, 100.0),
            ]),
            ..Default::default()
        }],
        ..Default::default()
    };

    anchor_labels_to_polyline(&mut layout);

    let label = layout
        .nodes
        .iter()
        .find(|node| node.id == "label")
        .expect("the label node is in the layout");
    assert_eq!(label.x, Some(-7.0));
    assert_eq!(label.y, Some(-9.0));
}

#[test]
fn reports_a_segment_that_passes_through_a_node_it_does_not_join() {
    let layout = LayoutData {
        nodes: vec![
            node("A", 0.0, 0.0, 10.0, 10.0),
            node("C", 25.0, 0.0, 10.0, 10.0),
            node("B", 50.0, 0.0, 10.0, 10.0),
        ],
        edges: vec![Edge {
            id: "A_B".to_string(),
            start: Some("A".to_string()),
            end: Some("B".to_string()),
            points: Some(vec![p(5.0, 0.0), p(45.0, 0.0)]),
            ..Default::default()
        }],
        ..Default::default()
    };

    let issues = collect_validation_issues(&layout);

    assert!(issues.iter().any(|issue| {
        issue.issue_type == ValidationIssueType::EdgeNodeOverlap && issue.target_id == "C"
    }));
}

#[test]
fn reports_two_edges_that_cross_between_unrelated_nodes() {
    let layout = LayoutData {
        nodes: vec![
            node("A", 0.0, 0.0, 10.0, 10.0),
            node("B", 100.0, 100.0, 10.0, 10.0),
            node("C", 100.0, 0.0, 10.0, 10.0),
            node("D", 0.0, 100.0, 10.0, 10.0),
        ],
        edges: vec![
            Edge {
                id: "A_B".to_string(),
                start: Some("A".to_string()),
                end: Some("B".to_string()),
                points: Some(vec![p(0.0, 0.0), p(100.0, 100.0)]),
                ..Default::default()
            },
            Edge {
                id: "C_D".to_string(),
                start: Some("C".to_string()),
                end: Some("D".to_string()),
                points: Some(vec![p(100.0, 0.0), p(0.0, 100.0)]),
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    let issues = collect_validation_issues(&layout);

    assert!(issues.iter().any(|issue| {
        issue.issue_type == ValidationIssueType::EdgeEdgeCrossing
            && issue.edge_id == "A_B"
            && issue.target_id == "C_D"
    }));
}
