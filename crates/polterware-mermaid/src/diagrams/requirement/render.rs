//! Source-to-[`Scene`] rendering for requirement diagrams: the Rust side of
//! `requirementRenderer.ts` `draw`.
//!
//! `draw` parses the text into the [`Db`](super::db), asks `getData()` for the layout nodes
//! and edges (see [`Ast::data`]), copies the configuration `draw` applies (`type`,
//! `layoutAlgorithm`, `direction`, `nodeSpacing`, `rankSpacing`, `markers`, `diagramId`),
//! runs the unified pipeline, and fits the viewport with `setupViewPortForSVG`. Here those
//! are [`super::parse`], [`super::layout_data::layout_data`], [`pipeline::render`] (which
//! includes the viewport fitted with the layout data's padding and `useMaxWidth`).
//!
//! The pipeline paints node boxes and edge geometry. This renderer then applies what the
//! requirement stylesheet gives those edges — `relationColor` for the stroke, the 10/7 dash
//! of a non-`contains` relationship, and the `requirement_contains`/`requirement_arrow`
//! markers — and paints the `<<relationship>>` edge labels, which the shared pipeline leaves
//! to the diagram. It also names each path's source edge, so a painter can hit-test it.
//!
//! Where the port stops short of upstream, the difference is in the painter-facing scene the
//! pipeline already documents: HTML labels (`foreignObject`) are plain text runs, the SVG
//! title is not inserted (the scene carries the diagram, not the page around it), and the
//! hand-drawn background/rough.js pass is not painted.

use crate::config::Config;
use crate::error::ParseError;
use crate::measure::{FallbackMeasure, Font, TextMeasure, plain_text_block};
use crate::render::edges::{EdgeData, insert_edge_label};
use crate::render::layout_dagre::LayoutError;
use crate::render::markers::marker_placement;
use crate::render::pipeline;
use crate::render::pipeline::LayoutData;
use crate::scene::{
    Color, Group, Item, PathStyle, Point, Rect, RectShape, Scene, SourceRef, TextRun,
};
use crate::theme::{Theme, ThemeVariables};

use super::ast::Ast;
use super::layout_data::{DEFAULT_DIAGRAM_ID, DIAGRAM_TYPE, layout_data};

/// Renders a requirement body (the text [`super::parse`] accepts) to a [`Scene`].
///
/// `config` is the diagram configuration after the source's frontmatter and directives have
/// been applied, as `getConfig()` returns it to `draw`.
pub fn draw(
    body: &str,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<Scene, RequirementRenderError> {
    let ast = super::parse(body)?;
    draw_ast(&ast, config, measure)
}

/// Renders an already parsed requirement diagram.
pub fn draw_ast(
    ast: &Ast,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<Scene, RequirementRenderError> {
    draw_layout_ast(ast, config, measure).map(|(_, scene)| scene)
}

/// Renders an already parsed requirement diagram and keeps the laid-out [`LayoutData`], whose
/// nodes and edges carry the positions and points the scene was painted from.
pub fn draw_layout_ast(
    ast: &Ast,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<(LayoutData, Scene), RequirementRenderError> {
    let mut data = layout_data(ast, config, DEFAULT_DIAGRAM_ID);
    let mut scene = pipeline::render(&mut data, measure).map_err(RequirementRenderError::Layout)?;
    complete_edges(&mut scene, &data, config, measure);
    Ok((data, scene))
}

/// Renders a requirement body with the crate's monospace fallback measurement.
pub fn render(body: &str, config: &Config) -> Result<Scene, RequirementRenderError> {
    draw(body, config, &FallbackMeasure)
}

/// The theme the pipeline measures and paints with (`dagre_render` installs the same
/// monospace 16 override).
fn theme_variables() -> ThemeVariables {
    let mut overrides = serde_json::Map::new();
    overrides.insert("fontFamily".into(), serde_json::json!("monospace"));
    overrides.insert("fontSize".into(), serde_json::json!("16px"));
    Theme::Default.variables(&overrides).unwrap_or_default()
}

/// `styles.js`': edge strokes use `relationColor`, and the classic `look` keeps a 1px width.
fn relation_style(theme: &ThemeVariables, config: &Config) -> PathStyle {
    let color = theme
        .color("relationColor")
        .map(|value| Color::new(value.to_string()))
        .unwrap_or_else(|| Color::new("#666"));
    let width = if config.look == "neo" {
        theme.number("strokeWidth").unwrap_or(1.0)
    } else {
        1.0
    };
    PathStyle {
        fill: None,
        stroke: Some(color),
        stroke_width: width,
        ..PathStyle::default()
    }
}

/// Completes the pipeline's edge paths with the requirement stylesheet and paints the edge
/// labels. The pipeline pushes one [`Item::Path`] per layout edge, in edge order.
fn complete_edges(
    scene: &mut Scene,
    data: &LayoutData,
    config: &Config,
    measure: &dyn TextMeasure,
) {
    let theme = theme_variables();
    let diagram_id = data
        .diagram_id
        .as_deref()
        .unwrap_or(DEFAULT_DIAGRAM_ID)
        .to_string();
    let font = Font {
        family: theme.color("fontFamily").unwrap_or("monospace").to_string(),
        size: theme
            .color("fontSize")
            .and_then(|value| value.trim_end_matches("px").parse::<f64>().ok())
            .or_else(|| theme.number("fontSize"))
            .unwrap_or(16.0),
        weight: crate::scene::NORMAL_WEIGHT,
        italic: false,
    };
    let label_color = theme
        .color("relationLabelColor")
        .map(|value| Color::new(value.to_string()))
        .unwrap_or_else(|| Color::new("#333"));
    let label_background = theme
        .color("edgeLabelBackground")
        .map(|value| Color::new(value.to_string()));
    let stroke = relation_style(&theme, config);
    let marker_color = stroke.stroke.clone();

    let mut labels: Vec<Item> = Vec::new();
    let mut path_index = 0;
    for item in &mut scene.items {
        let Item::Path(path) = item else {
            continue;
        };
        let Some(edge) = data.edges.get(path_index) else {
            break;
        };
        path_index += 1;
        path.source = Some(SourceRef::edge(edge.id.clone()));
        path.style = stroke.clone();
        if edge.pattern.as_deref() == Some("dashed") {
            path.style.dash = vec![10.0, 7.0];
        }
        if let Ok(placement) = marker_placement(
            edge.arrow_type_start
                .as_deref()
                .filter(|value| !value.is_empty()),
            edge.arrow_type_end
                .as_deref()
                .filter(|value| !value.is_empty()),
            &diagram_id,
            DIAGRAM_TYPE,
            false,
        ) {
            if let Some(marker) = placement.start {
                path.start_markers.push(if let Some(color) = marker_color.clone() {
                    marker.with_color(color)
                } else {
                    marker
                });
            }
            if let Some(marker) = placement.end {
                path.end_markers.push(if let Some(color) = marker_color.clone() {
                    marker.with_color(color)
                } else {
                    marker
                });
            }
        }

        let edge_data = EdgeData {
            id: edge.id.clone(),
            points: edge.points.clone().unwrap_or_default(),
            label: edge.label.clone(),
            label_width: edge.width.unwrap_or(0.0),
            label_height: edge.height.unwrap_or(0.0),
            label_x: edge.x,
            label_y: edge.y,
            thickness: edge.thickness.clone().unwrap_or_default(),
            pattern: edge.pattern.clone().unwrap_or_default(),
            arrow_type_start: edge.arrow_type_start.clone(),
            arrow_type_end: edge.arrow_type_end.clone(),
            ..Default::default()
        };
        for label in insert_edge_label(&edge_data) {
            let bounds = label.position.bounds;
            let mut group = Group::new()
                .with_class("edgeLabel")
                .with_source(SourceRef::edge(edge.id.clone()));
            group.transform =
                crate::scene::Transform::translate(bounds.x, bounds.y);
            if let Some(background) = label.background {
                let mut background_style = PathStyle::default();
                background_style.fill = label_background.clone();
                background_style.stroke = None;
                group.push(RectShape::new(
                    Rect::new(
                        background.x - bounds.x,
                        background.y - bounds.y,
                        background.width,
                        background.height,
                    ),
                    0.0,
                    background_style,
                ));
            }
            let block = plain_text_block(&label.text, &font, measure);
            let mut text = TextRun::new(Point::ZERO, font.clone(), block);
            text.fill = Some(label_color.clone());
            group.push(Item::Text(text));
            labels.push(Item::Group(group));
        }
    }
    scene.items.extend(labels);
}

/// What can stop a requirement render.
#[derive(Clone, Debug, PartialEq)]
pub enum RequirementRenderError {
    Parse(ParseError),
    Layout(LayoutError),
}

impl std::fmt::Display for RequirementRenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(error) => error.fmt(f),
            Self::Layout(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for RequirementRenderError {}

impl From<ParseError> for RequirementRenderError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

impl From<LayoutError> for RequirementRenderError {
    fn from(error: LayoutError) -> Self {
        Self::Layout(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const TOLERANCE: f64 = 0.01;

    fn fixtures() -> Vec<(String, Value)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/diagrams/requirement/tests/render");
        let mut fixtures: Vec<(String, Value)> = std::fs::read_dir(&dir)
            .unwrap_or_else(|error| panic!("read {}: {error}", dir.display()))
            .map(|entry| entry.expect("dir entry").path())
            .filter(|path| {
                path.extension().and_then(|extension| extension.to_str()) == Some("json")
            })
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
    fn renders_recorded_requirement_fixtures() {
        let fixtures = fixtures();
        assert!(
            fixtures.len() >= 8,
            "expected at least 8 fixtures, found {}",
            fixtures.len()
        );

        for (file, fixture) in fixtures {
            let source = fixture["source"].as_str().expect("fixture source");
            let body = crate::preprocess::preprocess(source).body;
            let ast = super::super::parse(&body)
                .unwrap_or_else(|error| panic!("{file}: parse: {error}"));
            let (data, scene) = draw_layout_ast(&ast, &Config::default(), &FallbackMeasure)
                .unwrap_or_else(|error| panic!("{file}: render: {error}"));
            let expected = &fixture["expected"];

            for node in expected["nodes"].as_array().expect("expected nodes") {
                let id = node["id"].as_str().expect("node id");
                let actual = data
                    .nodes
                    .iter()
                    .find(|candidate| candidate.id == id)
                    .unwrap_or_else(|| panic!("{file}: node {id} in layout"));
                let (x, y, width, height) = box_of(node);
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

            for edge in expected["edges"].as_array().expect("expected edges") {
                let id = edge["id"].as_str().expect("edge id");
                let actual = data
                    .edges
                    .iter()
                    .find(|candidate| candidate.id == id)
                    .unwrap_or_else(|| panic!("{file}: edge {id} in layout"));
                let expected_points = edge["points"].as_array().expect("points");
                let actual_points = actual.points.as_ref().expect("edge points");
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
                for key in ["x", "y", "width", "height"] {
                    if let Some(expected_value) = edge[key].as_f64() {
                        let actual_value = match key {
                            "x" => actual.x,
                            "y" => actual.y,
                            "width" => actual.width,
                            "height" => actual.height,
                            _ => unreachable!(),
                        }
                        .expect("edge box field");
                        close(
                            actual_value,
                            expected_value,
                            &format!("edge {id} {key}"),
                            &file,
                        );
                    }
                }
            }

            let expected_view_box = &expected["viewBox"];
            close(
                scene.view_box.x,
                expected_view_box["x"].as_f64().expect("viewBox x"),
                "viewBox x",
                &file,
            );
            close(
                scene.view_box.y,
                expected_view_box["y"].as_f64().expect("viewBox y"),
                "viewBox y",
                &file,
            );
            close(
                scene.view_box.width,
                expected_view_box["width"].as_f64().expect("viewBox width"),
                "viewBox width",
                &file,
            );
            close(
                scene.view_box.height,
                expected_view_box["height"].as_f64().expect("viewBox height"),
                "viewBox height",
                &file,
            );
        }
    }

    #[test]
    fn draws_relationship_paths_with_markers_and_labels() {
        let scene = render(
            "requirementDiagram\n  requirement a {\n    id: 1\n    text: the text\n  }\n  requirement b {\n  }\n  a - satisfies -> b\n  a - contains -> b\n",
            &Config::default(),
        )
        .expect("render requirement diagram");

        let paths: Vec<_> = scene
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Path(path) => Some(path),
                _ => None,
            })
            .collect();
        assert_eq!(paths.len(), 2);
        assert!(paths[0].end_markers.len() == 1);
        assert_eq!(paths[0].style.dash, [10.0, 7.0]);
        assert!(paths[1].start_markers.len() == 1);
        assert!(paths[1].style.dash.is_empty());

        let label_texts: Vec<String> = scene
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Group(group) if group.class.as_deref() == Some("edgeLabel") => {
                    group.items.iter().find_map(|item| match item {
                        Item::Text(text) => {
                            Some(text.lines.iter().map(|line| line.text()).collect::<String>())
                        }
                        _ => None,
                    })
                }
                _ => None,
            })
            .collect();
        assert_eq!(label_texts, ["<<satisfies>>", "<<contains>>"]);
    }

    #[test]
    fn draws_a_rendered_scene_with_view_box() {
        let scene = render(
            "requirementDiagram\n  requirement a {\n    id: 1\n  }\n",
            &Config::default(),
        )
        .expect("render requirement diagram");
        assert!(scene.view_box.width > 0.0);
        assert!(scene.view_box.height > 0.0);
        assert!(!scene.items.is_empty());
    }

    #[test]
    fn rejects_a_body_without_a_requirement_header() {
        let error = render("not a requirement diagram", &Config::default()).unwrap_err();
        assert!(matches!(error, RequirementRenderError::Parse(_)));
    }
}
