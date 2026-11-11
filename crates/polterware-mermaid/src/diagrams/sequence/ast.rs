//! The sequence database: the state upstream's `SequenceDB` holds once its parser has
//! applied every statement, read through the db's getters (`getActors`, `getMessages`,
//! `getBoxes`, `getCreatedActors`, `getDestroyedActors`, the accessibility getters, and
//! `showSequenceNumbers`).
//!
//! Renderer-only state stays out: upstream's `actorCnt` and `rectData` start as `null`
//! and only the renderer fills them, and the `notes` list is not exposed by any getter
//! (the note messages are in [`Ast::messages`] with [`LineType::Note`]).

/// A participant or actor, one entry of upstream's `actors` map.
#[derive(Clone, Debug, PartialEq)]
pub struct Actor {
    /// The map key, and the name the grammar declares it with.
    pub id: String,
    /// The declared name, always equal to `id` in the grammar.
    pub name: String,
    /// The drawn label: an `AS` alias, a metadata `alias`, or the id.
    pub description: String,
    /// Whether the label wraps (`description.wrap`, else `config.sequence.wrap`, else false).
    pub wrap: bool,
    /// The owning box, as an index into [`Ast::boxes`] (upstream `box`).
    pub box_index: Option<usize>,
    /// The actor declared before this one (upstream `prevActor`).
    pub prev_actor: Option<String>,
    /// The actor declared after this one (upstream `nextActor`).
    pub next_actor: Option<String>,
    /// Menu links from `links` and `link`.
    pub links: serde_json::Map<String, serde_json::Value>,
    /// Actor properties from `properties`.
    pub properties: serde_json::Map<String, serde_json::Value>,
    /// The participant type: `participant`, `actor`, `boundary`, `control`, `entity`,
    /// `database`, `collections`, or `queue`.
    pub actor_type: String,
}

/// A `box ... end` group.
#[derive(Clone, Debug, PartialEq)]
pub struct Box {
    /// The box title, `None` when the statement carried none (upstream `undefined`).
    pub name: Option<String>,
    /// Whether the title wraps.
    pub wrap: bool,
    /// The fill color; `transparent` when the statement carried no valid color.
    pub fill: String,
    /// The ids of the actors declared inside the box, in declaration order.
    pub actor_keys: Vec<String>,
}

/// The `type` of a message: upstream's `LINETYPE` numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum LineType {
    Solid = 0,
    Dotted = 1,
    Note = 2,
    SolidCross = 3,
    DottedCross = 4,
    SolidOpen = 5,
    DottedOpen = 6,
    LoopStart = 10,
    LoopEnd = 11,
    AltStart = 12,
    AltElse = 13,
    AltEnd = 14,
    OptStart = 15,
    OptEnd = 16,
    ActiveStart = 17,
    ActiveEnd = 18,
    ParStart = 19,
    ParAnd = 20,
    ParEnd = 21,
    RectStart = 22,
    RectEnd = 23,
    SolidPoint = 24,
    DottedPoint = 25,
    Autonumber = 26,
    CriticalStart = 27,
    CriticalOption = 28,
    CriticalEnd = 29,
    BreakStart = 30,
    BreakEnd = 31,
    ParOverStart = 32,
    BidirectionalSolid = 33,
    BidirectionalDotted = 34,
    SolidTop = 41,
    SolidBottom = 42,
    StickTop = 43,
    StickBottom = 44,
    SolidTopReverse = 45,
    SolidBottomReverse = 46,
    StickTopReverse = 47,
    StickBottomReverse = 48,
    SolidTopDotted = 51,
    SolidBottomDotted = 52,
    StickTopDotted = 53,
    StickBottomDotted = 54,
    SolidTopReverseDotted = 55,
    SolidBottomReverseDotted = 56,
    StickTopReverseDotted = 57,
    StickBottomReverseDotted = 58,
    CentralConnection = 59,
    CentralConnectionReverse = 60,
    CentralConnectionDual = 61,
}

/// Where a note sits relative to its actor(s): upstream's `PLACEMENT`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    LeftOf,
    RightOf,
    Over,
}

/// What a message carries: a label, or an `autonumber` counter instruction.
#[derive(Clone, Debug, PartialEq)]
pub enum MessageText {
    Text(String),
    SequenceIndex {
        /// The counter start, absent for a bare `autonumber`.
        start: Option<f64>,
        /// The counter step, absent for a bare `autonumber`.
        step: Option<f64>,
        /// Whether the counter is shown (`autonumber off` sets false).
        visible: bool,
    },
}

impl MessageText {
    /// The label text; empty for a counter instruction.
    pub fn as_str(&self) -> &str {
        match self {
            MessageText::Text(text) => text,
            MessageText::SequenceIndex { .. } => "",
        }
    }
}

/// One entry of upstream's `messages` list, in source order.
#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    /// The list index as a string, as upstream stores it.
    pub id: String,
    /// The sending actor; absent for block boundaries.
    pub from: Option<String>,
    /// The receiving actor; absent for block boundaries.
    pub to: Option<String>,
    pub message: MessageText,
    /// Whether the label wraps.
    pub wrap: bool,
    /// The message kind (arrow style, note, or block boundary).
    pub line_type: LineType,
    /// Set by a `+` activation suffix; false elsewhere.
    pub activate: bool,
    /// Set on note messages.
    pub placement: Option<Placement>,
    /// The central-connection marker (`59`, `60`, `61`), `0` when the message is not one.
    pub central_connection: u8,
}

/// The parsed diagram: upstream's `SequenceDB` state after `apply`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ast {
    /// The actors in declaration order (`getActors`).
    pub actors: Vec<Actor>,
    /// Created actors and the message index they were created at (`getCreatedActors`).
    pub created_actors: Vec<(String, usize)>,
    /// Destroyed actors and the message index they were destroyed at (`getDestroyedActors`).
    pub destroyed_actors: Vec<(String, usize)>,
    /// The declared boxes (`getBoxes`).
    pub boxes: Vec<Box>,
    /// Every message, note, boundary, and counter in source order (`getMessages`).
    pub messages: Vec<Message>,
    /// `showSequenceNumbers`: false after parsing; the renderer turns it on when it reads
    /// an `autonumber` message.
    pub sequence_numbers_enabled: bool,
    /// `getAccTitle`.
    pub acc_title: String,
    /// `getAccDescription`.
    pub acc_description: String,
    /// `getDiagramTitle`.
    pub diagram_title: String,
}

impl Ast {
    /// The actor with this id (`getActor`).
    pub fn actor(&self, id: &str) -> Option<&Actor> {
        self.actors.iter().find(|actor| actor.id == id)
    }

    /// The actor ids in declaration order (`getActorKeys`).
    pub fn actor_keys(&self) -> Vec<&str> {
        self.actors.iter().map(|actor| actor.id.as_str()).collect()
    }

    /// The message index a created actor was created at (`getCreatedActors`).
    pub fn created_actor(&self, id: &str) -> Option<usize> {
        self.created_actors
            .iter()
            .find(|(key, _)| key == id)
            .map(|(_, index)| *index)
    }

    /// The message index a destroyed actor was destroyed at (`getDestroyedActors`).
    pub fn destroyed_actor(&self, id: &str) -> Option<usize> {
        self.destroyed_actors
            .iter()
            .find(|(key, _)| key == id)
            .map(|(_, index)| *index)
    }
}

/// A parsed label and its `wrap:`/`nowrap:` prefix, if it had one.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Label {
    pub text: String,
    pub wrap: Option<bool>,
}

impl Label {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            wrap: None,
        }
    }
}
