//! Ports of `__tests__/direction.lr.transform.spec.ts`, `__tests__/lrTransform.anchorFootprint.spec.ts`,
//! and `__tests__/lrTransform.emptyLane.spec.ts`.

use std::collections::HashMap;

use crate::layout::swimlanes::direction::lr_transform::{
    LANE_TITLE_BAND, apply_lr_direction_transform,
};
use crate::layout::swimlanes::helpers::Direction;
use crate::layout::swimlanes::helpers::{
    Coordinates, Graph, OrderedLayers, WriteBackOptions, write_back_to_layout_data,
};
use crate::layout::swimlanes::post_processing::post_process_swimlane_layout;
use crate::render::pipeline::types::{LayoutData, Node};

fn plain(id: &str, x: f64, y: f64) -> Node {
    Node {
        id: id.to_string(),
        x: Some(x),
        y: Some(y),
        ..Default::default()
    }
}

fn make_test_layout() -> LayoutData {
    LayoutData {
        nodes: vec![
            plain("A", 0.0, 0.0),
            plain("B", 0.0, 100.0),
            plain("C", 50.0, 0.0),
        ],
        ..Default::default()
    }
}

fn graph_with_lanes_and_nodes() -> Graph {
    let lane1 = Node {
        id: "lane1".to_string(),
        is_group: Some(true),
        padding: Some(20.0),
        ..Default::default()
    };
    let lane2 = Node {
        id: "lane2".to_string(),
        is_group: Some(true),
        padding: Some(20.0),
        ..Default::default()
    };
    let content = |id: &str, parent: &str| Node {
        id: id.to_string(),
        is_group: Some(false),
        parent_id: Some(parent.to_string()),
        width: Some(80.0),
        height: Some(40.0),
        ..Default::default()
    };
    let layout = LayoutData {
        nodes: vec![
            lane1,
            lane2,
            content("A", "lane1"),
            content("B", "lane1"),
            content("C", "lane2"),
        ],
        ..Default::default()
    };
    let node_by_id: HashMap<String, Node> = layout
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node.clone()))
        .collect();
    Graph {
        nodes: vec!["A".to_string(), "B".to_string(), "C".to_string()],
        edges: Vec::new(),
        layout,
        node_by_id,
    }
}

fn ordered_layers() -> OrderedLayers {
    OrderedLayers {
        layers: vec![
            vec!["A".to_string(), "C".to_string()],
            vec!["B".to_string()],
        ],
    }
}

fn coordinates() -> Coordinates {
    let mut x = HashMap::new();
    x.insert("A".to_string(), -100.0);
    x.insert("B".to_string(), -100.0);
    x.insert("C".to_string(), 100.0);
    let mut y = HashMap::new();
    y.insert("A".to_string(), 0.0);
    y.insert("B".to_string(), 120.0);
    y.insert("C".to_string(), 0.0);
    Coordinates { x, y }
}

fn node<'a>(layout: &'a LayoutData, id: &str) -> &'a Node {
    layout
        .nodes
        .iter()
        .find(|node| node.id == id)
        .expect("node is in the layout")
}

#[test]
fn leaves_layout_unchanged_for_tb_direction() {
    let mut layout = make_test_layout();
    let before: Vec<(String, Option<f64>, Option<f64>)> = layout
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node.x, node.y))
        .collect();

    post_process_swimlane_layout(&mut layout, Some(Direction::Tb));

    for node in &layout.nodes {
        let previous = before
            .iter()
            .find(|entry| entry.0 == node.id)
            .expect("snapshot has the node");
        assert_eq!(node.x, previous.1);
        assert_eq!(node.y, previous.2);
    }
}

#[test]
fn maps_vertical_layering_to_horizontal_progression_for_lr() {
    let mut layout = make_test_layout();

    post_process_swimlane_layout(&mut layout, Some(Direction::Lr));

    let a = node(&layout, "A");
    let b = node(&layout, "B");
    let c = node(&layout, "C");

    assert_eq!(a.y, b.y);
    assert!(b.x.unwrap() > a.x.unwrap());
    assert_eq!(c.x, a.x);
    assert_ne!(c.y, a.y);
}

#[test]
fn maps_vertical_layering_to_reverse_vertical_progression_for_bt() {
    let mut layout = make_test_layout();

    post_process_swimlane_layout(&mut layout, Some(Direction::Bt));

    let a = node(&layout, "A");
    let b = node(&layout, "B");
    let c = node(&layout, "C");

    assert!(b.y.unwrap() < a.y.unwrap());
    assert_eq!(a.x, b.x);
    assert_eq!(c.y, a.y);
}

#[test]
fn maps_vertical_layering_to_reverse_horizontal_progression_for_rl() {
    let mut layout = make_test_layout();

    post_process_swimlane_layout(&mut layout, Some(Direction::Rl));

    let a = node(&layout, "A");
    let b = node(&layout, "B");
    let c = node(&layout, "C");

    assert_eq!(a.y, b.y);
    assert!(b.x.unwrap() < a.x.unwrap());
    assert_eq!(c.x, a.x);
    assert_ne!(c.y, a.y);
}

#[test]
fn positions_lanes_as_horizontal_strips_for_lr() {
    let mut g = graph_with_lanes_and_nodes();
    write_back_to_layout_data(
        &mut g,
        &ordered_layers(),
        &coordinates(),
        Some(WriteBackOptions {
            node_gap: Some(40.0),
            layer_gap: Some(120.0),
        }),
    );

    let lanes: Vec<Node> = g
        .layout
        .nodes
        .iter()
        .filter(|node| node.is_group.unwrap_or(false))
        .cloned()
        .collect();
    assert!(lanes.len() >= 2);

    let mut lanes_before = lanes.clone();
    lanes_before.sort_by(|a, b| a.id.cmp(&b.id));
    let lane1_before = &lanes_before[0];
    let lane2_before = &lanes_before[1];
    assert!((lane1_before.y.unwrap() - lane2_before.y.unwrap()).abs() < 1e-6);
    assert!((lane1_before.x.unwrap() - lane2_before.x.unwrap()).abs() > 1e-6);

    post_process_swimlane_layout(&mut g.layout, Some(Direction::Lr));

    let mut lanes_after: Vec<Node> = g
        .layout
        .nodes
        .iter()
        .filter(|node| node.is_group.unwrap_or(false))
        .cloned()
        .collect();
    lanes_after.sort_by(|a, b| a.id.cmp(&b.id));
    let lane1_after = &lanes_after[0];
    let lane2_after = &lanes_after[1];

    assert!((lane1_after.y.unwrap() - lane2_after.y.unwrap()).abs() > 1e-6);
    let delta_x = (lane1_after.x.unwrap() - lane2_after.x.unwrap()).abs();
    assert!(delta_x < 1e-6);
}

#[test]
fn keeps_lane_children_inside_their_lane_bounds_after_lr() {
    let mut g = graph_with_lanes_and_nodes();
    write_back_to_layout_data(
        &mut g,
        &ordered_layers(),
        &coordinates(),
        Some(WriteBackOptions {
            node_gap: Some(40.0),
            layer_gap: Some(120.0),
        }),
    );

    let lane_ids: Vec<String> = g
        .layout
        .nodes
        .iter()
        .filter(|node| node.is_group.unwrap_or(false))
        .map(|node| node.id.clone())
        .collect();
    assert!(!lane_ids.is_empty());

    post_process_swimlane_layout(&mut g.layout, Some(Direction::Lr));

    for lane_id in &lane_ids {
        let lane = node(&g.layout, lane_id);
        let lane_top = lane.y.unwrap() - lane.height.unwrap() / 2.0;
        let lane_bottom = lane.y.unwrap() + lane.height.unwrap() / 2.0;
        for child in g
            .layout
            .nodes
            .iter()
            .filter(|child| child.parent_id.as_deref() == Some(lane_id.as_str()))
        {
            let child_top = child.y.unwrap() - child.height.unwrap_or(0.0) / 2.0;
            let child_bottom = child.y.unwrap() + child.height.unwrap_or(0.0) / 2.0;
            assert!(child_top >= lane_top - 1e-6);
            assert!(child_bottom <= lane_bottom + 1e-6);
        }
    }
}

#[test]
fn keeps_nested_groups_as_child_clusters_inside_top_level_lr_lanes() {
    let mut layout = LayoutData {
        nodes: vec![
            Node {
                id: "lane1".to_string(),
                is_group: Some(true),
                padding: Some(20.0),
                ..Default::default()
            },
            Node {
                id: "lane2".to_string(),
                is_group: Some(true),
                padding: Some(20.0),
                ..Default::default()
            },
            Node {
                id: "nested".to_string(),
                is_group: Some(true),
                parent_id: Some("lane1".to_string()),
                padding: Some(12.0),
                shape: Some("rect".to_string()),
                ..Default::default()
            },
            Node {
                id: "A".to_string(),
                is_group: Some(false),
                parent_id: Some("nested".to_string()),
                x: Some(0.0),
                y: Some(0.0),
                width: Some(80.0),
                height: Some(40.0),
                ..Default::default()
            },
            Node {
                id: "B".to_string(),
                is_group: Some(false),
                parent_id: Some("lane2".to_string()),
                x: Some(120.0),
                y: Some(0.0),
                width: Some(80.0),
                height: Some(40.0),
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    post_process_swimlane_layout(&mut layout, Some(Direction::Lr));

    let lane1 = node(&layout, "lane1");
    let lane2 = node(&layout, "lane2");
    let nested = node(&layout, "nested");
    let child = node(&layout, "A");

    assert_eq!(nested.parent_id.as_deref(), Some("lane1"));
    assert_eq!(nested.shape.as_deref(), Some("rect"));
    assert!((nested.x.unwrap() - child.x.unwrap()).abs() < 1e-6);
    assert!(nested.width.unwrap() < lane1.width.unwrap());
    assert!((lane1.x.unwrap() - lane2.x.unwrap()).abs() < 1e-6);

    let nested_left = nested.x.unwrap() - nested.width.unwrap() / 2.0;
    let nested_right = nested.x.unwrap() + nested.width.unwrap() / 2.0;
    assert!(child.x.unwrap() - child.width.unwrap() / 2.0 >= nested_left - 1e-6);
    assert!(child.x.unwrap() + child.width.unwrap() / 2.0 <= nested_right + 1e-6);
}

#[test]
fn lays_out_a_lane_whose_groups_reference_each_other_as_parents() {
    let mut layout = LayoutData {
        nodes: vec![
            Node {
                id: "Lane".to_string(),
                is_group: Some(true),
                x: Some(0.0),
                y: Some(0.0),
                width: Some(200.0),
                height: Some(100.0),
                ..Default::default()
            },
            Node {
                id: "a".to_string(),
                is_group: Some(true),
                parent_id: Some("b".to_string()),
                x: Some(0.0),
                y: Some(0.0),
                width: Some(80.0),
                height: Some(40.0),
                ..Default::default()
            },
            Node {
                id: "b".to_string(),
                is_group: Some(true),
                parent_id: Some("a".to_string()),
                x: Some(0.0),
                y: Some(0.0),
                width: Some(80.0),
                height: Some(40.0),
                ..Default::default()
            },
            Node {
                id: "n".to_string(),
                is_group: Some(false),
                parent_id: Some("Lane".to_string()),
                x: Some(0.0),
                y: Some(0.0),
                width: Some(60.0),
                height: Some(30.0),
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    post_process_swimlane_layout(&mut layout, Some(Direction::Lr));
}

const HOST_WIDTH: f64 = 100.0;
const HOST_HEIGHT: f64 = 40.0;
const ANNOTATION_SIZE: f64 = 36.0;
const ANNOTATION_GAP: f64 = 20.0;
const REACH: f64 = ANNOTATION_GAP + ANNOTATION_SIZE;

fn layout_with_annotation(side: &str) -> LayoutData {
    LayoutData {
        nodes: vec![
            Node {
                id: "lane".to_string(),
                is_group: Some(true),
                padding: Some(20.0),
                ..Default::default()
            },
            Node {
                id: "host".to_string(),
                parent_id: Some("lane".to_string()),
                is_group: Some(false),
                x: Some(0.0),
                y: Some(0.0),
                width: Some(HOST_WIDTH),
                height: Some(HOST_HEIGHT),
                ..Default::default()
            },
            Node {
                id: "note".to_string(),
                parent_id: Some("lane".to_string()),
                is_group: Some(false),
                width: Some(ANNOTATION_SIZE),
                height: Some(ANNOTATION_SIZE),
                metadata: Some(serde_json::json!({
                    "anchorTo": { "hostId": "host", "side": side, "gap": ANNOTATION_GAP }
                })),
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}

fn body_bounds(layout: &LayoutData, direction: Direction) -> (f64, f64) {
    let lane = node(layout, "lane");
    let title = lane.group_title_rect.expect("lane has a title band");
    let lane_left = lane.x.unwrap() - lane.width.unwrap() / 2.0;
    let lane_right = lane.x.unwrap() + lane.width.unwrap() / 2.0;
    if direction == Direction::Lr {
        (title.right, lane_right)
    } else {
        (lane_left, title.left)
    }
}

#[test]
fn keeps_room_on_the_left_when_the_node_stands_off_to_the_left() {
    for direction in [Direction::Lr, Direction::Rl] {
        let mut layout = layout_with_annotation("left");
        apply_lr_direction_transform(&mut layout, direction);
        let host = node(&layout, "host");
        let (left, _) = body_bounds(&layout, direction);
        assert!(left <= host.x.unwrap() - HOST_WIDTH / 2.0 - REACH);
    }
}

#[test]
fn keeps_room_on_the_right_when_the_node_stands_off_to_the_right() {
    for direction in [Direction::Lr, Direction::Rl] {
        let mut layout = layout_with_annotation("right");
        apply_lr_direction_transform(&mut layout, direction);
        let host = node(&layout, "host");
        let (_, right) = body_bounds(&layout, direction);
        assert!(right >= host.x.unwrap() + HOST_WIDTH / 2.0 + REACH);
    }
}

#[test]
fn keeps_room_above_when_the_node_stands_off_above() {
    for direction in [Direction::Lr, Direction::Rl] {
        let mut layout = layout_with_annotation("top");
        apply_lr_direction_transform(&mut layout, direction);
        let host = node(&layout, "host");
        let lane = node(&layout, "lane");
        let content_top =
            crate::layout::swimlanes::helpers::metadata_number(lane, "swimlaneContentTop")
                .expect("lane content top is set");
        assert!(content_top <= host.y.unwrap() - HOST_HEIGHT / 2.0 - REACH);
    }
}

#[test]
fn keeps_room_below_when_the_node_stands_off_below() {
    for direction in [Direction::Lr, Direction::Rl] {
        let mut layout = layout_with_annotation("bottom");
        apply_lr_direction_transform(&mut layout, direction);
        let host = node(&layout, "host");
        let lane = node(&layout, "lane");
        assert!(
            lane.y.unwrap() + lane.height.unwrap() / 2.0
                >= host.y.unwrap() + HOST_HEIGHT / 2.0 + REACH
        );
    }
}

#[test]
fn leaves_an_empty_lane_as_the_layout_left_it() {
    let mut layout = LayoutData {
        nodes: vec![
            Node {
                id: "lane".to_string(),
                is_group: Some(true),
                ..Default::default()
            },
            Node {
                id: "empty".to_string(),
                is_group: Some(true),
                x: Some(5.0),
                y: Some(6.0),
                width: Some(0.0),
                height: Some(0.0),
                ..Default::default()
            },
            Node {
                id: "a".to_string(),
                parent_id: Some("lane".to_string()),
                is_group: Some(false),
                x: Some(0.0),
                y: Some(0.0),
                width: Some(80.0),
                height: Some(40.0),
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    apply_lr_direction_transform(&mut layout, Direction::Lr);

    let empty = node(&layout, "empty");
    assert_eq!(empty.x, Some(5.0));
    assert_eq!(empty.y, Some(6.0));
    assert_eq!(empty.width, Some(0.0));
    assert_eq!(empty.height, Some(0.0));
}

#[test]
fn lane_title_band_is_thirty_six() {
    assert_eq!(LANE_TITLE_BAND, 36.0);
}

/// The title band is where the flow starts after the rotation: the smallest node x sits one
/// band in from the layout's left edge.
#[test]
fn lr_moves_the_first_rank_past_the_title_band() {
    let mut layout = make_test_layout();
    apply_lr_direction_transform(&mut layout, Direction::Lr);
    let a = node(&layout, "A");
    assert_eq!(a.x, Some(LANE_TITLE_BAND));
}
