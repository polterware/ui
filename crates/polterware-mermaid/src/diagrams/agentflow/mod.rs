//! `agentflow` diagrams: a port of upstream mermaid 12.3.0
//! `packages/mermaid/src/diagrams/agentflow/` (`parser/agentflow.jison`, `agentflowParser.ts`,
//! `agentflowDb.ts`, `types.ts`, `shapes.ts`, and `diagnostics.ts`).
//!
//! The grammar is flowchart's surface with agentflow's vocabulary, so the lexer in `lexer.rs`
//! reproduces the jison start conditions and the parser in `parser.rs` is a recursive descent
//! over the production rules. [`Ast`] holds what upstream's database getters return after
//! parsing: vertices (connectors included), edges, classes, containers, the `global` scope,
//! element mappings, diagnostics, and the node and edge projection `getData` derives. Nothing
//! here lays out or draws it: [`render::draw`] hands the projection to the shared pipeline.
//!
//! The body starts with the header line; frontmatter and `%%{init}%%` directives are already
//! removed by [`crate::preprocess`].
//!
//! [`render`] is the Rust side of `agentflow/renderer.ts` `draw`: the `getData` projection
//! (see [`Ast`]'s `nodes` and `layout_edges`) becomes the pipeline's layout data, the pipeline
//! lays it out, and the agentflow stylesheet completes the edge paths, their markers and
//! labels, and the transparent `flowGroup` containers.
//!
//! # Where the port stops short of upstream
//!
//! * Configuration is not owned by this crate, so `getConfig` and `defaultConfig` are absent.
//!   `maxEdges` is the published default (500), and `flowchart.inheritDir` is false.
//! * `sanitizeText` is approximated: `script`/`style` elements are dropped and a stray `<`
//!   is escaped, but there is no DOMPurify (a DOM library).
//! * `@{ ... }` metadata runs through a YAML subset (js-yaml `JSON_SCHEMA` flow and block
//!   mappings, sequences, quoted and plain scalars, block scalars, comments). Anchors, aliases,
//!   and tags are rejected, and a YAML error's message carries a source line and column instead
//!   of js-yaml's excerpt gutter.
//! * Click callbacks (`setClickFun`, the DOM `funs`) and tooltips are not stored: a click only
//!   marks the node clickable, and the tooltip text is discarded, as it has no database
//!   consumer here.
//! * Colour-slot assignment (`colorSlots.ts`) is presentation and absent, so layout nodes carry
//!   no colour index.
//! * The `UNICODE_TEXT` rule accepts any non-ASCII character instead of upstream's exact
//!   code-point ranges.
//! * `getData` is computed once at the end of [`parse`]; upstream's idempotent repeated calls
//!   are not modelled.

mod ast;
mod db;
mod layout_data;
mod lexer;
mod metadata;
mod parser;
pub mod render;

pub use ast::{
    ArrowEnd, ArrowType, Ast, Diagnostic, Edge, EdgeSemantic, ElementMapping, ElementPosition,
    FlowClass, LabelType, LayoutEdge, MappingStats, MappingType, Node, SemanticConnector,
    SemanticEdge, SemanticModel, SemanticSubGraph, SemanticVertex, Severity, Stroke, SubGraph,
    Vertex, VertexKind,
};
pub use render::AgentflowRenderError;

use crate::error::ParseError;

/// Parses the diagram body (see [`crate::preprocess`]).
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    parser::parse(body)
}

/// Parses the diagram body, shifting element-mapping positions by a frontmatter line offset
/// (upstream's `setFrontmatterLineOffset`).
pub fn parse_with_frontmatter_offset(body: &str, offset: u32) -> Result<Ast, ParseError> {
    parser::parse_with_offset(body, offset)
}

#[cfg(test)]
mod tests;
