//! The flowchart database after parsing: what upstream's `FlowDB` getters return
//! (`flowDb.ts` `getVertices`, `getEdges`, `getClasses`, `getSubGraphs`, `getDirection`,
//! `getAccTitle`, `getAccDescription`, and `getData`).
//!
//! Upstream keeps DOM wiring (tooltips, click callbacks, rendering config) out of these
//! getters, so the parsed shape lives here as plain data: the raw vertices and edges the
//! grammar builds, and the layout nodes and edges `getData` derives from them.

use serde_json::{Map, Value};

/// Metadata attached with `@{ ... }` to a node, an edge, or a subgraph.
pub type Metadata = Map<String, Value>;

/// Upstream's `labelType`: how a label's text is interpreted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LabelType {
    #[default]
    Text,
    String,
    Markdown,
}

impl LabelType {
    pub fn as_str(self) -> &'static str {
        match self {
            LabelType::Text => "text",
            LabelType::String => "string",
            LabelType::Markdown => "markdown",
        }
    }
}

/// The stroke a link was drawn with (`destructLink`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stroke {
    Normal,
    Thick,
    Dotted,
    Invisible,
    /// The two ends of the link disagreed (`destructLink` reports `INVALID`).
    Invalid,
}

impl Stroke {
    pub fn as_str(self) -> &'static str {
        match self {
            Stroke::Normal => "normal",
            Stroke::Thick => "thick",
            Stroke::Dotted => "dotted",
            Stroke::Invisible => "invisible",
            Stroke::Invalid => "INVALID",
        }
    }
}

/// The arrow type a link was parsed into (`destructLink`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArrowType {
    Open,
    Point,
    Cross,
    Circle,
    DoublePoint,
    DoubleCross,
    DoubleCircle,
    /// The two ends disagreed, so upstream reports `INVALID` for both type and stroke.
    Invalid,
}

impl ArrowType {
    pub fn as_str(self) -> &'static str {
        match self {
            ArrowType::Open => "arrow_open",
            ArrowType::Point => "arrow_point",
            ArrowType::Cross => "arrow_cross",
            ArrowType::Circle => "arrow_circle",
            ArrowType::DoublePoint => "double_arrow_point",
            ArrowType::DoubleCross => "double_arrow_cross",
            ArrowType::DoubleCircle => "double_arrow_circle",
            ArrowType::Invalid => "INVALID",
        }
    }
}

/// The arrowhead end of a layout edge (`destructEdgeType`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ArrowEnd {
    #[default]
    None,
    Point,
    Cross,
    Circle,
}

impl ArrowEnd {
    pub fn as_str(self) -> &'static str {
        match self {
            ArrowEnd::None => "none",
            ArrowEnd::Point => "arrow_point",
            ArrowEnd::Cross => "arrow_cross",
            ArrowEnd::Circle => "arrow_circle",
        }
    }
}

/// A node stored by the grammar (`FlowVertex`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Vertex {
    pub id: String,
    pub text: Option<String>,
    pub label_type: LabelType,
    pub dom_id: String,
    /// The grammar type (`square`, `round`, `diamond`, ...) or a metadata `shape`.
    pub type_name: Option<String>,
    pub styles: Vec<String>,
    pub classes: Vec<String>,
    pub dir: Option<String>,
    pub props: Metadata,
    pub link: Option<String>,
    pub link_target: Option<String>,
    pub have_callback: bool,
    pub icon: Option<String>,
    pub form: Option<String>,
    pub pos: Option<String>,
    pub img: Option<String>,
    pub asset_width: Option<f64>,
    pub asset_height: Option<f64>,
    pub constraint: Option<String>,
}

/// A link stored by the grammar (`FlowEdge`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Edge {
    pub is_user_defined_id: bool,
    pub start: String,
    pub end: String,
    pub interpolate: Option<String>,
    pub type_name: Option<ArrowType>,
    pub stroke: Option<Stroke>,
    pub style: Option<Vec<String>>,
    pub length: Option<i64>,
    pub text: String,
    pub label_type: LabelType,
    pub classes: Vec<String>,
    pub id: Option<String>,
    pub animation: Option<String>,
    pub animate: Option<bool>,
    pub step: Option<u32>,
}

/// A class definition (`FlowClass`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FlowClass {
    pub id: String,
    pub styles: Vec<String>,
    pub text_styles: Vec<String>,
}

/// A subgraph (`FlowSubGraph`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SubGraph {
    pub id: String,
    pub title: String,
    pub nodes: Vec<String>,
    pub classes: Vec<String>,
    pub dir: Option<String>,
    pub label_type: LabelType,
    pub metadata: Option<Metadata>,
}

/// A node a subgraph listed but lost to an earlier, unrelated subgraph that lists it too.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DroppedSubGraphMember {
    pub node: String,
    pub kept_by: String,
    pub dropped_from: String,
}

/// A tooltip set by `click <id> ... "<tooltip>"`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tooltip {
    pub id: String,
    pub text: String,
}

/// A node `getData` hands to the layout engine (`Node`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub label_style: String,
    pub label_type: LabelType,
    pub parent_id: Option<String>,
    pub padding: f64,
    pub css_styles: Vec<String>,
    pub css_compiled_styles: Vec<String>,
    pub css_classes: String,
    pub shape: String,
    pub dir: Option<String>,
    pub is_group: bool,
    pub look: Option<String>,
    pub color_index: Option<usize>,
    pub dom_id: Option<String>,
    pub link: Option<String>,
    pub link_target: Option<String>,
    pub tooltip: Option<String>,
    pub icon: Option<String>,
    pub pos: Option<String>,
    pub img: Option<String>,
    pub asset_width: Option<f64>,
    pub asset_height: Option<f64>,
    pub min_width: Option<f64>,
    pub constraint: Option<String>,
    pub metadata: Option<Metadata>,
}

/// An edge `getData` hands to the layout engine (`Edge`).
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutEdge {
    pub id: String,
    pub is_user_defined_id: bool,
    pub start: String,
    pub end: String,
    pub type_name: String,
    pub label: String,
    pub label_type: LabelType,
    pub labelpos: String,
    pub thickness: Option<Stroke>,
    pub minlen: Option<i64>,
    pub classes: String,
    pub arrow_type_start: ArrowEnd,
    pub arrow_type_end: ArrowEnd,
    pub arrowhead_style: String,
    pub css_compiled_styles: Vec<String>,
    pub label_style: Vec<String>,
    pub style: Vec<String>,
    pub pattern: Option<Stroke>,
    pub look: Option<String>,
    pub animate: Option<bool>,
    pub animation: Option<String>,
    pub step: Option<u32>,
    pub step_count: Option<u32>,
    pub step_duration: Option<f64>,
    pub show_step_number: Option<bool>,
    pub curve: Option<String>,
}

/// The parsed diagram.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ast {
    pub direction: Option<String>,
    pub acc_title: String,
    pub acc_description: String,
    pub vertices: Vec<Vertex>,
    pub edges: Vec<Edge>,
    /// `edges.defaultInterpolate` in upstream.
    pub default_interpolate: Option<String>,
    /// `edges.defaultStyle` in upstream.
    pub default_style: Option<Vec<String>>,
    pub classes: Vec<FlowClass>,
    pub sub_graphs: Vec<SubGraph>,
    pub dropped_sub_graph_members: Vec<DroppedSubGraphMember>,
    pub tooltips: Vec<Tooltip>,
    /// `getData().nodes`.
    pub nodes: Vec<Node>,
    /// `getData().edges`.
    pub layout_edges: Vec<LayoutEdge>,
}

impl Ast {
    pub fn vertex(&self, id: &str) -> Option<&Vertex> {
        self.vertices.iter().find(|vertex| vertex.id == id)
    }

    pub fn edge(&self, id: &str) -> Option<&Edge> {
        self.edges
            .iter()
            .find(|edge| edge.id.as_deref() == Some(id))
    }

    pub fn class(&self, id: &str) -> Option<&FlowClass> {
        self.classes.iter().find(|class| class.id == id)
    }

    pub fn sub_graph(&self, id: &str) -> Option<&SubGraph> {
        self.sub_graphs.iter().find(|sub_graph| sub_graph.id == id)
    }

    pub fn tooltip(&self, id: &str) -> Option<&str> {
        self.tooltips
            .iter()
            .find(|tooltip| tooltip.id == id)
            .map(|tooltip| tooltip.text.as_str())
    }

    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|node| node.id == id)
    }
}
