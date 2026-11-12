//! `block` diagrams: a hand port of upstream mermaid 12.3.0
//! `packages/mermaid/src/diagrams/block/` — the lexer and grammar of `parser/block.jison`
//! (jison, like the flowchart), and the database semantics of `blockDB.ts` with the types of
//! `blockTypes.ts`.
//!
//! The body starts with the header line (`block` or `block-beta`); frontmatter and
//! `%%{init}%%` directives are already removed by [`crate::preprocess`]. [`Ast`] holds what
//! upstream's `BlockDB` getters answer after parsing: `getBlocks`, `getBlocksFlat`,
//! `getEdges`, `getClasses`, and `getColumns`, with the block object fields of
//! `blockTypes.ts`. [`render`] lays it out and draws it.
//!
//! # Where the port stops short of upstream
//!
//! * Configuration is not owned by this crate, so `getConfig` and `setDiagramId`/`getDiagramId`
//!   (the renderer passes the SVG id) are absent. `sanitizeText` runs at the default security
//!   level: `script` and `style` elements are dropped, a stray `<` is escaped, and
//!   `<br/>`/`</br>` serialize as `<br>`, but there is no DOMPurify. That is why the
//!   `dompurifyConfig` case of `block.spec.ts` and the `getConfig` case of `blockDB.spec.ts`
//!   are not ported.
//! * `generateId`'s random middle (`id-<random>-<n>`) is replaced by a counter (`id-1`,
//!   `id-2`, ...) in the order upstream calls it, per parse instead of per module. A test
//!   pins that order.
//! * `layout.ts`, `blockRenderer.ts`, `renderHelpers.ts`, and `styles.ts` are rendering, not
//!   database data; they live in [`render`] (`styles.ts`'s palette rules are not carried by the
//!   scene yet). `layout.spec.ts` and the stylesheet half of `blockColorIndex.spec.ts` are
//!   therefore not ported here. The `colorIndex` half is: it is database data.
//! * `encodeEntities`/`decodeEntities` (applied by `Diagram.fromText`, which also appends a
//!   newline) are not ported, as in the rest of this crate: a label keeps the entities it was
//!   written with. The trailing newline the lexer adds only reproduces what `fromText` hands
//!   the parser.
//! * A `style` statement that names a block the database does not know yet throws a
//!   `TypeError` upstream; it is a [`ParseError`] here, with the statement's span.
//! * Comments are removed by preprocessing upstream and by [`crate::preprocess`] here; a body
//!   handed to this module directly may still carry them, so it blanks `%%` lines before
//!   lexing (the block grammar has no comment rule of its own).
//! * JavaScript's `undefined` in the corner the grammar's array spreading leaves behind
//!   (`A --> B --> C`) is stored as an empty id/label and [`BlockType::Na`]; see `ast.rs`.
//! * Parse errors carry a [`Span`], where jison reports a line and an excerpt.
//!
//! No app consumes this yet: it replaces the placeholder `Ast`/`parse`, and
//! `Diagram::Block(Ast)` keeps its shape, so no app breaks.

mod ast;
mod db;
mod lexer;
mod parser;
pub mod render;

pub use ast::{Ast, Block, BlockType, ClassDef, Edge};

use crate::error::ParseError;

/// Parses the diagram body (see [`crate::preprocess`]).
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    parser::parse(body)
}

#[cfg(test)]
mod tests;
