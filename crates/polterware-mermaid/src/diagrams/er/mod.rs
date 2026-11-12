//! `er` diagrams: a port of upstream mermaid 12.3.0 `packages/mermaid/src/diagrams/er/`.
//!
//! The grammar (`parser/erDiagram.jison`) is ported by hand as a recursive-descent parser
//! in `parser.rs` over the lexer start conditions in `lexer.rs`, and the database semantics
//! of `erDb.ts` live in `db.rs`, so [`Ast`] holds what upstream's `ErDB` exposes through
//! its getters after parsing (`getEntities`, `getEntity`, `getRelationships`, `getClasses`,
//! `getSubGraphs`, `getDirection`, the accessibility getters, and the `getData` projection).
//!
//! The body the parser receives starts with the header line (`erDiagram`); frontmatter,
//! `%%{init}%%` directives, and `%%` comments are removed before it by
//! [`crate::preprocess`], so this module never sees them.
//!
//! # Where the port stops short of upstream
//!
//! * `sanitizeText` (DOMPurify) is not applied to accessibility text or subgraph titles:
//!   sanitizing belongs to the painter, which knows its security level. The newline
//!   cleanup `setAccDescription` performs is kept.
//! * Configuration is not read by the parser (`getConfig().look`, `getConfig().er`), so
//!   entities carry no `look` and `getData` has no `config` field; `layout_data.rs` fills
//!   those in for the renderer.
//! * Renderer-only state is left out: `cssCompiledStyles` and `colorIndex` are derived by
//!   [`Ast::data`] rather than stored on each entity, and `other: {}` is not modelled.
//! * `addSubGraph`'s `subCount`-based fallback id is kept but unreachable: the grammar
//!   always passes an id.
//! * The grammar's `title title_value` production can never fire: the lexer has no `title`
//!   token, so `getDiagramTitle` stays empty, as it does upstream for this diagram.
//! * A parse or lexical error carries a [`Span`](crate::error::Span) (line and column);
//!   upstream reports a jison line and a token name instead.
//!
//! The renderer is [`render`], ported from `erRenderer-unified.ts` over the shared layout
//! pipeline; `layout_data.rs` is the `getData` projection the renderer consumes. No app
//! consumes it yet, so the public `Diagram::Er(Ast)` shape is unchanged.

mod ast;
mod db;
mod layout_data;
mod lexer;
mod parser;
pub mod render;
#[cfg(test)]
mod tests;

pub use ast::{
    Ast, Attribute, Cardinality, Data, Entity, EntityClass, Identification, LabelType, LayoutEdge,
    Node, RelSpec, Relationship, SubGraph,
};

use crate::error::ParseError;

/// Parses the diagram body (see [`crate::preprocess`]).
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    parser::parse(body)
}
