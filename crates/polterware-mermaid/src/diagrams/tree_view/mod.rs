//! `tree_view` diagrams: a port of upstream mermaid 12.3.0
//! `packages/mermaid/src/diagrams/treeView/` (`parser.ts`, `db.ts`, `types.ts`,
//! `boxDrawingPreprocessor.ts`) and of the Langium grammar it feeds,
//! `packages/parser/src/language/treeView/` (`treeView.langium`, `valueConverter.ts`,
//! `tokenBuilder.ts`).
//!
//! The grammar is a Langium grammar, so the lexer in `lexer.rs` reproduces the token order
//! Langium's `DefaultTokenBuilder` produces and the parser in `parser.rs` is the recursive
//! descent of the rules. [`Ast`] holds what upstream's `TreeViewDB` getters return after
//! parsing: `getRoot`, `getCount`, `getDiagramTitle`, `getAccTitle`, and `getAccDescription`.
//! The body starts with the header line; frontmatter and `%%{init}%%` directives are already
//! removed by [`crate::preprocess`].
//!
//! # Where the port stops short of upstream
//!
//! * Nothing here lays out or draws, so `renderer.ts` and `styles.ts` are out of scope, and
//!   `Node.BBox` (which the renderer assigns) is not part of the database.
//! * `icons.ts` resolves a node's icon at render time from `showIcons` and the
//!   `filenameIcons`/`extensionIcons` config maps; this crate owns no configuration, so the
//!   database keeps only the `icon` annotation, as upstream's database does.
//! * `getConfig` and the `treeView` defaults are absent for the same reason.
//! * `sanitizeText` is approximated: `script`/`style` elements are dropped and a stray `<` is
//!   escaped, but there is no DOMPurify (a DOM library). This affects node descriptions, the
//!   title, and the two accessibility strings.
//! * Upstream remaps the `line N` text of a grammar error after the box-drawing conversion
//!   (`remapErrorLines`); this crate's [`ParseError`] carries a [`Span`](crate::error::Span)
//!   instead, so the span is moved to the original line and column. The column is
//!   approximate when the conversion rewrote the line's indentation.
//! * The lexer keeps `ML_COMMENT` and `NL` as hidden tokens and drops the unreachable `WS`
//!   (`INDENTATION` matches every space run first, exactly as the grammar's declaration order
//!   has it), so `%%` comments inside the body are skipped by the lexer.

mod ast;
mod box_drawing;
mod db;
mod lexer;
mod parser;

pub use ast::{Ast, Node, NodeType};

use crate::error::ParseError;

/// Parses the diagram body (see [`crate::preprocess`]).
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    parser::parse(body)
}

#[cfg(test)]
mod tests;
