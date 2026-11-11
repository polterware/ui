//! `mindmap` diagrams: a port of upstream mermaid 12.3.0
//! `packages/mermaid/src/diagrams/mindmap/`.
//!
//! The grammar (`parser/mindmap.jison`) is ported by hand as a recursive-descent parser in
//! `parser.rs` over the lexer start conditions in `lexer.rs`, and the database semantics of
//! `mindmapDb.ts` live in `db.rs` and `ast.rs`, so [`Ast`] holds what upstream's
//! `MindmapDB` exposes through its getters after parsing (`getMindmap`, `getType`,
//! `type2Str`) and through `getData` ([`Ast::data`]).
//!
//! The body the parser receives starts with the header line (`mindmap`); frontmatter and
//! `%%{init}%%` directives are already removed by [`crate::preprocess`], while `%%` comment
//! lines are kept, and the lexer skips them as upstream's grammar does.
//!
//! # Where the port stops short of upstream
//!
//! * `sanitizeText` (DOMPurify) is not applied to ids, descriptions, icons, or classes:
//!   sanitizing belongs to the painter, which knows its security level.
//! * Configuration is not read (`getConfig().mindmap.padding`, `maxNodeWidth`, `colorBy`,
//!   `theme`, `look`): nodes carry upstream's default width and padding, and
//!   [`Ast::data_with`] takes the color mode as its argument instead.
//! * `getData`'s `config`, `look`, and `rootNode` are not in [`Data`]: `config` belongs to
//!   the caller, and `rootNode` is [`Ast::root`].
//! * Layout output is not modelled: `x`/`y` are written after `getData` by the engine, and
//!   so is `height`, which stays `None` here until a caller sets it.
//! * A decoration with no node before it throws a `TypeError` upstream (a lookup on an
//!   empty array); here it is a [`ParseError`] carrying the decoration's span.
//! * A parse or lexical error carries a [`Span`](crate::error::Span) (line and column);
//!   upstream reports a jison line and a token name instead.
//!
//! The upstream spec files ported as tests are `mindmap.spec.ts`,
//! `mindmapDb.getData.test.ts`, and `mindmapColors.spec.ts`; what each port skips is listed
//! in `tests.rs`.
//!
//! No app consumes this yet: it replaces the placeholder `Ast`/`parse`, and
//! `Diagram::Mindmap(Ast)` keeps its shape, so no app breaks.

mod ast;
mod db;
mod lexer;
mod parser;
#[cfg(test)]
mod tests;

pub use ast::{
    Ast, ColorBy, DEFAULT_MAX_NODE_WIDTH, DEFAULT_PADDING, Data, LayoutEdge, LayoutNode,
    MAX_SECTIONS, Node, NodeType,
};

use crate::error::ParseError;

/// Parses the diagram body (see [`crate::preprocess`]).
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    parser::parse(body)
}
