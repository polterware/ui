//! The usecase database after parsing: what upstream's `UsecaseDB` getters return
//! (`usecaseDb.ts` `getActors`, `getUseCases`, `getSystemBoundaries`, `getRelationships`,
//! `getNotes`, `getJsonNodes`, `getClassDefs`, `getDirection`, `getAccTitle`,
//! `getAccDescription`, and `getAST`) plus the counters and symbol table `UsecaseFields`
//! holds.
//!
//! Upstream stores the collections in `Map`s whose insertion order is observable. Here each
//! collection is a `Vec` in the same order, with a lookup method as the `get`. `getConfig` is
//! left out because this crate owns no configuration, and `getData` belongs to the renderer.
//!
//! The graph AST ([`GraphAst`]) is the normalized, serializable view the parser builds for
//! the renderer and for canvas tooling (`usecaseAst.ts`); it is kept because upstream's
//! database commits it with the rest of the model.

use serde_json::{Map, Value};

use crate::error::Span;

/// Upstream's `LabelType`: how a label's text is interpreted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LabelType {
    #[default]
    Text,
    Markdown,
}

impl LabelType {
    pub fn as_str(self) -> &'static str {
        match self {
            LabelType::Text => "text",
            LabelType::Markdown => "markdown",
        }
    }
}

/// `ActorType`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ActorType {
    #[default]
    Normal,
    Hollow,
    Awesome,
    Icon,
}

impl ActorType {
    pub fn as_str(self) -> &'static str {
        match self {
            ActorType::Normal => "normal",
            ActorType::Hollow => "hollow",
            ActorType::Awesome => "awesome",
            ActorType::Icon => "icon",
        }
    }
}

/// `UseCaseShape`: the two shapes a use case can take.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UseCaseShape {
    #[default]
    Ellipse,
    Rect,
}

impl UseCaseShape {
    pub fn as_str(self) -> &'static str {
        match self {
            UseCaseShape::Ellipse => "ellipse",
            UseCaseShape::Rect => "rect",
        }
    }
}

/// `BoundaryType`: the geometry a system boundary is drawn with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BoundaryType {
    #[default]
    Rect,
    Package,
}

impl BoundaryType {
    pub fn as_str(self) -> &'static str {
        match self {
            BoundaryType::Rect => "rect",
            BoundaryType::Package => "package",
        }
    }
}

/// `RelationshipType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelationshipType {
    Association,
    Include,
    Extend,
    Generalization,
}

impl RelationshipType {
    pub fn as_str(self) -> &'static str {
        match self {
            RelationshipType::Association => "association",
            RelationshipType::Include => "include",
            RelationshipType::Extend => "extend",
            RelationshipType::Generalization => "generalization",
        }
    }
}

/// `Animation`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Animation {
    Fast,
    Slow,
}

impl Animation {
    pub fn as_str(self) -> &'static str {
        match self {
            Animation::Fast => "fast",
            Animation::Slow => "slow",
        }
    }
}

/// `Direction`, with `TD` normalized to `TB` as `setDirection` does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    Tb,
    Bt,
    Rl,
    #[default]
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

    /// Parses a `direction` operand; `TD` becomes `TB`.
    pub(crate) fn from_token(image: &str) -> Option<Self> {
        match image {
            "TD" | "TB" => Some(Direction::Tb),
            "BT" => Some(Direction::Bt),
            "RL" => Some(Direction::Rl),
            "LR" => Some(Direction::Lr),
            _ => None,
        }
    }
}

/// The arrow shape a relationship was parsed into (`ARROW_TYPE`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArrowType {
    SolidArrow,
    BackArrow,
    LineSolid,
    CircleArrow,
    CrossArrow,
    CircleArrowReversed,
    CrossArrowReversed,
}

impl ArrowType {
    /// The numeric code upstream stores in the graph AST.
    pub fn code(self) -> u8 {
        match self {
            ArrowType::SolidArrow => 0,
            ArrowType::BackArrow => 1,
            ArrowType::LineSolid => 2,
            ArrowType::CircleArrow => 3,
            ArrowType::CrossArrow => 4,
            ArrowType::CircleArrowReversed => 5,
            ArrowType::CrossArrowReversed => 6,
        }
    }
}

/// An actor (`Actor`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Actor {
    pub id: String,
    pub label: String,
    pub label_type: LabelType,
    pub type_: ActorType,
    pub icon: Option<String>,
    pub business: bool,
    pub stereotype: Option<String>,
    pub parent_id: Option<String>,
    pub classes: Vec<String>,
    pub styles: Vec<String>,
}

/// A use case (`UseCase`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UseCase {
    pub id: String,
    pub label: String,
    pub label_type: LabelType,
    pub shape: UseCaseShape,
    pub business: bool,
    pub stereotype: Option<String>,
    pub parent_id: Option<String>,
    pub classes: Vec<String>,
    pub styles: Vec<String>,
}

/// A system boundary (`SystemBoundary`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SystemBoundary {
    pub id: String,
    pub label: String,
    pub label_type: LabelType,
    pub type_: BoundaryType,
    pub members: Vec<String>,
    pub classes: Vec<String>,
    pub styles: Vec<String>,
}

/// A relationship edge (`Relationship`).
#[derive(Clone, Debug, PartialEq)]
pub struct Relationship {
    pub id: String,
    pub explicit_id: bool,
    pub source: String,
    pub target: String,
    pub type_: RelationshipType,
    pub arrow_type: ArrowType,
    pub label: Option<String>,
    pub label_type: Option<LabelType>,
    pub minlen: u32,
    pub classes: Vec<String>,
    pub styles: Vec<String>,
    pub animate: bool,
    pub animation: Option<Animation>,
}

/// A note attached to an actor or a use case (`UsecaseNote`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UsecaseNote {
    pub id: String,
    pub target: String,
    pub label: String,
    pub label_type: LabelType,
}

/// A JSON table node (`UsecaseJsonNode`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UsecaseJsonNode {
    pub id: String,
    /// The parsed object. Upstream's `Record<string, unknown>` keeps source order; serde's
    /// default map sorts keys, and the source order survives in `property_order`.
    pub value: Value,
    /// JSON Pointer to the property names of each object, in source order.
    pub property_order: Map<String, Value>,
    pub classes: Vec<String>,
    pub styles: Vec<String>,
}

/// A `classDef` definition (`ClassDef`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClassDef {
    pub id: String,
    pub styles: Vec<String>,
}

/// The kind of symbol an ID names (`UsecaseSymbolKind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsecaseSymbolKind {
    Actor,
    Usecase,
    Boundary,
    Json,
    Edge,
}

impl UsecaseSymbolKind {
    pub fn as_str(self) -> &'static str {
        match self {
            UsecaseSymbolKind::Actor => "actor",
            UsecaseSymbolKind::Usecase => "usecase",
            UsecaseSymbolKind::Boundary => "boundary",
            UsecaseSymbolKind::Json => "json",
            UsecaseSymbolKind::Edge => "edge",
        }
    }
}

/// A metadata property recorded on a statement (`MetadataOccurrence`).
#[derive(Clone, Debug, PartialEq)]
pub struct MetadataOccurrence {
    pub key: String,
    pub span: Span,
    pub key_span: Span,
    pub value_span: Span,
}

/// A node occurrence inside a statement (`NodeOccurrence`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NodeOccurrence {
    pub id: String,
    pub span: Span,
    pub id_span: Span,
    pub label_span: Option<Span>,
    pub defines: bool,
    pub stereotype_span: Option<Span>,
    pub metadata: Vec<MetadataOccurrence>,
    pub class_spans: Vec<Span>,
}

/// An edge occurrence inside a statement (`EdgeOccurrence`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EdgeOccurrence {
    pub id: String,
    pub span: Span,
    pub label_span: Option<Span>,
    pub id_span: Option<Span>,
    pub metadata: Vec<MetadataOccurrence>,
    pub class_spans: Vec<Span>,
}

/// The kind of a statement (`GraphStatement['kind']`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StatementKind {
    #[default]
    Node,
    Edge,
    Group,
    Note,
    Json,
    Metadata,
    EdgeMetadata,
    ClassDef,
    ClassAssign,
    Style,
    Direction,
    AccTitle,
    AccDescr,
    Comment,
    Blank,
}

/// One normalized source statement (`GraphStatement`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphStatement {
    pub kind: StatementKind,
    pub span: Span,
    pub nodes: Vec<NodeOccurrence>,
    pub edges: Vec<EdgeOccurrence>,
    pub group: Option<String>,
    pub id_span: Option<Span>,
    pub title_span: Option<Span>,
    pub end_span: Option<Span>,
    pub reference: Option<String>,
    pub reference_span: Option<Span>,
    pub children: Vec<GraphStatement>,
    pub metadata: Vec<MetadataOccurrence>,
    pub stereotype_span: Option<Span>,
    pub class_spans: Vec<Span>,
}

impl GraphStatement {
    pub(crate) fn new(kind: StatementKind, span: Span) -> Self {
        Self {
            kind,
            span,
            ..Self::default()
        }
    }
}

/// A normalized node (`GraphNode`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphNode {
    pub label: Option<String>,
    pub shape: String,
    pub classes: Vec<String>,
    pub styles: Vec<String>,
    pub attrs: Value,
}

/// A normalized edge (`GraphEdge`).
#[derive(Clone, Debug, PartialEq)]
pub struct GraphEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub label: Option<String>,
    pub classes: Vec<String>,
    pub styles: Vec<String>,
    pub attrs: Value,
}

/// A normalized group (`GraphGroup`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphGroup {
    pub title: Option<String>,
    pub nodes: Vec<String>,
    pub classes: Vec<String>,
    pub styles: Vec<String>,
    pub attrs: Value,
}

/// The graph AST header (`GraphAST['header']`).
#[derive(Clone, Debug, PartialEq)]
pub struct GraphHeader {
    pub keyword: String,
    pub direction: Direction,
    pub span: Span,
}

/// The normalized, serializable AST (`GraphAST`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphAst {
    pub version: u32,
    pub diagram_type: String,
    pub source: String,
    pub header: GraphHeader,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub nodes: Vec<(String, GraphNode)>,
    pub edges: Vec<GraphEdge>,
    pub groups: Vec<(String, GraphGroup)>,
    pub class_defs: Vec<ClassDef>,
    pub statements: Vec<GraphStatement>,
}

impl Default for GraphHeader {
    fn default() -> Self {
        Self {
            keyword: String::new(),
            direction: Direction::Lr,
            span: Span::default(),
        }
    }
}

impl GraphAst {
    pub fn node(&self, id: &str) -> Option<&GraphNode> {
        self.nodes
            .iter()
            .find(|(node_id, _)| node_id == id)
            .map(|(_, node)| node)
    }

    pub fn group(&self, id: &str) -> Option<&GraphGroup> {
        self.groups
            .iter()
            .find(|(group_id, _)| group_id == id)
            .map(|(_, group)| group)
    }

    pub fn edge(&self, id: &str) -> Option<&GraphEdge> {
        self.edges.iter().find(|edge| edge.id == id)
    }

    pub fn class_def(&self, id: &str) -> Option<&ClassDef> {
        self.class_defs.iter().find(|def| def.id == id)
    }
}

/// What upstream's usecase database holds after parsing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ast {
    pub actors: Vec<Actor>,
    pub use_cases: Vec<UseCase>,
    pub system_boundaries: Vec<SystemBoundary>,
    pub relationships: Vec<Relationship>,
    pub notes: Vec<UsecaseNote>,
    pub json_nodes: Vec<UsecaseJsonNode>,
    pub class_defs: Vec<ClassDef>,
    /// `UsecaseFields.symbols`: every declared ID and what it names.
    pub symbols: Vec<(String, UsecaseSymbolKind)>,
    pub direction: Direction,
    pub relationship_counter: usize,
    pub note_counter: usize,
    pub acc_title: String,
    pub acc_description: String,
    pub graph: GraphAst,
}

impl Ast {
    pub fn actor(&self, id: &str) -> Option<&Actor> {
        self.actors.iter().find(|actor| actor.id == id)
    }

    pub fn use_case(&self, id: &str) -> Option<&UseCase> {
        self.use_cases.iter().find(|use_case| use_case.id == id)
    }

    pub fn system_boundary(&self, id: &str) -> Option<&SystemBoundary> {
        self.system_boundaries
            .iter()
            .find(|boundary| boundary.id == id)
    }

    pub fn relationship(&self, id: &str) -> Option<&Relationship> {
        self.relationships
            .iter()
            .find(|relationship| relationship.id == id)
    }

    pub fn note(&self, id: &str) -> Option<&UsecaseNote> {
        self.notes.iter().find(|note| note.id == id)
    }

    pub fn json_node(&self, id: &str) -> Option<&UsecaseJsonNode> {
        self.json_nodes.iter().find(|node| node.id == id)
    }

    pub fn class_def(&self, id: &str) -> Option<&ClassDef> {
        self.class_defs.iter().find(|def| def.id == id)
    }

    pub fn symbol(&self, id: &str) -> Option<UsecaseSymbolKind> {
        self.symbols
            .iter()
            .find(|(symbol_id, _)| symbol_id == id)
            .map(|(_, kind)| *kind)
    }
}
