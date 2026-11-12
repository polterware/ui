//! Source-to-[`Scene`] rendering for mindmap diagrams: the Rust side of
//! `mindmap/mindmapRenderer.ts` `draw` over the shared layout pipeline.
//!
//! `draw` parses the text into the [`MindmapDB`](super::db), asks `getData()` for the layout
//! nodes and edges, copies the configuration `draw` applies (the diagram type, the resolved
//! layout algorithm, the diagram id, the shape sizes and paddings), runs the unified
//! pipeline with cose-bilkent, and fits the viewport with `setupViewPortForSVG`. Here those
//! are [`super::parse`], [`layout_data`], [`crate::render::pipeline::render`], and the
//! painting below, which applies the mindmap stylesheet's section colors.
//!
//! The look is the compiled `mindmap/styles.ts` sheet: every node shape takes the fill of its
//! section (`.section-N rect`, `path`, `circle`, `polygon`), its label the section's label
//! color, the undelimited node's underline `cScaleInv`, and every edge its child section's
//! stroke color and its depth's stroke width (`.section-edge-N`, `.edge-depth-N`). The
//! `neo` look swaps the fill and stroke for the theme's `mainBkg`/`nodeBorder` as
//! `genSections` does. The gradient `draw` adds to `<defs>` for `useGradient` is a paint
//! server rather than an item, so the scene carries the colors themselves and not the
//! `<linearGradient>` element.
//!
//! Where the port stops short of upstream, the difference is in the painter-facing scene the
//! pipeline already documents: HTML labels (`foreignObject`) are plain text runs, and icons
//! (`::icon(...)`) are not painted, because the scene carries no font-awesome glyphs.

use std::collections::HashMap;

use crate::config::Config;
use crate::error::ParseError;
use crate::measure::{FallbackMeasure, TextMeasure};
use crate::render::layout_dagre::LayoutError;
use crate::render::pipeline;
use crate::render::pipeline::LayoutData;
use crate::scene::{Color, Group, Item, PathStyle, Scene};

use super::ast::{Ast, Data};
use super::layout_data::{DEFAULT_DIAGRAM_ID, color_by, layout_data_from};

/// Renders a mindmap body (the text [`super::parse`] accepts) to a [`Scene`].
///
/// `config` is the diagram configuration after the source's frontmatter and directives have
/// been applied, as `getConfig()` returns it to `draw`.
pub fn draw(
    body: &str,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<Scene, MindmapRenderError> {
    let ast = super::parse(body)?;
    draw_ast(&ast, config, measure)
}

/// Renders an already parsed mindmap.
pub fn draw_ast(
    ast: &Ast,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<Scene, MindmapRenderError> {
    draw_layout_ast(ast, config, measure).map(|(_, scene)| scene)
}

/// Renders an already parsed mindmap and keeps the laid-out [`LayoutData`], whose nodes and
/// edges carry the positions and points the scene was painted from.
pub fn draw_layout_ast(
    ast: &Ast,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<(LayoutData, Scene), MindmapRenderError> {
    let data = ast.data_with(color_by(config));
    let mut layout = layout_data_from(&data, config, DEFAULT_DIAGRAM_ID);
    let mut scene = pipeline::render(&mut layout, measure).map_err(MindmapRenderError::Layout)?;
    paint(&mut scene, &data, &layout, config);
    Ok((layout, scene))
}

/// Renders a mindmap body with the crate's monospace fallback measurement.
pub fn render(body: &str, config: &Config) -> Result<Scene, MindmapRenderError> {
    draw(body, config, &FallbackMeasure)
}

/// The palette values one scene is painted with, read from the configuration's theme
/// variables exactly as `styles.ts` interpolates them.
struct Palette<'a> {
    config: &'a Config,
    theme: String,
    look: String,
}

impl Palette<'_> {
    fn color(&self, key: &str) -> Option<Color> {
        self.config
            .theme_variables
            .get(key)
            .and_then(|value| value.as_str())
            .map(Color::new)
    }

    /// `cScale{n}`, `cScaleInv{n}`, or `cScaleLabel{n}` for section `n` (`section + 1` in the
    /// stylesheet's loop), and the `git` values for the root section.
    fn scaled(&self, prefix: &str, section: Option<usize>) -> Option<Color> {
        match section {
            None if prefix == "cScale" => self.color("git0"),
            None if prefix == "cScaleLabel" => self.color("gitBranchLabel0"),
            None => self.color(&format!("{prefix}0")),
            Some(section) => self.color(&format!("{prefix}{}", section + 1)),
        }
    }

    fn is_redux(&self) -> bool {
        self.theme.contains("redux") || self.theme == "neutral"
    }

    /// The shape fill: the section color, or `mainBkg` for the themes `genSections` swaps.
    fn shape_fill(&self, section: Option<usize>) -> Option<Color> {
        if self.look == "neo" && self.is_redux() {
            self.color("mainBkg")
        } else {
            self.scaled("cScale", section)
        }
    }

    /// The shape stroke: `none` in the classic look, the section color (or `nodeBorder` for
    /// the redux themes) in the neo look.
    fn shape_stroke(&self, section: Option<usize>) -> Option<Color> {
        if self.look != "neo" {
            return None;
        }
        if self.theme.contains("redux") {
            self.color("nodeBorder")
        } else {
            self.scaled("cScale", section)
        }
    }

    fn shape_stroke_width(&self) -> f64 {
        if self.look == "neo" {
            self.config
                .theme_variables
                .get("strokeWidth")
                .and_then(|value| value.as_f64())
                .unwrap_or(2.0)
        } else {
            1.0
        }
    }

    /// The text fill. The root reads `gitBranchLabel0`; the `neutral` theme's neo labels read
    /// `cScaleLabel1`, as the stylesheet's ternary does.
    fn label_fill(&self, section: Option<usize>) -> Option<Color> {
        match (self.look.as_str(), section) {
            ("neo", None) if self.theme.contains("redux") => self.color("nodeBorder"),
            ("neo", None) => self.color("cScaleLabel0"),
            ("neo", Some(_)) if self.theme.contains("redux") => self.color("nodeBorder"),
            ("neo", Some(_)) if self.theme == "neutral" => self.color("cScaleLabel1"),
            _ => self.scaled("cScaleLabel", section),
        }
    }

    /// `.section-N line`: the underline under an undelimited node.
    fn line_color(&self, section: Option<usize>) -> Option<Color> {
        self.scaled("cScaleInv", section)
    }

    /// `.section-N line { stroke-width: 3 }`, which the gradient variant zeroes.
    fn line_width(&self) -> f64 {
        let uses_gradient = self
            .config
            .theme_variables
            .get("useGradient")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        if uses_gradient { 0.0 } else { 3.0 }
    }

    fn edge_color(&self, section: Option<usize>) -> Option<Color> {
        if self.look == "neo" && (self.theme.contains("redux") || self.theme == "neo-dark") {
            self.color("nodeBorder")
        } else {
            self.scaled("cScale", section)
        }
    }

    /// `.edge-depth-{depth + 1}: stroke-width: {17 - 3 * (depth + 2)}`. A non-positive width
    /// is not a valid CSS value, so `.edge`'s 3 wins, exactly as the browser's cascade would.
    fn edge_width(&self, depth: i32) -> f64 {
        let width = 17.0 - 3.0 * f64::from(depth + 2);
        if width > 0.0 { width } else { 3.0 }
    }
}

/// Applies the mindmap stylesheet's colors to the pipeline's scene and orders the items as
/// the cose-bilkent renderer's groups do (edges below nodes).
fn paint(scene: &mut Scene, data: &Data, layout: &LayoutData, config: &Config) {
    let palette = Palette {
        config,
        theme: config.theme.to_lowercase(),
        look: config.look.clone(),
    };
    let sections: HashMap<&str, Option<usize>> = data
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node.section))
        .collect();

    let mut edges = Vec::new();
    let mut nodes = Vec::new();
    let mut edge_index = 0;
    let mut items = std::mem::take(&mut scene.items);
    for item in items.drain(..) {
        match item {
            Item::Path(mut path) => {
                if let Some(edge) = layout
                    .edges
                    .iter()
                    .filter(|edge| edge.is_layout_only != Some(true))
                    .nth(edge_index)
                {
                    path.style = PathStyle {
                        fill: None,
                        stroke: palette.edge_color(edge_section(edge.id.as_str(), data)),
                        stroke_width: palette.edge_width(edge.depth.unwrap_or(0.0) as i32),
                        ..PathStyle::default()
                    };
                }
                edge_index += 1;
                edges.push(Item::Path(path));
            }
            Item::Group(mut group) => {
                if let Some(id) = group.source.as_ref().map(|source| source.id.clone())
                    && let Some(section) = sections.get(id.as_str()).copied()
                {
                    paint_group(&mut group, section, &palette);
                }
                nodes.push(Item::Group(group));
            }
            other => nodes.push(other),
        }
    }

    edges.extend(nodes);
    scene.items = edges;
}

/// The section an edge is painted with (`edge.section`, the child's section).
fn edge_section(id: &str, data: &Data) -> Option<usize> {
    data.edges
        .iter()
        .find(|edge| edge.id == id)
        .and_then(|edge| edge.section)
}

fn paint_group(group: &mut Group, section: Option<usize>, palette: &Palette<'_>) {
    let is_line = group
        .class
        .as_deref()
        .is_some_and(|class| class.starts_with("node-line-"));
    for item in &mut group.items {
        match item {
            Item::Group(inner) => {
                if inner
                    .class
                    .as_deref()
                    .is_some_and(|class| class.starts_with("node-line-"))
                {
                    paint_line(inner, section, palette);
                } else {
                    paint_group(inner, section, palette);
                }
            }
            Item::Path(path) => {
                if is_line {
                    path.style = line_style(section, palette);
                } else {
                    paint_style(&mut path.style, section, palette);
                }
            }
            Item::Rect(rect) => paint_style(&mut rect.style, section, palette),
            Item::Ellipse(ellipse) => paint_style(&mut ellipse.style, section, palette),
            Item::Text(text) => text.fill = palette.label_fill(section),
            Item::Image(_) => {}
        }
    }
}

fn paint_line(group: &mut Group, section: Option<usize>, palette: &Palette<'_>) {
    for item in &mut group.items {
        match item {
            Item::Path(path) => path.style = line_style(section, palette),
            Item::Group(inner) => paint_line(inner, section, palette),
            _ => {}
        }
    }
}

fn line_style(section: Option<usize>, palette: &Palette<'_>) -> PathStyle {
    PathStyle {
        fill: None,
        stroke: palette.line_color(section),
        stroke_width: palette.line_width(),
        ..PathStyle::default()
    }
}

fn paint_style(style: &mut PathStyle, section: Option<usize>, palette: &Palette<'_>) {
    style.fill = palette.shape_fill(section);
    style.stroke = palette.shape_stroke(section);
    style.stroke_width = palette.shape_stroke_width();
}

/// What can stop a mindmap render.
#[derive(Clone, Debug, PartialEq)]
pub enum MindmapRenderError {
    Parse(ParseError),
    Layout(LayoutError),
}

impl std::fmt::Display for MindmapRenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(error) => error.fmt(f),
            Self::Layout(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for MindmapRenderError {}

impl From<ParseError> for MindmapRenderError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

impl From<LayoutError> for MindmapRenderError {
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
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/diagrams/mindmap/tests/render");
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

    /// `insertNode` rewrites a plain `rect` node to `squareRect` on the layout data before
    /// drawing; this port leaves the projection's `rect` in place.
    fn normalized_shape(shape: Option<&str>) -> Option<&str> {
        match shape {
            Some("rect") => Some("squareRect"),
            other => other,
        }
    }

    #[test]
    fn renders_recorded_mindmap_fixtures() {
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
            let (layout, scene) = draw_layout_ast(&ast, &Config::default(), &FallbackMeasure)
                .unwrap_or_else(|error| panic!("{file}: render: {error}"));
            let expected = &fixture["expected"];

            for node in expected["nodes"].as_array().expect("expected nodes") {
                let id = node["id"].as_str().expect("node id");
                let actual = layout
                    .nodes
                    .iter()
                    .find(|candidate| candidate.id == id)
                    .unwrap_or_else(|| panic!("{file}: node {id} in layout"));
                close(
                    actual.x.expect("x"),
                    node["x"].as_f64().expect("node x"),
                    &format!("node {id} x"),
                    &file,
                );
                close(
                    actual.y.expect("y"),
                    node["y"].as_f64().expect("node y"),
                    &format!("node {id} y"),
                    &file,
                );
                close(
                    actual.width.expect("width"),
                    node["width"].as_f64().expect("node width"),
                    &format!("node {id} width"),
                    &file,
                );
                close(
                    actual.height.expect("height"),
                    node["height"].as_f64().expect("node height"),
                    &format!("node {id} height"),
                    &file,
                );
                assert_eq!(
                    actual.css_classes.as_deref(),
                    node["classes"].as_str(),
                    "{file}: node {id} classes"
                );
                assert_eq!(
                    normalized_shape(actual.shape.as_deref()),
                    node["shape"].as_str(),
                    "{file}: node {id} shape"
                );
            }

            for edge in expected["edges"].as_array().expect("expected edges") {
                let id = edge["id"].as_str().expect("edge id");
                let actual = layout
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
    fn paints_the_section_palette() {
        let scene = render("mindmap\n  root((root))\n    branch\n      leaf\n", &Config::default())
            .expect("render mindmap");
        let theme = &Config::default().theme_variables;
        let color = |key: &str| theme.get(key).and_then(|value| value.as_str());

        let groups: Vec<&Group> = scene
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Group(group) => Some(group),
                _ => None,
            })
            .collect();
        assert_eq!(groups.len(), 3);

        let root = groups
            .iter()
            .find(|group| group.source.as_ref().is_some_and(|s| s.id == "0"))
            .expect("root group");
        assert_eq!(
            shape_fill(root),
            color("git0").map(str::to_string),
            "root fill"
        );
        assert_eq!(
            label_fill(root),
            color("gitBranchLabel0").map(str::to_string),
            "root label"
        );

        let branch = groups
            .iter()
            .find(|group| group.source.as_ref().is_some_and(|s| s.id == "1"))
            .expect("branch group");
        assert_eq!(
            shape_fill(branch),
            color("cScale1").map(str::to_string),
            "section 0 fill"
        );
        assert_eq!(
            label_fill(branch),
            color("cScaleLabel1").map(str::to_string),
            "section 0 label"
        );

        let leaf = groups
            .iter()
            .find(|group| group.source.as_ref().is_some_and(|s| s.id == "2"))
            .expect("leaf group");
        assert_eq!(
            shape_fill(leaf),
            color("cScale1").map(str::to_string),
            "a branch's descendants keep its section"
        );
    }

    fn shape_fill(group: &Group) -> Option<String> {
        fn first_style(items: &[Item]) -> Option<&PathStyle> {
            for item in items {
                match item {
                    Item::Path(path) => return Some(&path.style),
                    Item::Rect(rect) => return Some(&rect.style),
                    Item::Ellipse(ellipse) => return Some(&ellipse.style),
                    Item::Group(inner) if inner.class.as_deref() != Some("node-line-") => {
                        if let Some(style) = first_style(&inner.items) {
                            return Some(style);
                        }
                    }
                    _ => {}
                }
            }
            None
        }
        first_style(&group.items).and_then(|style| style.fill.as_ref().map(|fill| fill.0.clone()))
    }

    fn label_fill(group: &Group) -> Option<String> {
        fn first_text(items: &[Item]) -> Option<&crate::scene::TextRun> {
            for item in items {
                match item {
                    Item::Text(text) => return Some(text),
                    Item::Group(inner) => {
                        if let Some(text) = first_text(&inner.items) {
                            return Some(text);
                        }
                    }
                    _ => {}
                }
            }
            None
        }
        first_text(&group.items)
            .and_then(|text| text.fill.as_ref().map(|fill| fill.0.clone()))
    }

    #[test]
    fn draws_a_rendered_scene_with_view_box() {
        let scene = render("mindmap\n  root((root))\n    one\n      two\n", &Config::default())
            .expect("render mindmap");
        assert!(scene.view_box.width > 0.0);
        assert!(scene.view_box.height > 0.0);
        assert!(!scene.items.is_empty());
    }

    #[test]
    fn rejects_a_body_without_a_mindmap_header() {
        let error = render("not a mindmap", &Config::default()).unwrap_err();
        assert!(matches!(error, MindmapRenderError::Parse(_)));
    }
}
