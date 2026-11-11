//! The state database after parsing: what upstream's `StateDB` getters return
//! (`stateDb.ts` `getStates`, `getRelations`, `getClasses`, `getLinks`, `getDirection`,
//! `getData`, and the common accessibility getters).
//!
//! `getData` returns the layout data `dataFetcher.ts` derives from the parsed statements:
//! [`Ast::nodes`] and [`Ast::edges`], with the labels already folded by `extract`'s last
//! pass (`node.description = node.label.slice(1)`, `node.label = node.label[0]`). Fields
//! that only mirror configuration (`look`, `wrappingWidth`, `minNodeWidth`) are left out:
//! this crate owns no config; where a value depends on it, the documented default is used.
//!
//! The grammar's intermediate statement tree is exposed too, because the getters return it:
//! a state's `doc` is the list of statements inside its braces, and `getStates().get(id).doc`
//! is what callers like the renderer walk.

/// A diagram direction (`stateCommon.ts` `DEFAULT_DIAGRAM_DIRECTION` is `TB`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    #[default]
    Tb,
    Bt,
    Rl,
    Lr,
}

impl Direction {
    pub fn as_str(self) -> &'static str {
        match self {
            Direction::Tb => "TB",
            Direction::Bt => "BT",
            Direction::Rl => "RL",
            Direction::Lr => "LR",
        }
    }
}

/// A state's `type` (`StateStmt['type']`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StateKind {
    #[default]
    Default,
    Fork,
    Join,
    Choice,
    Divider,
    Start,
    End,
}

impl StateKind {
    pub fn as_str(self) -> &'static str {
        match self {
            StateKind::Default => "default",
            StateKind::Fork => "fork",
            StateKind::Join => "join",
            StateKind::Choice => "choice",
            StateKind::Divider => "divider",
            StateKind::Start => "start",
            StateKind::End => "end",
        }
    }
}

/// Where a note sits relative to its state (`Note.position`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotePosition {
    LeftOf,
    RightOf,
}

impl NotePosition {
    pub fn as_str(self) -> &'static str {
        match self {
            NotePosition::LeftOf => "left of",
            NotePosition::RightOf => "right of",
        }
    }
}

/// The stroke pattern an edge carries (`Edge.pattern`); only the note edge sets it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgePattern {
    Solid,
    Dotted,
    Dashed,
}

impl EdgePattern {
    pub fn as_str(self) -> &'static str {
        match self {
            EdgePattern::Solid => "solid",
            EdgePattern::Dotted => "dotted",
            EdgePattern::Dashed => "dashed",
        }
    }
}

/// A note attached to a state (`Note`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    pub position: Option<NotePosition>,
    pub text: String,
}

/// A state declaration inside a document (`StateStmt`).
#[derive(Clone, Debug, PartialEq)]
pub struct StateStatement {
    pub id: String,
    /// The grammar's `type`; `None` where upstream leaves it `undefined` (a note statement).
    pub state_type: Option<StateKind>,
    /// The declared description. Upstream holds a string, or an array when the `as` id
    /// contained a colon (`state "name" as id: extra`); the array case is flattened here.
    pub description: Vec<String>,
    /// The statements inside the state's braces, when it has any.
    pub doc: Option<Vec<Statement>>,
    pub note: Option<Note>,
    /// Classes attached with `:::`, in declaration order.
    pub classes: Vec<String>,
    /// `true` for a `[*]` renamed `_start`, `false` for `_end` (`docTranslator`).
    pub start: Option<bool>,
}

impl StateStatement {
    /// Whether upstream sees a description here. A description is a string, and `''` is
    /// falsy; the colon form of an `as` id produces an array, which is always truthy.
    pub(crate) fn description_is_set(&self) -> bool {
        self.description.len() > 1
            || self
                .description
                .first()
                .is_some_and(|description| !description.is_empty())
    }
}

/// A transition (`RelationStmt`).
#[derive(Clone, Debug, PartialEq)]
pub struct RelationStatement {
    pub state1: StateStatement,
    pub state2: StateStatement,
    pub description: Option<String>,
}

/// One parsed statement (`Stmt`), plus the bare values the grammar's default actions leave
/// in the document (`Other`).
#[derive(Clone, Debug, PartialEq)]
pub enum Statement {
    State(StateStatement),
    Relation(RelationStatement),
    ClassDef {
        id: String,
        classes: String,
    },
    Style {
        id: String,
        style_class: String,
    },
    ApplyClass {
        id: String,
        style_class: String,
    },
    Direction(Direction),
    Click {
        id: String,
        url: String,
        tooltip: String,
    },
    /// A statement whose grammar action returns a plain value (`hide empty description`,
    /// `scale`, a bare `state name`, the floating-note `note`, an accessibility line).
    Other(String),
}

/// A state in the database (`getStates`), in insertion order.
#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub id: String,
    pub descriptions: Vec<String>,
    pub state_type: StateKind,
    pub doc: Option<Vec<Statement>>,
    pub note: Option<Note>,
    pub classes: Vec<String>,
    pub styles: Vec<String>,
    pub text_styles: Vec<String>,
}

/// A transition in the database (`getRelations`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Relation {
    pub id1: String,
    pub id2: String,
    pub relation_title: String,
}

/// A `classDef` style class (`getClasses`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyleClass {
    pub id: String,
    pub styles: Vec<String>,
    pub text_styles: Vec<String>,
}

/// A `click` link (`getLinks`). Upstream keys the map by the parsed id statement object;
/// this port keeps the id string, which is what the renderer reads out of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub id: String,
    pub url: String,
    pub tooltip: String,
}

/// A node of `getData()` (`NodeData`), after `extract`'s label pass.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: String,
    pub shape: Option<String>,
    pub label: String,
    /// The extra labels: `label.slice(1)` after the label pass (usually empty).
    pub description: Vec<String>,
    pub css_classes: String,
    pub css_compiled_styles: Vec<String>,
    pub css_styles: Vec<String>,
    pub dir: Option<Direction>,
    pub dom_id: String,
    pub node_type: Option<String>,
    pub is_group: bool,
    pub color_index: Option<u32>,
    pub padding: Option<f64>,
    pub rx: Option<f64>,
    pub ry: Option<f64>,
    pub center_label: Option<bool>,
    pub parent_id: Option<String>,
    pub position: Option<NotePosition>,
    pub label_style: Option<String>,
    pub label_type: Option<String>,
}

/// An edge of `getData()` (`Edge`).
#[derive(Clone, Debug, PartialEq)]
pub struct Edge {
    pub id: String,
    pub start: String,
    pub end: String,
    pub arrowhead: String,
    pub arrow_type_end: String,
    pub style: String,
    pub label_style: String,
    pub label: Option<String>,
    pub arrowhead_style: String,
    pub labelpos: String,
    pub label_type: String,
    pub thickness: String,
    pub classes: String,
    pub pattern: Option<EdgePattern>,
}

/// The parsed diagram.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ast {
    /// `getStates()`, in insertion order.
    pub states: Vec<State>,
    /// `getRelations()`.
    pub relations: Vec<Relation>,
    /// `getClasses()`.
    pub classes: Vec<StyleClass>,
    /// `getLinks()`.
    pub links: Vec<Link>,
    /// `getData().nodes`.
    pub nodes: Vec<Node>,
    /// `getData().edges`.
    pub edges: Vec<Edge>,
    /// `getDirection()`: the first top-level `direction` statement, else `TB`.
    pub direction: Direction,
    /// `getData().direction`: the last `direction` in the (translated) root document.
    pub layout_direction: Direction,
    /// `getAccTitle()`.
    pub acc_title: String,
    /// `getAccDescription()`.
    pub acc_description: String,
    /// `getDiagramTitle()`; this grammar has no title statement, so it stays empty.
    pub diagram_title: String,
}
