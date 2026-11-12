//! Source-to-[`Scene`] rendering for pie charts: the Rust side of `pieRenderer.ts` `draw`.
//!
//! A pie chart does not go through the unified layout pipeline: `draw` uses plain d3. It reads
//! the sections from [`PieDB`](super::db), keeps those whose share is at least 1%, lays them
//! out with `d3.pie().value(...).sort(null)` (input order over a full turn), drops the arcs
//! whose percentage rounds to zero, draws each arc with `d3.arc()` in the theme's pie colors,
//! puts the rounded percentage at the label arc's centroid, appends the title, and appends one
//! legend row per section at the configured position. This module ports that: [`draw`] parses,
//! [`draw_ast`] renders an already parsed database, and [`render`] uses the crate's monospace
//! fallback measurement.
//!
//! The constants are `draw`'s: a 450-unit square, a 40-unit margin, an 18-unit legend square
//! with 4 units of spacing, and an outer circle one half outer stroke wider than the radius.
//! The title sits at `y = -(height - 50) / 2` inside the group centred on the square, and the
//! viewport is the rectangle `draw` sets, widened left of zero when a long title would be
//! clipped.
//!
//! Text is measured in the `monospace` family, the rule the fixture harness renders with and
//! that [`FallbackMeasure`](crate::measure::FallbackMeasure) reproduces; the theme's family
//! would measure differently from the harness's browser font. Font sizes are the theme's
//! `pieSectionTextSize`, `pieLegendTextSize` and `pieTitleTextSize`, read with upstream's
//! `parseFontSize`.
//!
//! Where the port stops short of upstream: the class names (`pieCircle`, `legend`,
//! `pieTitleText`, ...) matter only to CSS, and a [`Scene`] carries paints rather than
//! selectors, so `draw` resolves the classes' declarations (stroke, stroke width, opacity,
//! fills) into the shapes. The `highlightSlice` class only scales and brightens a slice on
//! hover, which the scene does not express. An empty title is not emitted as an empty run.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use serde_json::Value;

use crate::config::Config;
use crate::error::ParseError;
use crate::measure::{Font, TextMeasure, TextOptions, shape_text};
use crate::render::shapes::label_image_utils::parse_font_size;
use crate::scene::{
    Color, EllipseShape, Group, Item, Path, PathShape, PathStyle, Point, Rect,
    RectShape, Scene, TextAnchor, TextRun, Transform,
};

use super::Ast;
use super::ast::Section;

/// `draw`'s `MARGIN`.
const MARGIN: f64 = 40.0;
/// `draw`'s `LEGEND_RECT_SIZE`.
const LEGEND_RECT_SIZE: f64 = 18.0;
/// `draw`'s `LEGEND_SPACING`.
const LEGEND_SPACING: f64 = 4.0;
/// `draw`'s `height` (and `pieWidth`, which copies it).
const HEIGHT: f64 = 450.0;
/// d3's `epsilon`, the threshold below which an angle or radius counts as empty.
const EPSILON: f64 = 1e-6;

/// Renders a pie body (the text [`super::parse`] accepts) to a [`Scene`].
///
/// `config` is the diagram configuration after the source's frontmatter and directives have
/// been applied, as `getConfig()` returns it to `draw`.
pub fn draw(body: &str, config: &Config, measure: &dyn TextMeasure) -> Result<Scene, ParseError> {
    let ast = super::parse(body)?;
    Ok(draw_ast(&ast, config, measure))
}

/// Renders an already parsed pie database.
pub fn draw_ast(ast: &Ast, config: &Config, measure: &dyn TextMeasure) -> Scene {
    draw_ast_with_theme(ast, config, &config.theme_variables, measure)
}

/// Renders a pie body with the crate's monospace fallback measurement.
pub fn render(body: &str, config: &Config) -> Result<Scene, ParseError> {
    draw(body, config, &crate::measure::FallbackMeasure)
}

/// One slice of the pie after `d3.pie()`: its angles in radians.
struct PieArc<'a> {
    section: &'a Section,
    start_angle: f64,
    end_angle: f64,
}

/// Renders an already parsed database with the theme variables `draw` reads.
fn draw_ast_with_theme(
    ast: &Ast,
    config: &Config,
    theme: &serde_json::Map<String, Value>,
    measure: &dyn TextMeasure,
) -> Scene {
    let pie_config = &config.pie;
    let total: f64 = ast.sections.iter().map(|section| section.value).sum();

    // `createPieArcs`: sections under 1% are dropped before d3 sees them, then `d3.pie()`
    // lays the rest out in input order over a full turn.
    let kept: Vec<&Section> = ast
        .sections
        .iter()
        .filter(|section| (section.value / total) * 100.0 >= 1.0)
        .collect();
    let kept_total: f64 = kept.iter().map(|section| section.value).sum();
    let step = if kept_total > 0.0 {
        TAU / kept_total
    } else {
        0.0
    };
    let mut start = 0.0;
    let arcs: Vec<PieArc<'_>> = kept
        .into_iter()
        .map(|section| {
            let end = start
                + if section.value > 0.0 {
                    section.value * step
                } else {
                    0.0
                };
            let arc = PieArc {
                section,
                start_angle: start,
                end_angle: end,
            };
            start = end;
            arc
        })
        .collect();

    // `filteredArcs`: a slice whose percentage rounds to 0% is not drawn.
    let arcs: Vec<PieArc<'_>> = arcs
        .into_iter()
        .filter(|arc| ((arc.section.value / total) * 100.0).round() != 0.0)
        .collect();

    let radius = HEIGHT / 2.0 - MARGIN;
    let inner_hole = if pie_config.donut_hole > 0.0 && pie_config.donut_hole <= 0.9 {
        pie_config.donut_hole
    } else {
        0.0
    };
    let inner_radius = inner_hole * radius;

    let outer_stroke_width = theme_color(theme, "pieOuterStrokeWidth")
        .and_then(parse_font_size)
        .unwrap_or(2.0);
    let stroke_width = theme_color(theme, "pieStrokeWidth")
        .and_then(parse_font_size)
        .unwrap_or(2.0);
    let opacity = theme_color(theme, "pieOpacity")
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(1.0);

    let palette: Vec<Option<Color>> = (1..=12)
        .map(|index| theme_color(theme, &format!("pie{index}")).map(Color::new))
        .collect();
    let color_of = |label: &str| -> Option<Color> {
        let index = ast
            .sections
            .iter()
            .position(|section| section.label == label)
            .unwrap_or(0);
        palette[index % palette.len()].clone()
    };

    let section_font = Font::new("monospace", text_size(theme, "pieSectionTextSize", 17.0));
    let legend_font = Font::new("monospace", text_size(theme, "pieLegendTextSize", 17.0));
    let title_font = Font::new("monospace", text_size(theme, "pieTitleTextSize", 25.0));

    let longest_text_width = ast
        .sections
        .iter()
        .map(|section| legend_text(section, ast.show_data))
        .map(|text| measure.width(&text, &legend_font))
        .fold(0.0, f64::max);
    let legend_height = LEGEND_RECT_SIZE + LEGEND_SPACING;
    let legend_count = ast.sections.len() as f64;
    let total_legend_height = legend_count * legend_height;
    let placement = match pie_config.legend_position.as_str() {
        "top" => LegendPlacement::Top,
        "bottom" => LegendPlacement::Bottom,
        "left" => LegendPlacement::Left,
        "center" => LegendPlacement::Center,
        _ => LegendPlacement::Right,
    };

    let mut root = Group::translated(HEIGHT / 2.0, HEIGHT / 2.0);
    let mut pie_group = Group::default();

    // The circle is one half outer stroke wider than the radius, so the stroke's outer edge
    // stays on the ring the margin leaves.
    let mut outer_style = PathStyle::default();
    outer_style.fill = None;
    outer_style.stroke = theme_color(theme, "pieOuterStrokeColor").map(Color::new);
    outer_style.stroke_width = outer_stroke_width;
    pie_group.push(EllipseShape::new(
        Point::ZERO,
        radius + outer_stroke_width / 2.0,
        radius + outer_stroke_width / 2.0,
        outer_style,
    ));

    let path_style = PathStyle {
        stroke: theme_color(theme, "pieStrokeColor").map(Color::new),
        stroke_width,
        opacity,
        ..PathStyle::default()
    };
    for arc in &arcs {
        let mut style = path_style.clone();
        style.fill = color_of(&arc.section.label);
        pie_group.push(PathShape::new(
            arc_path(arc.start_angle, arc.end_angle, inner_radius, radius),
            style,
        ));
    }

    let label_radius = radius * pie_config.text_position;
    for arc in &arcs {
        let middle = (arc.start_angle + arc.end_angle) / 2.0 - FRAC_PI_2;
        let position = Point::new(label_radius * middle.cos(), label_radius * middle.sin());
        let percentage = format!("{}%", ((arc.section.value / total) * 100.0).round());
        pie_group.push(text_run(
            &percentage,
            position,
            TextAnchor::Middle,
            &section_font,
            theme_color(theme, "pieSectionTextColor"),
            measure,
        ));
    }

    pie_group.transform = Transform::translate(
        match placement {
            LegendPlacement::Left => longest_text_width + LEGEND_RECT_SIZE + LEGEND_SPACING,
            _ => 0.0,
        },
        match placement {
            LegendPlacement::Top => total_legend_height + legend_height,
            _ => 0.0,
        },
    );
    root.push(pie_group);

    if !ast.diagram_title.is_empty() {
        root.push(text_run(
            &ast.diagram_title,
            Point::new(0.0, -(HEIGHT - 50.0) / 2.0),
            TextAnchor::Middle,
            &title_font,
            theme_color(theme, "pieTitleTextColor"),
            measure,
        ));
    }

    for (index, section) in ast.sections.iter().enumerate() {
        let vertical = index as f64 * legend_height
            - match placement {
                LegendPlacement::Top => radius,
                LegendPlacement::Bottom => -radius - legend_height,
                LegendPlacement::Center | LegendPlacement::Left | LegendPlacement::Right => {
                    legend_height * legend_count / 2.0
                }
            };
        let horizontal = match placement {
            LegendPlacement::Left => -radius - (LEGEND_RECT_SIZE + LEGEND_SPACING),
            LegendPlacement::Right => 12.0 * LEGEND_RECT_SIZE,
            LegendPlacement::Top | LegendPlacement::Bottom | LegendPlacement::Center => {
                -longest_text_width / 2.0 - (LEGEND_RECT_SIZE + LEGEND_SPACING)
            }
        };

        let mut legend = Group::translated(horizontal, vertical).with_class("legend");
        let color = color_of(&section.label);
        let mut rect_style = PathStyle::default();
        rect_style.fill = color.clone();
        rect_style.stroke = color;
        legend.push(RectShape::new(
            Rect::new(0.0, 0.0, LEGEND_RECT_SIZE, LEGEND_RECT_SIZE),
            0.0,
            rect_style,
        ));
        legend.push(text_run(
            &legend_text(section, ast.show_data),
            Point::new(
                LEGEND_RECT_SIZE + LEGEND_SPACING,
                LEGEND_RECT_SIZE - LEGEND_SPACING,
            ),
            TextAnchor::Start,
            &legend_font,
            theme_color(theme, "pieLegendTextColor"),
            measure,
        ));
        root.push(legend);
    }

    let mut chart_and_legend_height = HEIGHT;
    let mut chart_and_legend_width = HEIGHT + MARGIN;
    match placement {
        LegendPlacement::Top | LegendPlacement::Bottom => {
            chart_and_legend_height += total_legend_height;
        }
        LegendPlacement::Left | LegendPlacement::Right => {
            chart_and_legend_width += LEGEND_RECT_SIZE + LEGEND_SPACING + longest_text_width;
        }
        LegendPlacement::Center => {}
    }

    let title_width = if ast.diagram_title.is_empty() {
        0.0
    } else {
        measure.width(&ast.diagram_title, &title_font)
    };
    let title_left = HEIGHT / 2.0 - title_width / 2.0;
    let title_right = HEIGHT / 2.0 + title_width / 2.0;
    let view_box_x = 0.0_f64.min(title_left);
    let view_box_right = chart_and_legend_width.max(title_right);
    let view_box = Rect::new(
        view_box_x,
        0.0,
        view_box_right - view_box_x,
        chart_and_legend_height,
    );

    let mut scene = Scene::new(view_box);
    scene.push(root);
    scene
}

/// The four legend placements `draw` branches on; every other value falls to `right`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LegendPlacement {
    Top,
    Bottom,
    Left,
    Right,
    Center,
}

/// A theme variable that is a string, as `getConfig().themeVariables` stores colors and sizes.
fn theme_color<'a>(
    theme: &'a serde_json::Map<String, Value>,
    key: &str,
) -> Option<&'a str> {
    theme.get(key).and_then(Value::as_str)
}

/// A theme's text size in px (`parseFontSize`), or `fallback`.
fn text_size(theme: &serde_json::Map<String, Value>, key: &str, fallback: f64) -> f64 {
    theme_color(theme, key)
        .and_then(parse_font_size)
        .unwrap_or(fallback)
}

/// A one-line text run at `position`, shaped and measured like `createFormattedText` with
/// `htmlLabels: false`.
fn text_run(
    text: &str,
    position: Point,
    anchor: TextAnchor,
    font: &Font,
    fill: Option<&str>,
    measure: &dyn TextMeasure,
) -> TextRun {
    let options = TextOptions {
        width: f64::INFINITY,
        markdown: false,
        min_width: 0.0,
    };
    let block = shape_text(text, &options, font, measure).unwrap_or_default();
    let mut run = TextRun::new(position, font.clone(), block);
    run.anchor = anchor;
    run.fill = fill.map(Color::new);
    run
}

/// The legend row's text: the label, and the value in brackets under `showData`.
fn legend_text(section: &Section, show_data: bool) -> String {
    if show_data {
        format!("{} [{}]", section.label, section.value)
    } else {
        section.label.clone()
    }
}

/// `d3.arc()`'s path for one pie slice, with no padding and no rounded corners, which is what
/// `pieRenderer.ts` asks for: an outer ring arc, a line to the inner ring (or the centre), an
/// inner arc back, and `Z`.
fn arc_path(start_angle: f64, end_angle: f64, inner_radius: f64, outer_radius: f64) -> Path {
    let mut context = ArcContext::new();
    let a0 = start_angle - FRAC_PI_2;
    let a1 = end_angle - FRAC_PI_2;
    let da = (a1 - a0).abs();
    let clockwise = a1 > a0;
    let (r0, r1) = if outer_radius < inner_radius {
        (outer_radius, inner_radius)
    } else {
        (inner_radius, outer_radius)
    };

    if !(r1 > EPSILON) {
        context.move_to(0.0, 0.0);
    } else if da > TAU - EPSILON {
        context.move_to(r1 * a0.cos(), r1 * a0.sin());
        context.arc(0.0, 0.0, r1, a0, a1, !clockwise);
        if r0 > EPSILON {
            context.move_to(r0 * a1.cos(), r0 * a1.sin());
            context.arc(0.0, 0.0, r0, a1, a0, clockwise);
        }
    } else {
        let x01 = r1 * a0.cos();
        let y01 = r1 * a0.sin();
        let x10 = r0 * a1.cos();
        let y10 = r0 * a1.sin();

        if !(da > EPSILON) {
            context.move_to(x01, y01);
        } else {
            context.move_to(x01, y01);
            context.arc(0.0, 0.0, r1, a0, a1, !clockwise);
        }

        if !(r0 > EPSILON) || !(da > EPSILON) {
            context.line_to(x10, y10);
        } else {
            context.arc(0.0, 0.0, r0, a1, a0, clockwise);
        }
    }
    context.close();
    context.path
}

/// `d3-path`'s `Path`, restricted to what `d3.arc()` emits, in [`PathCommand`]s. It tracks the
/// current point and the subpath's start so `arc` can decide between a move, a line, and an
/// arc, exactly as `d3-path` does.
struct ArcContext {
    path: Path,
    start: Point,
    current: Option<Point>,
}

impl ArcContext {
    fn new() -> Self {
        Self {
            path: Path::new(),
            start: Point::ZERO,
            current: None,
        }
    }

    fn move_to(&mut self, x: f64, y: f64) {
        self.path.move_to(Point::new(x, y));
        self.start = Point::new(x, y);
        self.current = Some(self.start);
    }

    fn line_to(&mut self, x: f64, y: f64) {
        self.path.line_to(Point::new(x, y));
        self.current = Some(Point::new(x, y));
    }

    fn arc(&mut self, x: f64, y: f64, radius: f64, a0: f64, a1: f64, ccw: bool) {
        let dx = radius * a0.cos();
        let dy = radius * a0.sin();
        let start = Point::new(x + dx, y + dy);
        let sweep = !ccw;
        let mut da = if ccw { a0 - a1 } else { a1 - a0 };

        match self.current {
            None => self.move_to(start.x, start.y),
            Some(current)
                if (current.x - start.x).abs() > EPSILON
                    || (current.y - start.y).abs() > EPSILON =>
            {
                self.line_to(start.x, start.y);
            }
            Some(_) => {}
        }

        if radius == 0.0 {
            return;
        }
        if da < 0.0 {
            da = da % TAU + TAU;
        }
        if da > TAU - EPSILON {
            self.arc_to(radius, true, sweep, x - dx, y - dy);
            self.arc_to(radius, true, sweep, start.x, start.y);
        } else if da > EPSILON {
            let end = Point::new(x + radius * a1.cos(), y + radius * a1.sin());
            self.arc_to(radius, da >= PI, sweep, end.x, end.y);
        }
    }

    fn arc_to(&mut self, radius: f64, large_arc: bool, sweep: bool, x: f64, y: f64) {
        self.path
            .arc_to(radius, radius, 0.0, large_arc, sweep, Point::new(x, y));
        self.current = Some(Point::new(x, y));
    }

    fn close(&mut self) {
        if self.current.is_some() {
            self.path.close();
            self.current = Some(self.start);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOLERANCE: f64 = 0.01;

    fn fixtures() -> Vec<(String, Value)> {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/diagrams/pie/tests/render");
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

    fn text_of(run: &TextRun) -> String {
        run.lines
            .iter()
            .map(|line| line.text())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn renders_recorded_pie_fixtures() {
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
                expected["rootTransform"]["x"].as_f64().expect("root x"),
                "root x",
                &file,
            );
            close(
                root.transform.f,
                expected["rootTransform"]["y"].as_f64().expect("root y"),
                "root y",
                &file,
            );

            let pie_group = group(&root.items[0], &file, "pie group");
            close(
                pie_group.transform.e,
                expected["pieTransform"]["x"].as_f64().expect("pie x"),
                "pie x",
                &file,
            );
            close(
                pie_group.transform.f,
                expected["pieTransform"]["y"].as_f64().expect("pie y"),
                "pie y",
                &file,
            );

            let circle = match &pie_group.items[0] {
                Item::Ellipse(circle) => circle,
                _ => panic!("{file}: outer circle is not an ellipse"),
            };
            close(
                circle.center.x,
                expected["outerCircle"]["cx"].as_f64().expect("circle cx"),
                "circle cx",
                &file,
            );
            close(
                circle.center.y,
                expected["outerCircle"]["cy"].as_f64().expect("circle cy"),
                "circle cy",
                &file,
            );
            close(
                circle.radius_x,
                expected["outerCircle"]["r"].as_f64().expect("circle r"),
                "circle r",
                &file,
            );

            let expected_slices = expected["slices"].as_array().expect("slices");
            let actual_slices: Vec<&PathShape> = pie_group
                .items
                .iter()
                .filter_map(|item| match item {
                    Item::Path(path) => Some(path),
                    _ => None,
                })
                .collect();
            assert_eq!(
                actual_slices.len(),
                expected_slices.len(),
                "{file}: slice count"
            );
            for (index, (actual, expected)) in actual_slices.iter().zip(expected_slices).enumerate()
            {
                let actual_commands =
                    serde_json::to_value(&actual.path.commands).expect("serialize commands");
                close_value(
                    &actual_commands,
                    &expected["commands"],
                    &format!("slice {index} path"),
                    &file,
                );
                let fill = actual
                    .style
                    .fill
                    .as_ref()
                    .map(|color| color.0.clone())
                    .unwrap_or_default();
                assert_eq!(
                    fill,
                    expected["fill"].as_str().expect("slice fill"),
                    "{file}: slice {index} fill"
                );
            }

            let expected_labels = expected["sliceLabels"].as_array().expect("slice labels");
            let actual_labels: Vec<&TextRun> = pie_group
                .items
                .iter()
                .filter_map(|item| match item {
                    Item::Text(text) => Some(text),
                    _ => None,
                })
                .collect();
            assert_eq!(
                actual_labels.len(),
                expected_labels.len(),
                "{file}: slice label count"
            );
            for (index, (actual, expected)) in actual_labels.iter().zip(expected_labels).enumerate()
            {
                assert_eq!(
                    text_of(actual),
                    expected["text"].as_str().expect("label text"),
                    "{file}: slice label {index} text"
                );
                close(
                    actual.position.x,
                    expected["transform"]["x"].as_f64().expect("label x"),
                    &format!("slice label {index} x"),
                    &file,
                );
                close(
                    actual.position.y,
                    expected["transform"]["y"].as_f64().expect("label y"),
                    &format!("slice label {index} y"),
                    &file,
                );
                assert_eq!(
                    actual.anchor,
                    TextAnchor::Middle,
                    "{file}: slice label {index} anchor"
                );
            }

            let title = root.items.iter().find(|item| matches!(item, Item::Text(_)));
            match &expected["title"] {
                Value::Object(_) => {
                    let title = text(
                        title.unwrap_or_else(|| panic!("{file}: missing title text")),
                        &file,
                        "title",
                    );
                    assert_eq!(
                        text_of(title),
                        expected["title"]["text"].as_str().expect("title text"),
                        "{file}: title text"
                    );
                    close(
                        title.position.x,
                        expected["title"]["x"].as_f64().expect("title x"),
                        "title x",
                        &file,
                    );
                    close(
                        title.position.y,
                        expected["title"]["y"].as_f64().expect("title y"),
                        "title y",
                        &file,
                    );
                }
                _ => assert!(title.is_none(), "{file}: unexpected title text"),
            }

            let expected_legend = expected["legend"].as_array().expect("legend");
            let actual_legend: Vec<&Group> = root
                .items
                .iter()
                .filter_map(|item| match item {
                    Item::Group(group) if group.class.as_deref() == Some("legend") => Some(group),
                    _ => None,
                })
                .collect();
            assert_eq!(
                actual_legend.len(),
                expected_legend.len(),
                "{file}: legend row count"
            );
            for (index, (actual, expected)) in actual_legend.iter().zip(expected_legend).enumerate()
            {
                close(
                    actual.transform.e,
                    expected["transform"]["x"].as_f64().expect("legend x"),
                    &format!("legend {index} x"),
                    &file,
                );
                close(
                    actual.transform.f,
                    expected["transform"]["y"].as_f64().expect("legend y"),
                    &format!("legend {index} y"),
                    &file,
                );

                let rect = match &actual.items[0] {
                    Item::Rect(rect) => rect,
                    _ => panic!("{file}: legend {index} rect is not a rect"),
                };
                close(
                    rect.rect.width,
                    expected["rect"]["width"].as_f64().expect("rect width"),
                    &format!("legend {index} width"),
                    &file,
                );
                close(
                    rect.rect.height,
                    expected["rect"]["height"].as_f64().expect("rect height"),
                    &format!("legend {index} height"),
                    &file,
                );
                assert_eq!(
                    rect.style
                        .fill
                        .as_ref()
                        .map(|color| color.0.clone())
                        .unwrap_or_default(),
                    expected["rect"]["fill"].as_str().expect("rect fill"),
                    "{file}: legend {index} fill"
                );
                assert_eq!(
                    rect.style
                        .stroke
                        .as_ref()
                        .map(|color| color.0.clone())
                        .unwrap_or_default(),
                    expected["rect"]["stroke"].as_str().expect("rect stroke"),
                    "{file}: legend {index} stroke"
                );

                let label = text(&actual.items[1], &file, "legend text");
                assert_eq!(
                    text_of(label),
                    expected["text"]["text"].as_str().expect("legend text"),
                    "{file}: legend {index} text"
                );
                close(
                    label.position.x,
                    expected["text"]["x"].as_f64().expect("legend text x"),
                    &format!("legend {index} text x"),
                    &file,
                );
                close(
                    label.position.y,
                    expected["text"]["y"].as_f64().expect("legend text y"),
                    &format!("legend {index} text y"),
                    &file,
                );
            }
        }
    }

    #[test]
    fn lays_out_a_basic_pie() {
        let scene = render(
            "pie title Pets\n  \"Dogs\" : 386\n  \"Cats\" : 85\n  \"Rats\" : 15\n",
            &Config::default(),
        )
        .expect("render pie");
        assert!(scene.view_box.width > 0.0);
        assert!(scene.view_box.height > 0.0);
        assert!(!scene.items.is_empty());
    }

    #[test]
    fn rejects_a_body_without_a_pie_header() {
        let error = render("not a pie", &Config::default()).unwrap_err();
        assert!(error.message.contains("\"pie\""));
    }
}
