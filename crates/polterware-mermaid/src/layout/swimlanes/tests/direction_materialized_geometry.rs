//! Ports of `__tests__/direction.materializedGeometry.spec.ts` and the unit cases of
//! `__tests__/laneTitleBands.spec.ts` and `__tests__/laneTitleBands.pooled.spec.ts`.

use crate::layout::swimlanes::direction::geometry::{
    orthogonal_segments_for_points, orthogonal_segments_strictly_cross,
};
use crate::layout::swimlanes::direction::materialized_geometry::{
    collapse_redundant_rectangular_doglegs, lift_obstacle_hugging_same_side_rails,
    lift_top_lane_title_bands_above_rails, reassign_crossing_external_rail_channels,
    separate_shared_rendered_terminal_lanes, shift_left_lane_title_bands_left_of_rails,
    shortcut_redundant_orthogonal_jogs, swap_destination_terminal_tails_to_reduce_crossings,
};
use crate::layout::swimlanes::helpers::set_metadata_string;
use crate::render::pipeline::types::{Edge, GroupTitleRect, Node};
use crate::scene::Point;

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
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

fn node(id: &str, x: f64, y: f64, width: f64, height: f64) -> Node {
    Node {
        id: id.to_string(),
        is_group: Some(false),
        x: Some(x),
        y: Some(y),
        width: Some(width),
        height: Some(height),
        ..Default::default()
    }
}

fn group(id: &str, x: f64, y: f64, width: f64, height: f64, direction: &str) -> Node {
    let mut group = Node {
        id: id.to_string(),
        is_group: Some(true),
        x: Some(x),
        y: Some(y),
        width: Some(width),
        height: Some(height),
        ..Default::default()
    };
    set_metadata_string(&mut group, "direction", direction);
    group
}

fn title_rect(left: f64, right: f64, top: f64, bottom: f64) -> GroupTitleRect {
    GroupTitleRect {
        left,
        right,
        top,
        bottom,
    }
}

fn find<'a>(nodes: &'a [Node], id: &str) -> &'a Node {
    nodes.iter().find(|node| node.id == id).expect("node")
}

fn strict_crossing_count(edges: &[Edge]) -> usize {
    let mut count = 0;
    for (i, first) in edges.iter().enumerate() {
        let first_segments =
            orthogonal_segments_for_points(first.points.as_deref().unwrap_or(&[]), 1e-3);
        for second in &edges[i + 1..] {
            let second_segments =
                orthogonal_segments_for_points(second.points.as_deref().unwrap_or(&[]), 1e-3);
            for first in &first_segments {
                for second in &second_segments {
                    if orthogonal_segments_strictly_cross(
                        first.a, first.b, second.a, second.b, 1e-3,
                    ) {
                        count += 1;
                    }
                }
            }
        }
    }
    count
}

#[test]
fn separates_shared_visible_terminal_rails_on_the_same_node_face() {
    let nodes = vec![
        node("A", -40.0, -30.0, 10.0, 10.0),
        node("B", 0.0, 0.0, 10.0, 80.0),
    ];
    let mut edges = vec![
        edge("A_B_1", "A", "B", &[p(-30.0, 0.0), p(-5.0, 0.0)]),
        edge("A_B_2", "A", "B", &[p(-30.0, 0.0), p(-5.0, 0.0)]),
    ];

    separate_shared_rendered_terminal_lanes(&mut edges, &nodes);

    let mut terminal_ys: Vec<f64> = edges
        .iter()
        .map(|edge| edge.points.as_ref().unwrap().last().unwrap().y)
        .collect();
    terminal_ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_ne!(terminal_ys[0], terminal_ys[1]);
    assert!(terminal_ys.contains(&0.0));
}

#[test]
fn separates_near_parallel_terminal_rails_on_the_same_node_face() {
    let nodes = vec![
        node("A", -40.0, -30.0, 10.0, 10.0),
        node("B", 0.0, 0.0, 10.0, 60.0),
        node("C", -40.0, 30.0, 10.0, 10.0),
    ];
    let mut edges = vec![
        edge("A_B", "A", "B", &[p(-90.0, -4.0), p(-5.0, -4.0)]),
        edge("B_C", "B", "C", &[p(-5.0, 4.0), p(-90.0, 4.0)]),
    ];

    separate_shared_rendered_terminal_lanes(&mut edges, &nodes);

    let mut terminal_ys = [
        edges[0].points.as_ref().unwrap().last().unwrap().y,
        edges[1].points.as_ref().unwrap()[0].y,
    ];
    terminal_ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert!(terminal_ys[1] - terminal_ys[0] >= 16.0);
}

#[test]
fn collapses_a_provably_redundant_rectangular_dogleg() {
    let mut edges = vec![edge(
        "A_B",
        "A",
        "B",
        &[
            p(0.0, 0.0),
            p(10.0, 0.0),
            p(10.0, 10.0),
            p(0.0, 10.0),
            p(0.0, 20.0),
        ],
    )];

    collapse_redundant_rectangular_doglegs(&mut edges, &[]);

    assert_eq!(
        edges[0].points.as_deref(),
        Some([p(0.0, 0.0), p(0.0, 20.0)].as_slice())
    );
}

#[test]
fn lifts_a_same_side_rail_away_from_an_intervening_node_border() {
    let nodes = vec![
        node("26", -166.0, 54.0, 232.0, 66.0),
        node("27", 830.0, 54.0, 232.0, 66.0),
        node("28", 166.0, 54.0, 232.0, 108.0),
    ];
    let mut edges = vec![edge(
        "L_26_27_0",
        "26",
        "27",
        &[
            p(-166.0, 21.0),
            p(-166.0, 1.0),
            p(830.0, 1.0),
            p(830.0, 21.0),
        ],
    )];

    lift_obstacle_hugging_same_side_rails(&mut edges, &nodes);

    assert_eq!(
        edges[0].points.as_deref(),
        Some(
            [
                p(-166.0, 21.0),
                p(-166.0, -20.0),
                p(830.0, -20.0),
                p(830.0, 21.0)
            ]
            .as_slice()
        )
    );
}

#[test]
fn raises_top_lane_title_bands_above_a_clear_same_side_rail() {
    let mut general_manager = group("General_Manager", 166.0, 54.0, 996.0, 180.0, "TD");
    general_manager.group_title_rect = Some(title_rect(-332.0, 664.0, -36.0, -15.0));
    let mut nodes = vec![
        node("26", -166.0, 54.0, 232.0, 66.0),
        node("27", 830.0, 54.0, 232.0, 66.0),
        node("28", 166.0, 54.0, 232.0, 108.0),
        general_manager,
    ];
    let mut edges = vec![edge(
        "L_26_27_0",
        "26",
        "27",
        &[
            p(-166.0, 21.0),
            p(-166.0, 1.0),
            p(830.0, 1.0),
            p(830.0, 21.0),
        ],
    )];

    lift_obstacle_hugging_same_side_rails(&mut edges, &nodes);
    lift_top_lane_title_bands_above_rails(&edges, &mut nodes, None);

    assert_eq!(
        edges[0].points.as_deref(),
        Some(
            [
                p(-166.0, 21.0),
                p(-166.0, -20.0),
                p(830.0, -20.0),
                p(830.0, 21.0)
            ]
            .as_slice()
        )
    );
    let lane = find(&nodes, "General_Manager");
    assert_eq!(lane.y, Some(49.5));
    assert_eq!(lane.height, Some(189.0));
    assert_eq!(
        lane.group_title_rect,
        Some(title_rect(-332.0, 664.0, -45.0, -24.0))
    );
}

#[test]
fn shifts_lr_lane_title_bands_left_of_a_clear_left_side_rail() {
    let mut bod = group("BOD", 0.0, 100.0, 200.0, 200.0, "LR");
    bod.group_title_rect = Some(title_rect(-100.0, -64.0, 0.0, 200.0));
    let mut finance_head = group("Finance_Head", 0.0, 300.0, 200.0, 200.0, "LR");
    finance_head.group_title_rect = Some(title_rect(-100.0, -64.0, 200.0, 400.0));
    let mut nodes = vec![bod, finance_head];
    let points = [
        p(-80.0, 350.0),
        p(-80.0, 300.0),
        p(-90.0, 300.0),
        p(-90.0, 50.0),
        p(-80.0, 50.0),
    ];
    let edges = vec![edge("L_27_28_0", "27", "28", &points)];

    shift_left_lane_title_bands_left_of_rails(&edges, &mut nodes, None);

    assert_eq!(edges[0].points.as_deref(), Some(points.as_slice()));
    let bod = find(&nodes, "BOD");
    assert_eq!(bod.x, Some(-15.0));
    assert_eq!(bod.width, Some(230.0));
    assert_eq!(
        bod.group_title_rect,
        Some(title_rect(-130.0, -94.0, 0.0, 200.0))
    );
    let finance_head = find(&nodes, "Finance_Head");
    assert_eq!(finance_head.x, Some(-15.0));
    assert_eq!(finance_head.width, Some(230.0));
    assert_eq!(
        finance_head.group_title_rect,
        Some(title_rect(-130.0, -94.0, 200.0, 400.0))
    );
}

#[test]
fn swaps_shared_destination_terminal_tails_when_both_swaps_remove_a_crossing() {
    let c_exit_top_port = p(297.553125, 1171.1287841796875);
    let d_exit_right_port = p(318.890625, 1196.6287841796875);
    let nodes = vec![
        node("C", 292.21875, 287.5, 150.05624389648438, 91.0),
        node("D", 292.21875, 570.5, 94.11250305175781, 91.0),
        node("exit", 292.21875, 1196.6287841796875, 53.34375, 51.0),
    ];
    let mut edges = vec![
        edge(
            "L_C_exit_0",
            "C",
            "exit",
            &[
                p(367.2468719482422, 287.5),
                p(387.2468719482422, 287.5),
                p(387.2468719482422, 1151.1287841796875),
                p(297.553125, 1151.1287841796875),
                c_exit_top_port,
            ],
        ),
        edge(
            "L_D_exit_0",
            "D",
            "exit",
            &[
                p(339.2750015258789, 570.5),
                p(359.2750015258789, 570.5),
                p(359.2750015258789, 1196.6287841796875),
                d_exit_right_port,
            ],
        ),
    ];

    assert_eq!(strict_crossing_count(&edges), 1);

    swap_destination_terminal_tails_to_reduce_crossings(&mut edges, &nodes);

    assert_eq!(strict_crossing_count(&edges), 0);
    assert_eq!(
        edges[0].points.as_ref().unwrap().last().copied(),
        Some(d_exit_right_port)
    );
    assert_eq!(
        edges[1].points.as_ref().unwrap().last().copied(),
        Some(c_exit_top_port)
    );
}

#[test]
fn reassigns_overlapping_external_rail_channels_as_a_crossing_minimizing_bundle() {
    let nodes = vec![
        node("26", -22.0, 664.0, 116.0, 64.0),
        node("27", -22.0, 1602.0, 116.0, 116.0),
        node("28", -22.0, 1050.0, 116.0, 80.0),
    ];
    let mut edges = vec![
        edge(
            "L_26_27_0",
            "26",
            "27",
            &[
                p(-80.0, 664.0),
                p(-100.0, 664.0),
                p(-100.0, 1660.0),
                p(-80.0, 1660.0),
            ],
        ),
        edge(
            "L_27_28_0",
            "27",
            "28",
            &[
                p(36.0, 1544.0),
                p(65.0, 1544.0),
                p(65.0, 1467.0),
                p(-126.4, 1467.0),
                p(-126.4, 1132.0),
                p(36.0, 1132.0),
                p(36.0, 1050.0),
            ],
        ),
    ];

    assert_eq!(strict_crossing_count(&edges), 2);

    reassign_crossing_external_rail_channels(&mut edges, &nodes);

    assert_eq!(strict_crossing_count(&edges), 0);
    assert_eq!(edges[0].points.as_ref().unwrap()[1].x, -126.4);
    assert_eq!(edges[1].points.as_ref().unwrap()[3].x, -100.0);
}

#[test]
fn shortcuts_a_dominated_orthogonal_jog_when_clearance_is_preserved() {
    let nodes = vec![
        node("23", 36.0, 1328.0, 232.0, 108.0),
        node("27", 36.0, 1660.0, 232.0, 66.0),
        node("28", 36.0, 996.0, 232.0, 108.0),
    ];
    let mut edges = vec![edge(
        "L_27_28_0",
        "27",
        "28",
        &[
            p(36.0, 1544.0),
            p(65.0, 1544.0),
            p(65.0, 1467.0),
            p(-100.0, 1467.0),
            p(-100.0, 1132.0),
            p(36.0, 1132.0),
            p(36.0, 1050.0),
        ],
    )];

    shortcut_redundant_orthogonal_jogs(&mut edges, &nodes);

    assert_eq!(
        edges[0].points.as_deref(),
        Some(
            [
                p(36.0, 1544.0),
                p(36.0, 1467.0),
                p(-100.0, 1467.0),
                p(-100.0, 1132.0),
                p(36.0, 1132.0),
                p(36.0, 1050.0),
            ]
            .as_slice()
        )
    );
}

#[test]
fn shortcuts_a_redundant_jog_even_when_endpoint_nodes_are_unavailable() {
    let mut edges = vec![edge(
        "missing_endpoint_edge",
        "missing-start",
        "missing-end",
        &[
            p(36.0, 1544.0),
            p(65.0, 1544.0),
            p(65.0, 1467.0),
            p(-100.0, 1467.0),
            p(-100.0, 1132.0),
            p(36.0, 1132.0),
            p(36.0, 1050.0),
        ],
    )];

    shortcut_redundant_orthogonal_jogs(&mut edges, &[]);

    assert_eq!(
        edges[0].points.as_deref(),
        Some(
            [
                p(36.0, 1544.0),
                p(65.0, 1544.0),
                p(65.0, 1132.0),
                p(36.0, 1132.0),
                p(36.0, 1050.0),
            ]
            .as_slice()
        )
    );
}

const FIX_LEFT: f64 = -75.0;

fn close_terminal_layout(
    fix_height: f64,
    task_right: f64,
    arrival_y: f64,
) -> (Vec<Edge>, Vec<Node>) {
    let nodes = vec![
        node("task", task_right - 75.0, 0.0, 150.0, 45.0),
        node("fix", 0.0, 0.0, 150.0, fix_height),
        node("decision", task_right - 40.0, -130.0, 40.0, 40.0),
    ];
    let edges = vec![
        edge(
            "decision_fix",
            "decision",
            "fix",
            &[
                p(task_right - 40.0, -110.0),
                p(task_right - 40.0, arrival_y),
                p(FIX_LEFT, arrival_y),
            ],
        ),
        edge(
            "fix_task",
            "fix",
            "task",
            &[p(FIX_LEFT, 0.0), p(task_right, 0.0)],
        ),
    ];
    (edges, nodes)
}

#[test]
fn keeps_the_straight_connector_and_moves_the_other_track_to_the_end_of_the_face() {
    let (mut edges, nodes) = close_terminal_layout(45.0, -125.0, -10.0);
    separate_shared_rendered_terminal_lanes(&mut edges, &nodes);

    let fix_to_task = edges.iter().find(|edge| edge.id == "fix_task").unwrap();
    let decision_to_fix = edges.iter().find(|edge| edge.id == "decision_fix").unwrap();
    let ys: Vec<f64> = fix_to_task
        .points
        .as_ref()
        .unwrap()
        .iter()
        .map(|point| point.y)
        .collect();
    assert_eq!(ys, vec![0.0, 0.0]);
    assert_eq!(
        decision_to_fix.points.as_ref().unwrap().last().unwrap().y,
        -22.5 + 8.0
    );
}

#[test]
fn shifts_the_other_track_a_whole_step_when_the_face_has_room_for_it() {
    let (mut edges, nodes) = close_terminal_layout(80.0, -185.0, -10.0);
    separate_shared_rendered_terminal_lanes(&mut edges, &nodes);

    let fix_to_task = edges.iter().find(|edge| edge.id == "fix_task").unwrap();
    let decision_to_fix = edges.iter().find(|edge| edge.id == "decision_fix").unwrap();
    let ys: Vec<f64> = fix_to_task
        .points
        .as_ref()
        .unwrap()
        .iter()
        .map(|point| point.y)
        .collect();
    assert_eq!(ys, vec![0.0, 0.0]);
    assert_eq!(
        decision_to_fix.points.as_ref().unwrap().last().unwrap().y,
        -10.0 - 7.0
    );
}

#[test]
fn leaves_both_tracks_when_the_other_track_cannot_move_away_on_the_face() {
    let (mut edges, nodes) = close_terminal_layout(24.0, -110.0, -10.0);
    separate_shared_rendered_terminal_lanes(&mut edges, &nodes);

    let decision_to_fix = edges.iter().find(|edge| edge.id == "decision_fix").unwrap();
    assert_eq!(
        decision_to_fix.points.as_ref().unwrap().last().unwrap().y,
        -10.0
    );
}

#[test]
fn keeps_a_quarter_of_a_small_face_clear_at_each_corner() {
    let (mut edges, nodes) = close_terminal_layout(24.0, -110.0, -4.0);
    separate_shared_rendered_terminal_lanes(&mut edges, &nodes);

    let decision_to_fix = edges.iter().find(|edge| edge.id == "decision_fix").unwrap();
    assert_eq!(
        decision_to_fix.points.as_ref().unwrap().last().unwrap().y,
        -12.0 + 24.0 / 4.0
    );
}

#[test]
fn does_not_move_a_track_to_the_face_end_when_that_brings_it_closer_to_a_third_track() {
    let nodes = vec![
        node("task", -200.0, 0.0, 150.0, 45.0),
        node("fix", 0.0, 0.0, 150.0, 45.0),
        node("decision", -165.0, -130.0, 40.0, 40.0),
    ];
    let straight_at = |id: &str, y: f64| edge(id, "fix", "task", &[p(FIX_LEFT, y), p(-125.0, y)]);
    let upper = straight_at("fix_task_1", -12.0);
    let lower = straight_at("fix_task_2", 12.0);
    let decision_to_fix = edge(
        "decision_fix",
        "decision",
        "fix",
        &[p(-165.0, -110.0), p(-165.0, 5.0), p(FIX_LEFT, 5.0)],
    );
    let mut edges = vec![decision_to_fix, upper, lower];

    separate_shared_rendered_terminal_lanes(&mut edges, &nodes);

    let ys: Vec<f64> = edges[1]
        .points
        .as_ref()
        .unwrap()
        .iter()
        .map(|point| point.y)
        .collect();
    assert_eq!(ys, vec![-12.0, -12.0]);
    let ys: Vec<f64> = edges[2]
        .points
        .as_ref()
        .unwrap()
        .iter()
        .map(|point| point.y)
        .collect();
    assert_eq!(ys, vec![12.0, 12.0]);
    let arrival = edges[0].points.as_ref().unwrap().last().unwrap().y;
    let closest = (arrival + 12.0).abs().min((arrival - 12.0).abs());
    assert!(closest >= 12.0 - 5.0);
}

#[test]
fn moves_a_track_to_the_face_end_when_its_closest_track_stays_as_far_away() {
    let nodes = vec![
        node("task", -185.0, 0.0, 150.0, 45.0),
        node("fix", 0.0, 0.0, 150.0, 24.0),
        node("decision", -150.0, -130.0, 40.0, 40.0),
        node("other", -160.0, -230.0, 40.0, 40.0),
    ];
    let arrival = |id: &str, start: &str, x: f64, y: f64| {
        edge(id, start, "fix", &[p(x, -110.0), p(x, y), p(FIX_LEFT, y)])
    };
    let moved = arrival("decision_fix", "decision", -150.0, -4.0);
    let fix_to_task = edge(
        "fix_task",
        "fix",
        "task",
        &[p(FIX_LEFT, 0.0), p(-110.0, 0.0)],
    );
    let third = arrival("other_fix", "other", -160.0, -10.0);
    let mut edges = vec![moved, fix_to_task, third];

    separate_shared_rendered_terminal_lanes(&mut edges, &nodes);

    assert_eq!(
        edges[0].points.as_ref().unwrap().last().unwrap().y,
        -12.0 + 24.0 / 4.0
    );
    assert_eq!(edges[2].points.as_ref().unwrap().last().unwrap().y, -10.0);
}

#[test]
fn moves_a_track_to_the_face_end_past_tracks_on_another_face_or_another_node() {
    let nodes = vec![
        node("task", -200.0, 0.0, 150.0, 45.0),
        node("fix", 0.0, 0.0, 150.0, 45.0),
        node("decision", -165.0, -130.0, 40.0, 40.0),
        node("right", 200.0, 0.0, 150.0, 45.0),
        node("above", -90.0, -130.0, 40.0, 40.0),
        node("up", -14.0, -130.0, 40.0, 40.0),
    ];
    let fix_to_task = edge(
        "fix_task",
        "fix",
        "task",
        &[p(FIX_LEFT, 0.0), p(-125.0, 0.0)],
    );
    let decision_to_fix = edge(
        "decision_fix",
        "decision",
        "fix",
        &[p(-165.0, -110.0), p(-165.0, -10.0), p(FIX_LEFT, -10.0)],
    );
    let fix_to_right = edge(
        "fix_right",
        "fix",
        "right",
        &[p(75.0, -14.0), p(125.0, -14.0)],
    );
    let above_to_task = edge(
        "above_task",
        "above",
        "task",
        &[p(-90.0, -110.0), p(-90.0, -14.0), p(-125.0, -14.0)],
    );
    let fix_to_up = edge("fix_up", "fix", "up", &[p(-14.0, -22.5), p(-14.0, -110.0)]);
    let mut edges = vec![
        decision_to_fix,
        fix_to_task,
        fix_to_right,
        above_to_task,
        fix_to_up,
    ];

    separate_shared_rendered_terminal_lanes(&mut edges, &nodes);

    assert_eq!(
        edges[0].points.as_ref().unwrap().last().unwrap().y,
        -22.5 + 8.0
    );
}

#[test]
fn separates_a_later_pair_of_tracks_after_a_close_pair_that_cannot_move() {
    let at_face_limit = -12.0 + 24.0 / 4.0;
    let nodes = vec![
        node("task", -185.0, 0.0, 150.0, 45.0),
        node("fix", 0.0, 0.0, 150.0, 24.0),
        node("decision", -150.0, -130.0, 40.0, 40.0),
        node("A", 300.0, 0.0, 40.0, 80.0),
        node("B", 400.0, 0.0, 40.0, 80.0),
    ];
    let decision_to_fix = edge(
        "decision_fix",
        "decision",
        "fix",
        &[
            p(-150.0, -110.0),
            p(-150.0, at_face_limit),
            p(FIX_LEFT, at_face_limit),
        ],
    );
    let fix_to_task = edge(
        "fix_task",
        "fix",
        "task",
        &[p(FIX_LEFT, 0.0), p(-110.0, 0.0)],
    );
    let straight = |id: &str| edge(id, "A", "B", &[p(320.0, 0.0), p(380.0, 0.0)]);
    let mut edges = vec![
        decision_to_fix,
        fix_to_task,
        straight("A_B_1"),
        straight("A_B_2"),
    ];

    separate_shared_rendered_terminal_lanes(&mut edges, &nodes);

    assert_eq!(
        edges[0].points.as_ref().unwrap().last().unwrap().y,
        at_face_limit
    );
    assert_ne!(
        edges[2].points.as_ref().unwrap().last().unwrap().y,
        edges[3].points.as_ref().unwrap().last().unwrap().y
    );
}

#[test]
fn moves_one_of_two_identical_straight_connectors_even_though_both_are_straight() {
    let nodes = vec![
        node("A", -100.0, 0.0, 40.0, 80.0),
        node("B", 0.0, 0.0, 40.0, 80.0),
    ];
    let mut edges = vec![
        edge("A_B_1", "A", "B", &[p(-80.0, 0.0), p(-20.0, 0.0)]),
        edge("A_B_2", "A", "B", &[p(-80.0, 0.0), p(-20.0, 0.0)]),
    ];

    separate_shared_rendered_terminal_lanes(&mut edges, &nodes);

    assert_ne!(
        edges[0].points.as_ref().unwrap().last().unwrap().y,
        edges[1].points.as_ref().unwrap().last().unwrap().y
    );
}

#[test]
fn separates_two_tracks_on_a_face_too_narrow_for_a_whole_step_inside_its_corners() {
    let nodes = vec![
        node("A", 0.0, -100.0, 20.0, 40.0),
        node("B", 0.0, 0.0, 20.0, 40.0),
    ];
    let down = |id: &str| edge(id, "A", "B", &[p(0.0, -80.0), p(0.0, -20.0)]);
    let mut edges = vec![down("A_B_1"), down("A_B_2")];

    separate_shared_rendered_terminal_lanes(&mut edges, &nodes);

    let ends: Vec<f64> = edges
        .iter()
        .map(|edge| edge.points.as_ref().unwrap().last().unwrap().x)
        .collect();
    assert_ne!(ends[0], ends[1]);
    for end in ends {
        assert!(end.abs() <= 10.0 - 20.0 / 4.0);
    }
}

#[test]
fn moves_a_left_title_band_clear_of_a_rail_on_its_top_edge() {
    let mut lane = group("lane", 100.0, 200.0, 200.0, 200.0, "LR");
    lane.group_title_rect = Some(title_rect(0.0, 24.0, 100.0, 300.0));
    let mut nodes = vec![lane];
    let edges = vec![edge("rail", "a", "b", &[p(-40.0, 100.0), p(60.0, 100.0)])];

    shift_left_lane_title_bands_left_of_rails(&edges, &mut nodes, None);

    assert!(find(&nodes, "lane").group_title_rect.unwrap().left < 0.0);
}

#[test]
fn moves_a_left_title_band_clear_of_a_rail_on_its_bottom_edge() {
    let mut lane = group("lane", 100.0, 200.0, 200.0, 200.0, "LR");
    lane.group_title_rect = Some(title_rect(0.0, 24.0, 100.0, 300.0));
    let mut nodes = vec![lane];
    let edges = vec![edge("rail", "a", "b", &[p(-40.0, 300.0), p(60.0, 300.0)])];

    shift_left_lane_title_bands_left_of_rails(&edges, &mut nodes, None);

    assert!(find(&nodes, "lane").group_title_rect.unwrap().left < 0.0);
}

#[test]
fn leaves_a_left_title_band_where_it_is_for_a_clear_rail() {
    let mut lane = group("lane", 100.0, 200.0, 200.0, 200.0, "LR");
    lane.group_title_rect = Some(title_rect(0.0, 24.0, 100.0, 300.0));
    let mut nodes = vec![lane];
    let edges = vec![edge("rail", "a", "b", &[p(-40.0, 80.0), p(60.0, 80.0)])];

    shift_left_lane_title_bands_left_of_rails(&edges, &mut nodes, None);

    assert_eq!(
        find(&nodes, "lane").group_title_rect,
        Some(title_rect(0.0, 24.0, 100.0, 300.0))
    );
}

#[test]
fn reads_a_layout_direction_lr_as_lr_for_a_lane_without_its_own() {
    for layout in ["lr", "Lr"] {
        let mut lane = group("lane", 100.0, 200.0, 200.0, 200.0, "");
        lane.metadata = None;
        lane.group_title_rect = Some(title_rect(0.0, 24.0, 100.0, 300.0));
        let mut nodes = vec![lane];
        let edges = vec![edge("rail", "a", "b", &[p(10.0, 150.0), p(60.0, 150.0)])];

        shift_left_lane_title_bands_left_of_rails(&edges, &mut nodes, Some(layout));

        assert!(find(&nodes, "lane").group_title_rect.unwrap().left < 0.0);
    }
}

#[test]
fn lifts_the_title_band_of_a_lane_in_a_pool_above_a_rail_across_it() {
    let mut nodes = pooled_lane_nodes("TB");
    let edges = vec![edge("e", "a", "b", &[p(-50.0, -90.0), p(50.0, -90.0)])];

    lift_top_lane_title_bands_above_rails(&edges, &mut nodes, None);

    assert!(find(&nodes, "lane").group_title_rect.unwrap().bottom < -90.0);
    assert_eq!(find(&nodes, "pool").group_title_rect.unwrap().bottom, -79.0);
}

#[test]
fn shifts_the_title_band_of_a_lane_in_a_pool_left_of_a_rail_across_it() {
    let mut nodes = pooled_lane_nodes("LR");
    let edges = vec![edge("e", "a", "b", &[p(-80.0, -50.0), p(-80.0, 50.0)])];

    shift_left_lane_title_bands_left_of_rails(&edges, &mut nodes, None);

    assert!(find(&nodes, "lane").group_title_rect.unwrap().right < -80.0);
    assert_eq!(find(&nodes, "pool").group_title_rect.unwrap().right, -64.0);
}

fn pooled_lane_nodes(direction: &str) -> Vec<Node> {
    let title = if direction == "TB" {
        title_rect(-100.0, 100.0, -100.0, -79.0)
    } else {
        title_rect(-100.0, -64.0, -100.0, 100.0)
    };
    let mut pool = group("pool", 0.0, 0.0, 220.0, 220.0, direction);
    pool.group_title_rect = Some(title);
    set_metadata_string(&mut pool, "laneRole", "pool");
    let mut lane = group("lane", 0.0, 0.0, 200.0, 200.0, direction);
    lane.parent_id = Some("pool".to_string());
    lane.group_title_rect = Some(title);
    let mut other = group("other", 0.0, 300.0, 0.0, 0.0, direction);
    other.parent_id = Some("pool".to_string());

    vec![pool, lane, other]
}
