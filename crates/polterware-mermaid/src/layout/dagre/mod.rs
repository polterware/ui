//! Port of `dagre-d3-es` 7.0.14, the fork of
//! [dagre](https://github.com/dagrejs/dagre) that Mermaid 12.3.0 lays diagrams out
//! with (`src/dagre/` in
//! `research/mermaid-port/upstream/mermaid/node_modules/.pnpm/dagre-d3-es@7.0.14`),
//! over [`super::graphlib`] (its sibling `src/graphlib/`).
//!
//! Follows `src/dagre/`: `layout.js` and its full pipeline, `acyclic.js`,
//! `normalize.js`, `parent-dummy-chains.js`, `nesting-graph.js`,
//! `add-border-segments.js`, `coordinate-system.js`, `greedy-fas.js`,
//! `data/list.js`, `util.js`, `rank/`, `order/`, and `position/` (`bk.js`).
//! Iteration orders and tie-breaking are upstream's: layout depends on graphlib's
//! insertion order, and on the order of every JavaScript object and array it walks.
//!
//! dagre-d3-es is the pre-3.x dagre code base, so the newer `@dagrejs/dagre`
//! features are deliberately absent: no per-cluster `rankdir` in `layout.js`, no
//! `customOrder`, `constraints`, or `disableOptimalOrderHeuristic` in `order`, no
//! previous-layout memory in `acyclic`, `order`, or `position`, and no `rankalign`
//! graph attribute. Where an earlier version of this port carried them, the port
//! now follows dagre-d3-es instead.
//!
//! Where upstream labels are plain JavaScript objects, this port uses typed labels
//! ([`GraphLabel`], [`NodeLabel`], [`EdgeLabel`]). Attribute names are the Rust
//! fields; arbitrary extra attributes are not carried through layout (upstream's
//! `buildLayoutGraph` only copies a whitelist anyway). Upstream canonicalizes
//! attribute names to lower case, which typed labels make unnecessary; values keep
//! their case, as upstream's do. `undefined` numeric attributes are [`Option`]s,
//! and `undefined`/`null` labels stay empty. Upstream shares label objects between
//! the input graph and the graphs it builds; typed labels are copied instead, and
//! `order` republishes orders where the shared object would have carried them.
//!
//! Not ported because they carry no layout behavior: `debug.js` (`debugOrdering`),
//! `version.js`, the timing wrapper of `util.time`, and the package entry points.
//!
//! These functions throw where upstream throws: [`LayoutError`]. Graph operations
//! that layout only performs on graphs it built (setting a parent, for example) are
//! asserted instead.
//!
//! Differential fixtures from dagre-d3-es 7.0.14 live in `testdata/` and are
//! compared by `differential.rs`. The upstream unit tests are ported into each
//! module, with two adaptations: layer graphs copy node labels instead of sharing
//! them (so `order` republishes orders after each layer), and tests that relied on
//! JavaScript object identity or console logging are written against the typed
//! labels.

pub mod acyclic;
pub mod add_border_segments;
pub mod coordinate_system;
pub mod data;
pub mod greedy_fas;
mod layout;
pub mod nesting_graph;
pub mod normalize;
pub mod order;
pub mod parent_dummy_chains;
pub mod position;
pub mod rank;
pub mod util;

#[cfg(test)]
mod differential;
#[cfg(test)]
pub(crate) mod test_support;

use std::cmp::Ordering;

pub use layout::{build_layout_graph, layout, update_input_graph};

/// A point in layout coordinates, upstream's `Point`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    /// Horizontal coordinate.
    pub x: f64,
    /// Vertical coordinate.
    pub y: f64,
}

impl Point {
    /// A point at `(x, y)`.
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// The label of a layout graph, upstream's `GraphLabel`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphLabel {
    /// Width of the laid-out drawing.
    pub width: Option<f64>,
    /// Height of the laid-out drawing.
    pub height: Option<f64>,
    /// Direction of ranks: `TB`, `BT`, `LR`, or `RL` (any case).
    pub rankdir: Option<String>,
    /// Horizontal alignment of the drawing: `UL`, `UR`, `DL`, or `DR`.
    pub align: Option<String>,
    /// Separation between nodes in a rank.
    pub nodesep: Option<f64>,
    /// Separation between edges in a rank.
    pub edgesep: Option<f64>,
    /// Separation between ranks.
    pub ranksep: Option<f64>,
    /// Horizontal margin added around the drawing.
    pub marginx: Option<f64>,
    /// Vertical margin added around the drawing.
    pub marginy: Option<f64>,
    /// Cycle breaker: `greedy` selects [`greedy_fas`]; anything else uses the DFS.
    pub acyclicer: Option<String>,
    /// Ranker: `network-simplex`, `tight-tree`, `longest-path`, or `none`.
    pub ranker: Option<String>,
    /// The root dummy node of the nesting graph.
    pub nesting_root: Option<String>,
    /// The multiplier used to keep subgraph border ranks empty.
    pub node_rank_factor: Option<f64>,
    /// First dummy of each chain created by `normalize`.
    pub dummy_chains: Option<Vec<String>>,
    /// Highest rank assigned to a subgraph.
    pub max_rank: Option<f64>,
    /// The synthetic root of an ordering layer graph.
    pub root: Option<String>,
}

/// The kind of a dummy node, upstream's `dummy` union plus the truthy flag some
/// callers use. Values are compared as strings, as upstream compares them.
pub type DummyKind = str;

/// The label of a layout node, upstream's `NodeLabel`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NodeLabel {
    /// Width of the node.
    pub width: f64,
    /// Height of the node.
    pub height: f64,
    /// Center x coordinate, once positioned.
    pub x: Option<f64>,
    /// Center y coordinate, once positioned.
    pub y: Option<f64>,
    /// Rank of the node.
    pub rank: Option<f64>,
    /// Position of the node inside its rank.
    pub order: Option<usize>,
    /// Dummy kind: `root`, `edge`, `border`, `edge-label`, `edge-proxy`, `selfedge`.
    pub dummy: Option<String>,
    /// The original edge, on an edge-proxy or selfedge dummy.
    pub e: Option<super::graphlib::Edge>,
    /// The original edge, on a normalized dummy.
    pub edge_obj: Option<super::graphlib::Edge>,
    /// The original edge label, on a normalized or self-edge dummy.
    pub edge_label: Option<EdgeLabel>,
    /// Self edges removed before ranking, restored after ordering.
    pub self_edges: Option<Vec<SelfEdge>>,
    /// `borderLeft` or `borderRight` on a border dummy.
    pub border_type: Option<String>,
    /// Top border dummy of a subgraph.
    pub border_top: Option<String>,
    /// Bottom border dummy of a subgraph.
    pub border_bottom: Option<String>,
    /// Left border dummy of a subgraph, indexed by rank.
    pub border_left: Vec<Option<String>>,
    /// Right border dummy of a subgraph, indexed by rank.
    pub border_right: Vec<Option<String>>,
    /// The left border node of a subgraph inside an ordering layer graph.
    ///
    /// Upstream reuses `borderLeft` for this, where `buildLayerGraph` stores the
    /// string at the layer's rank; the typed label keeps the rank chain in
    /// [`NodeLabel::border_left`] and the singular layer reference here.
    pub border_left_node: Option<String>,
    /// The right border node of a subgraph inside an ordering layer graph.
    pub border_right_node: Option<String>,
    /// Lowest rank of a subgraph.
    pub min_rank: Option<f64>,
    /// Highest rank of a subgraph.
    pub max_rank: Option<f64>,
    /// Label position of an edge label dummy: `l`, `c`, or `r`.
    pub labelpos: Option<String>,
    /// Direction of a cluster's own layout.
    pub rankdir: Option<String>,
}

/// A self edge removed before layout, upstream's `SelfEdge`.
#[derive(Clone, Debug, PartialEq)]
pub struct SelfEdge {
    /// The original edge.
    pub e: super::graphlib::Edge,
    /// The original edge label.
    pub label: EdgeLabel,
}

/// The label of a layout edge, upstream's `EdgeLabel`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EdgeLabel {
    /// Control points of the edge route.
    pub points: Option<Vec<Point>>,
    /// Width of the edge label.
    pub width: Option<f64>,
    /// Height of the edge label.
    pub height: Option<f64>,
    /// Minimum length of the edge, in ranks.
    pub minlen: Option<f64>,
    /// Weight of the edge for ranking and ordering.
    pub weight: Option<f64>,
    /// Position of the edge label: `l`, `c`, or `r`.
    pub labelpos: Option<String>,
    /// Distance of an `l` or `r` label from the edge.
    pub labeloffset: Option<f64>,
    /// Rank where the edge label sits.
    pub label_rank: Option<f64>,
    /// Center x of the edge label, once positioned.
    pub x: Option<f64>,
    /// Center y of the edge label, once positioned.
    pub y: Option<f64>,
    /// Whether `acyclic` reversed this edge.
    pub reversed: bool,
    /// Name of the edge before it was reversed.
    pub forward_name: Option<String>,
    /// Whether `nesting_graph` created this edge.
    pub nesting_edge: bool,
    /// Cut value of the edge in the feasible tree.
    pub cutvalue: Option<f64>,
}

impl EdgeLabel {
    /// The width, defaulting to 0 when the label has none.
    pub fn width(&self) -> f64 {
        self.width.unwrap_or(0.0)
    }

    /// The height, defaulting to 0 when the label has none.
    pub fn height(&self) -> f64 {
        self.height.unwrap_or(0.0)
    }

    /// The minimum length, defaulting to 0 when the label has none.
    pub fn minlen(&self) -> f64 {
        self.minlen.unwrap_or(0.0)
    }

    /// The weight, defaulting to 0 when the label has none.
    pub fn weight(&self) -> f64 {
        self.weight.unwrap_or(0.0)
    }

    /// The label offset, defaulting to 0 when the label has none.
    pub fn labeloffset(&self) -> f64 {
        self.labeloffset.unwrap_or(0.0)
    }
}

/// A failure of a layout operation: the conditions under which upstream throws.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayoutError {
    /// `feasible-tree` was asked to rank an empty graph.
    GraphMustHaveAtLeastOneNode,
    /// `intersect-rect` was asked for the border through the rectangle's center.
    NoIntersection,
    /// A graphlib operation failed.
    Graph(String),
}

impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GraphMustHaveAtLeastOneNode => f.write_str("Graph must have at least one node"),
            Self::NoIntersection => {
                f.write_str("Not possible to find intersection inside of the rectangle")
            }
            Self::Graph(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for LayoutError {}

impl From<super::graphlib::GraphError> for LayoutError {
    fn from(error: super::graphlib::GraphError) -> Self {
        Self::Graph(error.to_string())
    }
}

/// A dagre layout graph: a multigraph with compound nodes and typed labels.
pub type LayeredGraph = super::graphlib::Graph<GraphLabel, NodeLabel, EdgeLabel>;

/// Upstream compares strings with JavaScript's `<`, which orders UTF-16 code units.
pub(crate) fn js_greater_than(a: &str, b: &str) -> bool {
    a.encode_utf16().cmp(b.encode_utf16()) == Ordering::Greater
}
