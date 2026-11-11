//! The semantics of upstream `sequenceDb.ts`: `apply` over the statements the parser
//! produced, `addActor`, `addSignal` (with its activation-count check), `addNote`,
//! `addLinks`, `addProperties`, box and title handling.

use super::ast::{Actor, Ast, Box, Label, LineType, Message, MessageText, Placement};
use super::lexer::js_trim;
use super::metadata;
use crate::error::{ParseError, Span};

/// A statement, in the shape upstream's parser passes to `SequenceDB.apply`.
pub(crate) struct Action {
    pub span: Span,
    pub kind: ActionKind,
}

/// The text and wrap flag of a box statement (`parseBoxData`).
pub(crate) struct BoxData {
    pub text: Option<String>,
    pub color: String,
    pub wrap: Option<bool>,
}

pub(crate) enum ActionKind {
    AddParticipant {
        actor: String,
        draw: Option<&'static str>,
        description: Option<Label>,
        config: Option<String>,
    },
    CreateParticipant {
        actor: String,
        draw: Option<&'static str>,
        description: Option<Label>,
        config: Option<String>,
    },
    DestroyParticipant {
        actor: String,
    },
    AddMessage {
        from: String,
        to: String,
        msg: Label,
        signal_type: LineType,
        activate: bool,
        central_connection: Option<u8>,
    },
    ActiveStart {
        actor: String,
    },
    ActiveEnd {
        actor: String,
    },
    CentralConnection {
        actor: String,
    },
    CentralConnectionReverse {
        actor: String,
    },
    AddNote {
        actors: Vec<String>,
        placement: Placement,
        text: Label,
    },
    AddLinks {
        actor: String,
        text: Label,
    },
    AddALink {
        actor: String,
        text: Label,
    },
    AddProperties {
        actor: String,
        text: Label,
    },
    AddDetails,
    SequenceIndex {
        index: Option<f64>,
        step: Option<f64>,
        visible: bool,
    },
    BoxStart {
        data: BoxData,
    },
    BoxEnd,
    LoopStart {
        text: Label,
    },
    LoopEnd,
    RectStart {
        text: Label,
    },
    RectEnd,
    OptStart {
        text: Label,
    },
    OptEnd,
    AltStart {
        text: Label,
    },
    AltElse {
        text: Label,
    },
    AltEnd,
    ParStart {
        text: Label,
        signal_type: LineType,
    },
    ParAnd {
        text: Label,
    },
    ParEnd,
    CriticalStart {
        text: Label,
    },
    CriticalOption {
        text: Label,
    },
    CriticalEnd,
    BreakStart {
        text: Label,
    },
    BreakEnd,
    SetAccTitle(String),
    SetAccDescription(String),
    SetDiagramTitle(String),
}

pub(crate) struct Db {
    actors: Vec<Actor>,
    created_actors: Vec<(String, usize)>,
    destroyed_actors: Vec<(String, usize)>,
    boxes: Vec<Box>,
    messages: Vec<Message>,
    current_box: Option<usize>,
    prev_actor: Option<String>,
    last_created: Option<String>,
    last_destroyed: Option<String>,
    acc_title: String,
    acc_description: String,
    diagram_title: String,
}

/// The parameters of upstream's `addSignal`.
struct Signal<'a> {
    from: Option<&'a str>,
    to: Option<&'a str>,
    message: Option<&'a Label>,
    line_type: LineType,
    activate: bool,
    central_connection: u8,
}

impl Db {
    pub fn new() -> Self {
        Self {
            actors: Vec::new(),
            created_actors: Vec::new(),
            destroyed_actors: Vec::new(),
            boxes: Vec::new(),
            messages: Vec::new(),
            current_box: None,
            prev_actor: None,
            last_created: None,
            last_destroyed: None,
            acc_title: String::new(),
            acc_description: String::new(),
            diagram_title: String::new(),
        }
    }

    pub fn apply_all(&mut self, actions: Vec<Action>) -> Result<(), ParseError> {
        for action in actions {
            self.apply(action)?;
        }
        Ok(())
    }

    pub fn into_ast(self) -> Ast {
        Ast {
            actors: self.actors,
            created_actors: self.created_actors,
            destroyed_actors: self.destroyed_actors,
            boxes: self.boxes,
            messages: self.messages,
            sequence_numbers_enabled: false,
            acc_title: self.acc_title,
            acc_description: self.acc_description,
            diagram_title: self.diagram_title,
        }
    }

    fn apply(&mut self, action: Action) -> Result<(), ParseError> {
        let span = action.span;
        match action.kind {
            ActionKind::AddParticipant {
                actor,
                draw,
                description,
                config,
            } => {
                self.add_actor(&actor, description, draw, config.as_deref(), span)?;
            }
            ActionKind::CreateParticipant {
                actor,
                draw,
                description,
                config,
            } => {
                if self.actor_index(&actor).is_some() {
                    return Err(ParseError::at(
                        "It is not possible to have actors with the same id, even if one is \
                         destroyed before the next is created. Use 'AS' aliases to simulate the \
                         behavior",
                        span,
                    ));
                }
                self.last_created = Some(actor.clone());
                self.add_actor(&actor, description, draw, config.as_deref(), span)?;
                self.created_actors.push((actor, self.messages.len()));
            }
            ActionKind::DestroyParticipant { actor } => {
                self.last_destroyed = Some(actor.clone());
                self.destroyed_actors.push((actor, self.messages.len()));
            }
            ActionKind::AddMessage {
                from,
                to,
                msg,
                signal_type,
                activate,
                central_connection,
            } => {
                if let Some(created) = self.last_created.clone() {
                    if to != created {
                        return Err(ParseError::at(
                            format!(
                                "The created participant {created} does not have an associated \
                                 creating message after its declaration. Please check the \
                                 sequence diagram."
                            ),
                            span,
                        ));
                    }
                    self.last_created = None;
                } else if let Some(destroyed) = self.last_destroyed.clone() {
                    if to != destroyed && from != destroyed {
                        return Err(ParseError::at(
                            format!(
                                "The destroyed participant {destroyed} does not have an associated \
                                 destroying message after its declaration. Please check the \
                                 sequence diagram."
                            ),
                            span,
                        ));
                    }
                    self.last_destroyed = None;
                }
                self.add_signal(
                    Signal {
                        from: Some(&from),
                        to: Some(&to),
                        message: Some(&msg),
                        line_type: signal_type,
                        activate,
                        central_connection: central_connection.unwrap_or(0),
                    },
                    span,
                )?;
            }
            ActionKind::ActiveStart { actor } => {
                self.add_signal(
                    Signal {
                        from: Some(&actor),
                        to: None,
                        message: None,
                        line_type: LineType::ActiveStart,
                        activate: false,
                        central_connection: 0,
                    },
                    span,
                )?;
            }
            ActionKind::ActiveEnd { actor } => {
                self.add_signal(
                    Signal {
                        from: Some(&actor),
                        to: None,
                        message: None,
                        line_type: LineType::ActiveEnd,
                        activate: false,
                        central_connection: 0,
                    },
                    span,
                )?;
            }
            ActionKind::CentralConnection { actor } => {
                self.add_signal(
                    Signal {
                        from: Some(&actor),
                        to: None,
                        message: None,
                        line_type: LineType::CentralConnection,
                        activate: false,
                        central_connection: 0,
                    },
                    span,
                )?;
            }
            ActionKind::CentralConnectionReverse { actor } => {
                self.add_signal(
                    Signal {
                        from: Some(&actor),
                        to: None,
                        message: None,
                        line_type: LineType::CentralConnectionReverse,
                        activate: false,
                        central_connection: 0,
                    },
                    span,
                )?;
            }
            ActionKind::AddNote {
                actors,
                placement,
                text,
            } => {
                self.add_note(&actors, placement, text);
            }
            ActionKind::AddLinks { actor, text } => self.add_links(&actor, &text, false),
            ActionKind::AddALink { actor, text } => self.add_links(&actor, &text, true),
            ActionKind::AddProperties { actor, text } => self.add_properties(&actor, &text),
            // Upstream reads the JSON details from a DOM element it looks up by id; with no
            // DOM the lookup throws and the catch swallows it, so this is a no-op here too.
            ActionKind::AddDetails => {}
            ActionKind::SequenceIndex {
                index,
                step,
                visible,
            } => {
                let id = self.messages.len().to_string();
                self.messages.push(Message {
                    id,
                    from: None,
                    to: None,
                    message: MessageText::SequenceIndex {
                        start: index,
                        step,
                        visible,
                    },
                    wrap: false,
                    line_type: LineType::Autonumber,
                    activate: false,
                    placement: None,
                    central_connection: 0,
                });
            }
            ActionKind::BoxStart { data } => {
                self.boxes.push(Box {
                    name: data.text,
                    wrap: data.wrap.unwrap_or(false),
                    fill: data.color,
                    actor_keys: Vec::new(),
                });
                self.current_box = Some(self.boxes.len() - 1);
            }
            ActionKind::BoxEnd => self.current_box = None,
            ActionKind::LoopStart { text } => self.add_boundary(text, LineType::LoopStart),
            ActionKind::LoopEnd => self.add_boundary_empty(LineType::LoopEnd),
            ActionKind::RectStart { text } => self.add_boundary(text, LineType::RectStart),
            ActionKind::RectEnd => self.add_boundary_empty(LineType::RectEnd),
            ActionKind::OptStart { text } => self.add_boundary(text, LineType::OptStart),
            ActionKind::OptEnd => self.add_boundary_empty(LineType::OptEnd),
            ActionKind::AltStart { text } => self.add_boundary(text, LineType::AltStart),
            ActionKind::AltElse { text } => self.add_boundary(text, LineType::AltElse),
            ActionKind::AltEnd => self.add_boundary_empty(LineType::AltEnd),
            ActionKind::ParStart { text, signal_type } => self.add_boundary(text, signal_type),
            ActionKind::ParAnd { text } => self.add_boundary(text, LineType::ParAnd),
            ActionKind::ParEnd => self.add_boundary_empty(LineType::ParEnd),
            ActionKind::CriticalStart { text } => self.add_boundary(text, LineType::CriticalStart),
            ActionKind::CriticalOption { text } => {
                self.add_boundary(text, LineType::CriticalOption)
            }
            ActionKind::CriticalEnd => self.add_boundary_empty(LineType::CriticalEnd),
            ActionKind::BreakStart { text } => self.add_boundary(text, LineType::BreakStart),
            ActionKind::BreakEnd => self.add_boundary_empty(LineType::BreakEnd),
            ActionKind::SetAccTitle(text) => self.acc_title = set_acc_title(&text),
            ActionKind::SetAccDescription(text) => {
                self.acc_description = set_acc_description(&text)
            }
            ActionKind::SetDiagramTitle(text) => self.diagram_title = text,
        }
        Ok(())
    }

    fn add_boundary(&mut self, text: Label, line_type: LineType) {
        let id = self.messages.len().to_string();
        self.messages.push(Message {
            id,
            from: None,
            to: None,
            message: MessageText::Text(text.text),
            wrap: text.wrap.unwrap_or(false),
            line_type,
            activate: false,
            placement: None,
            central_connection: 0,
        });
    }

    fn add_boundary_empty(&mut self, line_type: LineType) {
        self.add_boundary(
            Label {
                text: String::new(),
                wrap: None,
            },
            line_type,
        );
    }

    fn actor_index(&self, id: &str) -> Option<usize> {
        self.actors.iter().position(|actor| actor.id == id)
    }

    fn box_name(&self, index: usize) -> String {
        self.boxes
            .get(index)
            .and_then(|box_| box_.name.clone())
            .unwrap_or_else(|| "undefined".to_string())
    }

    fn add_actor(
        &mut self,
        id: &str,
        description: Option<Label>,
        draw: Option<&str>,
        config: Option<&str>,
        span: Span,
    ) -> Result<(), ParseError> {
        let mut assigned_box = self.current_box;
        let mut actor_type = draw.map(str::to_string);
        let doc = match config {
            Some(text) => Some(metadata::parse(text).map_err(|error| ParseError::at(error, span))?),
            None => None,
        };
        if let Some(doc) = &doc
            && let Some(kind) = &doc.type_
        {
            actor_type = Some(kind.clone());
        }
        let mut description = description;
        if let Some(doc) = &doc
            && let Some(alias) = &doc.alias
        {
            let use_alias = description
                .as_ref()
                .map(|label| label.text == id)
                .unwrap_or(true);
            if use_alias {
                let wrap = description.as_ref().and_then(|label| label.wrap);
                description = Some(Label {
                    text: alias.clone(),
                    wrap,
                });
            }
        }
        let existing = self.actor_index(id);
        if let Some(index) = existing {
            let old_box = self.actors[index].box_index;
            if let (Some(current), Some(previous)) = (self.current_box, old_box)
                && current != previous
            {
                let name = self.actors[index].name.clone();
                return Err(ParseError::at(
                    format!(
                        "A same participant should only be defined in one Box: {name} can't be \
                         in '{}' and in '{}' at the same time.",
                        self.box_name(previous),
                        self.box_name(current)
                    ),
                    span,
                ));
            }
            assigned_box = old_box.or(self.current_box);
            self.actors[index].box_index = assigned_box;
            if self.actors[index].name == id && description.is_none() {
                return Ok(());
            }
        }
        if description.is_none() {
            description = Some(Label::plain(id));
        }
        if actor_type.is_none() {
            description = Some(Label::plain(id));
        }
        let description = description.expect("a description is always set");
        let actor = Actor {
            id: id.to_string(),
            name: id.to_string(),
            description: description.text,
            wrap: description.wrap.unwrap_or(false),
            box_index: assigned_box,
            prev_actor: self.prev_actor.clone(),
            next_actor: None,
            links: serde_json::Map::new(),
            properties: serde_json::Map::new(),
            actor_type: actor_type.unwrap_or_else(|| "participant".to_string()),
        };
        match existing {
            Some(index) => self.actors[index] = actor,
            None => self.actors.push(actor),
        }
        if let Some(previous) = self.prev_actor.clone()
            && let Some(index) = self.actor_index(&previous)
        {
            self.actors[index].next_actor = Some(id.to_string());
        }
        if let Some(index) = self.current_box {
            self.boxes[index].actor_keys.push(id.to_string());
        }
        self.prev_actor = Some(id.to_string());
        Ok(())
    }

    fn activation_count(&self, part: &str) -> i32 {
        if part.is_empty() {
            return 0;
        }
        let mut count = 0;
        for message in &self.messages {
            if message.line_type == LineType::ActiveStart && message.from.as_deref() == Some(part) {
                count += 1;
            }
            if message.line_type == LineType::ActiveEnd && message.from.as_deref() == Some(part) {
                count -= 1;
            }
        }
        count
    }

    fn add_signal(&mut self, signal: Signal<'_>, span: Span) -> Result<(), ParseError> {
        let Signal {
            from,
            to,
            message,
            line_type,
            activate,
            central_connection,
        } = signal;
        if line_type == LineType::ActiveEnd {
            let part = from.unwrap_or("");
            if self.activation_count(part) < 1 {
                return Err(ParseError::at(
                    format!("Trying to inactivate an inactive participant ({part})"),
                    span,
                ));
            }
        }
        let id = self.messages.len().to_string();
        self.messages.push(Message {
            id,
            from: from.map(str::to_string),
            to: to.map(str::to_string),
            message: MessageText::Text(message.map(|label| label.text.clone()).unwrap_or_default()),
            wrap: message.and_then(|label| label.wrap).unwrap_or(false),
            line_type,
            activate,
            placement: None,
            central_connection,
        });
        Ok(())
    }

    fn add_note(&mut self, actors: &[String], placement: Placement, text: Label) {
        let from = actors.first().cloned().unwrap_or_default();
        let to = actors.get(1).cloned().unwrap_or_else(|| from.clone());
        let id = self.messages.len().to_string();
        self.messages.push(Message {
            id,
            from: Some(from),
            to: Some(to),
            message: MessageText::Text(text.text),
            wrap: text.wrap.unwrap_or(false),
            line_type: LineType::Note,
            activate: false,
            placement: Some(placement),
            central_connection: 0,
        });
    }

    fn add_links(&mut self, actor_id: &str, text: &Label, single: bool) {
        let Some(index) = self.actor_index(actor_id) else {
            return;
        };
        let sanitized = text.text.replace("&equals;", "=").replace("&amp;", "&");
        if single {
            let chars: Vec<char> = sanitized.chars().collect();
            let separator = chars.iter().position(|&ch| ch == '@');
            let end = match separator {
                Some(0) => chars.len().saturating_sub(1),
                Some(position) => position - 1,
                None => chars.len().saturating_sub(2),
            };
            let label = chars[..end.min(chars.len())]
                .iter()
                .collect::<String>()
                .trim()
                .to_string();
            let link = match separator {
                Some(position) => chars[position + 1..].iter().collect::<String>(),
                None => sanitized.clone(),
            };
            self.actors[index]
                .links
                .insert(label, serde_json::Value::String(link.trim().to_string()));
        } else if let Ok(serde_json::Value::Object(links)) = serde_json::from_str(&sanitized) {
            for (key, value) in links {
                self.actors[index].links.insert(key, value);
            }
        }
    }

    fn add_properties(&mut self, actor_id: &str, text: &Label) {
        let Some(index) = self.actor_index(actor_id) else {
            return;
        };
        if let Ok(serde_json::Value::Object(properties)) = serde_json::from_str(&text.text) {
            for (key, value) in properties {
                self.actors[index].properties.insert(key, value);
            }
        }
    }
}

/// `setAccTitle`: `sanitizeText(txt).replace(/^\s+/g, '')`.
fn set_acc_title(text: &str) -> String {
    text.trim_start_matches(is_js_space).to_string()
}

/// `setAccDescription`: `sanitizeText(txt).replace(/\n\s+/g, '\n')`.
fn set_acc_description(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        out.push(ch);
        if ch == '\n' {
            while chars.peek().is_some_and(|next| next.is_whitespace()) {
                chars.next();
            }
        }
    }
    out
}

fn is_js_space(ch: char) -> bool {
    super::lexer::is_space(ch)
}

/// `parseBoxData`'s first capture: the color function or the leading word.
fn split_box_color(raw: &str) -> (&str, &str) {
    for name in ["rgba", "rgb", "hsla", "hsl"] {
        if raw.get(..name.len()) == Some(name) {
            let rest = &raw[name.len()..];
            let spaces = rest.len() - rest.trim_start_matches([' ', '\t']).len();
            let after_spaces = &rest[spaces..];
            if after_spaces.starts_with('(')
                && let Some(close) = raw.rfind(')')
            {
                return (&raw[..=close], &raw[close + 1..]);
            }
        }
    }
    let word = raw
        .char_indices()
        .take_while(|(_, ch)| ch.is_ascii_alphanumeric() || *ch == '_')
        .last()
        .map(|(index, ch)| index + ch.len_utf8())
        .unwrap_or(0);
    (&raw[..word], &raw[word..])
}

/// Whether `CSS.supports('color', color)` would accept `color`: a named color, a CSS-wide
/// keyword, or an `rgb()`/`hsl()` function whose body is numeric. Arguments are not
/// validated the way a browser would.
fn is_css_color(color: &str) -> bool {
    let color = js_trim(color);
    if color.is_empty() {
        return false;
    }
    if let Some(body) = color
        .strip_prefix("rgb(")
        .or_else(|| color.strip_prefix("rgba("))
        .or_else(|| color.strip_prefix("hsl("))
        .or_else(|| color.strip_prefix("hsla("))
        .and_then(|rest| rest.strip_suffix(')'))
    {
        return !body.trim().is_empty()
            && body.chars().all(|ch| {
                ch.is_ascii_digit() || matches!(ch, '.' | ',' | '%' | ' ' | '/' | '+' | '-')
            });
    }
    let lower = color.to_ascii_lowercase();
    NAMED_COLORS.contains(&lower.as_str())
}

const NAMED_COLORS: &[&str] = &[
    "aliceblue",
    "antiquewhite",
    "aqua",
    "aquamarine",
    "azure",
    "beige",
    "bisque",
    "black",
    "blanchedalmond",
    "blue",
    "blueviolet",
    "brown",
    "burlywood",
    "cadetblue",
    "chartreuse",
    "chocolate",
    "coral",
    "cornflowerblue",
    "cornsilk",
    "crimson",
    "currentcolor",
    "cyan",
    "darkblue",
    "darkcyan",
    "darkgoldenrod",
    "darkgray",
    "darkgreen",
    "darkgrey",
    "darkkhaki",
    "darkmagenta",
    "darkolivegreen",
    "darkorange",
    "darkorchid",
    "darkred",
    "darksalmon",
    "darkseagreen",
    "darkslateblue",
    "darkslategray",
    "darkslategrey",
    "darkturquoise",
    "darkviolet",
    "deeppink",
    "deepskyblue",
    "dimgray",
    "dimgrey",
    "dodgerblue",
    "firebrick",
    "floralwhite",
    "forestgreen",
    "fuchsia",
    "gainsboro",
    "ghostwhite",
    "gold",
    "goldenrod",
    "gray",
    "green",
    "greenyellow",
    "grey",
    "honeydew",
    "hotpink",
    "indianred",
    "indigo",
    "inherit",
    "initial",
    "ivory",
    "khaki",
    "lavender",
    "lavenderblush",
    "lawngreen",
    "lemonchiffon",
    "lightblue",
    "lightcoral",
    "lightcyan",
    "lightgoldenrodyellow",
    "lightgray",
    "lightgreen",
    "lightgrey",
    "lightpink",
    "lightsalmon",
    "lightseagreen",
    "lightskyblue",
    "lightslategray",
    "lightslategrey",
    "lightsteelblue",
    "lightyellow",
    "lime",
    "limegreen",
    "linen",
    "magenta",
    "maroon",
    "mediumaquamarine",
    "mediumblue",
    "mediumorchid",
    "mediumpurple",
    "mediumseagreen",
    "mediumslateblue",
    "mediumspringgreen",
    "mediumturquoise",
    "mediumvioletred",
    "midnightblue",
    "mintcream",
    "mistyrose",
    "moccasin",
    "navajowhite",
    "navy",
    "oldlace",
    "olive",
    "olivedrab",
    "orange",
    "orangered",
    "orchid",
    "palegoldenrod",
    "palegreen",
    "paleturquoise",
    "palevioletred",
    "papayawhip",
    "peachpuff",
    "peru",
    "pink",
    "plum",
    "powderblue",
    "purple",
    "rebeccapurple",
    "red",
    "revert",
    "revert-layer",
    "rosybrown",
    "royalblue",
    "saddlebrown",
    "salmon",
    "sandybrown",
    "seagreen",
    "seashell",
    "sienna",
    "silver",
    "skyblue",
    "slateblue",
    "slategray",
    "slategrey",
    "snow",
    "springgreen",
    "steelblue",
    "tan",
    "teal",
    "thistle",
    "tomato",
    "transparent",
    "turquoise",
    "unset",
    "violet",
    "wheat",
    "white",
    "whitesmoke",
    "yellow",
    "yellowgreen",
];

/// `parseBoxData`: split color and title, fall back to `transparent` and the whole line
/// as the title when the color is not a CSS color, and pull out a `wrap:`/`nowrap:`
/// prefix from the title.
pub(crate) fn parse_box_data(raw: &str) -> BoxData {
    let (color_part, title_part) = split_box_color(raw);
    let mut color = if color_part.is_empty() {
        "transparent".to_string()
    } else {
        js_trim(color_part).to_string()
    };
    let mut title = if title_part.is_empty() {
        None
    } else {
        Some(js_trim(title_part).to_string())
    };
    if !is_css_color(&color) {
        color = "transparent".to_string();
        title = Some(js_trim(raw).to_string());
    }
    let (text, wrap) = extract_wrap(title.as_deref());
    BoxData {
        text: text.filter(|text| !text.is_empty()),
        color,
        wrap,
    }
}

/// `extractWrap`: a leading `:?wrap:` or `:?nowrap:` sets the flag and is removed.
pub(crate) fn extract_wrap(text: Option<&str>) -> (Option<String>, Option<bool>) {
    let Some(text) = text else {
        return (None, None);
    };
    let text = js_trim(text);
    let lowered = text.to_ascii_lowercase();
    let wrap = if lowered.starts_with("wrap:") || lowered.starts_with(":wrap:") {
        Some(true)
    } else if lowered.starts_with("nowrap:") || lowered.starts_with(":nowrap:") {
        Some(false)
    } else {
        None
    };
    let cleaned = match wrap {
        Some(_) => {
            let prefix = if text.starts_with(':') { 1 } else { 0 };
            let rest = &text[prefix..];
            let marker = if rest.to_ascii_lowercase().starts_with("nowrap:") {
                7
            } else {
                5
            };
            js_trim(&rest[marker..]).to_string()
        }
        None => text.to_string(),
    };
    (Some(cleaned), wrap)
}
