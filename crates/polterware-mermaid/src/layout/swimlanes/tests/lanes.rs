use crate::layout::swimlanes::lanes::{build_lane_model, is_lane_in};
use crate::render::pipeline::types::Node;

fn group(id: &str, parent_id: Option<&str>) -> Node {
    Node {
        id: id.to_string(),
        is_group: Some(true),
        parent_id: parent_id.map(str::to_string),
        ..Default::default()
    }
}

fn leaf(id: &str, parent_id: Option<&str>) -> Node {
    Node {
        id: id.to_string(),
        is_group: Some(false),
        parent_id: parent_id.map(str::to_string),
        ..Default::default()
    }
}

fn pool(id: &str) -> Node {
    Node {
        id: id.to_string(),
        is_group: Some(true),
        metadata: Some(serde_json::json!({ "laneRole": "pool" })),
        ..Default::default()
    }
}

#[test]
fn treats_a_group_with_no_parent_as_a_lane() {
    let model = build_lane_model(&[
        group("sales", None),
        leaf("a", Some("sales")),
        group("ops", None),
        group("nested", Some("ops")),
        leaf("b", Some("nested")),
        leaf("loose", None),
    ]);
    assert!(model.is_lane("sales"));
    assert!(model.is_lane("ops"));
    assert!(!model.is_lane("nested"));
    assert!(!model.is_lane("a"));
    assert!(!model.is_lane("loose"));
    assert_eq!(model.lane_id_of("a").as_deref(), Some("sales"));
    assert_eq!(model.lane_id_of("b").as_deref(), Some("ops"));
    assert_eq!(model.lane_id_of("sales"), None);
    assert_eq!(model.lane_id_of("nested").as_deref(), Some("ops"));
    assert_eq!(model.lane_id_of("loose"), None);
    assert_eq!(model.lane_id_of("missing"), None);
    assert!(!model.is_lane("missing"));
}

#[test]
fn survives_a_malformed_parent_cycle() {
    let cyclic = build_lane_model(&[group("a", Some("b")), group("b", Some("a"))]);
    assert!(!cyclic.is_lane("a"));
    assert_eq!(cyclic.lane_id_of("a"), None);
}

#[test]
fn recognises_a_declared_pool_that_holds_lanes() {
    let model = build_lane_model(&[
        group("customer", None),
        pool("shop"),
        group("sales", Some("shop")),
        group("warehouse", Some("shop")),
        leaf("order", Some("customer")),
        leaf("approve", Some("sales")),
        leaf("pick", Some("warehouse")),
    ]);
    assert!(model.is_pool("shop"));
    assert!(!model.is_lane("shop"));
    assert!(model.has_pools);
    assert!(model.is_lane("sales"));
    assert!(model.is_lane("warehouse"));
    assert_eq!(model.lane_id_of("approve").as_deref(), Some("sales"));
    assert_eq!(model.lane_id_of("pick").as_deref(), Some("warehouse"));
    assert_eq!(model.pool_id_of("approve").as_deref(), Some("shop"));
    assert_eq!(model.pool_id_of("sales").as_deref(), Some("shop"));
    assert!(model.is_lane("customer"));
    assert!(!model.is_pool("customer"));
    assert_eq!(model.lane_id_of("order").as_deref(), Some("customer"));
    assert_eq!(
        model.lanes_by_pool.get("shop"),
        Some(&vec!["sales".to_string(), "warehouse".to_string()])
    );
    assert!(!model.lanes_by_pool.contains_key("customer"));
}

#[test]
fn does_not_infer_a_pool_from_nesting_alone() {
    let nested = build_lane_model(&[
        group("lane1", None),
        group("inner", Some("lane1")),
        leaf("c", Some("inner")),
    ]);
    assert!(!nested.has_pools);
    assert!(!nested.is_lane("inner"));
    assert_eq!(nested.lane_id_of("c").as_deref(), Some("lane1"));
}

#[test]
fn treats_a_declared_pool_with_no_lanes_as_a_lane() {
    let solo = build_lane_model(&[pool("solo"), leaf("task", Some("solo"))]);
    assert!(!solo.is_pool("solo"));
    assert!(solo.is_lane("solo"));
    assert_eq!(solo.lane_id_of("task").as_deref(), Some("solo"));
}

#[test]
fn is_lane_in_matches_the_model() {
    let nodes = [
        group("lane", None),
        group("inner", Some("lane")),
        leaf("a", Some("inner")),
    ];
    let is_lane = is_lane_in(&nodes);
    assert!(is_lane(&nodes[0]));
    assert!(!is_lane(&nodes[1]));
    assert!(!is_lane(&nodes[2]));
}
