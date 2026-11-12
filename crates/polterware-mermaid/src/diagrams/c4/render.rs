//! Source-to-[`Scene`] rendering for C4 diagrams: the Rust side of upstream
//! `c4Renderer.ts`, `svgDraw.ts`, `c4ShapeAdapter.ts`, and `styles.js`.
//!
//! C4 does not go through the unified layout pipeline. `c4Renderer.draw` places every element
//! on the legacy grid ([`Bounds`]), draws each element through the unified shape registry
//! (`buildC4Node` plus `shapes[node.shape]`), then draws the relationships and the title with
//! the plain d3 code of `svgDraw.ts`. This module ports that pipeline: [`draw`] parses,
//! [`draw_ast`] renders an already parsed database, and [`render`] uses the crate's monospace
//! fallback measurement.
//!
//! # What the port keeps
//!
//! * The grid: `Bounds::insert`'s first-row and wrap rules, `c4ShapeInRow`, `c4BoundaryInRow`,
//!   `nextLinePaddingX`, `bumpLastMargin`, and the nested-boundary arithmetic of
//!   `drawInsideBoundary`. A shape's `x`/`y` is its top-left corner; the drawn group is
//!   translated to the shape's centre, as pass 2 does.
//! * The shape sizing of `person.ts`, `roundedRect.ts` (`drawRect`), `cylinder.ts`,
//!   `tiltedCylinder.ts` (the queue shape), and `subroutine.ts` (the `fr-rect` shape), and the
//!   stacked name/stereotype/description label of `c4LabelHelper.ts`.
//! * The relationship geometry of `c4Renderer.getIntersectPoints` and the five shapes'
//!   `intersect` functions, including the cylinder and horizontal-cylinder side correction.
//! * `svgDraw.ts`'s drawing: the first relationship is a line and every later one a quadratic
//!   path, labels sit at the segment midpoint plus `UpdateRelStyle`'s offsets, the type line is
//!   italic and one message line below the name, boundaries are dashed 2.5-radius rectangles,
//!   and the arrowhead/arrowend/filled-head/crosshead markers go into [`Scene::defs`].
//! * The viewBox arithmetic: `diagramMarginX` on both sides, `diagramMarginY` above and below,
//!   and the extra 60 units of `extraVertForTitle`.
//!
//! # Where the port stops short of upstream
//!
//! * `screen.availWidth` seeds the first wrap limit. A library has no monitor, so the port uses
//!   [`SCREEN_AVAIL_WIDTH`] (1920), the value the fixture harness gives jsdom's `screen`.
//! * Colors and classes are resolved into paints at build time. The element palette comes from
//!   the c4 config (`person_bg_color`, ...); `UpdateElementStyle` overrides it. The compiled
//!   stylesheet's `stroke-width: 2px` rule for `.c4-shape` children and its 12-unit corner
//!   radius for rounded/`fr-rect` nodes are applied directly, since a [`Scene`] carries styles
//!   rather than selectors.
//! * Text is measured in the `monospace` family, the rule the fixture harness renders with and
//!   that [`FallbackMeasure`](crate::measure::FallbackMeasure) reproduces; the config's
//!   `"Open Sans"` family would measure differently. Shape label sizes use the per-element
//!   config font sizes (and the `.c4-type` 0.75em / `.c4-descr` 0.82em factors). The boundary and
//!   relationship text is drawn at the root font size, as it is in a browser: `svgDraw.ts`
//!   writes a unitless `font-size`, which CSS rejects, so those runs inherit the SVG's size.
//! * The `computer`, `database`, and `clock` symbols `draw` inserts are never referenced by the
//!   C4 renderer (sprites are not drawn), so they are not added to the scene.
//! * A relationship naming an unknown endpoint is skipped instead of throwing, because the
//!   reachable parser only stores declared aliases.
//! * `parseInt`'s `NaN` stays `None` in the parsed offsets and becomes 0 here, as `drawRels`
//!   does with its `? parseInt(...) : 0`.

use std::collections::HashMap;

use crate::common::sanitize;
use crate::config::{C4Config, Config, NumberOrString};
use crate::error::ParseError;
use crate::measure::{Font, TextMeasure, TextOptions, shape_text};
use crate::render::intersect::{intersect_polygon, intersect_rect};
use crate::scene::{
    BOLD_WEIGHT, Color, EllipseShape, Group, Item, MarkerDef, MarkerOrient, MarkerRef, MarkerUnits,
    NORMAL_WEIGHT, Path, PathShape, PathStyle, Point, Rect, RectShape, Scene, Size, SourceRef,
    TextAnchor, TextBaseline, TextLine, TextRun,
};

use super::Ast;
use super::ast::{Boundary, Shape, Text, TextValue};

/// Upstream reads `screen.availWidth`; a library has no monitor, so the width limit is fixed.
const SCREEN_AVAIL_WIDTH: f64 = 1920.0;
/// `svgDraw.ts`'s arrowhead marker.
const ARROWHEAD: &str = "arrowhead";
/// `svgDraw.ts`'s arrowend marker.
const ARROWEND: &str = "arrowend";
/// `svgDraw.ts`'s filled-head marker (inserted, never referenced by C4 relationships).
const FILLED_HEAD: &str = "filled-head";
/// `svgDraw.ts`'s crosshead marker (inserted, never referenced by C4 relationships).
const CROSSHEAD: &str = "crosshead";
/// `c4LabelHelper.ts`'s gap between stacked label sections.
const SECTION_GAP: f64 = 3.0;
/// `c4LabelHelper.ts`'s minimum wrap width.
const MIN_WRAP_WIDTH: f64 = 32.0;
/// `createFormattedText`'s line advance, in em.
const LINE_HEIGHT_EM: f64 = 1.1;
/// `createFormattedText`'s fixed `<text y>` attribute.
const LABEL_TEXT_Y: f64 = -10.1;
/// The height `draw` reserves for a title.
const TITLE_EXTRA_HEIGHT: f64 = 60.0;

/// Renders a C4 body (the text [`super::parse`] accepts) to a [`Scene`].
pub fn draw(body: &str, config: &Config, measure: &dyn TextMeasure) -> Result<Scene, ParseError> {
    let ast = super::parse(body)?;
    Ok(draw_ast(&ast, config, measure))
}

/// Renders a C4 body with the crate's monospace fallback measurement.
pub fn render(body: &str, config: &Config) -> Result<Scene, ParseError> {
    draw(body, config, &crate::measure::FallbackMeasure)
}

/// JavaScript's `Math.round`: halves go up, not away from zero.
fn js_round(value: f64) -> f64 {
    (value + 0.5).floor()
}

fn text_of(text: &Text) -> &str {
    match &text.text {
        TextValue::Text(value) => value,
        TextValue::Named { .. } | TextValue::Bare(_) => "",
    }
}

fn option_text(text: Option<&Text>) -> &str {
    text.map(text_of).unwrap_or("")
}

fn font_size_number(size: &NumberOrString) -> f64 {
    match size {
        NumberOrString::Number(number) => *number,
        NumberOrString::Text(text) => {
            let digits: String = text
                .trim_start()
                .chars()
                .take_while(|character| character.is_ascii_digit() || *character == '-')
                .collect();
            digits.parse().unwrap_or(16.0)
        }
    }
}

/// A font for measuring one label section. The family is the fixture harness's monospace rule.
fn measure_font(size: f64) -> Font {
    Font::new("monospace", size)
}

/// `calculateTextDimensions`' width: the widest line, rounded.
fn text_width(text: &str, font: &Font, measure: &dyn TextMeasure) -> f64 {
    sanitize::split_breaks(text)
        .iter()
        .map(|line| measure.width(line, font))
        .fold(0.0, f64::max)
        .round()
}

/// `calculateTextDimensions`' height: every line's rounded ink height, summed.
fn text_height(text: &str, font: &Font, measure: &dyn TextMeasure) -> f64 {
    let line_height = js_round(measure.line_height(font));
    sanitize::split_breaks(text).len() as f64 * line_height
}

/// `c4LabelHelper.ts`'s section text with the c4 section styles applied.
fn sanitized_section(text: &str) -> String {
    sanitize::sanitize_text(text)
}

// ---- element configuration ----

fn element_font(c4: &C4Config, element_type: &str) -> crate::config::FontConfig {
    match element_type {
        "person" => c4.person_font(),
        "external_person" => c4.external_person_font(),
        "system" => c4.system_font(),
        "external_system" => c4.external_system_font(),
        "system_db" => c4.system_db_font(),
        "external_system_db" => c4.external_system_db_font(),
        "system_queue" => c4.system_queue_font(),
        "external_system_queue" => c4.external_system_queue_font(),
        "container" => c4.container_font(),
        "external_container" => c4.external_container_font(),
        "container_db" => c4.container_db_font(),
        "external_container_db" => c4.external_container_db_font(),
        "container_queue" => c4.container_queue_font(),
        "external_container_queue" => c4.external_container_queue_font(),
        "component" => c4.component_font(),
        "external_component" => c4.external_component_font(),
        "component_db" => c4.component_db_font(),
        "external_component_db" => c4.external_component_db_font(),
        "component_queue" => c4.component_queue_font(),
        "external_component_queue" => c4.external_component_queue_font(),
        _ => crate::config::FontConfig {
            font_family: "monospace".to_string(),
            font_size: NumberOrString::Number(16.0),
            font_weight: NumberOrString::Text("normal".to_string()),
        },
    }
}

/// The `elementCssStyles` palette: per-type fill and border from the c4 config.
fn element_colors(c4: &C4Config, element_type: &str) -> (Option<String>, Option<String>) {
    let pair = match element_type {
        "person" => (&c4.person_bg_color, &c4.person_border_color),
        "external_person" => (
            &c4.external_person_bg_color,
            &c4.external_person_border_color,
        ),
        "system" => (&c4.system_bg_color, &c4.system_border_color),
        "external_system" => (
            &c4.external_system_bg_color,
            &c4.external_system_border_color,
        ),
        "system_db" => (&c4.system_db_bg_color, &c4.system_db_border_color),
        "external_system_db" => (
            &c4.external_system_db_bg_color,
            &c4.external_system_db_border_color,
        ),
        "system_queue" => (&c4.system_queue_bg_color, &c4.system_queue_border_color),
        "external_system_queue" => (
            &c4.external_system_queue_bg_color,
            &c4.external_system_queue_border_color,
        ),
        "container" => (&c4.container_bg_color, &c4.container_border_color),
        "external_container" => (
            &c4.external_container_bg_color,
            &c4.external_container_border_color,
        ),
        "container_db" => (&c4.container_db_bg_color, &c4.container_db_border_color),
        "external_container_db" => (
            &c4.external_container_db_bg_color,
            &c4.external_container_db_border_color,
        ),
        "container_queue" => (
            &c4.container_queue_bg_color,
            &c4.container_queue_border_color,
        ),
        "external_container_queue" => (
            &c4.external_container_queue_bg_color,
            &c4.external_container_queue_border_color,
        ),
        "component" => (&c4.component_bg_color, &c4.component_border_color),
        "external_component" => (
            &c4.external_component_bg_color,
            &c4.external_component_border_color,
        ),
        "component_db" => (&c4.component_db_bg_color, &c4.component_db_border_color),
        "external_component_db" => (
            &c4.external_component_db_bg_color,
            &c4.external_component_db_border_color,
        ),
        "component_queue" => (
            &c4.component_queue_bg_color,
            &c4.component_queue_border_color,
        ),
        "external_component_queue" => (
            &c4.external_component_queue_bg_color,
            &c4.external_component_queue_border_color,
        ),
        _ => (&String::new(), &String::new()),
    };
    let fill = if pair.0.is_empty() {
        None
    } else {
        Some(pair.0.clone())
    };
    let stroke = if pair.1.is_empty() {
        None
    } else {
        Some(pair.1.clone())
    };
    (fill, stroke)
}

fn is_c4_element_type(element_type: &str) -> bool {
    matches!(
        element_type,
        "person"
            | "external_person"
            | "system"
            | "external_system"
            | "system_db"
            | "external_system_db"
            | "system_queue"
            | "external_system_queue"
            | "container"
            | "external_container"
            | "container_db"
            | "external_container_db"
            | "container_queue"
            | "external_container_queue"
            | "component"
            | "external_component"
            | "component_db"
            | "external_component_db"
            | "component_queue"
            | "external_component_queue"
    )
}

/// `c4ShapeAdapter.resolveNodeShape`.
fn resolve_shape(shape: &Shape) -> &'static str {
    fn keyword(value: &str) -> Option<&'static str> {
        match value.to_ascii_lowercase().as_str() {
            "person" => Some("person"),
            "box" | "rounded" => Some("rounded"),
            "cylinder" | "database" | "db" => Some("cylinder"),
            "queue" | "pipe" => Some("h-cyl"),
            "component" => Some("fr-rect"),
            _ => None,
        }
    }

    if let Some(explicit) = shape
        .shape
        .as_deref()
        .and_then(keyword)
        .or_else(|| shape.sprite.as_deref().and_then(keyword))
    {
        return explicit;
    }
    if let Some(tags) = shape.tags.as_deref() {
        for tag in tags.split(',') {
            if let Some(found) = keyword(tag.trim()) {
                return found;
            }
        }
    }
    let element_type = text_of(&shape.type_c4_shape);
    if element_type == "person" || element_type == "external_person" {
        return "person";
    }
    if element_type.ends_with("_db") {
        return "cylinder";
    }
    if element_type.ends_with("_queue") {
        return "h-cyl";
    }
    "rounded"
}

/// `c4ShapeAdapter`'s Structurizr stereotype line, `[Software System]` or `[Container: Rust]`.
fn stereotype_text(shape: &Shape) -> String {
    const NAMES: [(&str, &str); 4] = [
        ("person", "Person"),
        ("system", "Software System"),
        ("container", "Container"),
        ("component", "Component"),
    ];
    let element_type = text_of(&shape.type_c4_shape);
    let base = element_type
        .strip_prefix("external_")
        .unwrap_or(element_type);
    let base = base
        .strip_suffix("_db")
        .or_else(|| base.strip_suffix("_queue"))
        .unwrap_or(base);
    let label = NAMES
        .iter()
        .find(|(name, _)| *name == base)
        .map(|(_, display)| (*display).to_string())
        .unwrap_or_else(|| base.replace('_', " "));
    let techn = option_text(shape.techn.as_ref());
    if techn.is_empty() {
        format!("[{label}]")
    } else {
        format!("[{label}: {techn}]")
    }
}

// ---- stacked labels ----

#[derive(Clone)]
struct LabelSection {
    lines: Vec<TextLine>,
    font_size: f64,
    bold: bool,
    class: &'static str,
    width: f64,
    height: f64,
    top: f64,
}

#[derive(Clone)]
struct LabelStack {
    width: f64,
    height: f64,
    sections: Vec<LabelSection>,
}

impl LabelStack {
    fn build(
        name: &str,
        stereotype: &str,
        description: Option<&str>,
        element_font_size: f64,
        wrap_width: f64,
        bold_name: bool,
        measure: &dyn TextMeasure,
    ) -> Self {
        let sections: Vec<(&str, f64, bool, &'static str)> =
            std::iter::once((name, element_font_size, bold_name, "c4-name"))
                .chain(std::iter::once((
                    stereotype,
                    0.75 * element_font_size,
                    false,
                    "c4-type",
                )))
                .chain(description.map(|line| (line, 0.82 * element_font_size, false, "c4-descr")))
                .filter(|(text, _, _, _)| !text.is_empty())
                .collect();

        let mut rendered = Vec::with_capacity(sections.len());
        let mut total_width: f64 = 0.0;
        let mut total_height: f64 = 0.0;
        for (text, font_size, bold, class) in &sections {
            let body = sanitized_section(text);
            let font = measure_font(*font_size);
            let block = shape_text(
                &body,
                &TextOptions {
                    width: wrap_width,
                    markdown: false,
                    min_width: 0.0,
                },
                &font,
                measure,
            )
            .unwrap_or_default();
            let lines = block
                .lines
                .into_iter()
                .map(|line| {
                    if *bold {
                        TextLine::new(
                            line.spans
                                .into_iter()
                                .map(|mut span| {
                                    span.bold = true;
                                    span
                                })
                                .collect(),
                        )
                    } else {
                        line
                    }
                })
                .collect();
            total_width = total_width.max(block.size.width);
            total_height += block.size.height;
            rendered.push(LabelSection {
                lines,
                font_size: *font_size,
                bold: *bold,
                class,
                width: block.size.width,
                height: block.size.height,
                top: 0.0,
            });
        }
        if rendered.len() > 1 {
            total_height += SECTION_GAP * (rendered.len() - 1) as f64;
        }
        let mut top = 0.0;
        for section in &mut rendered {
            section.top = top;
            top += section.height + SECTION_GAP;
        }
        Self {
            width: total_width,
            height: total_height,
            sections: rendered,
        }
    }

    /// The label group `c4LabelHelper` returns, positioned at `offset`.
    fn into_group(self, offset: Point, fill: Option<Color>, source: Option<SourceRef>) -> Group {
        let mut group = Group::translated(offset.x, offset.y).with_class("label");
        group.source = source.clone();
        for section in self.sections {
            let mut section_group =
                Group::translated(self.width / 2.0, section.top).with_class(section.class);
            let size = Size::new(section.width, section.height);
            let mut run = TextRun {
                source: source.clone(),
                position: Point::new(0.0, LABEL_TEXT_Y),
                anchor: TextAnchor::Middle,
                baseline: TextBaseline::Alphabetic,
                rotation: 0.0,
                font: Font {
                    weight: if section.bold {
                        BOLD_WEIGHT
                    } else {
                        NORMAL_WEIGHT
                    },
                    ..measure_font(section.font_size)
                },
                fill: fill.clone(),
                opacity: 1.0,
                line_height: LINE_HEIGHT_EM * section.font_size,
                lines: section.lines,
                size,
                wrapping_width: None,
            };
            if run.lines.is_empty() {
                run.lines.push(TextLine::new(Vec::new()));
            }
            section_group.push(run);
            group.push(section_group);
        }
        group
    }
}

// ---- prepared shapes ----

#[derive(Clone, Copy, Debug)]
enum Geometry {
    Rounded {
        rx: f64,
    },
    Subroutine,
    Cylinder {
        w: f64,
        h: f64,
        rx: f64,
        ry: f64,
    },
    HorizontalCylinder {
        w: f64,
        h: f64,
        rx: f64,
        ry: f64,
    },
    Person {
        w: f64,
        body_top: f64,
        body_height: f64,
        head_radius: f64,
        body_radius: f64,
        total_height: f64,
    },
}

struct PreparedShape {
    element_type: String,
    label: LabelStack,
    label_offset: Point,
    width: f64,
    height: f64,
    geometry: Geometry,
    fill: Option<String>,
    stroke: Option<String>,
    font_color: String,
    outline: Option<Vec<Point>>,
}

impl PreparedShape {
    fn prepare(shape: &Shape, c4: &C4Config, measure: &dyn TextMeasure) -> Self {
        let element_type = text_of(&shape.type_c4_shape).to_string();
        let font = element_font(c4, &element_type);
        let font_size = font_size_number(&font.font_size);
        let wrap_width = if c4.wrap {
            (c4.width - 2.0 * c4.c4_shape_padding).max(MIN_WRAP_WIDTH)
        } else {
            f64::INFINITY
        };
        let description = match shape.descr.as_ref() {
            Some(text) if !option_text(Some(text)).is_empty() => Some(option_text(Some(text))),
            _ => None,
        };
        let label = LabelStack::build(
            text_of(&shape.label),
            &stereotype_text(shape),
            description,
            font_size,
            wrap_width,
            true,
            measure,
        );
        let (fill, stroke) = if is_c4_element_type(&element_type) {
            element_colors(c4, &element_type)
        } else {
            (None, None)
        };
        let fill = shape.bg_color.clone().or(fill);
        let stroke = shape.border_color.clone().or(stroke);
        let font_color = shape
            .font_color
            .clone()
            .unwrap_or_else(|| "#FFFFFF".to_string());

        let padding = c4.c4_shape_padding;
        let element_width = c4.width;
        let label_offset;
        let geometry;
        let (width, height);
        let mut outline = None;

        match resolve_shape(shape) {
            "person" => {
                let w = (label.width + padding * 2.0).max(element_width).max(100.0);
                let head_radius = (w * 0.23).clamp(16.0, 56.0);
                let overlap = head_radius * 0.27;
                let body_height = (label.height + padding * 2.0).max(0.0);
                let body_radius = (w * 0.177).min(body_height * 0.45);
                let total_height = body_height + 2.0 * head_radius - overlap;
                let top = -total_height / 2.0;
                let body_top = top + 2.0 * head_radius - overlap;
                let head_center_y = top + head_radius;
                label_offset = Point::new(
                    -label.width / 2.0,
                    body_top + body_height / 2.0 - label.height / 2.0,
                );
                width = w;
                height = total_height;
                geometry = Geometry::Person {
                    w,
                    body_top,
                    body_height,
                    head_radius,
                    body_radius,
                    total_height,
                };

                let phi_right = ((body_top - head_center_y) / head_radius)
                    .clamp(-1.0, 1.0)
                    .asin()
                    .to_degrees();
                let mut points = generate_circle_points(
                    0.0,
                    -head_center_y,
                    head_radius,
                    24,
                    180.0 + phi_right,
                    -phi_right,
                );
                points.extend(generate_circle_points(
                    -(-w / 2.0 + body_radius),
                    -(body_top + body_radius),
                    body_radius,
                    12,
                    90.0,
                    0.0,
                ));
                points.extend(generate_circle_points(
                    -(-w / 2.0 + body_radius),
                    -(total_height / 2.0 - body_radius),
                    body_radius,
                    12,
                    360.0,
                    270.0,
                ));
                points.extend(generate_circle_points(
                    -(w / 2.0 - body_radius),
                    -(total_height / 2.0 - body_radius),
                    body_radius,
                    12,
                    270.0,
                    180.0,
                ));
                points.extend(generate_circle_points(
                    -(w / 2.0 - body_radius),
                    -(body_top + body_radius),
                    body_radius,
                    12,
                    180.0,
                    90.0,
                ));
                outline = Some(points);
            }
            "cylinder" => {
                let label_padding = padding;
                let original_width = element_width;
                let node_width = (original_width - label_padding).max(8.0);
                let w = node_width.max(label.width) + label_padding;
                let rx = w / 2.0;
                let ry = rx / (2.5 + w / 50.0);
                let h = label.height + label_padding + ry;
                label_offset = Point::new(-label.width / 2.0, -label.height / 2.0 + padding / 1.5);
                width = w;
                height = h + 2.0 * ry;
                geometry = Geometry::Cylinder { w, h, rx, ry };
            }
            "h-cyl" => {
                let label_padding = padding / 2.0;
                let node_width = (element_width - label_padding).max(10.0);
                let h = label.height + label_padding;
                let ry = h / 2.0;
                let rx = ry / (2.5 + h / 50.0);
                let w = node_width.max(label.width) + rx + label_padding;
                label_offset = Point::new(-label.width / 2.0 - rx, -label.height / 2.0);
                width = w + 2.0 * rx;
                height = h;
                geometry = Geometry::HorizontalCylinder { w, h, rx, ry };
            }
            "fr-rect" => {
                let total_width = (label.width + 2.0 * 8.0 + padding).max(element_width);
                let total_height = (label.height + padding).max(0.0);
                let w = total_width - 2.0 * 8.0;
                let h = total_height;
                label_offset = Point::new(-label.width / 2.0, -label.height / 2.0);
                width = total_width;
                height = total_height;
                geometry = Geometry::Subroutine;
                let points = [
                    Point::new(0.0, 0.0),
                    Point::new(w, 0.0),
                    Point::new(w, -h),
                    Point::new(0.0, -h),
                    Point::new(0.0, 0.0),
                    Point::new(-8.0, 0.0),
                    Point::new(w + 8.0, 0.0),
                    Point::new(w + 8.0, -h),
                    Point::new(-8.0, -h),
                    Point::new(-8.0, 0.0),
                ];
                outline = Some(
                    points
                        .iter()
                        .map(|point| Point::new(point.x - w / 2.0, point.y + h / 2.0))
                        .collect(),
                );
            }
            _ => {
                let width_ = (label.width + padding * 2.0).max(element_width);
                let height_ = (label.height + padding * 2.0).max(0.0);
                label_offset = Point::new(-label.width / 2.0, -label.height / 2.0);
                width = width_;
                height = height_;
                geometry = Geometry::Rounded { rx: 12.0 };
            }
        }
        Self {
            element_type,
            label,
            label_offset,
            width,
            height,
            geometry,
            fill,
            stroke,
            font_color,
            outline,
        }
    }

    fn css_classes(&self) -> String {
        let mut classes = format!("node c4-shape c4-{}", self.element_type);
        if self.element_type.starts_with("external_") {
            classes.push_str(" c4-external");
        }
        classes
    }

    fn style(&self) -> PathStyle {
        PathStyle {
            fill: Some(Color::new(
                self.fill.clone().unwrap_or_else(|| "#000000".to_string()),
            )),
            stroke: self.stroke.clone().map(Color::new),
            stroke_width: 2.0,
            ..Default::default()
        }
    }

    fn shape_group(&self, source: SourceRef) -> Group {
        let mut group = Group::new().with_class("basic label-container");
        group.source = Some(source.clone());
        let style = self.style();
        match self.geometry {
            Geometry::Rounded { rx } => {
                group.push(
                    RectShape::new(
                        Rect::centered(Point::ZERO, Size::new(self.width, self.height)),
                        rx,
                        style,
                    )
                    .with_source(source),
                );
            }
            Geometry::Subroutine => {
                let path = Path::polygon(self.outline.clone().unwrap_or_default());
                group.push(PathShape::new(path, style).with_source(source));
            }
            Geometry::Cylinder { w, h, rx, ry } => {
                let mut path = Path::new();
                let left = -w / 2.0;
                let top = -h / 2.0;
                path.move_to(Point::new(left, top));
                path.arc_to(rx, ry, 0.0, false, false, Point::new(left + w, top));
                path.arc_to(rx, ry, 0.0, false, false, Point::new(left, top));
                path.line_to(Point::new(left, top + h));
                path.arc_to(rx, ry, 0.0, false, false, Point::new(left + w, top + h));
                path.line_to(Point::new(left + w, top));
                group.push(PathShape::new(path, style).with_source(source));
            }
            Geometry::HorizontalCylinder { w, h, rx, ry } => {
                let mut path = Path::new();
                let left = -w / 2.0;
                let top = h / 2.0;
                path.move_to(Point::new(left, top));
                path.arc_to(rx, ry, 0.0, false, true, Point::new(left, top - h));
                path.line_to(Point::new(left + w, top - h));
                path.arc_to(rx, ry, 0.0, false, true, Point::new(left + w, top));
                path.move_to(Point::new(left + w, top - h));
                path.arc_to(rx, ry, 0.0, false, false, Point::new(left + w, top));
                path.line_to(Point::new(left, top));
                group.push(PathShape::new(path, style).with_source(source));
            }
            Geometry::Person {
                w,
                body_top,
                body_height,
                head_radius,
                body_radius,
                total_height,
            } => {
                let _ = total_height;
                group.push(
                    RectShape::new(
                        Rect::new(-w / 2.0, body_top, w, body_height),
                        body_radius,
                        style.clone(),
                    )
                    .with_source(source.clone()),
                );
                let top = -total_height / 2.0;
                group.push(
                    EllipseShape::new(
                        Point::new(0.0, top + head_radius),
                        head_radius,
                        head_radius,
                        style,
                    )
                    .with_source(source),
                );
            }
        }
        group
    }
}

/// `util.ts`'s `generateCirclePoints` (positive y points down here, so negated as upstream).
fn generate_circle_points(
    center_x: f64,
    center_y: f64,
    radius: f64,
    count: usize,
    start_angle: f64,
    end_angle: f64,
) -> Vec<Point> {
    if count == 0 {
        return Vec::new();
    }
    let step = if count == 1 {
        0.0
    } else {
        (end_angle - start_angle).to_radians() / (count - 1) as f64
    };
    let start = start_angle.to_radians();
    (0..count)
        .map(|index| {
            let angle = start + index as f64 * step;
            Point::new(
                -(center_x + radius * angle.cos()),
                -(center_y + radius * angle.sin()),
            )
        })
        .collect()
}

// ---- layout ----

#[derive(Clone, Copy, Debug)]
struct BoundsData {
    startx: f64,
    stopx: f64,
    starty: f64,
    stopy: f64,
    width_limit: f64,
}

#[derive(Clone, Copy, Debug)]
struct NextBoundsData {
    startx: f64,
    stopx: f64,
    starty: f64,
    stopy: f64,
    cnt: usize,
}

struct Bounds {
    data: BoundsData,
    next: NextBoundsData,
}

impl Bounds {
    fn new() -> Self {
        Self {
            data: BoundsData {
                startx: 0.0,
                stopx: 0.0,
                starty: 0.0,
                stopy: 0.0,
                width_limit: 0.0,
            },
            next: NextBoundsData {
                startx: 0.0,
                stopx: 0.0,
                starty: 0.0,
                stopy: 0.0,
                cnt: 0,
            },
        }
    }

    fn set_data(&mut self, startx: f64, stopx: f64, starty: f64, stopy: f64) {
        self.next.startx = startx;
        self.data.startx = startx;
        self.next.stopx = stopx;
        self.data.stopx = stopx;
        self.next.starty = starty;
        self.data.starty = starty;
        self.next.stopy = stopy;
        self.data.stopy = stopy;
    }

    fn insert(
        &mut self,
        margin: f64,
        next_line_padding_x: f64,
        shape_in_row: f64,
        width: f64,
        height: f64,
    ) -> Point {
        self.next.cnt += 1;
        let mut startx = if self.next.startx == self.next.stopx {
            self.next.stopx + margin
        } else {
            self.next.stopx + margin * 2.0
        };
        let mut stopx = startx + width;
        let mut starty = self.next.starty + margin * 2.0;
        let mut stopy = starty + height;
        if startx >= self.data.width_limit
            || stopx >= self.data.width_limit
            || self.next.cnt as f64 > shape_in_row
        {
            startx = self.next.startx + margin + next_line_padding_x;
            starty = self.next.stopy + margin * 2.0;
            stopx = startx + width;
            self.next.stopx = stopx;
            self.next.starty = self.next.stopy;
            stopy = starty + height;
            self.next.stopy = stopy;
            self.next.cnt = 1;
        }
        self.data.startx = self.data.startx.min(startx);
        self.data.starty = self.data.starty.min(starty);
        self.data.stopx = self.data.stopx.max(stopx);
        self.data.stopy = self.data.stopy.max(stopy);
        self.next.startx = self.next.startx.min(startx);
        self.next.starty = self.next.starty.min(starty);
        self.next.stopx = self.next.stopx.max(stopx);
        self.next.stopy = self.next.stopy.max(stopy);
        Point::new(startx, starty)
    }

    fn bump_last_margin(&mut self, margin: f64) {
        self.data.stopx += margin;
        self.data.stopy += margin;
    }
}

#[derive(Clone, Copy, Debug)]
struct Placed {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    geometry: Geometry,
}

impl Placed {
    fn center(&self) -> Point {
        Point::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }
}

fn intersect_placed(placed: &Placed, outline: Option<&[Point]>, point: Point) -> Point {
    let center = placed.center();
    let rect = Rect::centered(center, Size::new(placed.width, placed.height));
    match placed.geometry {
        Geometry::Rounded { .. } | Geometry::Subroutine => {
            if let Some(outline) = outline {
                intersect_polygon(rect, outline, point)
            } else {
                intersect_rect(rect, point)
            }
        }
        Geometry::Person { .. } => outline
            .map(|points| intersect_polygon(rect, points, point))
            .unwrap_or_else(|| intersect_rect(rect, point)),
        Geometry::Cylinder { rx, ry, .. } => {
            let pos = intersect_rect(rect, point);
            let dx = pos.x - center.x;
            let within_x = dx.abs() < placed.width / 2.0
                || (dx.abs() == placed.width / 2.0
                    && (pos.y - center.y).abs() > placed.height / 2.0 - ry);
            if rx != 0.0 && within_x {
                let mut y = ry * ry * (1.0 - (dx * dx) / (rx * rx));
                if y > 0.0 {
                    y = y.sqrt();
                }
                y = ry - y;
                if point.y - center.y > 0.0 {
                    y = -y;
                }
                Point::new(pos.x, pos.y + y)
            } else {
                pos
            }
        }
        Geometry::HorizontalCylinder { rx, ry, .. } => {
            let pos = intersect_rect(rect, point);
            let dy = pos.y - center.y;
            let within_y = dy.abs() < placed.height / 2.0
                || (dy.abs() == placed.height / 2.0
                    && (pos.x - center.x).abs() > placed.width / 2.0 - rx);
            if ry != 0.0 && within_y {
                let mut x = rx * rx * (1.0 - (dy * dy) / (ry * ry));
                if x != 0.0 {
                    x = x.abs().sqrt();
                }
                x = rx - x;
                if point.x - center.x > 0.0 {
                    x = -x;
                }
                Point::new(pos.x + x, pos.y)
            } else {
                pos
            }
        }
    }
}

// ---- measured texts for boundaries and relationships ----

struct MeasuredText {
    text: String,
    width: f64,
    height: f64,
}

/// `calcC4ShapeTextWH`'s unwrapped branch. Wrapping is gated on the element's own `wrap` flag,
/// which the C4 parser always leaves false, so only the unwrapped measurement is reachable.
fn measure_text(text: &str, font_size: f64, measure: &dyn TextMeasure) -> MeasuredText {
    let font = measure_font(font_size);
    MeasuredText {
        text: text.to_string(),
        width: text_width(text, &font, measure),
        height: text_height(text, &font, measure),
    }
}

/// How a boundary or relationship text run is painted.
#[derive(Clone, Copy)]
struct RunStyle<'a> {
    font_size: f64,
    bold: bool,
    italic: bool,
    fill: &'a str,
}

fn boundary_text_run(
    text: &MeasuredText,
    x: f64,
    y: f64,
    width: f64,
    style: RunStyle<'_>,
    source: SourceRef,
) -> TextRun {
    let lines: Vec<TextLine> = sanitize::split_breaks(&text.text)
        .into_iter()
        .map(|line| {
            TextLine::new(vec![crate::scene::TextSpan {
                text: line.to_string(),
                bold: style.bold,
                italic: style.italic,
                ..Default::default()
            }])
        })
        .collect();
    TextRun {
        source: Some(source),
        position: Point::new(x + width / 2.0, y),
        anchor: TextAnchor::Middle,
        baseline: TextBaseline::Middle,
        rotation: 0.0,
        font: measure_font(style.font_size),
        fill: Some(Color::new(style.fill)),
        opacity: 1.0,
        line_height: style.font_size,
        lines,
        size: Size::new(text.width, text.height),
        wrapping_width: None,
    }
}

// ---- renderer state ----

struct RenderState<'a> {
    c4: &'a C4Config,
    root_font_size: f64,
    shape_in_row: f64,
    boundary_in_row: f64,
    prepared: HashMap<String, PreparedShape>,
    placed: HashMap<String, Placed>,
    boundaries: HashMap<String, Placed>,
    items: Vec<Item>,
    global_max_x: f64,
    global_max_y: f64,
    measure: &'a dyn TextMeasure,
    diagram_type: String,
}

impl RenderState<'_> {
    fn item_for(&mut self, item: impl Into<Item>) {
        self.items.push(item.into());
    }
}

fn draw_c4_shape_array(state: &mut RenderState<'_>, bounds: &mut Bounds, shapes: &[&Shape]) {
    let margin = state.c4.c4_shape_margin;
    for shape in shapes {
        let Some(prepared) = state.prepared.get(&shape.alias) else {
            continue;
        };
        let top_left = bounds.insert(
            margin,
            state.c4.next_line_padding_x,
            state.shape_in_row,
            prepared.width,
            prepared.height,
        );
        state.placed.insert(
            shape.alias.clone(),
            Placed {
                x: top_left.x,
                y: top_left.y,
                width: prepared.width,
                height: prepared.height,
                geometry: prepared.geometry,
            },
        );
    }
    // Draw in declaration order once every position is known (pass 2).
    for shape in shapes {
        let Some(placed) = state.placed.get(&shape.alias) else {
            continue;
        };
        let Some(prepared) = state.prepared.get(&shape.alias) else {
            continue;
        };
        let center = placed.center();
        let source = SourceRef::node(shape.alias.clone());
        let mut node_group = Group::new()
            .with_class(prepared.css_classes())
            .with_source(source.clone());
        node_group.push(prepared.shape_group(source.clone()));
        node_group.push(prepared.label.clone().into_group(
            prepared.label_offset,
            Some(Color::new(prepared.font_color.clone())),
            Some(source),
        ));
        let mut outer = Group::translated(center.x, center.y);
        outer.push(node_group);
        state.item_for(outer);
    }
    bounds.bump_last_margin(margin);
}

/// Renders an already parsed C4 database.
pub fn draw_ast(ast: &Ast, config: &Config, measure: &dyn TextMeasure) -> Scene {
    let c4 = &config.c4;
    let mut prepared = HashMap::new();
    for shape in &ast.shapes {
        prepared.insert(
            shape.alias.clone(),
            PreparedShape::prepare(shape, c4, measure),
        );
    }
    let mut state = RenderState {
        c4,
        root_font_size: config.font_size,
        shape_in_row: ast.shape_in_row as f64,
        boundary_in_row: ast.boundary_in_row as f64,
        prepared,
        placed: HashMap::new(),
        boundaries: HashMap::new(),
        items: Vec::new(),
        global_max_x: c4.diagram_margin_x,
        global_max_y: c4.diagram_margin_y,
        measure,
        diagram_type: ast.c4_type.clone().unwrap_or_default(),
    };

    let mut screen = Bounds::new();
    screen.set_data(
        c4.diagram_margin_x,
        c4.diagram_margin_x,
        c4.diagram_margin_y,
        c4.diagram_margin_y,
    );
    screen.data.width_limit = SCREEN_AVAIL_WIDTH;

    let roots: Vec<&Boundary> = ast.boundaries_within("");
    let mut screen_bounds = screen;
    draw_inside_boundary(ast, &mut state, &mut screen_bounds, &roots);

    let diagram_type = state.diagram_type.clone();
    draw_rels(ast, &mut state, &diagram_type);

    let mut scene = Scene::default();
    add_marker_defs(&mut scene);
    scene.items = std::mem::take(&mut state.items);

    let box_startx = c4.diagram_margin_x;
    let box_starty = c4.diagram_margin_y;
    let box_width = state.global_max_x - box_startx;
    let box_height = state.global_max_y - box_starty;
    let height = box_height + 2.0 * c4.diagram_margin_y;
    let width = box_width + 2.0 * c4.diagram_margin_x;

    if !ast.title.is_empty() {
        scene.push(TextRun {
            source: None,
            position: Point::new(
                box_width / 2.0 - 4.0 * c4.diagram_margin_x,
                box_starty + c4.diagram_margin_y,
            ),
            anchor: TextAnchor::Start,
            baseline: TextBaseline::Alphabetic,
            rotation: 0.0,
            font: measure_font(state.root_font_size),
            fill: None,
            opacity: 1.0,
            line_height: LINE_HEIGHT_EM * state.root_font_size,
            lines: vec![TextLine::new(vec![crate::scene::TextSpan::plain(
                ast.title.clone(),
            )])],
            size: Size::new(
                text_width(&ast.title, &measure_font(state.root_font_size), measure),
                text_height(&ast.title, &measure_font(state.root_font_size), measure),
            ),
            wrapping_width: None,
        });
    }

    let extra = if ast.title.is_empty() {
        0.0
    } else {
        TITLE_EXTRA_HEIGHT
    };
    scene.view_box = Rect::new(
        box_startx - c4.diagram_margin_x,
        -(c4.diagram_margin_y + extra),
        width,
        height + extra,
    );
    scene.bounds = scene.view_box;
    scene
}

fn draw_inside_boundary(
    ast: &Ast,
    state: &mut RenderState<'_>,
    parent_bounds: &mut Bounds,
    boundaries: &[&Boundary],
) {
    if boundaries.is_empty() {
        return;
    }
    let c4 = state.c4;
    let mut current = Bounds::new();
    current.data.width_limit =
        parent_bounds.data.width_limit / (state.boundary_in_row.min(boundaries.len() as f64));

    for (index, boundary) in boundaries.iter().enumerate() {
        let mut text_top = 0.0;
        if boundary.sprite.is_some() {
            text_top += 48.0;
        }

        let boundary_font_size = font_size_number(&c4.boundary_font_size);
        let label_font_size = boundary_font_size + 2.0;
        let label = measure_text(
            option_text(Some(&boundary.label)),
            label_font_size,
            state.measure,
        );
        let label_y = text_top + 8.0;
        text_top = label_y + label.height;

        let type_text = boundary
            .type_name
            .as_ref()
            .map(text_of)
            .unwrap_or("")
            .to_string();
        let type_measured = if type_text.is_empty() {
            None
        } else {
            let measured =
                measure_text(&format!("[{type_text}]"), boundary_font_size, state.measure);
            let type_y = text_top + 5.0;
            text_top = type_y + measured.height;
            Some((measured, type_y))
        };

        let descr_text = option_text(boundary.descr.as_ref()).to_string();
        let descr_measured = if descr_text.is_empty() {
            None
        } else {
            let measured = measure_text(&descr_text, boundary_font_size - 2.0, state.measure);
            let descr_y = text_top + 20.0;
            text_top = descr_y + measured.height;
            Some((measured, descr_y))
        };

        if index == 0 || index % (state.boundary_in_row as usize) == 0 {
            let x = parent_bounds.data.startx + c4.diagram_margin_x;
            let y = parent_bounds.data.stopy + c4.diagram_margin_y + text_top;
            current.set_data(x, x, y, y);
        } else {
            let x = if current.data.stopx != current.data.startx {
                current.data.stopx + c4.diagram_margin_x
            } else {
                current.data.startx
            };
            let y = current.data.starty;
            current.set_data(x, x, y, y);
        }

        let shapes = ast.shapes_within(&boundary.alias);
        if !shapes.is_empty() {
            draw_c4_shape_array(state, &mut current, &shapes);
        }

        let nested = ast.boundaries_within(&boundary.alias);
        if !nested.is_empty() {
            draw_inside_boundary(ast, state, &mut current, &nested);
        }

        if boundary.alias != "global" {
            draw_boundary(
                state,
                &mut current,
                boundary,
                &label,
                &type_measured,
                &descr_measured,
                label_y,
            );
        }

        parent_bounds.data.stopy =
            (current.data.stopy + c4.c4_shape_margin).max(parent_bounds.data.stopy);
        parent_bounds.data.stopx =
            (current.data.stopx + c4.c4_shape_margin).max(parent_bounds.data.stopx);
        state.global_max_x = state.global_max_x.max(parent_bounds.data.stopx);
        state.global_max_y = state.global_max_y.max(parent_bounds.data.stopy);
    }
}

fn draw_boundary(
    state: &mut RenderState<'_>,
    bounds: &mut Bounds,
    boundary: &Boundary,
    label: &MeasuredText,
    type_text: &Option<(MeasuredText, f64)>,
    descr_text: &Option<(MeasuredText, f64)>,
    label_y: f64,
) {
    let x = bounds.data.startx;
    let y = bounds.data.starty;
    let width = bounds.data.stopx - x;
    let height = bounds.data.stopy - y;
    state.boundaries.insert(
        boundary.alias.clone(),
        Placed {
            x,
            y,
            width,
            height,
            geometry: Geometry::Rounded { rx: 2.5 },
        },
    );

    let source = SourceRef::cluster(boundary.alias.clone());
    let fill = boundary
        .bg_color
        .clone()
        .map(Color::new)
        .unwrap_or_else(Color::none);
    let stroke = Color::new(
        boundary
            .border_color
            .clone()
            .unwrap_or_else(|| "#444444".to_string()),
    );
    let dashed = boundary.node_type.is_none();
    let mut style = PathStyle {
        fill: Some(fill),
        stroke: Some(stroke),
        stroke_width: 1.0,
        ..Default::default()
    };
    if dashed {
        style.dash = vec![7.0, 7.0];
    }
    let mut group = Group::new();
    group.push(
        RectShape::new(Rect::new(x, y, width, height), 2.5, style).with_source(source.clone()),
    );

    let root_font_size = state.root_font_size;
    group.push(boundary_text_run(
        label,
        x,
        y + label_y,
        width,
        RunStyle {
            font_size: root_font_size,
            bold: true,
            italic: false,
            fill: "#444444",
        },
        source.clone(),
    ));
    if let Some((measured, type_y)) = type_text {
        group.push(boundary_text_run(
            measured,
            x,
            y + type_y,
            width,
            RunStyle {
                font_size: root_font_size,
                bold: false,
                italic: false,
                fill: "#444444",
            },
            source.clone(),
        ));
    }
    if let Some((measured, descr_y)) = descr_text {
        group.push(boundary_text_run(
            measured,
            x,
            y + descr_y,
            width,
            RunStyle {
                font_size: root_font_size,
                bold: false,
                italic: false,
                fill: "#444444",
            },
            source,
        ));
    }
    state.item_for(group);
}

fn draw_rels(ast: &Ast, state: &mut RenderState<'_>, diagram_type: &str) {
    if ast.rels.is_empty() {
        return;
    }
    let mut group = Group::new();
    for (index, rel) in ast.rels.iter().enumerate() {
        let from = state
            .placed
            .get(&rel.from)
            .or_else(|| state.boundaries.get(&rel.from));
        let Some(from) = from else {
            continue;
        };
        let to = state
            .placed
            .get(&rel.to)
            .or_else(|| state.boundaries.get(&rel.to));
        let Some(to) = to else {
            continue;
        };
        let from_outline = state
            .prepared
            .get(&rel.from)
            .and_then(|prepared| prepared.outline.as_deref());
        let start_point = intersect_placed(from, from_outline, to.center());
        let to_outline = state
            .prepared
            .get(&rel.to)
            .and_then(|prepared| prepared.outline.as_deref());
        let end_point = intersect_placed(to, to_outline, from.center());

        let text_color = rel
            .text_color
            .clone()
            .unwrap_or_else(|| "#444444".to_string());
        let line_color = rel
            .line_color
            .clone()
            .unwrap_or_else(|| "#444444".to_string());
        let offset_x = rel.offset_x.unwrap_or(0) as f64;
        let offset_y = rel.offset_y.unwrap_or(0) as f64;

        let source = SourceRef::edge(format!("{}-{}", rel.from, rel.to));
        let style = PathStyle {
            fill: Some(Color::none()),
            stroke: Some(Color::new(line_color)),
            stroke_width: 1.0,
            ..Default::default()
        };
        let mut path = Path::new();
        if index == 0 {
            path.move_to(start_point);
            path.line_to(end_point);
        } else {
            let control = Point::new(
                start_point.x + (end_point.x - start_point.x) / 2.0
                    - (end_point.x - start_point.x) / 4.0,
                start_point.y + (end_point.y - start_point.y) / 2.0,
            );
            path.move_to(start_point);
            path.quad_to(control, end_point);
        }
        let mut shape = PathShape::new(path, style).with_source(source.clone());
        if rel.type_name != "rel_b" {
            shape = shape.with_end_marker(MarkerRef::new(ARROWHEAD));
        }
        if rel.type_name == "birel" || rel.type_name == "rel_b" {
            shape = shape.with_start_marker(MarkerRef::new(ARROWEND));
        }
        group.push(shape);

        let mut label_text = option_text(Some(&rel.label)).to_string();
        if diagram_type == "C4Dynamic" {
            label_text = format!("{}: {label_text}", index + 1);
        }
        let label_font = font_size_number(&state.c4.message_font_size);
        let label = measure_text(&label_text, label_font, state.measure);
        let mid_x =
            start_point.x.min(end_point.x) + (end_point.x - start_point.x).abs() / 2.0 + offset_x;
        let mid_y =
            start_point.y.min(end_point.y) + (end_point.y - start_point.y).abs() / 2.0 + offset_y;

        group.push(relation_text_run(
            &label,
            mid_x + label.width / 2.0,
            mid_y,
            RunStyle {
                font_size: state.root_font_size,
                bold: false,
                italic: false,
                fill: &text_color,
            },
            source.clone(),
        ));

        let techn_text = option_text(rel.techn.as_ref());
        if !techn_text.is_empty() {
            let techn = measure_text(techn_text, label_font, state.measure);
            let techn_y = mid_y + font_size_number(&state.c4.message_font_size) + 5.0;
            let drawn = MeasuredText {
                text: format!("[{techn_text}]"),
                width: techn.width,
                height: techn.height,
            };
            group.push(relation_text_run(
                &drawn,
                mid_x + label.width.max(techn.width) / 2.0,
                techn_y,
                RunStyle {
                    font_size: state.root_font_size,
                    bold: false,
                    italic: true,
                    fill: &text_color,
                },
                source,
            ));
        }
    }
    if !group.items.is_empty() {
        state.item_for(group);
    }
}

fn relation_text_run(
    text: &MeasuredText,
    x: f64,
    y: f64,
    style: RunStyle<'_>,
    source: SourceRef,
) -> TextRun {
    let lines: Vec<TextLine> = sanitize::split_breaks(&text.text)
        .into_iter()
        .map(|line| {
            TextLine::new(vec![crate::scene::TextSpan {
                text: line.to_string(),
                italic: style.italic,
                ..Default::default()
            }])
        })
        .collect();
    TextRun {
        source: Some(source),
        position: Point::new(x, y),
        anchor: TextAnchor::Middle,
        baseline: TextBaseline::Middle,
        rotation: 0.0,
        font: measure_font(style.font_size),
        fill: Some(Color::new(style.fill)),
        opacity: 1.0,
        line_height: style.font_size,
        lines,
        size: Size::new(text.width, text.height),
        wrapping_width: None,
    }
}

fn add_marker_defs(scene: &mut Scene) {
    let mut arrowhead = MarkerDef::new(ARROWHEAD, Rect::new(0.0, 0.0, 10.0, 10.0))
        .with_reference(Point::new(9.0, 5.0))
        .with_size(12.0, 12.0)
        .with_orient(MarkerOrient::Auto)
        .with_class("marker");
    arrowhead.units = MarkerUnits::UserSpaceOnUse;
    let mut head = Path::new();
    head.move_to(Point::new(0.0, 0.0));
    head.line_to(Point::new(10.0, 5.0));
    head.line_to(Point::new(0.0, 10.0));
    head.close();
    arrowhead.push(PathShape::new(
        head,
        PathStyle {
            fill: Some(Color::new("#333333")),
            stroke: Some(Color::new("#333333")),
            ..Default::default()
        },
    ));
    scene.define(arrowhead);

    let mut arrowend = MarkerDef::new(ARROWEND, Rect::new(0.0, 0.0, 10.0, 10.0))
        .with_reference(Point::new(1.0, 5.0))
        .with_size(12.0, 12.0)
        .with_orient(MarkerOrient::Auto)
        .with_class("marker");
    arrowend.units = MarkerUnits::UserSpaceOnUse;
    let mut tail = Path::new();
    tail.move_to(Point::new(10.0, 0.0));
    tail.line_to(Point::new(0.0, 5.0));
    tail.line_to(Point::new(10.0, 10.0));
    tail.close();
    arrowend.push(PathShape::new(
        tail,
        PathStyle {
            fill: Some(Color::new("#333333")),
            stroke: Some(Color::new("#333333")),
            ..Default::default()
        },
    ));
    scene.define(arrowend);

    let mut filled = MarkerDef::new(FILLED_HEAD, Rect::new(0.0, 0.0, 20.0, 28.0))
        .with_reference(Point::new(18.0, 7.0))
        .with_size(20.0, 28.0)
        .with_orient(MarkerOrient::Auto)
        .with_class("marker");
    filled.units = MarkerUnits::StrokeWidth;
    let mut filled_path = Path::new();
    filled_path.move_to(Point::new(18.0, 7.0));
    filled_path.line_to(Point::new(9.0, 13.0));
    filled_path.line_to(Point::new(14.0, 7.0));
    filled_path.line_to(Point::new(9.0, 1.0));
    filled_path.close();
    filled.push(PathShape::new(
        filled_path,
        PathStyle {
            fill: Some(Color::new("#333333")),
            stroke: Some(Color::new("#333333")),
            ..Default::default()
        },
    ));
    scene.define(filled);

    let mut crosshead = MarkerDef::new(CROSSHEAD, Rect::new(0.0, 0.0, 15.0, 8.0))
        .with_reference(Point::new(16.0, 4.0))
        .with_size(15.0, 8.0)
        .with_orient(MarkerOrient::Auto)
        .with_class("marker");
    crosshead.units = MarkerUnits::StrokeWidth;
    let mut arrow = Path::new();
    arrow.move_to(Point::new(9.0, 2.0));
    arrow.line_to(Point::new(9.0, 6.0));
    arrow.line_to(Point::new(16.0, 4.0));
    arrow.close();
    crosshead.push(PathShape::new(
        arrow,
        PathStyle {
            fill: Some(Color::new("#000000")),
            stroke: Some(Color::new("#000000")),
            ..Default::default()
        },
    ));
    let mut cross = Path::new();
    cross.move_to(Point::new(0.0, 1.0));
    cross.line_to(Point::new(6.0, 7.0));
    cross.move_to(Point::new(6.0, 1.0));
    cross.line_to(Point::new(0.0, 7.0));
    crosshead.push(PathShape::new(
        cross,
        PathStyle {
            fill: Some(Color::none()),
            stroke: Some(Color::new("#000000")),
            ..Default::default()
        },
    ));
    scene.define(crosshead);
}

#[cfg(test)]
mod tests;
