//! `kanban` diagrams. Port of upstream `packages/mermaid/src/diagrams/kanban/` at mermaid
//! 12.3.0: the lexer and grammar of `parser/kanban.jison`, and the database semantics of
//! `kanbanDb.ts` with its `getData` projection.
//!
//! The body the parser receives starts with the header line, with its line endings normalized;
//! frontmatter and `%%{init}%%` directives are removed by [`crate::preprocess`]. Nothing here
//! lays out or draws: the [`Ast`] holds what upstream's kanban database holds after parsing
//! (`getSections` and the node list), plus the nodes `getData` derives for a renderer.
//!
//! The grammar is jison, so `lexer.rs` reproduces the `%lex` rules in their order and states and
//! `parser.rs` is the recursive descent of the `%%` grammar; the tests in `tests.rs` port
//! `kanban.spec.ts`.
//!
//! # Where the port stops short of upstream
//!
//! * Configuration is not owned by this crate, so `getConfig` and the fields `addNode` and
//!   `getData` copy from it are absent: `mindmap.padding`, `mindmap.maxNodeWidth`, the section
//!   `width`/`padding`, and `look`. No `kanban.spec.ts` case reads them, so no case is skipped
//!   for this reason.
//! * `sanitizeText` is approximated: `script`/`style` elements are dropped and a stray `<` is
//!   escaped, but there is no DOMPurify (a DOM library).
//! * `@{ ... }` metadata runs through a YAML subset (js-yaml `JSON_SCHEMA` flow and block
//!   mappings, sequences, quoted/plain scalars, block scalars); exotic YAML is rejected.
//! * `decorateNode` dereferences the last node unconditionally, so a `::icon(...)` or `:::class`
//!   before the first item is a `TypeError` upstream; here it is a [`ParseError`] with a span.
//! * A metadata `label` is stringified before it is stored; upstream keeps the YAML value and
//!   would only fail later when the renderer sanitizes it. A metadata `shape` that is not a
//!   string is stringified too, where upstream's `.toLowerCase()` throws a `TypeError`.
//! * The `doc.shape === 'kanbanItem'` assignment in `addNode` is unreachable upstream too: that
//!   spelling fails the lowercase check just before it, so it stays unreachable here.
//! * `setElementForId`/`getElementById` (the DOM elements the renderer registers) are not
//!   ported, and nothing renders: `kanbanRenderer.ts` is out of scope.

mod ast;
mod db;
mod lexer;
mod metadata;
mod parser;

pub use ast::{Ast, DataNode, DataShape, Node, NodeType};

use crate::error::ParseError;

/// Parses the diagram body (see [`crate::preprocess`]).
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    let parsed = parser::parse(body)?;
    db::build(parsed)
}

#[cfg(test)]
mod tests;
