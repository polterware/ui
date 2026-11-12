//! Source-to-[`Scene`] rendering for kanban boards: the Rust side of
//! `kanbanRenderer.ts` `draw` plus the shape helpers it calls.
//!
//! Unlike the flowchart, class and ER renderers, upstream's kanban renderer does not use the
//! unified pipeline: it calls `insertCluster` (the `kanbanSection` cluster shape) and
//! `insertNode` (the `kanbanItem` shape) directly, then places each card under the section
//! title by hand. This module ports that placement line by line and produces the same scene
//! primitives the SVG would hold: the section rectangles and titles, the item cards, their
//! title/ticket/assignee labels, and the priority stripe.
//!
//! The look mirrors the kanban stylesheet (`diagrams/kanban/styles.ts`): `.section-N` colours
//! the section rectangles and titles, `.node rect` colours the cards, and `.cluster-label` /
//! `.label` colour the text. There is no gpui-kit counterpart for this diagram; the behaviour
//! reference is upstream's own renderer.
//!
//! # Where the port stops short of upstream
//!
//! * The `handDrawn` look's rough.js pass is not painted; the classic rectangles are drawn,
//!   as in the other shape ports.
//! * Empty ticket and assignee labels (upstream still appends an empty `<g class="label">`)
//!   draw no text and are not added to the scene.
//! * A section past `THEME_COLOR_LIMIT` has no `.section-N` rule upstream, so its rectangle
//!   keeps SVG's black fill and no stroke and its title the `.cluster-label` colour; the
//!   palette here stops at the same index instead of wrapping.
//! * The ticket link's `<a>` wrapper is not represented; the ticket text is a plain run.
//! * The kanban `<text y="-10.1">` baseline trick lives in `createText.ts` and is absorbed by
//!   the scene's text runs; positions follow the label groups upstream transforms.
//! * The SVG size attributes (`useMaxWidth`) have no scene equivalent; the view box matches
//!   `setupGraphViewbox` with `mindmap.padding`.
//! * Icon metadata adds a label background (`createFormattedText`'s `addSvgBackground`) and
//!   the four pixels it adds to the measured title box; the icon image itself is not drawn
//!   by `kanbanItem.ts`.

use crate::config::Config;
use crate::error::ParseError;
use crate::measure::{FallbackMeasure, Font, TextError, TextMeasure, TextOptions, shape_text};
use crate::scene::{
    Color, Group, Item, Path, PathShape, PathStyle, Point, Rect, RectShape, Scene, SourceRef,
    TextBlock, TextRun, Transform,
};
use crate::theme::color;

use super::ast::{Ast, DataNode};

/// The diagram id `render` prefixes DOM ids with when the caller names none, matching
/// `render.ts`'s prefixing in the other unified renderers.
pub const DEFAULT_DIAGRAM_ID: &str = "mermaid";

/// `kanbanRenderer.ts`'s local `padding` const.
const ITEM_PADDING: f64 = 10.0;

/// `defaultConfig.kanban.sectionWidth`.
const DEFAULT_SECTION_WIDTH: f64 = 200.0;

/// `kanbanRenderer.ts`'s initial `maxLabelHeight`.
const INITIAL_MAX_LABEL_HEIGHT: f64 = 25.0;

/// `kanbanItem.ts`'s `labelPaddingX` and `labelPaddingY`.
const LABEL_PADDING_X: f64 = 10.0;
const LABEL_PADDING_Y: f64 = 10.0;

/// `kanbanItem.ts`'s default corner radius (`rx ?? 5`).
const DEFAULT_RADIUS: f64 = 5.0;

/// `kanbanItem.ts`'s priority stripe width.
const PRIORITY_STROKE_WIDTH: f64 = 4.0;

/// `THEME_COLOR_LIMIT`'s default: the number of `.section-N` rules `genSections` writes.
const DEFAULT_THEME_COLOR_LIMIT: usize = 12;

/// SVG's initial `fill`, which a section rectangle keeps when no `.section-N` rule matches.
const SVG_DEFAULT_FILL: &str = "#000000";

/// The font the upstream harness measures with: mermaid's SVG labels use the configured
/// monospace family at the default 16px size.
fn font() -> Font {
    Font::new("monospace", 16.0)
}

/// Renders a kanban body (the text [`super::parse`] accepts) to a [`Scene`].
///
/// `config` is the diagram configuration after the source's frontmatter and directives have
/// been applied, as `getConfig()` returns it to `draw`. `diagram_id` is prefixed to every
/// DOM id upstream mirrors in the scene.
pub fn draw_with_id(
    body: &str,
    config: &Config,
    measure: &dyn TextMeasure,
    diagram_id: &str,
) -> Result<Scene, ParseError> {
    let ast = super::parse(body)?;
    draw_ast_with_id(&ast, config, measure, diagram_id)
}

/// Renders an already parsed kanban diagram.
pub fn draw_ast_with_id(
    ast: &Ast,
    config: &Config,
    measure: &dyn TextMeasure,
    diagram_id: &str,
) -> Result<Scene, ParseError> {
    let data = ast.data();
    // `conf?.kanban?.sectionWidth || 200`: `0` is the only falsy value.
    let section_width = if config.kanban.section_width == 0.0 {
        DEFAULT_SECTION_WIDTH
    } else {
        config.kanban.section_width
    };
    let item_width = section_width - 1.5 * ITEM_PADDING;
    let diagram_font = font();

    let sections: Vec<&DataNode> = data.iter().filter(|node| node.is_group).collect();
    let mut measured_sections = Vec::with_capacity(sections.len());
    let mut max_label_height = INITIAL_MAX_LABEL_HEIGHT;
    for (index, section) in sections.iter().enumerate() {
        let count = index + 1;
        let x = section_width * count as f64 + ((count - 1) as f64 * ITEM_PADDING) / 2.0;
        let block = measure_text(&section.label, section_width, &diagram_font, measure)?;
        max_label_height = max_label_height.max(block.size.height);
        measured_sections.push(MeasuredSection {
            node: section,
            count,
            x,
            label: block,
        });
    }

    // `getSubGraphTitleMargins`' `subGraphTitleTopMargin`, which `kanbanSection` adds to the
    // label's y.
    let title_margin = config.flowchart.sub_graph_title_margin.top;
    let top = (-section_width * 3.0) / 2.0 + max_label_height;
    let mut sections_group = Group::new().with_class("sections");
    let mut items_group = Group::new().with_class("items");
    let mut bounds: Option<Rect> = None;
    let mut measured_items = Vec::new();

    for section in &measured_sections {
        let section_items: Vec<&DataNode> = data
            .iter()
            .filter(|node| !node.is_group && node.parent_id.as_deref() == Some(&section.node.id))
            .collect();
        let mut cursor = top;
        for item in section_items {
            let title = measure_text(
                &item.label,
                item_width - LABEL_PADDING_X,
                &diagram_font,
                measure,
            )?;
            let ticket = item
                .ticket
                .as_deref()
                .filter(|ticket| !ticket.is_empty())
                .map(|ticket| {
                    measure_text(ticket, item_width - LABEL_PADDING_X, &diagram_font, measure)
                })
                .transpose()?;
            let assigned = item
                .assigned
                .as_deref()
                .filter(|assigned| !assigned.is_empty())
                .map(|assigned| {
                    measure_text(
                        assigned,
                        item_width - LABEL_PADDING_X,
                        &diagram_font,
                        measure,
                    )
                })
                .transpose()?;
            let height_adj = ticket
                .as_ref()
                .map(|ticket| ticket.size.height)
                .unwrap_or(0.0)
                .max(
                    assigned
                        .as_ref()
                        .map(|assigned| assigned.size.height)
                        .unwrap_or(0.0),
                )
                / 2.0;
            // `labelHelper`'s `addBackground = !!node.icon` widens the measured box by the
            // two pixels `createFormattedText` adds around the text on every side.
            let has_icon = item.icon.as_deref().is_some_and(|icon| !icon.is_empty());
            let title_height = title.size.height + if has_icon { 4.0 } else { 0.0 };
            let total_height = (title_height + LABEL_PADDING_Y * 2.0).max(0.0) + height_adj;
            let center_y = cursor + total_height / 2.0;
            cursor = center_y + total_height / 2.0 + ITEM_PADDING / 2.0;

            let source = SourceRef::node(item.id.clone());
            let mut item_group = Group::new()
                .with_class("node undefined")
                .with_transform(Transform::translate(section.x, center_y))
                .with_source(source.clone());
            item_group.id = Some(format!("{diagram_id}-{}", item.id));

            let total_width = item_width;
            let rect = Rect::new(
                -total_width / 2.0,
                -total_height / 2.0,
                total_width,
                total_height,
            );
            let (fill, stroke) = card_colors(config);
            item_group.push(
                RectShape::new(
                    rect,
                    radius(item.rx),
                    PathStyle {
                        fill: Some(Color::new(fill)),
                        stroke: Some(Color::new(stroke)),
                        stroke_width: 1.0,
                        ..PathStyle::default()
                    },
                )
                .with_source(source.clone()),
            );

            if !item.label.is_empty() {
                item_group.push(label_group(
                    Transform::translate(
                        ITEM_PADDING - total_width / 2.0,
                        -height_adj - title_height / 2.0,
                    ),
                    title,
                    &diagram_font,
                    item_text_color(config),
                    has_icon.then(|| label_background(config)),
                    source.clone(),
                ));
            }
            if let Some(ticket) = ticket {
                item_group.push(label_group(
                    Transform::translate(
                        ITEM_PADDING - total_width / 2.0,
                        -height_adj + title_height / 2.0,
                    ),
                    ticket,
                    &diagram_font,
                    item_text_color(config),
                    None,
                    source.clone(),
                ));
            }
            if let Some(assigned) = assigned {
                item_group.push(label_group(
                    Transform::translate(
                        ITEM_PADDING + total_width / 2.0
                            - assigned.size.width
                            - 2.0 * LABEL_PADDING_X,
                        -height_adj + title_height / 2.0,
                    ),
                    assigned,
                    &diagram_font,
                    item_text_color(config),
                    None,
                    source.clone(),
                ));
            }

            if item.priority.as_ref().is_some_and(js_truthy) {
                let priority_color = item
                    .priority
                    .as_ref()
                    .and_then(priority_color)
                    .map(Color::new);
                let x1 = rect.x + 2.0;
                let y1 = rect.y + (radius(item.rx) / 2.0).floor();
                let y2 = rect.y + total_height - (radius(item.rx) / 2.0).floor();
                let mut path = Path::new();
                path.move_to(Point::new(x1, y1));
                path.line_to(Point::new(x1, y2));
                item_group.push(
                    PathShape::new(
                        path,
                        PathStyle {
                            fill: None,
                            stroke: priority_color,
                            stroke_width: PRIORITY_STROKE_WIDTH,
                            ..PathStyle::default()
                        },
                    )
                    .with_source(source.clone()),
                );
            }

            let item_bounds = Rect::new(
                section.x + rect.x,
                center_y + rect.y,
                total_width,
                total_height,
            );
            bounds = Some(bounds.map_or(item_bounds, |bounds| bounds.union(item_bounds)));
            measured_items.push(item_group);
        }

        let height = (cursor - top + 3.0 * ITEM_PADDING).max(50.0)
            + (max_label_height - INITIAL_MAX_LABEL_HEIGHT);
        let mut section_group = Group::new()
            .with_class(format!("cluster undefined section-{}", section.count))
            .with_source(SourceRef::cluster(section.node.id.clone()));
        section_group.id = Some(format!("{diagram_id}-{}", section.node.id));
        let section_rect = Rect::new(
            section.x - section_width / 2.0,
            -section_width * 3.0 / 2.0,
            section_width,
            height,
        );
        let (fill, stroke) = section_colors(config, section.count);
        section_group.push(
            RectShape::new(
                section_rect,
                radius(None),
                PathStyle {
                    fill: Some(Color::new(fill)),
                    stroke: stroke.map(Color::new),
                    stroke_width: 1.0,
                    ..PathStyle::default()
                },
            )
            .with_source(SourceRef::cluster(section.node.id.clone())),
        );

        let label_width = section.label.size.width;
        let label_height = section.label.size.height;
        let label_y = -section_width * 3.0 / 2.0 + title_margin;
        let mut label = Group::new()
            .with_class("cluster-label")
            .with_transform(Transform::translate(section.x - label_width / 2.0, label_y))
            .with_source(SourceRef::cluster(section.node.id.clone()));
        label.push(
            TextRun::new(Point::ZERO, diagram_font.clone(), section.label.clone())
                .with_fill(Color::new(section_text_color(config, section.count)))
                .with_source(SourceRef::cluster(section.node.id.clone())),
        );
        section_group.push(label);

        bounds = Some(bounds.map_or(section_rect, |bounds| bounds.union(section_rect)));
        // The section title can be taller than the label box's initial estimate; the SVG's
        // `getBBox()` includes it, so the view box must too.
        bounds = Some(bounds.unwrap_or_default().union(Rect::new(
            section.x - label_width / 2.0,
            label_y,
            label_width,
            label_height,
        )));
        sections_group.push(section_group);
    }

    for item in measured_items {
        items_group.push(item);
    }

    let bounds = bounds.unwrap_or_default();
    let mut scene = Scene::fitted(bounds, config.mindmap.padding);
    scene.push(sections_group);
    scene.push(items_group);
    Ok(scene)
}

/// Renders an already parsed kanban diagram.
pub fn draw_ast(
    ast: &Ast,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<Scene, ParseError> {
    draw_ast_with_id(ast, config, measure, DEFAULT_DIAGRAM_ID)
}

/// Renders a kanban body with the crate's monospace fallback measurement.
pub fn render(body: &str, config: &Config) -> Result<Scene, ParseError> {
    draw_with_id(body, config, &FallbackMeasure, DEFAULT_DIAGRAM_ID)
}

/// Renders an already parsed kanban diagram with the crate's monospace fallback measurement.
pub fn render_ast(ast: &Ast, config: &Config) -> Result<Scene, ParseError> {
    draw_ast(ast, config, &FallbackMeasure)
}

fn measure_text(
    text: &str,
    width: f64,
    font: &Font,
    measure: &dyn TextMeasure,
) -> Result<TextBlock, ParseError> {
    shape_text(
        text,
        &TextOptions {
            width,
            markdown: true,
            min_width: 0.0,
        },
        font,
        measure,
    )
    .map_err(text_error)
}

fn text_error(error: TextError) -> ParseError {
    ParseError::new(error.to_string(), None)
}

fn label_group(
    transform: Transform,
    block: TextBlock,
    font: &Font,
    fill: &str,
    background: Option<String>,
    source: SourceRef,
) -> Item {
    let mut group = Group::new()
        .with_class("label")
        .with_transform(transform)
        .with_source(source.clone());
    if let Some(background) = background {
        // `createFormattedText`'s `addSvgBackground` rect: two pixels beyond the text on
        // every side, which is also why an icon widens/tallens the measured title box.
        group.push(
            RectShape::new(
                Rect::new(-2.0, -2.0, block.size.width + 4.0, block.size.height + 4.0),
                0.0,
                PathStyle {
                    fill: Some(Color::new(background)),
                    stroke: None,
                    ..PathStyle::default()
                },
            )
            .with_source(source.clone()),
        );
    }
    group.push(
        TextRun::new(Point::ZERO, font.clone(), block)
            .with_fill(fill)
            .with_source(source),
    );
    Item::Group(group)
}

fn radius(rx: Option<f64>) -> f64 {
    rx.unwrap_or(DEFAULT_RADIUS)
}

fn theme_string<'a>(config: &'a Config, key: &str) -> Option<&'a str> {
    config
        .theme_variables
        .get(key)
        .and_then(serde_json::Value::as_str)
}

fn theme_bool(config: &Config, key: &str) -> Option<bool> {
    config
        .theme_variables
        .get(key)
        .and_then(serde_json::Value::as_bool)
}

/// `.section-N` selects `cScale{N+1}` (styles.ts writes `.section-${i - 1}` for `cScale{i}`),
/// adjusted like `genSections`' `adjuster`. Returns `None` for a section past the rules.
///
/// `genSections` loops `i` over `0..THEME_COLOR_LIMIT`, so `.section-N` exists while
/// `N + 1 < THEME_COLOR_LIMIT`. A section past that keeps SVG's defaults: a black fill, no
/// stroke, and the `.cluster-label` text colour.
fn section_palette(config: &Config, count: usize) -> Option<(String, String, String)> {
    let limit = config
        .theme_variables
        .get("THEME_COLOR_LIMIT")
        .and_then(serde_json::Value::as_f64)
        .map(|limit| limit.max(0.0) as usize)
        .unwrap_or(DEFAULT_THEME_COLOR_LIMIT);
    let index = count + 1;
    if index >= limit {
        return None;
    }
    let base = theme_string(config, &format!("cScale{index}")).unwrap_or("#ECECFF");
    let dark = theme_bool(config, "darkMode").unwrap_or(false);
    let adjusted = if dark {
        color::darken(base, 10.0)
    } else {
        color::lighten(base, 10.0)
    }
    .unwrap_or_else(|_| base.to_string());
    let label = theme_string(config, &format!("cScaleLabel{index}"))
        .unwrap_or("#333")
        .to_string();
    Some((adjusted.clone(), adjusted, label))
}

fn section_colors(config: &Config, count: usize) -> (String, Option<String>) {
    match section_palette(config, count) {
        Some((fill, stroke, _)) => (fill, Some(stroke)),
        None => (SVG_DEFAULT_FILL.to_string(), None),
    }
}

fn section_text_color(config: &Config, count: usize) -> String {
    match section_palette(config, count) {
        Some((_, _, label)) => label,
        None => item_text_color(config).to_string(),
    }
}

fn card_colors(config: &Config) -> (String, String) {
    (
        theme_string(config, "background")
            .unwrap_or("#FFFFFF")
            .to_string(),
        theme_string(config, "nodeBorder")
            .unwrap_or("#9370DB")
            .to_string(),
    )
}

fn item_text_color(config: &Config) -> &str {
    theme_string(config, "textColor").unwrap_or("#333")
}

fn label_background(config: &Config) -> String {
    // `createFormattedText`'s background `<rect>` sits inside the `.node` group, so
    // `.node rect` fills it with the theme's `background`; the helper only pins its stroke
    // to none. `labelBackground` is for edge labels, not this one.
    theme_string(config, "background")
        .unwrap_or("#FFFFFF")
        .to_string()
}

/// `colorFromPriority` in `kanbanItem.ts`.
fn priority_color(priority: &serde_json::Value) -> Option<String> {
    let priority = priority.as_str()?;
    let color = match priority {
        "Very High" => "red",
        "High" => "orange",
        "Medium" => return None,
        "Low" => "blue",
        "Very Low" => "lightblue",
        _ => return None,
    };
    Some(color.to_string())
}

/// JavaScript truthiness, as `if (priority)` tests it.
fn js_truthy(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => false,
        serde_json::Value::Bool(value) => *value,
        serde_json::Value::Number(value) => value.as_f64().is_some_and(|value| value != 0.0),
        serde_json::Value::String(value) => !value.is_empty(),
        serde_json::Value::Array(value) => !value.is_empty(),
        serde_json::Value::Object(_) => true,
    }
}

struct MeasuredSection<'a> {
    node: &'a DataNode,
    count: usize,
    x: f64,
    label: TextBlock,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const TOLERANCE: f64 = 0.01;

    fn fixtures() -> Vec<(String, Value)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/diagrams/kanban/tests/render");
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

    fn rect_of(value: &Value) -> (f64, f64, f64, f64) {
        (
            value["x"].as_f64().expect("x"),
            value["y"].as_f64().expect("y"),
            value["width"].as_f64().expect("width"),
            value["height"].as_f64().expect("height"),
        )
    }

    fn group_items<'a>(group: &'a Group, class: &str) -> Vec<&'a Group> {
        group
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Group(group) if group.class.as_deref().is_some_and(|c| c.contains(class)) => {
                    Some(group)
                }
                _ => None,
            })
            .collect()
    }

    fn first_rect(group: &Group) -> Option<&RectShape> {
        group.items.iter().find_map(|item| match item {
            Item::Rect(rect) => Some(rect),
            Item::Group(group) => first_rect(group),
            _ => None,
        })
    }

    fn first_text(group: &Group) -> Option<&TextRun> {
        group.items.iter().find_map(|item| match item {
            Item::Text(text) => Some(text),
            Item::Group(group) => first_text(group),
            _ => None,
        })
    }

    fn group_id(group: &Group) -> Option<&str> {
        group.id.as_deref()
    }

    fn lines_of(run: &TextRun) -> String {
        run.lines
            .iter()
            .map(|line| line.text())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn check_label(group: &Group, expected: &Value, file: &str, what: &str) {
        let transform = expected["transform"].as_object().expect("label transform");
        close(
            group.transform.e,
            transform["x"].as_f64().expect("label x"),
            &format!("{what} label x"),
            file,
        );
        close(
            group.transform.f,
            transform["y"].as_f64().expect("label y"),
            &format!("{what} label y"),
            file,
        );
        let text = expected["text"].as_object().expect("label text");
        let run = first_text(group).unwrap_or_else(|| panic!("{file}: {what} text run"));
        close(
            run.size.width,
            text["width"].as_f64().expect("text width"),
            &format!("{what} text width"),
            file,
        );
        close(
            run.size.height,
            text["height"].as_f64().expect("text height"),
            &format!("{what} text height"),
            file,
        );
        assert_eq!(
            lines_of(run),
            text["lines"]
                .as_array()
                .expect("text lines")
                .iter()
                .map(|line| line.as_str().expect("line"))
                .collect::<Vec<_>>()
                .join("\n"),
            "{file}: {what} text lines"
        );
    }

    #[test]
    fn renders_recorded_kanban_fixtures() {
        let fixtures = fixtures();
        assert!(
            fixtures.len() >= 8,
            "expected at least 8 fixtures, found {}",
            fixtures.len()
        );

        for (file, fixture) in fixtures {
            let source = fixture["source"].as_str().expect("fixture source");
            let body = crate::preprocess::preprocess(source).body;
            let scene = render(&body, &Config::default())
                .unwrap_or_else(|error| panic!("{file}: render: {error}"));
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

            let sections_group = scene
                .items
                .iter()
                .find_map(|item| match item {
                    Item::Group(group) if group.class.as_deref() == Some("sections") => Some(group),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{file}: sections group"));
            let items_group = scene
                .items
                .iter()
                .find_map(|item| match item {
                    Item::Group(group) if group.class.as_deref() == Some("items") => Some(group),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{file}: items group"));

            for expected_section in expected["sections"].as_array().expect("sections") {
                let id = expected_section["id"].as_str().expect("section id");
                let group = sections_group
                    .items
                    .iter()
                    .find_map(|item| match item {
                        Item::Group(group) if group_id(group) == Some(id) => Some(group),
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("{file}: section {id}"));
                let rect = first_rect(group).unwrap_or_else(|| panic!("{file}: {id} rect"));
                let (x, y, width, height) = rect_of(&expected_section["rect"]);
                close(rect.rect.x, x, &format!("{id} rect x"), &file);
                close(rect.rect.y, y, &format!("{id} rect y"), &file);
                close(rect.rect.width, width, &format!("{id} rect width"), &file);
                close(
                    rect.rect.height,
                    height,
                    &format!("{id} rect height"),
                    &file,
                );
                close(
                    rect.rx,
                    expected_section["rect"]["rx"].as_f64().expect("rx"),
                    &format!("{id} rect rx"),
                    &file,
                );
                let label = group_items(group, "cluster-label");
                let label = label
                    .first()
                    .unwrap_or_else(|| panic!("{file}: {id} label"));
                check_label(label, &expected_section["label"], &file, id);
            }

            for expected_item in expected["items"].as_array().expect("items") {
                let id = expected_item["id"].as_str().expect("item id");
                let group = items_group
                    .items
                    .iter()
                    .find_map(|item| match item {
                        Item::Group(group) if group_id(group) == Some(id) => Some(group),
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("{file}: item {id}"));
                let translate = expected_item["transform"].as_object().expect("transform");
                let tx = translate["x"].as_f64().expect("item x");
                let ty = translate["y"].as_f64().expect("item y");
                close(group.transform.e, tx, &format!("{id} transform x"), &file);
                close(group.transform.f, ty, &format!("{id} transform y"), &file);

                let rect = first_rect(group).unwrap_or_else(|| panic!("{file}: {id} rect"));
                let (x, y, width, height) = rect_of(&expected_item["rect"]);
                close(rect.rect.x, x, &format!("{id} rect x"), &file);
                close(rect.rect.y, y, &format!("{id} rect y"), &file);
                close(rect.rect.width, width, &format!("{id} rect width"), &file);
                close(
                    rect.rect.height,
                    height,
                    &format!("{id} rect height"),
                    &file,
                );
                close(
                    rect.rx,
                    expected_item["rect"]["rx"].as_f64().expect("rx"),
                    &format!("{id} rect rx"),
                    &file,
                );

                let labels = group_items(group, "label");
                let expected_labels = expected_item["labels"].as_array().expect("labels");
                assert_eq!(
                    labels.len(),
                    expected_labels.len(),
                    "{file}: {id} label count"
                );
                for (index, expected_label) in expected_labels.iter().enumerate() {
                    check_label(
                        labels[index],
                        expected_label,
                        &file,
                        &format!("{id} label {index}"),
                    );
                }

                if let Some(expected_line) =
                    expected_item.get("priorityLine").and_then(Value::as_object)
                {
                    let path = group
                        .items
                        .iter()
                        .find_map(|item| match item {
                            Item::Path(path) => Some(path),
                            _ => None,
                        })
                        .unwrap_or_else(|| panic!("{file}: {id} priority line"));
                    let commands = &path.path.commands;
                    let (x1, y1) = match commands.first() {
                        Some(crate::scene::PathCommand::MoveTo { x, y }) => (*x, *y),
                        other => panic!("{file}: {id} priority start {other:?}"),
                    };
                    let (x2, y2) = match commands.get(1) {
                        Some(crate::scene::PathCommand::LineTo { x, y }) => (*x, *y),
                        other => panic!("{file}: {id} priority end {other:?}"),
                    };
                    close(
                        x1,
                        expected_line["x1"].as_f64().expect("x1"),
                        &format!("{id} line x1"),
                        &file,
                    );
                    close(
                        y1,
                        expected_line["y1"].as_f64().expect("y1"),
                        &format!("{id} line y1"),
                        &file,
                    );
                    close(
                        x2,
                        expected_line["x2"].as_f64().expect("x2"),
                        &format!("{id} line x2"),
                        &file,
                    );
                    close(
                        y2,
                        expected_line["y2"].as_f64().expect("y2"),
                        &format!("{id} line y2"),
                        &file,
                    );
                    assert_eq!(
                        path.style.stroke.as_ref().map(|color| color.0.as_str()),
                        expected_line["stroke"].as_str(),
                        "{file}: {id} priority stroke"
                    );
                }
            }
        }
    }

    #[test]
    fn renders_a_kanban_body_with_a_view_box() {
        let scene = render("kanban\n  Todo\n    Task\n", &Config::default()).expect("render");
        assert!(scene.view_box.width > 0.0);
        assert!(scene.view_box.height > 0.0);
        assert!(!scene.items.is_empty());
    }

    #[test]
    fn rejects_a_body_without_a_kanban_header() {
        assert!(render("not a kanban", &Config::default()).is_err());
    }

    #[test]
    fn maps_priority_stripes_like_upstream() {
        use serde_json::json;
        assert_eq!(priority_color(&json!("Very High")).as_deref(), Some("red"));
        assert_eq!(priority_color(&json!("High")).as_deref(), Some("orange"));
        assert_eq!(priority_color(&json!("Medium")), None);
        assert_eq!(priority_color(&json!("Low")).as_deref(), Some("blue"));
        assert_eq!(
            priority_color(&json!("Very Low")).as_deref(),
            Some("lightblue")
        );
        assert_eq!(priority_color(&json!("high")), None);
        assert_eq!(priority_color(&json!(5)), None);
    }

    #[test]
    fn maps_sections_to_the_theme_palette() {
        let config = Config::default();
        let scale = theme_string(&config, "cScale2").expect("cScale2");
        let expected = color::lighten(scale, 10.0).expect("lighten");
        assert_eq!(
            section_colors(&config, 1),
            (expected.clone(), Some(expected))
        );
        assert_eq!(
            section_text_color(&config, 1),
            theme_string(&config, "cScaleLabel2").expect("cScaleLabel2")
        );

        // `genSections` writes `.section-0` through `.section-10`; `.section-11` has no rule.
        assert!(section_palette(&config, 10).is_some());
        assert!(section_palette(&config, 11).is_none());
        assert_eq!(
            section_colors(&config, 11),
            (SVG_DEFAULT_FILL.to_string(), None)
        );
        assert_eq!(section_text_color(&config, 11), item_text_color(&config));
    }

    #[test]
    fn darkens_the_section_palette_only_when_the_theme_variables_say_dark() {
        let mut config = Config::default();
        let mut directive = serde_json::Map::new();
        directive.insert("theme".to_string(), serde_json::json!("dark"));
        config.apply(&directive).expect("dark theme");
        // A dark theme's variables do not set `darkMode` themselves, and `genSections`
        // reads only that flag, so the palette still lightens.
        let scale = theme_string(&config, "cScale2").expect("cScale2");
        let expected = color::lighten(scale, 10.0).expect("lighten");
        assert_eq!(section_colors(&config, 1).0, expected);

        let mut config = Config::default();
        let mut directive = serde_json::Map::new();
        directive.insert("theme".to_string(), serde_json::json!("dark"));
        directive.insert(
            "themeVariables".to_string(),
            serde_json::json!({ "darkMode": true }),
        );
        config.apply(&directive).expect("darkMode override");
        let scale = theme_string(&config, "cScale2").expect("cScale2");
        let expected = color::darken(scale, 10.0).expect("darken");
        assert_eq!(section_colors(&config, 1).0, expected);
    }

    #[test]
    fn moves_section_titles_by_the_subgraph_title_margin() {
        let mut config = Config::default();
        config.flowchart.sub_graph_title_margin.top = 7.0;
        let scene = render("kanban\n  Todo\n    Task\n", &config).expect("render");
        let section = scene
            .items
            .iter()
            .find_map(|item| match item {
                Item::Group(group) if group.class.as_deref() == Some("sections") => Some(group),
                _ => None,
            })
            .and_then(|group| group_items(group, "cluster").into_iter().next())
            .expect("section");
        let label = group_items(section, "cluster-label")
            .into_iter()
            .next()
            .expect("section label");
        close(label.transform.f, -300.0 + 7.0, "label y", "margin");
    }
}
