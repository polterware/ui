//! Source-to-[`Scene`] rendering for sankey diagrams: the Rust side of
//! `diagrams/sankey/sankeyRenderer.ts` `draw`.
//!
//! `draw` reads the `sankey` section of the configuration, takes `sankeyDB.ts`'s `getGraph()`,
//! configures the d3-sankey generator (ported in [`crate::layout::sankey`]), fills the node
//! rectangles, the node labels and the links, and fits the viewport with
//! `setupGraphViewbox(svg, 0, useMaxWidth)`. Here those are [`super::parse`], [`Sankey`], this
//! module's item construction, and this module's painted-item bounds grown by the zero padding.
//!
//! Look: `diagrams/sankey/styles.js` (`.node rect`, `.node-labels`, `.link`,
//! `.sankey-label-bg`, `.sankey-label-fg`) and the attributes `sankeyRenderer.ts` writes. The
//! design system has no Sankey control, so the look is mermaid's own.
//!
//! Behavior: `sankeyRenderer.ts` is the only consumer of d3-sankey in mermaid and gpui-kit has
//! no sankey renderer, so there is no second behavior reference: the renderer itself is the
//! model, followed call by call.
//!
//! Where the port stops short of upstream, the difference is in the painter-facing scene:
//!
//! * `linkColor: 'gradient'` paints each link with a `<linearGradient>` from the source node's
//!   color to the target node's. A [`PathStyle`] stroke is one color, so a gradient link takes
//!   its first stop, which is exactly what `linkColor: 'source'` draws; the second stop is
//!   still painted, on the target node's rectangle.
//! * `mix-blend-mode: multiply` on the links group and `shape-rendering: crispEdges` on the node
//!   rectangles are CSS properties [`Group`] and [`RectShape`] do not carry.
//! * The `outlined` label style draws every label twice: a `sankey-label-bg` run (a 4px
//!   `mainBkg` stroke behind the glyphs, `paint-order: stroke`) and a `sankey-label-fg` run in
//!   `textColor`. A [`TextRun`] has no stroke, so the halo is a second run filled with
//!   `mainBkg` and emitted first, where upstream emits it; the runs coincide, so the visible
//!   text is the `fg` run.
//! * `getText` writes `id\nvalue` into a single `<text>` with no `<tspan>`, and SVG's default
//!   `xml:space` handling collapses the newline to a space: the label is one line, `id value`,
//!   which is the run this scene builds and measures.
//! * The SVG ids `Uid.next` assigns (`node-1`, `linearGradient-2`, ...) are a page-wide
//!   counter; the scene numbers them per diagram (`node-1` by node order) and has no id for a
//!   link, which upstream never writes either (the gradient's id is the only one).
//! * `useMaxWidth` sizes the `<svg>` element, not the drawing; the scene carries the fitted
//!   viewBox and the painter sizes the page.

use crate::config::Config;
use crate::error::ParseError;
use crate::layout::sankey::{
    Sankey, SankeyAlignment, SankeyError, SankeyGraph, SankeyLink, SankeyLinkInput,
    SankeyNodeInput, number::format_number,
};
use crate::measure::{FallbackMeasure, Font, TextMeasure};
use crate::scene::{
    Color, Group, Item, Path, PathStyle, Point, Rect, RectShape, Scene, Size, SourceRef,
    TextAnchor, TextBlock, TextLine, TextRun, TextSpan, Transform,
};

use super::ast::Ast;

/// `d3.schemeTableau10`, the ordinal scale's range (d3-scale-chromatic's palette, the one
/// `d3scaleOrdinal(d3schemeTableau10)` cycles for nodes without a `nodeColors` entry).
pub const SCHEME_TABLEAU_10: [&str; 10] = [
    "#4e79a7", "#f28e2c", "#e15759", "#76b7b2", "#59a14f", "#edc949", "#af7aa1", "#ff9da7",
    "#9c755f", "#bab0ab",
];

/// The `font-size` attribute on the `node-labels` group, inherited by every label.
pub const LABEL_FONT_SIZE: f64 = 14.0;
/// The gap between a node's edge and its label, `d.x1 + 6` / `d.x0 - 6`.
pub const LABEL_OFFSET: f64 = 6.0;
/// Extra `nodePadding` when values are shown, `nodePadding + (showValues ? 15 : 0)`.
pub const VALUES_PADDING: f64 = 15.0;
/// `stroke-opacity` on the links group.
pub const LINK_STROKE_OPACITY: f64 = 0.5;
/// `dy` of a label when no values are shown (`0.35em`); a shown value uses no shift.
pub const NO_VALUES_DY_EM: f64 = 0.35;

/// Renders a sankey body (the text [`super::parse`] accepts) to a [`Scene`].
///
/// `config` is the diagram configuration after the source's frontmatter and directives have
/// been applied, as `getConfig()` returns it to `draw`.
pub fn draw(
    body: &str,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<Scene, SankeyRenderError> {
    let ast = super::parse(body)?;
    draw_ast(&ast, config, measure)
}

/// Renders an already parsed sankey diagram.
pub fn draw_ast(
    ast: &Ast,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<Scene, SankeyRenderError> {
    draw_layout_ast(ast, config, measure).map(|(_, scene)| scene)
}

/// Renders an already parsed sankey diagram and keeps the laid-out [`SankeyGraph`], whose nodes
/// and links carry the positions the scene was drawn from.
pub fn draw_layout_ast(
    ast: &Ast,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<(SankeyGraph, Scene), SankeyRenderError> {
    let sankey = &config.sankey;
    let width = sankey.width;
    let height = sankey.height;
    let show_values = sankey.show_values;

    let nodes: Vec<SankeyNodeInput> = ast
        .nodes
        .iter()
        .map(|node| SankeyNodeInput::new(node.id.clone()))
        .collect();
    let links: Vec<SankeyLinkInput> = ast
        .links
        .iter()
        .map(|link| SankeyLinkInput::new(link.source.clone(), link.target.clone(), link.value))
        .collect();

    let mut generator = Sankey::new();
    generator.node_width = sankey.node_width;
    generator.node_padding = sankey.node_padding + if show_values { VALUES_PADDING } else { 0.0 };
    generator.alignment = SankeyAlignment::from_name(&sankey.node_alignment).unwrap_or_default();
    generator.extent = [[0.0, 0.0], [width, height]];

    let graph = generator.layout(&nodes, &links)?;

    let mut palette = OrdinalColors::default();
    let mut colors: Vec<Color> = Vec::with_capacity(graph.nodes.len());
    for node in &graph.nodes {
        colors.push(node_color(
            &node.id,
            sankey.node_colors.as_ref(),
            &mut palette,
        ));
    }

    let mut scene = Scene::fitted(Rect::ZERO, 0.0);
    let mut nodes_group = Group::new().with_class("nodes");
    for (index, node) in graph.nodes.iter().enumerate() {
        let color = colors[index].clone();
        let source = SourceRef::node(node.id.clone());
        let mut group = Group::translated(node.x0, node.y0)
            .with_class("node")
            .with_source(source.clone());
        group.id = Some(format!("node-{}", index + 1));
        group.push(
            RectShape::new(
                Rect::new(0.0, 0.0, node.x1 - node.x0, node.y1 - node.y0),
                0.0,
                PathStyle::filled(color),
            )
            .with_source(source),
        );
        nodes_group.push(group);
    }
    scene.push(nodes_group);

    let theme = &config.theme_variables;
    let text_color = theme
        .get("textColor")
        .and_then(|value| value.as_str())
        .unwrap_or("#333");
    let label_background = theme
        .get("mainBkg")
        .and_then(|value| value.as_str())
        .or_else(|| theme.get("background").and_then(|value| value.as_str()))
        .unwrap_or("#fff");
    let font = Font::new("monospace", LABEL_FONT_SIZE);
    let central_layer = central_node_layer(&graph);

    let mut labels_group = Group::new().with_class("node-labels");
    let mut label_runs = Vec::with_capacity(graph.nodes.len());
    for node in &graph.nodes {
        let (x, anchor) = label_position(node, width, central_layer, &sankey.label_style);
        let dy = if show_values {
            0.0
        } else {
            NO_VALUES_DY_EM * LABEL_FONT_SIZE
        };
        let text = label_text(node, sankey.show_values, &sankey.prefix, &sankey.suffix);
        let block = label_block(&text, &font, measure);
        let mut run = TextRun::new(
            Point::new(x, (node.y0 + node.y1) / 2.0 + dy),
            font.clone(),
            block,
        )
        .with_source(SourceRef::node(node.id.clone()));
        run.anchor = anchor;
        label_runs.push(run);
    }
    if sankey.label_style == "outlined" {
        // `appendLabel('sankey-label-bg')` then `appendLabel('sankey-label-fg')`: every
        // background run first, then every foreground run.
        for run in &label_runs {
            labels_group.push(run.clone().with_fill(label_background));
        }
        for run in &label_runs {
            labels_group.push(run.clone().with_fill(text_color));
        }
    } else {
        for run in &label_runs {
            labels_group.push(run.clone().with_fill(text_color));
        }
    }
    scene.push(labels_group);

    let mut links_group = Group::new().with_class("links");
    for (index, link) in graph.links.iter().enumerate() {
        let source = SourceRef::edge(format!("link-{}", index + 1));
        let mut group = Group::new().with_class("link").with_source(source.clone());
        group.id = Some(format!("link-{}", index + 1));
        group.push(link_shape(&graph, link, &colors, &sankey.link_color).with_source(source));
        links_group.push(group);
    }
    scene.push(links_group);

    let bounds = scene_bounds(&scene.items).unwrap_or(Rect::ZERO);
    scene.bounds = bounds;
    scene.view_box = bounds;

    Ok((graph, scene))
}

/// Renders a sankey body with the crate's monospace fallback measurement.
pub fn render(body: &str, config: &Config) -> Result<Scene, SankeyRenderError> {
    draw(body, config, &FallbackMeasure)
}

/// What can stop a sankey render.
#[derive(Clone, Debug, PartialEq)]
pub enum SankeyRenderError {
    Parse(ParseError),
    Layout(SankeyError),
}

impl std::fmt::Display for SankeyRenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(error) => error.fmt(f),
            Self::Layout(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for SankeyRenderError {}

impl From<ParseError> for SankeyRenderError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

impl From<SankeyError> for SankeyRenderError {
    fn from(error: SankeyError) -> Self {
        Self::Layout(error)
    }
}

/// `d3scaleOrdinal(d3schemeTableau10)`: an implicit domain in first-seen order, one range color
/// per new id, cycling after the ten of [`SCHEME_TABLEAU_10`].
#[derive(Default)]
struct OrdinalColors {
    assigned: Vec<(String, String)>,
    next: usize,
}

impl OrdinalColors {
    fn get(&mut self, id: &str) -> String {
        if let Some((_, color)) = self.assigned.iter().find(|(key, _)| key == id) {
            return color.clone();
        }
        let color = SCHEME_TABLEAU_10[self.next % SCHEME_TABLEAU_10.len()].to_string();
        self.next += 1;
        self.assigned.push((id.to_string(), color.clone()));
        color
    }
}

/// `getNodeColor`: a `nodeColors` entry wins over the ordinal scale.
fn node_color(
    id: &str,
    node_colors: Option<&serde_json::Map<String, serde_json::Value>>,
    palette: &mut OrdinalColors,
) -> Color {
    let custom = node_colors
        .and_then(|colors| colors.get(id))
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let custom = custom.unwrap_or_else(|| palette.get(id));
    Color::new(custom)
}

/// `findCentralNodeLayer`: the layer of the first node with the largest value, the layer the
/// `outlined` label style measures label sides from.
fn central_node_layer(graph: &SankeyGraph) -> usize {
    let mut max_value = 0.0;
    let mut central_layer = 0;
    for node in &graph.nodes {
        if node.value > max_value {
            max_value = node.value;
            central_layer = node.layer;
        }
    }
    central_layer
}

/// `getLabelPosition`: `outlined` puts a label left of the central layer and right of every
/// layer at or past it; `legacy` puts it right of the left half and left of the right half.
fn label_position(
    node: &crate::layout::sankey::SankeyNode,
    width: f64,
    central_layer: usize,
    label_style: &str,
) -> (f64, TextAnchor) {
    if label_style == "outlined" {
        if node.layer < central_layer {
            return (node.x0 - LABEL_OFFSET, TextAnchor::End);
        }
        return (node.x1 + LABEL_OFFSET, TextAnchor::Start);
    }
    if node.x0 < width / 2.0 {
        (node.x1 + LABEL_OFFSET, TextAnchor::Start)
    } else {
        (node.x0 - LABEL_OFFSET, TextAnchor::End)
    }
}

/// `getText`, with SVG's whitespace handling applied: the value follows the id on one line.
fn label_text(
    node: &crate::layout::sankey::SankeyNode,
    show_values: bool,
    prefix: &str,
    suffix: &str,
) -> String {
    if !show_values {
        return node.id.clone();
    }
    format!(
        "{} {}{}{}",
        node.id,
        prefix,
        format_number(round_to_two_decimals(node.value)),
        suffix
    )
}

/// `Math.round(value * 100) / 100`, with JavaScript's rounding and its `NaN`/`Infinity`.
fn round_to_two_decimals(value: f64) -> f64 {
    if !value.is_finite() {
        return value;
    }
    // `Math.round`: halves go toward positive infinity.
    (value * 100.0 + 0.5).floor() / 100.0
}

/// The measured label. A label is one unwrapped line, and its box is the line's ink
/// ([`TextMeasure::line_height`]) tall, which is what `getBBox()` reports for the `<text>`.
fn label_block(text: &str, font: &Font, measure: &dyn TextMeasure) -> TextBlock {
    let width = measure.width(text, font);
    let ink_height = measure.line_height(font);
    let size = Size::new(width, ink_height);
    let line_height = crate::measure::LINE_HEIGHT_EM * font.size;
    TextBlock::new(
        vec![TextLine::new(vec![TextSpan::plain(text)])],
        size,
        line_height,
    )
}

/// One link: `d3SankeyLinkHorizontal`'s cubic in the paint `linkColor` names, at
/// `max(1, width)` thick and the links group's `stroke-opacity`.
fn link_shape(
    graph: &SankeyGraph,
    link: &SankeyLink,
    colors: &[Color],
    link_color: &str,
) -> crate::scene::PathShape {
    let source = &graph.nodes[link.source];
    let target = &graph.nodes[link.target];
    let middle = (source.x1 + target.x0) / 2.0;
    let mut path = Path::new();
    path.move_to(Point::new(source.x1, link.y0)).cubic_to(
        Point::new(middle, link.y0),
        Point::new(middle, link.y1),
        Point::new(target.x0, link.y1),
    );
    let stroke = match link_color {
        // A gradient is one color to the painter: the gradient's first stop.
        "gradient" | "source" => colors[link.source].clone(),
        "target" => colors[link.target].clone(),
        other => Color::new(other),
    };
    let style = PathStyle {
        fill: None,
        stroke: Some(stroke),
        stroke_opacity: LINK_STROKE_OPACITY,
        stroke_width: link.width.max(1.0),
        ..PathStyle::default()
    };
    crate::scene::PathShape::new(path, style)
}

/// The box the whole drawing fills: every painted item in diagram coordinates, groups'
/// transforms applied, matching the `getBBox()` that `setupGraphViewbox` fits the viewBox to.
fn scene_bounds(items: &[Item]) -> Option<Rect> {
    fn include(out: &mut Option<Rect>, rect: Rect) {
        *out = Some(out.map_or(rect, |bounds| bounds.union(rect)));
    }

    fn transformed(rect: Rect, transform: Transform) -> Rect {
        let corners = [
            transform.apply(Point::new(rect.x, rect.y)),
            transform.apply(Point::new(rect.right(), rect.y)),
            transform.apply(Point::new(rect.right(), rect.bottom())),
            transform.apply(Point::new(rect.x, rect.bottom())),
        ];
        let mut bounds = Rect::new(corners[0].x, corners[0].y, 0.0, 0.0);
        for corner in &corners[1..] {
            bounds = bounds.union(Rect::new(corner.x, corner.y, 0.0, 0.0));
        }
        bounds
    }

    fn walk(item: &Item, transform: Transform, out: &mut Option<Rect>) {
        match item {
            Item::Group(group) => {
                let transform = transform.then(group.transform);
                for child in &group.items {
                    walk(child, transform, out);
                }
            }
            Item::Path(shape) => {
                if let Some(bounds) = crate::render::pipeline::dagre::svg_path_bounds(&shape.path) {
                    include(out, transformed(bounds, transform));
                }
            }
            Item::Rect(shape) => include(out, transformed(shape.rect, transform)),
            Item::Ellipse(shape) => include(
                out,
                transformed(
                    Rect::new(
                        shape.center.x - shape.radius_x,
                        shape.center.y - shape.radius_y,
                        shape.radius_x * 2.0,
                        shape.radius_y * 2.0,
                    ),
                    transform,
                ),
            ),
            Item::Text(text) => include(out, transformed(text.bounds(), transform)),
            Item::Image(image) => include(out, transformed(image.rect, transform)),
        }
    }

    let mut bounds = None;
    for item in items {
        walk(item, Transform::IDENTITY, &mut bounds);
    }
    bounds
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::measure::FallbackMeasure;
    use serde_json::Value;

    const TOLERANCE: f64 = 0.01;

    fn fixtures() -> Vec<(String, Value)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/diagrams/sankey/tests/render");
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

    fn config_for(source: &str) -> Config {
        let preprocessed = crate::preprocess::preprocess(source);
        let mut config = Config::default();
        config
            .apply(&preprocessed.config)
            .unwrap_or_else(|error| panic!("config: {error}"));
        config
    }

    /// The drawn runs of the `node-labels` group, in paint order: the label texts, their
    /// position, anchor and the fill the run carries.
    fn label_runs(scene: &Scene) -> Vec<TextRun> {
        let mut runs = Vec::new();
        for item in &scene.items {
            if let Item::Group(group) = item
                && group.class.as_deref() == Some("node-labels")
            {
                for child in &group.items {
                    if let Item::Text(text) = child {
                        runs.push(text.clone());
                    }
                }
            }
        }
        runs
    }

    fn node_rects(scene: &Scene) -> Vec<(String, Rect, Color)> {
        let mut rects = Vec::new();
        for item in &scene.items {
            if let Item::Group(group) = item
                && group.class.as_deref() == Some("nodes")
            {
                for child in &group.items {
                    let Item::Group(node) = child else {
                        continue;
                    };
                    let Some(Item::Rect(rect)) = node.items.first() else {
                        continue;
                    };
                    let id = node
                        .source
                        .as_ref()
                        .map(|source| source.id.clone())
                        .unwrap_or_default();
                    let origin = node.transform.apply(Point::ZERO);
                    rects.push((
                        id,
                        Rect::new(
                            origin.x + rect.rect.x,
                            origin.y + rect.rect.y,
                            rect.rect.width,
                            rect.rect.height,
                        ),
                        rect.style.fill.clone().unwrap_or(Color::none()),
                    ));
                }
            }
        }
        rects
    }

    fn link_shapes(scene: &Scene) -> Vec<crate::scene::PathShape> {
        let mut links = Vec::new();
        for item in &scene.items {
            if let Item::Group(group) = item
                && group.class.as_deref() == Some("links")
            {
                for child in &group.items {
                    if let Item::Group(link) = child
                        && let Some(Item::Path(path)) = link.items.first()
                    {
                        links.push(path.clone());
                    }
                }
            }
        }
        links
    }

    fn point_of(value: &Value) -> Point {
        Point::new(
            value["x"].as_f64().expect("point x"),
            value["y"].as_f64().expect("point y"),
        )
    }

    #[test]
    fn renders_recorded_sankey_fixtures() {
        let fixtures = fixtures();
        assert!(
            fixtures.len() >= 8,
            "expected at least 8 fixtures, found {}",
            fixtures.len()
        );

        for (file, fixture) in fixtures {
            let source = fixture["source"].as_str().expect("fixture source");
            let body = crate::preprocess::preprocess(source).body;
            let config = config_for(source);
            let ast =
                super::super::parse(&body).unwrap_or_else(|error| panic!("{file}: parse: {error}"));
            let (graph, scene) = draw_layout_ast(&ast, &config, &FallbackMeasure)
                .unwrap_or_else(|error| panic!("{file}: render: {error}"));
            let expected = &fixture["expected"];

            let rects = node_rects(&scene);
            let expected_nodes = expected["nodes"].as_array().expect("expected nodes");
            assert_eq!(rects.len(), expected_nodes.len(), "{file}: node count");
            for (node, expected_node) in graph.nodes.iter().zip(expected_nodes) {
                let id = expected_node["id"].as_str().expect("node id");
                assert_eq!(node.id, id, "{file}: node order");
                let (actual_id, rect, fill) = rects
                    .iter()
                    .find(|(candidate, _, _)| candidate == id)
                    .unwrap_or_else(|| panic!("{file}: node {id} in scene"));
                assert_eq!(actual_id, id);
                close(
                    rect.x,
                    expected_node["x"].as_f64().expect("node x"),
                    &format!("node {id} x"),
                    &file,
                );
                close(
                    rect.y,
                    expected_node["y"].as_f64().expect("node y"),
                    &format!("node {id} y"),
                    &file,
                );
                close(
                    rect.width,
                    expected_node["width"].as_f64().expect("node width"),
                    &format!("node {id} width"),
                    &file,
                );
                close(
                    rect.height,
                    expected_node["height"].as_f64().expect("node height"),
                    &format!("node {id} height"),
                    &file,
                );
                assert_eq!(
                    fill.0,
                    expected_node["fill"].as_str().expect("node fill"),
                    "{file}: node {id} fill"
                );
                assert_eq!(
                    node.layer,
                    expected_node["layer"].as_u64().expect("node layer") as usize,
                    "{file}: node {id} layer"
                );
            }

            let runs = label_runs(&scene);
            let expected_labels = expected["labels"].as_array().expect("expected labels");
            assert_eq!(runs.len(), expected_labels.len(), "{file}: label count");
            let text_color = config
                .theme_variables
                .get("textColor")
                .and_then(Value::as_str)
                .unwrap_or("#333");
            let label_background = config
                .theme_variables
                .get("mainBkg")
                .and_then(Value::as_str)
                .unwrap_or("#fff");
            for (index, run) in runs.iter().enumerate() {
                let expected_label = &expected_labels[index];
                let what = format!("label {index}");
                close(
                    run.position.x,
                    expected_label["x"].as_f64().expect("label x"),
                    &format!("{what} x"),
                    &file,
                );
                let dy = expected_label["dy"].as_f64().expect("label dy");
                close(
                    run.position.y,
                    expected_label["y"].as_f64().expect("label y") + dy,
                    &format!("{what} y"),
                    &file,
                );
                let anchor = match expected_label["anchor"].as_str().expect("label anchor") {
                    "start" => TextAnchor::Start,
                    "middle" => TextAnchor::Middle,
                    "end" => TextAnchor::End,
                    other => panic!("{file}: unknown anchor {other}"),
                };
                assert_eq!(run.anchor, anchor, "{file}: {what} anchor");
                let text = expected_label["text"].as_str().expect("label text");
                let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
                assert_eq!(run.lines[0].text(), text, "{file}: {what} text");
                let expected_fill = match expected_label["class"].as_str() {
                    Some("sankey-label-bg") => label_background,
                    _ => text_color,
                };
                assert_eq!(
                    run.fill.as_ref().map(|fill| fill.0.as_str()),
                    Some(expected_fill),
                    "{file}: {what} fill"
                );
                assert_eq!(
                    run.source.as_ref().map(|source| source.id.as_str()),
                    expected_label["node"].as_str(),
                    "{file}: {what} source"
                );
            }

            let shapes = link_shapes(&scene);
            let expected_links = expected["links"].as_array().expect("expected links");
            assert_eq!(shapes.len(), expected_links.len(), "{file}: link count");
            for (link, (shape, expected_link)) in
                graph.links.iter().zip(shapes.iter().zip(expected_links))
            {
                let what = format!("link {}", link.index);
                let expected_path = &expected_link["path"];
                let commands = &shape.path.commands;
                assert_eq!(commands.len(), 2, "{file}: {what} command count");
                let Some(crate::scene::PathCommand::MoveTo { x, y }) = commands.first() else {
                    panic!("{file}: {what} does not start with a move");
                };
                close(
                    *x,
                    point_of(&expected_path["start"]).x,
                    &format!("{what} start x"),
                    &file,
                );
                close(
                    *y,
                    point_of(&expected_path["start"]).y,
                    &format!("{what} start y"),
                    &file,
                );
                let Some(crate::scene::PathCommand::CubicTo {
                    c1x,
                    c1y,
                    c2x,
                    c2y,
                    x,
                    y,
                }) = commands.get(1)
                else {
                    panic!("{file}: {what} is not a cubic");
                };
                close(
                    *c1x,
                    point_of(&expected_path["control1"]).x,
                    &format!("{what} c1 x"),
                    &file,
                );
                close(
                    *c1y,
                    point_of(&expected_path["control1"]).y,
                    &format!("{what} c1 y"),
                    &file,
                );
                close(
                    *c2x,
                    point_of(&expected_path["control2"]).x,
                    &format!("{what} c2 x"),
                    &file,
                );
                close(
                    *c2y,
                    point_of(&expected_path["control2"]).y,
                    &format!("{what} c2 y"),
                    &file,
                );
                close(
                    *x,
                    point_of(&expected_path["end"]).x,
                    &format!("{what} end x"),
                    &file,
                );
                close(
                    *y,
                    point_of(&expected_path["end"]).y,
                    &format!("{what} end y"),
                    &file,
                );
                close(
                    shape.style.stroke_width,
                    expected_link["strokeWidth"].as_f64().expect("stroke width"),
                    &format!("{what} stroke width"),
                    &file,
                );
                close(
                    shape.style.stroke_opacity,
                    LINK_STROKE_OPACITY,
                    &format!("{what} stroke opacity"),
                    &file,
                );
                assert_eq!(shape.style.fill, None, "{file}: {what} fill");
                let stroke = shape.style.stroke.as_ref().expect("stroke");
                if let Some(gradient) = expected_link["gradient"].as_object() {
                    let stops = gradient["stops"].as_array().expect("gradient stops");
                    close(
                        point_of(&expected_path["start"]).x,
                        gradient["x1"].as_f64().expect("gradient x1"),
                        &format!("{what} gradient x1"),
                        &file,
                    );
                    close(
                        point_of(&expected_path["end"]).x,
                        gradient["x2"].as_f64().expect("gradient x2"),
                        &format!("{what} gradient x2"),
                        &file,
                    );
                    assert_eq!(stops.len(), 2, "{file}: {what} gradient stops");
                    assert_eq!(
                        stroke.0,
                        stops[0]["color"].as_str().expect("start stop"),
                        "{file}: {what} gradient start color"
                    );
                    let target = &graph.nodes[link.target];
                    let target_rect = rects
                        .iter()
                        .find(|(id, _, _)| id == &target.id)
                        .expect("target rect");
                    assert_eq!(
                        stops[1]["color"].as_str().expect("end stop"),
                        target_rect.2.0,
                        "{file}: {what} gradient end color"
                    );
                } else {
                    assert_eq!(
                        stroke.0,
                        expected_link["stroke"].as_str().expect("link stroke"),
                        "{file}: {what} stroke"
                    );
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
            assert_eq!(
                scene.bounds, scene.view_box,
                "{file}: zero padding keeps the bounds"
            );
        }
    }

    #[test]
    fn the_pipeline_dispatch_draws_a_sankey_diagram() {
        let source = "---\nconfig:\n  sankey:\n    showValues: false\n---\nsankey-beta\n\nCoal,Industry,10\nIndustry,Homes,6\nIndustry,Commerce,4\n";
        let config = Config::default();
        let dispatched =
            crate::render_diagram(source, &config).expect("render through the dispatch");
        let direct = render(
            &crate::preprocess::preprocess(source).body,
            &config_for(source),
        )
        .expect("render sankey");
        assert_eq!(dispatched, direct);
    }

    #[test]
    fn draws_a_rendered_scene_with_view_box() {
        let scene = render("sankey-beta\n\nA,B,10\nB,C,4\nB,D,6\n", &Config::default())
            .expect("render sankey diagram");
        assert!(scene.view_box.width > 0.0);
        assert!(scene.view_box.height > 0.0);
        assert!(!scene.items.is_empty());
    }

    #[test]
    fn rejects_a_body_without_a_sankey_header() {
        let error = render("not a sankey diagram", &Config::default()).unwrap_err();
        assert!(matches!(error, SankeyRenderError::Parse(_)));
    }

    #[test]
    fn a_link_naming_no_node_is_a_layout_error() {
        let error = render("sankey-beta\n\nA,B,1\nB,C,2\n", &Config::default());
        assert!(error.is_ok());
        let ast = Ast {
            nodes: vec![
                super::super::Node {
                    id: "A".to_string(),
                },
                super::super::Node {
                    id: "B".to_string(),
                },
            ],
            links: vec![super::super::Link {
                source: "A".to_string(),
                target: "missing".to_string(),
                value: 1.0,
            }],
        };
        let error = draw_ast(&ast, &Config::default(), &FallbackMeasure).unwrap_err();
        assert!(matches!(
            error,
            SankeyRenderError::Layout(SankeyError::MissingNode(_))
        ));
    }

    #[test]
    fn an_empty_graph_is_a_layout_error() {
        let ast = Ast::default();
        let error = draw_ast(&ast, &Config::default(), &FallbackMeasure).unwrap_err();
        assert_eq!(error, SankeyRenderError::Layout(SankeyError::EmptyGraph));
    }

    #[test]
    fn a_cycle_is_a_layout_error() {
        let ast = super::super::parse("sankey-beta\n\nA,B,1\nB,A,1\n").expect("parse");
        let error = draw_ast(&ast, &Config::default(), &FallbackMeasure).unwrap_err();
        assert_eq!(error, SankeyRenderError::Layout(SankeyError::CircularLink));
    }

    #[test]
    fn the_value_label_rounds_like_javascript() {
        assert_eq!(format_number(round_to_two_decimals(124.729)), "124.73");
        assert_eq!(format_number(round_to_two_decimals(0.597)), "0.6");
        assert_eq!(format_number(round_to_two_decimals(280.0)), "280");
        assert_eq!(format_number(round_to_two_decimals(-0.005)), "0");
    }

    #[test]
    fn the_ordinal_palette_cycles_in_first_seen_order() {
        let mut palette = OrdinalColors::default();
        assert_eq!(palette.get("a"), "#4e79a7");
        assert_eq!(palette.get("b"), "#f28e2c");
        assert_eq!(palette.get("a"), "#4e79a7");
        for index in 2..9 {
            let _ = palette.get(&format!("n{index}"));
        }
        assert_eq!(palette.get("tenth"), "#bab0ab");
        assert_eq!(palette.get("eleventh"), "#4e79a7");
    }
}
