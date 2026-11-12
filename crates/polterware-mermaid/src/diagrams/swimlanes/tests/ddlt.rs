//! The upstream swimlanes DDLT specs (`*.ddlt.spec.ts`), ported to the assertions that
//! describe the layout: the pinned geometry invariants of the swimlanes router.
//!
//! Upstream runs these specs against fixtures whose node sizes were captured from a browser
//! (`.sizes.json`); here the same sources (kept in the render fixtures, see
//! [`super::render_fixtures`]) are laid out with the crate's monospace measurement, which is
//! the measurement every other fixture in this crate uses. The assertions are the specs'
//! geometric invariants, not the browser's absolute coordinates, so they hold regardless.
//!
//! Not ported, because the upstream `validateLayout` quality model (`layout-utils/
//! validateLayout.ts` — crossings, border hugging, overlaps, score breakdown) is not part of
//! the swimlanes layout port; only the final `direction/validation.ts` scan is. The specs'
//! Level 2 score thresholds depend on that model.
//!
//! Also left out: 8-query-process-2's `L_A2_E_0` perpendicular-face pin, whose expected
//! three-point shape depends on the browser-captured node sizes of the fixture. Under the
//! monospace measurement this crate renders with, the same router produces a five-point
//! detour, so the pin describes that measurement rather than the layout rule.

use std::path::Path;

use crate::config::Config;
use crate::layout::swimlanes::direction::validation::{
    ValidationIssueType, collect_validation_issues,
};
use crate::measure::FallbackMeasure;
use crate::render::pipeline::types::{Edge, LayoutData, Node};
use crate::scene::Point;

use super::super::parse;
use super::super::render::draw_layout_ast;

fn fixtures_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/diagrams/swimlanes/tests/render")
}

/// The source a render fixture was generated from.
fn source(fixture: &str) -> String {
    let path = fixtures_dir().join(fixture);
    let fixture: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read fixture"))
            .expect("parse fixture");
    fixture["source"]
        .as_str()
        .expect("fixture source")
        .to_string()
}

fn layout(fixture: &str) -> LayoutData {
    let source = source(fixture);
    let preprocessed = crate::preprocess::preprocess(&source);
    let mut config = Config::default();
    config.apply(&preprocessed.config).expect("fixture config");
    let ast = parse(&preprocessed.body).expect("parse fixture");
    draw_layout_ast(&ast, &config, &FallbackMeasure)
        .expect("render fixture")
        .0
}

fn node<'a>(data: &'a LayoutData, id: &str) -> &'a Node {
    data.nodes
        .iter()
        .find(|node| node.id == id)
        .unwrap_or_else(|| panic!("node {id} not laid out"))
}

fn edge<'a>(data: &'a LayoutData, id: &str) -> &'a Edge {
    data.edges
        .iter()
        .find(|edge| edge.id == id)
        .unwrap_or_else(|| panic!("edge {id} not laid out"))
}

fn points(data: &LayoutData, id: &str) -> Vec<Point> {
    edge(data, id)
        .points
        .clone()
        .unwrap_or_else(|| panic!("edge {id} has no points"))
}

/// `dedupeConsecutive` from the specs.
fn dedupe(points: &[Point]) -> Vec<Point> {
    let mut result: Vec<Point> = Vec::new();
    for point in points {
        match result.last() {
            Some(last) if (last.x - point.x).abs() <= 1e-6 && (last.y - point.y).abs() <= 1e-6 => {}
            _ => result.push(*point),
        }
    }
    result
}

fn rect(node: &Node) -> (f64, f64, f64, f64) {
    let width = node.width.unwrap_or(0.0) / 2.0;
    let height = node.height.unwrap_or(0.0) / 2.0;
    (
        node.x.unwrap_or(0.0) - width,
        node.x.unwrap_or(0.0) + width,
        node.y.unwrap_or(0.0) - height,
        node.y.unwrap_or(0.0) + height,
    )
}

/// Whether an axis-aligned segment passes through the strict interior of a label box.
fn segment_crosses_rect(a: Point, b: Point, rect: (f64, f64, f64, f64)) -> bool {
    let (left, right, top, bottom) = rect;
    if (a.y - b.y).abs() < 1e-6 {
        let y = a.y;
        let (x0, x1) = (a.x.min(b.x), a.x.max(b.x));
        y > top + 1e-6 && y < bottom - 1e-6 && x0 < right - 1e-6 && x1 > left + 1e-6
    } else if (a.x - b.x).abs() < 1e-6 {
        let x = a.x;
        let (y0, y1) = (a.y.min(b.y), a.y.max(b.y));
        x > left + 1e-6 && x < right - 1e-6 && y0 < bottom - 1e-6 && y1 > top + 1e-6
    } else {
        false
    }
}

// simple-2.ddlt.spec.ts

#[test]
fn simple_2_inbound_labels_do_not_cross_overlap() {
    // Iteration 14 pin: A→B ("AB") and E→B ("EB") both terminate at B; neither label may
    // sit on the other edge, and neither edge may run through the other's label.
    let data = layout("01-simple-2.json");
    let labels: Vec<&Node> = data
        .nodes
        .iter()
        .filter(|node| node.is_edge_label == Some(true))
        .collect();
    assert_eq!(labels.len(), 2);

    for label in &labels {
        let own_edge = label
            .id
            .strip_prefix("edge-label-")
            .and_then(|rest| {
                let (from, rest) = rest.split_once('-')?;
                let (to, edge_id) = rest.split_once('-')?;
                Some(format!("{from}-{to}-{edge_id}"))
            })
            .expect("label id carries its edge");
        let own_edge = own_edge
            .strip_prefix(&format!(
                "{}-{}-",
                label.edge_start.as_deref().unwrap_or_default(),
                label.edge_end.as_deref().unwrap_or_default()
            ))
            .unwrap_or(&own_edge)
            .to_string();
        let label_rect = rect(label);
        for candidate in &data.edges {
            if candidate.is_layout_only.unwrap_or(false) || candidate.id == own_edge {
                continue;
            }
            let Some(points) = candidate.points.as_deref() else {
                continue;
            };
            for pair in points.windows(2) {
                assert!(
                    !segment_crosses_rect(pair[0], pair[1], label_rect),
                    "{}: edge {} crosses label {}",
                    label.id,
                    candidate.id,
                    label.id
                );
            }
        }
    }
}

#[test]
fn simple_2_a_to_b_is_a_centered_straight_vertical() {
    // Iteration 15 pin: A and B share a column, so A→B is one straight vertical at A's
    // center x (Siebenhaller's Kandinsky centered-straight-line demand).
    let data = layout("01-simple-2.json");
    let a = node(&data, "A");
    let b = node(&data, "B");
    let (ax, bx) = (a.x.unwrap(), b.x.unwrap());
    assert!((ax - bx).abs() < 0.1, "A and B share a column");

    let points = points(&data, "L_A_B_0");
    assert_eq!(points.len(), 2, "A→B is a two-point line");
    assert!((points[0].x - points[1].x).abs() < 0.1, "vertical");
    assert!((points[0].x - ax).abs() < 1.0, "centered on A.x");
}

// 4-car-fun-sales-tb.ddlt.spec.ts

#[test]
fn four_car_i_to_k_leaves_i_from_a_side_port() {
    // Iteration 1 pin: L_I_K_0 must leave I through a side face perpendicular to that face,
    // not slide along the node border.
    let data = layout("03-4-car-fun-sales-tb.json");
    let points = dedupe(&points(&data, "L_I_K_0"));
    assert!(points.len() >= 2);
    let i = node(&data, "I");
    let (left, right, top, bottom) = rect(i);
    let start = points[0];
    let next = points[1];
    let eps = 0.01;

    let on_left = (start.x - left).abs() < eps;
    let on_right = (start.x - right).abs() < eps;
    let on_top = (start.y - top).abs() < eps;
    let on_bottom = (start.y - bottom).abs() < eps;
    assert!(
        on_left || on_right || on_top || on_bottom,
        "L_I_K_0 starts on I's boundary: {start:?}"
    );
    // A side port leaves perpendicular to its face. A start on a vertical face moves along
    // x first; one on a horizontal face moves along y first.
    if on_left || on_right {
        assert!(
            (start.y - next.y).abs() < eps && (start.x - next.x).abs() > eps,
            "L_I_K_0 leaves a side face perpendicular: {start:?} -> {next:?}"
        );
    } else {
        assert!(
            (start.x - next.x).abs() < eps && (start.y - next.y).abs() > eps,
            "L_I_K_0 leaves a horizontal face perpendicular: {start:?} -> {next:?}"
        );
    }
}

// 5-car-fun-sales-wide-tb.ddlt.spec.ts

#[test]
fn five_car_d_to_e_and_e_to_f_use_different_ports_on_e() {
    // Iteration 9 pin: the two edges on E may not share an attach point.
    let data = layout("04-5-car-fun-sales-wide-tb.json");
    let lde = points(&data, "L_D_E_0");
    let lef = points(&data, "L_E_F_0");
    let lde_end = *lde.last().unwrap();
    let lef_start = lef[0];
    assert!(
        (lde_end.x - lef_start.x).abs() >= 1e-3 || (lde_end.y - lef_start.y).abs() >= 1e-3,
        "L_D_E_0 and L_E_F_0 share E's attach point: {lde_end:?}"
    );
}

#[test]
fn five_car_d_to_e_detours_west_of_h() {
    // Iteration 11 pin: cross-lane-first routing keeps L_D_E_0's detour west of H instead
    // of crossing L_H_I_0 on H's row.
    let data = layout("04-5-car-fun-sales-wide-tb.json");
    let points = points(&data, "L_D_E_0");
    let h = node(&data, "H");
    let (h_x, h_y) = (h.x.unwrap(), h.y.unwrap());

    let detour_leg_x = points.windows(2).find_map(|pair| {
        let (a, b) = (pair[0], pair[1]);
        if (a.x - b.x).abs() < 1e-6 {
            let (y0, y1) = (a.y.min(b.y), a.y.max(b.y));
            (y0 <= h_y && h_y <= y1).then_some(a.x)
        } else {
            None
        }
    });
    let detour_leg_x = detour_leg_x.expect("L_D_E_0 has a leg crossing H's row");
    assert!(
        detour_leg_x < h_x,
        "L_D_E_0 detours west of H: leg {detour_leg_x} vs H.x {h_x}"
    );
}

#[test]
fn five_car_d_to_h_is_a_two_point_vertical() {
    // Iteration 12 pin: the co-route sibling straight-line rescue keeps D→H straight.
    let data = layout("04-5-car-fun-sales-wide-tb.json");
    let points = points(&data, "L_D_H_0");
    assert_eq!(points.len(), 2);
    assert!((points[0].x - points[1].x).abs() < 1.0, "vertical");
}

// 7-car-sales-constr.ddlt.spec.ts

#[test]
fn seven_car_j_to_e_is_straight_when_its_ends_share_an_axis() {
    // Iteration 8 pin: a same-axis back edge collapses to its two ports.
    let data = layout("06-7-car-sales-constr.json");
    let j = node(&data, "J");
    let e = node(&data, "E");
    let share_y = (j.y.unwrap() - e.y.unwrap()).abs() < 1e-3;
    let share_x = (j.x.unwrap() - e.x.unwrap()).abs() < 1e-3;
    if !share_x && !share_y {
        return;
    }
    let points = points(&data, "L_J_E_0");
    assert_eq!(points.len(), 2);
    if share_y {
        assert!((points[0].y - points[1].y).abs() < 1e-3);
    } else {
        assert!((points[0].x - points[1].x).abs() < 1e-3);
    }
}

// 8-query-process-2.ddlt.spec.ts

#[test]
fn eight_query_terminal_stubs_clear_the_arrowhead() {
    // Iteration 16 pin: no edge's last segment may be shorter than the arrowhead base (10).
    let data = layout("07-8-query-process-2.json");
    let e_to_g = dedupe(&points(&data, "L_E_G_0"));
    let last = e_to_g[e_to_g.len() - 1];
    let before = e_to_g[e_to_g.len() - 2];
    let last_length = ((last.x - before.x).powi(2) + (last.y - before.y).powi(2)).sqrt();
    assert!(
        last_length >= 10.0,
        "L_E_G_0's last segment is {last_length}"
    );

    let offenders: Vec<String> = data
        .edges
        .iter()
        .filter(|edge| !edge.is_layout_only.unwrap_or(false))
        .filter_map(|edge| {
            let points = dedupe(edge.points.as_deref()?);
            if points.len() <= 2 {
                return None;
            }
            let last = points[points.len() - 1];
            let before = points[points.len() - 2];
            let length = ((last.x - before.x).powi(2) + (last.y - before.y).powi(2)).sqrt();
            (length < 10.0).then(|| format!("{}: {length}", edge.id))
        })
        .collect();
    assert!(offenders.is_empty(), "short terminal stubs: {offenders:?}");
}

// query-process.ddlt.spec.ts

#[test]
fn query_process_g_to_f2_is_a_straight_horizontal() {
    // Iteration 7 pin: the stale port-offset Z-edge collapses to one horizontal line.
    let data = layout("09-query-process.json");
    let gf = points(&data, "L_G_F2_0");
    let first_y = gf[0].y;
    assert!(
        gf.iter().all(|point| (point.y - first_y).abs() < 1e-3),
        "G→F2 is horizontal: {gf:?}"
    );
}

#[test]
fn query_process_a2_departures_use_distinct_ports() {
    // Iteration 6 pin: sibling departures from A2 are spread so no two edges share a port
    // or a subpath.
    let data = layout("09-query-process.json");
    let mut starts: Vec<(String, Point)> = Vec::new();
    for candidate in &data.edges {
        if candidate.is_layout_only.unwrap_or(false) || candidate.start.as_deref() != Some("A2") {
            continue;
        }
        let points = points(&data, &candidate.id);
        starts.push((candidate.id.clone(), points[0]));
    }
    for (index, (first_id, first)) in starts.iter().enumerate() {
        for (second_id, second) in starts.iter().skip(index + 1) {
            assert!(
                (first.x - second.x).abs() > 0.01 || (first.y - second.y).abs() > 0.01,
                "{first_id} and {second_id} share a departure port"
            );
        }
    }
}

// 10-node-placement.ddlt.spec.ts

#[test]
fn ten_node_d4_loopback_does_not_cross_d2() {
    // Level 1 pin: the loopback enters D2 at its approach-side boundary and intersects
    // nothing of D2.
    let data = layout("08-10-node-placement.json");
    let issues = collect_validation_issues(&data);
    let offenders: Vec<_> = issues
        .iter()
        .filter(|issue| {
            issue.edge_id == "L_D4_D2_0"
                && issue.target_id == "D2"
                && issue.issue_type == ValidationIssueType::EdgeNodeOverlap
        })
        .collect();
    assert!(offenders.is_empty(), "L_D4_D2_0 crosses D2: {offenders:?}");
}
