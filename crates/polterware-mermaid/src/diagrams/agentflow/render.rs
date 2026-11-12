//! Source-to-[`Scene`] rendering for agentflow diagrams: the Rust side of
//! `agentflow/renderer.ts` `draw`.
//!
//! `draw` asks the database for the layout data `getData()` derives (see [`Ast`]'s `nodes`
//! and `layout_edges`, built in the database module), copies the configuration it applies
//! (`type`, `layoutAlgorithm`, `direction`, `nodeSpacing`, `rankSpacing`, `markers`,
//! `diagramId` — see [`super::layout_data`]), and hands everything to the shared unified
//! pipeline, `render(data4Layout, svg)`. Here that is [`super::layout_data::layout_data`] plus
//! [`pipeline::render`]; the pipeline paints the node shapes, the container clusters, and the
//! edge geometry the same way it does for a flowchart.
//!
//! What the pipeline leaves to the diagram, this module paints from `styles.ts`:
//!
//! * Edge paths take `lineColor` at the stylesheet's `strokeWidth ?? 2`, are unfilled
//!   (`.edgePaths .path`, `.flowchart-link`), and carry the `point`/`circle`/`cross` markers
//!   `draw` registers, coloured with `arrowheadColor`.
//! * Edge labels are drawn in `nodeTextColor` over the `edgeLabelBackground` half-opacity
//!   box (`.edgeLabel`), measured by the same monospace rule as the rest of the crate.
//! * An agentflow `flowGroup` cluster is transparent with a 0.75px `flowContainerStroke`
//!   (falling back to `secondaryBorderColor`) outline (`.flow-cluster rect`); the pipeline
//!   draws the container boxes, so this module re-strokes them.
//!
//! Where the port stops short of upstream, it is where the scene the pipeline describes stops:
//! HTML labels (`foreignObject`) are plain text runs, the SVG title is not inserted (the scene
//! carries the diagram, not the page around it), the per-kind palette of `colorSlots.ts` is
//! not applied (the Polterware theme paints the scene), and the hand-drawn rough.js pass is
//! not drawn.

use crate::config::Config;
use crate::error::ParseError;
use crate::measure::{FallbackMeasure, Font, TextMeasure, plain_text_block};
use crate::render::edges::{EdgeData, insert_edge_label};
use crate::render::layout_dagre::LayoutError;
use crate::render::markers::marker_placement;
use crate::render::pipeline;
use crate::render::pipeline::LayoutData;
use crate::render::pipeline::setup_view_port_for_svg;
use crate::scene::{
    Color, Group, Item, PathStyle, Point, Rect, RectShape, Scene, SourceKind, SourceRef, TextRun,
};
use crate::theme::{Theme, ThemeVariables};

use super::ast::Ast;
use super::layout_data::{DEFAULT_DIAGRAM_ID, DIAGRAM_TYPE, layout_data};

/// Renders an agentflow body (the text [`super::parse`] accepts) to a [`Scene`].
///
/// `config` is the diagram configuration after the source's frontmatter and directives have
/// been applied, as `getConfig()` returns it to `draw`.
pub fn draw(
    body: &str,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<Scene, AgentflowRenderError> {
    let ast = super::parse(body)?;
    draw_ast(&ast, config, measure)
}

/// Renders an already parsed agentflow diagram.
pub fn draw_ast(
    ast: &Ast,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<Scene, AgentflowRenderError> {
    draw_layout_ast(ast, config, measure).map(|(_, scene)| scene)
}

/// Renders an already parsed agentflow diagram and keeps the laid-out [`LayoutData`], whose
/// nodes and edges carry the positions and points the scene was painted from.
pub fn draw_layout_ast(
    ast: &Ast,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<(LayoutData, Scene), AgentflowRenderError> {
    let mut data = layout_data(ast, config, DEFAULT_DIAGRAM_ID);
    let mut scene = pipeline::render(&mut data, measure).map_err(AgentflowRenderError::Layout)?;
    complete_edges(&mut scene, &data, config, measure);
    fit_painted_bounds(&mut scene, &data);
    paint_flow_groups(&mut scene, &data);
    Ok((data, scene))
}

/// Renders an agentflow body with the crate's monospace fallback measurement.
pub fn render(body: &str, config: &Config) -> Result<Scene, AgentflowRenderError> {
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

/// `styles.ts`' `.edgePaths .path`: `lineColor` at `strokeWidth ?? 2`, with the
/// `.flowchart-link` rule keeping the fill empty.
fn edge_style(theme: &ThemeVariables) -> PathStyle {
    PathStyle {
        fill: None,
        stroke: Some(theme_color(theme, "lineColor", "#333333")),
        stroke_width: theme.number("strokeWidth").unwrap_or(2.0),
        ..PathStyle::default()
    }
}

/// Completes the pipeline's edge paths with the agentflow stylesheet, the markers `draw`
/// registers, and the edge labels. The pipeline pushes one [`Item::Path`] per layout edge, in
/// edge order.
fn complete_edges(
    scene: &mut Scene,
    data: &LayoutData,
    _config: &Config,
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
    let label_color = theme_color(
        &theme,
        if theme.color("nodeTextColor").is_some() {
            "nodeTextColor"
        } else {
            "textColor"
        },
        "#333333",
    );
    let label_background = theme
        .color("edgeLabelBackground")
        .map(|value| Color::new(value.to_string()));
    let stroke = edge_style(&theme);
    let marker_color = theme_color(&theme, "arrowheadColor", "#333333");

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
                    .push(marker.with_color(marker_color.clone()));
            }
            if let Some(marker) = placement.end {
                path.end_markers
                    .push(marker.with_color(marker_color.clone()));
            }
        }

        let edge_data = EdgeData {
            id: edge.id.clone(),
            points: edge.points.clone().unwrap_or_default(),
            label: edge
                .label
                .as_deref()
                .map(crate::measure::decode_html_entities),
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
                // `.edgeLabel rect` paints the background at half opacity.
                let background_style = PathStyle {
                    fill: label_background.clone(),
                    stroke: None,
                    opacity: 0.5,
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

/// Fits the viewport to the painted node geometry, not only to the layout boxes.
///
/// Upstream fits `setupViewPortForSVG` to the SVG's `getBBox()`, which is the shapes' drawn
/// boxes. The pipeline fits from the layout boxes alone, so a shape drawn even slightly
/// outside its box would be clipped. A diamond is one: `question.ts` paints its polygon half
/// a point right of the label, while the layout box stays centred. This unions the painted
/// geometry of the node groups into the bounds and re-fits.
fn fit_painted_bounds(scene: &mut Scene, data: &LayoutData) {
    let mut bounds = scene.bounds;
    for item in &scene.items {
        let Item::Group(group) = item else {
            continue;
        };
        if !group
            .source
            .as_ref()
            .is_some_and(|source| source.kind == SourceKind::Node)
        {
            continue;
        }
        if let Some(painted) = painted_bounds(item, Point::ZERO) {
            bounds = bounds.union(painted);
        }
    }
    if bounds == scene.bounds {
        return;
    }
    scene.bounds = bounds;
    let padding = data.padding.unwrap_or(8.0);
    let use_max_width = data.use_max_width.unwrap_or(false);
    setup_view_port_for_svg(scene, padding, "agentflow", use_max_width);
}

/// The box an item paints, with a group's translation applied to its children.
///
/// Text is left out: `labelHelper` sizes a node's box from its label, and upstream's fitted
/// bbox reads the shape container, so the label never widens a node's contribution.
fn painted_bounds(item: &Item, offset: Point) -> Option<Rect> {
    let translated = |rect: Rect| {
        Rect::new(
            rect.x + offset.x,
            rect.y + offset.y,
            rect.width,
            rect.height,
        )
    };
    match item {
        Item::Path(path) => {
            crate::render::pipeline::dagre::svg_path_bounds(&path.path).map(translated)
        }
        Item::Rect(rect) => Some(translated(rect.rect)),
        Item::Ellipse(ellipse) => Some(translated(Rect::new(
            ellipse.center.x - ellipse.radius_x,
            ellipse.center.y - ellipse.radius_y,
            ellipse.radius_x * 2.0,
            ellipse.radius_y * 2.0,
        ))),
        Item::Text(_) | Item::Image(_) => None,
        Item::Group(group) => {
            let offset = Point::new(offset.x + group.transform.e, offset.y + group.transform.f);
            group
                .items
                .iter()
                .filter_map(|child| painted_bounds(child, offset))
                .reduce(Rect::union)
        }
    }
}

/// `styles.ts`' `.flow-cluster rect`: an expanded agentflow container is transparent with a
/// 0.75px `flowContainerStroke` (or `secondaryBorderColor`) outline. The pipeline paints the
/// container boxes itself, so this re-strokes the ones whose layout node is a `flowGroup`.
fn paint_flow_groups(scene: &mut Scene, data: &LayoutData) {
    let theme = theme_variables();
    let stroke = theme
        .color("flowContainerStroke")
        .or_else(|| theme.color("secondaryBorderColor"))
        .map(|value| Color::new(value.to_string()))
        .unwrap_or_else(|| Color::new("#aaaa33"));
    let flow_groups: Vec<String> = data
        .nodes
        .iter()
        .filter(|node| node.is_group == Some(true) && node.shape.as_deref() == Some("flowGroup"))
        .map(|node| node.id.clone())
        .collect();
    for item in &mut scene.items {
        let Item::Group(group) = item else {
            continue;
        };
        let is_flow_group = group.source.as_ref().is_some_and(|source| {
            source.kind == SourceKind::Cluster && flow_groups.contains(&source.id)
        });
        if !is_flow_group {
            continue;
        }
        for child in &mut group.items {
            restyle_frame_rects(child, &stroke);
        }
    }
}

/// Re-strokes every rect a cluster group holds, including the ones nested in the role groups
/// `clusters.rs` draws around them.
fn restyle_frame_rects(item: &mut Item, stroke: &Color) {
    match item {
        Item::Rect(rect) => {
            rect.style.fill = Some(Color::new("none"));
            rect.style.stroke = Some(stroke.clone());
            rect.style.stroke_width = 0.75;
        }
        Item::Group(group) => {
            for child in &mut group.items {
                restyle_frame_rects(child, stroke);
            }
        }
        _ => {}
    }
}

/// What can stop an agentflow render.
#[derive(Clone, Debug, PartialEq)]
pub enum AgentflowRenderError {
    Parse(ParseError),
    Layout(LayoutError),
}

impl std::fmt::Display for AgentflowRenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(error) => error.fmt(f),
            Self::Layout(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for AgentflowRenderError {}

impl From<ParseError> for AgentflowRenderError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

impl From<LayoutError> for AgentflowRenderError {
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
            .join("src/diagrams/agentflow/tests/render");
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
    fn renders_recorded_agentflow_fixtures() {
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
        let scene = render(
            "agentflow-beta TB\n  a[Start] --> b[End]\n",
            &Config::default(),
        )
        .expect("render agentflow diagram");
        assert!(scene.view_box.width > 0.0);
        assert!(scene.view_box.height > 0.0);
        assert!(!scene.items.is_empty());
    }

    #[test]
    fn render_diagram_dispatches_the_agentflow_type() {
        let scene = crate::render_diagram(
            "agentflow-beta TB\n  a[Start] --> b[End]\n",
            &Config::default(),
        )
        .expect("render_diagram draws an agentflow diagram");
        assert!(!scene.items.is_empty());
    }

    #[test]
    fn paints_edge_paths_labels_and_markers() {
        let scene = render(
            "agentflow-beta TB\n  a[\"A\"]\n  b[\"B\"]\n  a -- go --> b\n",
            &Config::default(),
        )
        .expect("render agentflow diagram");

        let mut paths = 0;
        let mut labelled = false;
        for item in &scene.items {
            match item {
                Item::Path(path) => {
                    paths += 1;
                    assert!(path.style.fill.is_none(), "an edge is unfilled");
                    assert!(
                        !path.end_markers.is_empty(),
                        "an arrow edge carries its end marker"
                    );
                }
                Item::Group(group)
                    if group.class.as_deref().is_some_and(|class| {
                        class.split_whitespace().any(|name| name == "edgeLabel")
                    }) =>
                {
                    labelled = true;
                }
                _ => {}
            }
        }
        assert!(paths > 0, "the edge is painted");
        assert!(labelled, "the edge label is painted");
    }

    #[test]
    fn paints_a_flow_group_transparent_with_a_thin_stroke() {
        let scene = render(
            "agentflow-beta TB\n  flow one[One]\n    a[\"A\"]\n  end\n",
            &Config::default(),
        )
        .expect("render agentflow diagram");

        let container = scene
            .items
            .iter()
            .find_map(|item| match item {
                Item::Group(group)
                    if group
                        .source
                        .as_ref()
                        .is_some_and(|source| source.kind == SourceKind::Cluster) =>
                {
                    Some(group)
                }
                _ => None,
            })
            .expect("the flow container is painted");
        let rect = find_first_rect(&container.items).expect("the container's frame is a rect");
        assert_eq!(rect.style.fill, Some(Color::new("none")));
        assert_eq!(rect.style.stroke_width, 0.75);
    }

    #[test]
    fn rejects_a_body_without_an_agentflow_header() {
        let error = render("not an agentflow diagram", &Config::default()).unwrap_err();
        assert!(matches!(error, AgentflowRenderError::Parse(_)));
    }

    /// The first rect in a painted subtree, which is where a container's frame lives.
    fn find_first_rect(items: &[Item]) -> Option<&crate::scene::RectShape> {
        for item in items {
            match item {
                Item::Rect(rect) => return Some(rect),
                Item::Group(group) => {
                    if let Some(rect) = find_first_rect(&group.items) {
                        return Some(rect);
                    }
                }
                _ => {}
            }
        }
        None
    }
}
