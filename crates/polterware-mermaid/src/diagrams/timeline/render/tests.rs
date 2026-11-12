//! The timeline renderer against the geometry upstream draws, recorded by
//! `research/mermaid-port/harness/generate-timeline-render-fixtures.mjs`: every node group's
//! translate, its background path, bottom rule, wrapped text and section colors, every connector
//! and the activity line, the title, and the viewBox. Numbers compare within 0.01 units.

use serde_json::{Value, json};

use super::draw_ast;
use crate::config::Config;
use crate::measure::FallbackMeasure;
use crate::scene::{Group, Item, PathShape, Scene, TextRun};

const TOLERANCE: f64 = 0.01;

fn fixtures() -> Vec<(String, Value)> {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/diagrams/timeline/tests/render");
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

fn close_value(actual: &Value, expected: &Value, what: &str, file: &str) {
    match (actual, expected) {
        (Value::Number(actual), Value::Number(expected)) => close(
            actual.as_f64().expect("number"),
            expected.as_f64().expect("number"),
            what,
            file,
        ),
        (Value::Array(actual), Value::Array(expected)) => {
            assert_eq!(actual.len(), expected.len(), "{file}: {what}: length");
            for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                close_value(actual, expected, &format!("{what}[{index}]"), file);
            }
        }
        (Value::Object(actual), Value::Object(expected)) => {
            assert_eq!(actual.len(), expected.len(), "{file}: {what}: keys");
            for (key, expected) in expected {
                close_value(
                    actual.get(key).unwrap_or(&Value::Null),
                    expected,
                    &format!("{what}.{key}"),
                    file,
                );
            }
        }
        _ => assert_eq!(actual, expected, "{file}: {what}"),
    }
}

fn color(value: &Option<crate::scene::Color>) -> Value {
    value
        .as_ref()
        .map(|color| Value::String(color.0.clone()))
        .unwrap_or(Value::Null)
}

fn path_json(shape: &PathShape) -> Value {
    json!({
        "commands": shape
            .path
            .commands
            .iter()
            .map(|command| serde_json::to_value(command).expect("command"))
            .collect::<Vec<_>>(),
        "fill": color(&shape.style.fill),
        "stroke": color(&shape.style.stroke),
        "strokeWidth": if shape.style.stroke.is_some() {
            json!(shape.style.stroke_width)
        } else {
            Value::Null
        },
    })
}

fn line_json(shape: &PathShape, with_dash: bool) -> Value {
    let mut points = shape
        .path
        .commands
        .iter()
        .filter_map(|command| match command {
            crate::scene::PathCommand::MoveTo { x, y } => Some((*x, *y)),
            crate::scene::PathCommand::LineTo { x, y } => Some((*x, *y)),
            _ => None,
        });
    let (x1, y1) = points.next().expect("line start");
    let (x2, y2) = points.next().expect("line end");
    let mut value = json!({
        "x1": x1,
        "y1": y1,
        "x2": x2,
        "y2": y2,
        "stroke": color(&shape.style.stroke),
        "strokeWidth": shape.style.stroke_width,
    });
    if with_dash {
        value["dash"] = json!(shape.style.dash);
    }
    value
}

fn text_json(run: &TextRun, group: &Group) -> Value {
    let bounds = run.bounds();
    json!({
        "x": group.transform.e + bounds.x,
        "y": group.transform.f + bounds.y,
        "width": run.size.width,
        "height": run.size.height,
        "fontSize": run.font.size,
        "anchor": "middle",
        "fill": color(&run.fill),
        "lines": run
            .lines
            .iter()
            .map(|line| Value::String(line.text()))
            .collect::<Vec<_>>(),
    })
}

fn as_group<'a>(item: &'a Item, file: &str, what: &str) -> &'a Group {
    match item {
        Item::Group(group) => group,
        _ => panic!("{file}: {what} is not a group"),
    }
}

fn wrapper_json(wrapper: &Group, class: &str, file: &str) -> Value {
    let kind = match class {
        "taskWrapper" => "task",
        "eventWrapper" => "event",
        _ => "section",
    };
    let node = as_group(&wrapper.items[0], file, "node group");
    let section = node
        .class
        .as_deref()
        .and_then(|class| class.strip_prefix("timeline-node section-"))
        .unwrap_or_else(|| panic!("{file}: node class {:?}", node.class))
        .parse::<i64>()
        .expect("section index");
    let background = as_group(&node.items[0], file, "background group");
    let path = match &background.items[0] {
        Item::Path(path) => path,
        _ => panic!("{file}: background is not a path"),
    };
    let line = match background.items.get(1) {
        Some(Item::Path(line)) => line_json(line, false),
        _ => Value::Null,
    };
    let text_group = as_group(&node.items[1], file, "text group");
    let run = match &text_group.items[0] {
        Item::Text(run) => run,
        _ => panic!("{file}: node label is not text"),
    };
    json!({
        "kind": kind,
        "x": wrapper.transform.e,
        "y": wrapper.transform.f,
        "node": {
            "section": section,
            "path": path_json(path),
            "line": line,
            "text": text_json(run, text_group),
        },
    })
}

fn title_json(run: &TextRun) -> Value {
    let bounds = run.bounds();
    json!({
        "text": run.lines.first().map(|line| line.text()).unwrap_or_default(),
        "x": bounds.x,
        "y": bounds.y,
        "fontSize": run.font.size,
        "fontWeight": if run.font.is_bold() { "bold" } else { "normal" },
        "fill": color(&run.fill),
    })
}

fn actual_json(scene: &Scene) -> Value {
    let mut nodes = Vec::new();
    let mut lines = Vec::new();
    let mut title = Value::Null;
    for item in &scene.items {
        match item {
            Item::Text(run) => title = title_json(run),
            Item::Group(group) => {
                let class = group.class.as_deref().unwrap_or_default();
                if class == "lineWrapper" {
                    let line = match group.items.first() {
                        Some(Item::Path(line)) => line,
                        _ => panic!("line wrapper without a path"),
                    };
                    lines.push(line_json(line, true));
                } else {
                    nodes.push(wrapper_json(group, class, "scene"));
                }
            }
            _ => panic!("unexpected scene item"),
        }
    }
    json!({
        "viewBox": {
            "x": scene.view_box.x,
            "y": scene.view_box.y,
            "width": scene.view_box.width,
            "height": scene.view_box.height,
        },
        "nodes": nodes,
        "lines": lines,
        "title": title,
    })
}

#[test]
fn renders_recorded_timeline_fixtures() {
    let fixtures = fixtures();
    assert!(
        fixtures.len() >= 8,
        "expected at least 8 fixtures, found {}",
        fixtures.len()
    );

    for (file, fixture) in fixtures {
        let source = fixture["source"].as_str().expect("fixture source");
        let preprocessed = crate::preprocess::preprocess(source);
        let config = Config::from_source_config(&preprocessed.config)
            .unwrap_or_else(|error| panic!("{file}: config: {error}"));
        let ast = super::super::parse(&preprocessed.body)
            .unwrap_or_else(|error| panic!("{file}: parse: {error}"));
        let scene = draw_ast(&ast, &config, &FallbackMeasure);
        let actual = actual_json(&scene);
        let expected = &fixture["expected"];
        for key in ["viewBox", "nodes", "lines", "title"] {
            close_value(&actual[key], &expected[key], key, &file);
        }
    }
}

#[test]
fn dispatches_through_the_crate_entry_point() {
    let scene = crate::render_diagram("timeline\n    2002 : LinkedIn\n", &Config::default())
        .expect("dispatch timeline");
    assert!(!scene.items.is_empty());
    assert!(scene.view_box.width > 0.0);
    assert!(scene.marker_def("timeline-arrowhead").is_some());
}

#[test]
fn wraps_exactly_like_svg_draw() {
    let config = Config::default();
    let ctx = super::Ctx::new(&config, &FallbackMeasure);
    assert_eq!(ctx.wrap_lines("2002", 150.0), vec!["2002"]);
    assert_eq!(
        ctx.wrap_lines("An extremely long project phase", 150.0),
        vec![
            "An   extremely",
            "long   project",
            "phase",
            // `wrap` joins each whitespace token with a space, then trims the line.
        ]
    );
    assert_eq!(
        ctx.wrap_lines("[object Object]", 150.0),
        vec!["[object", "Object]"]
    );
}
