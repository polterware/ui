//! Source-to-[`Scene`] rendering for cynefin maps: the Rust side of `cynefinRenderer.ts` `draw`,
//! with the boundary geometry of `cynefinBoundaries.ts` and the declarations of `styles.ts`.
//!
//! A cynefin map does not go through the unified layout pipeline: `draw` builds the SVG by hand
//! from the configured canvas. [`draw`] parses, [`draw_ast`] renders an already parsed database,
//! and [`render`] uses the crate's monospace fallback measurement. The scene follows `draw`'s
//! order: four domain background rectangles, the two wavy boundaries and the cliff, the
//! confusion ellipse, the domain labels, the subtitles, the item badges, the transition arrows
//! with their marker, and the title. The group that holds all of it is translated by `padding`,
//! and the viewport is `0 0 (width + 2 * padding) (height + 2 * padding)`, the rectangle
//! `configureSvgSize` and the explicit `viewBox` set.
//!
//! The item badges are the one place `draw` measures text: each label's rendered width decides
//! its badge width. It measures with `getBBox()`, which the fixture harness answers with the
//! shared monospace rule, so [`TextMeasure`] measures the same. The label's own font size comes
//! from `.cynefinItemText` (`itemFontSize`).
//!
//! The wavy boundaries are seeded: `resolveSeed` takes the configured seed when it is finite and
//! non-zero, and otherwise hashes the SVG element id. A [`Scene`] carries no element id, so
//! [`render`] and [`draw`] use [`DIAGRAM_ID`] as the id; a caller that has the real one passes
//! it to [`draw_ast_with_id`].
//!
//! Where the port stops short of upstream: the class names (`cynefinDomainLabel`,
//! `cynefinBoundary`, ...) matter only to CSS, and a [`Scene`] carries paints rather than
//! selectors, so `draw` resolves the classes' declarations (fills, strokes, widths, dashes,
//! font sizes and styles) into the shapes. The `cynefinArrowHead` class is kept on the marker
//! definition for a painter that themes by marker kind. The accessibility `<title>`/`<desc>`
//! elements are not drawn: they are metadata, not paint, and [`Ast`] keeps the strings.

use serde_json::Value;

use crate::config::Config;
use crate::error::ParseError;
use crate::measure::{Font, TextMeasure, TextOptions, shape_text};
use crate::render::shapes::label_image_utils::parse_font_size;
use crate::scene::{
    Color, Group, MarkerDef, MarkerOrient, MarkerRef, Path, PathShape, PathStyle, Point, Rect,
    RectShape, Scene, TextAnchor, TextBaseline, TextRun,
};

use super::{Ast, DomainName};

/// The element id the wavy boundary seed falls back to when the configuration's seed is zero.
///
/// Upstream hashes the id of the SVG element it renders into, which a [`Scene`] does not carry;
/// this constant is the id the fixtures are generated with, so a default-seeded diagram is
/// reproducible.
pub const DIAGRAM_ID: &str = "cynefin";

/// `draw`'s `itemHeight`: the height of one item badge.
const ITEM_HEIGHT: f64 = 26.0;
/// `draw`'s `itemPaddingX`: the horizontal padding inside an item badge.
const ITEM_PADDING_X: f64 = 10.0;
/// `draw`'s `MAX_CONFUSION_ITEMS`: the confusion ellipse's overflow threshold.
const MAX_CONFUSION_ITEMS: usize = 3;
/// `generateFoldPath`'s and `generateHorizontalBoundary`'s `segments`.
const BOUNDARY_SEGMENTS: i32 = 7;

/// Renders a cynefin body (the text [`super::parse`] accepts) to a [`Scene`].
///
/// `config` is the diagram configuration after the source's frontmatter and directives have
/// been applied, as `getConfig()` returns it to `draw`.
pub fn draw(body: &str, config: &Config, measure: &dyn TextMeasure) -> Result<Scene, ParseError> {
    let ast = super::parse(body)?;
    Ok(draw_ast(&ast, config, measure))
}

/// Renders an already parsed cynefin database with [`DIAGRAM_ID`] as the element id.
pub fn draw_ast(ast: &Ast, config: &Config, measure: &dyn TextMeasure) -> Scene {
    draw_ast_with_id(ast, config, DIAGRAM_ID, measure)
}

/// Renders an already parsed cynefin database with `id` as the SVG element id.
///
/// The id seeds the wavy boundaries when the configuration does not set a non-zero seed, as
/// `resolveSeed(config.seed, id)` does.
pub fn draw_ast_with_id(ast: &Ast, config: &Config, id: &str, measure: &dyn TextMeasure) -> Scene {
    let cynefin = &config.cynefin;
    let style = CynefinStyle::from_config(config);
    let width = cynefin.width;
    let height = cynefin.height;
    let padding = cynefin.padding;
    let show_descriptions = cynefin.show_domain_descriptions;
    let total_width = width + padding * 2.0;
    let total_height = height + padding * 2.0;
    let seed = resolve_seed(cynefin.seed, id);

    let mut scene = Scene::new(Rect::new(0.0, 0.0, total_width, total_height));
    scene.bounds = Rect::new(padding, padding, width, height);

    let mut root = Group::translated(padding, padding);

    // 1. Domain background rectangles, in `quadrantDomains` order.
    let mut backgrounds = Group::new().with_class("cynefin-backgrounds");
    for name in QUADRANT_DOMAINS {
        let layout = domain_layout(name, width, height);
        let background_style = PathStyle {
            fill: Some(style.domain_bg(name)),
            fill_opacity: 0.4,
            ..PathStyle::default()
        };
        backgrounds.push(RectShape::new(
            Rect::new(layout.x, layout.y, layout.w, layout.h),
            0.0,
            background_style,
        ));
    }
    root.push(backgrounds);

    // 2. Wavy boundaries and 3. the cliff, all in one group.
    let boundary_style = PathStyle {
        fill: None,
        stroke: Some(style.boundary_color.clone()),
        stroke_width: style.boundary_width,
        dash: vec![6.0, 3.0],
        ..PathStyle::default()
    };
    let cliff_style = PathStyle {
        fill: None,
        stroke: Some(style.cliff_color.clone()),
        stroke_width: style.cliff_width,
        ..PathStyle::default()
    };
    let mut boundaries = Group::new().with_class("cynefin-boundaries");
    boundaries.push(PathShape::new(
        fold_path(width, height, seed, Some(cynefin.boundary_amplitude)),
        boundary_style.clone(),
    ));
    boundaries.push(PathShape::new(
        horizontal_boundary_path(
            width,
            height,
            seed + 100.0,
            Some(cynefin.boundary_amplitude),
        ),
        boundary_style,
    ));
    boundaries.push(PathShape::new(cliff_path(width, height), cliff_style));
    root.push(boundaries);

    // 4. Confusion ellipse at the centre.
    let confusion_rx = width * 0.15;
    let confusion_ry = height * 0.15;
    let confusion_style = PathStyle {
        fill: Some(style.confusion_bg.clone()),
        fill_opacity: 0.5,
        stroke: Some(style.boundary_color.clone()),
        stroke_width: 1.5,
        dash: vec![4.0, 2.0],
        ..PathStyle::default()
    };
    root.push(PathShape::new(
        confusion_path(width / 2.0, height / 2.0, confusion_rx, confusion_ry),
        confusion_style,
    ));

    // 5. Domain labels.
    let label_font = Font::new("monospace", style.domain_font_size).bold();
    let mut labels = Group::new().with_class("cynefin-labels");
    for name in QUADRANT_DOMAINS {
        let layout = domain_layout(name, width, height);
        let y = if show_descriptions {
            layout.cy - 30.0
        } else {
            layout.cy
        };
        labels.push(text_run(
            &capitalized(name),
            Point::new(layout.cx, y),
            TextAnchor::Middle,
            TextBaseline::Middle,
            &label_font,
            Some(&style.label_color),
            measure,
        ));
    }
    labels.push(text_run(
        "Confusion",
        Point::new(
            width / 2.0,
            if show_descriptions {
                height / 2.0 - 10.0
            } else {
                height / 2.0
            },
        ),
        TextAnchor::Middle,
        TextBaseline::Middle,
        &label_font,
        Some(&style.label_color),
        measure,
    ));
    root.push(labels);

    // 6. Domain description subtitles.
    if show_descriptions {
        let subtitle_font = Font::new("monospace", style.item_font_size - 1.0).italic();
        let mut subtitles = Group::new().with_class("cynefin-subtitles");
        for name in QUADRANT_DOMAINS {
            let layout = domain_layout(name, width, height);
            let meta = domain_meta(name);
            for (text, y) in [
                (meta.model, layout.cy - 10.0),
                (meta.practice, layout.cy + 5.0),
            ] {
                subtitles.push(text_run(
                    text,
                    Point::new(layout.cx, y),
                    TextAnchor::Middle,
                    TextBaseline::Middle,
                    &subtitle_font,
                    Some(&style.text_color),
                    measure,
                ));
            }
        }
        subtitles.push(text_run(
            domain_meta(DomainName::Confusion).practice,
            Point::new(width / 2.0, height / 2.0 + 8.0),
            TextAnchor::Middle,
            TextBaseline::Middle,
            &subtitle_font,
            Some(&style.text_color),
            measure,
        ));
        root.push(subtitles);
    }

    // 7. Item badges, one group per domain in `allDomains` order.
    let item_font = Font::new("monospace", style.item_font_size);
    let mut items_group = Group::new().with_class("cynefin-items");
    for name in ALL_DOMAINS {
        let Some(domain) = ast.domain(name) else {
            continue;
        };
        if domain.items.is_empty() {
            continue;
        }
        let layout = domain_layout(name, width, height);
        let is_confusion = name == DomainName::Confusion;

        let (items, overflow_count) = if is_confusion && domain.items.len() > MAX_CONFUSION_ITEMS {
            (
                &domain.items[..MAX_CONFUSION_ITEMS],
                domain.items.len() - MAX_CONFUSION_ITEMS,
            )
        } else {
            (&domain.items[..], 0)
        };

        let start_y = if is_confusion {
            let label_offset = if show_descriptions { 22.0 } else { 14.0 };
            layout.cy + label_offset
        } else {
            layout.cy + if show_descriptions { 25.0 } else { 15.0 }
        };

        for (index, item) in items.iter().enumerate() {
            let item_y = start_y + index as f64 * (ITEM_HEIGHT + 4.0);
            items_group.push(item_badge(
                &item.label,
                Point::new(layout.cx, item_y),
                style.domain_bg(name),
                false,
                &item_font,
                &style,
                measure,
            ));
        }

        if overflow_count > 0 {
            let overflow_y = start_y + items.len() as f64 * (ITEM_HEIGHT + 4.0);
            let overflow_label = format!("+{overflow_count} more");
            items_group.push(item_badge(
                &overflow_label,
                Point::new(layout.cx, overflow_y),
                style.domain_bg(name),
                true,
                &item_font,
                &style,
                measure,
            ));
        }
    }
    root.push(items_group);

    // 8. Transition arrows between domain centres.
    if !ast.transitions.is_empty() {
        let marker_id = format!("cynefin-arrow-{id}");
        scene.define(arrow_marker(&marker_id, &style.arrow_color));

        let mut arrows = Group::new().with_class("cynefin-arrows");
        for transition in &ast.transitions {
            if transition.from == transition.to {
                continue;
            }
            let from = domain_layout(transition.from, width, height);
            let to = domain_layout(transition.to, width, height);
            let (x1, y1) = (from.cx, from.cy);
            let (x2, y2) = (to.cx, to.cy);

            // Quadratic bezier with a perpendicular offset of 15% of the distance.
            let mx = (x1 + x2) / 2.0;
            let my = (y1 + y2) / 2.0;
            let dx = x2 - x1;
            let dy = y2 - y1;
            let len = (dx * dx + dy * dy).sqrt();
            let offset_amount = len * 0.15;
            let nx = -dy / len;
            let ny = dx / len;
            let cpx = mx + nx * offset_amount;
            let cpy = my + ny * offset_amount;

            let mut path = Path::new();
            path.move_to(Point::new(x1, y1))
                .quad_to(Point::new(cpx, cpy), Point::new(x2, y2));
            let arrow_style = PathStyle {
                fill: None,
                stroke: Some(style.arrow_color.clone()),
                stroke_width: style.arrow_width,
                ..PathStyle::default()
            };
            arrows.push(
                PathShape::new(path, arrow_style).with_end_marker(MarkerRef::new(&marker_id)),
            );

            if let Some(label) = transition
                .label
                .as_deref()
                .filter(|label| !label.is_empty())
            {
                let label_font = Font::new("monospace", style.item_font_size - 1.0);
                arrows.push(text_run(
                    label,
                    Point::new(cpx, cpy - 6.0),
                    TextAnchor::Middle,
                    TextBaseline::Alphabetic,
                    &label_font,
                    Some(&style.text_color),
                    measure,
                ));
            }
        }
        root.push(arrows);
    }

    // Title, in the padding above the canvas.
    if !ast.diagram_title.is_empty() {
        let title_font = Font::new("monospace", style.domain_font_size + 2.0).bold();
        root.push(text_run(
            &ast.diagram_title,
            Point::new(width / 2.0, -padding / 2.0),
            TextAnchor::Middle,
            TextBaseline::Middle,
            &title_font,
            Some(&style.label_color),
            measure,
        ));
    }

    scene.push(root);
    scene
}

/// Renders a cynefin body with the crate's monospace fallback measurement.
pub fn render(body: &str, config: &Config) -> Result<Scene, ParseError> {
    draw(body, config, &crate::measure::FallbackMeasure)
}

/// The four quadrant domains, in the order `draw` draws their backgrounds and labels.
const QUADRANT_DOMAINS: [DomainName; 4] = [
    DomainName::Complex,
    DomainName::Complicated,
    DomainName::Chaotic,
    DomainName::Clear,
];

/// The five domains, in the order `draw` draws their item badges.
const ALL_DOMAINS: [DomainName; 5] = [
    DomainName::Complex,
    DomainName::Complicated,
    DomainName::Chaotic,
    DomainName::Clear,
    DomainName::Confusion,
];

/// `DomainLayout`: a domain's centre and its rectangle.
#[derive(Clone, Copy)]
struct DomainLayout {
    cx: f64,
    cy: f64,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// `getDomainLayouts`: the five domain rectangles for a canvas.
fn domain_layout(name: DomainName, width: f64, height: f64) -> DomainLayout {
    let hw = width / 2.0;
    let hh = height / 2.0;
    match name {
        DomainName::Complex => DomainLayout {
            cx: hw / 2.0,
            cy: hh / 2.0,
            x: 0.0,
            y: 0.0,
            w: hw,
            h: hh,
        },
        DomainName::Complicated => DomainLayout {
            cx: hw + hw / 2.0,
            cy: hh / 2.0,
            x: hw,
            y: 0.0,
            w: hw,
            h: hh,
        },
        DomainName::Chaotic => DomainLayout {
            cx: hw / 2.0,
            cy: hh + hh / 2.0,
            x: 0.0,
            y: hh,
            w: hw,
            h: hh,
        },
        DomainName::Clear => DomainLayout {
            cx: hw + hw / 2.0,
            cy: hh + hh / 2.0,
            x: hw,
            y: hh,
            w: hw,
            h: hh,
        },
        DomainName::Confusion => DomainLayout {
            cx: hw,
            cy: hh,
            x: hw * 0.7,
            y: hh * 0.7,
            w: hw * 0.6,
            h: hh * 0.6,
        },
    }
}

/// `DOMAIN_META`: the decision model and practice type of a domain.
struct DomainMeta {
    model: &'static str,
    practice: &'static str,
}

fn domain_meta(name: DomainName) -> DomainMeta {
    match name {
        DomainName::Complex => DomainMeta {
            model: "Probe \u{2192} Sense \u{2192} Respond",
            practice: "Emergent Practices",
        },
        DomainName::Complicated => DomainMeta {
            model: "Sense \u{2192} Analyse \u{2192} Respond",
            practice: "Good Practices",
        },
        DomainName::Clear => DomainMeta {
            model: "Sense \u{2192} Categorise \u{2192} Respond",
            practice: "Best Practices",
        },
        DomainName::Chaotic => DomainMeta {
            model: "Act \u{2192} Sense \u{2192} Respond",
            practice: "Novel Practices",
        },
        DomainName::Confusion => DomainMeta {
            model: "",
            practice: "Disorder",
        },
    }
}

/// `domainName.charAt(0).toUpperCase() + domainName.slice(1)`.
fn capitalized(name: DomainName) -> String {
    let text = name.as_str();
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// The resolved cynefin theme block: colors, widths and font sizes from `styles.ts`.
struct CynefinStyle {
    domain_font_size: f64,
    item_font_size: f64,
    boundary_color: Color,
    boundary_width: f64,
    cliff_color: Color,
    cliff_width: f64,
    arrow_color: Color,
    arrow_width: f64,
    text_color: Color,
    label_color: Color,
    complex_bg: Color,
    complicated_bg: Color,
    chaotic_bg: Color,
    clear_bg: Color,
    confusion_bg: Color,
}

impl CynefinStyle {
    fn from_config(config: &Config) -> Self {
        let theme = &config.theme_variables;
        let cynefin = theme.get("cynefin").and_then(Value::as_object);
        let field = |key: &str| cynefin.and_then(|cynefin| cynefin.get(key));
        let color = |key: &str, fallback_key: &str, fallback: &str| {
            field(key)
                .and_then(Value::as_str)
                .or_else(|| theme.get(fallback_key).and_then(Value::as_str))
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

        Self {
            domain_font_size: number("domainFontSize", 16.0),
            item_font_size: number("itemFontSize", 12.0),
            boundary_color: color("boundaryColor", "lineColor", "#333333"),
            boundary_width: number("boundaryWidth", 2.0),
            cliff_color: color("cliffColor", "cliffColor", "#8B0000"),
            cliff_width: number("cliffWidth", 4.0),
            arrow_color: color("arrowColor", "lineColor", "#333333"),
            arrow_width: number("arrowWidth", 2.0),
            text_color: color("textColor", "textColor", "#333"),
            label_color: color("labelColor", "primaryTextColor", "#131300"),
            complex_bg: color("complexBg", "complexBg", "#E8F5E9"),
            complicated_bg: color("complicatedBg", "complicatedBg", "#E3F2FD"),
            chaotic_bg: color("chaoticBg", "chaoticBg", "#FBE9E7"),
            clear_bg: color("clearBg", "clearBg", "#FFF8E1"),
            confusion_bg: color("confusionBg", "confusionBg", "#F3E5F5"),
        }
    }

    /// `domainBg[domainName]`.
    fn domain_bg(&self, name: DomainName) -> Color {
        match name {
            DomainName::Complex => self.complex_bg.clone(),
            DomainName::Complicated => self.complicated_bg.clone(),
            DomainName::Chaotic => self.chaotic_bg.clone(),
            DomainName::Clear => self.clear_bg.clone(),
            DomainName::Confusion => self.confusion_bg.clone(),
        }
    }
}

/// One item badge: the `g` translated to the badge's corner, holding the rounded rectangle and
/// the centred label.
fn item_badge(
    label: &str,
    position: Point,
    background: Color,
    overflow: bool,
    font: &Font,
    style: &CynefinStyle,
    measure: &dyn TextMeasure,
) -> Group {
    let measured_width = measure.width(label, font);
    let measured_width = if measured_width > 0.0 {
        measured_width
    } else {
        // `item.label.length * 7`, a UTF-16 code unit per character.
        label.encode_utf16().count() as f64 * 7.0
    };
    let badge_width = measured_width + ITEM_PADDING_X * 2.0;
    let item_x = position.x - badge_width / 2.0;

    let badge_style = PathStyle {
        fill: Some(background),
        fill_opacity: if overflow { 0.6 } else { 0.95 },
        stroke: Some(style.boundary_color.clone()),
        stroke_width: 1.0,
        dash: if overflow { vec![3.0, 2.0] } else { Vec::new() },
        ..PathStyle::default()
    };

    let mut group = Group::translated(item_x, position.y);
    group.push(RectShape::new(
        Rect::new(0.0, 0.0, badge_width, ITEM_HEIGHT),
        4.0,
        badge_style,
    ));
    group.push(text_run(
        label,
        Point::new(badge_width / 2.0, ITEM_HEIGHT / 2.0),
        TextAnchor::Middle,
        TextBaseline::Central,
        font,
        Some(&style.text_color),
        measure,
    ));
    group
}

/// The `<marker>` of a transition arrow: `M 0 0 L 10 5 L 0 10 z` in the arrow color.
fn arrow_marker(id: &str, color: &Color) -> MarkerDef {
    let mut path = Path::new();
    path.move_to(Point::new(0.0, 0.0))
        .line_to(Point::new(10.0, 5.0))
        .line_to(Point::new(0.0, 10.0))
        .close();
    let mut marker = MarkerDef::new(id, Rect::new(0.0, 0.0, 10.0, 10.0))
        .with_reference(Point::new(9.0, 5.0))
        .with_size(6.0, 6.0)
        .with_orient(MarkerOrient::AutoStartReverse);
    marker.class = Some("cynefinArrowHead".to_string());
    marker
        .shapes
        .push(PathShape::new(path, PathStyle::filled(color.clone())));
    marker
}

/// A one-line text run at `position`, shaped and measured like `createFormattedText` with
/// `htmlLabels: false`.
fn text_run(
    text: &str,
    position: Point,
    anchor: TextAnchor,
    baseline: TextBaseline,
    font: &Font,
    fill: Option<&Color>,
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
    run.baseline = baseline;
    run.fill = fill.cloned();
    run
}

/// `seededRandom`: the mulberry32 generator, in 32-bit integer arithmetic.
pub(crate) fn seeded_random(seed: f64) -> f64 {
    // `(seed + 0x6d2b79f5) | 0` is ToInt32 of the sum.
    let mut t = ((seed + 0x6d2b79f5_u32 as f64) as i64 as u32) as i32;
    t = imul(t ^ (((t as u32) >> 15) as i32), t | 1);
    t ^= t.wrapping_add(imul(t ^ (((t as u32) >> 7) as i32), t | 61));
    ((t ^ (((t as u32) >> 14) as i32)) as u32) as f64 / 4294967296.0
}

/// JavaScript's `Math.imul(a, b)`: the low 32 bits of the product, as a signed integer.
fn imul(a: i32, b: i32) -> i32 {
    a.wrapping_mul(b)
}

/// `hashString`: the 32-bit string hash the boundary seed falls back to.
pub(crate) fn hash_string(text: &str) -> i32 {
    let mut hash: i32 = 0;
    for unit in text.encode_utf16() {
        hash = hash
            .wrapping_shl(5)
            .wrapping_sub(hash)
            .wrapping_add(unit as i32);
    }
    hash
}

/// `resolveSeed`: a finite non-zero configured seed, else the hash of the element id.
pub(crate) fn resolve_seed(configured_seed: f64, id: &str) -> f64 {
    if configured_seed.is_finite() && configured_seed != 0.0 {
        configured_seed
    } else {
        hash_string(id) as f64
    }
}

/// `generateFoldPath`: the vertical wavy line through the centre, seven cubic segments.
pub(crate) fn fold_path(
    width: f64,
    height: f64,
    seed: f64,
    amplitude_override: Option<f64>,
) -> Path {
    let cx = width / 2.0;
    let amplitude = amplitude_override.unwrap_or(width * 0.015);
    let seg_height = height / BOUNDARY_SEGMENTS as f64;
    let mut points = Vec::with_capacity(BOUNDARY_SEGMENTS as usize + 1);
    for i in 0..=BOUNDARY_SEGMENTS {
        let jitter = seeded_random(seed + i as f64 * 17.0) * amplitude * 2.0 - amplitude;
        points.push(Point::new(cx + jitter, i as f64 * seg_height));
    }

    let mut path = Path::new();
    path.move_to(points[0]);
    for i in 0..points.len() - 1 {
        let p0 = points[i];
        let p1 = points[i + 1];
        let mid_y = (p0.y + p1.y) / 2.0;
        let dir = if i % 2 == 0 { 1.0 } else { -1.0 };
        let offset = amplitude * 1.5 * dir * seeded_random(seed + i as f64 * 31.0 + 7.0);
        path.cubic_to(
            Point::new(p0.x + offset, mid_y),
            Point::new(p1.x - offset, mid_y),
            p1,
        );
    }
    path
}

/// `generateHorizontalBoundary`: the horizontal wavy line through the centre, seven cubic
/// segments.
pub(crate) fn horizontal_boundary_path(
    width: f64,
    height: f64,
    seed: f64,
    amplitude_override: Option<f64>,
) -> Path {
    let cy = height / 2.0;
    let amplitude = amplitude_override.unwrap_or(height * 0.015);
    let seg_width = width / BOUNDARY_SEGMENTS as f64;
    let mut points = Vec::with_capacity(BOUNDARY_SEGMENTS as usize + 1);
    for i in 0..=BOUNDARY_SEGMENTS {
        let jitter = seeded_random(seed + i as f64 * 23.0) * amplitude * 2.0 - amplitude;
        points.push(Point::new(i as f64 * seg_width, cy + jitter));
    }

    let mut path = Path::new();
    path.move_to(points[0]);
    for i in 0..points.len() - 1 {
        let p0 = points[i];
        let p1 = points[i + 1];
        let mid_x = (p0.x + p1.x) / 2.0;
        let dir = if i % 2 == 0 { 1.0 } else { -1.0 };
        let offset = amplitude * 1.5 * dir * seeded_random(seed + i as f64 * 37.0 + 11.0);
        path.cubic_to(
            Point::new(mid_x, p0.y + offset),
            Point::new(mid_x, p1.y - offset),
            p1,
        );
    }
    path
}

/// `generateCliffPath`: the steep S-curve between Clear and Chaotic.
pub(crate) fn cliff_path(width: f64, height: f64) -> Path {
    let cx = width / 2.0;
    let top_y = height * 0.5;
    let bottom_y = height;
    let amplitude = width * 0.03;
    let span = bottom_y - top_y;

    let mut path = Path::new();
    path.move_to(Point::new(cx, top_y))
        .cubic_to(
            Point::new(cx + amplitude, top_y + span * 0.2),
            Point::new(cx - amplitude * 1.5, top_y + span * 0.55),
            Point::new(cx + amplitude * 0.5, top_y + span * 0.75),
        )
        .cubic_to(
            Point::new(cx - amplitude, top_y + span * 0.85),
            Point::new(cx + amplitude * 0.3, top_y + span * 0.95),
            Point::new(cx, bottom_y),
        );
    path
}

/// `generateConfusionPath`: the ellipse drawn with two arcs.
pub(crate) fn confusion_path(cx: f64, cy: f64, rx: f64, ry: f64) -> Path {
    let mut path = Path::new();
    path.move_to(Point::new(cx - rx, cy))
        .arc_to(rx, ry, 0.0, true, true, Point::new(cx + rx, cy))
        .arc_to(rx, ry, 0.0, true, true, Point::new(cx - rx, cy))
        .close();
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Item;
    use serde_json::{Value, json};

    const TOLERANCE: f64 = 0.01;

    fn fixtures() -> Vec<(String, Value)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/diagrams/cynefin/tests/render");
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

    fn path<'a>(item: &'a Item, file: &str, what: &str) -> &'a PathShape {
        match item {
            Item::Path(path) => path,
            _ => panic!("{file}: {what} is not a path"),
        }
    }

    fn text<'a>(item: &'a Item, file: &str, what: &str) -> &'a TextRun {
        match item {
            Item::Text(text) => text,
            _ => panic!("{file}: {what} is not text"),
        }
    }

    fn rect<'a>(item: &'a Item, file: &str, what: &str) -> &'a RectShape {
        match item {
            Item::Rect(rect) => rect,
            _ => panic!("{file}: {what} is not a rect"),
        }
    }

    fn text_of(run: &TextRun) -> String {
        run.lines
            .iter()
            .map(|line| line.text())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn commands_json(path: &Path) -> Value {
        serde_json::to_value(&path.commands).expect("serialize commands")
    }

    fn color_json(color: Option<&Color>) -> Value {
        match color {
            Some(color) => Value::String(color.0.clone()),
            None => Value::Null,
        }
    }

    fn anchor_name(anchor: TextAnchor) -> &'static str {
        match anchor {
            TextAnchor::Start => "start",
            TextAnchor::Middle => "middle",
            TextAnchor::End => "end",
        }
    }

    fn baseline_name(baseline: TextBaseline) -> &'static str {
        match baseline {
            TextBaseline::Alphabetic => "auto",
            TextBaseline::Middle => "middle",
            TextBaseline::Central => "central",
            TextBaseline::Hanging => "hanging",
            TextBaseline::Top => "top",
            TextBaseline::Bottom => "bottom",
        }
    }

    fn text_json(run: &TextRun) -> Value {
        json!({
            "text": text_of(run),
            "x": run.position.x,
            "y": run.position.y,
            "anchor": anchor_name(run.anchor),
            "baseline": baseline_name(run.baseline),
            "fontSize": run.font.size,
            "bold": run.font.is_bold(),
            "italic": run.font.italic,
            "fill": color_json(run.fill.as_ref()),
        })
    }

    fn rect_json(shape: &RectShape) -> Value {
        json!({
            "x": shape.rect.x,
            "y": shape.rect.y,
            "width": shape.rect.width,
            "height": shape.rect.height,
            "rx": shape.rx,
            "ry": shape.ry,
            "fill": color_json(shape.style.fill.as_ref()),
            "fillOpacity": shape.style.fill_opacity,
            "stroke": color_json(shape.style.stroke.as_ref()),
            "strokeWidth": shape.style.stroke.as_ref().map(|_| shape.style.stroke_width),
            "dash": shape.style.dash,
        })
    }

    fn shape_json(shape: &PathShape) -> Value {
        let marker_end = shape
            .end_markers
            .first()
            .map(|marker| format!("url(#{})", marker.def));
        json!({
            "commands": commands_json(&shape.path),
            "fill": color_json(shape.style.fill.as_ref()),
            "fillOpacity": if shape.style.fill.is_some() { Some(shape.style.fill_opacity) } else { None },
            "stroke": color_json(shape.style.stroke.as_ref()),
            "strokeWidth": shape.style.stroke.as_ref().map(|_| shape.style.stroke_width),
            "dash": shape.style.dash,
            "markerEnd": marker_end,
        })
    }

    fn item_json(item: &Group) -> Value {
        let rect_item = item
            .items
            .first()
            .unwrap_or_else(|| panic!("item badge without a rect"));
        let text_item = item
            .items
            .get(1)
            .unwrap_or_else(|| panic!("item badge without text"));
        json!({
            "transform": {"x": item.transform.e, "y": item.transform.f},
            "rect": rect_json(rect(rect_item, "item", "rect")),
            "text": text_json(text(text_item, "item", "text")),
        })
    }

    fn marker_json(def: &MarkerDef) -> Value {
        let shape = def
            .shapes
            .first()
            .unwrap_or_else(|| panic!("marker without shapes"));
        json!({
            "viewBox": {
                "x": def.view_box.x,
                "y": def.view_box.y,
                "width": def.view_box.width,
                "height": def.view_box.height,
            },
            "refX": def.reference.x,
            "refY": def.reference.y,
            "markerWidth": def.width,
            "markerHeight": def.height,
            "orient": match def.orient {
                MarkerOrient::Auto => "auto",
                MarkerOrient::AutoStartReverse => "auto-start-reverse",
                MarkerOrient::Fixed { .. } => "fixed",
            },
            "fill": color_json(shape.style.fill.as_ref()),
            "stroke": color_json(shape.style.stroke.as_ref()),
            "commands": commands_json(&shape.path),
        })
    }

    /// The fixture shape of a rendered scene, keyed like the generator's `expected` object.
    fn summary(scene: &Scene, ast: &Ast, config: &Config) -> Value {
        let file = "scene";
        let root = group(&scene.items[0], file, "root");
        let backgrounds = group(&root.items[0], file, "backgrounds");
        let boundaries = group(&root.items[1], file, "boundaries");
        let confusion = path(&root.items[2], file, "confusion");
        let labels = group(&root.items[3], file, "labels");

        let mut cursor = 4;
        let subtitles = if config.cynefin.show_domain_descriptions {
            let group = group(&root.items[cursor], file, "subtitles");
            cursor += 1;
            Value::Array(
                group
                    .items
                    .iter()
                    .map(|item| text_json(text(item, file, "subtitle")))
                    .collect(),
            )
        } else {
            Value::Array(Vec::new())
        };
        let items = group(&root.items[cursor], file, "items");
        cursor += 1;

        let mut arrows = Vec::new();
        if !ast.transitions.is_empty() {
            let group = group(&root.items[cursor], file, "arrows");
            cursor += 1;
            for item in &group.items {
                match item {
                    Item::Path(path) => {
                        let mut value = shape_json(path);
                        value["label"] = Value::Null;
                        arrows.push(value);
                    }
                    Item::Text(run) => {
                        if let Some(last) = arrows.last_mut() {
                            last["label"] = text_json(run);
                        }
                    }
                    _ => panic!("{file}: unexpected arrow item"),
                }
            }
        }

        let title = root.items[cursor..]
            .iter()
            .find(|item| matches!(item, Item::Text(_)))
            .map(|item| text_json(text(item, file, "title")))
            .unwrap_or(Value::Null);

        json!({
            "viewBox": {
                "x": scene.view_box.x,
                "y": scene.view_box.y,
                "width": scene.view_box.width,
                "height": scene.view_box.height,
            },
            "rootTransform": {"x": root.transform.e, "y": root.transform.f},
            "backgrounds": backgrounds.items.iter().map(|item| rect_json(rect(item, file, "background"))).collect::<Vec<_>>(),
            "boundaries": boundaries.items.iter().map(|item| shape_json(path(item, file, "boundary"))).collect::<Vec<_>>(),
            "confusion": shape_json(confusion),
            "labels": labels.items.iter().map(|item| text_json(text(item, file, "label"))).collect::<Vec<_>>(),
            "subtitles": subtitles,
            "items": items.items.iter().map(|item| item_json(group(item, file, "item"))).collect::<Vec<_>>(),
            "arrows": arrows,
            "arrowHead": scene.defs.first().map(marker_json).unwrap_or(Value::Null),
            "title": title,
        })
    }

    #[test]
    fn renders_recorded_cynefin_fixtures() {
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
            let mut expected = fixture["expected"].clone();
            if let Value::Object(map) = &mut expected {
                map.remove("rawViewBox");
            }
            let actual = summary(&scene, &ast, &config);
            close_value(&actual, &expected, "expected", &file);
        }
    }

    #[test]
    fn render_diagram_dispatches_cynefin() {
        let scene = crate::render_diagram(
            "cynefin-beta\n  title Map\n  complex\n    \"A\"\n",
            &Config::default(),
        )
        .expect("cynefin renders");
        assert_eq!(scene.view_box, Rect::new(0.0, 0.0, 880.0, 680.0));
        let root = group(&scene.items[0], "dispatch", "root");
        assert_eq!(root.transform.e, 40.0);
        assert_eq!(root.transform.f, 40.0);
    }

    // Ported from `cynefin.spec.ts`'s `Cynefin Boundaries` and `resolveSeed` describes.

    #[test]
    fn seeded_random_is_deterministic_and_in_unit_interval() {
        assert_eq!(seeded_random(42.0), seeded_random(42.0));
        for seed in [0.0, 1.0, 100.0, 999999.0, -50.0] {
            let value = seeded_random(seed);
            assert!((0.0..1.0).contains(&value), "seed {seed}: {value}");
        }
    }

    #[test]
    fn hash_string_is_consistent_and_distinguishes_names() {
        assert_eq!(hash_string("cynefin"), hash_string("cynefin"));
        assert_ne!(hash_string("complex"), hash_string("chaotic"));
    }

    #[test]
    fn boundary_paths_are_deterministic_and_use_cubics() {
        let fold = fold_path(800.0, 600.0, 42.0, None);
        assert_eq!(fold, fold_path(800.0, 600.0, 42.0, None));
        assert!(matches!(
            fold.commands[0],
            crate::scene::PathCommand::MoveTo { .. }
        ));
        assert!(
            fold.commands
                .iter()
                .any(|command| matches!(command, crate::scene::PathCommand::CubicTo { .. }))
        );

        let horizontal = horizontal_boundary_path(800.0, 600.0, 42.0, None);
        assert!(matches!(
            horizontal.commands[0],
            crate::scene::PathCommand::MoveTo { .. }
        ));
        assert!(
            horizontal
                .commands
                .iter()
                .any(|command| matches!(command, crate::scene::PathCommand::CubicTo { .. }))
        );

        let cliff = cliff_path(800.0, 600.0);
        assert!(matches!(
            cliff.commands[0],
            crate::scene::PathCommand::MoveTo { .. }
        ));
        assert!(
            cliff
                .commands
                .iter()
                .any(|command| matches!(command, crate::scene::PathCommand::CubicTo { .. }))
        );
    }

    #[test]
    fn confusion_path_is_two_arcs_from_the_center_and_radii() {
        let path = confusion_path(400.0, 300.0, 50.0, 40.0);
        match path.commands[0] {
            crate::scene::PathCommand::MoveTo { x, y } => {
                assert_eq!((x, y), (350.0, 300.0));
            }
            _ => panic!("confusion path must start with a move"),
        }
        match path.commands[1] {
            crate::scene::PathCommand::ArcTo { rx, ry, x, .. } => {
                assert_eq!((rx, ry, x), (50.0, 40.0, 450.0));
            }
            _ => panic!("confusion path must use arcs"),
        }
        assert!(matches!(
            path.commands.last(),
            Some(crate::scene::PathCommand::Close)
        ));
    }

    #[test]
    fn resolve_seed_prefers_a_finite_non_zero_configuration() {
        assert_eq!(resolve_seed(42.0, "mermaid-1"), 42.0);
        assert_eq!(resolve_seed(-3.0, "mermaid-2"), -3.0);
        assert_eq!(
            resolve_seed(0.0, "mermaid-1"),
            hash_string("mermaid-1") as f64
        );
        assert_eq!(resolve_seed(7.0, "a"), resolve_seed(7.0, "b"));
        assert_ne!(
            resolve_seed(0.0, "mermaid-1"),
            resolve_seed(0.0, "mermaid-2")
        );
        assert_eq!(
            resolve_seed(f64::NAN, "mermaid-1"),
            hash_string("mermaid-1") as f64
        );
        assert_eq!(
            resolve_seed(f64::INFINITY, "mermaid-1"),
            hash_string("mermaid-1") as f64
        );
    }
}
