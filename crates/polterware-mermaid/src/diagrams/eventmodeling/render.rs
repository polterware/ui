//! Source-to-[`Scene`] rendering for event modeling diagrams: the Rust side of
//! `diagrams/eventmodeling/db.ts` `getState` and `renderer.ts` `draw`.
//!
//! An event modeling diagram does not go through the unified layout pipeline: `draw` reads a
//! state the database projects frame by frame and paints it with plain d3. This module ports
//! that projection and that painter. [`draw`] parses, [`draw_ast`] renders an already parsed
//! database, and [`render`] uses the crate's monospace fallback measurement.
//!
//! `getState` folds every frame through a decider (`decidePositionFrame`,
//! `decidePositionRelation`) and an evolver (`evolveFramePositioned`,
//! `evolveRelationPositioned`). A frame becomes a `Box` in a `Swimlane`; the swimlane is
//! chosen by the frame's entity type, and a namespaced entity starts a new swimlane whose
//! label carries the namespace. Frames with explicit `->>` sources draw a relation per source;
//! a frame without sources draws one from the previous frame in a different swimlane, except
//! the first frame and reset frames, which draw none. Boxes flow left to right per swimlane
//! (`contentStartX`, `boxOverlap`, `boxPadding`), and a second pass stacks the swimlanes
//! vertically with `swimlaneGap`.
//!
//! `draw` then appends, in order, a rounded `em-swimlane` group per swimlane (background rect
//! and bold label), an `em-box` group per box (rounded rect and a centered HTML label), one
//! `em-relation` path per relation (a straight line with the arrowhead marker), and the
//! `em-arrowhead` marker. This module draws the same items: [`RectShape`]s for the rects,
//! [`TextRun`]s for the labels (the HTML label of the `foreignObject` becomes styled lines), a
//! [`PathShape`] per relation with its end marker, and a [`MarkerDef`] for the arrowhead. The
//! look is `eventmodeling/renderer.ts`'s, painted with the design system's theme variables
//! (`emUiFill`, `emEventStroke`, `emSwimlaneBackgroundOdd`, `emArrowhead`, `emRelationStroke`,
//! and the `textColor` the root stylesheet gives the labels). The behavior and robustness --
//! reference inference, namespaced swimlanes, reset frames, and the frame-by-frame state --
//! follow `db.ts`; there is no gpui-kit counterpart for event modeling, so its rules are
//! `eventmodeling.spec.ts`'s.
//!
//! Text is measured in the `monospace` family, the rule the fixture harness renders with and
//! that [`FallbackMeasure`](crate::measure::FallbackMeasure) reproduces. Upstream's
//! `calculateTextDimensions` measures the HTML string literally (the `<b>` and `<code>` tags
//! included, break tags split into lines), and so does this port; that is what decides the box
//! sizes. The visible label, drawn from the same string, drops the tags into styling and
//! decodes `&nbsp;`.
//!
//! Where the port stops short of upstream: a [`Scene`] carries paints rather than selectors,
//! so the `em-*` class names become group classes and CSS declarations are resolved into the
//! shapes. `draw` paints neither the title nor the accessibility text, so the scene carries
//! neither. The arrowhead id upstream suffixes with the SVG id is the fixed `em-arrowhead`
//! here, because a scene is not tied to one SVG element id.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::common::sanitize::has_breaks;
use crate::config::Config;
use crate::error::ParseError;
use crate::measure::{Font, TextMeasure};
use crate::scene::{
    Group, Item, MarkerDef, MarkerOrient, MarkerRef, Path, PathShape, PathStyle, Point, Rect,
    RectShape, Scene, Size, SourceRef, TextBaseline, TextBlock, TextLine, TextRun, TextSpan,
};

use super::Ast;
use super::ast::FrameKind;

/// The id the arrowhead marker is defined and referenced by.
const MARKER_ID: &str = "em-arrowhead";
/// `diagramProps.swimlaneMinHeight`.
const SWIMLANE_MIN_HEIGHT: f64 = 70.0;
/// `diagramProps.swimlanePadding`.
const SWIMLANE_PADDING: f64 = 15.0;
/// `diagramProps.swimlaneGap`.
const SWIMLANE_GAP: f64 = 10.0;
/// `diagramProps.boxPadding`.
const BOX_PADDING: f64 = 10.0;
/// `diagramProps.boxOverlap`.
const BOX_OVERLAP: f64 = 90.0;
/// `diagramProps.boxMinWidth`.
const BOX_MIN_WIDTH: f64 = 80.0;
/// `diagramProps.boxMaxWidth`.
const BOX_MAX_WIDTH: f64 = 450.0;
/// `diagramProps.boxMinHeight`.
const BOX_MIN_HEIGHT: f64 = 80.0;
/// `diagramProps.boxMaxHeight`.
const BOX_MAX_HEIGHT: f64 = 750.0;
/// `diagramProps.contentStartX`.
const CONTENT_START_X: f64 = 250.0;
/// `diagramProps.textMaxWidth`, `450 - 2 * 10`.
const TEXT_MAX_WIDTH: f64 = 430.0;
/// `diagramProps.boxTextPadding`.
const BOX_TEXT_PADDING: f64 = 10.0;
/// `wrapLabelConfig.fontSize`.
const BOX_FONT_SIZE: f64 = 16.0;
/// `diagramProps.labelUiAutomation`.
const LABEL_UI_AUTOMATION: &str = "UI/Automation";
/// `diagramProps.labelUiAutomationPrefix`.
const LABEL_UI_AUTOMATION_PREFIX: &str = "UI/A: ";
/// `diagramProps.labelCommandReadModel`.
const LABEL_COMMAND_READ_MODEL: &str = "Command/Read Model";
/// `diagramProps.labelCommandReadModelPrefix`.
const LABEL_COMMAND_READ_MODEL_PREFIX: &str = "C/RM: ";
/// `diagramProps.labelEvents`.
const LABEL_EVENTS: &str = "Events";
/// `diagramProps.labelEventsPrefix`.
const LABEL_EVENTS_PREFIX: &str = "Stream: ";
/// The fallback box stroke and fill of an entity type upstream has no case for.
const UNKNOWN_FILL: &str = "red";
const UNKNOWN_STROKE: &str = "black";

/// Renders an event modeling body (the text [`super::parse`] accepts) to a [`Scene`].
pub fn draw(body: &str, config: &Config, measure: &dyn TextMeasure) -> Result<Scene, ParseError> {
    let ast = super::parse(body)?;
    Ok(draw_ast(&ast, config, measure))
}

/// Renders an already parsed event modeling database.
pub fn draw_ast(ast: &Ast, config: &Config, measure: &dyn TextMeasure) -> Scene {
    let state = state_of(ast, config, measure);
    let theme = &config.theme_variables;
    let padding = config.eventmodeling.padding;

    let text_color = theme_color(theme, "textColor").unwrap_or("#333");
    let swimlane_fill = theme_color(theme, "emSwimlaneBackgroundOdd").unwrap_or("rgb(250,250,250)");
    let swimlane_stroke =
        theme_color(theme, "emSwimlaneBackgroundStroke").unwrap_or("rgb(240,240,240)");
    let arrowhead_color = theme_color(theme, "emArrowhead").unwrap_or("#000000");
    let relation_stroke = theme_color(theme, "emRelationStroke").unwrap_or("#000");

    let text_font = Font::new("monospace", BOX_FONT_SIZE);
    let mut items: Vec<Item> = Vec::new();
    let mut bounds = Rect::ZERO;
    let mut has_bounds = false;

    for swimlane in &state.sorted_swimlanes {
        let rect = Rect::new(
            0.0,
            swimlane.y,
            state.max_r + SWIMLANE_PADDING,
            swimlane.height,
        );
        let mut group = Group::new()
            .with_class("em-swimlane")
            .with_source(SourceRef::cluster(format!("swimlane-{}", swimlane.index)));
        group.push(RectShape::new(
            rect,
            3.0,
            PathStyle::filled(swimlane_fill).with_stroke(swimlane_stroke, 1.0),
        ));
        let label_block = TextBlock::new(
            vec![TextLine::new(vec![TextSpan::bold(swimlane.label.clone())])],
            Size::new(
                measure.width(&swimlane.label, &text_font),
                measure.line_height(&text_font),
            ),
            1.1 * BOX_FONT_SIZE,
        );
        group.push(
            TextRun::new(
                Point::new(30.0, swimlane.y + 30.0),
                text_font.clone().bold(),
                label_block,
            )
            .with_fill(text_color),
        );
        items.push(Item::Group(group));
        bounds = union(bounds, rect, &mut has_bounds);
    }

    for event_box in &state.boxes {
        let swimlane = &state.swimlanes[&event_box.swimlane_index];
        let rect = Rect::new(
            event_box.x,
            swimlane.y + SWIMLANE_PADDING,
            event_box.dimension.width,
            event_box.dimension.height,
        );
        let mut group = Group::new()
            .with_class("em-box")
            .with_source(SourceRef::node(event_box.frame_name.clone()));
        let style = PathStyle::default()
            .with_fill(event_box.visual.fill.as_str())
            .with_stroke(event_box.visual.stroke.as_str(), 1.0);
        group.push(RectShape::new(rect, 3.0, style));

        let lines = label_lines(&event_box.text);
        let block = text_block(&lines, &text_font, measure);
        let mut label = TextRun::centered(rect.center(), text_font.clone(), block)
            .with_fill(text_color)
            .with_source(SourceRef::node(event_box.frame_name.clone()));
        label.baseline = TextBaseline::Middle;
        group.push(label);
        items.push(Item::Group(group));
        bounds = union(bounds, rect, &mut has_bounds);
    }

    for relation in &state.relations {
        let (source, target) = relation_points(&state, relation);
        let shape = PathShape::new(
            Path::polyline([source, target]),
            PathStyle::stroked(relation_stroke, 1.0),
        )
        .with_source(SourceRef::edge(format!(
            "{}-{}",
            relation.source_frame, relation.target_frame
        )))
        .with_end_marker(MarkerRef::new(MARKER_ID).with_color(arrowhead_color));
        items.push(Item::Path(shape));
        bounds = union(
            bounds,
            Rect::new(
                source.x.min(target.x),
                source.y.min(target.y),
                (source.x - target.x).abs(),
                (source.y - target.y).abs(),
            ),
            &mut has_bounds,
        );
    }

    let mut scene = Scene::fitted(bounds, padding);
    scene.items = items;
    scene.define(arrowhead(arrowhead_color));
    scene
}

/// Renders an event modeling body with the crate's monospace fallback measurement.
pub fn render(body: &str, config: &Config) -> Result<Scene, ParseError> {
    draw(body, config, &crate::measure::FallbackMeasure)
}

fn union(bounds: Rect, rect: Rect, has_bounds: &mut bool) -> Rect {
    if *has_bounds {
        bounds.union(rect)
    } else {
        *has_bounds = true;
        rect
    }
}

/// The arrowhead marker of `draw`: `polygon 0 0, 10 3.5, 0 7`, 10 by 7, on the path end.
fn arrowhead(color: &str) -> MarkerDef {
    let mut marker = MarkerDef::new(MARKER_ID, Rect::new(0.0, 0.0, 10.0, 7.0))
        .with_reference(Point::new(10.0, 3.5))
        .with_size(10.0, 7.0)
        .with_orient(MarkerOrient::Auto);
    marker.push(PathShape::new(
        Path::polygon([
            Point::new(0.0, 0.0),
            Point::new(10.0, 3.5),
            Point::new(0.0, 7.0),
        ]),
        PathStyle::filled(color),
    ));
    marker
}

/// The visible lines of a box's HTML label: tags become styling, `<br>` breaks lines, and
/// `&nbsp;` decodes to a space.
fn label_lines(html: &str) -> Vec<TextLine> {
    let mut lines = Vec::new();
    for raw in crate::measure::split_breaks(html) {
        let bold = raw.contains("<b>");
        let code = raw.contains("<code");
        let text = strip_tags(&raw)
            .replace("&nbsp;", " ")
            .replace("&amp;", "&");
        let span = if bold {
            TextSpan::bold(text)
        } else if code {
            TextSpan::code(text)
        } else {
            TextSpan::plain(text)
        };
        lines.push(TextLine::new(vec![span]));
    }
    lines
}

/// Drops every HTML tag, keeping the text between tags.
fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        if in_tag {
            if ch == '>' {
                in_tag = false;
            }
        } else if ch == '<' {
            in_tag = true;
        } else {
            out.push(ch);
        }
    }
    out
}

/// Measures a label's lines into the block a painter centers in the box.
fn text_block(lines: &[TextLine], font: &Font, measure: &dyn TextMeasure) -> TextBlock {
    let width = lines
        .iter()
        .map(|line| measure.line_width(line, font))
        .fold(0.0, f64::max);
    let line_height = 1.1 * font.size;
    let height = line_height * lines.len() as f64;
    TextBlock::new(lines.to_vec(), Size::new(width, height), line_height)
}

/// What `getState` projects: the boxes, swimlanes, and relations `draw` paints.
struct State {
    boxes: Vec<Box>,
    swimlanes: BTreeMap<usize, Swimlane>,
    relations: Vec<Relation>,
    previous_swimlane_number: Option<usize>,
    max_r: f64,
    sorted_swimlanes: Vec<Swimlane>,
}

/// One positioned frame (`Box` in `types.ts`).
#[derive(Clone)]
struct Box {
    x: f64,
    r: f64,
    dimension: Size,
    swimlane_index: usize,
    visual: Visual,
    /// The HTML label, `<b>` and `<code>` included.
    text: String,
    frame_name: String,
}

/// One row (`Swimlane` in `types.ts`).
#[derive(Clone)]
struct Swimlane {
    index: usize,
    label: String,
    r: f64,
    y: f64,
    height: f64,
    max_height: f64,
}

/// A frame's fill and stroke.
#[derive(Clone)]
struct Visual {
    fill: String,
    stroke: String,
}

/// One drawn relation. The endpoints are recomputed when the drawing happens, from the final
/// swimlane rows, as `renderD3Relation` does; the coordinates stored in a `Relation` upstream
/// are not the ones `draw` paints.
struct Relation {
    source_box: usize,
    target_box: usize,
    source_frame: String,
    target_frame: String,
}

/// The layout `getState` computes, with the text measurement it needs.
fn state_of(ast: &Ast, config: &Config, measure: &dyn TextMeasure) -> State {
    let font = Font::new("monospace", BOX_FONT_SIZE);
    let mut state = State {
        boxes: Vec::new(),
        swimlanes: BTreeMap::new(),
        relations: Vec::new(),
        previous_swimlane_number: None,
        max_r: 0.0,
        sorted_swimlanes: Vec::new(),
    };

    for (index, frame) in ast.frames.iter().enumerate() {
        let text = calculate_text_props(frame, &ast.data_entities, measure, &font);
        position_frame(&mut state, config, frame, text);
        position_relations(&mut state, ast, index);
    }

    state.sorted_swimlanes = state.swimlanes.values().cloned().collect();
    state
}

/// `calculateTextProps`: the HTML label and the measured size of its text.
fn calculate_text_props(
    frame: &super::ast::Frame,
    data_entities: &[super::ast::DataEntity],
    measure: &dyn TextMeasure,
    font: &Font,
) -> TextProps {
    let name = crate::common::sanitize::sanitize_text(&extract_name(&frame.entity_identifier));
    let wrapped_name = wrap_label(&name, TEXT_MAX_WIDTH, measure, font);
    let mut content = format!("<b>{wrapped_name}</b>");
    let mut to_html: Option<String> = None;

    if let Some(inline) = frame
        .data_inline_value
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        let value = js_substring_from(inline, index_of(inline, '{').map(|at| at + 1).unwrap_or(0));
        let value = js_substring_to(&value, last_index_of(&value, '}'));
        to_html = Some(html_text(&value, measure, font));
    }

    if let Some(entity) = frame
        .data_reference
        .as_deref()
        .and_then(|reference| data_entities.iter().find(|entity| entity.name == reference))
    {
        let start = index_of_str(&entity.data_block_value, "{\n")
            .map(|at| at + 2)
            .unwrap_or(1);
        let value = js_substring_from(&entity.data_block_value, start);
        let value = js_substring_to(&value, last_index_of(&value, '}'));
        let mut html = html_text(&value, measure, font);
        html.push_str("<br/>");
        to_html = Some(html);
    }

    let has_rendered_data = to_html.is_some();
    if let Some(html) = &to_html {
        content.push_str(&format!(
            "<br/><br/><code style=\"text-align: left; display: block;max-width:{TEXT_MAX_WIDTH}px\">{html}</code>"
        ));
    }

    let dimensions = measure.dimensions(&content, font);
    TextProps {
        content,
        width: if has_rendered_data {
            dimensions.width / 3.0
        } else {
            dimensions.width
        },
        height: dimensions.height,
    }
}

/// A data block's text after `sanitizeText`, `wrapLabel`, and the `&nbsp;` substitution.
fn html_text(value: &str, measure: &dyn TextMeasure, font: &Font) -> String {
    let sanitized = crate::common::sanitize::sanitize_text(value);
    wrap_label(&sanitized, TEXT_MAX_WIDTH, measure, font).replace(' ', "&nbsp;")
}

/// The label and measured size of a frame's text (`TextProps`).
struct TextProps {
    content: String,
    width: f64,
    height: f64,
}

/// `extractName`: the part after the namespace dot, or the whole identifier.
fn extract_name(identifier: &str) -> String {
    let parts: Vec<&str> = identifier.split('.').collect();
    if parts.len() == 2 {
        parts[1].to_string()
    } else {
        identifier.to_string()
    }
}

/// `extractNamespace`: the part before the namespace dot, when there is exactly one dot.
fn extract_namespace(identifier: &str) -> Option<&str> {
    let parts: Vec<&str> = identifier.split('.').collect();
    if parts.len() == 2 {
        Some(parts[0])
    } else {
        None
    }
}

/// JavaScript's `String.prototype.slice`-like `substring(start)`.
fn js_substring_from(text: &str, start: usize) -> String {
    text.chars().skip(start).collect()
}

/// JavaScript's `substring(0, end)`: a negative or zero `end` is empty.
fn js_substring_to(text: &str, end: Option<usize>) -> String {
    let Some(end) = end else {
        return String::new();
    };
    if end <= 1 {
        return String::new();
    }
    text.chars().take(end - 1).collect()
}

fn index_of(text: &str, needle: char) -> Option<usize> {
    text.chars().position(|ch| ch == needle)
}

fn last_index_of(text: &str, needle: char) -> Option<usize> {
    text.char_indices()
        .rev()
        .find(|(_, ch)| *ch == needle)
        .map(|(index, _)| index)
}

/// JavaScript's `indexOf(needle)` for a short string, as a character offset.
fn index_of_str(text: &str, needle: &str) -> Option<usize> {
    text.find(needle).map(|byte| text[..byte].chars().count())
}

/// `wrapLabel`: greedily wraps words to `max_width`, breaking a too-long word with hyphens.
fn wrap_label(label: &str, max_width: f64, measure: &dyn TextMeasure, font: &Font) -> String {
    if label.is_empty() {
        return String::new();
    }
    if has_breaks(label) {
        return label.to_string();
    }
    let words: Vec<&str> = label.split(' ').filter(|word| !word.is_empty()).collect();
    let mut completed: Vec<String> = Vec::new();
    let mut next_line = String::new();
    let count = words.len();
    for (index, word) in words.iter().enumerate() {
        let word_length = text_width(&format!("{word} "), measure, font);
        let next_line_length = text_width(&next_line, measure, font);
        if word_length > max_width {
            let (mut hyphenated, remaining) = break_string(word, max_width, measure, font);
            completed.push(next_line);
            completed.append(&mut hyphenated);
            next_line = remaining;
        } else if next_line_length + word_length >= max_width {
            completed.push(std::mem::take(&mut next_line));
            next_line = (*word).to_string();
        } else if next_line.is_empty() {
            next_line = (*word).to_string();
        } else {
            next_line.push(' ');
            next_line.push_str(word);
        }
        if index + 1 == count {
            completed.push(std::mem::take(&mut next_line));
        }
    }
    completed
        .into_iter()
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("<br/>")
}

/// `calculateTextWidth`: the rounded width `calculateTextDimensions` reports.
fn text_width(text: &str, measure: &dyn TextMeasure, font: &Font) -> f64 {
    measure.dimensions(text, font).width
}

/// `breakString`: splits one too-long word at the width, hyphenating every line but the last.
fn break_string(
    word: &str,
    max_width: f64,
    measure: &dyn TextMeasure,
    font: &Font,
) -> (Vec<String>, String) {
    let characters: Vec<char> = word.chars().collect();
    let mut lines = Vec::new();
    let mut current = String::new();
    for (index, character) in characters.iter().enumerate() {
        let next = format!("{current}{character}");
        if text_width(&next, measure, font) >= max_width {
            let is_last = characters.len() == index + 1;
            if is_last {
                lines.push(next);
            } else {
                lines.push(format!("{next}-"));
            }
            current.clear();
        } else {
            current = next;
        }
    }
    (lines, current)
}

/// `calculateSwimlaneProps`: the swimlane a frame belongs to.
fn swimlane_props(
    frame: &super::ast::Frame,
    swimlanes: &BTreeMap<usize, Swimlane>,
) -> (usize, String) {
    let namespace = extract_namespace(&frame.entity_identifier);
    let existing = find_swimlane_by_namespace(swimlanes, namespace);
    if let Some(swimlane) = existing {
        return (swimlane.index, swimlane.label.clone());
    }
    match frame.model_entity_type.as_str() {
        "ui" | "pcr" | "processor" => match namespace {
            Some(namespace) => (
                find_next_available_index(swimlanes, 0, 100),
                format!("{LABEL_UI_AUTOMATION_PREFIX}{namespace}"),
            ),
            None => (0, LABEL_UI_AUTOMATION.to_string()),
        },
        "rmo" | "readmodel" | "cmd" | "command" => match namespace {
            Some(namespace) => (
                find_next_available_index(swimlanes, 100, 200),
                format!("{LABEL_COMMAND_READ_MODEL_PREFIX}{namespace}"),
            ),
            None => (100, LABEL_COMMAND_READ_MODEL.to_string()),
        },
        _ => match namespace {
            Some(namespace) => (
                find_next_available_index(swimlanes, 200, 300),
                format!("{LABEL_EVENTS_PREFIX}{namespace}"),
            ),
            None => (200, LABEL_EVENTS.to_string()),
        },
    }
}

/// `findSwimlaneByNamespace`. Upstream never stores `namespace` on a swimlane, so this always
/// answers `None`; keeping the lookup is what makes a namespaced frame start its own row.
fn find_swimlane_by_namespace<'a>(
    _swimlanes: &'a BTreeMap<usize, Swimlane>,
    _namespace: Option<&str>,
) -> Option<&'a Swimlane> {
    None
}

/// `findNextAvailableIndex`: the slot after the highest index inside the band.
fn find_next_available_index(
    swimlanes: &BTreeMap<usize, Swimlane>,
    boundary_min: usize,
    boundary_max: usize,
) -> usize {
    swimlanes
        .keys()
        .copied()
        .filter(|index| *index > boundary_min && *index < boundary_max)
        .fold(boundary_min, usize::max)
        + 1
}

/// `calculateEntityVisualProps`: the theme fills and strokes by frame type.
fn visual_of(frame: &super::ast::Frame, config: &Config) -> Visual {
    let theme = &config.theme_variables;
    let color = |fill: &str, stroke: &str, fill_default: &str, stroke_default: &str| Visual {
        fill: theme_color(theme, fill).unwrap_or(fill_default).to_string(),
        stroke: theme_color(theme, stroke)
            .unwrap_or(stroke_default)
            .to_string(),
    };
    match frame.model_entity_type.as_str() {
        "ui" => color("emUiFill", "emUiStroke", "white", "#dbdada"),
        "pcr" | "processor" => color("emProcessorFill", "emProcessorStroke", "#edb3f6", "#b88cbf"),
        "rmo" | "readmodel" => color("emReadModelFill", "emReadModelStroke", "#d3f1a2", "#a3b732"),
        "cmd" | "command" => color("emCommandFill", "emCommandStroke", "#bcd6fe", "#679ac3"),
        "evt" | "event" => color("emEventFill", "emEventStroke", "#ffb778", "#c19a0f"),
        _ => Visual {
            fill: UNKNOWN_FILL.to_string(),
            stroke: UNKNOWN_STROKE.to_string(),
        },
    }
}

fn theme_color<'a>(theme: &'a serde_json::Map<String, Value>, key: &str) -> Option<&'a str> {
    theme.get(key).and_then(Value::as_str)
}

/// `dispatch(state, PositionFrame)`: decide and evolve a frame's box.
fn position_frame(state: &mut State, config: &Config, frame: &super::ast::Frame, text: TextProps) {
    let visual = visual_of(frame, config);
    let dimension = Size::new(
        text.width + 2.0 * BOX_TEXT_PADDING,
        text.height + 2.0 * BOX_TEXT_PADDING,
    );
    evolve_frame_positioned(state, frame, &text.content, visual, dimension);
}

/// `evolveFramePositioned`: places the box in its swimlane and re-stacks the swimlanes.
fn evolve_frame_positioned(
    state: &mut State,
    frame: &super::ast::Frame,
    text: &str,
    visual: Visual,
    event_dimension: Size,
) {
    let (swimlane_index, label) = swimlane_props(frame, &state.swimlanes);
    let swimlane = state
        .swimlanes
        .entry(swimlane_index)
        .or_insert_with(|| Swimlane {
            index: swimlane_index,
            label,
            r: 0.0,
            y: swimlane_index as f64 * SWIMLANE_MIN_HEIGHT + SWIMLANE_GAP,
            height: SWIMLANE_MIN_HEIGHT,
            max_height: SWIMLANE_MIN_HEIGHT,
        });
    let swimlane = swimlane.clone();

    let last_box = state.boxes.last();
    let previous_swimlane = state
        .previous_swimlane_number
        .and_then(|number| state.swimlanes.get(&number))
        .cloned();

    let dimension = Size::new(
        event_dimension.width.clamp(BOX_MIN_WIDTH, BOX_MAX_WIDTH) + 2.0 * BOX_PADDING,
        event_dimension.height.clamp(BOX_MIN_HEIGHT, BOX_MAX_HEIGHT) + 2.0 * BOX_PADDING,
    );

    let x = calculate_x(&swimlane, previous_swimlane.as_ref(), last_box);
    let r = x + dimension.width + BOX_PADDING;
    let max_r = state
        .swimlanes
        .values()
        .map(|swimlane| swimlane.r)
        .chain(std::iter::once(r))
        .fold(f64::NEG_INFINITY, f64::max);

    if let Some(swimlane) = state.swimlanes.get_mut(&swimlane_index) {
        swimlane.r = x + dimension.width;
        swimlane.max_height = swimlane.max_height.max(dimension.height);
        swimlane.height = swimlane.max_height.max(SWIMLANE_MIN_HEIGHT) + 2.0 * SWIMLANE_PADDING;
    }

    state.boxes.push(Box {
        x,
        r,
        dimension,
        swimlane_index,
        visual,
        text: text.to_string(),
        frame_name: frame.name.clone(),
    });
    state.previous_swimlane_number = Some(swimlane_index);
    state.max_r = max_r;

    restack_swimlanes(state);
}

/// The swimlane `y` pass at the end of `evolveFramePositioned`.
fn restack_swimlanes(state: &mut State) {
    let indices: Vec<usize> = state.swimlanes.keys().copied().collect();
    for (position, index) in indices.iter().enumerate() {
        let y = if position == 0 {
            0.0
        } else {
            let previous = indices[position - 1];
            let previous_swimlane = &state.swimlanes[&previous];
            previous_swimlane.y + previous_swimlane.height + SWIMLANE_GAP
        };
        state.swimlanes.get_mut(index).expect("swimlane exists").y = y;
    }
}

/// `calculateX`: the box's left edge.
fn calculate_x(
    swimlane: &Swimlane,
    previous_swimlane: Option<&Swimlane>,
    last_box: Option<&Box>,
) -> f64 {
    let Some(previous_swimlane) = previous_swimlane else {
        return CONTENT_START_X;
    };
    if previous_swimlane.index == swimlane.index && swimlane.r != 0.0 {
        return swimlane.r + BOX_PADDING;
    }
    match last_box {
        None => CONTENT_START_X,
        Some(last_box) => last_box.r - BOX_OVERLAP + BOX_PADDING,
    }
}

/// `dispatch(state, PositionRelation)`: decides which frame feeds the current one and draws a
/// relation from each source.
fn position_relations(state: &mut State, ast: &Ast, index: usize) {
    let frame = &ast.frames[index];
    if index == 0 && frame.source_frames.is_empty() {
        return;
    }
    let Some(target_box) = state.boxes.get(index).cloned() else {
        return;
    };
    let target_swimlane = target_box.swimlane_index;
    let mut relations = Vec::new();

    if !frame.source_frames.is_empty() {
        let source_names: Vec<&str> = ast
            .frames
            .iter()
            .filter(|current| {
                frame
                    .source_frames
                    .iter()
                    .any(|source| source == &current.name)
            })
            .map(|current| current.name.as_str())
            .collect();
        for name in source_names {
            if let Some(source) = box_index_by_frame(&state.boxes, name) {
                relations.push(Relation {
                    source_box: source,
                    target_box: index,
                    source_frame: name.to_string(),
                    target_frame: frame.name.clone(),
                });
            }
        }
    } else if frame.kind != FrameKind::Reset
        && let Some(source) = box_index_by_line(&state.boxes, target_swimlane, index)
    {
        relations.push(Relation {
            source_box: source,
            target_box: index,
            source_frame: state.boxes[source].frame_name.clone(),
            target_frame: frame.name.clone(),
        });
    }

    state.relations.extend(relations);
}

fn box_index_by_frame(boxes: &[Box], name: &str) -> Option<usize> {
    boxes
        .iter()
        .position(|event_box| event_box.frame_name == name)
}

/// `findBoxByLineIndex`: the first earlier box in a different swimlane, starting at the target
/// box itself.
fn box_index_by_line(boxes: &[Box], target_swimlane: usize, line_index: usize) -> Option<usize> {
    let mut index = line_index;
    loop {
        let event_box = boxes.get(index)?;
        if event_box.swimlane_index != target_swimlane {
            return Some(index);
        }
        index = index.checked_sub(1)?;
    }
}

/// `renderD3Relation`'s endpoints: from the source box's two-thirds point to the target box's
/// one-third point, on the facing edges.
fn relation_points(state: &State, relation: &Relation) -> (Point, Point) {
    let source_box = &state.boxes[relation.source_box];
    let target_box = &state.boxes[relation.target_box];
    let source_swimlane = &state.swimlanes[&source_box.swimlane_index];
    let target_swimlane = &state.swimlanes[&target_box.swimlane_index];
    let source_box_y = source_swimlane.y + SWIMLANE_PADDING;
    let target_box_y = target_swimlane.y + SWIMLANE_PADDING;
    let upwards = source_box_y > target_box_y;

    let source_x = source_box.x + (source_box.dimension.width * 2.0) / 3.0;
    let target_x = target_box.x + target_box.dimension.width / 3.0;
    let (source_y, target_y) = if upwards {
        (source_box_y, target_box_y + target_box.dimension.height)
    } else {
        (source_box_y + source_box.dimension.height, target_box_y)
    };
    (
        Point::new(source_x, source_y),
        Point::new(target_x, target_y),
    )
}
