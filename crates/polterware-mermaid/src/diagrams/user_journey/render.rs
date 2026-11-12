//! Source-to-[`Scene`] rendering for journey diagrams: the port of upstream's
//! `journeyRenderer.ts` `draw` and the helpers in `svgDraw.js` it calls.
//!
//! A journey diagram does not go through the unified layout pipeline: `draw` uses plain d3.
//! It builds the actor legend (`drawActorLegend`), then walks the tasks in declaration order,
//! opening a section rectangle whenever the section changes (`drawSection`), drawing each task
//! as a dashed vertical line, a face whose height and expression follow the score, a box with
//! the task's actor circles and its label (`drawTask`), and finally the title and the activity
//! line with its arrowhead. This module ports that: [`draw`] parses, [`draw_ast`] renders an
//! already parsed database, and [`render`] uses the crate's monospace fallback measurement.
//!
//! The constants are `draw`'s: actor circles of radius 7 at `x = 20`, labels starting at
//! `x = 40` with `boxTextMargin * 2` of indent, sections at `y = 50` with the configured
//! `width` and `height`, 200 units from one task to the next (`taskMargin + width`), task
//! lines ending at `300 + 5 * 30`, and the activity line at `height * 4`. The `bounds` object
//! answers the same box: tasks widen it by `diagramMarginX + taskMargin`, and the viewBox is
//! `${box.startx} -25 ${width} ${height + 70 when titled}`.
//!
//! Text is measured in the `monospace` family, the rule the fixture harness renders with and
//! that [`FallbackMeasure`](crate::measure::FallbackMeasure) reproduces; the theme's family
//! would measure differently from the harness's browser font. The actor legend's wrapping
//! (`maxLabelWidth`, words, and hyphen-broken long words) uses the same measurement. The
//! default `textPlacement` is `fo`: upstream appends a `<foreignObject>` and a fallback
//! `<text>` to a `<switch>`, and a browser paints the HTML label. A [`Scene`] carries text
//! runs, not DOM, so the port draws the fallback `<text>` geometry, which centers the label
//! the same way the HTML table cell does. `old` placement draws one baseline-anchored line;
//! `tspan` and `fo` draw one `<text>` per `<br>` line.
//!
//! Where the port stops short of upstream: the class names (`actor-0`, `journey-section`,
//! `task-type-1`, ...) matter only to CSS, and a [`Scene`] carries paints rather than
//! selectors, so `draw` resolves the declarations into the shapes. The title's `font-size` is
//! a CSS length (`4ex` by default), resolved against the base font size with `ex` as `0.5em`;
//! a unitless `taskFontSize` is not a valid CSS font size, so the task labels inherit the
//! base size exactly as a browser drops the declaration, while the `<tspan>` `dy` still uses
//! the number upstream does. The diagram id prefixes the arrowhead marker's id and the task
//! line ids; a scene owns no document id, so the marker is `arrowhead`. The `<title>` tooltip
//! elements on task actor circles are not drawn (a scene is paint, not DOM). Font weight is
//! carried on the title run, so the measured width of bold text may differ by the fallback
//! measure's weight factor.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use serde_json::Value;

use crate::config::{Config, NumberOrString};
use crate::error::ParseError;
use crate::measure::{Font, TextMeasure};
use crate::render::shapes::label_image_utils::parse_font_size;
use crate::scene::{
    Color, EllipseShape, Group, MarkerDef, MarkerOrient, MarkerRef, Path, PathCommand, PathShape,
    PathStyle, Point, Rect, RectShape, Scene, Size, TextAnchor, TextBaseline, TextBlock, TextLine,
    TextRun, TextSpan,
};

use super::Ast;

/// The actor circle's radius (`drawCircle` calls with `r: 7`).
const ACTOR_RADIUS: f64 = 7.0;
/// The legend circle's `cx`.
const LEGEND_CX: f64 = 20.0;
/// The legend label's `x`.
const LEGEND_X: f64 = 40.0;
/// The first legend row's `cy`.
const LEGEND_START_Y: f64 = 60.0;
/// One legend row's height.
const LEGEND_ROW_HEIGHT: f64 = 20.0;
/// The face circle's radius (`drawFace`).
const FACE_RADIUS: f64 = 15.0;
/// The face circle's `stroke-width`.
const FACE_STROKE_WIDTH: f64 = 2.0;
/// The eye circle's radius.
const EYE_RADIUS: f64 = 1.5;
/// The eye circles' offset from the face center.
const EYE_OFFSET: f64 = FACE_RADIUS / 3.0;
/// The mouth arc's inner radius.
const MOUTH_INNER_RADIUS: f64 = FACE_RADIUS / 2.0;
/// The mouth arc's outer radius.
const MOUTH_OUTER_RADIUS: f64 = FACE_RADIUS / 2.2;
/// The task line's `y2` and the last row of faces (`300 + 5 * 30`).
const TASK_LINE_END_Y: f64 = 450.0;
/// The first face row's `y`.
const FACE_BASE_Y: f64 = 300.0;
/// The vertical distance between score rows.
const SCORE_ROW_HEIGHT: f64 = 30.0;
/// The section rectangle's `y`.
const SECTION_Y: f64 = 50.0;
/// The title's `y`.
const TITLE_Y: f64 = 25.0;
/// The extra viewBox height a title adds (`extraVertForTitle`).
const TITLE_VIEWBOX_EXTRA: f64 = 70.0;
/// The viewBox's `y`, leaving room above `y = 0` for the title.
const VIEWBOX_Y: f64 = -25.0;
/// The section and task rectangle's corner radius.
const BOX_RADIUS: f64 = 3.0;
/// d3's `epsilon`, the threshold below which an angle or radius counts as empty.
const EPSILON: f64 = 1e-6;

/// Renders a journey body (the text [`super::parse`] accepts) to a [`Scene`].
///
/// `config` is the diagram configuration after the source's frontmatter and directives have
/// been applied, as `getConfig()` returns it to `draw`.
pub fn draw(body: &str, config: &Config, measure: &dyn TextMeasure) -> Result<Scene, ParseError> {
    let ast = super::parse(body)?;
    Ok(draw_ast(&ast, config, measure))
}

/// Renders an already parsed journey database.
pub fn draw_ast(ast: &Ast, config: &Config, measure: &dyn TextMeasure) -> Scene {
    draw_ast_with_theme(ast, config, &config.theme_variables, measure)
}

/// Renders a journey body with the crate's monospace fallback measurement.
pub fn render(body: &str, config: &Config) -> Result<Scene, ParseError> {
    draw(body, config, &crate::measure::FallbackMeasure)
}

/// `getActors()`'s order: the unique people of every task, sorted like
/// `Array.prototype.sort()`. [`Ast::actors`] already holds it.
fn actor_color<'a>(name: &str, actors: &'a [String], colours: &'a [String]) -> Option<&'a str> {
    let position = actors.iter().position(|actor| actor == name)?;
    colours
        .get(position % colours.len().max(1))
        .map(String::as_str)
}

/// Renders an already parsed database with the theme variables `draw` reads.
fn draw_ast_with_theme(
    ast: &Ast,
    config: &Config,
    theme: &serde_json::Map<String, Value>,
    measure: &dyn TextMeasure,
) -> Scene {
    let conf = &config.journey;
    let base_font_size = base_font_size(config, theme);
    let text_color = theme
        .get("textColor")
        .and_then(Value::as_str)
        .unwrap_or("#333");

    // `draw`: the actor palette is assigned in `getActors()` order.
    let actors = &ast.actors;
    let actor_colours = &conf.actor_colours;
    let actor_color_of =
        |name: &str| -> Option<Color> { actor_color(name, actors, actor_colours).map(Color::new) };

    // Fonts are measured in the harness's monospace family.
    let legend_font = Font::new("monospace", base_font_size);
    let label_font = Font::new(
        "monospace",
        label_font_size(&conf.task_font_size, base_font_size),
    );
    let title_font = Font::new("monospace", title_font_size(conf, base_font_size)).bold();
    let task_step = task_font_step(&conf.task_font_size);

    let mut scene_items: Vec<crate::scene::Item> = Vec::new();

    // `drawActorLegend`: circles and (word-wrapped) labels.
    let mut max_width: f64 = 0.0;
    let mut y_pos = LEGEND_START_Y;
    for (position, name) in actors.iter().enumerate() {
        let fill = actor_colours.get(position % actor_colours.len().max(1));
        scene_items.push(
            circle(
                Point::new(LEGEND_CX, y_pos),
                ACTOR_RADIUS,
                fill.map(Color::new).or_else(|| Some(Color::new("#000000"))),
                Some(Color::new("#000")),
                1.0,
            )
            .into(),
        );
        let lines = wrapped_actor_lines(name, conf.max_label_width, &legend_font, measure);
        for (index, line) in lines.iter().enumerate() {
            let position = Point::new(
                LEGEND_X + conf.box_text_margin * 2.0,
                y_pos + 7.0 + index as f64 * LEGEND_ROW_HEIGHT,
            );
            let line_width = measure.width(line, &legend_font);
            if line_width > max_width && line_width > conf.left_margin - line_width {
                max_width = line_width;
            }
            scene_items.push(
                text_run(
                    line,
                    position,
                    TextAnchor::Start,
                    TextBaseline::Alphabetic,
                    legend_font.clone(),
                    Some(Color::new(text_color)),
                    measure,
                )
                .into(),
            );
        }
        y_pos += LEGEND_ROW_HEIGHT.max(lines.len() as f64 * LEGEND_ROW_HEIGHT);
    }
    let left_margin = conf.left_margin + max_width;

    // `bounds.insert(0, 0, leftMargin, actors.length * 50)`.
    let mut stopx = left_margin;
    let mut stopy = actors.len() as f64 * 50.0;

    let section_v_height = conf.height * 2.0 + conf.diagram_margin_y;
    let task_pos = section_v_height;

    // `drawTasks`.
    let mut last_section: &str = "";
    let mut section_number = 0_usize;
    let mut fill = String::from("#CCC");
    let mut colour = String::from("black");
    for (index, task) in ast.tasks.iter().enumerate() {
        if last_section != task.section {
            let section_fills = &conf.section_fills;
            let section_colours = &conf.section_colours;
            let fill_index = section_number % section_fills.len().max(1);
            if let Some(value) = section_fills.get(fill_index) {
                fill = value.clone();
            }
            if let Some(value) = section_colours.get(fill_index) {
                colour = value.clone();
            }

            // Count how many consecutive tasks share this section.
            let mut task_in_section_count = 0_usize;
            for following in &ast.tasks[index..] {
                if following.section == task.section {
                    task_in_section_count += 1;
                } else {
                    break;
                }
            }

            let section_x =
                index as f64 * conf.task_margin + index as f64 * conf.width + left_margin;
            let section_width = conf.width * task_in_section_count as f64
                + conf.diagram_margin_x * (task_in_section_count as f64 - 1.0);
            let mut group = Group::new();
            group.push(RectShape::new(
                Rect::new(section_x, SECTION_Y, section_width, conf.height),
                BOX_RADIUS,
                PathStyle {
                    fill: Some(Color::new(fill.clone())),
                    stroke: Some(Color::new("#666")),
                    ..PathStyle::default()
                },
            ));
            push_centered_label(
                &mut group,
                &task.section,
                section_x,
                SECTION_Y,
                section_width,
                conf.height,
                &colour,
                &label_font,
                task_step,
                conf,
                measure,
            );
            scene_items.push(group.into());

            last_section = task.section.as_str();
            section_number += 1;
        }

        let task_x = index as f64 * conf.task_margin + index as f64 * conf.width + left_margin;
        let center = task_x + conf.width / 2.0;
        let mut group = Group::new();

        // The dashed line under the task, then the face.
        let mut line = Path::new();
        line.move_to(Point::new(center, task_pos));
        line.line_to(Point::new(center, TASK_LINE_END_Y));
        group.push(PathShape::new(
            line,
            PathStyle::stroked("#666", 1.0).with_dash([4.0, 2.0]),
        ));
        draw_face(
            &mut group,
            center,
            FACE_BASE_Y + (5.0 - task.score) * SCORE_ROW_HEIGHT,
            task.score,
            text_color,
        );

        // The task box.
        group.push(RectShape::new(
            Rect::new(task_x, task_pos, conf.width, conf.height),
            BOX_RADIUS,
            PathStyle {
                fill: Some(Color::new(fill.clone())),
                stroke: Some(Color::new("#666")),
                ..PathStyle::default()
            },
        ));

        // One circle per actor, ten units apart from `task.x + 14`.
        let mut x_pos = task_x + 14.0;
        for person in &task.people {
            let Some(actor_fill) = actor_color_of(person) else {
                continue;
            };
            group.push(circle(
                Point::new(x_pos, task_pos),
                ACTOR_RADIUS,
                Some(actor_fill),
                Some(Color::new("#000")),
                1.0,
            ));
            x_pos += 10.0;
        }

        push_centered_label(
            &mut group,
            &task.task,
            task_x,
            task_pos,
            conf.width,
            conf.height,
            &colour,
            &label_font,
            task_step,
            conf,
            measure,
        );
        scene_items.push(group.into());

        stopx = stopx.max(task_x + conf.diagram_margin_x + conf.task_margin);
        stopy = stopy.max(TASK_LINE_END_Y);
    }

    let height = stopy + 2.0 * conf.diagram_margin_y;
    let width = left_margin + stopx + 2.0 * conf.diagram_margin_x;

    // The title.
    if !ast.diagram_title.is_empty() {
        scene_items.push(
            text_run(
                &ast.diagram_title,
                Point::new(left_margin, TITLE_Y),
                TextAnchor::Start,
                TextBaseline::Alphabetic,
                title_font.clone(),
                Some(title_fill(conf, text_color)),
                measure,
            )
            .into(),
        );
    }

    let extra_vert_for_title = if ast.diagram_title.is_empty() {
        0.0
    } else {
        TITLE_VIEWBOX_EXTRA
    };

    // The activity line, drawn last.
    let mut activity = Path::new();
    activity.move_to(Point::new(left_margin, conf.height * 4.0));
    activity.line_to(Point::new(width - left_margin - 4.0, conf.height * 4.0));
    scene_items.push(
        PathShape::new(activity, PathStyle::stroked("black", 4.0))
            .with_end_marker(MarkerRef::new("arrowhead"))
            .into(),
    );

    let view_box = Rect::new(0.0, VIEWBOX_Y, width, height + extra_vert_for_title);
    let mut scene = Scene::new(view_box);

    // `initGraphics`: `M 0,0 V 4 L6,2 Z` in a 6x4 marker.
    let mut marker = MarkerDef::new("arrowhead", Rect::new(0.0, 0.0, 6.0, 4.0))
        .with_reference(Point::new(5.0, 2.0))
        .with_size(6.0, 4.0)
        .with_orient(MarkerOrient::Auto);
    marker.push(PathShape::new(
        Path::polygon([
            Point::new(0.0, 0.0),
            Point::new(0.0, 4.0),
            Point::new(6.0, 2.0),
        ]),
        PathStyle::filled(Color::new("black")),
    ));
    scene.define(marker);

    for item in scene_items {
        scene.push(item);
    }
    scene
}

/// One circle at `center`, the shape `drawCircle` appends.
fn circle(
    center: Point,
    radius: f64,
    fill: Option<Color>,
    stroke: Option<Color>,
    stroke_width: f64,
) -> EllipseShape {
    EllipseShape::new(
        center,
        radius,
        radius,
        PathStyle {
            fill,
            stroke,
            stroke_width,
            ..PathStyle::default()
        },
    )
}

/// `drawFace`: the head circle, two eyes, and the mouth for the score.
fn draw_face(group: &mut Group, cx: f64, cy: f64, score: f64, text_color: &str) {
    group.push(circle(
        Point::new(cx, cy),
        FACE_RADIUS,
        Some(Color::new("#FFF8DC")),
        Some(Color::new("#999")),
        FACE_STROKE_WIDTH,
    ));

    let mut face = Group::new();
    for dx in [-EYE_OFFSET, EYE_OFFSET] {
        face.push(circle(
            Point::new(cx + dx, cy - EYE_OFFSET),
            EYE_RADIUS,
            Some(Color::new("#666")),
            Some(Color::new("#666")),
            FACE_STROKE_WIDTH,
        ));
    }

    if score > 3.0 {
        // `smile`: a half ring, translated two units below the center.
        let path = arc_path(
            FRAC_PI_2,
            3.0 * FRAC_PI_2,
            MOUTH_INNER_RADIUS,
            MOUTH_OUTER_RADIUS,
        );
        face.push(PathShape::new(
            moved(path, cx, cy + 2.0),
            mouth_style(text_color),
        ));
    } else if score < 3.0 {
        // `sad`: the other half ring, translated seven units below the center.
        let path = arc_path(
            3.0 * FRAC_PI_2,
            5.0 * FRAC_PI_2,
            MOUTH_INNER_RADIUS,
            MOUTH_OUTER_RADIUS,
        );
        face.push(PathShape::new(
            moved(path, cx, cy + 7.0),
            mouth_style(text_color),
        ));
    } else {
        let mut line = Path::new();
        line.move_to(Point::new(cx - 5.0, cy + 7.0));
        line.line_to(Point::new(cx + 5.0, cy + 7.0));
        face.push(PathShape::new(line, PathStyle::stroked("#666", 1.0)));
    }
    group.push(face);
}

/// `.mouth`'s paint: `stroke: #666` and the root's inherited fill.
fn mouth_style(text_color: &str) -> PathStyle {
    PathStyle {
        fill: Some(Color::new(text_color)),
        stroke: Some(Color::new("#666")),
        ..PathStyle::default()
    }
}

/// A path translated by `(dx, dy)`, the `transform="translate(...)"` upstream puts on the
/// mouth arc.
fn moved(path: Path, dx: f64, dy: f64) -> Path {
    let commands = path
        .commands
        .into_iter()
        .map(|command| match command {
            PathCommand::MoveTo { x, y } => PathCommand::move_to(Point::new(x + dx, y + dy)),
            PathCommand::LineTo { x, y } => PathCommand::line_to(Point::new(x + dx, y + dy)),
            PathCommand::CubicTo {
                c1x,
                c1y,
                c2x,
                c2y,
                x,
                y,
            } => PathCommand::cubic_to(
                Point::new(c1x + dx, c1y + dy),
                Point::new(c2x + dx, c2y + dy),
                Point::new(x + dx, y + dy),
            ),
            PathCommand::QuadTo { cx, cy, x, y } => {
                PathCommand::quad_to(Point::new(cx + dx, cy + dy), Point::new(x + dx, y + dy))
            }
            PathCommand::ArcTo {
                rx,
                ry,
                rotation,
                large_arc,
                sweep,
                x,
                y,
            } => PathCommand::arc_to(
                rx,
                ry,
                rotation,
                large_arc,
                sweep,
                Point::new(x + dx, y + dy),
            ),
            PathCommand::Close => PathCommand::Close,
        })
        .collect();
    Path { commands }
}

/// `_drawTextCandidateFunc(conf)` for a section or a task: one run per `<br>` line for `fo`
/// and `tspan`, one baseline-anchored run for `old`, each centered in the box.
#[allow(clippy::too_many_arguments)]
fn push_centered_label(
    group: &mut Group,
    content: &str,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    colour: &str,
    font: &Font,
    task_step: f64,
    conf: &crate::config::JourneyConfig,
    measure: &dyn TextMeasure,
) {
    match conf.text_placement.as_str() {
        "old" => {
            // `byText`: one text at the box center plus five, with no fill attribute.
            group.push(text_run(
                &content.replace("\n", " "),
                Point::new(x + width / 2.0, y + height / 2.0 + 5.0),
                TextAnchor::Middle,
                TextBaseline::Alphabetic,
                font.clone(),
                None,
                measure,
            ));
        }
        _ => {
            // `byTspan`: one text per `<br>` line, each shifted by its `dy`.
            let lines = split_breaks(content);
            let count = lines.len() as f64;
            for (index, line) in lines.iter().enumerate() {
                let dy = index as f64 * task_step - task_step * (count - 1.0) / 2.0;
                group.push(text_run(
                    line,
                    Point::new(x + width / 2.0, y + height / 2.0 + dy),
                    TextAnchor::Middle,
                    TextBaseline::Central,
                    font.clone(),
                    Some(Color::new(colour)),
                    measure,
                ));
            }
        }
    }
}

/// `content.split(/<br\s*\/?>/gi)`.
fn split_breaks(text: &str) -> Vec<String> {
    let lower = text.to_ascii_lowercase();
    let bytes = text.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0_usize;
    let mut cursor = 0_usize;
    while cursor < bytes.len() {
        if lower.as_bytes()[cursor] == b'<' && lower[cursor..].starts_with("<br") {
            let after = cursor + 3;
            let mut end = after;
            while lower
                .as_bytes()
                .get(end)
                .is_some_and(u8::is_ascii_whitespace)
            {
                end += 1;
            }
            if lower.as_bytes().get(end) == Some(&b'/') {
                end += 1;
            }
            if lower.as_bytes().get(end) == Some(&b'>') {
                lines.push(text[start..cursor].to_string());
                cursor = end + 1;
                start = cursor;
                continue;
            }
        }
        cursor += 1;
    }
    lines.push(text[start..].to_string());
    lines
}

/// A one-line text run, shaped and measured like `byTspan`'s `<text><tspan>`.
fn text_run(
    text: &str,
    position: Point,
    anchor: TextAnchor,
    baseline: TextBaseline,
    font: Font,
    fill: Option<Color>,
    measure: &dyn TextMeasure,
) -> TextRun {
    let width = measure.width(text, &font);
    let line_height = measure.line_height(&font);
    let block = TextBlock::new(
        vec![TextLine::new(vec![TextSpan::plain(text)])],
        Size::new(width, line_height),
        line_height,
    );
    let mut run = TextRun::new(position, font, block);
    run.anchor = anchor;
    run.baseline = baseline;
    run.fill = fill;
    run
}

/// The base font size the svg's `font-size` gives the labels, from the config's `fontSize`
/// or the theme's `fontSize`.
fn base_font_size(config: &Config, theme: &serde_json::Map<String, Value>) -> f64 {
    if config.font_size > 0.0 {
        config.font_size
    } else {
        theme
            .get("fontSize")
            .and_then(Value::as_str)
            .and_then(parse_font_size)
            .unwrap_or(16.0)
    }
}

/// The label font size: `.style('font-size', taskFontSize)` only applies when the value is a
/// CSS length, and the default is the unitless number 14, which a browser drops.
fn label_font_size(value: &NumberOrString, base: f64) -> f64 {
    match value {
        NumberOrString::Text(text) => css_length(text, base).unwrap_or(base),
        NumberOrString::Number(_) => base,
    }
}

/// The `<tspan>` `dy` step: `taskFontSize` coerced to a number, as `i * taskFontSize` does.
fn task_font_step(value: &NumberOrString) -> f64 {
    match value {
        NumberOrString::Number(number) => *number,
        NumberOrString::Text(text) => js_number(text),
    }
}

/// `Number(text)`, for the numbers a font size can hold.
fn js_number(text: &str) -> f64 {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return 0.0;
    }
    trimmed.parse().unwrap_or(f64::NAN)
}

/// The title's `font-size`: an SVG presentation attribute, so a CSS length resolved against
/// the base size; anything a browser rejects falls back to the inherited size.
fn title_font_size(conf: &crate::config::JourneyConfig, base: f64) -> f64 {
    css_length(&conf.title_font_size, base).unwrap_or(base)
}

/// The title's `fill`: `titleColor` when configured, otherwise the inherited text color.
fn title_fill(conf: &crate::config::JourneyConfig, text_color: &str) -> Color {
    if conf.title_color.is_empty() {
        Color::new(text_color)
    } else {
        Color::new(conf.title_color.clone())
    }
}

/// A CSS `<length>` in px: `px`, `pt`, `em`, `ex`, `%`, or the unitless zero.
fn css_length(value: &str, base: f64) -> Option<f64> {
    let value = value.trim();
    let end = value
        .char_indices()
        .find(|(_, ch)| !(ch.is_ascii_digit() || matches!(ch, '.' | '+' | '-')))
        .map_or(value.len(), |(index, _)| index);
    let number: f64 = value.get(..end)?.parse().ok()?;
    let unit = value.get(end..)?.trim().to_ascii_lowercase();
    match unit.as_str() {
        "px" => Some(number),
        "pt" => Some(number * 4.0 / 3.0),
        "em" => Some(number * base),
        "ex" => Some(number * base * 0.5),
        "%" => Some(number / 100.0 * base),
        "" if number == 0.0 => Some(0.0),
        _ => None,
    }
}

/// `drawActorLegend`'s wrapping: the label as one line when it fits, otherwise words, with a
/// hyphen break for a word wider than the limit.
fn wrapped_actor_lines(
    name: &str,
    max_label_width: f64,
    font: &Font,
    measure: &dyn TextMeasure,
) -> Vec<String> {
    if measure.width(name, font) <= max_label_width {
        return vec![name.to_string()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in name.split(' ') {
        let test_line = if current.is_empty() {
            word.to_string()
        } else {
            format!("{current} {word}")
        };
        if measure.width(&test_line, font) > max_label_width {
            if !current.is_empty() {
                lines.push(current.clone());
            }
            current = word.to_string();
            if measure.width(word, font) > max_label_width {
                let mut broken = String::new();
                for ch in word.chars() {
                    broken.push(ch);
                    let with_hyphen = format!("{broken}-");
                    if measure.width(&with_hyphen, font) > max_label_width {
                        let mut line = broken.clone();
                        line.pop();
                        lines.push(format!("{line}-"));
                        broken = ch.to_string();
                    }
                }
                current = broken;
            }
        } else {
            current = test_line;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

/// `d3.arc()`'s path for the mouth, with no padding and no rounded corners: the same
/// generator `pieRenderer.ts` uses, and the same one [`crate::diagrams::pie::render`] ports.
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

    if not_above(r1, EPSILON) {
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

        if not_above(da, EPSILON) {
            context.move_to(x01, y01);
        } else {
            context.move_to(x01, y01);
            context.arc(0.0, 0.0, r1, a0, a1, !clockwise);
        }

        if not_above(r0, EPSILON) || not_above(da, EPSILON) {
            context.line_to(x10, y10);
        } else {
            context.arc(0.0, 0.0, r0, a1, a0, clockwise);
        }
    }
    context.close();
    context.path
}

/// d3-path's `!(value > threshold)` guard: false only for a value above the threshold, so a
/// NaN takes the empty branch as it does upstream.
fn not_above(value: f64, threshold: f64) -> bool {
    value.is_nan() || value <= threshold
}

/// `d3-path`'s `Path`, restricted to what `d3.arc()` emits, in [`PathCommand`]s.
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
    use crate::scene::Item;
    use serde_json::Value;

    const TOLERANCE: f64 = 0.01;

    /// One painted shape, flattened out of the scene's groups and with every transform
    /// applied, in paint order.
    #[derive(Debug)]
    enum Flat {
        Circle {
            cx: f64,
            cy: f64,
            r: f64,
            fill: Option<String>,
            stroke: Option<String>,
        },
        Rect {
            x: f64,
            y: f64,
            width: f64,
            height: f64,
            rx: f64,
            fill: Option<String>,
            stroke: Option<String>,
        },
        Line {
            x1: f64,
            y1: f64,
            x2: f64,
            y2: f64,
            stroke: Option<String>,
            width: f64,
            dash: Vec<f64>,
        },
        Path {
            commands: Vec<PathCommand>,
            fill: Option<String>,
            stroke: Option<String>,
            width: f64,
            marker_end: bool,
        },
        Text {
            text: String,
            lines: Vec<String>,
            x: f64,
            y: f64,
            anchor: TextAnchor,
            baseline: TextBaseline,
            font_size: f64,
            fill: Option<String>,
        },
    }

    fn fixtures() -> Vec<(String, Value)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/diagrams/user_journey/tests/render");
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

    fn fill_of(option: &Option<Color>) -> Option<String> {
        option.as_ref().map(|color| color.0.clone())
    }

    fn flatten(items: &[Item], tx: f64, ty: f64, out: &mut Vec<Flat>) {
        for item in items {
            match item {
                Item::Group(group) => flatten(
                    &group.items,
                    tx + group.transform.e,
                    ty + group.transform.f,
                    out,
                ),
                Item::Ellipse(ellipse) => out.push(Flat::Circle {
                    cx: ellipse.center.x + tx,
                    cy: ellipse.center.y + ty,
                    r: ellipse.radius_x,
                    fill: fill_of(&ellipse.style.fill),
                    stroke: fill_of(&ellipse.style.stroke),
                }),
                Item::Rect(rect) => out.push(Flat::Rect {
                    x: rect.rect.x + tx,
                    y: rect.rect.y + ty,
                    width: rect.rect.width,
                    height: rect.rect.height,
                    rx: rect.rx,
                    fill: fill_of(&rect.style.fill),
                    stroke: fill_of(&rect.style.stroke),
                }),
                Item::Path(shape) => {
                    let commands = &shape.path.commands;
                    let is_line = commands.len() == 2
                        && matches!(&commands[0], PathCommand::MoveTo { .. })
                        && matches!(&commands[1], PathCommand::LineTo { .. });
                    if is_line {
                        let (
                            PathCommand::MoveTo { x: x1, y: y1 },
                            PathCommand::LineTo { x: x2, y: y2 },
                        ) = (&commands[0], &commands[1])
                        else {
                            unreachable!("checked above")
                        };
                        out.push(Flat::Line {
                            x1: *x1 + tx,
                            y1: *y1 + ty,
                            x2: *x2 + tx,
                            y2: *y2 + ty,
                            stroke: fill_of(&shape.style.stroke),
                            width: shape.style.stroke_width,
                            dash: shape.style.dash.clone(),
                        });
                    } else {
                        out.push(Flat::Path {
                            commands: commands.clone(),
                            fill: fill_of(&shape.style.fill),
                            stroke: fill_of(&shape.style.stroke),
                            width: shape.style.stroke_width,
                            marker_end: !shape.end_markers.is_empty(),
                        });
                    }
                }
                Item::Text(run) => out.push(Flat::Text {
                    text: run
                        .lines
                        .iter()
                        .map(|line| line.text())
                        .collect::<Vec<_>>()
                        .join("\n"),
                    lines: run.lines.iter().map(|line| line.text()).collect(),
                    x: run.position.x + tx,
                    y: run.position.y + ty,
                    anchor: run.anchor,
                    baseline: run.baseline,
                    font_size: run.font.size,
                    fill: fill_of(&run.fill),
                }),
                Item::Image(_) => panic!("journey draws no images"),
            }
        }
    }

    fn expected_text_position(shape: &Value) -> (f64, f64) {
        let x = shape["x"].as_f64().expect("text x");
        let dy = shape["tspans"]
            .as_array()
            .and_then(|tspans| tspans.first())
            .and_then(|tspan| tspan["dy"].as_str())
            .and_then(|dy| dy.parse::<f64>().ok())
            .unwrap_or(0.0);
        let y = shape["y"].as_f64().expect("text y");
        (x, y + dy)
    }

    fn expected_anchor(value: &str) -> TextAnchor {
        match value {
            "middle" => TextAnchor::Middle,
            "end" => TextAnchor::End,
            _ => TextAnchor::Start,
        }
    }

    fn expected_baseline(value: &str) -> TextBaseline {
        match value {
            "central" => TextBaseline::Central,
            "middle" => TextBaseline::Middle,
            "hanging" => TextBaseline::Hanging,
            "top" => TextBaseline::Top,
            "bottom" => TextBaseline::Bottom,
            _ => TextBaseline::Alphabetic,
        }
    }

    fn expected_commands(shape: &Value) -> Vec<PathCommand> {
        let mut commands = Vec::new();
        for command in shape["commands"].as_array().expect("path commands") {
            let op = command["op"].as_str().expect("command op");
            let number = |key: &str| command[key].as_f64().expect(key);
            commands.push(match op {
                "moveTo" => PathCommand::move_to(Point::new(number("x"), number("y"))),
                "lineTo" => PathCommand::line_to(Point::new(number("x"), number("y"))),
                "cubicTo" => PathCommand::cubic_to(
                    Point::new(number("c1x"), number("c1y")),
                    Point::new(number("c2x"), number("c2y")),
                    Point::new(number("x"), number("y")),
                ),
                "quadTo" => PathCommand::quad_to(
                    Point::new(number("cx"), number("cy")),
                    Point::new(number("x"), number("y")),
                ),
                "arcTo" => PathCommand::arc_to(
                    number("rx"),
                    number("ry"),
                    number("rotation"),
                    command["large_arc"].as_bool().expect("large_arc"),
                    command["sweep"].as_bool().expect("sweep"),
                    Point::new(number("x"), number("y")),
                ),
                "close" => PathCommand::Close,
                other => panic!("unknown path command {other}"),
            });
        }
        commands
    }

    fn assert_commands(actual: &[PathCommand], expected: &[PathCommand], what: &str, file: &str) {
        assert_eq!(
            actual.len(),
            expected.len(),
            "{file}: {what}: command count"
        );
        for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
            let what = format!("{what}[{index}]");
            match (actual, expected) {
                (PathCommand::MoveTo { x, y }, PathCommand::MoveTo { x: ex, y: ey })
                | (PathCommand::LineTo { x, y }, PathCommand::LineTo { x: ex, y: ey }) => {
                    close(*x, *ex, &format!("{what}.x"), file);
                    close(*y, *ey, &format!("{what}.y"), file);
                }
                (
                    PathCommand::ArcTo {
                        rx,
                        ry,
                        rotation,
                        large_arc,
                        sweep,
                        x,
                        y,
                    },
                    PathCommand::ArcTo {
                        rx: erx,
                        ry: ery,
                        rotation: erotation,
                        large_arc: elarge_arc,
                        sweep: esweep,
                        x: ex,
                        y: ey,
                    },
                ) => {
                    close(*rx, *erx, &format!("{what}.rx"), file);
                    close(*ry, *ery, &format!("{what}.ry"), file);
                    close(*rotation, *erotation, &format!("{what}.rotation"), file);
                    assert_eq!(large_arc, elarge_arc, "{file}: {what}.large_arc");
                    assert_eq!(sweep, esweep, "{file}: {what}.sweep");
                    close(*x, *ex, &format!("{what}.x"), file);
                    close(*y, *ey, &format!("{what}.y"), file);
                }
                (PathCommand::Close, PathCommand::Close) => {}
                _ => panic!("{file}: {what}: command kind differs: {actual:?} vs {expected:?}"),
            }
        }
    }

    #[test]
    fn renders_recorded_user_journey_fixtures() {
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

            let expected_shapes = expected["shapes"].as_array().expect("shapes");
            let mut actual_shapes = Vec::new();
            flatten(&scene.items, 0.0, 0.0, &mut actual_shapes);
            assert_eq!(
                actual_shapes.len(),
                expected_shapes.len(),
                "{file}: shape count (actual {actual_shapes:#?})"
            );

            for (index, (actual, expected)) in actual_shapes.iter().zip(expected_shapes).enumerate()
            {
                let what = format!("shape {index} ({})", expected["kind"]);
                match (actual, expected["kind"].as_str().expect("kind")) {
                    (
                        Flat::Circle {
                            cx,
                            cy,
                            r,
                            fill,
                            stroke,
                        },
                        "circle",
                    ) => {
                        close(*cx, expected["cx"].as_f64().expect("cx"), &what, &file);
                        close(*cy, expected["cy"].as_f64().expect("cy"), &what, &file);
                        close(*r, expected["r"].as_f64().expect("r"), &what, &file);
                        if let Some(expected_fill) = expected["fill"].as_str() {
                            assert_eq!(fill.as_deref(), Some(expected_fill), "{file}: {what}.fill");
                        }
                        if let Some(expected_stroke) = expected["stroke"].as_str() {
                            assert_eq!(
                                stroke.as_deref(),
                                Some(expected_stroke),
                                "{file}: {what}.stroke"
                            );
                        }
                    }
                    (
                        Flat::Rect {
                            x,
                            y,
                            width,
                            height,
                            rx,
                            fill,
                            stroke,
                        },
                        "rect",
                    ) => {
                        close(*x, expected["x"].as_f64().expect("rect x"), &what, &file);
                        close(*y, expected["y"].as_f64().expect("rect y"), &what, &file);
                        close(
                            *width,
                            expected["width"].as_f64().expect("rect width"),
                            &what,
                            &file,
                        );
                        close(
                            *height,
                            expected["height"].as_f64().expect("rect height"),
                            &what,
                            &file,
                        );
                        close(*rx, expected["rx"].as_f64().expect("rect rx"), &what, &file);
                        if let Some(expected_fill) = expected["fill"].as_str() {
                            assert_eq!(fill.as_deref(), Some(expected_fill), "{file}: {what}.fill");
                        }
                        if let Some(expected_stroke) = expected["stroke"].as_str() {
                            assert_eq!(
                                stroke.as_deref(),
                                Some(expected_stroke),
                                "{file}: {what}.stroke"
                            );
                        }
                    }
                    (
                        Flat::Line {
                            x1,
                            y1,
                            x2,
                            y2,
                            stroke,
                            width,
                            dash,
                        },
                        "line",
                    ) => {
                        close(*x1, expected["x1"].as_f64().expect("line x1"), &what, &file);
                        close(*y1, expected["y1"].as_f64().expect("line y1"), &what, &file);
                        close(*x2, expected["x2"].as_f64().expect("line x2"), &what, &file);
                        close(*y2, expected["y2"].as_f64().expect("line y2"), &what, &file);
                        close(
                            *width,
                            expected["strokeWidth"].as_f64().expect("line width"),
                            &what,
                            &file,
                        );
                        let expected_dash: Vec<f64> = expected["dash"]
                            .as_array()
                            .expect("line dash")
                            .iter()
                            .map(|value| value.as_f64().expect("dash"))
                            .collect();
                        assert_eq!(*dash, expected_dash, "{file}: {what}.dash");
                        if let Some(expected_stroke) = expected["stroke"].as_str() {
                            assert_eq!(
                                stroke.as_deref(),
                                Some(expected_stroke),
                                "{file}: {what}.stroke"
                            );
                        }
                    }
                    (
                        Flat::Path {
                            commands,
                            fill,
                            stroke,
                            width,
                            marker_end,
                        },
                        "path",
                    ) => {
                        let mut expected_commands = expected_commands(expected);
                        let translate = &expected["translate"];
                        let dx = translate["x"].as_f64().unwrap_or(0.0);
                        let dy = translate["y"].as_f64().unwrap_or(0.0);
                        for command in &mut expected_commands {
                            translate_command(command, dx, dy);
                        }
                        assert_commands(commands, &expected_commands, &what, &file);
                        assert_eq!(
                            *marker_end,
                            expected["markerEnd"].is_string(),
                            "{file}: {what}.markerEnd"
                        );
                        if let Some(expected_fill) = expected["fill"].as_str() {
                            assert_eq!(fill.as_deref(), Some(expected_fill), "{file}: {what}.fill");
                        }
                        if let Some(expected_stroke) = expected["stroke"].as_str() {
                            assert_eq!(
                                stroke.as_deref(),
                                Some(expected_stroke),
                                "{file}: {what}.stroke"
                            );
                            close(
                                *width,
                                expected["strokeWidth"].as_f64().expect("path width"),
                                &what,
                                &file,
                            );
                        }
                    }
                    (
                        Flat::Text {
                            text,
                            lines,
                            x,
                            y,
                            anchor,
                            baseline,
                            font_size,
                            fill,
                        },
                        "text",
                    ) => {
                        let expected_lines: Vec<String> = expected["lines"]
                            .as_array()
                            .expect("text lines")
                            .iter()
                            .map(|value| value.as_str().expect("line").to_string())
                            .collect();
                        assert_eq!(
                            text,
                            &expected["text"].as_str().expect("text"),
                            "{file}: {what}.text"
                        );
                        assert_eq!(lines, &expected_lines, "{file}: {what}.lines");
                        let (expected_x, expected_y) = expected_text_position(expected);
                        close(*x, expected_x, &format!("{what}.x"), &file);
                        close(*y, expected_y, &format!("{what}.y"), &file);
                        assert_eq!(
                            *anchor,
                            expected_anchor(expected["anchor"].as_str().expect("anchor")),
                            "{file}: {what}.anchor"
                        );
                        assert_eq!(
                            *baseline,
                            expected_baseline(expected["baseline"].as_str().expect("baseline")),
                            "{file}: {what}.baseline"
                        );
                        close(
                            *font_size,
                            expected["fontSize"].as_f64().expect("fontSize"),
                            &format!("{what}.fontSize"),
                            &file,
                        );
                        if let Some(expected_fill) =
                            expected["fill"].as_str().filter(|fill| !fill.is_empty())
                        {
                            assert_eq!(fill.as_deref(), Some(expected_fill), "{file}: {what}.fill");
                        }
                    }
                    (actual, kind) => {
                        panic!("{file}: {what}: scene shape {actual:?} does not match {kind}")
                    }
                }
            }

            // The arrowhead marker: 6x4 with the reference point two units up, and the
            // `M 0,0 V 4 L6,2 Z` outline.
            let marker = scene
                .marker_def("arrowhead")
                .unwrap_or_else(|| panic!("{file}: no arrowhead marker"));
            let expected_marker = &expected["marker"];
            close(
                marker.view_box.width,
                expected_marker["markerWidth"]
                    .as_f64()
                    .expect("markerWidth"),
                "marker width",
                &file,
            );
            close(
                marker.view_box.height,
                expected_marker["markerHeight"]
                    .as_f64()
                    .expect("markerHeight"),
                "marker height",
                &file,
            );
            close(
                marker.reference.x,
                expected_marker["refX"].as_f64().expect("refX"),
                "marker refX",
                &file,
            );
            close(
                marker.reference.y,
                expected_marker["refY"].as_f64().expect("refY"),
                "marker refY",
                &file,
            );
            let expected_marker_commands = expected_commands(expected_marker);
            assert_eq!(marker.shapes.len(), 1, "{file}: marker shapes");
            assert_commands(
                &marker.shapes[0].path.commands,
                &expected_marker_commands,
                "marker path",
                &file,
            );
        }
    }

    #[test]
    fn render_diagram_dispatches_user_journey() {
        let scene = crate::render_diagram(
            "journey\n  title A day\n  section Morning\n    Wake up: 5: Me\n",
            &Config::default(),
        )
        .expect("journey renders");
        assert!(scene.view_box.width > 0.0);
        assert!(scene.view_box.height > 0.0);
        assert!(scene.marker_def("arrowhead").is_some());
    }

    fn translate_command(command: &mut PathCommand, dx: f64, dy: f64) {
        match command {
            PathCommand::MoveTo { x, y } | PathCommand::LineTo { x, y } => {
                *x += dx;
                *y += dy;
            }
            PathCommand::CubicTo {
                c1x,
                c1y,
                c2x,
                c2y,
                x,
                y,
            } => {
                *c1x += dx;
                *c1y += dy;
                *c2x += dx;
                *c2y += dy;
                *x += dx;
                *y += dy;
            }
            PathCommand::QuadTo { cx, cy, x, y } => {
                *cx += dx;
                *cy += dy;
                *x += dx;
                *y += dy;
            }
            PathCommand::ArcTo { x, y, .. } => {
                *x += dx;
                *y += dy;
            }
            PathCommand::Close => {}
        }
    }
}
