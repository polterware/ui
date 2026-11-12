//! Source-to-[`Scene`] rendering for radar charts: the Rust side of `radar/renderer.ts`
//! `draw`.
//!
//! A radar chart does not go through the unified layout pipeline: `draw` uses plain d3. It
//! reads the axes, curves, options and title from [`Ast`](super::Ast), centres a group in a
//! width-by-height frame with the four margins, draws the graticule (a ring per tick, or a
//! polygon through the axis angles), the axis lines and their labels, one closed curve per
//! series, one legend row per series, and the title. This module ports that: [`draw`] parses,
//! [`draw_ast`] renders an already parsed database, and [`render`] uses the crate's monospace
//! fallback measurement.
//!
//! The CSS in `radar/styles.ts` is resolved into the shapes, because a [`Scene`] carries paints
//! rather than selectors. The graticule is `graticuleColor` at `graticuleOpacity` with a
//! `graticuleStrokeWidth` stroke, axes are `axisColor` at `axisStrokeWidth`, a curve and its
//! legend box are the theme's `cScale{n}` with `curveOpacity` and `curveStrokeWidth`, and the
//! text is the SVG's inherited `fill` (`textColor`) at the theme's `fontSize` for the title and
//! `axisLabelFontSize`/`legendFontSize` for the labels. `.radarAxisLabel`'s `color` does not
//! paint SVG text, so the labels keep the inherited fill exactly as they do upstream.
//!
//! `closedRoundCurve` is the Catmull-Rom-to-cubic-Bézier conversion with `curveTension`, and
//! `relativeRadius` clips each entry between `min` and `max` before scaling it to the radius,
//! which is `Math.min(width, height) / 2`. A curve without one entry per axis is skipped, as
//! upstream skips it, but it still keeps its index in the palette and the legend.
//!
//! Where the port stops short of upstream: the SVG's `class` names matter only to CSS and are
//! not carried by the shapes, and the frame's group keeps neither class nor id, as upstream's.
//! `configureSvgSize`'s `width` and
//! `max-width` attributes have no scene equivalent, so the scene records the `viewBox`
//! rectangle the renderer sets (the width plus both horizontal margins, the height plus both
//! vertical margins). `useMaxWidth` therefore does not change the scene. A fractional `ticks`
//! draws `ceil(ticks)` rings, matching a JavaScript loop with an integer counter. Text is
//! measured in the `monospace` family, the rule the fixture harness renders with and that
//! [`FallbackMeasure`](crate::measure::FallbackMeasure) reproduces; a curve or graticule with
//! no points draws nothing, where upstream emits an empty polygon or fails on the empty
//! spline.

use std::f64::consts::{FRAC_PI_2, PI};

use serde_json::Value;

use crate::config::Config;
use crate::error::ParseError;
use crate::measure::{Font, TextMeasure, plain_text_block};
use crate::render::shapes::label_image_utils::parse_font_size;
use crate::scene::{
    Color, EllipseShape, Group, Path, PathShape, PathStyle, Point, Rect, RectShape, Scene,
    TextAnchor, TextBaseline, TextRun,
};

use super::Ast;
use super::ast::Graticule;

/// `labelPad` in `drawAxes`.
const LABEL_PAD: f64 = 4.0;
/// The legend square's width and height in `drawLegend`.
const LEGEND_BOX_SIZE: f64 = 12.0;
/// The distance between legend rows in `drawLegend`.
const LEGEND_LINE_HEIGHT: f64 = 20.0;
/// The label's `x` offset inside a legend row in `drawLegend`.
const LEGEND_TEXT_OFFSET: f64 = 16.0;
/// The family the fixture harness measures with, and therefore this port.
const FONT_FAMILY: &str = "monospace";

/// Renders a radar body (the text [`super::parse`] accepts) to a [`Scene`].
///
/// `config` is the diagram configuration after the source's frontmatter and directives have
/// been applied, as `getConfig()` returns it to `draw`.
pub fn draw(body: &str, config: &Config, measure: &dyn TextMeasure) -> Result<Scene, ParseError> {
    let ast = super::parse(body)?;
    Ok(draw_ast(&ast, config, measure))
}

/// Renders an already parsed radar database.
pub fn draw_ast(ast: &Ast, config: &Config, measure: &dyn TextMeasure) -> Scene {
    let radar = &config.radar;
    let style = RadarStyle::from_theme(config);

    let total_width = radar.width + radar.margin_left + radar.margin_right;
    let total_height = radar.height + radar.margin_top + radar.margin_bottom;
    let center = Point::new(
        radar.margin_left + radar.width / 2.0,
        radar.margin_top + radar.height / 2.0,
    );
    let radius = radar.width.min(radar.height) / 2.0;

    let mut root = Group::translated(center.x, center.y);

    draw_graticule(
        &mut root,
        &ast.axes,
        radius,
        ast.options.ticks,
        ast.options.graticule,
        &style,
    );
    draw_axes(
        &mut root,
        &ast.axes,
        radius,
        radar.axis_scale_factor,
        radar.axis_label_factor,
        &style,
        measure,
    );
    draw_curves(
        &mut root,
        &ast.axes,
        &ast.curves,
        ast.options.min,
        max_value(ast),
        ast.options.graticule,
        radar.curve_tension,
        radius,
        &style,
    );
    draw_legend(
        &mut root,
        &ast.curves,
        ast.options.show_legend,
        radar.width,
        radar.height,
        radar.margin_top,
        radar.margin_right,
        &style,
        measure,
    );
    draw_title(
        &mut root,
        &ast.diagram_title,
        radar.height,
        radar.margin_top,
        &style,
        measure,
    );

    let mut scene = Scene::new(Rect::new(0.0, 0.0, total_width, total_height));
    scene.push(root);
    scene
}

/// Renders a radar body with the crate's monospace fallback measurement.
pub fn render(body: &str, config: &Config) -> Result<Scene, ParseError> {
    draw(body, config, &crate::measure::FallbackMeasure)
}

/// The resolved declarations of `radar/styles.ts` and the theme variables it reads.
struct RadarStyle {
    axis_color: Color,
    axis_stroke_width: f64,
    axis_label_font_size: f64,
    curve_opacity: f64,
    curve_stroke_width: f64,
    graticule_color: Color,
    graticule_opacity: f64,
    graticule_stroke_width: f64,
    legend_font_size: f64,
    title_font_size: f64,
    text_color: Color,
    palette: Vec<Color>,
}

impl RadarStyle {
    fn from_theme(config: &Config) -> Self {
        let radar = config
            .theme_variables
            .get("radar")
            .and_then(Value::as_object);
        let field = |key: &str| radar.and_then(|radar| radar.get(key));
        let color = |key: &str, fallback: &str| {
            field(key)
                .and_then(Value::as_str)
                .map(Color::new)
                .unwrap_or_else(|| Color::new(fallback))
        };
        let number = |key: &str, fallback: f64| {
            field(key)
                .and_then(|value| match value {
                    Value::Number(number) => number.as_f64(),
                    Value::String(text) => parse_font_size(text),
                    _ => None,
                })
                .unwrap_or(fallback)
        };
        let text_color = config
            .theme_variables
            .get("textColor")
            .and_then(Value::as_str)
            .map(Color::new)
            .unwrap_or_else(|| Color::new("#333"));
        let title_font_size = config
            .theme_variables
            .get("fontSize")
            .and_then(|value| match value {
                Value::Number(number) => number.as_f64(),
                Value::String(text) => parse_font_size(text),
                _ => None,
            })
            .unwrap_or(16.0);
        let palette = (0..12)
            .filter_map(|index| {
                config
                    .theme_variables
                    .get(&format!("cScale{index}"))
                    .and_then(Value::as_str)
                    .map(Color::new)
            })
            .collect();

        Self {
            axis_color: color("axisColor", "#333333"),
            axis_stroke_width: number("axisStrokeWidth", 2.0),
            axis_label_font_size: number("axisLabelFontSize", 12.0),
            curve_opacity: number("curveOpacity", 0.5),
            curve_stroke_width: number("curveStrokeWidth", 2.0),
            graticule_color: color("graticuleColor", "#DEDEDE"),
            graticule_opacity: number("graticuleOpacity", 0.3),
            graticule_stroke_width: number("graticuleStrokeWidth", 1.0),
            legend_font_size: number("legendFontSize", 12.0),
            title_font_size,
            text_color,
            palette,
        }
    }

    /// The palette entry a curve index paints with; an index past the theme's twelve colors
    /// has no CSS rule upstream, so the shape keeps SVG's default black fill.
    fn curve_color(&self, index: usize) -> Option<&Color> {
        self.palette.get(index)
    }
}

/// `drawGraticule`: a ring per tick, or a polygon through the axis angles.
fn draw_graticule(
    root: &mut Group,
    axes: &[super::ast::Axis],
    radius: f64,
    ticks: f64,
    graticule: Graticule,
    style: &RadarStyle,
) {
    let ring_style = PathStyle {
        fill: Some(style.graticule_color.clone()),
        fill_opacity: style.graticule_opacity,
        stroke: Some(style.graticule_color.clone()),
        stroke_width: style.graticule_stroke_width,
        ..PathStyle::default()
    };
    let mut i = 0.0;
    while i < ticks {
        let r = (radius * (i + 1.0)) / ticks;
        match graticule {
            Graticule::Circle => {
                root.push(EllipseShape::new(Point::ZERO, r, r, ring_style.clone()))
            }
            Graticule::Polygon => {
                let points: Vec<Point> = axes
                    .iter()
                    .enumerate()
                    .map(|(j, _)| {
                        let angle = (2.0 * j as f64 * PI) / axes.len() as f64 - FRAC_PI_2;
                        Point::new(r * angle.cos(), r * angle.sin())
                    })
                    .collect();
                if !points.is_empty() {
                    root.push(PathShape::new(polygon_path(&points), ring_style.clone()));
                }
            }
        }
        i += 1.0;
    }
}

/// `drawAxes`: one radius line and one outward-pushed label per axis.
#[allow(clippy::too_many_arguments)]
fn draw_axes(
    root: &mut Group,
    axes: &[super::ast::Axis],
    radius: f64,
    axis_scale_factor: f64,
    axis_label_factor: f64,
    style: &RadarStyle,
    measure: &dyn TextMeasure,
) {
    let line_style = PathStyle::stroked(style.axis_color.clone(), style.axis_stroke_width);
    let font = Font::new(FONT_FAMILY, style.axis_label_font_size);
    let num_axes = axes.len() as f64;

    for (i, axis) in axes.iter().enumerate() {
        let angle = (2.0 * i as f64 * PI) / num_axes - FRAC_PI_2;
        let (sin_a, cos_a) = angle.sin_cos();

        let mut line = Path::new();
        line.move_to(Point::ZERO);
        line.line_to(Point::new(
            radius * axis_scale_factor * cos_a,
            radius * axis_scale_factor * sin_a,
        ));
        root.push(PathShape::new(line, line_style.clone()));

        let anchor = if cos_a > 0.01 {
            TextAnchor::Start
        } else if cos_a < -0.01 {
            TextAnchor::End
        } else {
            TextAnchor::Middle
        };
        let baseline = if sin_a > 0.01 {
            TextBaseline::Hanging
        } else if sin_a < -0.01 {
            TextBaseline::Alphabetic
        } else {
            TextBaseline::Central
        };
        let block = plain_text_block(&axis.label, &font, measure);
        let mut run = TextRun::new(
            Point::new(
                radius * axis_label_factor * cos_a + LABEL_PAD * cos_a,
                radius * axis_label_factor * sin_a + LABEL_PAD * sin_a,
            ),
            font.clone(),
            block,
        );
        run.anchor = anchor;
        run.baseline = baseline;
        run.fill = Some(style.text_color.clone());
        root.push(run);
    }
}

/// `drawCurves`: one closed path (circle graticule) or polygon per curve that has one entry
/// per axis.
#[allow(clippy::too_many_arguments)]
fn draw_curves(
    root: &mut Group,
    axes: &[super::ast::Axis],
    curves: &[super::ast::Curve],
    min_value: f64,
    max_value: f64,
    graticule: Graticule,
    tension: f64,
    radius: f64,
    style: &RadarStyle,
) {
    let num_axes = axes.len() as f64;

    for (index, curve) in curves.iter().enumerate() {
        if curve.entries.len() != axes.len() {
            continue;
        }
        let points: Vec<Point> = curve
            .entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let angle = (2.0 * PI * i as f64) / num_axes - FRAC_PI_2;
                let r = relative_radius(*entry, min_value, max_value, radius);
                Point::new(r * angle.cos(), r * angle.sin())
            })
            .collect();

        let paint = curve_style(style, index);
        match graticule {
            Graticule::Circle => {
                if points.is_empty() {
                    continue;
                }
                root.push(PathShape::new(closed_round_curve(&points, tension), paint));
            }
            Graticule::Polygon => {
                if points.is_empty() {
                    continue;
                }
                root.push(PathShape::new(polygon_path(&points), paint));
            }
        }
    }
}

/// `drawLegend`: one square marker and label per curve, in the upper-right corner.
#[allow(clippy::too_many_arguments)]
fn draw_legend(
    root: &mut Group,
    curves: &[super::ast::Curve],
    show_legend: bool,
    width: f64,
    height: f64,
    margin_top: f64,
    margin_right: f64,
    style: &RadarStyle,
    measure: &dyn TextMeasure,
) {
    if !show_legend {
        return;
    }

    let legend_x = ((width / 2.0 + margin_right) * 3.0) / 4.0;
    let legend_y = (-(height / 2.0 + margin_top) * 3.0) / 4.0;
    let font = Font::new(FONT_FAMILY, style.legend_font_size);

    for (index, curve) in curves.iter().enumerate() {
        let mut row = Group::translated(legend_x, legend_y + index as f64 * LEGEND_LINE_HEIGHT);

        let box_style = match style.curve_color(index) {
            Some(color) => PathStyle {
                fill: Some(color.clone()),
                fill_opacity: style.curve_opacity,
                stroke: Some(color.clone()),
                stroke_width: 1.0,
                ..PathStyle::default()
            },
            None => PathStyle::default(),
        };
        row.push(RectShape::new(
            Rect::new(0.0, 0.0, LEGEND_BOX_SIZE, LEGEND_BOX_SIZE),
            0.0,
            box_style,
        ));

        let block = plain_text_block(&curve.label, &font, measure);
        let mut text = TextRun::new(Point::new(LEGEND_TEXT_OFFSET, 0.0), font.clone(), block);
        text.anchor = TextAnchor::Start;
        text.baseline = TextBaseline::Hanging;
        text.fill = Some(style.text_color.clone());
        row.push(text);

        root.push(row);
    }
}

/// The title above the chart, at `y = -height / 2 - marginTop`.
fn draw_title(
    root: &mut Group,
    title: &str,
    height: f64,
    margin_top: f64,
    style: &RadarStyle,
    measure: &dyn TextMeasure,
) {
    let font = Font::new(FONT_FAMILY, style.title_font_size);
    let block = plain_text_block(title, &font, measure);
    let mut run = TextRun::new(Point::new(0.0, -height / 2.0 - margin_top), font, block);
    run.anchor = TextAnchor::Middle;
    run.baseline = TextBaseline::Hanging;
    run.fill = Some(style.text_color.clone());
    root.push(run);
}

/// The curve's paint: its palette color, or SVG's default fill when the palette has no entry.
fn curve_style(style: &RadarStyle, index: usize) -> PathStyle {
    match style.curve_color(index) {
        Some(color) => PathStyle {
            fill: Some(color.clone()),
            fill_opacity: style.curve_opacity,
            stroke: Some(color.clone()),
            stroke_width: style.curve_stroke_width,
            ..PathStyle::default()
        },
        None => PathStyle::default(),
    }
}

/// The maximum value the chart scales to: `options.max`, or the largest entry upstream's
/// `Math.max(...curves.map(...))` finds.
fn max_value(ast: &Ast) -> f64 {
    ast.options.max.unwrap_or_else(|| {
        ast.curves
            .iter()
            .flat_map(|curve| curve.entries.iter())
            .fold(f64::NEG_INFINITY, |max, value| max.max(*value))
    })
}

/// `relativeRadius`: clip between `min` and `max`, then scale to the radius.
pub fn relative_radius(value: f64, min_value: f64, max_value: f64, radius: f64) -> f64 {
    let clipped = value.max(min_value).min(max_value);
    (radius * (clipped - min_value)) / (max_value - min_value)
}

/// `closedRoundCurve`: a closed Catmull-Rom spline as cubic Béziers, ending with `Z`.
pub fn closed_round_curve(points: &[Point], tension: f64) -> Path {
    let mut path = Path::new();
    let num_points = points.len();
    if num_points == 0 {
        return path;
    }
    path.move_to(points[0]);
    for i in 0..num_points {
        let p0 = points[(i + num_points - 1) % num_points];
        let p1 = points[i];
        let p2 = points[(i + 1) % num_points];
        let p3 = points[(i + 2) % num_points];
        let control1 = Point::new(
            p1.x + (p2.x - p0.x) * tension,
            p1.y + (p2.y - p0.y) * tension,
        );
        let control2 = Point::new(
            p2.x - (p3.x - p1.x) * tension,
            p2.y - (p3.y - p1.y) * tension,
        );
        path.cubic_to(control1, control2, p2);
    }
    path.close();
    path
}

/// An SVG `<polygon>` as a closed path: move to the first point, line through the rest, close.
fn polygon_path(points: &[Point]) -> Path {
    let mut path = Path::new();
    if points.is_empty() {
        return path;
    }
    path.move_to(points[0]);
    for point in &points[1..] {
        path.line_to(*point);
    }
    path.close();
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Item, PathCommand};
    use serde_json::Value;

    const TOLERANCE: f64 = 0.01;

    fn fixtures() -> Vec<(String, Value)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/diagrams/radar/tests/render");
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

    fn path<'a>(item: &'a Item, file: &str, what: &str) -> &'a PathShape {
        match item {
            Item::Path(path) => path,
            _ => panic!("{file}: {what} is not a path"),
        }
    }

    fn ellipse<'a>(item: &'a Item, file: &str, what: &str) -> &'a EllipseShape {
        match item {
            Item::Ellipse(ellipse) => ellipse,
            _ => panic!("{file}: {what} is not an ellipse"),
        }
    }

    fn text_of(run: &TextRun) -> String {
        run.lines
            .iter()
            .map(|line| line.text())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn color_of(value: &Value) -> Option<Color> {
        value.as_str().map(Color::new)
    }

    fn assert_style(style: &PathStyle, expected: &Value, file: &str, what: &str) {
        assert_eq!(
            style.fill,
            color_of(&expected["fill"]),
            "{file}: {what}: fill"
        );
        close(
            style.fill_opacity,
            expected["fillOpacity"].as_f64().expect("fillOpacity"),
            &format!("{what} fillOpacity"),
            file,
        );
        assert_eq!(
            style.stroke,
            color_of(&expected["stroke"]),
            "{file}: {what}: stroke"
        );
        close(
            style.stroke_width,
            expected["strokeWidth"].as_f64().expect("strokeWidth"),
            &format!("{what} strokeWidth"),
            file,
        );
        close(
            style.stroke_opacity,
            expected["strokeOpacity"].as_f64().expect("strokeOpacity"),
            &format!("{what} strokeOpacity"),
            file,
        );
        close(
            style.opacity,
            expected["opacity"].as_f64().expect("opacity"),
            &format!("{what} opacity"),
            file,
        );
    }

    fn assert_commands(actual: &Path, expected: &Value, file: &str, what: &str) {
        let expected = expected
            .as_array()
            .unwrap_or_else(|| panic!("{file}: {what} commands"));
        assert_eq!(
            actual.commands.len(),
            expected.len(),
            "{file}: {what}: command count"
        );
        for (index, (actual, expected)) in actual.commands.iter().zip(expected).enumerate() {
            let op = expected["op"].as_str().expect("command op");
            let label = format!("{what}[{index}]");
            match (op, actual) {
                ("moveTo", PathCommand::MoveTo { x, y })
                | ("lineTo", PathCommand::LineTo { x, y }) => {
                    close(
                        *x,
                        expected["x"].as_f64().expect("x"),
                        &format!("{label} x"),
                        file,
                    );
                    close(
                        *y,
                        expected["y"].as_f64().expect("y"),
                        &format!("{label} y"),
                        file,
                    );
                }
                (
                    "cubicTo",
                    PathCommand::CubicTo {
                        c1x,
                        c1y,
                        c2x,
                        c2y,
                        x,
                        y,
                    },
                ) => {
                    for key in ["c1x", "c1y", "c2x", "c2y", "x", "y"] {
                        let actual = match key {
                            "c1x" => *c1x,
                            "c1y" => *c1y,
                            "c2x" => *c2x,
                            "c2y" => *c2y,
                            "x" => *x,
                            _ => *y,
                        };
                        close(
                            actual,
                            expected[key].as_f64().expect("cubic coordinate"),
                            &format!("{label} {key}"),
                            file,
                        );
                    }
                }
                ("quadTo", PathCommand::QuadTo { cx, cy, x, y }) => {
                    for (key, actual) in [("cx", *cx), ("cy", *cy), ("x", *x), ("y", *y)] {
                        close(
                            actual,
                            expected[key].as_f64().expect("quad coordinate"),
                            &format!("{label} {key}"),
                            file,
                        );
                    }
                }
                (
                    "arcTo",
                    PathCommand::ArcTo {
                        rx,
                        ry,
                        rotation,
                        large_arc,
                        sweep,
                        x,
                        y,
                    },
                ) => {
                    for (key, actual) in [
                        ("rx", *rx),
                        ("ry", *ry),
                        ("rotation", *rotation),
                        ("x", *x),
                        ("y", *y),
                    ] {
                        close(
                            actual,
                            expected[key].as_f64().expect("arc coordinate"),
                            &format!("{label} {key}"),
                            file,
                        );
                    }
                    assert_eq!(
                        *large_arc,
                        expected["large_arc"].as_bool().expect("large_arc")
                    );
                    assert_eq!(*sweep, expected["sweep"].as_bool().expect("sweep"));
                }
                ("close", PathCommand::Close) => {}
                _ => panic!("{file}: {label}: unexpected {op}"),
            }
        }
    }

    /// The commands an SVG `<polygon points>` becomes in a scene path.
    fn polygon_commands(points: &Value) -> Value {
        let points = points.as_array().expect("points");
        let mut commands = Vec::new();
        for (index, point) in points.iter().enumerate() {
            commands.push(serde_json::json!({
                "op": if index == 0 { "moveTo" } else { "lineTo" },
                "x": point["x"],
                "y": point["y"],
            }));
        }
        commands.push(serde_json::json!({ "op": "close" }));
        Value::Array(commands)
    }

    fn assert_text(run: &TextRun, expected: &Value, file: &str, what: &str) {
        assert_eq!(
            text_of(run),
            expected["text"].as_str().expect("text"),
            "{file}: {what}: text"
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
        close(
            run.font.size,
            expected["fontSize"].as_f64().expect("font size"),
            &format!("{what} font size"),
            file,
        );
        assert_eq!(
            run.anchor,
            match expected["anchor"].as_str().expect("anchor") {
                "middle" => TextAnchor::Middle,
                "end" => TextAnchor::End,
                _ => TextAnchor::Start,
            },
            "{file}: {what}: anchor"
        );
        assert_eq!(
            run.baseline,
            match expected["baseline"].as_str().expect("baseline") {
                "hanging" => TextBaseline::Hanging,
                "central" => TextBaseline::Central,
                _ => TextBaseline::Alphabetic,
            },
            "{file}: {what}: baseline"
        );
        assert_eq!(
            run.fill,
            color_of(&expected["fill"]),
            "{file}: {what}: fill"
        );
    }

    #[test]
    fn renders_recorded_radar_fixtures() {
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
            let scene = draw_ast(&ast, &config, &crate::measure::FallbackMeasure);
            let expected = &fixture["expected"];

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

            let root = group(&scene.items[0], &file, "root");
            close(
                root.transform.e,
                expected["frame"]["transform"]["x"]
                    .as_f64()
                    .expect("frame x"),
                "frame x",
                &file,
            );
            close(
                root.transform.f,
                expected["frame"]["transform"]["y"]
                    .as_f64()
                    .expect("frame y"),
                "frame y",
                &file,
            );

            let mut index = 0;

            let graticule = expected["graticule"].as_array().expect("graticule");
            for (ring, expected_ring) in graticule.iter().enumerate() {
                let item = &root.items[index];
                index += 1;
                let what = format!("graticule[{ring}]");
                match expected_ring["kind"].as_str().expect("graticule kind") {
                    "circle" => {
                        let circle = ellipse(item, &file, &what);
                        close(circle.center.x, 0.0, &format!("{what} cx"), &file);
                        close(circle.center.y, 0.0, &format!("{what} cy"), &file);
                        close(
                            circle.radius_x,
                            expected_ring["r"].as_f64().expect("graticule r"),
                            &format!("{what} r"),
                            &file,
                        );
                        close(
                            circle.radius_y,
                            expected_ring["r"].as_f64().expect("graticule r"),
                            &format!("{what} radius y"),
                            &file,
                        );
                        assert_style(&circle.style, &expected_ring["style"], &file, &what);
                    }
                    "polygon" => {
                        let path = path(item, &file, &what);
                        let commands = polygon_commands(&expected_ring["points"]);
                        assert_commands(&path.path, &commands, &file, &what);
                        assert_style(&path.style, &expected_ring["style"], &file, &what);
                    }
                    other => panic!("{file}: unknown graticule kind {other}"),
                }
            }

            let axes = expected["axes"].as_array().expect("axes");
            let labels = expected["axisLabels"].as_array().expect("axis labels");
            assert_eq!(axes.len(), labels.len(), "{file}: axis label count");
            for (axis, (expected_axis, expected_label)) in axes.iter().zip(labels).enumerate() {
                let what = format!("axis[{axis}]");
                let path = path(&root.items[index], &file, &what);
                index += 1;
                let commands = serde_json::json!([
                    { "op": "moveTo", "x": 0.0, "y": 0.0 },
                    {
                        "op": "lineTo",
                        "x": expected_axis["x2"],
                        "y": expected_axis["y2"],
                    }
                ]);
                assert_commands(&path.path, &commands, &file, &what);
                assert_eq!(
                    path.style.stroke,
                    color_of(&expected_axis["stroke"]),
                    "{file}: {what}: stroke"
                );
                close(
                    path.style.stroke_width,
                    expected_axis["strokeWidth"]
                        .as_f64()
                        .expect("axis stroke width"),
                    &format!("{what} stroke width"),
                    &file,
                );

                let what = format!("axisLabel[{axis}]");
                assert_text(
                    text(&root.items[index], &file, &what),
                    expected_label,
                    &file,
                    &what,
                );
                index += 1;
            }

            let curves = expected["curves"].as_array().expect("curves");
            for (curve, expected_curve) in curves.iter().enumerate() {
                let item = &root.items[index];
                index += 1;
                let what = format!("curve[{curve}]");
                let path = path(item, &file, &what);
                let commands = if expected_curve["kind"].as_str() == Some("path") {
                    expected_curve["commands"].clone()
                } else {
                    polygon_commands(&expected_curve["points"])
                };
                assert_commands(&path.path, &commands, &file, &what);
                assert_style(&path.style, &expected_curve["style"], &file, &what);
            }

            let legend = expected["legend"].as_array().expect("legend");
            for (row, expected_row) in legend.iter().enumerate() {
                let item = &root.items[index];
                index += 1;
                let what = format!("legend[{row}]");
                let group = group(item, &file, &what);
                close(
                    group.transform.e,
                    expected_row["transform"]["x"].as_f64().expect("legend x"),
                    &format!("{what} x"),
                    &file,
                );
                close(
                    group.transform.f,
                    expected_row["transform"]["y"].as_f64().expect("legend y"),
                    &format!("{what} y"),
                    &file,
                );
                let rect = match &group.items[0] {
                    Item::Rect(rect) => rect,
                    _ => panic!("{file}: {what} rect is not a rect"),
                };
                close(
                    rect.rect.width,
                    expected_row["rect"]["width"]
                        .as_f64()
                        .expect("legend width"),
                    &format!("{what} width"),
                    &file,
                );
                close(
                    rect.rect.height,
                    expected_row["rect"]["height"]
                        .as_f64()
                        .expect("legend height"),
                    &format!("{what} height"),
                    &file,
                );
                assert_style(&rect.style, &expected_row["rect"]["style"], &file, &what);
                assert_text(
                    text(&group.items[1], &file, &what),
                    &expected_row["text"],
                    &file,
                    &what,
                );
            }

            match &expected["title"] {
                Value::Null => {}
                expected_title => {
                    let item = &root.items[index];
                    index += 1;
                    assert_text(text(item, &file, "title"), expected_title, &file, "title");
                }
            }

            assert_eq!(
                index,
                root.items.len(),
                "{file}: unexpected items after the title"
            );
        }
    }

    #[test]
    fn relative_radius_clips_and_scales() {
        assert_eq!(relative_radius(5.0, 0.0, 10.0, 300.0), 150.0);
        assert_eq!(relative_radius(-5.0, 0.0, 10.0, 300.0), 0.0);
        assert_eq!(relative_radius(15.0, 0.0, 10.0, 300.0), 300.0);
    }

    #[test]
    fn closed_round_curve_starts_at_the_first_point_and_closes() {
        let points = [
            Point::new(0.0, -10.0),
            Point::new(10.0, 0.0),
            Point::new(0.0, 10.0),
            Point::new(-10.0, 0.0),
        ];
        let path = closed_round_curve(&points, 0.17);
        assert_eq!(
            path.commands.first(),
            Some(&PathCommand::MoveTo { x: 0.0, y: -10.0 })
        );
        assert_eq!(path.commands.last(), Some(&PathCommand::Close));
        // One `M`, four cubic segments (one per point), and `Z`.
        assert_eq!(path.commands.len(), 6);
    }

    #[test]
    fn render_diagram_dispatches_radar() {
        let scene = crate::render_diagram(
            "radar-beta\naxis a[\"A\"], b[\"B\"], c[\"C\"]\ncurve one{1, 2, 3}\nmax 3\n",
            &Config::default(),
        )
        .expect("dispatch radar");
        assert_eq!(scene.items.len(), 1);
    }
}
