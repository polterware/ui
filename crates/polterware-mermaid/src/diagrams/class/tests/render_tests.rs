//! Source-to-scene fixtures recorded from upstream `mermaid.render`.
//!
//! Each JSON file in `render/` holds a class diagram `source` and the `expected` output
//! `research/mermaid-port/harness/generate-class-render-fixtures.mjs` printed for it:
//! positioned node boxes, namespace cluster boxes, edge points and label boxes, and the fitted
//! `viewBox`. Regenerate them with
//! `node research/mermaid-port/harness/generate-class-render-fixtures.mjs`.

use serde_json::Value;

use super::super::parse;
use super::super::render::draw_layout_ast;
use crate::config::Config;
use crate::measure::FallbackMeasure;

const TOLERANCE: f64 = 0.01;

fn fixtures() -> Vec<(String, Value)> {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/diagrams/class/tests/render");
    let mut fixtures: Vec<(String, Value)> = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("read {}: {error}", dir.display()))
        .map(|entry| entry.expect("dir entry").path())
        .filter(|path| path.extension().and_then(|extension| extension.to_str()) == Some("json"))
        .map(|path| {
            let text = std::fs::read_to_string(&path).expect("read fixture");
            let value: Value = serde_json::from_str(&text).expect("parse fixture");
            (
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
                    .to_string(),
                value,
            )
        })
        .collect();
    fixtures.sort_by(|a, b| a.0.cmp(&b.0));
    fixtures
}

fn close(actual: f64, expected: f64, what: &str, file: &str) {
    assert!(
        (actual - expected).abs() <= TOLERANCE,
        "{file}: {what}: actual {actual} vs expected {expected}"
    );
}

fn box_of(value: &Value) -> (f64, f64, f64, f64) {
    (
        value["x"].as_f64().expect("x"),
        value["y"].as_f64().expect("y"),
        value["width"].as_f64().expect("width"),
        value["height"].as_f64().expect("height"),
    )
}

#[test]
fn renders_recorded_class_fixtures() {
    let fixtures = fixtures();
    assert!(
        fixtures.len() >= 10,
        "expected at least 10 fixtures, found {}",
        fixtures.len()
    );

    for (file, fixture) in fixtures {
        let source = fixture["source"].as_str().expect("fixture source");
        let body = crate::preprocess::preprocess(source).body;
        let ast = parse(&body).unwrap_or_else(|error| panic!("{file}: parse: {error}"));
        let (data, scene) = draw_layout_ast(&ast, &Config::default(), &FallbackMeasure)
            .unwrap_or_else(|error| panic!("{file}: render: {error}"));
        let expected = &fixture["expected"];

        for node in expected["nodes"].as_array().expect("expected nodes") {
            // A node nested in a namespace keeps its position in the layout graph, not on the
            // data object upstream's harness captures, so it has no recorded box to compare.
            let Some(x) = node["x"].as_f64() else {
                continue;
            };
            let id = node["id"].as_str().expect("node id");
            let actual = data
                .nodes
                .iter()
                .find(|candidate| candidate.id == id)
                .unwrap_or_else(|| panic!("{file}: node {id} in layout"));
            let y = node["y"].as_f64().expect("y");
            let width = node["width"].as_f64().expect("width");
            let height = node["height"].as_f64().expect("height");
            close(actual.x.expect("x"), x, &format!("node {id} x"), &file);
            close(actual.y.expect("y"), y, &format!("node {id} y"), &file);
            close(
                actual.width.expect("width"),
                width,
                &format!("node {id} width"),
                &file,
            );
            close(
                actual.height.expect("height"),
                height,
                &format!("node {id} height"),
                &file,
            );
        }

        for cluster in expected["clusters"].as_array().expect("expected clusters") {
            let id = cluster["id"].as_str().expect("cluster id");
            let actual = data
                .nodes
                .iter()
                .find(|candidate| candidate.id == id)
                .unwrap_or_else(|| panic!("{file}: cluster {id} in layout"));
            let (x, y, width, height) = box_of(cluster);
            close(actual.x.expect("x"), x, &format!("cluster {id} x"), &file);
            close(actual.y.expect("y"), y, &format!("cluster {id} y"), &file);
            close(
                actual.width.expect("width"),
                width,
                &format!("cluster {id} width"),
                &file,
            );
            close(
                actual.height.expect("height"),
                height,
                &format!("cluster {id} height"),
                &file,
            );
        }

        for edge in expected["edges"].as_array().expect("expected edges") {
            let id = edge["id"].as_str().expect("edge id");
            let actual = data
                .edges
                .iter()
                .find(|candidate| candidate.id == id)
                .unwrap_or_else(|| panic!("{file}: edge {id} in layout"));
            let expected_points = edge["points"].as_array().expect("points");
            let actual_points = actual.points.as_ref().expect("actual points");
            assert_eq!(
                actual_points.len(),
                expected_points.len(),
                "{file}: edge {id} point count"
            );
            for (index, (actual_point, expected_point)) in
                actual_points.iter().zip(expected_points).enumerate()
            {
                close(
                    actual_point.x,
                    expected_point["x"].as_f64().expect("point x"),
                    &format!("edge {id} point {index} x"),
                    &file,
                );
                close(
                    actual_point.y,
                    expected_point["y"].as_f64().expect("point y"),
                    &format!("edge {id} point {index} y"),
                    &file,
                );
            }
            for (field, actual_value) in [
                ("x", actual.x),
                ("y", actual.y),
                ("width", actual.width),
                ("height", actual.height),
            ] {
                if let Some(expected_value) = edge[field].as_f64() {
                    close(
                        actual_value.unwrap_or(0.0),
                        expected_value,
                        &format!("edge {id} {field}"),
                        &file,
                    );
                }
            }
        }

        let view_box = &expected["viewBox"];
        close(
            scene.view_box.x,
            view_box["x"].as_f64().expect("viewBox x"),
            "viewBox x",
            &file,
        );
        close(
            scene.view_box.y,
            view_box["y"].as_f64().expect("viewBox y"),
            "viewBox y",
            &file,
        );
        close(
            scene.view_box.width,
            view_box["width"].as_f64().expect("viewBox width"),
            "viewBox width",
            &file,
        );
        close(
            scene.view_box.height,
            view_box["height"].as_f64().expect("viewBox height"),
            "viewBox height",
            &file,
        );
    }
}
