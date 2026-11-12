//! Source-to-[`Scene`] rendering for use case diagrams: the Rust side of
//! `usecaseRenderer.ts` `draw`.
//!
//! `draw` parses the text into the use case database, asks `getData()` for the layout nodes
//! and edges (see [`super::layout_data`]), copies the configuration it applies (`type`,
//! `layoutAlgorithm`, `direction`, spacing, fonts, `markers`, `diagramId`, padding,
//! `useMaxWidth`), prepares the labels, runs the unified pipeline, and fits the viewport
//! with `setupViewPortForSVG`. Here those are [`super::parse`], [`layout_data`],
//! [`pipeline::render`] (which includes the viewport fitted with the layout data's padding
//! and `useMaxWidth`).
//!
//! The pipeline paints node shapes, cluster frames and edge geometry. This renderer then
//! applies the use case stylesheet to the edges — `lineColor` for the stroke, the 3/3 dash
//! of `include`, `extend`, and `note` relationships, and the `point`/`circle`/`cross`/
//! `extension` markers — and paints the relationship labels at the stylesheet's 10px. It
//! also names each path's source edge, so a painter can hit-test it.
//!
//! Where the port stops short of upstream, the difference is in the painter-facing scene the
//! pipeline already documents: HTML labels (`foreignObject`) are plain text runs, the
//! accessible names, `data-usecase-*` attributes and `usecase-stereotype` span belong to the
//! DOM the scene does not carry, the SVG title is not inserted, and the hand-drawn
//! background/rough.js pass is not painted.

use crate::config::Config;
use crate::error::ParseError;
use crate::measure::{FallbackMeasure, Font, TextMeasure, decode_html_entities, plain_text_block};
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

/// Renders a use case body (the text [`super::parse`] accepts) to a [`Scene`].
///
/// `config` is the diagram configuration after the source's frontmatter and directives have
/// been applied, as `getConfig()` returns it to `draw`.
pub fn draw(
    body: &str,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<Scene, UsecaseRenderError> {
    let ast = super::parse(body)?;
    draw_ast(&ast, config, measure)
}

/// Renders an already parsed use case diagram.
pub fn draw_ast(
    ast: &Ast,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<Scene, UsecaseRenderError> {
    draw_layout_ast(ast, config, measure).map(|(_, scene)| scene)
}

/// Renders an already parsed use case diagram and keeps the laid-out [`LayoutData`], whose
/// nodes and edges carry the positions and points the scene was painted from.
pub fn draw_layout_ast(
    ast: &Ast,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<(LayoutData, Scene), UsecaseRenderError> {
    let mut data = layout_data(ast, config, DEFAULT_DIAGRAM_ID);
    let mut scene = pipeline::render(&mut data, measure).map_err(UsecaseRenderError::Layout)?;
    complete_edges(&mut scene, &data, measure);
    Ok((data, scene))
}

/// Renders a use case body with the crate's monospace fallback measurement.
pub fn render(body: &str, config: &Config) -> Result<Scene, UsecaseRenderError> {
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

fn theme_color(theme: &ThemeVariables, name: &str, fallback: &str) -> Color {
    theme
        .color(name)
        .map(|value| Color::new(value.to_string()))
        .unwrap_or_else(|| Color::new(fallback))
}

/// The use case stylesheet's relationship stroke: `lineColor`, one pixel, no fill.
fn relationship_style(theme: &ThemeVariables) -> PathStyle {
    PathStyle {
        fill: None,
        stroke: Some(theme_color(theme, "lineColor", "#333")),
        stroke_width: 1.0,
        ..PathStyle::default()
    }
}

/// `.relationship-include`, `.relationship-extend` and `.relationship-note` carry
/// `stroke-dasharray: 3`.
fn is_dashed(edge: &pipeline::Edge) -> bool {
    edge.classes.as_deref().is_some_and(|classes| {
        classes.split_whitespace().any(|class| {
            matches!(
                class,
                "relationship-include" | "relationship-extend" | "relationship-note"
            )
        })
    })
}

/// Completes the pipeline's edge paths with the use case stylesheet and paints the
/// relationship labels. The pipeline pushes one [`Item::Path`] per layout edge, in edge order.
fn complete_edges(scene: &mut Scene, data: &LayoutData, measure: &dyn TextMeasure) {
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
    let label_color = theme_color(&theme, "primaryTextColor", "#333");
    let label_background = theme
        .color("edgeLabelBackground")
        .map(|value| Color::new(value.to_string()));
    let stroke = relationship_style(&theme);
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
        if is_dashed(edge) {
            path.style.dash = vec![3.0, 3.0];
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
                path.start_markers
                    .push(if let Some(color) = marker_color.clone() {
                        marker.with_color(color)
                    } else {
                        marker
                    });
            }
            if let Some(marker) = placement.end {
                path.end_markers
                    .push(if let Some(color) = marker_color.clone() {
                        marker.with_color(color)
                    } else {
                        marker
                    });
            }
        }

        let edge_data = EdgeData {
            id: edge.id.clone(),
            points: edge.points.clone().unwrap_or_default(),
            label: edge.label.as_deref().map(decode_html_entities),
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
            group.transform = crate::scene::Transform::translate(bounds.x, bounds.y);
            if label.background.is_some() {
                // `createFormattedText` already measured the edge label with two pixels of
                // background padding on each side, so the background is this box.
                let background_style = PathStyle {
                    fill: label_background.clone(),
                    stroke: None,
                    ..PathStyle::default()
                };
                group.push(RectShape::new(
                    Rect::new(0.0, 0.0, bounds.width, bounds.height),
                    0.0,
                    background_style,
                ));
            }
            let block = plain_text_block(&label.text, &font, measure);
            // The label group is anchored at the background's top-left, so centre the text
            // in the box and give it `createText`'s first-baseline offset below the origin.
            let x = bounds.width / 2.0;
            let y = (bounds.height - block.size.height) / 2.0 + font.size - 10.1;
            let mut text = TextRun::centered(Point::new(x, y), font.clone(), block);
            text.fill = Some(label_color.clone());
            group.push(Item::Text(text));
            labels.push(Item::Group(group));
        }
    }
    scene.items.extend(labels);
}

/// What can stop a use case render.
#[derive(Clone, Debug, PartialEq)]
pub enum UsecaseRenderError {
    Parse(ParseError),
    Layout(LayoutError),
}

impl std::fmt::Display for UsecaseRenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(error) => error.fmt(f),
            Self::Layout(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for UsecaseRenderError {}

impl From<ParseError> for UsecaseRenderError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

impl From<LayoutError> for UsecaseRenderError {
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
            .join("src/diagrams/usecase/tests/render");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut fixtures: Vec<(String, Value)> = entries
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
    fn renders_recorded_usecase_fixtures() {
        let fixtures = fixtures();
        assert!(
            fixtures.len() >= 8,
            "expected at least 8 fixtures, found {}",
            fixtures.len()
        );

        for (file, fixture) in fixtures {
            let source = fixture["source"].as_str().expect("fixture source");
            let body = crate::preprocess::preprocess(source).body;
            let ast =
                super::super::parse(&body).unwrap_or_else(|error| panic!("{file}: parse: {error}"));
            let (data, _scene) = draw_layout_ast(&ast, &Config::default(), &FallbackMeasure)
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
                let expected_points = edge["points"].as_array().expect("edge points");
                let points = actual.points.clone().unwrap_or_default();
                assert_eq!(
                    points.len(),
                    expected_points.len(),
                    "{file}: edge {id} point count"
                );
                for (index, point) in expected_points.iter().enumerate() {
                    let x = point["x"].as_f64().expect("point x");
                    let y = point["y"].as_f64().expect("point y");
                    close(
                        points[index].x,
                        x,
                        &format!("edge {id} point {index} x"),
                        &file,
                    );
                    close(
                        points[index].y,
                        y,
                        &format!("edge {id} point {index} y"),
                        &file,
                    );
                }
            }

            let expected_view = &expected["viewBox"];
            let view = scene_view_box(&_scene);
            close(
                view.0,
                expected_view["x"].as_f64().expect("viewBox x"),
                "viewBox x",
                &file,
            );
            close(
                view.1,
                expected_view["y"].as_f64().expect("viewBox y"),
                "viewBox y",
                &file,
            );
            close(
                view.2,
                expected_view["width"].as_f64().expect("viewBox width"),
                "viewBox width",
                &file,
            );
            close(
                view.3,
                expected_view["height"].as_f64().expect("viewBox height"),
                "viewBox height",
                &file,
            );
        }
    }

    fn scene_view_box(scene: &Scene) -> (f64, f64, f64, f64) {
        let view = scene.view_box;
        (view.x, view.y, view.width, view.height)
    }
}
