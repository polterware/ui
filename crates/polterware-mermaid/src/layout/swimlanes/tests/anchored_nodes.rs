use std::collections::HashMap;

use crate::layout::swimlanes::anchored_nodes::{
    AnchorPin, AnchorSpace, MAX_ANCHOR_GAP, PinOptions, anchor_footprints, clear_anchored_overlaps,
    clear_anchored_overlaps_with, collect_anchored_ids, default_anchor_side, pin_anchored_nodes,
    read_anchor, resolve_anchor_host_id, square_anchored_edges,
};
use crate::layout::swimlanes::helpers::Direction;
use crate::render::pipeline::types::{Edge, LayoutData, Node};
use crate::scene::Point;

fn node(id: &str) -> Node {
    Node {
        id: id.to_string(),
        ..Default::default()
    }
}

fn anchored(id: &str, anchor_to: serde_json::Value) -> Node {
    Node {
        id: id.to_string(),
        metadata: Some(serde_json::json!({ "anchorTo": anchor_to })),
        ..Default::default()
    }
}

fn host(id: &str) -> Node {
    Node {
        id: id.to_string(),
        x: Some(100.0),
        y: Some(50.0),
        width: Some(100.0),
        height: Some(80.0),
        ..Default::default()
    }
}

fn layout(nodes: Vec<Node>) -> LayoutData {
    LayoutData {
        nodes,
        ..Default::default()
    }
}

fn by_id(nodes: &[Node]) -> HashMap<String, Node> {
    nodes.iter().map(|n| (n.id.clone(), n.clone())).collect()
}

#[test]
fn read_anchor_reads_a_host_id() {
    assert_eq!(
        read_anchor(Some(&anchored("b", serde_json::json!({ "hostId": "t1" }))))
            .unwrap()
            .host_id,
        "t1"
    );
}

#[test]
fn read_anchor_keeps_a_valid_side_and_slot() {
    let anchor = read_anchor(Some(&anchored(
        "b",
        serde_json::json!({ "hostId": "t1", "side": "left", "slot": 2 }),
    )))
    .unwrap();
    assert_eq!(
        anchor.side,
        Some(crate::layout::swimlanes::anchored_nodes::AnchorSide::Left)
    );
    assert_eq!(anchor.slot, Some(2.0));
}

#[test]
fn read_anchor_drops_a_side_that_is_not_a_border() {
    let anchor = read_anchor(Some(&anchored(
        "b",
        serde_json::json!({ "hostId": "t1", "side": "sideways" }),
    )))
    .unwrap();
    assert_eq!(anchor.side, None);
}

#[test]
fn read_anchor_drops_a_non_finite_slot() {
    let anchor = read_anchor(Some(&anchored(
        "b",
        serde_json::json!({ "hostId": "t1", "slot": serde_json::Value::Null }),
    )))
    .unwrap();
    assert_eq!(anchor.slot, None);
}

#[test]
fn read_anchor_keeps_a_gap_and_clamps_a_huge_one() {
    let anchor = read_anchor(Some(&anchored(
        "b",
        serde_json::json!({ "hostId": "t1", "gap": 20 }),
    )))
    .unwrap();
    assert_eq!(anchor.gap, Some(20.0));
    let huge = read_anchor(Some(&anchored(
        "b",
        serde_json::json!({ "hostId": "t1", "gap": 1e308 }),
    )))
    .unwrap();
    assert_eq!(huge.gap, Some(MAX_ANCHOR_GAP));

    let mut data = layout(vec![
        host("t1"),
        Node {
            width: Some(40.0),
            height: Some(60.0),
            ..anchored("b", serde_json::json!({ "hostId": "t1", "gap": 1e308 }))
        },
    ]);
    pin_anchored_nodes(
        &mut data,
        PinOptions {
            space: AnchorSpace::Final,
            direction: Direction::Lr,
        },
    );
    assert!(data.nodes[1].y.unwrap().is_finite());
}

#[test]
fn read_anchor_rejects_bad_gaps_and_shapes() {
    for gap in [0.0, -4.0] {
        let value = serde_json::json!({ "hostId": "t1", "gap": gap });
        assert_eq!(read_anchor(Some(&anchored("b", value))).unwrap().gap, None);
    }
    assert!(read_anchor(Some(&node("b"))).is_none());
    assert!(read_anchor(Some(&anchored("b", serde_json::json!("not-an-object")))).is_none());
    assert!(read_anchor(Some(&anchored("b", serde_json::json!({ "hostId": "" })))).is_none());
    assert!(read_anchor(Some(&anchored("b", serde_json::json!({ "hostId": "b" })))).is_none());
    assert!(read_anchor(None).is_none());
}

#[test]
fn resolve_anchor_host_id_resolves_a_direct_host_and_a_chain() {
    let nodes = [
        host("t1"),
        anchored("b", serde_json::json!({ "hostId": "t1" })),
        anchored("b2", serde_json::json!({ "hostId": "b" })),
    ];
    let map = by_id(&nodes);
    assert_eq!(resolve_anchor_host_id("b", &map).as_deref(), Some("t1"));
    assert_eq!(resolve_anchor_host_id("b2", &map).as_deref(), Some("t1"));
}

#[test]
fn resolve_anchor_host_id_rejects_missing_hosts_cycles_and_unanchored_nodes() {
    let missing = by_id(&[anchored("b", serde_json::json!({ "hostId": "missing" }))]);
    assert_eq!(resolve_anchor_host_id("b", &missing), None);
    let cyclic = by_id(&[
        anchored("x", serde_json::json!({ "hostId": "y" })),
        anchored("y", serde_json::json!({ "hostId": "x" })),
    ]);
    assert_eq!(resolve_anchor_host_id("x", &cyclic), None);
    assert_eq!(resolve_anchor_host_id("t1", &by_id(&[host("t1")])), None);
}

#[test]
fn collect_anchored_ids_only_collects_nodes_the_layout_can_skip() {
    let nodes = [
        host("t1"),
        anchored("ok", serde_json::json!({ "hostId": "t1" })),
        anchored("dangling", serde_json::json!({ "hostId": "missing" })),
        anchored("cycleA", serde_json::json!({ "hostId": "cycleB" })),
        anchored("cycleB", serde_json::json!({ "hostId": "cycleA" })),
        node("plain"),
    ];
    assert_eq!(
        collect_anchored_ids(&nodes),
        std::collections::HashSet::from(["ok".to_string()])
    );
}

#[test]
fn default_anchor_side_is_right_in_canonical_space_and_direction_dependent_in_final() {
    for direction in [Direction::Tb, Direction::Lr, Direction::Bt, Direction::Rl] {
        assert_eq!(
            default_anchor_side(AnchorSpace::Canonical, direction).as_str(),
            "right"
        );
    }
    assert_eq!(
        default_anchor_side(AnchorSpace::Final, Direction::Lr).as_str(),
        "bottom"
    );
    assert_eq!(
        default_anchor_side(AnchorSpace::Final, Direction::Rl).as_str(),
        "bottom"
    );
    assert_eq!(
        default_anchor_side(AnchorSpace::Final, Direction::Tb).as_str(),
        "right"
    );
    assert_eq!(
        default_anchor_side(AnchorSpace::Final, Direction::Bt).as_str(),
        "right"
    );
}

fn final_space() -> PinOptions {
    PinOptions {
        space: AnchorSpace::Final,
        direction: Direction::Tb,
    }
}

#[test]
fn pin_puts_the_centre_on_the_chosen_border() {
    for (side, x, y) in [
        ("bottom", 100.0, 90.0),
        ("top", 100.0, 10.0),
        ("right", 150.0, 50.0),
        ("left", 50.0, 50.0),
    ] {
        let mut data = layout(vec![
            host("t1"),
            Node {
                width: Some(36.0),
                height: Some(36.0),
                ..anchored("b", serde_json::json!({ "hostId": "t1", "side": side }))
            },
        ]);
        pin_anchored_nodes(&mut data, final_space());
        assert_eq!(data.nodes[1].x, Some(x), "side {side}");
        assert_eq!(data.nodes[1].y, Some(y), "side {side}");
    }
}

#[test]
fn pin_centres_a_run_and_orders_it_by_slot() {
    let mut data = layout(vec![
        host("t1"),
        Node {
            width: Some(36.0),
            height: Some(36.0),
            ..anchored(
                "b1",
                serde_json::json!({ "hostId": "t1", "side": "bottom" }),
            )
        },
        Node {
            width: Some(36.0),
            height: Some(36.0),
            ..anchored(
                "b2",
                serde_json::json!({ "hostId": "t1", "side": "bottom" }),
            )
        },
    ]);
    pin_anchored_nodes(&mut data, final_space());
    assert_eq!(data.nodes[1].x, Some(78.0));
    assert_eq!(data.nodes[2].x, Some(122.0));
    assert_eq!(data.nodes[1].y, Some(90.0));
    assert_eq!(data.nodes[2].y, Some(90.0));

    let mut slotted = layout(vec![
        host("t1"),
        Node {
            width: Some(36.0),
            ..anchored(
                "second",
                serde_json::json!({ "hostId": "t1", "side": "bottom", "slot": 2 }),
            )
        },
        Node {
            width: Some(36.0),
            ..anchored(
                "first",
                serde_json::json!({ "hostId": "t1", "side": "bottom", "slot": 1 }),
            )
        },
    ]);
    pin_anchored_nodes(&mut slotted, final_space());
    assert!(slotted.nodes[2].x.unwrap() < slotted.nodes[1].x.unwrap());
}

#[test]
fn pin_is_idempotent_and_reports_the_outward_normal() {
    let mut data = layout(vec![
        host("t1"),
        Node {
            width: Some(36.0),
            height: Some(36.0),
            ..anchored("b", serde_json::json!({ "hostId": "t1", "side": "bottom" }))
        },
    ]);
    let first = pin_anchored_nodes(&mut data, final_space());
    let second = pin_anchored_nodes(&mut data, final_space());
    assert_eq!(second, first);
    assert_eq!(second[0].outward, Point::new(0.0, 1.0));
    assert_eq!(second[0].host_id, "t1");
}

#[test]
fn pin_ignores_an_explicit_side_in_canonical_space() {
    let mut data = layout(vec![
        host("t1"),
        Node {
            width: Some(36.0),
            height: Some(36.0),
            ..anchored("b", serde_json::json!({ "hostId": "t1", "side": "left" }))
        },
    ]);
    pin_anchored_nodes(
        &mut data,
        PinOptions {
            space: AnchorSpace::Canonical,
            direction: Direction::Lr,
        },
    );
    assert_eq!(data.nodes[1].x, Some(150.0));
    assert_eq!(data.nodes[1].y, Some(50.0));
}

#[test]
fn pin_leaves_nodes_alone_without_a_finite_host_or_anchor() {
    let mut data = layout(vec![
        node("t1"),
        Node {
            width: Some(36.0),
            ..anchored("b", serde_json::json!({ "hostId": "t1" }))
        },
    ]);
    assert!(pin_anchored_nodes(&mut data, final_space()).is_empty());
    assert_eq!(data.nodes[1].x, None);

    let mut nothing = layout(vec![
        host("t1"),
        Node {
            width: Some(36.0),
            ..anchored("dangling", serde_json::json!({ "hostId": "missing" }))
        },
        Node {
            width: Some(36.0),
            ..anchored("cycleA", serde_json::json!({ "hostId": "cycleB" }))
        },
        Node {
            width: Some(36.0),
            ..anchored("cycleB", serde_json::json!({ "hostId": "cycleA" }))
        },
    ]);
    assert!(pin_anchored_nodes(&mut nothing, final_space()).is_empty());
}

#[test]
fn pin_does_not_place_a_node_with_a_non_finite_size() {
    for (key, value) in [
        ("width", f64::NAN),
        ("height", f64::NAN),
        ("width", f64::INFINITY),
        ("height", f64::NEG_INFINITY),
    ] {
        let mut bad = Node {
            width: Some(36.0),
            height: Some(36.0),
            ..anchored(
                "bad",
                serde_json::json!({ "hostId": "h", "side": "bottom" }),
            )
        };
        if key == "width" {
            bad.width = Some(value);
        } else {
            bad.height = Some(value);
        }
        let good = Node {
            width: Some(36.0),
            height: Some(36.0),
            ..anchored(
                "good",
                serde_json::json!({ "hostId": "h", "side": "bottom" }),
            )
        };
        let mut data = layout(vec![host("h"), bad, good]);
        let pins = pin_anchored_nodes(&mut data, final_space());
        assert_eq!(
            pins.iter()
                .map(|pin| pin.node_id.as_str())
                .collect::<Vec<_>>(),
            vec!["good"]
        );
        assert_eq!(data.nodes[1].x, None);
        assert_eq!(data.nodes[2].x, Some(100.0));
        assert_eq!(data.nodes[2].y, Some(90.0));
    }
}

#[test]
fn stand_off_clears_the_border_and_accounts_for_the_node_own_size() {
    let mut data = layout(vec![
        host("h"),
        Node {
            width: Some(40.0),
            height: Some(60.0),
            ..anchored("a", serde_json::json!({ "hostId": "h", "gap": 20 }))
        },
    ]);
    pin_anchored_nodes(
        &mut data,
        PinOptions {
            space: AnchorSpace::Final,
            direction: Direction::Lr,
        },
    );
    assert_eq!(data.nodes[1].y, Some(140.0));

    let mut no_gap = layout(vec![
        host("h"),
        Node {
            width: Some(40.0),
            height: Some(60.0),
            ..anchored("a", serde_json::json!({ "hostId": "h" }))
        },
    ]);
    pin_anchored_nodes(
        &mut no_gap,
        PinOptions {
            space: AnchorSpace::Final,
            direction: Direction::Lr,
        },
    );
    assert_eq!(no_gap.nodes[1].y, Some(90.0));
}

#[test]
fn stand_off_gives_each_place_the_width_of_its_own_node() {
    let mut data = layout(vec![
        host("h"),
        Node {
            width: Some(40.0),
            height: Some(60.0),
            ..anchored("narrow", serde_json::json!({ "hostId": "h", "gap": 20 }))
        },
        Node {
            width: Some(200.0),
            height: Some(60.0),
            ..anchored("wide", serde_json::json!({ "hostId": "h", "gap": 20 }))
        },
    ]);
    pin_anchored_nodes(
        &mut data,
        PinOptions {
            space: AnchorSpace::Final,
            direction: Direction::Lr,
        },
    );
    assert_eq!(data.nodes[1].x, Some(-4.0));
    assert_eq!(data.nodes[2].x, Some(124.0));
}

#[test]
fn footprints_report_the_room_a_run_needs() {
    let nodes = [
        host("h"),
        Node {
            width: Some(40.0),
            height: Some(60.0),
            ..anchored("a", serde_json::json!({ "hostId": "h", "gap": 20 }))
        },
    ];
    let footprint = anchor_footprints(&nodes, Some(Direction::Lr));
    let bottom = footprint
        .get("h")
        .unwrap()
        .get(&crate::layout::swimlanes::anchored_nodes::AnchorSide::Bottom)
        .unwrap();
    assert_eq!(
        (bottom.before, bottom.after, bottom.beyond),
        (20.0, 20.0, 80.0)
    );

    let right = anchor_footprints(&nodes, Some(Direction::Tb))
        .get("h")
        .unwrap()
        .get(&crate::layout::swimlanes::anchored_nodes::AnchorSide::Right)
        .copied()
        .unwrap();
    assert_eq!((right.before, right.after, right.beyond), (68.0, 0.0, 60.0));

    let on_border = [
        host("h"),
        Node {
            width: Some(40.0),
            height: Some(60.0),
            ..anchored("a", serde_json::json!({ "hostId": "h" }))
        },
    ];
    assert!(anchor_footprints(&on_border, Some(Direction::Lr)).is_empty());
}

#[test]
fn footprints_reserve_each_side_and_ignore_the_border_runs_reach() {
    let nodes = [
        host("h"),
        Node {
            width: Some(40.0),
            height: Some(60.0),
            ..anchored(
                "a",
                serde_json::json!({ "hostId": "h", "gap": 20, "side": "left" }),
            )
        },
        Node {
            width: Some(120.0),
            height: Some(30.0),
            ..anchored(
                "b",
                serde_json::json!({ "hostId": "h", "gap": 20, "side": "top" }),
            )
        },
    ];
    let reserved = anchor_footprints(&nodes, Some(Direction::Tb));
    let host_map = reserved.get("h").unwrap();
    let left = host_map
        .get(&crate::layout::swimlanes::anchored_nodes::AnchorSide::Left)
        .unwrap();
    assert_eq!((left.before, left.after, left.beyond), (68.0, 0.0, 60.0));
    let top = host_map
        .get(&crate::layout::swimlanes::anchored_nodes::AnchorSide::Top)
        .unwrap();
    assert_eq!((top.before, top.after, top.beyond), (60.0, 60.0, 50.0));

    let tall = [
        host("h"),
        Node {
            width: Some(40.0),
            height: Some(60.0),
            ..anchored("a", serde_json::json!({ "hostId": "h", "gap": 20 }))
        },
        Node {
            width: Some(36.0),
            height: Some(200.0),
            ..anchored("tall", serde_json::json!({ "hostId": "h" }))
        },
    ];
    let beyond = anchor_footprints(&tall, Some(Direction::Lr))
        .get("h")
        .unwrap()
        .get(&crate::layout::swimlanes::anchored_nodes::AnchorSide::Bottom)
        .unwrap()
        .beyond;
    assert_eq!(beyond, 80.0);
}

fn pin_for(id: &str, x: f64, y: f64) -> AnchorPin {
    AnchorPin {
        node_id: id.to_string(),
        host_id: "h".to_string(),
        side: crate::layout::swimlanes::anchored_nodes::AnchorSide::Right,
        x,
        y,
        outward: Point::new(1.0, 0.0),
    }
}

#[test]
fn clearing_steps_outward_until_the_node_is_clear() {
    let other = Node {
        x: Some(190.0),
        y: Some(12.0),
        width: Some(40.0),
        height: Some(40.0),
        ..node("other")
    };
    let standing = Node {
        x: Some(190.0),
        y: Some(12.0),
        width: Some(40.0),
        height: Some(60.0),
        ..anchored("a", serde_json::json!({ "hostId": "h", "gap": 20 }))
    };
    let mut data = layout(vec![host("h"), other, standing]);
    let cleared = clear_anchored_overlaps(&mut data, &[pin_for("a", 190.0, 12.0)]);
    assert_eq!((cleared[0].x, cleared[0].y), (230.0, 12.0));
    assert_eq!(data.nodes[2].x, Some(230.0));
    assert_eq!(data.nodes[2].y, Some(12.0));
}

#[test]
fn clearing_leaves_the_pin_when_nothing_is_in_the_way() {
    let standing = Node {
        x: Some(190.0),
        y: Some(12.0),
        width: Some(40.0),
        height: Some(60.0),
        ..anchored("a", serde_json::json!({ "hostId": "h", "gap": 20 }))
    };
    let mut data = layout(vec![host("h"), standing]);
    let pin = pin_for("a", 190.0, 12.0);
    let cleared = clear_anchored_overlaps(&mut data, &[pin]);
    assert_eq!((cleared[0].x, cleared[0].y), (190.0, 12.0));
    assert_eq!(data.nodes[1].x, Some(190.0));
}

#[test]
fn clearing_does_not_clear_the_host_or_a_lane_it_would_leave() {
    let standing = Node {
        x: Some(120.0),
        y: Some(50.0),
        width: Some(40.0),
        height: Some(60.0),
        ..anchored("a", serde_json::json!({ "hostId": "h", "gap": 20 }))
    };
    let mut data = layout(vec![host("h"), standing]);
    let cleared = clear_anchored_overlaps(&mut data, &[pin_for("a", 120.0, 50.0)]);
    assert_eq!((cleared[0].x, cleared[0].y), (120.0, 50.0));
}

#[test]
fn clearing_stays_inside_a_lane_and_keeps_stepping_when_there_is_room() {
    let lane = |right: f64| {
        let mut node = Node {
            is_group: Some(true),
            x: Some(right - 200.0),
            y: Some(50.0),
            width: Some(400.0),
            height: Some(200.0),
            ..node("lane")
        };
        node.metadata = Some(serde_json::json!({ "laneRole": "lane" }));
        node
    };
    let other = Node {
        x: Some(190.0),
        y: Some(12.0),
        width: Some(40.0),
        height: Some(40.0),
        ..node("other")
    };
    let standing = Node {
        x: Some(190.0),
        y: Some(12.0),
        width: Some(40.0),
        height: Some(60.0),
        ..anchored("a", serde_json::json!({ "hostId": "h", "gap": 20 }))
    };
    let parented = Node {
        parent_id: Some("lane".to_string()),
        ..host("h")
    };
    let mut tight = layout(vec![
        lane(230.0),
        parented.clone(),
        other.clone(),
        standing.clone(),
    ]);
    let pin = pin_for("a", 190.0, 12.0);
    let cleared = clear_anchored_overlaps(&mut tight, &[pin]);
    assert_eq!((cleared[0].x, cleared[0].y), (190.0, 12.0));
    assert_eq!(tight.nodes[3].x, Some(190.0));

    let mut roomy = layout(vec![lane(400.0), parented, other, standing]);
    let cleared = clear_anchored_overlaps(&mut roomy, &[pin_for("a", 190.0, 12.0)]);
    assert_eq!(cleared[0].x, 230.0);
}

#[test]
fn clearing_passes_border_nodes_through_and_still_clears_later_ones() {
    let on_border = Node {
        x: Some(200.0),
        y: Some(50.0),
        width: Some(36.0),
        height: Some(36.0),
        ..anchored("e", serde_json::json!({ "hostId": "h" }))
    };
    let standing = Node {
        x: Some(200.0),
        y: Some(50.0),
        width: Some(40.0),
        height: Some(60.0),
        ..anchored("b", serde_json::json!({ "hostId": "h", "gap": 20 }))
    };
    let mut data = layout(vec![host("h"), on_border, standing]);
    let edge = pin_for("e", 200.0, 50.0);
    let cleared = clear_anchored_overlaps(&mut data, &[edge, pin_for("b", 200.0, 50.0)]);
    assert_eq!((cleared[0].x, cleared[0].y), (200.0, 50.0));
    assert_eq!(data.nodes[1].x, Some(200.0));
    assert_eq!(cleared[1].x, 240.0);
}

#[test]
fn clearing_gives_up_after_twenty_four_steps() {
    let wall = Node {
        x: Some(200.0),
        y: Some(200.0),
        width: Some(10_000.0),
        height: Some(10_000.0),
        ..node("wall")
    };
    let standing = Node {
        x: Some(200.0),
        y: Some(200.0),
        width: Some(40.0),
        height: Some(60.0),
        ..anchored("a", serde_json::json!({ "hostId": "h", "gap": 20 }))
    };
    let mut data = layout(vec![host("h"), wall, standing]);
    let pin = AnchorPin {
        x: 200.0,
        y: 200.0,
        ..pin_for("a", 200.0, 200.0)
    };
    let cleared = clear_anchored_overlaps(&mut data, &[pin]);
    assert_eq!(data.nodes[2].x, Some(200.0 + 24.0 * 8.0));
    assert_eq!(cleared[0].x, 200.0 + 24.0 * 8.0);
    assert_eq!(cleared[0].y, 200.0);
}

#[test]
fn clearing_uses_a_custom_lane_resolver() {
    let nodes = vec![
        Node {
            is_group: Some(true),
            x: Some(200.0),
            y: Some(200.0),
            width: Some(100.0),
            height: Some(400.0),
            ..node("lane")
        },
        Node {
            parent_id: Some("lane".to_string()),
            ..host("h")
        },
        Node {
            x: Some(200.0),
            y: Some(200.0),
            width: Some(60.0),
            height: Some(60.0),
            ..node("wall")
        },
        Node {
            x: Some(200.0),
            y: Some(200.0),
            width: Some(40.0),
            height: Some(60.0),
            ..anchored("a", serde_json::json!({ "hostId": "h", "gap": 20 }))
        },
    ];
    let mut data = layout(nodes.clone());
    let pin = pin_for("a", 200.0, 200.0);
    let cleared = clear_anchored_overlaps_with(&mut data, &[pin], |group| group.id == "lane");
    assert_eq!(cleared[0].x, 200.0);

    let mut plain = layout(nodes);
    let cleared = clear_anchored_overlaps(&mut plain, &[pin_for("a", 200.0, 200.0)]);
    assert!(cleared[0].x > 200.0);
}

#[test]
fn lane_model_resolver_holds_nodes_inside_a_plain_lane() {
    let nodes = vec![
        Node {
            is_group: Some(true),
            x: Some(0.0),
            y: Some(0.0),
            width: Some(600.0),
            height: Some(200.0),
            ..node("lane")
        },
        Node {
            is_group: Some(true),
            parent_id: Some("lane".to_string()),
            x: Some(0.0),
            y: Some(0.0),
            width: Some(100.0),
            height: Some(200.0),
            ..node("inner")
        },
        Node {
            parent_id: Some("inner".to_string()),
            width: Some(40.0),
            height: Some(40.0),
            ..host("host")
        },
        Node {
            x: Some(60.0),
            y: Some(0.0),
            width: Some(30.0),
            height: Some(40.0),
            ..node("wall")
        },
        Node {
            x: Some(40.0),
            y: Some(0.0),
            width: Some(20.0),
            height: Some(20.0),
            parent_id: Some("inner".to_string()),
            ..anchored("note", serde_json::json!({ "hostId": "host", "gap": 10 }))
        },
    ];
    let mut data = layout(nodes.clone());
    let pin = pin_for("note", 40.0, 0.0);
    let cleared = clear_anchored_overlaps(&mut data, std::slice::from_ref(&pin))
        .into_iter()
        .next()
        .unwrap();
    assert!(cleared.x > 40.0);
}

fn stub_layout(extra: Node, edge: Edge) -> LayoutData {
    LayoutData {
        nodes: vec![host("h"), extra],
        edges: vec![edge],
        ..Default::default()
    }
}

fn edge_to_host(start: &str) -> Edge {
    Edge {
        id: "e".to_string(),
        start: Some(start.to_string()),
        end: Some("h".to_string()),
        points: Some(vec![Point::new(-1.0, -1.0)]),
        ..Default::default()
    }
}

#[test]
fn square_anchored_edges_replaces_the_route_with_axis_aligned_legs() {
    let mut data = stub_layout(
        Node {
            width: Some(40.0),
            height: Some(60.0),
            ..anchored("a", serde_json::json!({ "hostId": "h", "gap": 20 }))
        },
        edge_to_host("a"),
    );
    let pins = pin_anchored_nodes(
        &mut data,
        PinOptions {
            space: AnchorSpace::Final,
            direction: Direction::Lr,
        },
    );
    square_anchored_edges(&mut data, &pins);
    let points = data.edges[0].points.as_ref().unwrap();
    assert!(points.len() >= 2);
    for pair in points.windows(2) {
        assert!(
            (pair[0].x - pair[1].x).abs() < 1.0 || (pair[0].y - pair[1].y).abs() < 1.0,
            "every leg runs along one axis"
        );
    }
    assert_eq!(points.last().map(|point| point.x), Some(100.0));
}

#[test]
fn square_anchored_edges_leaves_foreign_and_missing_routes_alone() {
    let mut data = layout(vec![
        host("h"),
        host("other"),
        Node {
            width: Some(40.0),
            height: Some(60.0),
            ..anchored("a", serde_json::json!({ "hostId": "h", "gap": 20 }))
        },
    ]);
    data.edges.push(crate::render::pipeline::types::Edge {
        id: "e".to_string(),
        start: Some("a".to_string()),
        end: Some("other".to_string()),
        points: Some(vec![Point::new(1.0, 2.0)]),
        ..Default::default()
    });
    let pins = pin_anchored_nodes(
        &mut data,
        PinOptions {
            space: AnchorSpace::Final,
            direction: Direction::Lr,
        },
    );
    square_anchored_edges(&mut data, &pins);
    assert_eq!(data.edges[0].points, Some(vec![Point::new(1.0, 2.0)]));
}

#[test]
fn stand_off_places_a_node_on_each_border_and_reports_the_way_out() {
    for (side, x, y, outward) in [
        ("top", 100.0, -40.0, Point::new(0.0, -1.0)),
        ("bottom", 100.0, 140.0, Point::new(0.0, 1.0)),
        ("left", 10.0, 12.0, Point::new(-1.0, 0.0)),
        ("right", 190.0, 12.0, Point::new(1.0, 0.0)),
    ] {
        let mut data = layout(vec![
            host("h"),
            Node {
                width: Some(40.0),
                height: Some(60.0),
                ..anchored(
                    "a",
                    serde_json::json!({ "hostId": "h", "side": side, "gap": 20 }),
                )
            },
        ]);
        let pins = pin_anchored_nodes(&mut data, final_space());
        assert_eq!(data.nodes[1].x, Some(x), "side {side}");
        assert_eq!(data.nodes[1].y, Some(y), "side {side}");
        assert_eq!(pins[0].outward, outward, "side {side}");
    }
}

#[test]
fn pin_requires_a_finite_host_box_and_a_finite_node_size() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for key in 0..4 {
            let mut bad_host = host("h");
            match key {
                0 => bad_host.x = Some(value),
                1 => bad_host.y = Some(value),
                2 => bad_host.width = Some(value),
                _ => bad_host.height = Some(value),
            }
            let mut data = layout(vec![
                bad_host,
                Node {
                    width: Some(36.0),
                    height: Some(36.0),
                    ..anchored("a", serde_json::json!({ "hostId": "h", "side": "bottom" }))
                },
            ]);
            assert!(
                pin_anchored_nodes(&mut data, final_space()).is_empty(),
                "host measure {value}"
            );
            assert_eq!(data.nodes[1].x, None);
        }
    }
}

fn route(
    side: &str,
    at: (f64, f64),
    face: Option<&str>,
    host_box: (f64, f64, f64, f64),
) -> Vec<Point> {
    let (hx, hy, hw, hh) = host_box;
    let mut a = Node {
        width: Some(40.0),
        height: Some(60.0),
        x: Some(at.0),
        y: Some(at.1),
        ..node("a")
    };
    if let Some(face) = face {
        a.metadata = Some(serde_json::json!({ "attachFace": face }));
    }
    let mut data = layout(vec![
        Node {
            x: Some(hx),
            y: Some(hy),
            width: Some(hw),
            height: Some(hh),
            ..node("h")
        },
        a,
    ]);
    data.edges.push(crate::render::pipeline::types::Edge {
        id: "e".to_string(),
        start: Some("a".to_string()),
        end: Some("h".to_string()),
        points: Some(vec![Point::new(-1.0, -1.0)]),
        ..Default::default()
    });
    let (outward, pin_side) = match side {
        "top" => (
            Point::new(0.0, -1.0),
            crate::layout::swimlanes::anchored_nodes::AnchorSide::Top,
        ),
        "bottom" => (
            Point::new(0.0, 1.0),
            crate::layout::swimlanes::anchored_nodes::AnchorSide::Bottom,
        ),
        "left" => (
            Point::new(-1.0, 0.0),
            crate::layout::swimlanes::anchored_nodes::AnchorSide::Left,
        ),
        _ => (
            Point::new(1.0, 0.0),
            crate::layout::swimlanes::anchored_nodes::AnchorSide::Right,
        ),
    };
    let pin = AnchorPin {
        node_id: "a".to_string(),
        host_id: "h".to_string(),
        side: pin_side,
        x: 0.0,
        y: 0.0,
        outward,
    };
    square_anchored_edges(&mut data, &[pin]);
    data.edges[0].points.clone().unwrap()
}

fn pts(pairs: &[(f64, f64)]) -> Vec<Point> {
    pairs.iter().map(|(x, y)| Point::new(*x, *y)).collect()
}

#[test]
fn squares_the_line_to_the_nearest_landing_on_the_host_border() {
    let host_box = (100.0, 50.0, 100.0, 80.0);
    assert_eq!(
        route("bottom", (200.0, 140.0), None, host_box),
        pts(&[(200.0, 140.0), (200.0, 95.0), (142.0, 95.0), (142.0, 50.0)])
    );
    assert_eq!(
        route("top", (200.0, -40.0), None, host_box),
        pts(&[(200.0, -40.0), (200.0, 5.0), (142.0, 5.0), (142.0, 50.0)])
    );
    assert_eq!(
        route("right", (190.0, 150.0), None, host_box),
        pts(&[(190.0, 150.0), (145.0, 150.0), (145.0, 82.0), (100.0, 82.0)])
    );
    assert_eq!(
        route("left", (10.0, 150.0), None, host_box),
        pts(&[(10.0, 150.0), (55.0, 150.0), (55.0, 82.0), (100.0, 82.0)])
    );
    assert_eq!(
        route("bottom", (100.0, 140.0), None, host_box),
        pts(&[(100.0, 140.0), (100.0, 50.0)])
    );
}

#[test]
fn leaves_a_node_by_its_attach_face_before_turning() {
    let host_box = (100.0, 50.0, 100.0, 80.0);
    assert_eq!(
        route("bottom", (100.0, 140.0), Some("right"), host_box),
        pts(&[(100.0, 140.0), (136.0, 140.0), (136.0, 50.0)])
    );
    assert_eq!(
        route("bottom", (100.0, 140.0), Some("left"), host_box),
        pts(&[(100.0, 140.0), (64.0, 140.0), (64.0, 50.0)])
    );
    assert_eq!(
        route("right", (190.0, 150.0), Some("top"), host_box),
        pts(&[
            (190.0, 150.0),
            (190.0, 104.0),
            (145.0, 104.0),
            (145.0, 82.0),
            (100.0, 82.0)
        ])
    );
}
