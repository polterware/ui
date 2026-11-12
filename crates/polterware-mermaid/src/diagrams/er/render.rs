//! Source-to-[`Scene`] rendering for ER diagrams: the Rust side of
//! `erRenderer-unified.ts` `draw`.
//!
//! `draw` parses the text into the [`ErDB`](super::db), asks `getData()` for the layout
//! nodes and edges, copies the configuration `draw` applies (`type`, `layoutAlgorithm`,
//! `direction`, `nodeSpacing`, `rankSpacing`, `markers`, `diagramId`), runs the unified
//! pipeline, and sets the viewport with `setupViewPortForSVG`. Here those are
//! [`super::parse`], [`layout_data`], [`crate::render::pipeline::render`] (which includes the
//! viewport fitted with the layout data's padding and `useMaxWidth`).
//!
//! Where the port stops short of upstream, the difference is in the painter-facing scene the
//! pipeline already documents: HTML labels (`foreignObject`) are plain text runs, the SVG
//! title is not inserted (the scene carries the diagram, not the page around it), and the
//! hand-drawn background/rough.js pass is not painted.

use crate::config::Config;
use crate::error::ParseError;
use crate::measure::{FallbackMeasure, TextMeasure};
use crate::render::layout_dagre::LayoutError;
use crate::render::pipeline;
use crate::render::pipeline::LayoutData;
use crate::scene::Scene;

use super::ast::Ast;
use super::layout_data::{DEFAULT_DIAGRAM_ID, layout_data};

/// Renders an ER body (the text [`super::parse`] accepts) to a [`Scene`].
///
/// `config` is the diagram configuration after the source's frontmatter and directives have
/// been applied, as `getConfig()` returns it to `draw`.
pub fn draw(
    body: &str,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<Scene, ErRenderError> {
    let ast = super::parse(body)?;
    draw_ast(&ast, config, measure)
}

/// Renders an already parsed ER diagram.
pub fn draw_ast(
    ast: &Ast,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<Scene, ErRenderError> {
    draw_layout_ast(ast, config, measure).map(|(_, scene)| scene)
}

/// Renders an already parsed ER diagram and keeps the laid-out [`LayoutData`], whose nodes
/// and edges carry the positions and points the scene was painted from.
pub fn draw_layout_ast(
    ast: &Ast,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<(LayoutData, Scene), ErRenderError> {
    let mut data = layout_data(ast, config, DEFAULT_DIAGRAM_ID);
    let scene = pipeline::render(&mut data, measure).map_err(ErRenderError::Layout)?;
    Ok((data, scene))
}

/// Renders an ER body with the crate's monospace fallback measurement.
pub fn render(body: &str, config: &Config) -> Result<Scene, ErRenderError> {
    draw(body, config, &FallbackMeasure)
}

/// What can stop an ER render.
#[derive(Clone, Debug, PartialEq)]
pub enum ErRenderError {
    Parse(ParseError),
    Layout(LayoutError),
}

impl std::fmt::Display for ErRenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(error) => error.fmt(f),
            Self::Layout(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ErRenderError {}

impl From<ParseError> for ErRenderError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

impl From<LayoutError> for ErRenderError {
    fn from(error: LayoutError) -> Self {
        Self::Layout(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use serde_json::Value;

    const TOLERANCE: f64 = 0.01;

    fn fixtures() -> Vec<(String, Value)> {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/diagrams/er/tests/render");
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
    fn renders_recorded_er_fixtures() {
        let fixtures = fixtures();
        assert!(
            fixtures.len() >= 10,
            "expected at least 10 fixtures, found {}",
            fixtures.len()
        );

        for (file, fixture) in fixtures {
            let source = fixture["source"].as_str().expect("fixture source");
            let body = crate::preprocess::preprocess(source).body;
            let ast =
                super::super::parse(&body).unwrap_or_else(|error| panic!("{file}: parse: {error}"));
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
                expected_view_box["height"]
                    .as_f64()
                    .expect("viewBox height"),
                "viewBox height",
                &file,
            );
        }
    }

    #[test]
    fn draws_a_rendered_scene_with_view_box() {
        let scene = render("erDiagram\n  A ||--o{ B : has\n", &Config::default())
            .expect("render er diagram");
        assert!(scene.view_box.width > 0.0);
        assert!(scene.view_box.height > 0.0);
        assert!(!scene.items.is_empty());
    }

    #[test]
    fn rejects_a_body_without_an_er_header() {
        let error = render("not an er diagram", &Config::default()).unwrap_err();
        assert!(matches!(error, ErRenderError::Parse(_)));
    }
}
