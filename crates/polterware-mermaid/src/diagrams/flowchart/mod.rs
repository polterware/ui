//! `flowchart` diagrams. Port of upstream `packages/mermaid/src/diagrams/flowchart/` at mermaid
//! 12.3.0: the lexer and grammar of `parser/flow.jison`, and the database semantics of
//! `flowDb.ts` with its `getData` layout projection.
//!
//! The body the parser receives starts with the header line (`graph`/`flowchart` and its
//! direction); frontmatter and `%%{init}%%` directives are removed by [`crate::preprocess`].
//! Nothing here lays out or draws: the [`Ast`] holds exactly what upstream's `FlowDB` getters
//! return, including the nodes and edges `getData` derives for a layout engine.
//!
//! # Where the port stops short of upstream
//!
//! * `sanitizeText` is approximated: `script`/`style` elements are dropped and a stray `<` is
//!   escaped, but there is no DOMPurify (a DOM library).
//! * `@{ ... }` metadata runs through a YAML subset (js-yaml `JSON_SCHEMA` flow and block
//!   mappings, quoted/plain scalars, block scalars); exotic YAML is rejected.
//! * The `UNICODE_TEXT` rule accepts any non-ASCII character instead of upstream's exact
//!   code-point ranges.
//! * Click callbacks (`setClickFun`, the DOM `funs`) are not registered: the node is marked
//!   clickable, and the tooltip/link/target are kept as data.
//! * A `subgraph` with no id or title is created anonymously; upstream's database would throw a
//!   `TypeError` on that grammar branch.
//! * Comments are stripped by preprocessing upstream (`diagram-api/comments.ts`); the grammar
//!   never sees them, so this module does not strip `%%` lines either.
//! * An out-of-range `linkStyle` index raises the documented bounds error; upstream's string
//!   indices slip past its `typeof` check and fail on the array access instead.

mod ast;
mod db;
mod lexer;
mod metadata;
mod parser;

pub use ast::{
    ArrowEnd, ArrowType, Ast, DroppedSubGraphMember, Edge, FlowClass, LabelType, LayoutEdge, Node,
    Stroke, SubGraph, Tooltip, Vertex,
};

use crate::error::ParseError;

/// Parses the diagram body (see [`crate::preprocess`]).
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    parser::parse(body)
}

#[cfg(test)]
mod tests;
