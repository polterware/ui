//! The architecture database after parsing: the records `architectureDb.ts` keeps in its
//! `nodes`, `groups`, `edges`, and `layoutHints` maps, and the three strings shared with
//! `diagrams/common/commonDb.ts` (`getDiagramTitle`, `getAccTitle`, `getAccDescription`).
//!
//! The upstream getters are [`Ast::node`] (`getNode`), [`Ast::services`] (`getServices`),
//! [`Ast::junctions`] (`getJunctions`), and the public fields for the rest. Upstream keeps
//! nodes and groups in `Map`s whose insertion order is observable; here they are `Vec`s in
//! the same order. A node also keeps the edges it touches (`ArchitectureNode.edges`), which
//! is the same list entry pushed into both endpoints, so it is copied here.
//!
//! `getDataStructures` is the one derived value: upstream computes it on first call and
//! caches it, [`Ast::data_structures`] computes it from the parsed state each time, which is
//! the same result because the state does not change after parsing.

use super::db;

/// `ArchitectureDirection`: the side of a service an edge leaves from or arrives at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Top,
    Bottom,
}

impl Direction {
    /// `isArchitectureDirectionX`.
    pub fn is_horizontal(self) -> bool {
        matches!(self, Direction::Left | Direction::Right)
    }

    /// `isArchitectureDirectionY`.
    pub fn is_vertical(self) -> bool {
        matches!(self, Direction::Top | Direction::Bottom)
    }
}

/// `ArchitectureDirectionPair`: the directions of both ends of an edge, source first.
///
/// The upstream type is the two-character string `${sourceDir}${targetDir}`, and a pair
/// whose two characters match (`LL`, `RR`, `TT`, `BB`) is not a pair at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectionPair {
    pub source: Direction,
    pub target: Direction,
}

impl DirectionPair {
    /// `getArchitectureDirectionPair`: `None` for `LL`, `RR`, `TT`, and `BB`.
    pub fn new(source: Direction, target: Direction) -> Option<Self> {
        (source != target).then_some(Self { source, target })
    }
}

/// `ArchitectureAlignment`: how two connected directions line up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alignment {
    Vertical,
    Horizontal,
    Bend,
}

impl Alignment {
    /// `getArchitectureDirectionAlignment`.
    pub fn for_directions(source: Direction, target: Direction) -> Self {
        if source.is_horizontal() != target.is_horizontal() {
            Alignment::Bend
        } else if source.is_horizontal() {
            Alignment::Horizontal
        } else {
            Alignment::Vertical
        }
    }
}

/// An `ArchitectureGroup` (`groups` in the upstream database).
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub id: String,
    /// `ARCH_ICON`, with the parentheses removed.
    pub icon: Option<String>,
    /// `ARCH_TITLE`, with the brackets and outer quotes removed.
    pub title: Option<String>,
    /// The `in` parent, which the database requires to be a group declared before it.
    pub container: Option<String>,
}

/// An `ArchitectureService` (`getServices`).
#[derive(Clone, Debug, PartialEq)]
pub struct Service {
    pub id: String,
    /// `ARCH_ICON`, with the parentheses removed.
    pub icon: Option<String>,
    /// A `STRING` icon, which the grammar accepts in place of `ARCH_ICON`.
    pub icon_text: Option<String>,
    /// `ARCH_TITLE`, with the brackets and outer quotes removed.
    pub title: Option<String>,
    /// The `in` parent, which the database requires to be a group declared before it.
    pub container: Option<String>,
    /// The edges this node touches, in the order `addEdge` pushed them.
    pub edges: Vec<Edge>,
}

/// An `ArchitectureJunction` (`getJunctions`).
#[derive(Clone, Debug, PartialEq)]
pub struct Junction {
    pub id: String,
    /// The `in` parent, which the database requires to be a group declared before it.
    pub container: Option<String>,
    /// The edges this node touches, in the order `addEdge` pushed them.
    pub edges: Vec<Edge>,
}

/// `ArchitectureNode`: the service or junction union the `nodes` map holds.
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Service(Service),
    Junction(Junction),
}

impl Node {
    pub fn id(&self) -> &str {
        match self {
            Node::Service(service) => &service.id,
            Node::Junction(junction) => &junction.id,
        }
    }

    /// The `in` parent of the node.
    pub fn container(&self) -> Option<&str> {
        match self {
            Node::Service(service) => service.container.as_deref(),
            Node::Junction(junction) => junction.container.as_deref(),
        }
    }

    pub fn edges(&self) -> &[Edge] {
        match self {
            Node::Service(service) => &service.edges,
            Node::Junction(junction) => &junction.edges,
        }
    }

    pub fn as_service(&self) -> Option<&Service> {
        match self {
            Node::Service(service) => Some(service),
            Node::Junction(_) => None,
        }
    }

    pub fn as_junction(&self) -> Option<&Junction> {
        match self {
            Node::Service(_) => None,
            Node::Junction(junction) => Some(junction),
        }
    }

    /// `lhsNode.edges.push(edge)` / `rhsNode.edges.push(edge)`.
    pub(crate) fn push_edge(&mut self, edge: Edge) {
        match self {
            Node::Service(service) => service.edges.push(edge),
            Node::Junction(junction) => junction.edges.push(edge),
        }
    }
}

/// `ArchitectureEdge`, as `addEdge` stores it.
#[derive(Clone, Debug, PartialEq)]
pub struct Edge {
    pub lhs_id: String,
    pub lhs_dir: Direction,
    /// `ARROW_INTO` before the edge body (`:R<--`).
    pub lhs_into: bool,
    /// The `{group}` modifier after the left-hand id.
    pub lhs_group: bool,
    pub rhs_id: String,
    pub rhs_dir: Direction,
    /// `ARROW_INTO` after the edge body (`-->L:`).
    pub rhs_into: bool,
    /// The `{group}` modifier after the right-hand id.
    pub rhs_group: bool,
    /// The `ARCH_TITLE` of a `-[title]-` edge.
    pub title: Option<String>,
}

/// `ArchitectureAlignmentDirection`: the axis an `align` directive pins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlignmentDirection {
    Row,
    Column,
}

impl AlignmentDirection {
    /// The word upstream prints in its error messages.
    pub fn as_str(self) -> &'static str {
        match self {
            AlignmentDirection::Row => "row",
            AlignmentDirection::Column => "column",
        }
    }
}

/// `ArchitectureLayoutHint`, as `addLayoutHint` stores it.
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutHint {
    pub direction: AlignmentDirection,
    pub members: Vec<String>,
}

/// What upstream's architecture database holds after parsing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ast {
    pub groups: Vec<Group>,
    /// The `nodes` map: every service in declaration order, then every junction, which is
    /// the order `populateDb` writes them in.
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub layout_hints: Vec<LayoutHint>,
    /// `getDiagramTitle()`; empty when the diagram has no title.
    pub diagram_title: String,
    /// `getAccTitle()`; empty when the diagram has no accessible title.
    pub acc_title: String,
    /// `getAccDescription()`; empty when the diagram has no accessible description.
    pub acc_description: String,
}

impl Ast {
    /// `getNode(id)`.
    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|node| node.id() == id)
    }

    /// `getServices()`.
    pub fn services(&self) -> impl Iterator<Item = &Service> {
        self.nodes.iter().filter_map(Node::as_service)
    }

    /// `getJunctions()`.
    pub fn junctions(&self) -> impl Iterator<Item = &Junction> {
        self.nodes.iter().filter_map(Node::as_junction)
    }

    /// `getDataStructures()`: the adjacency list, the spatial maps of its connected
    /// components, and the alignments between groups.
    pub fn data_structures(&self) -> DataStructures {
        db::data_structures(self)
    }
}

/// `ArchitectureDataStructures`. The upstream maps are `Vec` pairs in insertion order, and
/// a lookup method stands in for `Map.get`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DataStructures {
    /// `adjList`: each node in the `nodes` map's order, with the direction pairs of its edges
    /// and the neighbour at the other end.
    pub adjacency: Vec<(String, Vec<(DirectionPair, String)>)>,
    /// `spatialMaps`: one map per connected component, each node and its BFS position.
    pub spatial_maps: Vec<Vec<(String, (i32, i32))>>,
    /// `groupAlignments`: `architectureGroupAlignmentKey(a, b)` and the alignment, never
    /// `Bend` (upstream skips those).
    pub group_alignments: Vec<(String, Alignment)>,
}

impl DataStructures {
    /// `adjList.get(id)`.
    pub fn adjacency_of(&self, id: &str) -> Option<&[(DirectionPair, String)]> {
        self.adjacency
            .iter()
            .find(|(node_id, _)| node_id == id)
            .map(|(_, directions)| directions.as_slice())
    }

    /// `groupAlignments.get(architectureGroupAlignmentKey(a, b))`.
    pub fn group_alignment(&self, group_a: &str, group_b: &str) -> Option<Alignment> {
        let key = group_alignment_key(group_a, group_b);
        self.group_alignments
            .iter()
            .find(|(existing, _)| *existing == key)
            .map(|(_, alignment)| *alignment)
    }
}

/// `architectureGroupAlignmentKey`: the two ids in sorted order, JSON-quoted, joined by a
/// dash. The sort is byte order, which matches JavaScript's UTF-16 code unit order for the
/// ASCII ids the grammar produces.
pub fn group_alignment_key(group_a: &str, group_b: &str) -> String {
    let (lower, upper) = if group_a <= group_b {
        (group_a, group_b)
    } else {
        (group_b, group_a)
    };
    format!("{}-{}", json_string(lower), json_string(upper))
}

/// `JSON.stringify` for a string: quotes, and escapes quote, backslash, and control
/// characters. Group ids are `ID` tokens, so only the quotes are ever added.
fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}
