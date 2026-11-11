//! `swimlanes` diagrams: a port of upstream mermaid 12.3.0
//! `packages/mermaid/src/diagrams/swimlanes/` (`detector.ts`, `swimlanesDiagram.ts`, and
//! `styles.ts`).
//!
//! Upstream's swimlanes diagram owns neither a grammar nor a database. `swimlanesDiagram.ts`
//! builds it with flowchart's exported `createFlowDiagram`, so a `swimlane-beta` body is
//! parsed by `packages/mermaid/src/diagrams/flowchart/parser/flow.jison` and stored in a
//! `FlowDB`; the swimlanes plugin only adds lane styles (`styles.ts`, rendering) and the
//! `swimlane.layout` default (configuration). The one grammar touch is the keyword itself:
//! `flow.jison` accepts `"swimlane-beta"` as a `GRAPH` token next to `graph`, `flowchart`,
//! and `flowchart-elk`.
//!
//! This module follows that split. [`Ast`] is the flowchart database after parsing
//! (including the nodes and edges `getData` derives), and [`parse`] runs the flowchart
//! parser, which already accepts the `swimlane-beta` header; the grammar, lexer, and
//! database semantics live in [`crate::diagrams::flowchart`]. Detection is likewise not
//! owned here: [`crate::diagrams::detect`] ports `detector.ts`.
//!
//! Nothing here lays out or draws: marking a lane with `shape: "swimlane"`, dropping nodes
//! from a later lane, ordering lanes, and warning about nested groups belong to
//! `rendering-util/layout-algorithms/swimlanes/helpers.ts`, which is a layout concern.
//!
//! # Where the port stops short of upstream
//!
//! * Configuration is not owned by this crate, so the `swimlane` schema default
//!   (`layout: swimlane`) and its overrides are absent. That is why the layout assertions of
//!   `swimlanesDiagram.spec.ts` are not ported.
//! * `styles.ts` (the lane palette CSS) is rendering and is not ported.

pub use crate::diagrams::flowchart::Ast;

use crate::error::ParseError;

/// Parses the diagram body (see [`crate::preprocess`]). The body starts with the header
/// line; frontmatter and `%%{init}%%` directives are already removed.
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    crate::diagrams::flowchart::parse(body)
}

#[cfg(test)]
mod tests;
