//! Source-to-[`Scene`] rendering for sequence diagrams: a line-by-line port of
//! upstream's `sequenceRenderer.ts` and `svgDraw.js` (Mermaid 12.3.0).
//!
//! Visual appearance mirrors `@polterware/frontend-design-system` sequence styling
//! and behaves like upstream's coordinate layout.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::error::ParseError;
use crate::measure::{FallbackMeasure, Font, TextDimensions, TextMeasure};
use crate::scene::{
    Color, EllipseShape, MarkerDef, MarkerOrient, MarkerRef, MarkerUnits, Path, PathShape,
    PathStyle, Point, Rect, RectShape, Scene, Size, SourceRef, TextBaseline, TextBlock, TextLine,
    TextRun, TextSpan,
};

use super::ast::{Ast, LineType, Message, MessageText, Placement};

/// Width of specialized actor types (actor, control, entity, database).
pub const ACTOR_TYPE_WIDTH: f64 = 36.0;
/// Height of the stick-figure actor glyph.
pub const ACTOR_GLYPH_HEIGHT: f64 = 65.0;

/// Default line-height advance in fallback text measurement (in em).
const LINE_INK_EM: f64 = 1.15;

/// What can stop a sequence diagram render.
#[derive(Clone, Debug, PartialEq)]
pub enum SequenceRenderError {
    Parse(ParseError),
}

impl std::fmt::Display for SequenceRenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for SequenceRenderError {}

impl From<ParseError> for SequenceRenderError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

/// Laid out actor element.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ActorLayout {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub stopy: f64,
}

/// Laid out box container element.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BoxLayout {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub startx: f64,
    pub starty: f64,
    pub stopx: f64,
    pub stopy: f64,
}

/// Laid out message element.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MessageLayout {
    pub id: String,
    pub from: String,
    pub to: String,
    #[serde(rename = "type")]
    pub line_type: u8,
    pub startx: f64,
    pub stopx: f64,
    pub starty: f64,
    pub stopy: f64,
    pub height: f64,
}

/// Laid out note element.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NoteLayout {
    pub message: String,
    pub startx: f64,
    pub stopx: f64,
    pub starty: f64,
    pub stopy: f64,
    pub width: f64,
    pub height: f64,
}

/// Laid out loop or block structure.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LoopLayout {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub startx: f64,
    pub stopx: f64,
    pub starty: f64,
    pub stopy: f64,
    pub height: f64,
}

/// Laid out edge element.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EdgeLayout {
    pub id: String,
    pub points: Vec<Point>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x1: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y1: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x2: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y2: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub d: Option<String>,
}

/// Laid out sequence diagram data.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SequenceLayoutData {
    pub actors: Vec<ActorLayout>,
    pub boxes: Vec<BoxLayout>,
    pub messages: Vec<MessageLayout>,
    pub notes: Vec<NoteLayout>,
    pub loops: Vec<LoopLayout>,
    pub edges: Vec<EdgeLayout>,
    pub view_box: Rect,
}

/// Internal actor rendering metadata.
#[derive(Clone, Debug)]
struct ActorModel {
    name: String,
    description: String,
    actor_type: String,
    wrap: bool,
    x: f64,
    starty: f64,
    stopy: Option<f64>,
    width: f64,
    height: f64,
    margin: f64,
    box_index: Option<usize>,
    prev_actor: Option<String>,
    next_actor: Option<String>,
    actor_text_height: f64,
}

/// Internal box rendering metadata.
#[derive(Clone, Debug)]
struct BoxModel {
    name: Option<String>,
    wrap: bool,
    fill: String,
    actor_keys: Vec<String>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    startx: f64,
    starty: f64,
    stopx: f64,
    stopy: f64,
    margin: f64,
    text_max_height: f64,
}

/// Internal activation item.
#[derive(Clone, Debug)]
struct Activation {
    startx: f64,
    starty: f64,
    stopx: f64,
    actor: String,
}

/// Internal loop item tracked during vertical layout.
#[derive(Clone, Debug)]
struct LoopItem {
    id: String,
    title: Option<String>,
    wrap: bool,
    width: Option<f64>,
    height: f64,
    fill: Option<String>,
    startx: Option<f64>,
    starty: f64,
    stopx: Option<f64>,
    stopy: Option<f64>,
    sections: Vec<LoopSection>,
    section_titles: Vec<String>,
    kind: String,
}

#[derive(Clone, Copy, Debug)]
struct LoopSection {
    y: f64,
    height: f64,
}

/// Internal bounds state ported from `bounds` in `sequenceRenderer.ts`.
#[derive(Default)]
struct BoundsState {
    startx: Option<f64>,
    stopx: Option<f64>,
    starty: Option<f64>,
    stopy: Option<f64>,
    vertical_pos: f64,
    saved_vertical_pos: f64,
    sequence_items: Vec<LoopItem>,
    activations: Vec<Activation>,
}

impl BoundsState {
    fn update_val(curr: &mut Option<f64>, val: f64, fun: fn(f64, f64) -> f64) {
        *curr = Some(match *curr {
            Some(existing) => fun(val, existing),
            None => val,
        });
    }

    fn update_bounds(&mut self, startx: f64, starty: f64, stopx: f64, stopy: f64, box_margin: f64) {
        let seq_len = self.sequence_items.len();
        for (i, item) in self.sequence_items.iter_mut().enumerate() {
            let n = (seq_len - i) as f64;
            item.starty = item.starty.min(starty - n * box_margin);
            Self::update_val(&mut item.stopy, stopy + n * box_margin, f64::max);
            Self::update_val(&mut self.startx, startx - n * box_margin, f64::min);
            Self::update_val(&mut self.stopx, stopx + n * box_margin, f64::max);
            Self::update_val(&mut item.startx, startx - n * box_margin, f64::min);
            Self::update_val(&mut item.stopx, stopx + n * box_margin, f64::max);
            Self::update_val(&mut self.starty, starty - n * box_margin, f64::min);
            Self::update_val(&mut self.stopy, stopy + n * box_margin, f64::max);
        }

        let act_len = self.activations.len();
        for (i, item) in self.activations.iter_mut().enumerate() {
            let n = (act_len - i) as f64;
            item.starty = item.starty.min(starty - n * box_margin);
            Self::update_val(&mut self.startx, startx - n * box_margin, f64::min);
            Self::update_val(&mut self.stopx, stopx + n * box_margin, f64::max);
        }
    }

    fn insert(&mut self, startx: f64, starty: f64, stopx: f64, stopy: f64, box_margin: f64) {
        let min_x = startx.min(stopx);
        let max_x = startx.max(stopx);
        let min_y = starty.min(stopy);
        let max_y = starty.max(stopy);

        Self::update_val(&mut self.startx, min_x, f64::min);
        Self::update_val(&mut self.starty, min_y, f64::min);
        Self::update_val(&mut self.stopx, max_x, f64::max);
        Self::update_val(&mut self.stopy, max_y, f64::max);

        self.update_bounds(min_x, min_y, max_x, max_y, box_margin);
    }

    fn bump_vertical_pos(&mut self, bump: f64) {
        self.vertical_pos += bump;
        Self::update_val(&mut self.stopy, self.vertical_pos, f64::max);
    }

    fn new_activation(&mut self, actor: &str, actors: &HashMap<String, ActorModel>, activation_width: f64) {
        let actor_rect = match actors.get(actor) {
            Some(a) => a,
            None => return,
        };
        let stacked_size = self.actor_activations(actor).len() as f64;
        let x = actor_rect.x + actor_rect.width / 2.0 + ((stacked_size - 1.0) * activation_width) / 2.0;
        self.activations.push(Activation {
            startx: x,
            starty: self.vertical_pos + 2.0,
            stopx: x + activation_width,
            actor: actor.to_string(),
        });
    }

    fn end_activation(&mut self, actor: &str) -> Option<Activation> {
        let pos = self.activations.iter().rposition(|a| a.actor == actor)?;
        Some(self.activations.remove(pos))
    }

    fn actor_activations(&self, actor: &str) -> Vec<&Activation> {
        self.activations.iter().filter(|a| a.actor == actor).collect()
    }

    fn activation_bounds(&self, actor: &str, actors: &HashMap<String, ActorModel>) -> (f64, f64) {
        let default_x = actors.get(actor).map(|a| a.x + a.width / 2.0).unwrap_or(0.0);
        let acts = self.actor_activations(actor);
        let left = acts
            .iter()
            .fold(default_x - 1.0, |acc, a| acc.min(a.startx));
        let right = acts
            .iter()
            .fold(default_x + 1.0, |acc, a| acc.max(a.stopx));
        (left, right)
    }

    fn new_loop(&mut self, id: String, title: Option<String>, wrap: bool, fill: Option<String>, kind: String) {
        self.sequence_items.push(LoopItem {
            id,
            title,
            wrap,
            width: None,
            height: 0.0,
            fill,
            startx: None,
            starty: self.vertical_pos,
            stopx: None,
            stopy: None,
            sections: Vec::new(),
            section_titles: Vec::new(),
            kind,
        });
    }

    fn end_loop(&mut self) -> Option<LoopItem> {
        self.sequence_items.pop()
    }

    fn add_section_to_loop(&mut self, message: &str) {
        if let Some(loop_item) = self.sequence_items.last_mut() {
            loop_item.sections.push(LoopSection {
                y: self.vertical_pos,
                height: 0.0,
            });
            loop_item.section_titles.push(message.to_string());
        }
    }
}

fn dims_size(dims: &TextDimensions) -> Size {
    Size::new(dims.width, dims.height)
}

fn num_or_str_f64(val: &crate::config::NumberOrString, default: f64) -> f64 {
    match val {
        crate::config::NumberOrString::Number(n) => *n,
        crate::config::NumberOrString::Text(s) => s
            .trim_end_matches(|c: char| !c.is_numeric() && c != '.')
            .parse()
            .unwrap_or(default),
    }
}

fn num_or_str_u16(val: &crate::config::NumberOrString, default: u16) -> u16 {
    match val {
        crate::config::NumberOrString::Number(n) => *n as u16,
        crate::config::NumberOrString::Text(s) => match s.to_ascii_lowercase().as_str() {
            "bold" => 700,
            "bolder" => 800,
            "lighter" => 300,
            "normal" => 400,
            _ => s.parse().unwrap_or(default),
        },
    }
}

fn actor_font(conf: &Config) -> Font {
    let family = if !conf.font_family.is_empty() {
        conf.font_family.clone()
    } else {
        conf.sequence.actor_font_family.clone()
    };
    let size = num_or_str_f64(&conf.sequence.actor_font_size, 14.0);
    let weight = num_or_str_u16(&conf.sequence.actor_font_weight, 400);
    Font::new(family, size).with_weight(weight)
}

fn message_font(conf: &Config) -> Font {
    let family = if !conf.font_family.is_empty() {
        conf.font_family.clone()
    } else {
        conf.sequence.message_font_family.clone()
    };
    let size = num_or_str_f64(&conf.sequence.message_font_size, 16.0);
    let weight = num_or_str_u16(&conf.sequence.message_font_weight, 400);
    Font::new(family, size).with_weight(weight)
}

fn note_font(conf: &Config) -> Font {
    let family = if !conf.font_family.is_empty() {
        conf.font_family.clone()
    } else {
        conf.sequence.note_font_family.clone()
    };
    let size = num_or_str_f64(&conf.sequence.note_font_size, 14.0);
    let weight = num_or_str_u16(&conf.sequence.note_font_weight, 400);
    Font::new(family, size).with_weight(weight)
}

/// Port of `breakString` from `utils.ts`.
pub fn break_string(
    word: &str,
    max_width: f64,
    hyphen: &str,
    font: &Font,
    measure: &dyn TextMeasure,
) -> (Vec<String>, String) {
    let chars: Vec<char> = word.chars().collect();
    let mut lines = Vec::new();
    let mut current_line = String::new();

    for (index, ch) in chars.iter().enumerate() {
        let mut next_line = current_line.clone();
        next_line.push(*ch);
        let line_width = measure.dimensions(&next_line, font).width;
        if line_width >= max_width {
            let is_last = index + 1 == chars.len();
            if is_last {
                lines.push(next_line);
            } else {
                lines.push(format!("{next_line}{hyphen}"));
            }
            current_line.clear();
        } else {
            current_line = next_line;
        }
    }

    (lines, current_line)
}

/// Port of `wrapLabel` from `utils.ts`.
pub fn wrap_label(label: &str, max_width: f64, font: &Font, measure: &dyn TextMeasure) -> String {
    if label.is_empty() {
        return String::new();
    }
    if crate::measure::split_breaks(label).len() > 1 {
        return label.to_string();
    }
    let words: Vec<&str> = label.split(' ').filter(|w| !w.is_empty()).collect();
    let mut completed_lines: Vec<String> = Vec::new();
    let mut next_line = String::new();

    for (index, word) in words.iter().enumerate() {
        let word_with_space = format!("{word} ");
        let word_length = measure.dimensions(&word_with_space, font).width;
        let next_line_length = if next_line.is_empty() {
            0.0
        } else {
            measure.dimensions(&next_line, font).width
        };

        if word_length > max_width {
            let (hyphenated, remaining) = break_string(word, max_width, "-", font, measure);
            if !next_line.is_empty() {
                completed_lines.push(next_line);
            }
            completed_lines.extend(hyphenated);
            next_line = remaining;
        } else if next_line_length + word_length >= max_width {
            if !next_line.is_empty() {
                completed_lines.push(next_line);
            }
            next_line = word.to_string();
        } else {
            if next_line.is_empty() {
                next_line = word.to_string();
            } else {
                next_line.push(' ');
                next_line.push_str(word);
            }
        }

        if index + 1 == words.len() && !next_line.is_empty() {
            completed_lines.push(next_line.clone());
        }
    }

    completed_lines
        .into_iter()
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("<br/>")
}

fn is_reverse_arrow_type(line_type: LineType) -> bool {
    matches!(
        line_type,
        LineType::SolidTopReverse
            | LineType::SolidTopReverseDotted
            | LineType::SolidBottomReverse
            | LineType::SolidBottomReverseDotted
            | LineType::StickTopReverse
            | LineType::StickTopReverseDotted
            | LineType::StickBottomReverse
            | LineType::StickBottomReverseDotted
    )
}

fn is_bidirectional_arrow_type(line_type: LineType) -> bool {
    matches!(
        line_type,
        LineType::BidirectionalSolid | LineType::BidirectionalDotted
    )
}

fn is_dotted_arrow_type(line_type: LineType) -> bool {
    matches!(
        line_type,
        LineType::Dotted
            | LineType::DottedCross
            | LineType::DottedPoint
            | LineType::DottedOpen
            | LineType::BidirectionalDotted
            | LineType::SolidTopDotted
            | LineType::SolidBottomDotted
            | LineType::StickTopDotted
            | LineType::StickBottomDotted
            | LineType::SolidTopReverseDotted
            | LineType::SolidBottomReverseDotted
            | LineType::StickTopReverseDotted
            | LineType::StickBottomReverseDotted
    )
}

fn has_central_connection(msg: &Message) -> bool {
    msg.central_connection == LineType::CentralConnection as u8
        || msg.central_connection == LineType::CentralConnectionReverse as u8
        || msg.central_connection == LineType::CentralConnectionDual as u8
}

fn calculate_central_connection_offset(msg: &Message, is_arrow_to_right: bool) -> f64 {
    let mut offset = 0.0;
    if msg.central_connection == LineType::CentralConnectionReverse as u8
        || msg.central_connection == LineType::CentralConnectionDual as u8
    {
        offset += 4.0;
    }
    if (msg.central_connection == LineType::CentralConnectionReverse as u8
        || msg.central_connection == LineType::CentralConnectionDual as u8)
        && (msg.line_type == LineType::BidirectionalSolid || msg.line_type == LineType::BidirectionalDotted)
    {
        offset += if is_arrow_to_right { 0.0 } else { -6.0 };
    }
    offset
}

/// Port of `getMaxMessageWidthPerActor` from `sequenceRenderer.ts`.
fn get_max_message_width_per_actor(
    actors: &HashMap<String, ActorModel>,
    messages: &[Message],
    conf: &Config,
    measure: &dyn TextMeasure,
) -> HashMap<String, f64> {
    let mut max_widths: HashMap<String, f64> = HashMap::new();
    let msg_font = message_font(conf);
    let n_font = note_font(conf);

    for msg in messages {
        let to_actor = msg.to.as_deref().and_then(|id| actors.get(id));
        let from_actor = msg.from.as_deref().and_then(|id| actors.get(id));

        if let (Some(to_act), Some(from_act)) = (to_actor, from_actor) {
            if msg.placement == Some(Placement::LeftOf) && to_act.prev_actor.is_none() {
                continue;
            }
            if msg.placement == Some(Placement::RightOf) && to_act.next_actor.is_none() {
                continue;
            }

            let is_note = msg.placement.is_some();
            let font = if is_note { &n_font } else { &msg_font };
            let raw_msg = msg.message.as_str();
            let wrapped = if msg.wrap {
                wrap_label(raw_msg, conf.sequence.width - 2.0 * conf.sequence.wrap_padding, font, measure)
            } else {
                raw_msg.to_string()
            };
            let dims = measure.dimensions(&wrapped, font);
            let message_width = dims.width + 2.0 * conf.sequence.wrap_padding;

            let to_key = msg.to.as_deref().unwrap_or("");
            let from_key = msg.from.as_deref().unwrap_or("");

            if !is_note && from_act.name == to_act.next_actor.as_deref().unwrap_or("") {
                let entry = max_widths.entry(to_key.to_string()).or_insert(0.0);
                *entry = entry.max(message_width);
            } else if !is_note && from_act.name == to_act.prev_actor.as_deref().unwrap_or("") {
                let entry = max_widths.entry(from_key.to_string()).or_insert(0.0);
                *entry = entry.max(message_width);
            } else if !is_note && from_act.name == to_act.name {
                let half = message_width / 2.0;
                let entry1 = max_widths.entry(from_key.to_string()).or_insert(0.0);
                *entry1 = entry1.max(half);
                let entry2 = max_widths.entry(to_key.to_string()).or_insert(0.0);
                *entry2 = entry2.max(half);
            } else if msg.placement == Some(Placement::RightOf) {
                let entry = max_widths.entry(from_key.to_string()).or_insert(0.0);
                *entry = entry.max(message_width);
            } else if msg.placement == Some(Placement::LeftOf) {
                if let Some(prev) = &to_act.prev_actor {
                    let entry = max_widths.entry(prev.clone()).or_insert(0.0);
                    *entry = entry.max(message_width);
                }
            } else if msg.placement == Some(Placement::Over) {
                if let Some(prev) = &to_act.prev_actor {
                    let entry = max_widths.entry(prev.clone()).or_insert(0.0);
                    *entry = entry.max(message_width / 2.0);
                }
                if to_act.next_actor.is_some() {
                    let entry = max_widths.entry(from_key.to_string()).or_insert(0.0);
                    *entry = entry.max(message_width / 2.0);
                }
            }
        }
    }

    max_widths
}

/// Port of `calculateActorMargins` from `sequenceRenderer.ts`.
fn calculate_actor_margins(
    actors: &mut HashMap<String, ActorModel>,
    actor_keys: &[String],
    max_widths: &HashMap<String, f64>,
    boxes: &mut [BoxModel],
    conf: &Config,
    measure: &dyn TextMeasure,
) -> f64 {
    let act_font = actor_font(conf);
    let mut max_height: f64 = 0.0;

    for key in actor_keys {
        if let Some(actor) = actors.get_mut(key) {
            if actor.wrap {
                actor.description = wrap_label(
                    &actor.description,
                    conf.sequence.width - 2.0 * conf.sequence.wrap_padding,
                    &act_font,
                    measure,
                );
            }
            let dims = measure.dimensions(&actor.description, &act_font);
            actor.width = if actor.wrap {
                conf.sequence.width
            } else {
                conf.sequence.width.max(dims.width + 2.0 * conf.sequence.wrap_padding)
            };

            if conf.look == "neo" {
                actor.actor_text_height = dims.height;
                let stack = 44.0 + 6.0 + dims.height + 6.0;
                actor.height = stack.max(conf.sequence.height);
            } else {
                actor.height = if actor.wrap {
                    dims.height.max(conf.sequence.height)
                } else {
                    conf.sequence.height
                };
            }
            max_height = max_height.max(actor.height);
        }
    }

    for (actor_key, message_width) in max_widths {
        let (actor_width, next_actor_key) = match actors.get(actor_key) {
            Some(a) => (a.width, a.next_actor.clone()),
            None => continue,
        };

        if let Some(next_key) = next_actor_key {
            if let Some(next_actor) = actors.get(&next_key) {
                let required = message_width + conf.sequence.actor_margin
                    - actor_width / 2.0
                    - next_actor.width / 2.0;
                if let Some(a) = actors.get_mut(actor_key) {
                    a.margin = required.max(conf.sequence.actor_margin);
                }
                continue;
            }
        }

        let required = message_width + conf.sequence.actor_margin - actor_width / 2.0;
        if let Some(a) = actors.get_mut(actor_key) {
            a.margin = required.max(conf.sequence.actor_margin);
        }
    }

    let mut max_box_height: f64 = 0.0;
    let msg_font = message_font(conf);
    for box_model in boxes.iter_mut() {
        let mut total_width: f64 = box_model
            .actor_keys
            .iter()
            .map(|k| {
                actors
                    .get(k)
                    .map(|a| a.width + a.margin)
                    .unwrap_or(0.0)
            })
            .sum();

        total_width += conf.sequence.box_margin * 8.0;
        total_width -= 2.0 * conf.sequence.box_text_margin;

        if let Some(name) = &box_model.name {
            if box_model.wrap {
                box_model.name = Some(wrap_label(
                    name,
                    total_width - 2.0 * conf.sequence.wrap_padding,
                    &msg_font,
                    measure,
                ));
            }
            let box_dims = measure.dimensions(box_model.name.as_deref().unwrap_or(""), &msg_font);
            max_box_height = max_box_height.max(box_dims.height);
            let min_width = total_width.max(box_dims.width + 2.0 * conf.sequence.wrap_padding);
            box_model.margin = conf.sequence.box_text_margin;
            if total_width < min_width {
                let missing = (min_width - total_width) / 2.0;
                box_model.margin += missing;
            }
        }
    }

    for box_model in boxes.iter_mut() {
        box_model.text_max_height = max_box_height;
    }

    max_height.max(conf.sequence.height)
}

struct LoopWidth {
    width: f64,
}

/// Port of `calculateLoopBounds` from `sequenceRenderer.ts`.
fn calculate_loop_bounds(
    messages: &[Message],
    actors: &HashMap<String, ActorModel>,
    conf: &Config,
    measure: &dyn TextMeasure,
) -> HashMap<String, LoopWidth> {
    struct StackItem {
        id: String,
        from: f64,
        to: f64,
        width: f64,
    }

    let mut loops = HashMap::new();
    let mut stack: Vec<StackItem> = Vec::new();
    let mut temp_bounds = BoundsState::default();

    for msg in messages {
        match msg.line_type {
            LineType::LoopStart
            | LineType::AltStart
            | LineType::OptStart
            | LineType::ParStart
            | LineType::ParOverStart
            | LineType::CriticalStart
            | LineType::BreakStart => {
                stack.push(StackItem {
                    id: msg.id.clone(),
                    from: f64::INFINITY,
                    to: f64::NEG_INFINITY,
                    width: 0.0,
                });
            }
            LineType::AltElse | LineType::ParAnd | LineType::CriticalOption => {
                if !msg.message.as_str().is_empty() {
                    if let Some(current) = stack.last() {
                        loops.insert(current.id.clone(), LoopWidth { width: current.width });
                        loops.insert(msg.id.clone(), LoopWidth { width: current.width });
                    }
                }
            }
            LineType::LoopEnd
            | LineType::AltEnd
            | LineType::OptEnd
            | LineType::ParEnd
            | LineType::CriticalEnd
            | LineType::BreakEnd => {
                if let Some(current) = stack.pop() {
                    loops.insert(current.id.clone(), LoopWidth { width: current.width });
                }
            }
            LineType::ActiveStart => {
                let actor_name = msg.from.as_deref().unwrap_or("");
                temp_bounds.new_activation(actor_name, actors, conf.sequence.activation_width);
            }
            LineType::ActiveEnd => {
                let actor_name = msg.from.as_deref().unwrap_or("");
                temp_bounds.end_activation(actor_name);
            }
            _ => {}
        }

        if msg.placement.is_some() {
            let note = build_note_model(msg, actors, conf, measure);
            for stk in &mut stack {
                stk.from = stk.from.min(note.startx);
                stk.to = stk.to.max(note.startx + note.width);
                stk.width = stk.width.max((stk.from - stk.to).abs()) - conf.sequence.label_box_width;
            }
        } else {
            let model = build_message_model(msg, actors, &temp_bounds, conf, measure);
            if model.startx.is_some() && model.stopx.is_some() && !stack.is_empty() {
                let startx = model.startx.unwrap();
                let stopx = model.stopx.unwrap();
                for stk in &mut stack {
                    if startx == stopx {
                        let from = actors.get(msg.from.as_deref().unwrap_or(""));
                        let to = actors.get(msg.to.as_deref().unwrap_or(""));
                        if let (Some(f), Some(t)) = (from, to) {
                            stk.from = stk.from.min(f.x - model.width / 2.0).min(f.x - f.width / 2.0);
                            stk.to = stk.to.max(t.x + model.width / 2.0).max(t.x + f.width / 2.0);
                            stk.width = stk.width.max((stk.to - stk.from).abs()) - conf.sequence.label_box_width;
                        }
                    } else {
                        stk.from = stk.from.min(startx);
                        stk.to = stk.to.max(stopx);
                        stk.width = stk.width.max(model.width) - conf.sequence.label_box_width;
                    }
                }
            }
        }
    }

    loops
}

#[derive(Clone, Debug, Default)]
struct InternalNoteModel {
    width: f64,
    height: f64,
    startx: f64,
    stopx: f64,
    starty: f64,
    stopy: f64,
    message: String,
}

/// Port of `buildNoteModel` from `sequenceRenderer.ts`.
fn build_note_model(
    msg: &Message,
    actors: &HashMap<String, ActorModel>,
    conf: &Config,
    measure: &dyn TextMeasure,
) -> InternalNoteModel {
    let from_actor = actors.get(msg.from.as_deref().unwrap_or(""));
    let to_actor = actors.get(msg.to.as_deref().unwrap_or(""));
    let startx = from_actor.map(|a| a.x).unwrap_or(0.0);
    let stopx = to_actor.map(|a| a.x).unwrap_or(0.0);
    let from_width = from_actor.map(|a| a.width).unwrap_or(conf.sequence.width);
    let to_width = to_actor.map(|a| a.width).unwrap_or(conf.sequence.width);
    let should_wrap = msg.wrap && !msg.message.as_str().is_empty();
    let n_font = note_font(conf);

    let raw_msg = msg.message.as_str();
    let dims_text = if should_wrap {
        wrap_label(raw_msg, conf.sequence.width, &n_font, measure)
    } else {
        raw_msg.to_string()
    };
    let mut text_dimensions = measure.dimensions(&dims_text, &n_font);

    let mut model = InternalNoteModel {
        width: if should_wrap {
            conf.sequence.width
        } else {
            conf.sequence.width.max(text_dimensions.width + 2.0 * conf.sequence.note_margin)
        },
        height: 0.0,
        startx,
        stopx: 0.0,
        starty: 0.0,
        stopy: 0.0,
        message: raw_msg.to_string(),
    };

    match msg.placement {
        Some(Placement::RightOf) => {
            model.width = if should_wrap {
                conf.sequence.width.max(text_dimensions.width)
            } else {
                (from_width / 2.0 + to_width / 2.0).max(text_dimensions.width + 2.0 * conf.sequence.note_margin)
            };
            model.startx = startx + (from_width + conf.sequence.actor_margin) / 2.0;
        }
        Some(Placement::LeftOf) => {
            model.width = if should_wrap {
                conf.sequence.width.max(text_dimensions.width + 2.0 * conf.sequence.note_margin)
            } else {
                (from_width / 2.0 + to_width / 2.0).max(text_dimensions.width + 2.0 * conf.sequence.note_margin)
            };
            model.startx = startx - model.width + (from_width - conf.sequence.actor_margin) / 2.0;
        }
        _ if msg.to == msg.from => {
            let wrap_max = conf.sequence.width.max(from_width);
            text_dimensions = measure.dimensions(
                &if should_wrap {
                    wrap_label(raw_msg, wrap_max, &n_font, measure)
                } else {
                    raw_msg.to_string()
                },
                &n_font,
            );
            model.width = if should_wrap {
                wrap_max
            } else {
                from_width
                    .max(conf.sequence.width)
                    .max(text_dimensions.width + 2.0 * conf.sequence.note_margin)
            };
            model.startx = startx + (from_width - model.width) / 2.0;
        }
        _ => {
            model.width = (startx + from_width / 2.0 - (stopx + to_width / 2.0)).abs() + conf.sequence.actor_margin;
            model.startx = if startx < stopx {
                startx + from_width / 2.0 - conf.sequence.actor_margin / 2.0
            } else {
                stopx + to_width / 2.0 - conf.sequence.actor_margin / 2.0
            };
        }
    }

    if should_wrap {
        model.message = wrap_label(raw_msg, model.width - 2.0 * conf.sequence.wrap_padding, &n_font, measure);
    }

    model
}

#[derive(Clone, Debug)]
struct InternalMessageModel {
    id: String,
    from: String,
    to: String,
    width: f64,
    height: f64,
    startx: Option<f64>,
    stopx: Option<f64>,
    starty: f64,
    stopy: f64,
    message: String,
    line_type: LineType,
    wrap: bool,
    from_bounds: f64,
    to_bounds: f64,
    sequence_index: f64,
    sequence_visible: bool,
}

/// Port of `buildMessageModel` from `sequenceRenderer.ts`.
fn build_message_model(
    msg: &Message,
    actors: &HashMap<String, ActorModel>,
    bounds: &BoundsState,
    conf: &Config,
    measure: &dyn TextMeasure,
) -> InternalMessageModel {
    let from_key = msg.from.as_deref().unwrap_or("");
    let to_key = msg.to.as_deref().unwrap_or("");

    let (from_left, from_right) = bounds.activation_bounds(from_key, actors);
    let (to_left, to_right) = bounds.activation_bounds(to_key, actors);
    let is_arrow_to_right = from_left <= to_left;
    let mut startx = if is_arrow_to_right { from_right } else { from_left };
    let mut stopx = if is_arrow_to_right { to_left } else { to_right };

    if conf.look == "neo" {
        let offset = 3.0;
        if msg.line_type != LineType::SolidOpen {
            stopx += if is_arrow_to_right { -offset } else { offset };
        }
        if is_bidirectional_arrow_type(msg.line_type) {
            startx += if is_arrow_to_right { offset } else { -offset };
        }
    }

    startx += calculate_central_connection_offset(msg, is_arrow_to_right);
    let is_arrow_to_activation = (to_left - to_right).abs() > 2.0;

    let adjust_value = |val: f64| -> f64 {
        if is_arrow_to_right { -val } else { val }
    };

    if from_key == to_key {
        stopx = startx;
    } else {
        if msg.activate && !is_arrow_to_activation {
            stopx += adjust_value(conf.sequence.activation_width / 2.0 - 1.0);
        }

        if !matches!(
            msg.line_type,
            LineType::SolidOpen
                | LineType::DottedOpen
                | LineType::StickTop
                | LineType::StickBottom
                | LineType::StickTopDotted
                | LineType::StickBottomDotted
                | LineType::SolidTopReverseDotted
                | LineType::SolidBottomReverseDotted
                | LineType::StickTopReverse
                | LineType::StickBottomReverse
                | LineType::StickTopReverseDotted
                | LineType::StickBottomReverseDotted
                | LineType::SolidTopReverse
                | LineType::SolidBottomReverse
        ) {
            stopx += adjust_value(3.0);
        }

        if is_bidirectional_arrow_type(msg.line_type)
            || is_reverse_arrow_type(msg.line_type)
        {
            startx -= adjust_value(3.0);
        }
    }

    let all_bounds = [from_left, from_right, to_left, to_right];
    let bounded_width = (startx - stopx).abs();
    let msg_font = message_font(conf);
    let mut message_str = msg.message.as_str().to_string();
    if msg.wrap && !message_str.is_empty() {
        message_str = wrap_label(
            &message_str,
            (bounded_width + 2.0 * conf.sequence.wrap_padding).max(conf.sequence.width),
            &msg_font,
            measure,
        );
    }
    let msg_dims = measure.dimensions(&message_str, &msg_font);

    let width = if msg.wrap { 0.0 } else { msg_dims.width + 2.0 * conf.sequence.wrap_padding }
        .max(bounded_width + 2.0 * conf.sequence.wrap_padding)
        .max(conf.sequence.width);

    let min_bound = all_bounds.iter().copied().fold(f64::INFINITY, f64::min);
    let max_bound = all_bounds.iter().copied().fold(f64::NEG_INFINITY, f64::max);

    InternalMessageModel {
        id: msg.id.clone(),
        from: from_key.to_string(),
        to: to_key.to_string(),
        width,
        height: 0.0,
        startx: Some(startx),
        stopx: Some(stopx),
        starty: 0.0,
        stopy: 0.0,
        message: message_str,
        line_type: msg.line_type,
        wrap: msg.wrap,
        from_bounds: min_bound,
        to_bounds: max_bound,
        sequence_index: 1.0,
        sequence_visible: false,
    }
}

/// Port of `boundMessage` from `sequenceRenderer.ts`.
fn bound_message(
    msg_model: &mut InternalMessageModel,
    bounds: &mut BoundsState,
    conf: &Config,
    measure: &dyn TextMeasure,
) -> f64 {
    bounds.bump_vertical_pos(10.0);
    let startx = msg_model.startx.unwrap_or(0.0);
    let stopx = msg_model.stopx.unwrap_or(0.0);
    let msg_font = message_font(conf);
    let lines = crate::measure::split_breaks(&msg_model.message).len().max(1) as f64;
    let text_dims = measure.dimensions(&msg_model.message, &msg_font);

    let line_height = text_dims.height / lines;
    msg_model.height += line_height;
    bounds.bump_vertical_pos(line_height);

    let mut total_offset = text_dims.height - 10.0;
    let line_start_y: f64;

    if startx == stopx {
        line_start_y = if !conf.sequence.right_angles {
            total_offset += conf.sequence.box_margin;
            bounds.vertical_pos + total_offset
        } else {
            bounds.vertical_pos + total_offset
        };
        total_offset += 30.0;
        let dx = (text_dims.width / 2.0).max(conf.sequence.width / 2.0);
        bounds.insert(
            startx - dx,
            bounds.vertical_pos - 10.0 + total_offset,
            stopx + dx,
            bounds.vertical_pos + 30.0 + total_offset,
            conf.sequence.box_margin,
        );
    } else {
        total_offset += conf.sequence.box_margin;
        line_start_y = bounds.vertical_pos + total_offset;
        bounds.insert(startx, line_start_y - 10.0, stopx, line_start_y, conf.sequence.box_margin);
    }

    bounds.bump_vertical_pos(total_offset);
    msg_model.height += total_offset;
    msg_model.stopy = msg_model.starty + msg_model.height;
    bounds.insert(
        msg_model.from_bounds,
        msg_model.starty,
        msg_model.to_bounds,
        msg_model.stopy,
        conf.sequence.box_margin,
    );

    line_start_y
}

/// Port of `adjustCreatedDestroyedData` from `sequenceRenderer.ts`.
fn adjust_created_destroyed_data(
    msg: &Message,
    msg_model: &mut InternalMessageModel,
    line_start_y: f64,
    index: usize,
    actors: &mut HashMap<String, ActorModel>,
    created_actors: &[(String, usize)],
    destroyed_actors: &[(String, usize)],
    bounds: &mut BoundsState,
    conf: &Config,
) {
    let to_key = msg.to.as_deref().unwrap_or("");
    let from_key = msg.from.as_deref().unwrap_or("");
    let from_x = actors.get(from_key).map(|a| a.x).unwrap_or(0.0);
    let to_x = actors.get(to_key).map(|a| a.x).unwrap_or(0.0);

    let is_created = created_actors.iter().any(|(id, idx)| id == to_key && *idx == index);
    let is_destroyed_sender = destroyed_actors.iter().any(|(id, idx)| id == from_key && *idx == index);
    let is_destroyed_receiver = destroyed_actors.iter().any(|(id, idx)| id == to_key && *idx == index);

    let actor_types = ["actor", "control", "entity", "database"];

    if is_created {
        if let Some(actor) = actors.get_mut(to_key) {
            let adj = if actor_types.contains(&actor.actor_type.as_str()) {
                ACTOR_TYPE_WIDTH / 2.0 + 3.0
            } else {
                actor.width / 2.0 + 3.0
            };
            if actor.x < from_x {
                bounds.insert(
                    msg_model.stopx.unwrap() - adj,
                    msg_model.starty,
                    msg_model.startx.unwrap(),
                    msg_model.stopy + actor.height / 2.0 + conf.sequence.note_margin,
                    conf.sequence.box_margin,
                );
                msg_model.stopx = Some(msg_model.stopx.unwrap() + adj);
            } else {
                bounds.insert(
                    msg_model.startx.unwrap(),
                    msg_model.starty,
                    msg_model.stopx.unwrap() + adj,
                    msg_model.stopy + actor.height / 2.0 + conf.sequence.note_margin,
                    conf.sequence.box_margin,
                );
                msg_model.stopx = Some(msg_model.stopx.unwrap() - adj);
            }
            actor.starty = line_start_y - actor.height / 2.0;
            bounds.bump_vertical_pos(actor.height / 2.0);
        }
    } else if is_destroyed_sender {
        if let Some(actor) = actors.get_mut(from_key) {
            if conf.sequence.mirror_actors {
                let adj = if actor_types.contains(&actor.actor_type.as_str()) {
                    ACTOR_TYPE_WIDTH / 2.0
                } else {
                    actor.width / 2.0
                };
                if actor.x < to_x {
                    bounds.insert(
                        msg_model.startx.unwrap() - adj,
                        msg_model.starty,
                        msg_model.stopx.unwrap(),
                        msg_model.stopy + actor.height / 2.0 + conf.sequence.note_margin,
                        conf.sequence.box_margin,
                    );
                    msg_model.startx = Some(msg_model.startx.unwrap() + adj);
                } else {
                    bounds.insert(
                        msg_model.stopx.unwrap(),
                        msg_model.starty,
                        msg_model.startx.unwrap() + adj,
                        msg_model.stopy + actor.height / 2.0 + conf.sequence.note_margin,
                        conf.sequence.box_margin,
                    );
                    msg_model.startx = Some(msg_model.startx.unwrap() - adj);
                }
            }
            actor.stopy = Some(line_start_y - actor.height / 2.0);
            bounds.bump_vertical_pos(actor.height / 2.0);
        }
    } else if is_destroyed_receiver {
        if let Some(actor) = actors.get_mut(to_key) {
            if conf.sequence.mirror_actors {
                let adj = if actor_types.contains(&actor.actor_type.as_str()) {
                    ACTOR_TYPE_WIDTH / 2.0 + 3.0
                } else {
                    actor.width / 2.0 + 3.0
                };
                if actor.x < from_x {
                    bounds.insert(
                        msg_model.stopx.unwrap() - adj,
                        msg_model.starty,
                        msg_model.startx.unwrap(),
                        msg_model.stopy + actor.height / 2.0 + conf.sequence.note_margin,
                        conf.sequence.box_margin,
                    );
                    msg_model.stopx = Some(msg_model.stopx.unwrap() + adj);
                } else {
                    bounds.insert(
                        msg_model.startx.unwrap(),
                        msg_model.starty,
                        msg_model.stopx.unwrap() + adj,
                        msg_model.stopy + actor.height / 2.0 + conf.sequence.note_margin,
                        conf.sequence.box_margin,
                    );
                    msg_model.stopx = Some(msg_model.stopx.unwrap() - adj);
                }
            }
            actor.stopy = Some(line_start_y - actor.height / 2.0);
            bounds.bump_vertical_pos(actor.height / 2.0);
        }
    }
}

/// Port of `addActorRenderingData` from `sequenceRenderer.ts`.
fn add_actor_rendering_data(
    actors: &mut HashMap<String, ActorModel>,
    actor_keys: &[String],
    boxes: &mut [BoxModel],
    created_actors: &[(String, usize)],
    bounds: &mut BoundsState,
    conf: &Config,
    is_footer: bool,
) {
    let mut prev_width: f64 = 0.0;
    let mut prev_margin: f64 = 0.0;
    let mut prev_box: Option<usize> = None;
    let mut max_height: f64 = 0.0;

    for actor_key in actor_keys {
        let box_idx = actors.get(actor_key).and_then(|a| a.box_index);

        if let Some(p_box) = prev_box {
            if prev_box != box_idx {
                prev_margin += conf.sequence.box_margin + boxes[p_box].margin;
            }
        }

        if let Some(b_idx) = box_idx {
            if prev_box != Some(b_idx) {
                if !is_footer {
                    boxes[b_idx].x = prev_width + prev_margin;
                    boxes[b_idx].y = bounds.vertical_pos;
                }
                prev_margin += boxes[b_idx].margin;
            }
        }

        if let Some(actor) = actors.get_mut(actor_key) {
            actor.width = actor.width.max(conf.sequence.width);
            actor.height = actor.height.max(conf.sequence.height);
            actor.margin = if actor.margin > 0.0 { actor.margin } else { conf.sequence.actor_margin };

            max_height = max_height.max(actor.height);

            if created_actors.iter().any(|(id, _)| id == &actor.name) {
                prev_margin += actor.width / 2.0;
            }

            actor.x = prev_width + prev_margin;
            actor.starty = bounds.vertical_pos;

            bounds.insert(
                actor.x,
                bounds.vertical_pos,
                actor.x + actor.width,
                actor.height,
                conf.sequence.box_margin,
            );

            prev_width += actor.width + prev_margin;
            if let Some(b_idx) = actor.box_index {
                boxes[b_idx].width = prev_width + boxes[b_idx].margin - boxes[b_idx].x;
            }
            prev_margin = actor.margin;
            prev_box = actor.box_index;
        }
    }

    bounds.bump_vertical_pos(max_height);
}

/// Renders an already parsed sequence diagram and returns layout data and scene.
pub fn draw_layout_ast(
    ast: &Ast,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<(SequenceLayoutData, Scene), SequenceRenderError> {
    let mut conf = config.clone();
    // Default appearance settings
    if conf.sequence.actor_font_family.is_empty() {
        conf.sequence.actor_font_family = "\"Open Sans\", sans-serif".to_string();
    }
    if conf.sequence.message_font_family.is_empty() {
        conf.sequence.message_font_family = "\"trebuchet ms\", verdana, arial, sans-serif".to_string();
    }
    if conf.sequence.note_font_family.is_empty() {
        conf.sequence.note_font_family = "\"trebuchet ms\", verdana, arial, sans-serif".to_string();
    }

    let mut actors: HashMap<String, ActorModel> = HashMap::new();
    let mut actor_keys: Vec<String> = Vec::new();

    for actor in &ast.actors {
        actor_keys.push(actor.id.clone());
        actors.insert(
            actor.id.clone(),
            ActorModel {
                name: actor.name.clone(),
                description: actor.description.clone(),
                actor_type: actor.actor_type.clone(),
                wrap: actor.wrap,
                x: 0.0,
                starty: 0.0,
                stopy: None,
                width: conf.sequence.width,
                height: conf.sequence.height,
                margin: 0.0,
                box_index: actor.box_index,
                prev_actor: actor.prev_actor.clone(),
                next_actor: actor.next_actor.clone(),
                actor_text_height: 0.0,
            },
        );
    }

    let mut boxes: Vec<BoxModel> = ast
        .boxes
        .iter()
        .map(|b| BoxModel {
            name: b.name.clone(),
            wrap: b.wrap,
            fill: b.fill.clone(),
            actor_keys: b.actor_keys.clone(),
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            startx: 0.0,
            starty: 0.0,
            stopx: 0.0,
            stopy: 0.0,
            margin: 0.0,
            text_max_height: 0.0,
        })
        .collect();

    let max_widths = get_max_message_width_per_actor(&actors, &ast.messages, &conf, measure);
    conf.sequence.height = calculate_actor_margins(
        &mut actors,
        &actor_keys,
        &max_widths,
        &mut boxes,
        &conf,
        measure,
    );

    let has_boxes = !boxes.is_empty();
    let has_box_titles = boxes.iter().any(|b| b.name.is_some());

    let mut bounds = BoundsState::default();

    if has_boxes {
        bounds.bump_vertical_pos(conf.sequence.box_margin);
        if has_box_titles {
            let max_box_h = boxes.first().map(|b| b.text_max_height).unwrap_or(0.0);
            bounds.bump_vertical_pos(max_box_h);
        }
    }

    if conf.sequence.hide_unused_participants {
        let mut used = HashSet::new();
        for msg in &ast.messages {
            if let Some(from) = &msg.from {
                used.insert(from.clone());
            }
            if let Some(to) = &msg.to {
                used.insert(to.clone());
            }
        }
        actor_keys.retain(|k| used.contains(k));
    }

    add_actor_rendering_data(
        &mut actors,
        &actor_keys,
        &mut boxes,
        &ast.created_actors,
        &mut bounds,
        &conf,
        false,
    );

    let loop_widths = calculate_loop_bounds(&ast.messages, &actors, &conf, measure);

    // Track state for message models
    let mut sequence_index: f64 = 1.0;
    let mut sequence_index_step: f64 = 1.0;
    let mut show_sequence_numbers = ast.sequence_numbers_enabled;

    let mut messages_to_draw: Vec<(InternalMessageModel, f64, Message)> = Vec::new();
    let mut background_rects: Vec<LoopItem> = Vec::new();
    let mut completed_notes: Vec<InternalNoteModel> = Vec::new();
    let mut completed_loops: Vec<LoopItem> = Vec::new();

    let msg_font = message_font(&conf);

    for (index, msg) in ast.messages.iter().enumerate() {
        match msg.line_type {
            LineType::Note => {
                if bounds.sequence_items.last().map(|it| it.kind == "par").unwrap_or(false) {
                    bounds.vertical_pos = bounds.saved_vertical_pos;
                }
                let mut note = build_note_model(msg, &actors, &conf, measure);
                bounds.bump_vertical_pos(conf.sequence.box_margin);
                note.height = conf.sequence.box_margin;
                note.starty = bounds.vertical_pos;
                let text_height = measure.dimensions(&note.message, &note_font(&conf)).height;
                let total_h = text_height + 2.0 * conf.sequence.note_margin;
                note.height += total_h;
                bounds.bump_vertical_pos(total_h);
                note.stopy = note.starty + total_h;
                note.stopx = note.startx + note.width;
                bounds.insert(
                    note.startx,
                    note.starty,
                    note.stopx,
                    note.stopy,
                    conf.sequence.box_margin,
                );
                completed_notes.push(note);
            }
            LineType::ActiveStart => {
                if let Some(from) = &msg.from {
                    bounds.new_activation(from, &actors, conf.sequence.activation_width);
                }
            }
            LineType::CentralConnection | LineType::CentralConnectionReverse => {
                if let Some(from) = &msg.from {
                    bounds.new_activation(from, &actors, conf.sequence.activation_width);
                }
            }
            LineType::ActiveEnd => {
                if let Some(from) = &msg.from {
                    if let Some(mut act_data) = bounds.end_activation(from) {
                        if act_data.starty + 18.0 > bounds.vertical_pos {
                            act_data.starty = bounds.vertical_pos - 6.0;
                            bounds.vertical_pos += 12.0;
                        }
                        bounds.insert(
                            act_data.startx,
                            bounds.vertical_pos - 10.0,
                            act_data.stopx,
                            bounds.vertical_pos,
                            conf.sequence.box_margin,
                        );
                    }
                }
            }
            LineType::LoopStart
            | LineType::OptStart
            | LineType::AltStart
            | LineType::CriticalStart
            | LineType::BreakStart => {
                bounds.bump_vertical_pos(conf.sequence.box_margin);
                let mut height_adjust = conf.sequence.box_margin + conf.sequence.box_text_margin;
                let mut loop_title = None;
                if let Some(lw) = loop_widths.get(&msg.id) {
                    let wrapped = wrap_label(
                        &format!("[{}]", msg.message.as_str()),
                        lw.width - 2.0 * conf.sequence.wrap_padding,
                        &msg_font,
                        measure,
                    );
                    let text_dims = measure.dimensions(&wrapped, &msg_font);
                    let total_offset = text_dims.height.max(conf.sequence.label_box_height);
                    height_adjust = conf.sequence.box_margin + conf.sequence.box_text_margin + total_offset;
                    loop_title = Some(wrapped);
                }
                let kind = match msg.line_type {
                    LineType::LoopStart => "loop",
                    LineType::OptStart => "opt",
                    LineType::AltStart => "alt",
                    LineType::CriticalStart => "critical",
                    LineType::BreakStart => "break",
                    _ => "loop",
                };
                bounds.new_loop(msg.id.clone(), loop_title, msg.wrap, None, kind.to_string());
                bounds.bump_vertical_pos(height_adjust);
            }
            LineType::ParStart | LineType::ParOverStart => {
                bounds.bump_vertical_pos(conf.sequence.box_margin);
                let mut height_adjust = conf.sequence.box_margin + conf.sequence.box_text_margin;
                let mut loop_title = None;
                if let Some(lw) = loop_widths.get(&msg.id) {
                    let wrapped = wrap_label(
                        &format!("[{}]", msg.message.as_str()),
                        lw.width - 2.0 * conf.sequence.wrap_padding,
                        &msg_font,
                        measure,
                    );
                    let text_dims = measure.dimensions(&wrapped, &msg_font);
                    let total_offset = text_dims.height.max(conf.sequence.label_box_height);
                    height_adjust = conf.sequence.box_margin + conf.sequence.box_text_margin + total_offset;
                    loop_title = Some(wrapped);
                }
                bounds.new_loop(msg.id.clone(), loop_title, msg.wrap, None, "par".to_string());
                bounds.saved_vertical_pos = bounds.vertical_pos;
                bounds.bump_vertical_pos(height_adjust);
            }
            LineType::LoopEnd
            | LineType::OptEnd
            | LineType::AltEnd
            | LineType::ParEnd
            | LineType::CriticalEnd
            | LineType::BreakEnd => {
                if let Some(mut loop_model) = bounds.end_loop() {
                    let stopy = loop_model.stopy.unwrap_or(bounds.vertical_pos);
                    bounds.bump_vertical_pos(stopy - bounds.vertical_pos);
                    loop_model.height = (stopy - loop_model.starty).round();
                    completed_loops.push(loop_model);
                }
            }
            LineType::AltElse | LineType::ParAnd | LineType::CriticalOption => {
                bounds.bump_vertical_pos(conf.sequence.box_margin + conf.sequence.box_text_margin);
                let mut height_adjust = conf.sequence.box_margin;
                let mut section_title = msg.message.as_str().to_string();
                if let Some(lw) = loop_widths.get(&msg.id) {
                    section_title = wrap_label(
                        &section_title,
                        lw.width - 2.0 * conf.sequence.wrap_padding,
                        &msg_font,
                        measure,
                    );
                    let text_dims = measure.dimensions(&section_title, &msg_font);
                    let total_offset = text_dims.height.max(conf.sequence.label_box_height);
                    height_adjust = conf.sequence.box_margin + total_offset;
                }
                bounds.add_section_to_loop(&section_title);
                bounds.bump_vertical_pos(height_adjust);
            }
            LineType::RectStart => {
                bounds.bump_vertical_pos(conf.sequence.box_margin);
                let fill = if !msg.message.as_str().is_empty() {
                    msg.message.as_str().to_string()
                } else {
                    "rgba(128, 128, 128, 0.5)".to_string()
                };
                bounds.new_loop(msg.id.clone(), None, false, Some(fill), "rect".to_string());
                bounds.bump_vertical_pos(conf.sequence.box_margin);
            }
            LineType::RectEnd => {
                if let Some(mut loop_model) = bounds.end_loop() {
                    let stopy = loop_model.stopy.unwrap_or(bounds.vertical_pos);
                    bounds.bump_vertical_pos(stopy - bounds.vertical_pos);
                    loop_model.height = (stopy - loop_model.starty).round();
                    background_rects.push(loop_model.clone());
                    completed_loops.push(loop_model);
                }
            }
            LineType::Autonumber => {
                if let MessageText::SequenceIndex { start, step, visible } = &msg.message {
                    if let Some(s) = start {
                        sequence_index = *s;
                    }
                    if let Some(st) = step {
                        sequence_index_step = *st;
                    }
                    show_sequence_numbers = *visible;
                }
            }
            _ => {
                let mut msg_model = build_message_model(msg, &actors, &bounds, &conf, measure);
                msg_model.starty = bounds.vertical_pos;
                msg_model.sequence_index = sequence_index;
                msg_model.sequence_visible = show_sequence_numbers;
                let line_start_y = bound_message(&mut msg_model, &mut bounds, &conf, measure);
                adjust_created_destroyed_data(
                    msg,
                    &mut msg_model,
                    line_start_y,
                    index,
                    &mut actors,
                    &ast.created_actors,
                    &ast.destroyed_actors,
                    &mut bounds,
                    &conf,
                );
                messages_to_draw.push((msg_model, line_start_y, msg.clone()));

                // Step sequence counter
                sequence_index = ((sequence_index + sequence_index_step) * 100.0).round() / 100.0;
            }
        }
    }

    // Mirror actors if enabled
    if conf.sequence.mirror_actors {
        bounds.bump_vertical_pos(conf.sequence.box_margin * 2.0);
        let mut max_footer_h: f64 = 0.0;
        for key in &actor_keys {
            if let Some(actor) = actors.get_mut(key) {
                if actor.stopy.is_none() {
                    actor.stopy = Some(bounds.vertical_pos);
                }
                max_footer_h = max_footer_h.max(actor.height);
            }
        }
        bounds.bump_vertical_pos(max_footer_h + conf.sequence.box_margin);
    }

    // Finalize box bounds
    for box_model in boxes.iter_mut() {
        box_model.height = bounds.vertical_pos - box_model.y;
        bounds.insert(
            box_model.x,
            box_model.y,
            box_model.x + box_model.width,
            box_model.height,
            conf.sequence.box_margin,
        );
        let box_padding = conf.sequence.box_margin * 2.0;
        box_model.startx = box_model.x - box_padding;
        box_model.starty = box_model.y - box_padding * 0.25;
        box_model.stopx = box_model.startx + box_model.width + 2.0 * box_padding;
        box_model.stopy = box_model.starty + box_model.height + box_padding * 0.75;
    }

    if has_boxes {
        bounds.bump_vertical_pos(conf.sequence.box_margin);
    }

    let box_startx = bounds.startx.unwrap_or(0.0);
    let box_stopx = bounds.stopx.unwrap_or(0.0);
    let box_starty = bounds.starty.unwrap_or(0.0);
    let box_stopy = bounds.stopy.unwrap_or(0.0);

    let box_height = box_stopy - box_starty;
    let mut height = box_height + 2.0 * conf.sequence.diagram_margin_y;
    if conf.sequence.mirror_actors {
        height = height - conf.sequence.box_margin + conf.sequence.bottom_margin_adj;
    }

    let box_width = box_stopx - box_startx;
    let width = box_width + 2.0 * conf.sequence.diagram_margin_x;

    let has_title = !ast.diagram_title.is_empty();
    let extra_vert_for_title = if has_title { 40.0 } else { 0.0 };
    let extra_height_for_neo_actors = if !actors.is_empty() && conf.look == "neo" {
        30.0
    } else {
        0.0
    };

    let view_box = Rect::new(
        box_startx - conf.sequence.diagram_margin_x,
        -(conf.sequence.diagram_margin_y + extra_vert_for_title),
        width,
        height + extra_vert_for_title + extra_height_for_neo_actors,
    );

    // Build LayoutData
    let mut layout_actors = Vec::new();
    for key in &actor_keys {
        if let Some(actor) = actors.get(key) {
            layout_actors.push(ActorLayout {
                id: actor.name.clone(),
                x: actor.x,
                y: actor.starty,
                width: actor.width,
                height: actor.height,
                stopy: actor.stopy.unwrap_or(bounds.vertical_pos),
            });
        }
    }

    let mut layout_boxes = Vec::new();
    for b in &boxes {
        layout_boxes.push(BoxLayout {
            name: b.name.clone(),
            x: b.x,
            y: b.y,
            width: b.width,
            height: b.height,
            startx: b.startx,
            starty: b.starty,
            stopx: b.stopx,
            stopy: b.stopy,
        });
    }

    let mut layout_messages = Vec::new();
    let mut layout_edges = Vec::new();

    for (msg_model, line_start_y, msg) in &messages_to_draw {
        layout_messages.push(MessageLayout {
            id: msg_model.id.clone(),
            from: msg_model.from.clone(),
            to: msg_model.to.clone(),
            line_type: msg_model.line_type as u8,
            startx: msg_model.startx.unwrap_or(0.0),
            stopx: msg_model.stopx.unwrap_or(0.0),
            starty: msg_model.starty,
            stopy: msg_model.stopy,
            height: msg_model.height,
        });

        let startx = msg_model.startx.unwrap_or(0.0);
        let stopx = msg_model.stopx.unwrap_or(0.0);
        let edge_id = format!("i{}", msg_model.id);

        if startx == stopx {
            let is_autonumber_on = msg_model.sequence_visible || conf.sequence.show_sequence_numbers;
            let is_reverse = is_reverse_arrow_type(msg_model.line_type);
            let is_bi = is_bidirectional_arrow_type(msg_model.line_type);
            let line_start_x = startx + if is_autonumber_on && (is_reverse || is_bi) { 10.0 } else { 0.0 };

            let d = if conf.sequence.right_angles {
                let text_dims = measure.dimensions(&msg_model.message, &msg_font);
                let mid_x = startx + (conf.sequence.width / 2.0).max(text_dims.width / 2.0);
                format!("M  {line_start_x},{line_start_y} H {mid_x} V {} H {startx}", line_start_y + 25.0)
            } else {
                format!(
                    "M {},{} C {},{} {},{} {},{}",
                    line_start_x,
                    line_start_y,
                    line_start_x + 60.0,
                    line_start_y - 10.0,
                    startx + 60.0,
                    line_start_y + 30.0,
                    startx,
                    line_start_y + 20.0,
                )
            };
            layout_edges.push(EdgeLayout {
                id: edge_id,
                points: Vec::new(),
                x1: None,
                y1: None,
                x2: None,
                y2: None,
                d: Some(d),
            });
        } else {
            let mut line_start_x = startx;
            let mut line_stop_x = stopx;

            if msg_model.sequence_visible || conf.sequence.show_sequence_numbers {
                let has_central = has_central_connection(msg);
                if is_bidirectional_arrow_type(msg_model.line_type) {
                    if startx < stopx {
                        line_start_x = startx + 12.0;
                    } else {
                        line_start_x = startx - 6.0 + if has_central { -5.0 } else { 0.0 };
                        if msg.central_connection == LineType::CentralConnectionDual as u8
                            || msg.central_connection == LineType::CentralConnectionReverse as u8
                        {
                            line_start_x -= 7.5;
                        }
                    }
                } else if is_reverse_arrow_type(msg_model.line_type) {
                    if stopx > startx {
                        line_stop_x = stopx - 12.0;
                    } else {
                        line_stop_x = stopx - 6.0;
                        if msg.central_connection == LineType::CentralConnectionDual as u8
                            || msg.central_connection == LineType::CentralConnectionReverse as u8
                        {
                            line_start_x -= 7.5;
                        }
                    }
                    if has_central {
                        line_stop_x += 15.0;
                    }
                } else {
                    line_start_x = startx + 6.0;
                }
            }

            layout_edges.push(EdgeLayout {
                id: edge_id,
                points: vec![
                    Point::new(line_start_x, *line_start_y),
                    Point::new(line_stop_x, *line_start_y),
                ],
                x1: Some(line_start_x),
                y1: Some(*line_start_y),
                x2: Some(line_stop_x),
                y2: Some(*line_start_y),
                d: None,
            });
        }
    }

    let mut layout_notes = Vec::new();
    for n in &completed_notes {
        layout_notes.push(NoteLayout {
            message: n.message.clone(),
            startx: n.startx,
            stopx: n.stopx,
            starty: n.starty,
            stopy: n.stopy,
            width: n.width,
            height: n.height,
        });
    }

    let mut layout_loops = Vec::new();
    for l in &completed_loops {
        layout_loops.push(LoopLayout {
            title: l.title.clone(),
            startx: l.startx.unwrap_or(0.0),
            stopx: l.stopx.unwrap_or(0.0),
            starty: l.starty,
            stopy: l.stopy.unwrap_or(0.0),
            height: l.height,
        });
    }

    // Construct Scene items
    let mut scene = Scene::new(view_box);
    scene.bounds = Rect::new(box_startx, box_starty, box_width, box_height);

    // Marker definitions
    scene.define(MarkerDef {
        id: "arrowhead".to_string(),
        view_box: Rect::new(-1.0, 0.0, 12.0, 12.0),
        reference: Point::new(7.9, 5.0),
        width: 12.0,
        height: 12.0,
        units: MarkerUnits::UserSpaceOnUse,
        orient: MarkerOrient::AutoStartReverse,
        class: Some("arrowhead".to_string()),
        shapes: vec![PathShape::new(
            Path::polygon([Point::new(-1.0, 0.0), Point::new(10.0, 5.0), Point::new(0.0, 10.0)]),
            PathStyle::filled(Color::current()),
        )],
    });

    scene.define(MarkerDef {
        id: "crosshead".to_string(),
        view_box: Rect::new(0.0, 0.0, 15.0, 8.0),
        reference: Point::new(4.0, 4.5),
        width: 15.0,
        height: 8.0,
        units: MarkerUnits::UserSpaceOnUse,
        orient: MarkerOrient::Auto,
        class: Some("crosshead".to_string()),
        shapes: vec![
            PathShape::new(
                Path::polyline([Point::new(1.0, 2.0), Point::new(6.0, 7.0)]),
                PathStyle::stroked(Color::current(), 1.0),
            ),
            PathShape::new(
                Path::polyline([Point::new(6.0, 2.0), Point::new(1.0, 7.0)]),
                PathStyle::stroked(Color::current(), 1.0),
            ),
        ],
    });

    scene.define(MarkerDef {
        id: "filled-head".to_string(),
        view_box: Rect::new(0.0, 0.0, 20.0, 28.0),
        reference: Point::new(15.5, 7.0),
        width: 20.0,
        height: 28.0,
        units: MarkerUnits::UserSpaceOnUse,
        orient: MarkerOrient::Auto,
        class: Some("filled-head".to_string()),
        shapes: vec![PathShape::new(
            Path::polygon([
                Point::new(18.0, 7.0),
                Point::new(9.0, 13.0),
                Point::new(14.0, 7.0),
                Point::new(9.0, 1.0),
            ]),
            PathStyle::filled(Color::current()),
        )],
    });

    scene.define(MarkerDef {
        id: "sequencenumber".to_string(),
        view_box: Rect::new(0.0, 0.0, 60.0, 40.0),
        reference: Point::new(15.0, 15.0),
        width: 60.0,
        height: 40.0,
        units: MarkerUnits::UserSpaceOnUse,
        orient: MarkerOrient::Auto,
        class: Some("sequencenumber".to_string()),
        shapes: vec![PathShape::new(
            Path::new(),
            PathStyle::filled(Color::current()),
        )],
    });

    // 1. Boxes background
    for b in &boxes {
        let box_style = PathStyle {
            fill: Some(Color::new(&b.fill)),
            stroke: Some(Color::new("rgba(0, 0, 0, 0.5)")),
            stroke_width: 1.0,
            ..Default::default()
        };
        scene.push(
            RectShape::new(
                Rect::new(b.startx, b.starty, b.stopx - b.startx, b.stopy - b.starty),
                0.0,
                box_style,
            )
            .with_source(SourceRef::cluster("box")),
        );

        if let Some(name) = &b.name {
            let title_pos = Point::new(
                b.x + b.width / 2.0,
                b.y + conf.sequence.box_text_margin + b.text_max_height / 2.0,
            );
            let block = TextBlock::new(
                vec![TextLine::new(vec![TextSpan::plain(name)])],
                dims_size(&measure.dimensions(name, &msg_font)),
                18.0,
            );
            scene.push(TextRun::centered(title_pos, msg_font.clone(), block));
        }
    }

    // 2. Rect backgrounds
    for r in &background_rects {
        let fill = r.fill.clone().unwrap_or_else(|| "rgba(128, 128, 128, 0.5)".to_string());
        let rect_style = PathStyle::filled(Color::new(fill));
        let rx = r.startx.unwrap_or(0.0);
        let ry = r.starty;
        let rw = r.stopx.unwrap_or(0.0) - rx;
        let rh = r.stopy.unwrap_or(0.0) - ry;
        scene.push(
            RectShape::new(Rect::new(rx, ry, rw, rh), 0.0, rect_style)
                .with_source(SourceRef::cluster(&r.id)),
        );
    }

    // 3. Loops
    for l in &completed_loops {
        if l.kind == "rect" {
            continue;
        }
        let lx = l.startx.unwrap_or(0.0);
        let ly = l.starty;
        let lw = l.stopx.unwrap_or(0.0) - lx;
        let lh = l.stopy.unwrap_or(0.0) - ly;
        let line_style = PathStyle::stroked(Color::new("#000000"), 2.0);

        // Border rectangle
        scene.push(RectShape::new(Rect::new(lx, ly, lw, lh), 0.0, line_style.clone()));

        // Divider lines
        for sec in &l.sections {
            let mut div_path = Path::new();
            div_path.move_to(Point::new(lx, sec.y));
            div_path.line_to(Point::new(lx + lw, sec.y));
            let div_style = PathStyle::stroked(Color::new("#000000"), 1.0).with_dash(vec![3.0, 3.0]);
            scene.push(PathShape::new(div_path, div_style));
        }

        // Label box
        let label_w = (conf.sequence.label_box_width).max(50.0);
        let label_h = conf.sequence.label_box_height;
        let polygon = Path::polygon([
            Point::new(lx, ly),
            Point::new(lx + label_w, ly),
            Point::new(lx + label_w, ly + label_h - 7.0),
            Point::new(lx + label_w - 8.4, ly + label_h),
            Point::new(lx, ly + label_h),
        ]);
        let label_style = PathStyle {
            fill: Some(Color::new("#eaeaea")),
            stroke: Some(Color::new("#666666")),
            stroke_width: 1.0,
            ..Default::default()
        };
        scene.push(PathShape::new(polygon, label_style));

        // Label text
        let kind_text = &l.kind;
        let kind_dims = measure.dimensions(kind_text, &msg_font);
        let kind_block = TextBlock::new(
            vec![TextLine::new(vec![TextSpan::plain(kind_text)])],
            dims_size(&kind_dims),
            16.0,
        );
        scene.push(TextRun::centered(
            Point::new(lx + label_w / 2.0, ly + label_h / 2.0),
            msg_font.clone(),
            kind_block,
        ));

        // Loop title text
        if let Some(title) = &l.title {
            let title_x = lx + conf.sequence.label_box_width / 2.0 + lw / 2.0;
            let title_y = ly + conf.sequence.box_margin + conf.sequence.box_text_margin;
            let title_dims = measure.dimensions(title, &msg_font);
            let title_block = TextBlock::new(
                vec![TextLine::new(vec![TextSpan::plain(title)])],
                dims_size(&title_dims),
                16.0,
            );
            scene.push(TextRun::centered(
                Point::new(title_x, title_y),
                msg_font.clone(),
                title_block,
            ));
        }

        // Section titles
        for (i, sec_title) in l.section_titles.iter().enumerate() {
            if let Some(sec) = l.sections.get(i) {
                let sec_x = lx + lw / 2.0;
                let sec_y = sec.y + conf.sequence.box_margin + conf.sequence.box_text_margin;
                let dims = measure.dimensions(sec_title, &msg_font);
                let block = TextBlock::new(
                    vec![TextLine::new(vec![TextSpan::plain(sec_title)])],
                    dims_size(&dims),
                    16.0,
                );
                scene.push(TextRun::centered(Point::new(sec_x, sec_y), msg_font.clone(), block));
            }
        }
    }

    // 4. Notes
    let note_style = PathStyle {
        fill: Some(Color::new("#edf2ae")),
        stroke: Some(Color::new("#666666")),
        stroke_width: 1.0,
        ..Default::default()
    };
    let n_font = note_font(&conf);
    for n in &completed_notes {
        let note_h = n.stopy - n.starty;
        scene.push(
            RectShape::new(Rect::new(n.startx, n.starty, n.width, note_h), 0.0, note_style.clone())
                .with_source(SourceRef::node("note")),
        );

        let lines: Vec<TextLine> = crate::measure::split_breaks(&n.message)
            .into_iter()
            .map(|l| TextLine::new(vec![TextSpan::plain(l)]))
            .collect();
        let dims = measure.dimensions(&n.message, &n_font);
        let block = TextBlock::new(lines, dims_size(&dims), 14.0);
        let text_center = Point::new(n.startx + n.width / 2.0, n.starty + note_h / 2.0);
        let mut run = TextRun::centered(text_center, n_font.clone(), block);
        run.baseline = TextBaseline::Middle;
        scene.push(run);
    }

    // 5. Actors (Top and Lifelines)
    let act_font = actor_font(&conf);
    for key in &actor_keys {
        if let Some(actor) = actors.get(key) {
            let center = actor.x + actor.width / 2.0;
            let center_y = actor.starty + actor.height;
            let lifeline_end_y = if conf.sequence.mirror_actors {
                actor.stopy.unwrap_or(bounds.vertical_pos)
            } else {
                actor.stopy.unwrap_or(bounds.vertical_pos) + actor.height / 2.0
            };

            // Lifeline
            let mut lifeline = Path::new();
            lifeline.move_to(Point::new(center, center_y));
            lifeline.line_to(Point::new(center, lifeline_end_y));
            let lifeline_style = PathStyle::stroked(Color::new("#999999"), 0.5);
            scene.push(
                PathShape::new(lifeline, lifeline_style)
                    .with_source(SourceRef::edge(&actor.name)),
            );

            // Actor shape
            let actor_style = PathStyle {
                fill: Some(Color::new("#eaeaea")),
                stroke: Some(Color::new("#666666")),
                stroke_width: 1.0,
                ..Default::default()
            };

            match actor.actor_type.as_str() {
                "actor" => {
                    let mut man = Path::new();
                    man.move_to(Point::new(center, actor.starty + 25.0));
                    man.line_to(Point::new(center, actor.starty + 45.0));
                    man.move_to(Point::new(center - 18.0, actor.starty + 33.0));
                    man.line_to(Point::new(center + 18.0, actor.starty + 33.0));
                    man.move_to(Point::new(center - 18.0, actor.starty + 60.0));
                    man.line_to(Point::new(center, actor.starty + 45.0));
                    man.line_to(Point::new(center + 16.0, actor.starty + 60.0));
                    scene.push(PathShape::new(man, actor_style.clone()));
                    scene.push(EllipseShape::new(
                        Point::new(center, actor.starty + 10.0),
                        15.0,
                        15.0,
                        actor_style.clone(),
                    ));
                }
                "database" => {
                    let w = actor.width / 3.0;
                    let rx = w / 2.0;
                    let ry = rx / (2.5 + w / 50.0);
                    let h = actor.width / 3.0;
                    let mut cyl = Path::new();
                    cyl.move_to(Point::new(actor.x + w, actor.starty + 2.0 * ry));
                    cyl.arc_to(rx, ry, 0.0, false, false, Point::new(actor.x + 2.0 * w, actor.starty + 2.0 * ry));
                    cyl.arc_to(rx, ry, 0.0, false, false, Point::new(actor.x + w, actor.starty + 2.0 * ry));
                    cyl.line_to(Point::new(actor.x + w, actor.starty + h));
                    cyl.arc_to(rx, ry, 0.0, false, false, Point::new(actor.x + 2.0 * w, actor.starty + h));
                    cyl.line_to(Point::new(actor.x + 2.0 * w, actor.starty + 2.0 * ry));
                    scene.push(PathShape::new(cyl, actor_style.clone()));
                }
                _ => {
                    scene.push(
                        RectShape::new(
                            Rect::new(actor.x, actor.starty, actor.width, actor.height),
                            3.0,
                            actor_style.clone(),
                        )
                        .with_source(SourceRef::node(&actor.name)),
                    );
                }
            }

            // Actor label
            let text_pos = Point::new(
                actor.x + actor.width / 2.0,
                actor.starty + actor.height / 2.0 + 5.0,
            );
            let dims = measure.dimensions(&actor.description, &act_font);
            let block = TextBlock::new(
                vec![TextLine::new(vec![TextSpan::plain(&actor.description)])],
                dims_size(&dims),
                14.0,
            );
            scene.push(TextRun::centered(text_pos, act_font.clone(), block));
        }
    }

    // 6. Messages and Edges
    for (i, edge) in layout_edges.iter().enumerate() {
        let (msg_model, line_start_y, _msg) = &messages_to_draw[i];
        let is_dotted = is_dotted_arrow_type(msg_model.line_type);
        let mut style = PathStyle::stroked(Color::new("#000000"), 2.0);
        if is_dotted {
            style = style.with_dash(vec![3.0, 3.0]);
        }

        // Message text
        let text_x = msg_model.startx.unwrap_or(0.0).min(msg_model.stopx.unwrap_or(0.0));
        let text_w = (msg_model.stopx.unwrap_or(0.0) - msg_model.startx.unwrap_or(0.0)).abs();
        let dims = measure.dimensions(&msg_model.message, &msg_font);
        let block = TextBlock::new(
            vec![TextLine::new(vec![TextSpan::plain(&msg_model.message)])],
            dims_size(&dims),
            16.0,
        );
        let text_pos = Point::new(text_x + text_w / 2.0, msg_model.starty + 10.0 + dims.height / 2.0);
        let mut text_run = TextRun::centered(text_pos, msg_font.clone(), block);
        text_run.baseline = TextBaseline::Middle;
        scene.push(text_run);

        // Message Path
        if let Some(d_str) = &edge.d {
            let mut path = Path::new();
            if let Some(rest) = d_str.strip_prefix("M ") {
                let parts: Vec<&str> = rest.split_whitespace().collect();
                if parts.len() >= 4 && parts[1] == "C" {
                    let start_coords: Vec<f64> = parts[0].split(',').filter_map(|s| s.parse().ok()).collect();
                    let c1_coords: Vec<f64> = parts[2].split(',').filter_map(|s| s.parse().ok()).collect();
                    let c2_coords: Vec<f64> = parts[3].split(',').filter_map(|s| s.parse().ok()).collect();
                    let end_coords: Vec<f64> = parts.get(4).unwrap_or(&"").split(',').filter_map(|s| s.parse().ok()).collect();
                    if start_coords.len() == 2 && c1_coords.len() == 2 && c2_coords.len() == 2 && end_coords.len() == 2 {
                        path.move_to(Point::new(start_coords[0], start_coords[1]));
                        path.cubic_to(
                            Point::new(c1_coords[0], c1_coords[1]),
                            Point::new(c2_coords[0], c2_coords[1]),
                            Point::new(end_coords[0], end_coords[1]),
                        );
                    }
                }
            }
            let mut shape = PathShape::new(path, style).with_source(SourceRef::edge(&edge.id));
            shape.end_markers.push(MarkerRef::new("arrowhead"));
            scene.push(shape);
        } else if edge.points.len() >= 2 {
            let p1 = edge.points[0];
            let p2 = edge.points[1];
            let mut line_path = Path::new();
            line_path.move_to(p1);
            line_path.line_to(p2);

            let mut shape = PathShape::new(line_path, style).with_source(SourceRef::edge(&edge.id));

            match msg_model.line_type {
                LineType::Solid | LineType::Dotted => {
                    shape.end_markers.push(MarkerRef::new("arrowhead"));
                }
                LineType::BidirectionalSolid | LineType::BidirectionalDotted => {
                    shape.start_markers.push(MarkerRef::new("arrowhead"));
                    shape.end_markers.push(MarkerRef::new("arrowhead"));
                }
                LineType::SolidPoint | LineType::DottedPoint => {
                    shape.end_markers.push(MarkerRef::new("filled-head"));
                }
                LineType::SolidCross | LineType::DottedCross => {
                    shape.end_markers.push(MarkerRef::new("crosshead"));
                }
                _ => {}
            }
            scene.push(shape);
        }

        // Autonumber rendering
        if msg_model.sequence_visible || conf.sequence.show_sequence_numbers {
            let is_self = msg_model.startx == msg_model.stopx;
            let is_ltr = msg_model.startx.unwrap_or(0.0) <= msg_model.stopx.unwrap_or(0.0);
            let autonumber_x = if is_self {
                msg_model.from_bounds + 1.0
            } else if is_reverse_arrow_type(msg_model.line_type) {
                if is_ltr { msg_model.to_bounds - 1.0 } else { msg_model.from_bounds + 1.0 }
            } else {
                if is_ltr { msg_model.from_bounds + 1.0 } else { msg_model.to_bounds - 1.0 }
            };

            let auto_circle = EllipseShape::new(
                Point::new(autonumber_x, *line_start_y),
                6.0,
                6.0,
                PathStyle::filled(Color::new("#000000")),
            );
            scene.push(auto_circle);

            let num_str = format!("{}", msg_model.sequence_index);
            let num_font = Font::new("sans-serif", 12.0);
            let num_dims = measure.dimensions(&num_str, &num_font);
            let num_block = TextBlock::new(
                vec![TextLine::new(vec![TextSpan::plain(&num_str)])],
                dims_size(&num_dims),
                12.0,
            );
            let mut num_run = TextRun::centered(
                Point::new(autonumber_x, line_start_y + 4.0),
                num_font,
                num_block,
            );
            num_run.fill = Some(Color::new("#ffffff"));
            num_run.baseline = TextBaseline::Middle;
            scene.push(num_run);
        }
    }

    // 7. Footer Actors (if mirrored)
    if conf.sequence.mirror_actors {
        for key in &actor_keys {
            if let Some(actor) = actors.get(key) {
                let footer_y = actor.stopy.unwrap_or(bounds.vertical_pos);
                let actor_style = PathStyle {
                    fill: Some(Color::new("#eaeaea")),
                    stroke: Some(Color::new("#666666")),
                    stroke_width: 1.0,
                    ..Default::default()
                };

                scene.push(
                    RectShape::new(
                        Rect::new(actor.x, footer_y, actor.width, actor.height),
                        3.0,
                        actor_style,
                    )
                    .with_source(SourceRef::node(&actor.name)),
                );

                let text_pos = Point::new(
                    actor.x + actor.width / 2.0,
                    footer_y + actor.height / 2.0 + 5.0,
                );
                let dims = measure.dimensions(&actor.description, &act_font);
                let block = TextBlock::new(
                    vec![TextLine::new(vec![TextSpan::plain(&actor.description)])],
                    dims_size(&dims),
                    14.0,
                );
                scene.push(TextRun::centered(text_pos, act_font.clone(), block));
            }
        }
    }

    // 8. Diagram Title
    if has_title {
        let title_x = (box_stopx - box_startx) / 2.0 - 2.0 * conf.sequence.diagram_margin_x;
        let title_font = Font::new("sans-serif", 16.0).bold();
        let dims = measure.dimensions(&ast.diagram_title, &title_font);
        let block = TextBlock::new(
            vec![TextLine::new(vec![TextSpan::plain(&ast.diagram_title)])],
            dims_size(&dims),
            16.0,
        );
        scene.push(TextRun::new(Point::new(title_x, -25.0), title_font, block));
    }

    let layout_data = SequenceLayoutData {
        actors: layout_actors,
        boxes: layout_boxes,
        messages: layout_messages,
        notes: layout_notes,
        loops: layout_loops,
        edges: layout_edges,
        view_box,
    };

    Ok((layout_data, scene))
}

/// Renders an already parsed sequence diagram.
pub fn draw_ast(
    ast: &Ast,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<Scene, SequenceRenderError> {
    draw_layout_ast(ast, config, measure).map(|(_, scene)| scene)
}

/// Renders a sequence diagram body to a [`Scene`].
pub fn draw(
    body: &str,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<Scene, SequenceRenderError> {
    let ast = super::parse(body)?;
    draw_ast(&ast, config, measure)
}

/// Renders a sequence diagram body with the crate's monospace fallback measurement.
pub fn render(body: &str, config: &Config) -> Result<Scene, SequenceRenderError> {
    draw(body, config, &FallbackMeasure)
}
