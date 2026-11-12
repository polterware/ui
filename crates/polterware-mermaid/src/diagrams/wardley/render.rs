//! Source-to-[`Scene`] rendering for wardley maps: the Rust side of `wardleyRenderer.ts`
//! `draw`.
//!
//! A wardley map does not go through the unified layout pipeline: `draw` uses plain d3. It
//! reads the map from [`Ast`](super::Ast) (`getWardleyData` plus `getDiagramTitle`), and draws,
//! in this order: the size-and-padding box, the title, the axes, the evolution stages, the
//! optional grid, the pipeline boxes with their dotted evolution links, the flow links and
//! their labels, the evolution trends, the nodes (outsource/buy/build ring, market triangle,
//! circle, pipeline square, inertia bar, label), the annotations and their numbered box, the
//! notes, and the accelerator/deaccelerator arrows. Three arrow markers live in the scene's
//! defs. This module ports that: [`draw`] parses, [`draw_ast`] renders an already parsed
//! database, and [`render`] uses the crate's monospace fallback measurement.
//!
//! Coordinates are upstream's: a node's `(evolution, visibility)` percentages project into the
//! chart box `padding .. width - padding` by `width - 2 * padding`, and the visibility axis
//! grows upward. The defaults are `wardley-beta`'s (a 900 by 600 canvas, 48 of padding, a 6
//! node radius, an 8 label offset, 12 and 10 for the axis and label font sizes, no grid), and
//! a `size [width, height]` directive overrides the canvas.
//!
//! Text is measured in the `monospace` family for the annotations box, the only place `draw`
//! measures (`getComputedTextLength` and `getBBox`): the rule the fixture harness renders with
//! and that [`FallbackMeasure`](crate::measure::FallbackMeasure) reproduces. Every other label
//! is placed by arithmetic. The theme colors come from `themeVariables.wardley` (the nested
//! object `theme-base.js` builds), with `getTheme`'s own string fallbacks behind it.
//!
//! Where the port stops short of upstream: `styles.ts` also compiles CSS for the classes
//! `draw` emits, and a [`Scene`] carries paints rather than selectors. CSS outranks the
//! presentation attributes `draw` sets, so a browser repaints some of them from the theme (the
//! buy/build/outsource rings take `componentFill`, a dashed link takes `4 4`, an evolved or
//! anchor label takes `componentLabelColor`, the annotations' white circles and box take
//! `annotationFill`, and their text takes `annotationTextColor`). The scene resolves the
//! attributes `draw` writes, which is what the fixture SVG stores. The class names
//! (`wardley-node`, `wardley-link`, ...) are kept on the groups so a painter can theme by kind.
//!
//! The fixtures exercise the default `wardley-beta` configuration: this checkout's
//! `defaultConfig.ts` has no `wardley-beta` section, so upstream drops a directive for it
//! before `getConfigValues` reads it. The Rust side reads [`Config::wardley_beta`], and a
//! separate test covers the `showGrid` grid the fixtures cannot reach.

use std::collections::HashMap;
use std::f64::consts::SQRT_2;

use serde_json::Value;

use crate::config::{Config, WardleyConfig};
use crate::error::ParseError;
use crate::measure::{FallbackMeasure, Font, TextMeasure, TextOptions, shape_text};
use crate::scene::{
    Color, EllipseShape, Group, MarkerDef, MarkerOrient, MarkerRef, Path, PathShape, PathStyle,
    Point, Rect, RectShape, Scene, TextAnchor, TextBaseline, TextRun,
};

use super::Ast;
use super::ast::{Flow, Node, SourceStrategy};

/// `DEFAULT_STAGES`, used when the diagram has no `evolution` line.
const DEFAULT_STAGES: [&str; 4] = ["Genesis", "Custom Built", "Product", "Commodity"];
/// The diagram id this renderer prefixes its marker ids with. Upstream uses the id
/// `mermaid.render` was called with; the scene id is only namespacing for the painter.
const DIAGRAM_ID: &str = "wardley";
/// The padding around a pipeline box, `draw`'s `padding`.
const PIPELINE_PADDING: f64 = 15.0;
/// The annotations box's inner padding.
const ANNOTATIONS_PADDING: f64 = 10.0;
/// The baseline advance between two annotations-box lines.
const ANNOTATIONS_LINE_HEIGHT: f64 = 16.0;
/// The annotations box's font size.
const ANNOTATIONS_FONT_SIZE: f64 = 11.0;
/// The extra width `draw` adds to the annotations box "for safety".
const ANNOTATIONS_EXTRA_WIDTH: f64 = 105.0;
/// How far above their line link labels sit.
const LINK_LABEL_OFFSET: f64 = 8.0;

/// Renders a wardley body (the text [`super::parse`] accepts) to a [`Scene`].
///
/// `config` is the diagram configuration after the source's frontmatter and directives have
/// been applied, as `getConfig()` returns it to `draw`.
pub fn draw(body: &str, config: &Config, measure: &dyn TextMeasure) -> Result<Scene, ParseError> {
    let ast = super::parse(body)?;
    Ok(draw_ast(&ast, config, measure))
}

/// Renders a wardley body with the crate's monospace fallback measurement.
pub fn render(body: &str, config: &Config) -> Result<Scene, ParseError> {
    draw(body, config, &FallbackMeasure)
}

/// Renders an already parsed wardley database.
pub fn draw_ast(ast: &Ast, config: &Config, measure: &dyn TextMeasure) -> Scene {
    let values = Values::new(config, ast);
    let theme = Theme::from_config(config);

    let width = ast
        .size
        .map(|size| size.width)
        .unwrap_or(values.config.width);
    let height = ast
        .size
        .map(|size| size.height)
        .unwrap_or(values.config.height);
    let chart_width = width - values.config.padding * 2.0;
    let chart_height = height - values.config.padding * 2.0;
    let square_size = values.config.node_radius * 1.6;

    let project_x = |value: f64| values.config.padding + (value / 100.0) * chart_width;
    let project_y = |value: f64| height - values.config.padding - (value / 100.0) * chart_height;

    let mut scene = Scene::new(Rect::new(0.0, 0.0, width, height));
    define_markers(&mut scene, &theme);

    let mut root = Group::default().with_class("wardley-map");
    root.push(RectShape::new(
        Rect::new(0.0, 0.0, width, height),
        0.0,
        PathStyle::filled(Color::new(&theme.background)),
    ));

    if !ast.diagram_title.is_empty() {
        root.push(text_run(
            &ast.diagram_title,
            Point::new(width / 2.0, values.config.padding / 2.0),
            TextAnchor::Middle,
            TextBaseline::Middle,
            Font::new("monospace", values.config.axis_font_size * 1.05).with_weight(700),
            Some(&theme.axis_text),
            0.0,
            measure,
        ));
    }

    root.push(axes_group(
        &values,
        &theme,
        width,
        height,
        chart_width,
        chart_height,
    ));
    root.push(stages_group(&values, &theme, height, chart_width));

    if values.config.show_grid {
        root.push(grid_group(
            &values,
            &theme,
            width,
            height,
            chart_width,
            chart_height,
        ));
    }

    let mut positions = Positions::new(ast, &project_x, &project_y);

    if !ast.pipelines.is_empty() {
        let (boxes, links) = pipelines_groups(ast, &mut positions, &theme, &values, square_size);
        root.push(boxes);
        root.push(links);
    }

    root.push(links_group(ast, &positions, &theme, &values, square_size));
    root.push(trends_group(
        ast, &positions, &theme, &values, &project_x, &project_y,
    ));
    root.push(nodes_group(ast, &positions, &theme, &values, square_size));

    if !ast.annotations.is_empty() {
        root.push(annotations_group(
            ast, &theme, &values, width, height, &project_x, &project_y, measure,
        ));
    }
    if !ast.notes.is_empty() {
        root.push(notes_group(ast, &theme, &project_x, &project_y));
    }
    if !ast.accelerators.is_empty() {
        root.push(arrows_group(
            &ast.accelerators,
            &theme,
            &project_x,
            &project_y,
        ));
    }
    if !ast.deaccelerators.is_empty() {
        root.push(deaccelerators_group(ast, &theme, &project_x, &project_y));
    }

    scene.push(root);
    scene
}

/// The resolved configuration `draw` reads (`getConfigValues`) plus the axes configuration.
struct Values {
    config: WardleyConfig,
    stages: Option<Vec<String>>,
    boundaries: Option<Vec<f64>>,
    x_label: String,
    y_label: String,
}

impl Values {
    fn new(config: &Config, ast: &Ast) -> Self {
        Self {
            config: config.wardley_beta.clone(),
            stages: ast.axes.stages.clone(),
            boundaries: ast.axes.stage_boundaries.clone(),
            x_label: ast
                .axes
                .x_label
                .clone()
                .unwrap_or_else(|| "Evolution".to_string()),
            y_label: ast
                .axes
                .y_label
                .clone()
                .unwrap_or_else(|| "Visibility".to_string()),
        }
    }
}

/// `getTheme`: the `themeVariables.wardley` colors, with `draw`'s fallbacks behind them.
struct Theme {
    background: String,
    axis: String,
    axis_text: String,
    grid: String,
    component_fill: String,
    component_stroke: String,
    component_label: String,
    link_stroke: String,
    evolution_stroke: String,
}

impl Theme {
    fn from_config(config: &Config) -> Self {
        let theme = &config.theme_variables;
        let wardley = theme.get("wardley").and_then(Value::as_object);
        let value = |key: &str| {
            wardley
                .and_then(|object| object.get(key))
                .and_then(Value::as_str)
        };
        let flat = |key: &str| theme.get(key).and_then(Value::as_str);
        let pick = |nested: Option<&str>, fallback: Option<&str>, last: &str| {
            nested.or(fallback).unwrap_or(last).to_string()
        };
        Self {
            background: pick(value("backgroundColor"), flat("background"), "#fff"),
            axis: pick(value("axisColor"), None, "#000"),
            axis_text: pick(value("axisTextColor"), flat("primaryTextColor"), "#222"),
            grid: pick(value("gridColor"), None, "rgba(100, 100, 100, 0.2)"),
            component_fill: pick(value("componentFill"), None, "#fff"),
            component_stroke: pick(value("componentStroke"), None, "#000"),
            component_label: pick(
                value("componentLabelColor"),
                flat("primaryTextColor"),
                "#222",
            ),
            link_stroke: pick(value("linkStroke"), None, "#000"),
            evolution_stroke: pick(value("evolutionStroke"), None, "#dc3545"),
        }
    }
}

/// The projected position of every node, by node index.
struct Positions {
    points: Vec<Point>,
    index: HashMap<String, usize>,
}

impl Positions {
    fn new(ast: &Ast, project_x: &dyn Fn(f64) -> f64, project_y: &dyn Fn(f64) -> f64) -> Self {
        let mut index = HashMap::new();
        let mut points = Vec::with_capacity(ast.nodes.len());
        for (position, node) in ast.nodes.iter().enumerate() {
            index.entry(node.id.clone()).or_insert(position);
            points.push(Point::new(project_x(node.x), project_y(node.y)));
        }
        Self { points, index }
    }

    fn get(&self, id: &str) -> Option<Point> {
        self.index.get(id).map(|position| self.points[*position])
    }

    fn get_mut(&mut self, id: &str) -> Option<&mut Point> {
        let position = *self.index.get(id)?;
        self.points.get_mut(position)
    }
}

/// The three arrow markers `draw` defines, in the order it appends them.
fn define_markers(scene: &mut Scene, theme: &Theme) {
    scene.define(arrow_marker(
        &format!("arrow-{DIAGRAM_ID}"),
        9.0,
        6.0,
        MarkerOrient::AutoStartReverse,
        &theme.evolution_stroke,
        false,
    ));
    scene.define(arrow_marker(
        &format!("link-arrow-end-{DIAGRAM_ID}"),
        9.0,
        5.0,
        MarkerOrient::Auto,
        &theme.link_stroke,
        false,
    ));
    scene.define(arrow_marker(
        &format!("link-arrow-start-{DIAGRAM_ID}"),
        1.0,
        5.0,
        MarkerOrient::Auto,
        &theme.link_stroke,
        true,
    ));
}

/// One 10-by-10 triangular arrow marker.
fn arrow_marker(
    id: &str,
    reference_x: f64,
    size: f64,
    orient: MarkerOrient,
    fill: &str,
    reversed: bool,
) -> MarkerDef {
    let mut path = Path::new();
    if reversed {
        path.move_to(Point::new(10.0, 0.0));
        path.line_to(Point::new(0.0, 5.0));
        path.line_to(Point::new(10.0, 10.0));
    } else {
        path.move_to(Point::new(0.0, 0.0));
        path.line_to(Point::new(10.0, 5.0));
        path.line_to(Point::new(0.0, 10.0));
    }
    path.close();
    let mut marker = MarkerDef::new(id, Rect::new(0.0, 0.0, 10.0, 10.0))
        .with_reference(Point::new(reference_x, 5.0))
        .with_size(size, size)
        .with_orient(orient);
    marker.push(PathShape::new(
        path,
        PathStyle {
            fill: Some(Color::new(fill)),
            stroke: None,
            ..PathStyle::default()
        },
    ));
    marker
}

/// `wardley-axes`: the two axis lines and their labels.
fn axes_group(
    values: &Values,
    theme: &Theme,
    width: f64,
    height: f64,
    chart_width: f64,
    chart_height: f64,
) -> Group {
    let padding = values.config.padding;
    let mut group = Group::default().with_class("wardley-axes");
    group.push(line_shape(
        padding,
        height - padding,
        width - padding,
        height - padding,
        &theme.axis,
        1.0,
        &[],
    ));
    group.push(line_shape(
        padding,
        padding,
        padding,
        height - padding,
        &theme.axis,
        1.0,
        &[],
    ));
    group.push(text_run(
        &values.x_label,
        Point::new(padding + chart_width / 2.0, height - padding / 4.0),
        TextAnchor::Middle,
        TextBaseline::Alphabetic,
        Font::new("monospace", values.config.axis_font_size).with_weight(700),
        Some(&theme.axis_text),
        0.0,
        &FallbackMeasure,
    ));
    group.push(text_run(
        &values.y_label,
        Point::new(padding / 3.0, padding + chart_height / 2.0),
        TextAnchor::Middle,
        TextBaseline::Alphabetic,
        Font::new("monospace", values.config.axis_font_size).with_weight(700),
        Some(&theme.axis_text),
        -90.0,
        &FallbackMeasure,
    ));
    group
}

/// `wardley-stages`: the stage labels and their dashed dividers.
fn stages_group(values: &Values, theme: &Theme, height: f64, chart_width: f64) -> Group {
    let padding = values.config.padding;
    let stages: Vec<String> = match &values.stages {
        Some(stages) if !stages.is_empty() => stages.clone(),
        _ => DEFAULT_STAGES
            .iter()
            .map(|stage| stage.to_string())
            .collect(),
    };
    let boundaries = values.boundaries.clone();
    let positions: Vec<(f64, f64)> = match boundaries {
        Some(boundaries) if boundaries.len() == stages.len() => {
            let mut previous = 0.0;
            boundaries
                .into_iter()
                .map(|boundary| {
                    let span = (previous, boundary);
                    previous = boundary;
                    span
                })
                .collect()
        }
        _ => {
            let step = 1.0 / stages.len() as f64;
            (0..stages.len())
                .map(|index| (index as f64 * step, (index as f64 + 1.0) * step))
                .collect()
        }
    };

    let mut group = Group::default().with_class("wardley-stages");
    for (index, stage) in stages.iter().enumerate() {
        let (start, end) = positions[index];
        let start_x = padding + start * chart_width;
        let end_x = padding + end * chart_width;
        let center_x = (start_x + end_x) / 2.0;

        if index > 0 {
            let mut divider = line_shape(
                start_x,
                padding,
                start_x,
                height - padding,
                "#000",
                1.0,
                &[5.0, 5.0],
            );
            divider.style.opacity = 0.8;
            group.push(divider);
        }

        group.push(text_run(
            stage,
            Point::new(center_x, height - padding / 1.5),
            TextAnchor::Middle,
            TextBaseline::Alphabetic,
            Font::new("monospace", values.config.axis_font_size - 2.0),
            Some(&theme.axis_text),
            0.0,
            &FallbackMeasure,
        ));
    }
    group
}

/// `wardley-grid`: the three dashed quarter lines when `showGrid` is on.
fn grid_group(
    values: &Values,
    theme: &Theme,
    width: f64,
    height: f64,
    chart_width: f64,
    chart_height: f64,
) -> Group {
    let padding = values.config.padding;
    let mut group = Group::default().with_class("wardley-grid");
    for step in 1..4 {
        let ratio = step as f64 / 4.0;
        let x = padding + chart_width * ratio;
        group.push(line_shape(
            x,
            padding,
            x,
            height - padding,
            &theme.grid,
            1.0,
            &[2.0, 6.0],
        ));
        let y = height - padding - chart_height * ratio;
        group.push(line_shape(
            padding,
            y,
            width - padding,
            y,
            &theme.grid,
            1.0,
            &[2.0, 6.0],
        ));
    }
    group
}

/// `wardley-pipelines` and `wardley-pipeline-links`: the pipeline boxes and the dotted
/// evolution links between their components. Mutates the parent node's position, as `draw`
/// does when it puts the parent on the box's top edge.
fn pipelines_groups(
    ast: &Ast,
    positions: &mut Positions,
    theme: &Theme,
    values: &Values,
    square_size: f64,
) -> (Group, Group) {
    let mut boxes = Group::default().with_class("wardley-pipelines");
    let mut links = Group::default().with_class("wardley-pipeline-links");

    for pipeline in &ast.pipelines {
        if pipeline.component_ids.is_empty() {
            continue;
        }

        let mut components: Vec<(&Node, Point)> = pipeline
            .component_ids
            .iter()
            .filter_map(|id| {
                let position = positions.get(id)?;
                let node = ast.node(id)?;
                Some((node, position))
            })
            .collect();
        components.sort_by(|a, b| {
            a.0.x
                .partial_cmp(&b.0.x)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        for pair in components.windows(2) {
            links.push(line_shape(
                pair[0].1.x,
                pair[0].1.y,
                pair[1].1.x,
                pair[1].1.y,
                &theme.link_stroke,
                1.0,
                &[4.0, 4.0],
            ));
        }

        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut y = 0.0;
        for id in &pipeline.component_ids {
            if let Some(position) = positions.get(id) {
                min_x = min_x.min(position.x);
                max_x = max_x.max(position.x);
                y = position.y;
            }
        }
        if min_x == f64::INFINITY || max_x == f64::NEG_INFINITY {
            continue;
        }

        let height = values.config.node_radius * 4.0;
        let box_top = y - height / 2.0;
        let center_x = (min_x + max_x) / 2.0;
        if let Some(parent) = positions.get_mut(&pipeline.node_id) {
            parent.x = center_x;
            parent.y = box_top - square_size / 6.0;
        }

        boxes.push(RectShape::new(
            Rect::new(
                min_x - PIPELINE_PADDING,
                box_top,
                max_x - min_x + PIPELINE_PADDING * 2.0,
                height,
            ),
            4.0,
            PathStyle {
                fill: None,
                stroke: Some(Color::new(&theme.axis)),
                stroke_width: 1.5,
                ..PathStyle::default()
            },
        ));
    }

    (boxes, links)
}

/// `wardley-links`: the flow links and the labels above them.
fn links_group(
    ast: &Ast,
    positions: &Positions,
    theme: &Theme,
    values: &Values,
    square_size: f64,
) -> Group {
    let mut group = Group::default().with_class("wardley-links");
    let valid: Vec<super::Link> = ast
        .links
        .iter()
        .filter(|link| {
            if positions.get(&link.source).is_none() || positions.get(&link.target).is_none() {
                return false;
            }
            let pipeline = ast
                .pipelines
                .iter()
                .find(|pipeline| pipeline.node_id == link.target);
            !pipeline.is_some_and(|pipeline| pipeline.component_ids.contains(&link.source))
        })
        .cloned()
        .collect();

    let radius_of = |id: &str| {
        ast.node(id)
            .filter(|node| node.is_pipeline_parent)
            .map(|_| square_size / SQRT_2)
            .unwrap_or(values.config.node_radius)
    };

    for link in &valid {
        let Some(source) = positions.get(&link.source) else {
            continue;
        };
        let Some(target) = positions.get(&link.target) else {
            continue;
        };
        let source_radius = radius_of(&link.source);
        let target_radius = radius_of(&link.target);
        let dx = target.x - source.x;
        let dy = target.y - source.y;
        let distance = (dx * dx + dy * dy).sqrt();
        let x1 = source.x + (dx / distance) * source_radius;
        let y1 = source.y + (dy / distance) * source_radius;
        let back_dx = source.x - target.x;
        let back_dy = source.y - target.y;
        let back_distance = (back_dx * back_dx + back_dy * back_dy).sqrt();
        let x2 = target.x + (back_dx / back_distance) * target_radius;
        let y2 = target.y + (back_dy / back_distance) * target_radius;

        let mut shape = line_shape(
            x1,
            y1,
            x2,
            y2,
            &theme.link_stroke,
            1.0,
            if link.dashed { &[6.0, 6.0] } else { &[] },
        );
        match link.flow {
            Some(Flow::Forward) => {
                shape
                    .end_markers
                    .push(MarkerRef::new(format!("link-arrow-end-{DIAGRAM_ID}")));
            }
            Some(Flow::Backward) => {
                shape
                    .start_markers
                    .push(MarkerRef::new(format!("link-arrow-start-{DIAGRAM_ID}")));
            }
            Some(Flow::Bidirectional) => {
                shape
                    .start_markers
                    .push(MarkerRef::new(format!("link-arrow-start-{DIAGRAM_ID}")));
                shape
                    .end_markers
                    .push(MarkerRef::new(format!("link-arrow-end-{DIAGRAM_ID}")));
            }
            None => {}
        }
        group.push(shape);
    }

    for link in valid.iter().filter(|link| link.label.is_some()) {
        let Some(source) = positions.get(&link.source) else {
            continue;
        };
        let Some(target) = positions.get(&link.target) else {
            continue;
        };
        let mid_x = (source.x + target.x) / 2.0;
        let mid_y = (source.y + target.y) / 2.0;
        let dx = target.x - source.x;
        let dy = target.y - source.y;
        let distance = (dx * dx + dy * dy).sqrt();
        let perp_x = dy / distance;
        let perp_y = -dx / distance;
        let label_x = mid_x + perp_x * LINK_LABEL_OFFSET;
        let label_y = mid_y + perp_y * LINK_LABEL_OFFSET;
        let mut angle = dy.atan2(dx) * 180.0 / std::f64::consts::PI;
        if !(-90.0..=90.0).contains(&angle) {
            angle += 180.0;
        }
        group.push(text_run(
            link.label.as_deref().unwrap_or_default(),
            Point::new(label_x, label_y),
            TextAnchor::Middle,
            TextBaseline::Middle,
            Font::new("monospace", values.config.label_font_size),
            Some(&theme.axis_text),
            angle,
            &FallbackMeasure,
        ));
    }

    group
}

/// `wardley-trends`: one arrow per `evolve`, shortened to stop at the target circle's edge.
fn trends_group(
    ast: &Ast,
    positions: &Positions,
    theme: &Theme,
    values: &Values,
    project_x: &dyn Fn(f64) -> f64,
    project_y: &dyn Fn(f64) -> f64,
) -> Group {
    let mut group = Group::default().with_class("wardley-trends");
    for trend in &ast.trends {
        let Some(origin) = positions.get(&trend.node_id) else {
            continue;
        };
        let target_x = project_x(trend.target_x);
        let target_y = project_y(trend.target_y);
        let dx = target_x - origin.x;
        let dy = target_y - origin.y;
        let distance = (dx * dx + dy * dy).sqrt();
        let shorten_by = values.config.node_radius + 2.0;
        let adjusted_x2 = if distance > shorten_by {
            target_x - (dx / distance) * shorten_by
        } else {
            target_x
        };
        let adjusted_y2 = if distance > shorten_by {
            target_y - (dy / distance) * shorten_by
        } else {
            target_y
        };
        let mut shape = line_shape(
            origin.x,
            origin.y,
            adjusted_x2,
            adjusted_y2,
            &theme.evolution_stroke,
            1.0,
            &[4.0, 4.0],
        );
        shape
            .end_markers
            .push(MarkerRef::new(format!("arrow-{DIAGRAM_ID}")));
        group.push(shape);
    }
    group
}

/// `wardley-nodes`: one group per node, with its strategy ring, shape, inertia bar, and label.
fn nodes_group(
    ast: &Ast,
    positions: &Positions,
    theme: &Theme,
    values: &Values,
    square_size: f64,
) -> Group {
    let radius = values.config.node_radius;
    let mut group = Group::default().with_class("wardley-nodes");

    for node in &ast.nodes {
        let Some(position) = positions.get(&node.id) else {
            continue;
        };
        let class = match &node.class_name {
            Some(class) => format!("wardley-node wardley-node--{class}"),
            None => "wardley-node".to_string(),
        };
        let mut node_group = Group::default().with_class(class);

        if let Some(strategy) = node.source_strategy
            && strategy != SourceStrategy::Market
        {
            let (fill, stroke) = match strategy {
                SourceStrategy::Outsource => ("#666", theme.component_stroke.clone()),
                SourceStrategy::Buy => ("#ccc", theme.component_stroke.clone()),
                SourceStrategy::Build => ("#eee", "#000".to_string()),
                SourceStrategy::Market => unreachable!(),
            };
            node_group.push(circle_shape(
                position,
                radius * 2.0,
                fill,
                Some(&stroke),
                1.0,
            ));
        }

        if node.is_pipeline_parent {
            node_group.push(RectShape::new(
                Rect::new(
                    position.x - square_size / 2.0,
                    position.y - square_size / 2.0,
                    square_size,
                    square_size,
                ),
                0.0,
                PathStyle {
                    fill: Some(Color::new(&theme.component_fill)),
                    stroke: Some(Color::new(&theme.component_stroke)),
                    stroke_width: 1.0,
                    ..PathStyle::default()
                },
            ));
        } else if node.source_strategy == Some(SourceStrategy::Market) {
            market_shapes(&mut node_group, position, radius, &theme.component_stroke);
        } else if node.class_name.as_deref() != Some("anchor") {
            node_group.push(circle_shape(
                position,
                radius,
                &theme.component_fill,
                Some(&theme.component_stroke),
                1.0,
            ));
        }

        if node.inertia {
            let mut offset = if node.is_pipeline_parent {
                square_size / 2.0 + 15.0
            } else {
                radius + 15.0
            };
            if node.source_strategy.is_some() {
                offset += radius + 10.0;
            }
            let line_height = if node.is_pipeline_parent {
                square_size
            } else {
                radius * 2.0
            };
            node_group.push(line_shape(
                position.x + offset,
                position.y - line_height / 2.0,
                position.x + offset,
                position.y + line_height / 2.0,
                &theme.component_stroke,
                6.0,
                &[],
            ));
        }

        let (label_x, label_y) = label_position(node, position, values);
        let (fill, weight, anchor, baseline) = match node.class_name.as_deref() {
            Some("evolved") => (
                theme.evolution_stroke.clone(),
                400,
                TextAnchor::Start,
                TextBaseline::Alphabetic,
            ),
            Some("anchor") => (
                "#000".to_string(),
                700,
                TextAnchor::Middle,
                TextBaseline::Middle,
            ),
            _ => (
                theme.component_label.clone(),
                400,
                TextAnchor::Start,
                TextBaseline::Alphabetic,
            ),
        };
        node_group.push(text_run(
            &node.label,
            Point::new(label_x, label_y),
            anchor,
            baseline,
            Font::new("monospace", values.config.label_font_size).with_weight(weight),
            Some(&fill),
            0.0,
            &FallbackMeasure,
        ));

        group.push(node_group);
    }
    group
}

/// The market overlay: the ring, the triangle's three lines, and the three white dots.
fn market_shapes(group: &mut Group, position: Point, radius: f64, stroke: &str) {
    group.push(circle_shape(
        position,
        radius * 2.0,
        "white",
        Some(stroke),
        1.0,
    ));
    let small = radius * 0.7;
    let triangle = radius * 1.2;
    let cos = (std::f64::consts::PI / 6.0).cos();
    let sin = (std::f64::consts::PI / 6.0).sin();
    let top = Point::new(position.x, position.y - triangle);
    let bottom_left = Point::new(position.x - triangle * cos, position.y + triangle * sin);
    let bottom_right = Point::new(position.x + triangle * cos, position.y + triangle * sin);
    group.push(line_shape(
        top.x,
        top.y,
        bottom_left.x,
        bottom_left.y,
        stroke,
        1.0,
        &[],
    ));
    group.push(line_shape(
        bottom_left.x,
        bottom_left.y,
        bottom_right.x,
        bottom_right.y,
        stroke,
        1.0,
        &[],
    ));
    group.push(line_shape(
        bottom_right.x,
        bottom_right.y,
        top.x,
        top.y,
        stroke,
        1.0,
        &[],
    ));
    for dot in [top, bottom_left, bottom_right] {
        group.push(circle_shape(dot, small, "white", Some(stroke), 2.0));
    }
}

/// `draw`'s label offsets: anchors center on the node, components sit to the upper right with
/// an extra 10 for a source strategy, and an explicit `label [x, y]` wins.
fn label_position(node: &Node, position: Point, values: &Values) -> (f64, f64) {
    if node.class_name.as_deref() == Some("anchor") {
        let x = node
            .label_offset_x
            .map(|offset| position.x + offset)
            .unwrap_or(position.x);
        let y = node
            .label_offset_y
            .map(|offset| position.y + offset)
            .unwrap_or(position.y - 3.0);
        return (x, y);
    }

    let mut default_x = values.config.node_label_offset;
    if node.source_strategy.is_some() && node.label_offset_x.is_none() {
        default_x += 10.0;
    }
    let mut default_y = -values.config.node_label_offset;
    if node.source_strategy.is_some() && node.label_offset_y.is_none() {
        default_y -= 10.0;
    }
    (
        position.x + node.label_offset_x.unwrap_or(default_x),
        position.y + node.label_offset_y.unwrap_or(default_y),
    )
}

/// `wardley-annotations`: the numbered marks and the text box below them.
#[allow(clippy::too_many_arguments)]
fn annotations_group(
    ast: &Ast,
    theme: &Theme,
    values: &Values,
    width: f64,
    height: f64,
    project_x: &dyn Fn(f64) -> f64,
    project_y: &dyn Fn(f64) -> f64,
    measure: &dyn TextMeasure,
) -> Group {
    let padding = values.config.padding;
    let mut group = Group::default().with_class("wardley-annotations");

    for annotation in &ast.annotations {
        let coordinates: Vec<Point> = annotation
            .coordinates
            .iter()
            .map(|coordinate| Point::new(project_x(coordinate.x), project_y(coordinate.y)))
            .collect();

        for pair in coordinates.windows(2) {
            group.push(line_shape(
                pair[0].x,
                pair[0].y,
                pair[1].x,
                pair[1].y,
                &theme.axis,
                1.5,
                &[4.0, 4.0],
            ));
        }

        for coordinate in &coordinates {
            let mut mark = Group::default().with_class("wardley-annotation");
            mark.push(EllipseShape::new(
                *coordinate,
                10.0,
                10.0,
                PathStyle {
                    fill: Some(Color::new("white")),
                    stroke: Some(Color::new(&theme.axis)),
                    stroke_width: 1.5,
                    ..PathStyle::default()
                },
            ));
            mark.push(text_run(
                &js_number(annotation.number),
                *coordinate,
                TextAnchor::Middle,
                TextBaseline::Central,
                Font::new("monospace", 10.0).with_weight(700),
                Some(&theme.axis_text),
                0.0,
                measure,
            ));
            group.push(mark);
        }
    }

    if let Some(box_point) = ast.annotations_box {
        let mut box_x = project_x(box_point.x);
        let mut box_y = project_y(box_point.y);
        let mut text_box = Group::default().with_class("wardley-annotations-box");

        let mut sorted: Vec<&super::Annotation> = ast
            .annotations
            .iter()
            .filter(|annotation| annotation.text.is_some())
            .collect();
        sorted.sort_by(|a, b| {
            a.number
                .partial_cmp(&b.number)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let font = Font::new("monospace", ANNOTATIONS_FONT_SIZE);
        let labels: Vec<String> = sorted
            .iter()
            .map(|annotation| {
                format!(
                    "{}. {}",
                    js_number(annotation.number),
                    annotation.text.as_deref().unwrap_or_default()
                )
            })
            .collect();
        let max_width = labels
            .iter()
            .map(|label| measure.width(label, &font))
            .fold(0.0, f64::max);
        let max_height = labels
            .iter()
            .map(|_| measure.line_height(&font))
            .fold(0.0, f64::max);

        if !labels.is_empty() {
            let box_width = max_width + ANNOTATIONS_PADDING * 2.0 + ANNOTATIONS_EXTRA_WIDTH;
            let box_height = sorted.len() as f64 * ANNOTATIONS_LINE_HEIGHT
                + ANNOTATIONS_PADDING * 2.0
                + max_height / 2.0;

            let min_x = padding;
            let max_x = width - padding - box_width;
            let min_y = padding;
            let max_y = height - padding - box_height;
            box_x = box_x.max(min_x).min(max_x);
            box_y = box_y.max(min_y).min(max_y);

            text_box.push(RectShape::new(
                Rect::new(box_x, box_y, box_width, box_height),
                4.0,
                PathStyle {
                    fill: Some(Color::new("white")),
                    stroke: Some(Color::new(&theme.axis)),
                    stroke_width: 1.5,
                    ..PathStyle::default()
                },
            ));
            for (index, label) in labels.iter().enumerate() {
                text_box.push(text_run(
                    label,
                    Point::new(
                        box_x + ANNOTATIONS_PADDING,
                        box_y
                            + ANNOTATIONS_PADDING
                            + (index as f64 + 1.0) * ANNOTATIONS_LINE_HEIGHT,
                    ),
                    TextAnchor::Start,
                    TextBaseline::Middle,
                    font.clone(),
                    Some(&theme.axis_text),
                    0.0,
                    measure,
                ));
            }
        }

        group.push(text_box);
    }

    group
}

/// `wardley-notes`: one bold line per note.
fn notes_group(
    ast: &Ast,
    theme: &Theme,
    project_x: &dyn Fn(f64) -> f64,
    project_y: &dyn Fn(f64) -> f64,
) -> Group {
    let mut group = Group::default().with_class("wardley-notes");
    for note in &ast.notes {
        group.push(text_run(
            &note.text,
            Point::new(project_x(note.x), project_y(note.y)),
            TextAnchor::Start,
            TextBaseline::Alphabetic,
            Font::new("monospace", 11.0).with_weight(700),
            Some(&theme.axis_text),
            0.0,
            &FallbackMeasure,
        ));
    }
    group
}

/// `wardley-accelerators`: one right-pointing arrow and its label per accelerator.
fn arrows_group(
    accelerators: &[super::Accelerator],
    theme: &Theme,
    project_x: &dyn Fn(f64) -> f64,
    project_y: &dyn Fn(f64) -> f64,
) -> Group {
    let mut group = Group::default().with_class("wardley-accelerators");
    for accelerator in accelerators {
        let x = project_x(accelerator.x);
        let y = project_y(accelerator.y);
        group.push(PathShape::new(
            arrow_path(x, y, false),
            PathStyle {
                fill: Some(Color::new("white")),
                stroke: Some(Color::new(&theme.component_stroke)),
                stroke_width: 1.0,
                ..PathStyle::default()
            },
        ));
        group.push(text_run(
            &accelerator.name,
            Point::new(x + 30.0, y + 30.0),
            TextAnchor::Middle,
            TextBaseline::Alphabetic,
            Font::new("monospace", 10.0).with_weight(700),
            Some(&theme.axis_text),
            0.0,
            &FallbackMeasure,
        ));
    }
    group
}

/// `wardley-deaccelerators`: the same arrow pointing left.
fn deaccelerators_group(
    ast: &Ast,
    theme: &Theme,
    project_x: &dyn Fn(f64) -> f64,
    project_y: &dyn Fn(f64) -> f64,
) -> Group {
    let mut group = Group::default().with_class("wardley-deaccelerators");
    for deaccelerator in &ast.deaccelerators {
        let x = project_x(deaccelerator.x);
        let y = project_y(deaccelerator.y);
        group.push(PathShape::new(
            arrow_path(x, y, true),
            PathStyle {
                fill: Some(Color::new("white")),
                stroke: Some(Color::new(&theme.component_stroke)),
                stroke_width: 1.0,
                ..PathStyle::default()
            },
        ));
        group.push(text_run(
            &deaccelerator.name,
            Point::new(x + 30.0, y + 30.0),
            TextAnchor::Middle,
            TextBaseline::Alphabetic,
            Font::new("monospace", 10.0).with_weight(700),
            Some(&theme.axis_text),
            0.0,
            &FallbackMeasure,
        ));
    }
    group
}

/// `draw`'s accelerator `arrowPath`: a 60 by 30 shaft with a 20-wide head, pointing right or
/// left.
fn arrow_path(x: f64, y: f64, left: bool) -> Path {
    const WIDTH: f64 = 60.0;
    const HEIGHT: f64 = 30.0;
    const HEAD: f64 = 20.0;
    let mut path = Path::new();
    if left {
        path.move_to(Point::new(x + WIDTH, y - HEIGHT / 2.0));
        path.line_to(Point::new(x + HEAD, y - HEIGHT / 2.0));
        path.line_to(Point::new(x + HEAD, y - HEIGHT / 2.0 - 8.0));
        path.line_to(Point::new(x, y));
        path.line_to(Point::new(x + HEAD, y + HEIGHT / 2.0 + 8.0));
        path.line_to(Point::new(x + HEAD, y + HEIGHT / 2.0));
        path.line_to(Point::new(x + WIDTH, y + HEIGHT / 2.0));
    } else {
        path.move_to(Point::new(x, y - HEIGHT / 2.0));
        path.line_to(Point::new(x + WIDTH - HEAD, y - HEIGHT / 2.0));
        path.line_to(Point::new(x + WIDTH - HEAD, y - HEIGHT / 2.0 - 8.0));
        path.line_to(Point::new(x + WIDTH, y));
        path.line_to(Point::new(x + WIDTH - HEAD, y + HEIGHT / 2.0 + 8.0));
        path.line_to(Point::new(x + WIDTH - HEAD, y + HEIGHT / 2.0));
        path.line_to(Point::new(x, y + HEIGHT / 2.0));
    }
    path.close();
    path
}

/// A straight, optionally dashed and unfilled line.
fn line_shape(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    stroke: &str,
    width: f64,
    dash: &[f64],
) -> PathShape {
    let mut path = Path::new();
    path.move_to(Point::new(x1, y1));
    path.line_to(Point::new(x2, y2));
    PathShape::new(
        path,
        PathStyle {
            fill: None,
            stroke: Some(Color::new(stroke)),
            stroke_width: width,
            dash: dash.to_vec(),
            ..PathStyle::default()
        },
    )
}

/// A filled circle with an optional stroke and stroke width.
fn circle_shape(
    center: Point,
    radius: f64,
    fill: &str,
    stroke: Option<&str>,
    width: f64,
) -> EllipseShape {
    EllipseShape::new(
        center,
        radius,
        radius,
        PathStyle {
            fill: Some(Color::new(fill)),
            stroke: stroke.map(Color::new),
            stroke_width: width,
            ..PathStyle::default()
        },
    )
}

/// A one-line text run, shaped and measured like `createFormattedText` with `htmlLabels:
/// false`. `rotation` turns it clockwise around `position`.
#[allow(clippy::too_many_arguments)]
fn text_run(
    text: &str,
    position: Point,
    anchor: TextAnchor,
    baseline: TextBaseline,
    font: Font,
    fill: Option<&str>,
    rotation: f64,
    measure: &dyn TextMeasure,
) -> TextRun {
    let options = TextOptions {
        width: f64::INFINITY,
        markdown: false,
        min_width: 0.0,
    };
    let block = shape_text(text, &options, &font, measure).unwrap_or_default();
    let mut run = TextRun::new(position, font, block);
    run.anchor = anchor;
    run.baseline = baseline;
    run.rotation = rotation;
    run.fill = fill.map(Color::new);
    run
}

/// `${value}` in a label: JavaScript's `Number::toString` for the integer annotation numbers.
fn js_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value}")
    }
}

#[cfg(test)]
mod tests;
