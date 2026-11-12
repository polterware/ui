//! Source-to-[`Scene`] rendering for treemap diagrams: the Rust side of
//! `diagrams/treemap/renderer.ts` `draw`.
//!
//! A treemap does not go through the unified layout pipeline: `draw` uses plain d3 and the
//! d3-hierarchy port in [`crate::layout::hierarchy`]. It reads the tree from
//! [`super::Ast::root`], lays it out with `hierarchy().sum(...).sort(...)` and
//! `treemap().size(...).padding*(...).round(true)` (squarify tiling), then draws the title, one
//! group per branch ("section": a header rect, the body rect, a clipped label, and a clipped
//! value) and one group per leaf (a rect, a clipped label, and a clipped value), and fits the
//! viewport with `setupViewPortForSVG(svg, diagramPadding, 'flowchart', useMaxWidth)`.
//!
//! Look: `diagrams/treemap/styles.ts` and the attributes `draw` writes. The design system has
//! no treemap control, so the look is mermaid's own.
//!
//! Behavior: `renderer.ts` is the only consumer of d3-hierarchy in mermaid and gpui-kit has no
//! treemap renderer, so there is no second behavior reference: the renderer is the model,
//! followed call by call, including the two text-fitting loops that shrink or hide a leaf's
//! label and value and the section label's character-by-character truncation.
//!
//! Where the port stops short of upstream, the difference is in the painter-facing scene:
//!
//! * The class names `treemapSectionHeader`, `treemapLeaf`, `treemapLabel`, ... matter only to
//!   CSS, and a [`Scene`] carries paints rather than selectors, so `draw` resolves the compiled
//!   class declarations (`styles2String`) into the rectangles' fill, stroke, opacity and the
//!   runs' font and fill. The group classes (`treemapSection`, `treemapLeafGroup leaf5x`) are
//!   kept for a painter that themes by class.
//! * SVG `display: none` items paint nothing and have no box, so they are left out instead of
//!   being emitted hidden: the root section's header, body, label and value, a leaf label that
//!   does not fit, and a value whose column has no visible label. Empty `<text>` elements are
//!   left out as well.
//! * The section and leaf texts are clipped by upstream's `<clipPath>` rectangles. GPUI clips
//!   to rectangles, so the clip is a [`Group::clip`] around the run: `(0, 0, w - 12, 25)` for a
//!   section and `(0, 0, w - 4, h - 4)` for a leaf.
//! * `styles.ts` sets the title's fill and `font-size: 14px`; `draw` writes no inline style on
//!   it, so the computed values come from that stylesheet (the fixture harness applies the
//!   same 14px because jsdom does not cascade the injected `<style>`).
//! * `useMaxWidth` sizes the `<svg>` element, not the drawing; the scene carries the fitted
//!   viewBox and the painter sizes the page.
//! * The value format is d3-format's. This module ports the specifiers the config default and
//!   the currency branches use (`,`, `.Nf`, `d`, `g`, `%`, `e`, the default `.12~g`, and the
//!   `$` prefixes); a specifier outside them falls back to d3's default `,` formatting, which
//!   is what upstream's `catch` does when `format` throws.
//!
//! Text is measured in the `monospace` family, the rule the fixture harness renders with and
//! that [`FallbackMeasure`](crate::measure::FallbackMeasure) reproduces, even where a class sets
//! another family; the family is still carried into the run for the painter.

use crate::config::Config;
use crate::error::ParseError;
use crate::layout::hierarchy::treemap::{Padding, Treemap};
use crate::layout::hierarchy::{Hierarchy, Node, NodeId};
use crate::measure::{FallbackMeasure, Font, LINE_HEIGHT_EM, TextMeasure};
use crate::render::pipeline::setup_view_port_for_svg::setup_view_port_for_svg;
use crate::scene::{
    Color, Group, Item, PathStyle, Point, Rect, RectShape, Scene, Size, TextAnchor, TextBaseline,
    TextBlock, TextLine, TextRun, TextSpan, Transform,
};

use super::Ast;
use super::ast::TreemapNode;

/// `draw`'s `SECTION_INNER_PADDING`: the inner padding of a section and the unit `nodeWidth`
/// and `nodeHeight` are multiplied by.
const SECTION_INNER_PADDING: f64 = 10.0;
/// `draw`'s `SECTION_HEADER_HEIGHT`.
const SECTION_HEADER_HEIGHT: f64 = 25.0;
/// `draw`'s `titleHeight` when the diagram has a title.
const TITLE_HEIGHT: f64 = 30.0;
/// `draw`'s fallback width when `nodeWidth` is falsy.
const DEFAULT_WIDTH: f64 = 960.0;
/// `draw`'s fallback height when `nodeHeight` is falsy.
const DEFAULT_HEIGHT: f64 = 500.0;
/// `.treemapTitle`'s `font-size` from `styles.ts`.
const TITLE_FONT_SIZE: f64 = 14.0;
/// The section label's inline `font-size`.
const SECTION_LABEL_FONT_SIZE: f64 = 12.0;
/// The section value's inline `font-size`.
const SECTION_VALUE_FONT_SIZE: f64 = 10.0;
/// The label's base size before the fit loop; `isComplexTreemap` picks the second.
const LEAF_LABEL_FONT_SIZES: [f64; 2] = [38.0, 16.0];
/// The value's base size before the fit loop; `isComplexTreemap` picks the second.
const LEAF_VALUE_FONT_SIZES: [f64; 2] = [28.0, 14.0];
/// The label's floor size; `isComplexTreemap` picks the second.
const LEAF_MIN_LABEL_FONT_SIZES: [f64; 2] = [8.0, 4.0];
/// The value's floor size; `isComplexTreemap` picks the second.
const LEAF_MIN_VALUE_FONT_SIZES: [f64; 2] = [6.0, 4.0];
/// The label padding; `isComplexTreemap` picks the second.
const LEAF_LABEL_PADDINGS: [f64; 2] = [4.0, 2.0];
/// The minimum available width and height to display a leaf label; complex picks the second.
const LEAF_MIN_DISPLAY_THRESHOLDS: [f64; 2] = [10.0, 8.0];
/// The gap between a leaf label and its value; complex picks the second.
const LEAF_LABEL_VALUE_SPACINGS: [f64; 2] = [2.0, 1.0];
/// A leaf's value font size as a factor of its label font size.
const VALUE_SCALE_FACTOR: f64 = 0.6;
/// The bottom padding the leaf value's fit check keeps.
const LEAF_VALUE_BOTTOM_PADDING: f64 = 4.0;

/// Renders a treemap body (the text [`super::parse`] accepts) to a [`Scene`].
///
/// `config` is the diagram configuration after the source's frontmatter and directives have
/// been applied, as `getConfig()` returns it to `draw`.
pub fn draw(body: &str, config: &Config, measure: &dyn TextMeasure) -> Result<Scene, ParseError> {
    let ast = super::parse(body)?;
    Ok(draw_ast(&ast, config, measure))
}

/// Renders an already parsed treemap database.
pub fn draw_ast(ast: &Ast, config: &Config, measure: &dyn TextMeasure) -> Scene {
    let treemap_config = &config.treemap;
    let inner_padding = treemap_config.padding;
    let title = ast.diagram_title.clone();
    let title_height = if title.is_empty() { 0.0 } else { TITLE_HEIGHT };
    let width = if is_truthy(treemap_config.node_width) {
        treemap_config.node_width * SECTION_INNER_PADDING
    } else {
        DEFAULT_WIDTH
    };
    let height = if is_truthy(treemap_config.node_height) {
        treemap_config.node_height * SECTION_INNER_PADDING
    } else {
        DEFAULT_HEIGHT
    };

    let mut hierarchy = Hierarchy::build(ast.root.clone(), |node: &TreemapNode| {
        node.children
            .clone()
            .filter(|children| !children.is_empty())
    });
    hierarchy.sum(|node| node.value);
    // `sort((a, b) => (b.value ?? 0) - (a.value ?? 0))`: a NaN comparison sorts as equal.
    hierarchy.sort_by(|a, b| {
        let a = a.value.unwrap_or(0.0);
        let b = b.value.unwrap_or(0.0);
        b.partial_cmp(&a).unwrap_or(std::cmp::Ordering::Equal)
    });

    let layout = Treemap::new()
        .size(width, height)
        .padding_top(Padding::function(|node: &Node<TreemapNode>| {
            if is_branch(node) {
                SECTION_HEADER_HEIGHT + SECTION_INNER_PADDING
            } else {
                0.0
            }
        }))
        .padding_inner(Padding::constant(inner_padding))
        .padding_left(Padding::function(|node: &Node<TreemapNode>| {
            if is_branch(node) {
                SECTION_INNER_PADDING
            } else {
                0.0
            }
        }))
        .padding_right(Padding::function(|node: &Node<TreemapNode>| {
            if is_branch(node) {
                SECTION_INNER_PADDING
            } else {
                0.0
            }
        }))
        .padding_bottom(Padding::function(|node: &Node<TreemapNode>| {
            if is_branch(node) {
                SECTION_INNER_PADDING
            } else {
                0.0
            }
        }))
        .round(true);
    layout.layout(&mut hierarchy);

    let theme = &config.theme_variables;
    let theme_value = |key: &str| {
        theme
            .get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let mut fill_scale = OrdinalScale::new(
        std::iter::once("transparent".to_string())
            .chain((0..12).map(|index| theme_value(&format!("cScale{index}"))))
            .collect(),
    );
    let mut peer_scale = OrdinalScale::new(
        std::iter::once("transparent".to_string())
            .chain((0..12).map(|index| theme_value(&format!("cScalePeer{index}"))))
            .collect(),
    );
    let mut label_scale = OrdinalScale::new(
        (0..12)
            .map(|index| theme_value(&format!("cScaleLabel{index}")))
            .collect(),
    );

    let format = ValueFormat::from_config(&treemap_config.value_format);
    let branches: Vec<NodeId> = hierarchy
        .descendants()
        .into_iter()
        .filter(|&id| is_branch(hierarchy.node(id)))
        .collect();
    let leaves = hierarchy.leaves();

    // `draw` assigns the section fills, then the section strokes, then the leaf fills (and
    // their strokes, which reuse the fill color), each in element order. Equal names share a
    // range entry; the `transparent` first entry is why the root's name gets index 0.
    let section_fills: Vec<String> = branches
        .iter()
        .map(|&id| fill_scale.get(&hierarchy.node(id).data.name))
        .collect();
    let section_strokes: Vec<String> = branches
        .iter()
        .map(|&id| peer_scale.get(&hierarchy.node(id).data.name))
        .collect();
    let leaf_fills: Vec<String> = leaves
        .iter()
        .map(|&id| {
            let node = hierarchy.node(id);
            let key = match node.parent {
                Some(parent) => hierarchy.node(parent).data.name.clone(),
                None => node.data.name.clone(),
            };
            fill_scale.get(&key)
        })
        .collect();

    let mut scene = Scene::new(Rect::ZERO);

    if let Some(run) = text_run(
        &title,
        Point::new(width / 2.0, title_height / 2.0),
        TextAnchor::Middle,
        TextBaseline::Middle,
        TITLE_FONT_SIZE,
        400,
        false,
        "monospace",
        Some(theme_value("titleColor")),
        measure,
    ) {
        scene.push(run);
    }

    let mut container = Group::translated(0.0, title_height).with_class("treemapContainer");
    let is_complex = leaves.len() > 20;
    let complex_index = usize::from(is_complex);
    let show_values = treemap_config.show_values;

    for (index, &id) in branches.iter().enumerate() {
        let node = hierarchy.node(id);
        // The root section's header, body, label and value are all `display: none`.
        if node.depth == 0 {
            continue;
        }
        let section_width = node.x1 - node.x0;
        let section_height = node.y1 - node.y0;
        let mut group = Group::translated(node.x0, node.y0).with_class("treemapSection");
        group.push(RectShape::new(
            Rect::new(0.0, 0.0, section_width, SECTION_HEADER_HEIGHT),
            0.0,
            PathStyle {
                fill: None,
                fill_opacity: 0.6,
                stroke: None,
                stroke_width: 0.6,
                ..PathStyle::default()
            },
        ));

        let styles = node.data.css_compiled_styles.as_deref().unwrap_or(&[]);
        let mut body_style = PathStyle {
            fill: Some(Color::new(section_fills[index].clone())),
            fill_opacity: 0.6,
            stroke: Some(Color::new(section_strokes[index].clone())),
            stroke_opacity: 0.4,
            stroke_width: 2.0,
            ..PathStyle::default()
        };
        let body_visible = apply_node_styles(styles, &mut body_style);
        if body_visible {
            group.push(RectShape::new(
                Rect::new(0.0, 0.0, section_width, section_height),
                0.0,
                body_style,
            ));
        }

        let label_style = label_class_style(styles);
        let label_color = label_scale.get(&node.data.name);
        let label_size = label_style.font_size.unwrap_or(SECTION_LABEL_FONT_SIZE);
        let label_fill = label_style.fill.clone().unwrap_or(label_color);
        let value = node.value.unwrap_or(0.0);
        let value_text = if is_truthy(value) {
            format.render(value)
        } else {
            String::new()
        };
        let label_available = if show_values && is_truthy(value) {
            section_width - 10.0 - 30.0 - 10.0 - 6.0
        } else {
            section_width - 6.0 - 6.0
        };
        let label_available = label_available.max(15.0);
        let label_text =
            truncate_section_label(&node.data.name, label_available, label_size, measure);

        let mut clip = Group::new().with_clip(Rect::new(
            0.0,
            0.0,
            (section_width - 12.0).max(0.0),
            SECTION_HEADER_HEIGHT,
        ));
        if let Some(run) = text_run(
            &label_text,
            Point::new(6.0, SECTION_HEADER_HEIGHT / 2.0),
            TextAnchor::Start,
            TextBaseline::Middle,
            label_size,
            label_style.font_weight.unwrap_or(700),
            label_style.italic.unwrap_or(false),
            label_style.family.as_deref().unwrap_or("monospace"),
            Some(label_fill),
            measure,
        ) {
            clip.push(run);
        }

        if show_values {
            let value_style = &label_style;
            let value_size = value_style.font_size.unwrap_or(SECTION_VALUE_FONT_SIZE);
            let value_fill = value_style
                .fill
                .clone()
                .unwrap_or_else(|| label_scale.get(&node.data.name));
            if let Some(run) = text_run(
                &value_text,
                Point::new(section_width - 10.0, SECTION_HEADER_HEIGHT / 2.0),
                TextAnchor::End,
                TextBaseline::Middle,
                value_size,
                value_style.font_weight.unwrap_or(400),
                value_style.italic.unwrap_or(true),
                value_style.family.as_deref().unwrap_or("monospace"),
                Some(value_fill),
                measure,
            ) {
                clip.push(run);
            }
        }

        if !clip.items.is_empty() {
            group.push(clip);
        }
        container.push(group);
    }

    let label_base = LEAF_LABEL_FONT_SIZES[complex_index];
    let value_base = LEAF_VALUE_FONT_SIZES[complex_index];
    let min_label = LEAF_MIN_LABEL_FONT_SIZES[complex_index];
    let min_value = LEAF_MIN_VALUE_FONT_SIZES[complex_index];
    let label_padding = LEAF_LABEL_PADDINGS[complex_index];
    let min_display = LEAF_MIN_DISPLAY_THRESHOLDS[complex_index];
    let spacing = LEAF_LABEL_VALUE_SPACINGS[complex_index];

    for (index, &id) in leaves.iter().enumerate() {
        let node = hierarchy.node(id);
        let leaf_width = node.x1 - node.x0;
        let leaf_height = node.y1 - node.y0;
        let styles = node.data.css_compiled_styles.as_deref().unwrap_or(&[]);
        let color = leaf_fills[index].clone();

        let mut group = Group::translated(node.x0, node.y0)
            .with_class(leaf_class(index, node.data.class_selector.as_deref()));
        let mut rect_style = PathStyle {
            fill: Some(Color::new(color.clone())),
            fill_opacity: 0.3,
            stroke: Some(Color::new(color)),
            stroke_width: 3.0,
            ..PathStyle::default()
        };
        let rect_visible = apply_node_styles(styles, &mut rect_style);
        if rect_visible {
            group.push(RectShape::new(
                Rect::new(0.0, 0.0, leaf_width, leaf_height),
                0.0,
                rect_style,
            ));
        }

        let label_style = label_class_style(styles);
        let label_color = label_scale.get(&node.data.name);
        let label_fill = label_style.fill.clone().unwrap_or(label_color);
        let mut current_label_size = match label_style.font_size {
            Some(size) => size.trunc(),
            None => label_base,
        };
        let locked_label_size = label_style.font_size.is_some();
        // A class `font-size` is `!important`, so every `style.fontSize` the fit loop writes
        // loses to it, while the loop's variable still counts down.
        let mut applied_label_size = label_style.font_size.unwrap_or(current_label_size);

        let available_width = leaf_width - 2.0 * label_padding;
        let available_height = leaf_height - 2.0 * label_padding;
        let mut label_visible = true;
        if available_width < min_display || available_height < min_display {
            label_visible = false;
        } else {
            // 1. Shrink the label until it fits the width.
            while measure.width(&node.data.name, &label_font(applied_label_size)) > available_width
                && current_label_size > min_label
            {
                current_label_size -= 1.0;
                if !locked_label_size {
                    applied_label_size = current_label_size;
                }
            }

            // 2. Shrink both the label and the prospective value until their combined height
            // fits.
            let mut prospective = prospective_value_size(current_label_size, value_base, min_value);
            let mut combined = current_label_size + spacing + prospective;
            while combined > available_height && current_label_size > min_label {
                current_label_size -= 1.0;
                prospective = prospective_value_size(current_label_size, value_base, min_value);
                if prospective < min_value && current_label_size == min_label {
                    break;
                }
                if !locked_label_size {
                    applied_label_size = current_label_size;
                }
                combined = current_label_size + spacing + prospective;
            }
            if !locked_label_size {
                applied_label_size = current_label_size;
            }

            // 3. The final visibility check.
            if is_complex {
                if current_label_size < min_label || available_height < min_label {
                    label_visible = false;
                }
            } else {
                let too_wide = measure.width(&node.data.name, &label_font(applied_label_size))
                    > available_width;
                if too_wide
                    || current_label_size < min_label
                    || available_height < current_label_size
                {
                    label_visible = false;
                }
            }
        }

        let mut clip = Group::new().with_clip(Rect::new(
            0.0,
            0.0,
            (leaf_width - 4.0).max(0.0),
            (leaf_height - 4.0).max(0.0),
        ));
        let label_run = if label_visible {
            text_run(
                &node.data.name,
                Point::new(leaf_width / 2.0, leaf_height / 2.0),
                TextAnchor::Middle,
                TextBaseline::Middle,
                applied_label_size,
                label_style.font_weight.unwrap_or(400),
                label_style.italic.unwrap_or(false),
                label_style.family.as_deref().unwrap_or("monospace"),
                Some(label_fill.clone()),
                measure,
            )
        } else {
            None
        };
        if let Some(run) = label_run {
            clip.push(run);
        }

        if show_values && label_visible {
            let value = node.value.unwrap_or(0.0);
            let value_text = if is_truthy(value) {
                format.render(value)
            } else {
                String::new()
            };
            let value_style = label_class_style(styles);
            let actual_value_size =
                prospective_value_size(applied_label_size, value_base, min_value);
            let applied_value_size = value_style.font_size.unwrap_or(actual_value_size);
            let label_center_y = leaf_height / 2.0;
            let value_top = label_center_y + applied_label_size / 2.0 + spacing;
            let available_width = leaf_width - 2.0 * label_padding;
            let max_value_bottom = leaf_height - LEAF_VALUE_BOTTOM_PADDING;
            let too_wide =
                measure.width(&value_text, &label_font(applied_value_size)) > available_width;
            let too_tall = value_top + actual_value_size > max_value_bottom;
            let too_small = actual_value_size < min_value;
            if !too_wide && !too_tall && !too_small {
                let value_fill = value_style
                    .fill
                    .clone()
                    .unwrap_or_else(|| label_scale.get(&node.data.name));
                if let Some(run) = text_run(
                    &value_text,
                    Point::new(leaf_width / 2.0, value_top),
                    TextAnchor::Middle,
                    TextBaseline::Hanging,
                    applied_value_size,
                    value_style.font_weight.unwrap_or(400),
                    value_style.italic.unwrap_or(false),
                    value_style.family.as_deref().unwrap_or("monospace"),
                    Some(value_fill),
                    measure,
                ) {
                    clip.push(run);
                }
            }
        }

        if !clip.items.is_empty() {
            group.push(clip);
        }
        container.push(group);
    }

    scene.push(container);

    if let Some(bounds) = scene_bounds(&scene.items) {
        scene.bounds = bounds;
    }
    let padding = treemap_config.diagram_padding;
    setup_view_port_for_svg(
        &mut scene,
        padding,
        "flowchart",
        treemap_config.use_max_width,
    );
    scene
}

/// Renders a treemap body with the crate's monospace fallback measurement.
pub fn render(body: &str, config: &Config) -> Result<Scene, ParseError> {
    draw(body, config, &FallbackMeasure)
}

/// Upstream's truthiness for the numeric config and values (`0`, `-0`, and `NaN` are falsy).
fn is_truthy(value: f64) -> bool {
    value != 0.0 && !value.is_nan()
}

/// `d.children && d.children.length > 0`.
fn is_branch(node: &Node<TreemapNode>) -> bool {
    node.children
        .as_ref()
        .is_some_and(|children| !children.is_empty())
}

/// `Math.round(value * 0.6)` clamped to the value range, `Math.max(min, Math.min(base, round))`.
fn prospective_value_size(label_size: f64, base: f64, min: f64) -> f64 {
    let scaled = (label_size * VALUE_SCALE_FACTOR + 0.5).floor();
    scaled.clamp(min, base)
}

/// The class `draw` writes for a leaf group: `leaf{i}{ classSelector }x`.
fn leaf_class(index: usize, class_selector: Option<&str>) -> String {
    match class_selector {
        Some(selector) => format!("treemapNode treemapLeafGroup leaf{index} {selector}x"),
        None => format!("treemapNode treemapLeafGroup leaf{index}x"),
    }
}

/// The monospace font the fit loops measure with.
fn label_font(size: f64) -> Font {
    Font::new("monospace", size)
}

/// A one-line text run, measured like the harness's browser-shaped `getBBox`: the monospace
/// advance of the text and a `1.15 × size` ink height, independent of weight and slant.
#[allow(clippy::too_many_arguments)]
fn text_run(
    text: &str,
    position: Point,
    anchor: TextAnchor,
    baseline: TextBaseline,
    size: f64,
    weight: u16,
    italic: bool,
    family: &str,
    fill: Option<String>,
    measure: &dyn TextMeasure,
) -> Option<TextRun> {
    if text.is_empty() {
        return None;
    }
    let measure_font = label_font(size);
    let width = measure.width(text, &measure_font);
    let height = measure.line_height(&measure_font);
    let block = TextBlock::new(
        vec![TextLine::new(vec![TextSpan::plain(text)])],
        Size::new(width, height),
        LINE_HEIGHT_EM * size,
    );
    let font = Font {
        family: family.to_string(),
        size,
        weight,
        italic,
    };
    let mut run = TextRun::new(position, font, block);
    run.anchor = anchor;
    run.baseline = baseline;
    run.fill = fill.map(Color::new);
    Some(run)
}

/// The section label's truncation loop: keep the longest prefix whose `prefix...` fits, or
/// `...`, or nothing, exactly as `draw`'s `each` walks the length down.
fn truncate_section_label(
    text: &str,
    available: f64,
    size: f64,
    measure: &dyn TextMeasure,
) -> String {
    let font = label_font(size);
    if measure.width(text, &font) <= available {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut length = chars.len();
    while length > 0 {
        length -= 1;
        if length == 0 {
            let ellipsis = "...";
            return if measure.width(ellipsis, &font) > available {
                String::new()
            } else {
                ellipsis.to_string()
            };
        }
        let candidate = format!("{}...", chars[..length].iter().collect::<String>());
        if measure.width(&candidate, &font) <= available {
            return candidate;
        }
    }
    String::new()
}

/// The declarations of `styles2String`, after `compileStyles`' map has collapsed duplicate
/// keys: `style.split(':')` keeps only the second piece, and the last declaration wins.
fn style_map(styles: &[String]) -> Vec<(String, String)> {
    let mut map: Vec<(String, String)> = Vec::new();
    for style in styles {
        let mut parts = style.split(':');
        let key = parts.next().unwrap_or_default().trim().to_string();
        let value = parts.next().unwrap_or_default().trim().to_string();
        match map.iter_mut().find(|(existing, _)| *existing == key) {
            Some(entry) => entry.1 = value,
            None => map.push((key, value)),
        }
    }
    map
}

/// `isLabelStyle` from `handDrawnShapeStyles.ts`, applied to a declaration's key.
fn is_label_style(key: &str) -> bool {
    matches!(
        key,
        "color"
            | "font-size"
            | "font-family"
            | "font-weight"
            | "font-style"
            | "text-decoration"
            | "text-align"
            | "text-transform"
            | "line-height"
            | "letter-spacing"
            | "word-spacing"
            | "text-shadow"
            | "text-overflow"
            | "white-space"
            | "word-wrap"
            | "word-break"
            | "overflow-wrap"
            | "hyphens"
    )
}

/// The label declarations a compiled class sets on a text run. Every declaration is
/// `!important` inline, so it overrides the run's own inline `fill` and `font-size`.
#[derive(Clone, Debug, Default)]
struct LabelStyle {
    fill: Option<String>,
    font_size: Option<f64>,
    font_weight: Option<u16>,
    italic: Option<bool>,
    family: Option<String>,
}

/// `styles2String(...).labelStyles`, with `color:` turned into the run's fill.
fn label_class_style(styles: &[String]) -> LabelStyle {
    let mut label = LabelStyle::default();
    for (key, value) in style_map(styles) {
        if !is_label_style(&key) {
            continue;
        }
        match key.as_str() {
            "color" => label.fill = Some(value),
            "font-size" => {
                if let Some(size) = parse_css_number(&value) {
                    label.font_size = Some(size);
                }
            }
            "font-weight" => {
                label.font_weight = Some(parse_font_weight(&value));
            }
            "font-style" => {
                label.italic = Some(value.starts_with("italic") || value.starts_with("oblique"));
            }
            "font-family" => label.family = Some(value),
            _ => {}
        }
    }
    label
}

/// Applies the node declarations of a compiled class to a rectangle's paint, returning
/// `display`'s visibility (`false` when the class sets `display: none`).
fn apply_node_styles(styles: &[String], style: &mut PathStyle) -> bool {
    let mut visible = true;
    for (key, value) in style_map(styles) {
        if is_label_style(&key) {
            continue;
        }
        match key.as_str() {
            "fill" => style.fill = css_color(&value),
            "fill-opacity" => {
                if let Some(opacity) = parse_css_number(&value) {
                    style.fill_opacity = opacity;
                }
            }
            "stroke" => style.stroke = css_color(&value),
            "stroke-opacity" => {
                if let Some(opacity) = parse_css_number(&value) {
                    style.stroke_opacity = opacity;
                }
            }
            "stroke-width" => {
                if let Some(width) = parse_css_number(&value) {
                    style.stroke_width = width;
                }
            }
            "stroke-dasharray" => {
                style.dash = value
                    .split([' ', ','])
                    .filter_map(|part| part.trim().parse::<f64>().ok())
                    .collect();
            }
            "opacity" => {
                if let Some(opacity) = parse_css_number(&value) {
                    style.opacity = opacity;
                }
            }
            "display" if value == "none" => visible = false,
            _ => {}
        }
    }
    visible
}

/// A CSS color value as a [`PathStyle`] field: `none` paints nothing, everything else (including
/// `transparent`) is a color string the painter resolves.
fn css_color(value: &str) -> Option<Color> {
    if value == "none" {
        None
    } else {
        Some(Color::new(value))
    }
}

/// The leading number of a CSS value (`2px` -> 2, `0.4` -> 0.4).
fn parse_css_number(value: &str) -> Option<f64> {
    let value = value.trim();
    let mut end = 0;
    for (index, ch) in value.char_indices() {
        if ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == '+' {
            end = index + ch.len_utf8();
        } else {
            break;
        }
    }
    value[..end].parse().ok()
}

/// A CSS `font-weight` as a [`Font::weight`].
fn parse_font_weight(value: &str) -> u16 {
    match value.trim() {
        "bold" => 700,
        "bolder" => 700,
        "normal" => 400,
        "lighter" => 300,
        other => other.parse::<f64>().map_or(400, |weight| weight as u16),
    }
}

/// `d3.scaleOrdinal().range(range)`: an implicit domain in first-seen order, one range entry
/// per new key, cycling after the range's length. The fill and peer scales start their range
/// with `transparent`, so the first name that reaches them maps to that slot.
struct OrdinalScale {
    range: Vec<String>,
    domain: Vec<String>,
}

impl OrdinalScale {
    fn new(range: Vec<String>) -> Self {
        Self {
            range,
            domain: Vec::new(),
        }
    }

    fn get(&mut self, key: &str) -> String {
        if let Some(index) = self.domain.iter().position(|existing| existing == key) {
            return self.range[index % self.range.len()].clone();
        }
        let index = self.domain.len();
        self.domain.push(key.to_string());
        self.range[index % self.range.len()].clone()
    }
}

/// The fitted content bounds: every painted rectangle and text run, with group transforms.
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
            Item::Rect(shape) => include(out, transformed(shape.rect, transform)),
            Item::Text(text) => include(out, transformed(text.bounds(), transform)),
            Item::Path(shape) => {
                if let Some(bounds) = crate::render::pipeline::dagre::svg_path_bounds(&shape.path) {
                    include(out, transformed(bounds, transform));
                }
            }
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
            Item::Image(image) => include(out, transformed(image.rect, transform)),
        }
    }

    let mut bounds = None;
    for item in items {
        walk(item, Transform::IDENTITY, &mut bounds);
    }
    bounds
}

/// d3-format's format specifier, as far as treemap's `valueFormat` branches need it.
#[derive(Clone, Debug, PartialEq)]
struct NumberFormat {
    comma: bool,
    precision: usize,
    trim: bool,
    kind: char,
    prefix: String,
    suffix: String,
}

impl NumberFormat {
    /// d3-format's default specifier, `.12~g` with comma grouping: `format(',')`.
    fn comma() -> Self {
        Self {
            comma: true,
            precision: 12,
            trim: true,
            kind: 'g',
            prefix: String::new(),
            suffix: String::new(),
        }
    }

    /// `d3.format(specifier)`, or `None` for a specifier this parser does not understand.
    fn parse(specifier: &str) -> Option<Self> {
        let chars: Vec<char> = specifier.chars().collect();
        let mut index = 0;
        let mut comma = false;
        let mut precision = 6usize;
        let mut precision_given = false;
        let mut trim = false;
        let mut prefix = String::new();

        // Fill and align (`<`, `>`, `^`, `=`), then sign, symbol, zero, width.
        if chars
            .get(index + 1)
            .is_some_and(|next| "<>=^".contains(*next))
        {
            index += 2;
        } else if chars.get(index).is_some_and(|ch| "<>=^".contains(*ch)) {
            index += 1;
        }
        if chars
            .get(index)
            .is_some_and(|ch| matches!(ch, '+' | '-' | '(' | ' '))
        {
            index += 1;
        }
        if chars.get(index).is_some_and(|ch| matches!(ch, '$' | '#')) {
            prefix = chars[index].to_string();
            index += 1;
        }
        if chars.get(index) == Some(&'0') {
            index += 1;
        }
        while chars.get(index).is_some_and(char::is_ascii_digit) {
            index += 1;
        }
        if chars.get(index) == Some(&',') {
            comma = true;
            index += 1;
        }
        if chars.get(index) == Some(&'.') {
            index += 1;
            let start = index;
            while chars.get(index).is_some_and(char::is_ascii_digit) {
                index += 1;
            }
            if start == index {
                return None;
            }
            let digits: String = chars[start..index].iter().collect();
            precision = digits.parse().ok()?;
            precision_given = true;
        }
        if chars.get(index) == Some(&'~') {
            trim = true;
            index += 1;
        }
        let kind = chars.get(index).map(|ch| ch.to_ascii_lowercase());
        if kind.is_some() {
            index += 1;
        }
        if index != chars.len() {
            return None;
        }

        // The `""` type, and any invalid type, is an alias for `.12~g`; `n` for `,g`.
        let (kind, precision, trim) = match kind {
            None if precision_given => ('g', precision, true),
            None => ('g', 12, true),
            Some('n') => ('g', precision, trim),
            Some(kind) if matches!(kind, 'd' | 'f' | 'g' | 'e' | '%') => (kind, precision, trim),
            Some(_) => ('g', 12, true),
        };
        let comma = comma || matches!(kind, 'g') && specifier.contains('n');
        let precision = if matches!(kind, 'g') {
            precision.clamp(1, 21)
        } else if matches!(kind, 'f' | '%') {
            precision.min(20)
        } else {
            precision
        };
        let suffix = if kind == '%' {
            "%".to_string()
        } else {
            String::new()
        };
        Some(Self {
            comma,
            precision,
            trim,
            kind,
            prefix,
            suffix,
        })
    }

    fn render(&self, value: f64) -> String {
        let negative = value < 0.0 || (value == 0.0 && value.is_sign_negative());
        let absolute = value.abs();
        let mut body = if absolute.is_nan() {
            "NaN".to_string()
        } else if absolute.is_infinite() {
            "Infinity".to_string()
        } else {
            match self.kind {
                'd' => format!("{:.0}", js_round(absolute)),
                'f' => format!("{:.*}", self.precision, absolute),
                '%' => format!("{:.*}", self.precision, absolute * 100.0),
                'e' => scientific(&format!("{:.*e}", self.precision, absolute)),
                _ => to_precision(absolute, self.precision),
            }
        };
        if self.trim {
            body = trim_number(&body);
        }
        if self.comma {
            body = group_digits(&body);
        }
        let rounded_to_zero = body
            .replace(',', "")
            .parse::<f64>()
            .is_ok_and(|rounded| rounded == 0.0);
        let sign = if negative && !rounded_to_zero {
            "\u{2212}"
        } else {
            ""
        };
        format!("{sign}{}{body}{}", self.prefix, self.suffix)
    }
}

/// The `valueFormat` branches of `draw`'s `try`: `$0,0`, other `$` with commas, a plain `$`,
/// or a d3 specifier; any parse failure falls back to the default comma format.
#[derive(Clone, Debug, PartialEq)]
struct ValueFormat {
    dollar: bool,
    format: NumberFormat,
}

impl ValueFormat {
    fn from_config(value_format: &str) -> Self {
        let spec = if value_format.is_empty() {
            ","
        } else {
            value_format
        };
        if spec == "$0,0" {
            return Self {
                dollar: true,
                format: NumberFormat::comma(),
            };
        }
        if let Some(rest) = spec.strip_prefix('$') {
            if spec.contains(',') {
                let precision = precision_of(spec);
                let format_spec = match precision {
                    Some(precision) => format!(",.{precision}"),
                    None => ",".to_string(),
                };
                return Self {
                    dollar: true,
                    format: NumberFormat::parse(&format_spec).unwrap_or_else(NumberFormat::comma),
                };
            }
            let rest = rest.to_string();
            return Self {
                dollar: true,
                format: NumberFormat::parse(&rest).unwrap_or_else(NumberFormat::comma),
            };
        }
        Self {
            dollar: false,
            format: NumberFormat::parse(spec).unwrap_or_else(NumberFormat::comma),
        }
    }

    fn render(&self, value: f64) -> String {
        let body = self.format.render(value);
        if self.dollar {
            format!("${body}")
        } else {
            body
        }
    }
}

/// `/\.\d+/` on a specifier, as `draw`'s currency branch extracts it.
fn precision_of(spec: &str) -> Option<String> {
    let start = spec.find('.')?;
    let digits: String = spec[start + 1..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    (!digits.is_empty()).then_some(digits)
}

/// JavaScript's `Math.round`.
fn js_round(value: f64) -> f64 {
    if !value.is_finite() || value == 0.0 {
        return value;
    }
    let rounded = (value + 0.5).floor();
    if rounded == 0.0 && value < 0.0 {
        -0.0
    } else {
        rounded
    }
}

/// `Number.prototype.toPrecision(p)` for a non-negative value, without trimming.
fn to_precision(value: f64, precision: usize) -> String {
    if value == 0.0 {
        return if precision > 1 {
            format!("0.{}", "0".repeat(precision - 1))
        } else {
            "0".to_string()
        };
    }
    let exponential = format!("{:.*e}", precision.saturating_sub(1), value);
    let (mantissa, exponent) = exponential.split_once('e').unwrap_or((&exponential, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let digits: String = mantissa.chars().filter(|ch| *ch != '.').collect();
    let digits = if digits.is_empty() { "0" } else { &digits };

    if exponent < -6 || exponent >= precision as i32 {
        let scientific = if digits.len() > 1 {
            format!(
                "{}.{}e{}{}",
                &digits[..1],
                &digits[1..],
                if exponent >= 0 { "+" } else { "-" },
                exponent.abs()
            )
        } else {
            format!(
                "{}e{}{}",
                digits,
                if exponent >= 0 { "+" } else { "-" },
                exponent.abs()
            )
        };
        return scientific;
    }
    if exponent >= 0 {
        let integer_digits = exponent as usize + 1;
        if digits.len() <= integer_digits {
            return format!("{}{}", digits, "0".repeat(integer_digits - digits.len()));
        }
        return format!(
            "{}.{}",
            &digits[..integer_digits],
            &digits[integer_digits..]
        );
    }
    format!("0.{}{}", "0".repeat((-exponent - 1) as usize), digits)
}

/// `formatTrim`: `1.2000` -> `1.2`, `0.300` -> `0.3`, `1e+21` unchanged.
fn trim_number(value: &str) -> String {
    let (mantissa, exponent) = match value.split_once('e') {
        Some((mantissa, exponent)) => (mantissa, Some(exponent)),
        None => (value, None),
    };
    let mantissa = if mantissa.contains('.') {
        mantissa.trim_end_matches('0').trim_end_matches('.')
    } else {
        mantissa
    };
    match exponent {
        Some(exponent) => format!("{mantissa}e{exponent}"),
        None => mantissa.to_string(),
    }
}

/// `formatGroup`: group the integer part in threes, leaving the fraction or exponent alone.
fn group_digits(value: &str) -> String {
    let split = value
        .char_indices()
        .find(|(_, ch)| !ch.is_ascii_digit())
        .map_or(value.len(), |(index, _)| index);
    let (integer, rest) = value.split_at(split);
    if integer.is_empty() {
        return value.to_string();
    }
    let mut grouped = String::new();
    for (index, ch) in integer.chars().enumerate() {
        if index > 0 && (integer.len() - index) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    grouped.push_str(rest);
    grouped
}

/// `Number.prototype.toExponential`'s exponent sign, which Rust's `{:e}` omits.
fn scientific(value: &str) -> String {
    match value.split_once('e') {
        Some((mantissa, exponent)) if !exponent.starts_with('-') && !exponent.starts_with('+') => {
            format!("{mantissa}e+{exponent}")
        }
        _ => value.to_string(),
    }
}

#[cfg(test)]
mod tests;
