//! Fixture tests for the wardley renderer: every fixture is a source plus the geometry the
//! upstream harness read back from `wardleyRenderer.ts`'s SVG, compared within 0.01px.

use serde_json::Value;

use crate::config::Config;
use crate::measure::FallbackMeasure;
use crate::scene::{
    EllipseShape, Group, Item, MarkerOrient, PathCommand, PathShape, RectShape, Scene, TextAnchor,
    TextBaseline, TextRun,
};

use super::{DIAGRAM_ID, draw_ast};

const TOLERANCE: f64 = 0.01;

fn fixtures() -> Vec<(String, Value)> {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/diagrams/wardley/tests/render");
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

fn text_of(run: &TextRun) -> String {
    run.lines
        .iter()
        .map(|line| line.text())
        .collect::<Vec<_>>()
        .join("\n")
}

fn group<'a>(item: &'a Item, file: &str, what: &str) -> &'a Group {
    match item {
        Item::Group(group) => group,
        _ => panic!("{file}: {what} is not a group"),
    }
}

fn text<'a>(item: &'a Item, file: &str, what: &str) -> &'a TextRun {
    match item {
        Item::Text(text) => text,
        _ => panic!("{file}: {what} is not text"),
    }
}

fn circle<'a>(item: &'a Item, file: &str, what: &str) -> &'a EllipseShape {
    match item {
        Item::Ellipse(circle) => circle,
        _ => panic!("{file}: {what} is not a circle"),
    }
}

fn rect<'a>(item: &'a Item, file: &str, what: &str) -> &'a RectShape {
    match item {
        Item::Rect(rect) => rect,
        _ => panic!("{file}: {what} is not a rect"),
    }
}

fn path<'a>(item: &'a Item, file: &str, what: &str) -> &'a PathShape {
    match item {
        Item::Path(path) => path,
        _ => panic!("{file}: {what} is not a path"),
    }
}

fn find_group<'a>(items: &'a [Item], class: &str) -> Option<&'a Group> {
    items.iter().find_map(|item| match item {
        Item::Group(group) if group.class.as_deref() == Some(class) => Some(group),
        _ => None,
    })
}

fn required_group<'a>(items: &'a [Item], class: &str, file: &str) -> &'a Group {
    find_group(items, class).unwrap_or_else(|| panic!("{file}: missing group {class}"))
}

fn color_of(value: &Value) -> Option<&str> {
    value.as_str().filter(|value| *value != "none")
}

fn color_string(value: &Option<crate::scene::Color>) -> Option<&str> {
    value.as_ref().map(|color| color.0.as_str())
}

fn dash_of(value: &Value) -> Vec<f64> {
    value
        .as_str()
        .map(|dash| {
            dash.split([' ', ','])
                .filter(|part| !part.is_empty())
                .map(|part| part.parse::<f64>().expect("dash value"))
                .collect()
        })
        .unwrap_or_default()
}

fn anchor_of(value: &Value) -> TextAnchor {
    match value.as_str().unwrap_or("start") {
        "middle" => TextAnchor::Middle,
        "end" => TextAnchor::End,
        _ => TextAnchor::Start,
    }
}

fn baseline_of(value: &Value) -> TextBaseline {
    match value.as_str().unwrap_or("auto") {
        "middle" => TextBaseline::Middle,
        "central" => TextBaseline::Central,
        "hanging" => TextBaseline::Hanging,
        "top" => TextBaseline::Top,
        "bottom" => TextBaseline::Bottom,
        _ => TextBaseline::Alphabetic,
    }
}

fn check_text(run: &TextRun, expected: &Value, what: &str, file: &str) {
    assert_eq!(
        text_of(run),
        expected["text"].as_str().unwrap_or_default(),
        "{file}: {what} text"
    );
    close(
        run.position.x,
        expected["x"].as_f64().expect("text x"),
        &format!("{what} x"),
        file,
    );
    close(
        run.position.y,
        expected["y"].as_f64().expect("text y"),
        &format!("{what} y"),
        file,
    );
    if let Some(fill) = expected["fill"].as_str() {
        assert_eq!(color_string(&run.fill), Some(fill), "{file}: {what} fill");
    }
    close(
        run.font.size,
        expected["fontSize"].as_f64().expect("font size"),
        &format!("{what} font size"),
        file,
    );
    let weight = if expected["fontWeight"].as_str() == Some("bold") {
        700
    } else {
        400
    };
    assert_eq!(run.font.weight, weight, "{file}: {what} font weight");
    assert_eq!(
        run.anchor,
        anchor_of(&expected["anchor"]),
        "{file}: {what} anchor"
    );
    assert_eq!(
        run.baseline,
        baseline_of(&expected["baseline"]),
        "{file}: {what} baseline"
    );
    match expected["rotate"].as_object() {
        Some(rotate) => {
            close(
                run.rotation,
                rotate["angle"].as_f64().expect("rotation angle"),
                &format!("{what} rotation"),
                file,
            );
            close(
                run.position.x,
                rotate["x"].as_f64().expect("rotation x"),
                &format!("{what} rotation x"),
                file,
            );
            close(
                run.position.y,
                rotate["y"].as_f64().expect("rotation y"),
                &format!("{what} rotation y"),
                file,
            );
        }
        None => close(run.rotation, 0.0, &format!("{what} rotation"), file),
    }
}

fn marker_kind(definition: &str) -> &str {
    definition
        .strip_suffix(DIAGRAM_ID)
        .and_then(|definition| definition.strip_suffix('-'))
        .unwrap_or(definition)
}

fn check_line(shape: &PathShape, expected: &Value, what: &str, file: &str) {
    let (x1, y1, x2, y2) = match &shape.path.commands[..] {
        [
            PathCommand::MoveTo { x, y },
            PathCommand::LineTo { x: x2, y: y2 },
        ] => (*x, *y, *x2, *y2),
        other => panic!("{file}: {what}: unexpected line commands {other:?}"),
    };
    close(
        x1,
        expected["x1"].as_f64().expect("x1"),
        &format!("{what} x1"),
        file,
    );
    close(
        y1,
        expected["y1"].as_f64().expect("y1"),
        &format!("{what} y1"),
        file,
    );
    close(
        x2,
        expected["x2"].as_f64().expect("x2"),
        &format!("{what} x2"),
        file,
    );
    close(
        y2,
        expected["y2"].as_f64().expect("y2"),
        &format!("{what} y2"),
        file,
    );
    assert_eq!(
        color_string(&shape.style.stroke),
        expected["stroke"].as_str(),
        "{file}: {what} stroke"
    );
    close(
        shape.style.stroke_width,
        expected["strokeWidth"].as_f64().expect("stroke width"),
        &format!("{what} stroke width"),
        file,
    );
    let dash = dash_of(&expected["dash"]);
    assert_eq!(
        shape.style.dash.len(),
        dash.len(),
        "{file}: {what} dash length"
    );
    for (index, (actual, expected)) in shape.style.dash.iter().zip(&dash).enumerate() {
        close(*actual, *expected, &format!("{what} dash {index}"), file);
    }
    close(
        shape.style.opacity,
        expected["opacity"].as_f64().unwrap_or(1.0),
        &format!("{what} opacity"),
        file,
    );
    let start = shape
        .start_markers
        .first()
        .map(|marker| marker_kind(&marker.def));
    assert_eq!(
        start,
        expected["markerStart"].as_str(),
        "{file}: {what} marker start"
    );
    let end = shape
        .end_markers
        .first()
        .map(|marker| marker_kind(&marker.def));
    assert_eq!(
        end,
        expected["markerEnd"].as_str(),
        "{file}: {what} marker end"
    );
}

fn check_circle(shape: &EllipseShape, expected: &Value, what: &str, file: &str) {
    close(
        shape.center.x,
        expected["cx"].as_f64().expect("cx"),
        &format!("{what} cx"),
        file,
    );
    close(
        shape.center.y,
        expected["cy"].as_f64().expect("cy"),
        &format!("{what} cy"),
        file,
    );
    close(
        shape.radius_x,
        expected["r"].as_f64().expect("r"),
        &format!("{what} r"),
        file,
    );
    close(shape.radius_y, shape.radius_x, &format!("{what} ry"), file);
    assert_eq!(
        color_string(&shape.style.fill),
        expected["fill"].as_str(),
        "{file}: {what} fill"
    );
    assert_eq!(
        color_string(&shape.style.stroke),
        expected["stroke"].as_str(),
        "{file}: {what} stroke"
    );
    close(
        shape.style.stroke_width,
        expected["strokeWidth"].as_f64().expect("stroke width"),
        &format!("{what} stroke width"),
        file,
    );
}

fn check_rect(shape: &RectShape, expected: &Value, what: &str, file: &str) {
    close(
        shape.rect.x,
        expected["x"].as_f64().expect("x"),
        &format!("{what} x"),
        file,
    );
    close(
        shape.rect.y,
        expected["y"].as_f64().expect("y"),
        &format!("{what} y"),
        file,
    );
    close(
        shape.rect.width,
        expected["width"].as_f64().expect("width"),
        &format!("{what} width"),
        file,
    );
    close(
        shape.rect.height,
        expected["height"].as_f64().expect("height"),
        &format!("{what} height"),
        file,
    );
    close(
        shape.rx,
        expected["rx"].as_f64().expect("rx"),
        &format!("{what} rx"),
        file,
    );
    close(
        shape.ry,
        expected["ry"].as_f64().expect("ry"),
        &format!("{what} ry"),
        file,
    );
    assert_eq!(
        color_string(&shape.style.fill),
        color_of(&expected["fill"]),
        "{file}: {what} fill"
    );
    assert_eq!(
        color_string(&shape.style.stroke),
        color_of(&expected["stroke"]),
        "{file}: {what} stroke"
    );
    close(
        shape.style.stroke_width,
        expected["strokeWidth"].as_f64().expect("stroke width"),
        &format!("{what} stroke width"),
        file,
    );
}

fn check_path(shape: &PathShape, expected: &Value, what: &str, file: &str) {
    let actual = serde_json::to_value(&shape.path.commands).expect("serialize commands");
    close_value(
        &actual,
        &expected["commands"],
        &format!("{what} path"),
        file,
    );
    assert_eq!(
        color_string(&shape.style.fill),
        color_of(&expected["fill"]),
        "{file}: {what} fill"
    );
    assert_eq!(
        color_string(&shape.style.stroke),
        color_of(&expected["stroke"]),
        "{file}: {what} stroke"
    );
    // A path with no stroke carries no `stroke-width` attribute upstream.
    if color_of(&expected["stroke"]).is_some() {
        close(
            shape.style.stroke_width,
            expected["strokeWidth"].as_f64().expect("stroke width"),
            &format!("{what} stroke width"),
            file,
        );
    }
}

fn check_marker(scene: &Scene, expected: &Value, file: &str) {
    let kind = expected["kind"].as_str().expect("marker kind");
    let id = format!("{kind}-{DIAGRAM_ID}");
    let marker = scene
        .marker_def(&id)
        .unwrap_or_else(|| panic!("{file}: missing marker {id}"));
    close(
        marker.view_box.x,
        expected["viewBox"]["x"].as_f64().unwrap(),
        "marker viewBox x",
        file,
    );
    close(
        marker.view_box.y,
        expected["viewBox"]["y"].as_f64().unwrap(),
        "marker viewBox y",
        file,
    );
    close(
        marker.view_box.width,
        expected["viewBox"]["width"].as_f64().unwrap(),
        "marker viewBox width",
        file,
    );
    close(
        marker.view_box.height,
        expected["viewBox"]["height"].as_f64().unwrap(),
        "marker viewBox height",
        file,
    );
    close(
        marker.reference.x,
        expected["refX"].as_f64().unwrap(),
        "marker refX",
        file,
    );
    close(
        marker.reference.y,
        expected["refY"].as_f64().unwrap(),
        "marker refY",
        file,
    );
    close(
        marker.width,
        expected["markerWidth"].as_f64().unwrap(),
        "marker width",
        file,
    );
    close(
        marker.height,
        expected["markerHeight"].as_f64().unwrap(),
        "marker height",
        file,
    );
    let orient = match expected["orient"].as_str().unwrap_or("auto") {
        "auto-start-reverse" => MarkerOrient::AutoStartReverse,
        _ => MarkerOrient::Auto,
    };
    assert_eq!(marker.orient, orient, "{file}: marker orient");
    let shape = marker
        .shapes
        .first()
        .unwrap_or_else(|| panic!("{file}: empty marker"));
    check_path(shape, &expected["path"], &format!("marker {kind}"), file);
}

fn check_node_group(group: &Group, expected: &Value, file: &str, index: usize) {
    assert_eq!(
        group.class.as_deref(),
        expected["className"].as_str(),
        "{file}: node {index} class"
    );
    let shapes = expected["shapes"].as_array().expect("node shapes");
    assert_eq!(
        group.items.len(),
        shapes.len() + 1,
        "{file}: node {index} item count"
    );
    for (shape_index, shape) in shapes.iter().enumerate() {
        let what = format!("node {index} shape {shape_index}");
        let item = &group.items[shape_index];
        match shape["kind"].as_str().expect("shape kind") {
            "circle" => check_circle(circle(item, file, &what), shape, &what, file),
            "rect" => check_rect(rect(item, file, &what), shape, &what, file),
            "line" => check_line(path(item, file, &what), shape, &what, file),
            "path" => check_path(path(item, file, &what), shape, &what, file),
            other => panic!("{file}: {what}: unexpected shape kind {other}"),
        }
    }
    let label = expected["label"]
        .as_object()
        .unwrap_or_else(|| panic!("{file}: node {index} label"));
    assert_eq!(
        label["order"].as_u64(),
        Some(shapes.len() as u64),
        "{file}: node {index} label order"
    );
    let last = group.items.last().expect("node label");
    let what = format!("node {index} label");
    check_text(
        text(last, file, &what),
        &Value::Object(label.clone()),
        &what,
        file,
    );
}

#[test]
fn renders_recorded_wardley_fixtures() {
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

        let root = group(&scene.items[0], &file, "root");
        assert_eq!(
            root.class.as_deref(),
            Some("wardley-map"),
            "{file}: root class"
        );

        let background = rect(&root.items[0], &file, "background");
        close(
            background.rect.width,
            expected["background"]["width"]
                .as_f64()
                .expect("background width"),
            "background width",
            &file,
        );
        close(
            background.rect.height,
            expected["background"]["height"]
                .as_f64()
                .expect("background height"),
            "background height",
            &file,
        );
        assert_eq!(
            color_string(&background.style.fill),
            expected["background"]["fill"].as_str(),
            "{file}: background fill"
        );

        // The title is the only text directly under the map group.
        let title = root.items.iter().find_map(|item| match item {
            Item::Text(text) => Some(text),
            _ => None,
        });
        match expected["title"].as_object() {
            Some(title_expected) => {
                let title = title.unwrap_or_else(|| panic!("{file}: missing title"));
                check_text(
                    title,
                    &Value::Object(title_expected.clone()),
                    "title",
                    &file,
                );
            }
            None => assert!(title.is_none(), "{file}: unexpected title"),
        }

        let axes = required_group(&root.items, "wardley-axes", &file);
        let expected_axes = &expected["axes"];
        let axis_lines = expected_axes["lines"].as_array().expect("axis lines");
        assert_eq!(axes.items.len(), axis_lines.len() + 2, "{file}: axes count");
        for (index, expected) in axis_lines.iter().enumerate() {
            check_line(
                path(&axes.items[index], &file, "axis line"),
                expected,
                &format!("axis line {index}"),
                &file,
            );
        }
        let axis_labels = expected_axes["labels"].as_array().expect("axis labels");
        for (index, expected) in axis_labels.iter().enumerate() {
            check_text(
                text(&axes.items[axis_lines.len() + index], &file, "axis label"),
                expected,
                &format!("axis label {index}"),
                &file,
            );
        }

        let stages = required_group(&root.items, "wardley-stages", &file);
        let expected_stages = &expected["stages"];
        let stage_labels = expected_stages["labels"].as_array().expect("stage labels");
        let stage_dividers = expected_stages["dividers"]
            .as_array()
            .expect("stage dividers");
        let mut item_index = 0;
        for (index, expected) in stage_labels.iter().enumerate() {
            if index > 0 {
                check_line(
                    path(&stages.items[item_index], &file, "stage divider"),
                    &stage_dividers[index - 1],
                    &format!("stage divider {index}"),
                    &file,
                );
                item_index += 1;
            }
            check_text(
                text(&stages.items[item_index], &file, "stage label"),
                expected,
                &format!("stage label {index}"),
                &file,
            );
            item_index += 1;
        }

        let grid = expected["grid"].as_array().expect("grid");
        match find_group(&root.items, "wardley-grid") {
            Some(group) => {
                assert_eq!(group.items.len(), grid.len(), "{file}: grid line count");
                for (index, expected) in grid.iter().enumerate() {
                    check_line(
                        path(&group.items[index], &file, "grid line"),
                        expected,
                        &format!("grid line {index}"),
                        &file,
                    );
                }
            }
            None => assert!(grid.is_empty(), "{file}: missing grid"),
        }

        let expected_pipelines = &expected["pipelines"];
        let expected_boxes = expected_pipelines["boxes"]
            .as_array()
            .expect("pipeline boxes");
        let expected_pipeline_links = expected_pipelines["links"]
            .as_array()
            .expect("pipeline links");
        match find_group(&root.items, "wardley-pipelines") {
            Some(group) => {
                assert_eq!(
                    group.items.len(),
                    expected_boxes.len(),
                    "{file}: pipeline boxes"
                );
                for (index, expected) in expected_boxes.iter().enumerate() {
                    check_rect(
                        rect(&group.items[index], &file, "pipeline box"),
                        expected,
                        &format!("pipeline box {index}"),
                        &file,
                    );
                }
            }
            None => assert!(expected_boxes.is_empty(), "{file}: missing pipeline group"),
        }
        match find_group(&root.items, "wardley-pipeline-links") {
            Some(group) => {
                assert_eq!(
                    group.items.len(),
                    expected_pipeline_links.len(),
                    "{file}: pipeline links"
                );
                for (index, expected) in expected_pipeline_links.iter().enumerate() {
                    check_line(
                        path(&group.items[index], &file, "pipeline link"),
                        expected,
                        &format!("pipeline link {index}"),
                        &file,
                    );
                }
            }
            None => assert!(
                expected_pipeline_links.is_empty(),
                "{file}: missing pipeline links group"
            ),
        }

        let links = required_group(&root.items, "wardley-links", &file);
        let expected_links = expected["links"].as_array().expect("links");
        let expected_labels = expected["linkLabels"].as_array().expect("link labels");
        assert_eq!(
            links.items.len(),
            expected_links.len() + expected_labels.len(),
            "{file}: links count"
        );
        for (index, expected) in expected_links.iter().enumerate() {
            check_line(
                path(&links.items[index], &file, "link"),
                expected,
                &format!("link {index}"),
                &file,
            );
        }
        for (index, expected) in expected_labels.iter().enumerate() {
            check_text(
                text(
                    &links.items[expected_links.len() + index],
                    &file,
                    "link label",
                ),
                expected,
                &format!("link label {index}"),
                &file,
            );
        }

        let trends = required_group(&root.items, "wardley-trends", &file);
        let expected_trends = expected["trends"].as_array().expect("trends");
        assert_eq!(
            trends.items.len(),
            expected_trends.len(),
            "{file}: trends count"
        );
        for (index, expected) in expected_trends.iter().enumerate() {
            check_line(
                path(&trends.items[index], &file, "trend"),
                expected,
                &format!("trend {index}"),
                &file,
            );
        }

        let nodes = required_group(&root.items, "wardley-nodes", &file);
        let expected_nodes = expected["nodes"].as_array().expect("nodes");
        assert_eq!(
            nodes.items.len(),
            expected_nodes.len(),
            "{file}: node count"
        );
        for (index, expected) in expected_nodes.iter().enumerate() {
            check_node_group(
                group(&nodes.items[index], &file, "node"),
                expected,
                &file,
                index,
            );
        }

        let expected_annotations = &expected["annotations"];
        let expected_marks = expected_annotations["marks"]
            .as_array()
            .expect("annotation marks");
        let expected_annotation_lines = expected_annotations["lines"]
            .as_array()
            .expect("annotation lines");
        match find_group(&root.items, "wardley-annotations") {
            Some(annotations) => {
                let mut item_index = 0;
                for (index, expected) in expected_annotation_lines.iter().enumerate() {
                    check_line(
                        path(&annotations.items[item_index], &file, "annotation line"),
                        expected,
                        &format!("annotation line {index}"),
                        &file,
                    );
                    item_index += 1;
                }
                for (index, expected) in expected_marks.iter().enumerate() {
                    let mark = group(&annotations.items[item_index], &file, "annotation mark");
                    assert_eq!(
                        mark.class.as_deref(),
                        Some("wardley-annotation"),
                        "{file}: annotation mark class"
                    );
                    check_circle(
                        circle(&mark.items[0], &file, "annotation circle"),
                        &expected["circle"],
                        &format!("annotation mark {index}"),
                        &file,
                    );
                    check_text(
                        text(&mark.items[1], &file, "annotation number"),
                        &expected["text"],
                        &format!("annotation number {index}"),
                        &file,
                    );
                    item_index += 1;
                }
                match expected_annotations["box"].as_object() {
                    Some(box_expected) => {
                        let text_box =
                            group(&annotations.items[item_index], &file, "annotations box");
                        assert_eq!(
                            text_box.class.as_deref(),
                            Some("wardley-annotations-box"),
                            "{file}: annotations box class"
                        );
                        check_rect(
                            rect(&text_box.items[0], &file, "annotations box rect"),
                            &box_expected["rect"],
                            "annotations box rect",
                            &file,
                        );
                        let expected_texts = box_expected["texts"]
                            .as_array()
                            .expect("annotations box texts");
                        assert_eq!(
                            text_box.items.len(),
                            expected_texts.len() + 1,
                            "{file}: annotations box items"
                        );
                        for (index, expected) in expected_texts.iter().enumerate() {
                            check_text(
                                text(&text_box.items[index + 1], &file, "annotations box text"),
                                expected,
                                &format!("annotations box text {index}"),
                                &file,
                            );
                        }
                    }
                    None => assert_eq!(
                        annotations.items.len(),
                        item_index,
                        "{file}: unexpected annotations box"
                    ),
                }
            }
            None => assert!(
                expected_marks.is_empty() && expected_annotation_lines.is_empty(),
                "{file}: missing annotations group"
            ),
        }

        let notes = expected["notes"].as_array().expect("notes");
        match find_group(&root.items, "wardley-notes") {
            Some(group) => {
                assert_eq!(group.items.len(), notes.len(), "{file}: notes count");
                for (index, expected) in notes.iter().enumerate() {
                    check_text(
                        text(&group.items[index], &file, "note"),
                        expected,
                        &format!("note {index}"),
                        &file,
                    );
                }
            }
            None => assert!(notes.is_empty(), "{file}: missing notes group"),
        }

        for (class, key) in [
            ("wardley-accelerators", "accelerators"),
            ("wardley-deaccelerators", "deaccelerators"),
        ] {
            let expected_paths = expected[key]["paths"].as_array().expect("arrow paths");
            let expected_labels = expected[key]["labels"].as_array().expect("arrow labels");
            match find_group(&root.items, class) {
                Some(group) => {
                    assert_eq!(
                        group.items.len(),
                        expected_paths.len() + expected_labels.len(),
                        "{file}: {key} count"
                    );
                    for (index, expected) in expected_paths.iter().enumerate() {
                        check_path(
                            path(&group.items[index], &file, key),
                            expected,
                            &format!("{key} path {index}"),
                            &file,
                        );
                    }
                    for (index, expected) in expected_labels.iter().enumerate() {
                        check_text(
                            text(&group.items[expected_paths.len() + index], &file, key),
                            expected,
                            &format!("{key} label {index}"),
                            &file,
                        );
                    }
                }
                None => assert!(
                    expected_paths.is_empty() && expected_labels.is_empty(),
                    "{file}: missing {class}"
                ),
            }
        }

        let markers = expected["markers"].as_array().expect("markers");
        assert_eq!(scene.defs.len(), markers.len(), "{file}: marker count");
        for expected in markers {
            check_marker(&scene, expected, &file);
        }
    }
}

#[test]
fn renders_a_basic_map() {
    let scene = super::render(
        "wardley-beta\ntitle Tea\ncomponent Tea [0.5, 0.5]\ncomponent Water [0.3, 0.4]\nTea -> Water\n",
        &Config::default(),
    )
    .expect("render wardley");
    assert!(scene.view_box.width > 0.0);
    assert!(scene.view_box.height > 0.0);
    assert!(!scene.items.is_empty());
}

#[test]
fn dispatches_through_render_diagram() {
    let scene = crate::render_diagram(
        "wardley-beta\ntitle Tea\ncomponent Tea [0.5, 0.5]\n",
        &Config::default(),
    )
    .expect("render_diagram draws wardley maps");
    assert!(!scene.items.is_empty());
}

#[test]
fn draws_the_grid_when_the_configuration_asks_for_it() {
    let mut config = Config::default();
    config.wardley_beta.show_grid = true;
    let scene =
        super::render("wardley-beta\ncomponent A [0.5, 0.5]\n", &config).expect("render wardley");
    let root = group(&scene.items[0], "scene", "root");
    let grid = find_group(&root.items, "wardley-grid").expect("grid group");
    assert_eq!(grid.items.len(), 6);
    // The default canvas: padding 48, chart width 804, chart height 504.
    let expected: [(f64, f64, f64, f64); 6] = [
        (249.0, 48.0, 249.0, 552.0),
        (48.0, 426.0, 852.0, 426.0),
        (450.0, 48.0, 450.0, 552.0),
        (48.0, 300.0, 852.0, 300.0),
        (651.0, 48.0, 651.0, 552.0),
        (48.0, 174.0, 852.0, 174.0),
    ];
    for (index, (x1, y1, x2, y2)) in expected.iter().enumerate() {
        let line = path(&grid.items[index], "scene", "grid line");
        match &line.path.commands[..] {
            [
                PathCommand::MoveTo { x, y },
                PathCommand::LineTo { x: ex, y: ey },
            ] => {
                close(*x, *x1, "grid x1", "scene");
                close(*y, *y1, "grid y1", "scene");
                close(*ex, *x2, "grid x2", "scene");
                close(*ey, *y2, "grid y2", "scene");
            }
            other => panic!("unexpected grid commands {other:?}"),
        }
    }
}

#[test]
fn rejects_a_body_without_a_wardley_header() {
    let error = super::render("component A [0.5, 0.5]\n", &Config::default()).unwrap_err();
    assert!(
        error.message.contains("expected KW_WARDLEY"),
        "unexpected message: {}",
        error.message
    );
}
