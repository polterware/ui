//! End-to-end fixtures: a swimlanes source rendered through the whole pipeline, compared
//! with `mermaid.render` under jsdom. Regenerate with
//! `node generate-swimlanes-render-fixtures.mjs <dir>` from the harness.

use std::path::Path;

use crate::config::Config;
use crate::measure::FallbackMeasure;

use super::super::parse;
use super::super::render::draw_layout_ast;

fn fixtures_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/diagrams/swimlanes/tests/render")
}

fn close(actual: f64, expected: f64, what: &str, name: &str) {
    assert!(
        (actual - expected).abs() <= 0.01,
        "{name}: {what}: actual {actual} vs expected {expected}"
    );
}

#[test]
fn swimlane_render_fixtures_match_mermaid_render() {
    let dir = fixtures_dir();
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .expect("read swimlane render fixtures")
        .map(|entry| entry.expect("dir entry").path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect();
    entries.sort();
    assert!(entries.len() >= 8, "expected at least 8 fixtures");

    for path in entries {
        let fixture: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read fixture"))
                .expect("parse fixture json");
        let name = fixture["name"].as_str().expect("fixture name");
        let source = fixture["source"].as_str().expect("fixture source");

        let preprocessed = crate::preprocess::preprocess(source);
        let mut config = Config::default();
        config
            .apply(&preprocessed.config)
            .unwrap_or_else(|error| panic!("{name}: config: {error}"));
        let ast =
            parse(&preprocessed.body).unwrap_or_else(|error| panic!("{name}: parse: {error}"));
        let (data, scene) = draw_layout_ast(&ast, &config, &FallbackMeasure)
            .unwrap_or_else(|error| panic!("{name}: render: {error}"));

        let nodes: Vec<_> = data
            .nodes
            .iter()
            .filter(|node| node.is_group != Some(true) && node.is_edge_label != Some(true))
            .collect();
        let expected_nodes = fixture["nodes"].as_array().expect("nodes");
        assert_eq!(nodes.len(), expected_nodes.len(), "{name}: node count");
        for expected in expected_nodes {
            let id = expected["id"].as_str().expect("node id");
            let node = nodes
                .iter()
                .find(|node| node.id == id)
                .unwrap_or_else(|| panic!("{name}: node {id} not laid out"));
            close(
                node.x.expect("x"),
                expected["x"].as_f64().unwrap(),
                &format!("node {id} x"),
                name,
            );
            close(
                node.y.expect("y"),
                expected["y"].as_f64().unwrap(),
                &format!("node {id} y"),
                name,
            );
            close(
                node.width.expect("width"),
                expected["width"].as_f64().unwrap(),
                &format!("node {id} width"),
                name,
            );
            close(
                node.height.expect("height"),
                expected["height"].as_f64().unwrap(),
                &format!("node {id} height"),
                name,
            );
        }

        let lanes: Vec<_> = data
            .nodes
            .iter()
            .filter(|node| node.is_group == Some(true))
            .collect();
        let expected_lanes = fixture["lanes"].as_array().expect("lanes");
        assert_eq!(lanes.len(), expected_lanes.len(), "{name}: lane count");
        for expected in expected_lanes {
            let id = expected["id"].as_str().expect("lane id");
            let lane = lanes
                .iter()
                .find(|lane| lane.id == id)
                .unwrap_or_else(|| panic!("{name}: lane {id} not laid out"));
            close(
                lane.x.expect("x"),
                expected["x"].as_f64().unwrap(),
                &format!("lane {id} x"),
                name,
            );
            close(
                lane.y.expect("y"),
                expected["y"].as_f64().unwrap(),
                &format!("lane {id} y"),
                name,
            );
            close(
                lane.width.expect("width"),
                expected["width"].as_f64().unwrap(),
                &format!("lane {id} width"),
                name,
            );
            close(
                lane.height.expect("height"),
                expected["height"].as_f64().unwrap(),
                &format!("lane {id} height"),
                name,
            );
        }

        let labels: Vec<_> = data
            .nodes
            .iter()
            .filter(|node| node.is_edge_label == Some(true))
            .collect();
        let expected_labels = fixture["labels"].as_array().expect("labels");
        assert_eq!(labels.len(), expected_labels.len(), "{name}: label count");
        for expected in expected_labels {
            let id = expected["id"].as_str().expect("label id");
            let label = labels
                .iter()
                .find(|label| label.id == id)
                .unwrap_or_else(|| panic!("{name}: label {id} not laid out"));
            assert_eq!(
                label.label.as_deref(),
                expected["text"].as_str(),
                "{name}: label {id} text"
            );
            close(
                label.x.expect("x"),
                expected["x"].as_f64().unwrap(),
                &format!("label {id} x"),
                name,
            );
            close(
                label.y.expect("y"),
                expected["y"].as_f64().unwrap(),
                &format!("label {id} y"),
                name,
            );
            close(
                label.width.expect("width"),
                expected["width"].as_f64().unwrap(),
                &format!("label {id} width"),
                name,
            );
            close(
                label.height.expect("height"),
                expected["height"].as_f64().unwrap(),
                &format!("label {id} height"),
                name,
            );
        }

        let expected_edges = fixture["edges"].as_array().expect("edges");
        assert_eq!(data.edges.len(), expected_edges.len(), "{name}: edge count");
        for expected in expected_edges {
            let id = expected["id"].as_str().expect("edge id");
            let edge = data
                .edges
                .iter()
                .find(|edge| edge.id == id)
                .unwrap_or_else(|| panic!("{name}: edge {id} not laid out"));
            let points = edge
                .points
                .as_ref()
                .unwrap_or_else(|| panic!("{name}: edge {id} has no points"));
            let expected_points = expected["points"].as_array().expect("points");
            assert_eq!(
                points.len(),
                expected_points.len(),
                "{name}: edge {id} point count"
            );
            for (index, (point, expected)) in points.iter().zip(expected_points).enumerate() {
                close(
                    point.x,
                    expected["x"].as_f64().unwrap(),
                    &format!("edge {id} point {index} x"),
                    name,
                );
                close(
                    point.y,
                    expected["y"].as_f64().unwrap(),
                    &format!("edge {id} point {index} y"),
                    name,
                );
            }
        }

        let view_box = &fixture["viewBox"];
        close(
            scene.view_box.x,
            view_box["x"].as_f64().unwrap(),
            "viewBox x",
            name,
        );
        close(
            scene.view_box.y,
            view_box["y"].as_f64().unwrap(),
            "viewBox y",
            name,
        );
        close(
            scene.view_box.width,
            view_box["width"].as_f64().unwrap(),
            "viewBox width",
            name,
        );
        close(
            scene.view_box.height,
            view_box["height"].as_f64().unwrap(),
            "viewBox height",
            name,
        );
    }
}
