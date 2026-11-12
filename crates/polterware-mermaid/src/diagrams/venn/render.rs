//! Source-to-[`Scene`] rendering for Venn diagrams: the Rust side of
//! `diagrams/venn/vennRenderer.ts` `draw`.
//!
//! A venn diagram does not go through the unified layout pipeline: `draw` builds its SVG with
//! plain d3 through [`@upsetjs/venn.js`](https://github.com/upsetjs/venn.js), ported in
//! [`crate::layout::venn`]. It reads the database's subsets, text nodes and style entries,
//! adds the pairwise areas a three-or-more-set union needs, lays the circles out and scales
//! them into the configured viewport, draws one filled and stroked circle per set and one
//! intersection path per union, writes each area's label at its computed text centre, then
//! appends the title and the text nodes. This module ports that: [`draw`] parses, [`draw_ast`]
//! renders an already parsed database, and [`render`] uses the crate's monospace fallback
//! measurement.
//!
//! Look: `diagrams/venn/styles.ts` (`.venn-title`, `.venn-circle text`, `.venn-intersection
//! text`, `.venn-text-node`) and the inline styles `vennRenderer.ts` writes. Colors are the
//! theme's `venn1`…`venn8` palette, `primaryColor` as the fallback, and the text colors
//! derived with khroma's `lighten`/`darken` for a dark or light background.
//!
//! Behavior: the renderer itself is the model, followed call by call; gpui-kit has no venn
//! renderer, so there is no second behavior reference.
//!
//! The constants are `draw`'s: the viewport is `config.venn.width` × `config.venn.height`
//! (800 × 450 by default), the title is `32 * scale` tall at `y = 32 * scale` plus a
//! `48 * scale` title band, where `scale = width / 1600`; area labels are `48 * scale` and
//! text nodes `40 * scale`.
//!
//! Text is measured in the `monospace` family, the rule the fixture harness renders with and
//! that [`FallbackMeasure`](crate::measure::FallbackMeasure) reproduces. The set labels wrap
//! with the width venn.js measures before `draw` sets the inline font size, which the browser
//! takes from the `.venn-circle text { font-size: 48px }` stylesheet rule; the harness has no
//! SVG cascade, so its generator installs that size for the measurement to match a browser.
//!
//! Where the port stops short of upstream: the class names (`venn-area`, `venn-circle`,
//! `venn-set-N`, `venn-intersection`, `venn-text-nodes`) matter only to CSS, and a [`Scene`]
//! carries paints rather than selectors, so `draw` resolves the classes' declarations and the
//! inline styles into the shapes. `look: 'handDrawn'` draws the circles through rough.js; a
//! [`Scene`] has no sketch pass, so the rough branch is not represented and the classic path
//! is drawn instead. `useDebugLayout` is carried through (its circle and grid cell), because it
//! is plain SVG geometry. The SVG ids `Uid.next` assigns (`venn-1`, ...) are page-wide and
//! are not written by this renderer, so the scene carries no ids. `useMaxWidth` sizes the
//! `<svg>` element, not the drawing; the scene carries the fixed viewBox and the painter sizes
//! the page.

use std::collections::HashMap;

use serde_json::{Map, Value};

use crate::config::Config;
use crate::error::ParseError;
use crate::layout::venn::{self, LayoutArea, VennArea};
use crate::measure::{FallbackMeasure, Font, TextMeasure, TextOptions, shape_text};
use crate::render::shapes::label_image_utils::parse_font_size;
use crate::scene::{
    Color, EllipseShape, Group, PathStyle, Point, Rect, RectShape, Scene, TextAnchor, TextBaseline,
    TextRun,
};
use crate::theme::color;

use super::Ast;
use super::ast::{StyleEntry, Subset};

/// `REFERENCE_WIDTH` in `draw`: the width the `scale` factor divides by.
const REFERENCE_WIDTH: f64 = 1600.0;
/// `draw`'s title font size before scaling.
const TITLE_FONT_SIZE: f64 = 32.0;
/// `draw`'s title band height before scaling.
const TITLE_BAND: f64 = 48.0;
/// `draw`'s area label font size before scaling.
const AREA_FONT_SIZE: f64 = 48.0;
/// `wrapText`'s `lineHeight`, in ems.
const WRAP_LINE_HEIGHT: f64 = 1.1;
/// `wrapText`'s `maxLines`.
const WRAP_MAX_LINES: usize = 3;
/// `renderTextNodes`'s area font size before scaling.
const TEXT_NODE_FONT_SIZE: f64 = 40.0;
/// The circle's default `fill-opacity`.
const CIRCLE_FILL_OPACITY: f64 = 0.1;
/// The area's `stroke-opacity`.
const STROKE_OPACITY: f64 = 0.95;

/// Renders a venn body (the text [`super::parse`] accepts) to a [`Scene`].
///
/// `config` is the diagram configuration after the source's frontmatter and directives have
/// been applied, as `getConfig()` returns it to `draw`.
pub fn draw(body: &str, config: &Config, measure: &dyn TextMeasure) -> Result<Scene, ParseError> {
    let ast = super::parse(body)?;
    Ok(draw_ast(&ast, config, measure))
}

/// Renders an already parsed venn database.
pub fn draw_ast(ast: &Ast, config: &Config, measure: &dyn TextMeasure) -> Scene {
    draw_ast_with_theme(ast, config, &config.theme_variables, measure)
}

/// Renders a venn body with the crate's monospace fallback measurement.
pub fn render(body: &str, config: &Config) -> Result<Scene, ParseError> {
    draw(body, config, &FallbackMeasure)
}

/// The pieces of one drawn area, resolved the way `draw` styles them.
struct DrawArea<'a> {
    subset: &'a Subset,
    area: &'a LayoutArea,
    label: Option<String>,
    is_circle: bool,
    circle_index: usize,
}

/// Renders an already parsed database with the theme variables `draw` reads.
fn draw_ast_with_theme(
    ast: &Ast,
    config: &Config,
    theme: &Map<String, Value>,
    measure: &dyn TextMeasure,
) -> Scene {
    let venn_config = &config.venn;
    let width = venn_config.width;
    let height = venn_config.height;
    let padding = venn_config.padding;
    let scale = width / REFERENCE_WIDTH;
    let title_height = if ast.diagram_title.is_empty() {
        0.0
    } else {
        TITLE_BAND * scale
    };

    let render_sets = ensure_pairwise_subsets(&ast.subsets);
    let style_by_key = build_style_by_key(&ast.styles);
    let palette: Vec<String> = (1..=8)
        .filter_map(|index| theme_string(theme, &format!("venn{index}")))
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect();
    let primary = theme_string(theme, "primaryColor")
        .unwrap_or("")
        .to_string();
    let default_text_color = theme_string(theme, "primaryTextColor")
        .or_else(|| theme_string(theme, "textColor"))
        .unwrap_or("")
        .to_string();
    let set_text_color = theme_string(theme, "vennSetTextColor").map(str::to_string);
    let background = theme_string(theme, "background").unwrap_or("#f4f4f4");
    let theme_dark = color::is_dark(background).unwrap_or(false);

    // The layout the renderer asks for separately, used only to place text nodes.
    let layout = venn::layout(
        &to_areas(&render_sets),
        width,
        height - title_height,
        padding,
    );
    let mut layout_by_key: HashMap<String, (&LayoutArea, Option<String>)> = HashMap::new();
    for (area, subset) in layout.areas.iter().zip(&render_sets) {
        layout_by_key.insert(stable_sets_key(&area.sets), (area, subset.label.clone()));
    }

    // `VennDiagram()` drops zero-sized single sets and every area that names them.
    let mut removed_sets: Vec<String> = Vec::new();
    for subset in &render_sets {
        if subset.sets.len() == 1 && subset.size == 0.0 {
            removed_sets.push(subset.sets[0].clone());
        }
    }
    let chart_data: Vec<(&Subset, VennArea)> = render_sets
        .iter()
        .filter(|subset| {
            !subset
                .sets
                .iter()
                .any(|set| removed_sets.iter().any(|removed| removed == set))
        })
        .map(|subset| (subset, VennArea::new(subset.sets.clone(), subset.size)))
        .collect();
    let chart_areas: Vec<VennArea> = chart_data.iter().map(|(_, area)| area.clone()).collect();
    // `VennDiagram()` never gets `.padding()`, so the chart keeps venn.js's own default of 15;
    // `layout()` is the call that reads `config.padding`.
    let chart = venn::chart(&chart_areas, width, height - title_height, 15.0);

    let mut drawn: Vec<DrawArea<'_>> = Vec::new();
    let mut circle_index = 0;
    for (subset, area) in chart_data
        .iter()
        .map(|(subset, _)| *subset)
        .zip(&chart.areas)
    {
        let is_circle = subset.sets.len() == 1;
        drawn.push(DrawArea {
            subset,
            area,
            label: subset.label.clone(),
            is_circle,
            circle_index,
        });
        if is_circle {
            circle_index += 1;
        }
    }

    let view_box = Rect::new(0.0, 0.0, width, height);
    let mut scene = Scene::new(view_box);

    if !ast.diagram_title.is_empty() {
        let fill = theme_string(theme, "vennTitleTextColor")
            .filter(|value| !value.is_empty())
            .or_else(|| theme_string(theme, "titleColor"))
            .map(Color::new);
        let font = Font::new("monospace", TITLE_FONT_SIZE * scale);
        let mut run = TextRun::centered(
            Point::new(width / 2.0, TITLE_FONT_SIZE * scale),
            font.clone(),
            shape_text(
                &ast.diagram_title,
                &TextOptions {
                    width: f64::INFINITY,
                    markdown: false,
                    min_width: 0.0,
                },
                &font,
                measure,
            )
            .unwrap_or_default(),
        );
        run.baseline = TextBaseline::Middle;
        run.fill = fill;
        scene.push(run);
    }

    let mut group = Group::translated(0.0, title_height);
    for area in &drawn {
        let sets_key = stable_sets_key(&area.subset.sets);
        let custom_style = style_for(&style_by_key, &sets_key);

        let (path_style, text_fill, class) = if area.is_circle {
            let palette_color = if palette.is_empty() {
                None
            } else {
                palette.get(area.circle_index % palette.len()).cloned()
            };
            let base_color = custom_style_value(custom_style, "fill")
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .or(palette_color)
                .unwrap_or_else(|| primary.clone());
            let fill_opacity = custom_style
                .and_then(|style| style_value(style, "fill-opacity"))
                .map(|value| value.parse::<f64>().unwrap_or(1.0))
                .unwrap_or(CIRCLE_FILL_OPACITY);
            let stroke_color = custom_style_value(custom_style, "stroke")
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| base_color.clone());
            let stroke_width = custom_style_value(custom_style, "stroke-width")
                .filter(|value| !value.is_empty())
                .and_then(parse_font_size)
                .unwrap_or(5.0 * scale);
            let text_color = custom_style_value(custom_style, "color")
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| {
                    let derived = if theme_dark {
                        color::lighten(&base_color, 30.0)
                    } else {
                        color::darken(&base_color, 30.0)
                    };
                    derived.unwrap_or(base_color.clone())
                });
            (
                PathStyle {
                    fill: Some(Color::new(base_color)),
                    fill_opacity,
                    stroke: Some(Color::new(stroke_color)),
                    stroke_width,
                    stroke_opacity: STROKE_OPACITY,
                    ..PathStyle::default()
                },
                Some(text_color),
                format!("venn-area venn-circle venn-set-{}", area.circle_index % 8),
            )
        } else {
            let custom_fill = custom_style_value(custom_style, "fill")
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            let text_color = custom_style_value(custom_style, "color")
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .or_else(|| set_text_color.clone())
                .unwrap_or_else(|| default_text_color.clone());
            (
                PathStyle {
                    fill: Some(Color::new(
                        custom_fill
                            .clone()
                            .unwrap_or_else(|| "transparent".to_string()),
                    )),
                    fill_opacity: if custom_fill.is_some() { 1.0 } else { 0.0 },
                    ..PathStyle::default()
                },
                Some(text_color),
                "venn-area venn-intersection".to_string(),
            )
        };

        let mut area_group = Group::new().with_class(class);
        area_group.push(crate::scene::PathShape::new(
            area.area.path.clone(),
            path_style,
        ));

        let label = area_label(area);
        let lines = wrap_text(&label, area, &chart, measure);
        if !lines.is_empty() {
            let font = Font::new("monospace", AREA_FONT_SIZE * scale);
            let centre = area.area.text;
            let initial = 0.35 - (lines.len().saturating_sub(1) as f64 * WRAP_LINE_HEIGHT) / 2.0;
            let line_height = WRAP_LINE_HEIGHT * AREA_FONT_SIZE * scale;
            let width = lines
                .iter()
                .map(|line| measure.width(line, &font))
                .fold(0.0, f64::max);
            let height =
                measure.line_height(&font) + line_height * lines.len().saturating_sub(1) as f64;
            let run = TextRun {
                position: Point::new(
                    js_floor(centre.x),
                    js_floor(centre.y) + initial * AREA_FONT_SIZE * scale,
                ),
                anchor: TextAnchor::Middle,
                baseline: TextBaseline::Alphabetic,
                font,
                line_height,
                lines: lines
                    .into_iter()
                    .map(|line| {
                        crate::scene::TextLine::new(vec![crate::scene::TextSpan::plain(line)])
                    })
                    .collect(),
                size: crate::scene::Size::new(width, height),
                fill: text_fill.map(Color::new),
                ..TextRun::default()
            };
            area_group.push(run);
        }
        group.push(area_group);
    }

    if !ast.texts.is_empty() {
        group.push(render_text_nodes(
            ast,
            &layout_by_key,
            &style_by_key,
            set_text_color.as_deref(),
            config.venn.use_debug_layout,
            scale,
            measure,
        ));
    }

    scene.push(group);
    scene
}

/// `renderTextNodes(config, layoutByKey, dummyD3root, textNodes, scale, styleByKey)`.
#[allow(clippy::too_many_arguments)]
fn render_text_nodes(
    ast: &Ast,
    layout_by_key: &HashMap<String, (&LayoutArea, Option<String>)>,
    style_by_key: &[(String, Vec<(String, String)>)],
    set_text_color: Option<&str>,
    use_debug_layout: bool,
    scale: f64,
    measure: &dyn TextMeasure,
) -> Group {
    let mut text_group = Group::new().with_class("venn-text-nodes");

    let mut nodes_by_area: Vec<(String, Vec<&super::ast::TextNode>)> = Vec::new();
    for node in &ast.texts {
        let key = stable_sets_key(&node.sets);
        match nodes_by_area.iter_mut().find(|(name, _)| *name == key) {
            Some((_, nodes)) => nodes.push(node),
            None => nodes_by_area.push((key, vec![node])),
        }
    }

    for (key, nodes) in nodes_by_area {
        let Some((area, area_label)) = layout_by_key.get(&key) else {
            continue;
        };
        let centre = area.text;
        let min_circle_radius = area
            .circles
            .iter()
            .map(|circle| circle.radius)
            .fold(f64::INFINITY, f64::min);
        let inner_radius_raw = area
            .circles
            .iter()
            .map(|circle| circle.radius - (centre.x - circle.x).hypot(centre.y - circle.y))
            .fold(f64::INFINITY, f64::min);
        let mut inner_radius = if inner_radius_raw.is_finite() {
            inner_radius_raw.max(0.0)
        } else {
            0.0
        };
        if inner_radius == 0.0 && min_circle_radius.is_finite() {
            inner_radius = min_circle_radius * 0.6;
        }

        let mut area_group = Group::new().with_class("venn-text-area");
        if use_debug_layout {
            area_group.push(EllipseShape::new(
                Point::new(centre.x, centre.y),
                inner_radius,
                inner_radius,
                PathStyle {
                    fill: None,
                    stroke: Some(Color::new("purple")),
                    stroke_width: 1.5 * scale,
                    dash: vec![6.0 * scale, 4.0 * scale],
                    ..PathStyle::default()
                },
            ));
        }

        let inner_width = (80.0 * scale).max(inner_radius * 2.0 * 0.95);
        let inner_height = (60.0 * scale).max(inner_radius * 2.0 * 0.95);
        let has_label = area_label.as_deref().is_some_and(|label| !label.is_empty());
        let label_offset_base = if has_label {
            (32.0 * scale).min(inner_radius * 0.25)
        } else {
            0.0
        };
        let label_offset = label_offset_base + if nodes.len() <= 2 { 30.0 * scale } else { 0.0 };
        let start_x = centre.x - inner_width / 2.0;
        let start_y = centre.y - inner_height / 2.0 + label_offset;
        let cols = ((nodes.len() as f64).sqrt().ceil() as usize).max(1);
        let rows = nodes.len().div_ceil(cols).max(1);
        let cell_width = inner_width / cols as f64;
        let cell_height = inner_height / rows as f64;

        for (index, node) in nodes.iter().enumerate() {
            let col = index % cols;
            let row = index / cols;
            let x = start_x + cell_width * (col as f64 + 0.5);
            let y = start_y + cell_height * (row as f64 + 0.5);

            if use_debug_layout {
                area_group.push(RectShape::new(
                    Rect::new(
                        start_x + cell_width * col as f64,
                        start_y + cell_height * row as f64,
                        cell_width,
                        cell_height,
                    ),
                    0.0,
                    PathStyle {
                        fill: None,
                        stroke: Some(Color::new("teal")),
                        stroke_width: scale,
                        dash: vec![4.0 * scale, 3.0 * scale],
                        ..PathStyle::default()
                    },
                ));
            }

            let box_width = cell_width * 0.9;
            let font = Font::new("monospace", TEXT_NODE_FONT_SIZE * scale);
            let text = node.label.clone().unwrap_or_else(|| node.id.clone());
            let fill = style_for(style_by_key, &node.id)
                .and_then(|style| style_value(style, "color"))
                .filter(|value| !value.is_empty())
                .map(Color::new)
                .or_else(|| set_text_color.map(Color::new));
            let block = shape_text(
                &text,
                &TextOptions {
                    width: box_width,
                    markdown: false,
                    min_width: 0.0,
                },
                &font,
                measure,
            )
            .unwrap_or_default();
            let mut run = TextRun::centered(Point::new(x, y), font, block);
            run.baseline = TextBaseline::Middle;
            run.fill = fill;
            run.wrapping_width = Some(box_width);
            area_group.push(run);
        }
        text_group.push(area_group);
    }

    text_group
}

/// `ensurePairwiseSubsets(subsets)`: adds a synthetic area for every pair of a three-or-more-set
/// union that the source did not name. The stored subsets are not mutated.
pub fn ensure_pairwise_subsets(subsets: &[Subset]) -> Vec<Subset> {
    let mut existing_keys: Vec<String> = subsets
        .iter()
        .map(|subset| stable_sets_key(&subset.sets))
        .collect();

    let mut individual_sizes: Vec<(String, f64)> = Vec::new();
    for subset in subsets {
        if subset.sets.len() == 1 {
            match individual_sizes
                .iter_mut()
                .find(|(set, _)| *set == subset.sets[0])
            {
                Some((_, size)) => *size = subset.size,
                None => individual_sizes.push((subset.sets[0].clone(), subset.size)),
            }
        }
    }

    let mut synthetic: Vec<Subset> = Vec::new();
    for subset in subsets {
        if subset.sets.len() < 3 {
            continue;
        }
        let mut members = subset.sets.clone();
        members.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
        for i in 0..members.len().saturating_sub(1) {
            for j in (i + 1)..members.len() {
                let pair = vec![members[i].clone(), members[j].clone()];
                let key = stable_sets_key(&pair);
                if existing_keys.contains(&key) {
                    continue;
                }
                existing_keys.push(key);
                let size_a = individual_sizes
                    .iter()
                    .find(|(set, _)| *set == pair[0])
                    .map(|(_, size)| *size);
                let size_b = individual_sizes
                    .iter()
                    .find(|(set, _)| *set == pair[1])
                    .map(|(_, size)| *size);
                let pair_size = match (size_a, size_b) {
                    (Some(size_a), Some(size_b)) => size_a.min(size_b) / 4.0,
                    _ => 2.5,
                };
                synthetic.push(Subset {
                    sets: pair,
                    size: pair_size,
                    label: None,
                });
            }
        }
    }

    if synthetic.is_empty() {
        return subsets.to_vec();
    }
    let mut result = subsets.to_vec();
    result.extend(synthetic);
    result
}

fn to_areas(subsets: &[Subset]) -> Vec<VennArea> {
    subsets
        .iter()
        .map(|subset| VennArea::new(subset.sets.clone(), subset.size))
        .collect()
}

/// `stableSetsKey`: the sorted set ids joined with `|`.
fn stable_sets_key(sets: &[String]) -> String {
    sets.join("|")
}

/// `buildStyleByKey`: an object assignment per target key, so a repeated key keeps its first
/// position and takes the last value.
fn build_style_by_key(styles: &[StyleEntry]) -> Vec<(String, Vec<(String, String)>)> {
    let mut map: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for entry in styles {
        let key = stable_sets_key(&entry.targets);
        match map.iter_mut().find(|(name, _)| *name == key) {
            Some((_, existing)) => {
                for (name, value) in &entry.styles {
                    match existing.iter_mut().find(|(property, _)| property == name) {
                        Some((_, current)) => *current = value.clone(),
                        None => existing.push((name.clone(), value.clone())),
                    }
                }
            }
            None => map.push((key, entry.styles.clone())),
        }
    }
    map
}

fn style_for<'a>(
    styles: &'a [(String, Vec<(String, String)>)],
    key: &str,
) -> Option<&'a Vec<(String, String)>> {
    styles
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, style)| style)
}

fn style_value<'a>(style: &'a [(String, String)], property: &str) -> Option<&'a str> {
    style
        .iter()
        .find(|(name, _)| name == property)
        .map(|(_, value)| value.as_str())
}

fn custom_style_value<'a>(
    style: Option<&'a Vec<(String, String)>>,
    property: &str,
) -> Option<&'a str> {
    style.and_then(|style| style_value(style, property))
}

/// A theme variable that is a string.
fn theme_string<'a>(theme: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    theme.get(key).and_then(Value::as_str)
}

/// `label(d)`: the subset's label, else the set name for a single set, else empty.
fn area_label(area: &DrawArea<'_>) -> String {
    if let Some(label) = &area.label
        && !label.is_empty()
    {
        return label.clone();
    }
    if area.is_circle {
        return area.subset.sets[0].clone();
    }
    String::new()
}

/// `wrapText(circles, label)` without the DOM: the lines it would write as `<tspan>`s.
fn wrap_text(
    label: &str,
    area: &DrawArea<'_>,
    chart: &crate::layout::venn::VennLayout,
    measure: &dyn TextMeasure,
) -> Vec<String> {
    if label.is_empty() {
        return Vec::new();
    }
    let width = area
        .subset
        .sets
        .first()
        .and_then(|set| chart.circle(set))
        .map(|circle| circle.radius)
        .unwrap_or(50.0);
    let mut words = split_js_whitespace(label);
    words.reverse();
    let min_chars = (utf16_len(label) + words.len()) as f64 / WRAP_MAX_LINES as f64;

    let mut word = words.pop().unwrap_or_default();
    let mut line: Vec<String> = vec![word.clone()];
    let mut lines: Vec<String> = vec![word.clone()];

    loop {
        word = match words.pop() {
            Some(word) => word,
            None => break,
        };
        line.push(word.clone());
        let joined = line.join(" ");
        if utf16_len(&joined) as f64 > min_chars
            && measure.width(&joined, &Font::new("monospace", AREA_FONT_SIZE)) > width
        {
            line.pop();
            if let Some(current) = lines.last_mut() {
                *current = line.join(" ");
            }
            line = vec![word.clone()];
            lines.push(word);
        } else if let Some(current) = lines.last_mut() {
            *current = joined;
        }
    }

    lines
}

/// JavaScript's `String.prototype.split(/\s+/)`: maximal whitespace runs separate, and a run at
/// either end leaves an empty element.
fn split_js_whitespace(text: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut saw_content = false;
    let mut in_gap = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !current.is_empty() {
                parts.push(std::mem::take(&mut current));
                saw_content = true;
            } else if !saw_content && parts.is_empty() && !in_gap {
                parts.push(String::new());
            }
            in_gap = true;
        } else {
            current.push(ch);
            in_gap = false;
        }
    }
    if !current.is_empty() {
        parts.push(current);
    } else if in_gap {
        parts.push(String::new());
    }
    parts
}

/// JavaScript's `String.prototype.length`: UTF-16 code units.
fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// `Math.floor` on the text centre coordinates.
fn js_floor(value: f64) -> f64 {
    value.floor()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Item;

    const TOLERANCE: f64 = 0.01;

    fn fixtures() -> Vec<(String, Value)> {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/diagrams/venn/tests/render");
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

    fn text_of(run: &TextRun) -> Vec<String> {
        run.lines.iter().map(|line| line.text()).collect()
    }

    #[test]
    fn renders_recorded_venn_fixtures() {
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

            let mut items = scene.items.iter();
            if let Some(expected_title) = expected["title"].as_object() {
                let title = match items.next() {
                    Some(Item::Text(text)) => text,
                    other => panic!("{file}: missing title text: {other:?}"),
                };
                assert_eq!(
                    text_of(title).join("\n"),
                    expected_title["text"].as_str().expect("title text"),
                    "{file}: title text"
                );
                close(
                    title.position.x,
                    expected_title["x"].as_f64().expect("title x"),
                    "title x",
                    &file,
                );
                close(
                    title.position.y,
                    expected_title["y"].as_f64().expect("title y"),
                    "title y",
                    &file,
                );
                close(
                    title.font.size,
                    expected_title["fontSize"]
                        .as_f64()
                        .expect("title font size"),
                    "title font size",
                    &file,
                );
                assert_eq!(
                    title.fill.as_ref().map(|color| color.0.clone()),
                    expected_title["fill"].as_str().map(str::to_string),
                    "{file}: title fill"
                );
            }

            let root = match items.next() {
                Some(Item::Group(group)) => group,
                other => panic!("{file}: missing root group: {other:?}"),
            };
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

            let expected_areas = expected["areas"].as_array().expect("areas");
            let mut group_items = root.items.iter();
            for (index, expected_area) in expected_areas.iter().enumerate() {
                let area_group = match group_items.next() {
                    Some(Item::Group(group)) => group,
                    other => panic!("{file}: area {index} is not a group: {other:?}"),
                };
                assert_eq!(
                    area_group.class.as_deref(),
                    expected_area["className"].as_str(),
                    "{file}: area {index} class"
                );

                let path = match area_group.items.first() {
                    Some(Item::Path(path)) => path,
                    other => panic!("{file}: area {index} path missing: {other:?}"),
                };
                close_value(
                    &serde_json::to_value(&path.path.commands).expect("serialize path"),
                    &expected_area["path"]["commands"],
                    &format!("area {index} path"),
                    &file,
                );
                assert_eq!(
                    path.style.fill.as_ref().map(|color| color.0.clone()),
                    expected_area["path"]["fill"].as_str().map(str::to_string),
                    "{file}: area {index} fill"
                );
                close(
                    path.style.fill_opacity,
                    expected_area["path"]["fillOpacity"]
                        .as_f64()
                        .expect("fill opacity"),
                    &format!("area {index} fill opacity"),
                    &file,
                );
                assert_eq!(
                    path.style.stroke.as_ref().map(|color| color.0.clone()),
                    expected_area["path"]["stroke"].as_str().map(str::to_string),
                    "{file}: area {index} stroke"
                );
                if let Some(stroke_width) = expected_area["path"]["strokeWidth"].as_f64() {
                    close(
                        path.style.stroke_width,
                        stroke_width,
                        &format!("area {index} stroke width"),
                        &file,
                    );
                }

                let expected_text = &expected_area["text"];
                let expected_content = expected_text["content"].as_str().unwrap_or_default();
                let run = area_group.items.iter().find_map(|item| match item {
                    Item::Text(text) => Some(text),
                    _ => None,
                });
                if expected_content.is_empty() {
                    assert!(run.is_none(), "{file}: area {index} unexpectedly has text");
                    continue;
                }
                let run = run.unwrap_or_else(|| panic!("{file}: area {index} missing text run"));
                assert_eq!(
                    text_of(run).join("\n"),
                    expected_content,
                    "{file}: area {index} text"
                );
                close(
                    run.position.x,
                    expected_text["x"].as_f64().expect("text x"),
                    &format!("area {index} text x"),
                    &file,
                );
                close(
                    run.font.size,
                    expected_text["fontSize"].as_f64().expect("text font size"),
                    &format!("area {index} text font size"),
                    &file,
                );
                assert_eq!(
                    run.fill.as_ref().map(|color| color.0.clone()),
                    expected_text["fill"].as_str().map(str::to_string),
                    "{file}: area {index} text fill"
                );
                let expected_lines = expected_text["lines"].as_array().expect("text lines");
                assert_eq!(
                    run.lines.len(),
                    expected_lines.len(),
                    "{file}: area {index} line count"
                );
                for (line_index, (line, expected_line)) in
                    run.lines.iter().zip(expected_lines).enumerate()
                {
                    assert_eq!(
                        line.text(),
                        expected_line["text"].as_str().expect("line text"),
                        "{file}: area {index} line {line_index}"
                    );
                    close(
                        run.position.y + run.line_height * line_index as f64,
                        expected_line["baseline"].as_f64().expect("line baseline"),
                        &format!("area {index} line {line_index} baseline"),
                        &file,
                    );
                }
            }

            let expected_text_nodes = expected["textNodes"].as_array().expect("text nodes");
            if expected_text_nodes.is_empty() {
                assert!(
                    group_items.next().is_none(),
                    "{file}: unexpected text node group"
                );
            } else {
                let text_group = match group_items.next() {
                    Some(Item::Group(group)) => group,
                    other => panic!("{file}: missing text node group: {other:?}"),
                };
                let mut node_items = text_group.items.iter();
                for (index, expected_node) in expected_text_nodes.iter().enumerate() {
                    let area_group = match node_items.next() {
                        Some(Item::Group(group)) => group,
                        other => panic!("{file}: text node area {index}: {other:?}"),
                    };
                    if index == 0 {
                        match expected["debug"].as_object() {
                            Some(debug) => {
                                let circle = match area_group.items.first() {
                                    Some(Item::Ellipse(circle)) => circle,
                                    other => {
                                        panic!("{file}: missing debug circle: {other:?}")
                                    }
                                };
                                let expected_circle = &debug["circle"];
                                close(
                                    circle.center.x,
                                    expected_circle["cx"].as_f64().expect("debug cx"),
                                    "debug circle cx",
                                    &file,
                                );
                                close(
                                    circle.center.y,
                                    expected_circle["cy"].as_f64().expect("debug cy"),
                                    "debug circle cy",
                                    &file,
                                );
                                close(
                                    circle.radius_x,
                                    expected_circle["r"].as_f64().expect("debug r"),
                                    "debug circle r",
                                    &file,
                                );
                                assert_eq!(
                                    circle.style.stroke.as_ref().map(|color| color.0.clone()),
                                    expected_circle["stroke"].as_str().map(str::to_string),
                                    "{file}: debug circle stroke"
                                );
                                let actual_cells: Vec<&crate::scene::RectShape> = area_group
                                    .items
                                    .iter()
                                    .filter_map(|item| match item {
                                        Item::Rect(rect) => Some(rect),
                                        _ => None,
                                    })
                                    .collect();
                                let expected_cells =
                                    debug["cells"].as_array().expect("debug cells");
                                assert_eq!(
                                    actual_cells.len(),
                                    expected_cells.len(),
                                    "{file}: debug cell count"
                                );
                                for (cell_index, (cell, expected_cell)) in
                                    actual_cells.iter().zip(expected_cells).enumerate()
                                {
                                    close(
                                        cell.rect.x,
                                        expected_cell["x"].as_f64().expect("cell x"),
                                        &format!("debug cell {cell_index} x"),
                                        &file,
                                    );
                                    close(
                                        cell.rect.y,
                                        expected_cell["y"].as_f64().expect("cell y"),
                                        &format!("debug cell {cell_index} y"),
                                        &file,
                                    );
                                    close(
                                        cell.rect.width,
                                        expected_cell["width"].as_f64().expect("cell width"),
                                        &format!("debug cell {cell_index} width"),
                                        &file,
                                    );
                                    close(
                                        cell.rect.height,
                                        expected_cell["height"].as_f64().expect("cell height"),
                                        &format!("debug cell {cell_index} height"),
                                        &file,
                                    );
                                }
                            }
                            None => assert!(
                                !area_group.items.iter().any(|item| {
                                    matches!(item, Item::Ellipse(_) | Item::Rect(_))
                                }),
                                "{file}: unexpected debug geometry"
                            ),
                        }
                    }
                    let run = area_group.items.iter().find_map(|item| match item {
                        Item::Text(text) => Some(text),
                        _ => None,
                    });
                    let run = run.unwrap_or_else(|| panic!("{file}: text node {index} missing"));
                    assert_eq!(
                        text_of(run).join("\n"),
                        expected_node["text"].as_str().expect("node text"),
                        "{file}: text node {index} text"
                    );
                    close(
                        run.position.x,
                        expected_node["x"].as_f64().expect("node x"),
                        &format!("text node {index} x"),
                        &file,
                    );
                    close(
                        run.position.y,
                        expected_node["y"].as_f64().expect("node y"),
                        &format!("text node {index} y"),
                        &file,
                    );
                    close(
                        run.font.size,
                        expected_node["fontSize"].as_f64().expect("node font size"),
                        &format!("text node {index} font size"),
                        &file,
                    );
                    if let Some(color) = expected_node["color"].as_str() {
                        assert_eq!(
                            run.fill.as_ref().map(|color| color.0.clone()),
                            Some(color.to_string()),
                            "{file}: text node {index} color"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn draws_a_circle_for_each_set_and_a_path_for_each_union() {
        let scene = render(
            "venn-beta\n  set A\n  set B\n  union A, B[\"both\"]\n",
            &Config::default(),
        )
        .expect("render venn");
        let root = match &scene.items[0] {
            Item::Group(group) => group,
            item => panic!("root is not a group: {item:?}"),
        };
        let classes: Vec<Option<&str>> = root
            .items
            .iter()
            .map(|item| match item {
                Item::Group(group) => group.class.as_deref(),
                _ => None,
            })
            .collect();
        assert!(
            classes
                .iter()
                .any(|class| class == &Some("venn-area venn-circle venn-set-0"))
        );
        assert!(
            classes
                .iter()
                .any(|class| class == &Some("venn-area venn-circle venn-set-1"))
        );
        assert!(
            classes
                .iter()
                .any(|class| class == &Some("venn-area venn-intersection"))
        );
    }

    #[test]
    fn pairwise_subsets_fill_a_three_set_union() {
        let ast =
            super::super::parse("venn-beta\n  set A\n  set B\n  set C\n  union A, B, C[\"all\"]\n")
                .expect("parse venn");
        let subsets = ensure_pairwise_subsets(&ast.subsets);
        assert_eq!(subsets.len(), 7);
        assert!(subsets.iter().any(|subset| subset.sets == ["A", "B"]));
        assert!(subsets.iter().any(|subset| subset.sets == ["A", "C"]));
        assert!(subsets.iter().any(|subset| subset.sets == ["B", "C"]));
    }

    #[test]
    fn render_diagram_dispatches_the_venn_renderer() {
        let scene = crate::render_diagram(
            "venn-beta\n  set A\n  set B\n  union A, B[\"both\"]\n",
            &Config::default(),
        )
        .expect("render venn through render_diagram");
        assert_eq!(scene.view_box, Rect::new(0.0, 0.0, 800.0, 450.0));
        assert!(!scene.items.is_empty());
    }

    #[test]
    fn rejects_a_body_without_a_venn_header() {
        let error = render("set A\n", &Config::default()).unwrap_err();
        assert!(
            error.message.contains("expected VENN"),
            "unexpected message: {}",
            error.message
        );
    }
}
