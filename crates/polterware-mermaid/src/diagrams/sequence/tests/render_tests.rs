//! Source-to-scene fixtures recorded from upstream `mermaid.render`.
//!
//! Each JSON file in `render/` holds a sequence diagram `source` and the `expected` output
//! `research/mermaid-port/harness/generate-sequence-render-fixtures.mjs` printed for it:
//! positioned actors, boxes, messages, notes, loops, edge points and the fitted `viewBox`.

use serde_json::Value;

use super::parse;
use super::render::draw_layout_ast;
use crate::config::Config;
use crate::measure::FallbackMeasure;

const TOLERANCE: f64 = 0.01;

fn fixtures() -> Vec<(String, Value)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/diagrams/sequence/tests/render");
    let mut fixtures: Vec<(String, Value)> = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("read {}: {error}", dir.display()))
        .map(|entry| entry.expect("dir entry").path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
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

#[test]
fn renders_recorded_sequence_fixtures() {
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

        let mut config = Config::default();
        config.font_family = "monospace".to_string();

        let (data, scene) = draw_layout_ast(&ast, &config, &FallbackMeasure)
            .unwrap_or_else(|error| panic!("{file}: render: {error}"));
        let expected = &fixture["expected"];

        // 1. Actors
        if let Some(expected_actors) = expected["actors"].as_array() {
            assert_eq!(
                data.actors.len(),
                expected_actors.len(),
                "{file}: actor count"
            );
            for actor in expected_actors {
                let id = actor["id"].as_str().expect("actor id");
                let actual = data
                    .actors
                    .iter()
                    .find(|candidate| candidate.id == id)
                    .unwrap_or_else(|| panic!("{file}: actor {id} in layout"));
                close(actual.x, actor["x"].as_f64().expect("x"), &format!("actor {id} x"), &file);
                close(actual.y, actor["y"].as_f64().expect("y"), &format!("actor {id} y"), &file);
                close(actual.width, actor["width"].as_f64().expect("width"), &format!("actor {id} width"), &file);
                close(actual.height, actor["height"].as_f64().expect("height"), &format!("actor {id} height"), &file);
                close(actual.stopy, actor["stopy"].as_f64().expect("stopy"), &format!("actor {id} stopy"), &file);
            }
        }

        // 2. Boxes
        if let Some(expected_boxes) = expected["boxes"].as_array() {
            assert_eq!(
                data.boxes.len(),
                expected_boxes.len(),
                "{file}: box count"
            );
            for (idx, b) in expected_boxes.iter().enumerate() {
                let actual = &data.boxes[idx];
                if let Some(name) = b["name"].as_str() {
                    assert_eq!(actual.name.as_deref(), Some(name), "{file}: box {idx} name");
                }
                close(actual.x, b["x"].as_f64().expect("x"), &format!("box {idx} x"), &file);
                close(actual.y, b["y"].as_f64().expect("y"), &format!("box {idx} y"), &file);
                close(actual.width, b["width"].as_f64().expect("width"), &format!("box {idx} width"), &file);
                close(actual.height, b["height"].as_f64().expect("height"), &format!("box {idx} height"), &file);
                close(actual.startx, b["startx"].as_f64().expect("startx"), &format!("box {idx} startx"), &file);
                close(actual.starty, b["starty"].as_f64().expect("starty"), &format!("box {idx} starty"), &file);
                close(actual.stopx, b["stopx"].as_f64().expect("stopx"), &format!("box {idx} stopx"), &file);
                close(actual.stopy, b["stopy"].as_f64().expect("stopy"), &format!("box {idx} stopy"), &file);
            }
        }

        // 3. Messages
        if let Some(expected_messages) = expected["messages"].as_array() {
            assert_eq!(
                data.messages.len(),
                expected_messages.len(),
                "{file}: message count"
            );
            for (idx, msg) in expected_messages.iter().enumerate() {
                let actual = &data.messages[idx];
                let id = msg["id"].as_str().expect("id");
                assert_eq!(actual.id, id, "{file}: message {idx} id");
                assert_eq!(actual.from, msg["from"].as_str().expect("from"), "{file}: message {idx} from");
                assert_eq!(actual.to, msg["to"].as_str().expect("to"), "{file}: message {idx} to");
                assert_eq!(actual.line_type, msg["type"].as_u64().expect("type") as u8, "{file}: message {idx} type");
                close(actual.startx, msg["startx"].as_f64().expect("startx"), &format!("message {idx} startx"), &file);
                close(actual.stopx, msg["stopx"].as_f64().expect("stopx"), &format!("message {idx} stopx"), &file);
                close(actual.starty, msg["starty"].as_f64().expect("starty"), &format!("message {idx} starty"), &file);
                close(actual.stopy, msg["stopy"].as_f64().expect("stopy"), &format!("message {idx} stopy"), &file);
                close(actual.height, msg["height"].as_f64().expect("height"), &format!("message {idx} height"), &file);
            }
        }

        // 4. Notes
        if let Some(expected_notes) = expected["notes"].as_array() {
            assert_eq!(
                data.notes.len(),
                expected_notes.len(),
                "{file}: note count"
            );
            for (idx, note) in expected_notes.iter().enumerate() {
                let actual = &data.notes[idx];
                close(actual.startx, note["startx"].as_f64().expect("startx"), &format!("note {idx} startx"), &file);
                close(actual.stopx, note["stopx"].as_f64().expect("stopx"), &format!("note {idx} stopx"), &file);
                close(actual.starty, note["starty"].as_f64().expect("starty"), &format!("note {idx} starty"), &file);
                close(actual.stopy, note["stopy"].as_f64().expect("stopy"), &format!("note {idx} stopy"), &file);
                close(actual.width, note["width"].as_f64().expect("width"), &format!("note {idx} width"), &file);
                close(actual.height, note["height"].as_f64().expect("height"), &format!("note {idx} height"), &file);
            }
        }

        // 5. Loops
        if let Some(expected_loops) = expected["loops"].as_array() {
            assert_eq!(
                data.loops.len(),
                expected_loops.len(),
                "{file}: loop count"
            );
            for (idx, l) in expected_loops.iter().enumerate() {
                let actual = &data.loops[idx];
                if let Some(title) = l["title"].as_str() {
                    assert_eq!(actual.title.as_deref(), Some(title), "{file}: loop {idx} title");
                }
                close(actual.startx, l["startx"].as_f64().expect("startx"), &format!("loop {idx} startx"), &file);
                close(actual.stopx, l["stopx"].as_f64().expect("stopx"), &format!("loop {idx} stopx"), &file);
                close(actual.starty, l["starty"].as_f64().expect("starty"), &format!("loop {idx} starty"), &file);
                close(actual.stopy, l["stopy"].as_f64().expect("stopy"), &format!("loop {idx} stopy"), &file);
                close(actual.height, l["height"].as_f64().expect("height"), &format!("loop {idx} height"), &file);
            }
        }

        // 6. Edges
        if let Some(expected_edges) = expected["edges"].as_array() {
            assert_eq!(
                data.edges.len(),
                expected_edges.len(),
                "{file}: edge count"
            );
            for expected_edge in expected_edges {
                let id = expected_edge["id"].as_str().expect("edge id");
                let actual = data
                    .edges
                    .iter()
                    .find(|candidate| candidate.id == id)
                    .unwrap_or_else(|| panic!("{file}: edge {id} in layout"));

                if let Some(d) = expected_edge["d"].as_str() {
                    assert_eq!(actual.d.as_deref(), Some(d), "{file}: edge {id} d");
                } else {
                    let expected_pts = expected_edge["points"].as_array().expect("points");
                    assert_eq!(actual.points.len(), expected_pts.len(), "{file}: edge {id} point count");
                    for (p_idx, (act_pt, exp_pt)) in actual.points.iter().zip(expected_pts).enumerate() {
                        close(act_pt.x, exp_pt["x"].as_f64().expect("x"), &format!("edge {id} pt {p_idx} x"), &file);
                        close(act_pt.y, exp_pt["y"].as_f64().expect("y"), &format!("edge {id} pt {p_idx} y"), &file);
                    }
                }
            }
        }

        // 7. ViewBox
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
