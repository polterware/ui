use std::collections::HashMap;

use crate::layout::swimlanes::helpers::{
    Coordinates, DEFAULT_SWIMLANE_ID, Graph, OrderedLayers, SharedMember, WriteBackOptions,
    prepare_layout_for_swimlanes, write_back_to_layout_data,
};
use crate::render::pipeline::types::{Edge, LayoutData, Node};

fn node(id: &str, is_group: bool) -> Node {
    Node {
        id: id.to_string(),
        is_group: Some(is_group),
        ..Default::default()
    }
}

fn names(layout: &LayoutData) -> Vec<String> {
    layout.nodes.iter().map(|node| node.id.clone()).collect()
}

#[test]
fn marks_group_nodes_with_swimlane_cluster_shape() {
    let mut layout = LayoutData {
        nodes: vec![node("g1", true), node("n1", false)],
        edges: Vec::new(),
        ..Default::default()
    };
    prepare_layout_for_swimlanes(&mut layout);
    assert_eq!(layout.nodes[0].shape.as_deref(), Some("swimlane"));
    assert_eq!(layout.nodes[1].shape, None);
}

#[test]
fn assigns_ungrouped_content_nodes_to_a_synthetic_default_lane() {
    let mut layout = LayoutData {
        nodes: vec![
            node("lane1", true),
            Node {
                id: "grouped".to_string(),
                is_group: Some(false),
                parent_id: Some("lane1".to_string()),
                ..Default::default()
            },
            node("loose", false),
        ],
        edges: Vec::new(),
        ..Default::default()
    };
    prepare_layout_for_swimlanes(&mut layout);
    let default_lane = layout
        .nodes
        .iter()
        .find(|node| node.id == DEFAULT_SWIMLANE_ID)
        .expect("default lane");
    assert!(default_lane.is_group.unwrap_or(false));
    assert_eq!(default_lane.shape.as_deref(), Some("swimlane"));
    assert_eq!(
        layout
            .nodes
            .iter()
            .find(|node| node.id == "loose")
            .unwrap()
            .parent_id
            .as_deref(),
        Some(DEFAULT_SWIMLANE_ID)
    );
    assert_eq!(
        layout
            .nodes
            .iter()
            .find(|node| node.id == "grouped")
            .unwrap()
            .parent_id
            .as_deref(),
        Some("lane1")
    );
}

#[test]
fn gives_the_synthetic_default_lane_the_diagram_look_and_a_free_color_slot() {
    let mut layout = LayoutData {
        nodes: vec![
            Node {
                color_index: Some(0),
                look: Some("handDrawn".to_string()),
                ..node("lane1", true)
            },
            Node {
                parent_id: Some("lane1".to_string()),
                color_index: Some(1),
                ..node("nested", true)
            },
            Node {
                color_index: Some(2),
                look: Some("handDrawn".to_string()),
                ..node("lane2", true)
            },
            node("loose", false),
        ],
        edges: Vec::new(),
        ..Default::default()
    };
    layout.config.look = "handDrawn".to_string();
    prepare_layout_for_swimlanes(&mut layout);
    let default_lane = layout
        .nodes
        .iter()
        .find(|node| node.id == DEFAULT_SWIMLANE_ID)
        .expect("default lane");
    assert_eq!(default_lane.look.as_deref(), Some("handDrawn"));
    assert_eq!(default_lane.color_index, Some(3));
}

#[test]
fn starts_the_default_lane_at_slot_zero_when_no_container_has_a_color() {
    let mut layout = LayoutData {
        nodes: vec![node("loose", false)],
        edges: Vec::new(),
        ..Default::default()
    };
    layout.config.look = String::new();
    prepare_layout_for_swimlanes(&mut layout);
    let default_lane = layout
        .nodes
        .iter()
        .find(|node| node.id == DEFAULT_SWIMLANE_ID)
        .expect("default lane");
    assert_eq!(default_lane.color_index, Some(0));
    assert_eq!(default_lane.look, None);
}

#[test]
fn only_treats_top_level_groups_as_swimlane_lanes() {
    let mut layout = LayoutData {
        nodes: vec![
            node("lane1", true),
            Node {
                parent_id: Some("lane1".to_string()),
                shape: Some("rect".to_string()),
                ..node("nested", true)
            },
            Node {
                parent_id: Some("nested".to_string()),
                ..node("child", false)
            },
        ],
        edges: Vec::new(),
        ..Default::default()
    };
    prepare_layout_for_swimlanes(&mut layout);
    assert_eq!(
        layout
            .nodes
            .iter()
            .find(|node| node.id == "lane1")
            .unwrap()
            .shape
            .as_deref(),
        Some("swimlane")
    );
    assert_eq!(
        layout
            .nodes
            .iter()
            .find(|node| node.id == "nested")
            .unwrap()
            .shape
            .as_deref(),
        Some("rect")
    );
}

fn shared_member() -> SharedMember {
    SharedMember {
        node: "a".to_string(),
        kept_by: "lane1".to_string(),
        dropped_from: "phase1".to_string(),
    }
}

fn with_shared_member(mut layout: LayoutData, member: SharedMember) -> LayoutData {
    layout.other = Some(serde_json::json!({
        "droppedSubGraphMembers": [{
            "node": member.node,
            "keptBy": member.kept_by,
            "droppedFrom": member.dropped_from,
        }],
    }));
    layout
}

#[test]
fn removes_the_lane_a_shared_node_emptied() {
    let mut layout = with_shared_member(
        LayoutData {
            nodes: vec![
                node("lane1", true),
                node("phase1", true),
                Node {
                    parent_id: Some("lane1".to_string()),
                    ..node("a", false)
                },
            ],
            edges: Vec::new(),
            ..Default::default()
        },
        shared_member(),
    );
    prepare_layout_for_swimlanes(&mut layout);
    assert_eq!(names(&layout), vec!["lane1", "a"]);
}

#[test]
fn keeps_a_lane_the_author_left_empty() {
    let mut layout = LayoutData {
        nodes: vec![
            node("lane1", true),
            node("placeholder", true),
            node("a", false),
        ],
        edges: Vec::new(),
        ..Default::default()
    };
    layout.nodes[2].parent_id = Some("lane1".to_string());
    prepare_layout_for_swimlanes(&mut layout);
    assert_eq!(names(&layout), vec!["lane1", "placeholder", "a"]);
}

#[test]
fn keeps_a_lane_that_still_has_members_after_losing_one() {
    let mut layout = with_shared_member(
        LayoutData {
            nodes: vec![
                node("lane1", true),
                node("phase1", true),
                Node {
                    parent_id: Some("lane1".to_string()),
                    ..node("a", false)
                },
                Node {
                    parent_id: Some("phase1".to_string()),
                    ..node("b", false)
                },
            ],
            edges: Vec::new(),
            ..Default::default()
        },
        shared_member(),
    );
    prepare_layout_for_swimlanes(&mut layout);
    assert_eq!(names(&layout), vec!["lane1", "phase1", "a", "b"]);
}

#[test]
fn keeps_an_emptied_lane_that_an_edge_points_at() {
    let mut layout = with_shared_member(
        LayoutData {
            nodes: vec![
                node("lane1", true),
                node("phase1", true),
                Node {
                    parent_id: Some("lane1".to_string()),
                    ..node("a", false)
                },
            ],
            edges: vec![Edge {
                id: "e1".to_string(),
                start: Some("a".to_string()),
                end: Some("phase1".to_string()),
                ..Default::default()
            }],
            ..Default::default()
        },
        shared_member(),
    );
    prepare_layout_for_swimlanes(&mut layout);
    assert!(names(&layout).contains(&"phase1".to_string()));
}

#[test]
fn resolves_a_node_kept_by_a_nested_subgraph_to_the_lane_around_it() {
    let mut layout = with_shared_member(
        LayoutData {
            nodes: vec![
                node("lane1", true),
                Node {
                    parent_id: Some("lane1".to_string()),
                    ..node("inner", true)
                },
                Node {
                    parent_id: Some("inner".to_string()),
                    ..node("a", false)
                },
                node("lane2", true),
            ],
            edges: Vec::new(),
            ..Default::default()
        },
        SharedMember {
            node: "a".to_string(),
            kept_by: "inner".to_string(),
            dropped_from: "lane2".to_string(),
        },
    );
    prepare_layout_for_swimlanes(&mut layout);
    assert_eq!(names(&layout), vec!["lane1", "inner", "a"]);
}

fn graph_with(nodes: Vec<Node>, node_ids: &[&str], edges: Vec<Edge>) -> Graph {
    let layout = LayoutData {
        nodes: nodes.clone(),
        edges: edges.clone(),
        ..Default::default()
    };
    let node_by_id: HashMap<String, Node> = nodes
        .iter()
        .map(|node| (node.id.clone(), node.clone()))
        .collect();
    Graph {
        nodes: node_ids.iter().map(|id| (*id).to_string()).collect(),
        edges: Vec::new(),
        layout,
        node_by_id,
    }
}

fn coords(entries: &[(&str, f64, f64)]) -> Coordinates {
    let mut x = HashMap::new();
    let mut y = HashMap::new();
    for (id, cx, cy) in entries {
        x.insert((*id).to_string(), *cx);
        y.insert((*id).to_string(), *cy);
    }
    Coordinates { x, y }
}

#[test]
fn makes_all_top_level_lanes_share_height_and_vertical_center() {
    let mut lane1 = node("lane1", true);
    lane1.padding = Some(20.0);
    let mut lane2 = node("lane2", true);
    lane2.padding = Some(20.0);
    let a = Node {
        parent_id: Some("lane1".to_string()),
        width: Some(80.0),
        height: Some(40.0),
        ..node("A", false)
    };
    let b = Node {
        parent_id: Some("lane1".to_string()),
        width: Some(80.0),
        height: Some(40.0),
        ..node("B", false)
    };
    let c = Node {
        parent_id: Some("lane2".to_string()),
        width: Some(80.0),
        height: Some(40.0),
        ..node("C", false)
    };
    let mut g = graph_with(vec![lane1, lane2, a, b, c], &["A", "B", "C"], Vec::new());
    let ordered = OrderedLayers {
        layers: vec![
            vec!["A".to_string(), "C".to_string()],
            vec!["B".to_string()],
        ],
    };
    let coordinates = coords(&[("A", -100.0, 0.0), ("C", 100.0, 0.0), ("B", -100.0, 120.0)]);
    write_back_to_layout_data(
        &mut g,
        &ordered,
        &coordinates,
        Some(WriteBackOptions {
            node_gap: Some(40.0),
            layer_gap: Some(120.0),
        }),
    );
    let lane1 = g
        .layout
        .nodes
        .iter()
        .find(|node| node.id == "lane1")
        .unwrap();
    let lane2 = g
        .layout
        .nodes
        .iter()
        .find(|node| node.id == "lane2")
        .unwrap();
    assert!(lane1.height.unwrap() > 0.0);
    assert!(lane2.height.unwrap() > 0.0);
    assert!((lane1.height.unwrap() - lane2.height.unwrap()).abs() < 1e-6);
    assert!((lane1.y.unwrap() - lane2.y.unwrap()).abs() < 1e-6);
}

#[test]
fn keeps_a_minimum_horizontal_padding_between_a_top_lane_and_its_children() {
    let mut lane = node("lane", true);
    lane.padding = Some(8.0);
    let child = Node {
        parent_id: Some("lane".to_string()),
        width: Some(100.0),
        height: Some(40.0),
        ..node("child", false)
    };
    let mut g = graph_with(vec![lane, child], &["child"], Vec::new());
    write_back_to_layout_data(
        &mut g,
        &OrderedLayers {
            layers: vec![vec!["child".to_string()]],
        },
        &coords(&[("child", 0.0, 0.0)]),
        Some(WriteBackOptions {
            node_gap: Some(40.0),
            layer_gap: Some(120.0),
        }),
    );
    let lane = g
        .layout
        .nodes
        .iter()
        .find(|node| node.id == "lane")
        .unwrap();
    assert!(lane.width.unwrap() >= 140.0);
}

#[test]
fn sizes_a_group_from_its_content_not_an_anchored_nodes_stray_coordinate() {
    let mut lane = node("lane", true);
    lane.padding = Some(20.0);
    let a = Node {
        parent_id: Some("lane".to_string()),
        width: Some(80.0),
        height: Some(40.0),
        ..node("A", false)
    };
    let stray = Node {
        parent_id: Some("lane".to_string()),
        width: Some(36.0),
        height: Some(36.0),
        x: Some(4000.0),
        y: Some(4000.0),
        metadata: Some(serde_json::json!({ "anchorTo": { "hostId": "A" } })),
        ..node("stray", false)
    };
    let mut g = graph_with(vec![lane, a, stray], &["A"], Vec::new());
    write_back_to_layout_data(
        &mut g,
        &OrderedLayers {
            layers: vec![vec!["A".to_string()]],
        },
        &coords(&[("A", 0.0, 0.0)]),
        Some(WriteBackOptions {
            node_gap: Some(40.0),
            layer_gap: Some(120.0),
        }),
    );
    let lane = g
        .layout
        .nodes
        .iter()
        .find(|node| node.id == "lane")
        .unwrap();
    assert!(lane.width.unwrap() < 1000.0);
    assert!(lane.height.unwrap() < 1000.0);
}
