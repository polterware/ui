//! The agentflow database after parsing: what upstream's `AgentFlowDB` getters return
//! (`agentflowDb.ts` `getVertices`, `getEdges`, `getSubGraphs`, `getConnectors`,
//! `getElementMappings`, `getDiagnostics`, `getMappingStats`, and `getSemanticModel`), plus the
//! node and edge projection `getData` derives for the renderer.
//!
//! Upstream's database holds DOM wiring, configuration, and colour-slot assignment; those are
//! presentation and stay out. Everything the parser decides lives here as plain data.

use serde_json::{Map, Value};

use super::metadata;

/// Metadata attached with `@{ ... }` to a node, an edge, a subgraph, or a connector.
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

/// The canonical meaning of an edge operator (`types.ts` `EdgeSemantic`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeSemantic {
    /// `-->`: execution order.
    Sequence,
    /// `-.-`: a reference document attached to the source.
    Reference,
    /// `--x`: failure, cancellation, or escalation.
    Failure,
}

impl EdgeSemantic {
    pub fn as_str(self) -> &'static str {
        match self {
            EdgeSemantic::Sequence => "sequence",
            EdgeSemantic::Reference => "reference",
            EdgeSemantic::Failure => "failure",
        }
    }
}

/// The semantic kind derived from a vertex's shape (`types.ts` `VertexKind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VertexKind {
    Tool,
    Action,
    Input,
    Refdoc,
    Decision,
    Connector,
    Task,
}

impl VertexKind {
    pub fn as_str(self) -> &'static str {
        match self {
            VertexKind::Tool => "tool",
            VertexKind::Action => "action",
            VertexKind::Input => "input",
            VertexKind::Refdoc => "refdoc",
            VertexKind::Decision => "decision",
            VertexKind::Connector => "connector",
            VertexKind::Task => "task",
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
    /// The grammar shape (`square`, `diamond`, ...) or the resolved metadata `shape`.
    pub type_name: Option<String>,
    pub styles: Vec<String>,
    pub classes: Vec<String>,
    pub dir: Option<String>,
    pub props: Metadata,
    pub link: Option<String>,
    pub link_target: Option<String>,
    pub have_callback: bool,
    pub metadata: Option<Metadata>,
    /// Set by `addConnector` for a node declared with the `connector` keyword.
    pub is_connector: bool,
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
    pub edge_semantic: Option<EdgeSemantic>,
    pub style: Option<Vec<String>>,
    pub length: Option<i64>,
    pub text: String,
    pub label_type: LabelType,
    pub classes: Vec<String>,
    pub id: Option<String>,
    pub animation: Option<String>,
    pub animate: Option<bool>,
    pub metadata: Option<Metadata>,
}

/// A class definition (`FlowClass`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FlowClass {
    pub id: String,
    pub styles: Vec<String>,
    pub text_styles: Vec<String>,
}

/// A subgraph (`FlowSubGraph`). `type_name` is `flow` in v0.8.1.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SubGraph {
    pub id: String,
    pub title: String,
    pub nodes: Vec<String>,
    pub classes: Vec<String>,
    pub dir: Option<String>,
    pub label_type: LabelType,
    pub type_name: Option<String>,
    pub metadata: Option<Metadata>,
}

/// A position in the diagram source (`types.ts` `ElementPosition`). Lines are 1-based and
/// columns are 0-based, as jison reports them; `start`/`end` are byte offsets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ElementPosition {
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub start_index: usize,
    pub end_index: usize,
}

/// The kinds of top-level statement the parser maps (`types.ts` `AgentflowStatementType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MappingType {
    Vertex,
    Edge,
    SubGraph,
    Connector,
    /// A standalone `id@{ ... }` block annotating an element declared elsewhere.
    Attachment,
}

impl MappingType {
    pub fn as_str(self) -> &'static str {
        match self {
            MappingType::Vertex => "vertex",
            MappingType::Edge => "edge",
            MappingType::SubGraph => "subgraph",
            MappingType::Connector => "connector",
            MappingType::Attachment => "attachment",
        }
    }
}

/// One element-to-position mapping (`types.ts` `AgentflowElementMapping`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElementMapping {
    pub id: String,
    pub type_name: MappingType,
    pub position: ElementPosition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Warning,
    Error,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

/// Every diagnostic id in the shared vocabulary (`diagnostics.ts` `AgentflowWarning`).
///
/// Only `SHAPE_UNSUPPORTED`, `SHAPE_REMOVED`, and `CONTAINMENT_VIOLATION` are emitted here,
/// matching `PARSER_EMITTED_DIAGNOSTICS`; the rest are reserved for the semantics module.
pub const SHAPE_UNSUPPORTED: &str = "SHAPE_UNSUPPORTED";
pub const SHAPE_REMOVED: &str = "SHAPE_REMOVED";
pub const CONTAINMENT_VIOLATION: &str = "CONTAINMENT_VIOLATION";

/// A structured warning or error (`types.ts` `AgentflowDiagnostic`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub id: String,
    pub severity: Severity,
    pub message: String,
    pub node_id: Option<String>,
    pub edge_id: Option<String>,
    pub position: Option<ElementPosition>,
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

/// A node `getData` hands to the renderer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub label_type: LabelType,
    pub parent_id: Option<String>,
    pub padding: f64,
    pub css_styles: Vec<String>,
    pub css_compiled_styles: Vec<String>,
    pub css_classes: String,
    pub shape: String,
    pub dir: Option<String>,
    pub is_group: bool,
    pub dom_id: Option<String>,
    pub link: Option<String>,
    pub link_target: Option<String>,
    pub metadata: Option<Metadata>,
}

/// An edge `getData` hands to the renderer.
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
    pub animate: Option<bool>,
    pub animation: Option<String>,
    pub curve: Option<String>,
    pub metadata: Option<Metadata>,
}

/// A vertex as `getSemanticModel` exposes it.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticVertex {
    pub id: String,
    pub label: Option<String>,
    pub shape: Option<String>,
    pub vertex_kind: VertexKind,
    pub metadata: Option<Metadata>,
}

/// An edge as `getSemanticModel` exposes it.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticEdge {
    pub start: String,
    pub end: String,
    pub id: Option<String>,
    pub label: Option<String>,
    pub type_name: Option<String>,
    pub stroke: Option<Stroke>,
    pub edge_semantic: Option<EdgeSemantic>,
    pub length: Option<i64>,
    pub metadata: Option<Metadata>,
}

/// A container as `getSemanticModel` exposes it.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticSubGraph {
    pub id: String,
    pub type_name: Option<String>,
    pub title: Option<String>,
    pub nodes: Vec<String>,
    pub metadata: Option<Metadata>,
    pub direction: Option<String>,
}

/// A connector as `getSemanticModel` exposes it.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticConnector {
    pub id: String,
    pub title: Option<String>,
    pub metadata: Option<Metadata>,
}

/// The projection `getSemanticModel` returns (`types.ts` `AgentflowSemanticModel`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SemanticModel {
    pub direction: Option<String>,
    pub vertices: Vec<SemanticVertex>,
    pub edges: Vec<SemanticEdge>,
    pub sub_graphs: Vec<SemanticSubGraph>,
    pub connectors: Vec<SemanticConnector>,
    pub diagnostics: Vec<Diagnostic>,
}

/// The parsed diagram: the database's state after parsing, plus the `getData` projection.
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
    /// Ids declared with the `connector` keyword, in declaration order.
    pub connectors: Vec<String>,
    /// Ids scoped by a `global ... end` block.
    pub global_nodes: Vec<String>,
    pub element_mappings: Vec<ElementMapping>,
    pub diagnostics: Vec<Diagnostic>,
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

    pub fn connector(&self, id: &str) -> Option<&Vertex> {
        self.vertices
            .iter()
            .find(|vertex| vertex.id == id && vertex.is_connector)
    }

    /// `getConnectors()`.
    pub fn connectors(&self) -> Vec<&Vertex> {
        self.connectors
            .iter()
            .filter_map(|id| self.vertex(id))
            .collect()
    }

    /// The first mapping with this id (`getElementById`).
    pub fn element_by_id(&self, id: &str) -> Option<&ElementMapping> {
        self.element_mappings
            .iter()
            .find(|mapping| mapping.id == id)
    }

    /// `getElementsOnLine`.
    pub fn elements_on_line(&self, line: u32) -> Vec<&ElementMapping> {
        self.element_mappings
            .iter()
            .filter(|mapping| {
                line >= mapping.position.start_line && line <= mapping.position.end_line
            })
            .collect()
    }

    /// `getElementAtPosition`: the innermost (smallest span) mapping covering the point.
    pub fn element_at_position(&self, line: u32, column: u32) -> Option<&ElementMapping> {
        let mut candidates = self.element_mappings.iter().filter(|mapping| {
            let position = mapping.position;
            if line < position.start_line || line > position.end_line {
                return false;
            }
            if line == position.start_line && column < position.start_column {
                return false;
            }
            if line == position.end_line && column > position.end_column {
                return false;
            }
            true
        });
        let mut smallest = candidates.next()?;
        for candidate in candidates {
            let span = |mapping: &ElementMapping| {
                (
                    i64::from(mapping.position.end_line) - i64::from(mapping.position.start_line),
                    i64::from(mapping.position.end_column)
                        - i64::from(mapping.position.start_column),
                )
            };
            let (small_lines, small_columns) = span(smallest);
            let (cur_lines, cur_columns) = span(candidate);
            let tighter = cur_lines < small_lines
                || (cur_lines == small_lines && cur_columns < small_columns);
            if tighter {
                smallest = candidate;
            }
        }
        Some(smallest)
    }

    /// `getMappingStats`.
    pub fn mapping_stats(&self) -> MappingStats {
        let mut stats = MappingStats::default();
        for mapping in &self.element_mappings {
            match mapping.type_name {
                MappingType::Vertex => stats.vertices += 1,
                MappingType::Edge => stats.edges += 1,
                MappingType::SubGraph => stats.subgraphs += 1,
                MappingType::Connector => stats.connectors += 1,
                MappingType::Attachment => stats.attachments += 1,
            }
        }
        stats.total_elements = self.element_mappings.len();
        stats
    }

    /// `isToolDefinition`: the resolved shape is `subroutine` or one of its aliases.
    pub fn is_tool_definition(&self, vertex: &Vertex) -> bool {
        metadata::SUBROUTINE_ALIASES.contains(&vertex.type_name.as_deref().unwrap_or(""))
    }

    /// `getTools`, in vertex insertion order.
    pub fn tools(&self) -> Vec<&Vertex> {
        self.vertices
            .iter()
            .filter(|vertex| self.is_tool_definition(vertex))
            .collect()
    }

    /// `deriveVertexKind`.
    pub fn vertex_kind(&self, vertex: &Vertex) -> VertexKind {
        if vertex.is_connector {
            return VertexKind::Connector;
        }
        if self.is_tool_definition(vertex) {
            return VertexKind::Tool;
        }
        let resolved = metadata::resolve_shape_alias(vertex.type_name.as_deref());
        match resolved {
            Some("hexagon") | Some("hex") => VertexKind::Action,
            Some("lean-right") | Some("lean_right") => VertexKind::Input,
            Some("lin-doc") | Some("lined-document") => VertexKind::Refdoc,
            Some("diamond") => VertexKind::Decision,
            _ => VertexKind::Task,
        }
    }

    /// `getSemanticModel`: the presentation-stripped view of the database.
    pub fn semantic_model(&self) -> SemanticModel {
        let sub_graph_ids: Vec<&str> = self.sub_graphs.iter().map(|sg| sg.id.as_str()).collect();
        let mut vertices = Vec::new();
        let mut connectors = Vec::new();
        for vertex in &self.vertices {
            if sub_graph_ids.contains(&vertex.id.as_str()) {
                continue;
            }
            if vertex.is_connector {
                let mut connector = SemanticConnector {
                    id: vertex.id.clone(),
                    title: None,
                    metadata: None,
                };
                if let Some(text) = &vertex.text
                    && text != &vertex.id
                {
                    connector.title = Some(text.clone());
                }
                connector.metadata = semantic_metadata(vertex.metadata.as_ref());
                connectors.push(connector);
                continue;
            }
            let resolved = metadata::resolve_shape_alias(vertex.type_name.as_deref());
            vertices.push(SemanticVertex {
                id: vertex.id.clone(),
                label: vertex.text.clone(),
                shape: resolved.map(str::to_string),
                vertex_kind: self.vertex_kind(vertex),
                metadata: semantic_metadata(vertex.metadata.as_ref()),
            });
        }
        let edges = self
            .edges
            .iter()
            .map(|edge| SemanticEdge {
                start: edge.start.clone(),
                end: edge.end.clone(),
                id: edge.id.clone(),
                label: Some(edge.text.clone()).filter(|text| !text.is_empty()),
                type_name: edge.type_name.map(|kind| kind.as_str().to_string()),
                stroke: edge.stroke,
                edge_semantic: edge.edge_semantic,
                length: edge.length,
                metadata: semantic_metadata(edge.metadata.as_ref()),
            })
            .collect();
        let sub_graphs = self
            .sub_graphs
            .iter()
            .map(|sub_graph| SemanticSubGraph {
                id: sub_graph.id.clone(),
                type_name: sub_graph.type_name.clone(),
                title: Some(sub_graph.title.clone()),
                nodes: sub_graph.nodes.clone(),
                metadata: semantic_metadata(sub_graph.metadata.as_ref()),
                direction: sub_graph.dir.clone(),
            })
            .collect();
        SemanticModel {
            direction: self.direction.clone(),
            vertices,
            edges,
            sub_graphs,
            connectors,
            diagnostics: self.diagnostics.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MappingStats {
    pub vertices: usize,
    pub edges: usize,
    pub subgraphs: usize,
    pub connectors: usize,
    pub attachments: usize,
    pub total_elements: usize,
}

/// Metadata keys that are presentation-only per §11 and stripped from the semantic model.
const SEMANTIC_METADATA_SKIP_KEYS: [&str; 10] = [
    "shape", "view", "icon", "img", "form", "pos", "w", "h", "class", "style",
];

fn semantic_metadata(metadata: Option<&Metadata>) -> Option<Metadata> {
    let metadata = metadata?;
    let mut kept = Metadata::new();
    for (key, value) in metadata {
        if !SEMANTIC_METADATA_SKIP_KEYS.contains(&key.as_str()) && key != "labelType" {
            kept.insert(key.clone(), value.clone());
        }
    }
    if kept.is_empty() { None } else { Some(kept) }
}
