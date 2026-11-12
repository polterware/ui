//! The C4 render fixtures: every source in `../tests/render` is rendered with the same
//! monospace fallback measurement the fixture generator uses, flattened to the SVG geometry
//! upstream appends, and compared within 0.01 px.

use serde_json::{Value, json};

use super::draw_ast;
use crate::config::Config;
use crate::measure::FallbackMeasure;
use crate::scene::{Color, Item, PathCommand, TextAnchor, TextBaseline, Transform};

const TOLERANCE: f64 = 0.01;

fn fixtures() -> Vec<(String, Value)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/diagrams/c4/tests/render");
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

fn color_string(color: &Option<Color>) -> Value {
    color
        .as_ref()
        .map(|color| Value::String(color.0.clone()))
        .unwrap_or(Value::Null)
}

fn translate_command(command: &PathCommand, dx: f64, dy: f64) -> Value {
    match *command {
        PathCommand::MoveTo { x, y } => json!({ "op": "moveTo", "x": x + dx, "y": y + dy }),
        PathCommand::LineTo { x, y } => json!({ "op": "lineTo", "x": x + dx, "y": y + dy }),
        PathCommand::CubicTo {
            c1x,
            c1y,
            c2x,
            c2y,
            x,
            y,
        } => json!({
            "op": "cubicTo",
            "c1x": c1x + dx,
            "c1y": c1y + dy,
            "c2x": c2x + dx,
            "c2y": c2y + dy,
            "x": x + dx,
            "y": y + dy,
        }),
        PathCommand::QuadTo { cx, cy, x, y } => json!({
            "op": "quadTo",
            "cx": cx + dx,
            "cy": cy + dy,
            "x": x + dx,
            "y": y + dy,
        }),
        PathCommand::ArcTo {
            rx,
            ry,
            rotation,
            large_arc,
            sweep,
            x,
            y,
        } => json!({
            "op": "arcTo",
            "rx": rx,
            "ry": ry,
            "rotation": rotation,
            "large_arc": large_arc,
            "sweep": sweep,
            "x": x + dx,
            "y": y + dy,
        }),
        PathCommand::Close => json!({ "op": "close" }),
    }
}

fn marker(value: &crate::scene::MarkerRef) -> Value {
    Value::String(format!("url(#{})", value.def))
}

struct Flat {
    shapes: Vec<Value>,
    texts: Vec<Value>,
}

fn flatten(items: &[Item], transform: Transform, flat: &mut Flat) {
    for item in items {
        match item {
            Item::Group(group) => {
                flatten(&group.items, transform.then(group.transform), flat);
            }
            Item::Rect(rect) => {
                let point = transform.apply(crate::scene::Point::new(rect.rect.x, rect.rect.y));
                flat.shapes.push(json!({
                    "kind": "rect",
                    "x": point.x,
                    "y": point.y,
                    "width": rect.rect.width,
                    "height": rect.rect.height,
                    "rx": rect.rx,
                    "ry": rect.ry,
                    "fill": color_string(&rect.style.fill),
                    "stroke": color_string(&rect.style.stroke),
                    "strokeWidth": rect.style.stroke_width,
                    "dash": rect.style.dash,
                }));
            }
            Item::Ellipse(ellipse) => {
                let point = transform.apply(ellipse.center);
                if (ellipse.radius_x - ellipse.radius_y).abs() <= TOLERANCE {
                    flat.shapes.push(json!({
                        "kind": "circle",
                        "cx": point.x,
                        "cy": point.y,
                        "r": ellipse.radius_x,
                        "fill": color_string(&ellipse.style.fill),
                        "stroke": color_string(&ellipse.style.stroke),
                        "strokeWidth": ellipse.style.stroke_width,
                        "dash": ellipse.style.dash,
                    }));
                }
            }
            Item::Path(path) => {
                flat.shapes.push(json!({
                    "kind": "path",
                    "commands": path
                        .path
                        .commands
                        .iter()
                        .map(|command| translate_command(command, transform.e, transform.f))
                        .collect::<Vec<_>>(),
                    "fill": color_string(&path.style.fill),
                    "stroke": color_string(&path.style.stroke),
                    "strokeWidth": path.style.stroke_width,
                    "dash": path.style.dash,
                    "markerStart": path.start_markers.first().map(marker),
                    "markerEnd": path.end_markers.first().map(marker),
                }));
            }
            Item::Text(text) => {
                let point = transform.apply(text.position);
                let bold = text
                    .lines
                    .iter()
                    .any(|line| line.spans.iter().any(|span| span.bold));
                let italic = text
                    .lines
                    .iter()
                    .any(|line| line.spans.iter().any(|span| span.italic));
                flat.texts.push(json!({
                    "text": text.lines.iter().map(|line| line.text()).collect::<Vec<_>>().join("\n"),
                    "lines": text.lines.iter().map(|line| line.text()).collect::<Vec<_>>(),
                    "x": point.x,
                    "y": point.y,
                    "anchor": match text.anchor {
                        TextAnchor::Start => "start",
                        TextAnchor::Middle => "middle",
                        TextAnchor::End => "end",
                    },
                    "baseline": match text.baseline {
                        TextBaseline::Alphabetic => "alphabetic",
                        TextBaseline::Middle => "middle",
                        TextBaseline::Central => "central",
                        TextBaseline::Hanging => "hanging",
                        TextBaseline::Top => "top",
                        TextBaseline::Bottom => "bottom",
                    },
                    "fill": color_string(&text.fill),
                    "fontSize": text.font.size,
                    "bold": bold,
                    "italic": italic,
                }));
            }
            Item::Image(_) => {}
        }
    }
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

#[test]
fn renders_recorded_c4_fixtures() {
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
        let expected = &fixture["expected"];

        close(
            scene.view_box.x,
            expected["viewBox"]["x"].as_f64().expect("viewBox x"),
            "viewBox x",
            &file,
        );
        close(
            scene.view_box.y,
            expected["viewBox"]["y"].as_f64().expect("viewBox y"),
            "viewBox y",
            &file,
        );
        close(
            scene.view_box.width,
            expected["viewBox"]["width"]
                .as_f64()
                .expect("viewBox width"),
            "viewBox width",
            &file,
        );
        close(
            scene.view_box.height,
            expected["viewBox"]["height"]
                .as_f64()
                .expect("viewBox height"),
            "viewBox height",
            &file,
        );

        let mut flat = Flat {
            shapes: Vec::new(),
            texts: Vec::new(),
        };
        flatten(&scene.items, Transform::IDENTITY, &mut flat);

        close_value(
            &Value::Array(flat.shapes),
            &expected["shapes"],
            "shapes",
            &file,
        );
        close_value(
            &Value::Array(flat.texts),
            &expected["texts"],
            "texts",
            &file,
        );
    }
}

#[test]
fn render_diagram_dispatches_c4() {
    let scene = crate::render_diagram(
        "C4Context\n  title Context\n  Person(a, \"A\")\n  System(b, \"B\")\n  Rel(a, b, \"Uses\")\n",
        &Config::default(),
    )
    .expect("renders through the dispatch");
    assert!(!scene.items.is_empty());
    assert!(scene.view_box.height > 0.0);
}
