//! `sequence` diagrams: a port of upstream mermaid 12.3.0
//! `packages/mermaid/src/diagrams/sequence/`.
//!
//! The grammar (`parser/sequenceDiagram.jison`) is ported by hand as a recursive-descent
//! parser in `parser.rs` over the lexer start conditions in `lexer.rs`, and the database
//! semantics of `sequenceDb.ts` live in `db.rs`, so [`Ast`] holds what upstream's
//! `SequenceDB` exposes through its getters after parsing (`getActors`, `getMessages`,
//! `getBoxes`, `getCreatedActors`, `getDestroyedActors`, `getAccTitle`,
//! `getAccDescription`, `getDiagramTitle`, `showSequenceNumbers`).
//!
//! Differences from upstream, all forced by the environment:
//! - There is no configuration object here (frontmatter and `%%{init}%%` are removed by
//!   [`crate::preprocess`]), so `config.sequence.wrap` and `setWrap` are absent and a label
//!   without a `wrap:`/`nowrap:` marker gets `wrap = false`, the upstream default.
//! - `addDetails` reads a DOM element upstream; with no DOM the lookup throws and the
//!   catch swallows it, so it is a no-op here.
//! - `sanitizeText`/`sanitizeUrl` (DOMPurify) are not applied: sanitizing belongs to the
//!   painter, which knows its HTML mode and security level.
//! - `parseBoxData` checks a color against the CSS named-color list and a numeric
//!   `rgb()`/`hsl()` body instead of `CSS.supports`, and `addActor` parses participant
//!   metadata with a small YAML subset instead of `js-yaml` (flow mappings, or a block
//!   mapping when the text spans lines, with scalar and nested flow values).
//! - `links`/`properties` payloads are applied when they parse to a JSON object; upstream's
//!   `for...in` also walks arrays and strings.
//! - Parse and `apply` errors carry a [`Span`](crate::error::Span) (line and column).
//!
//! Renderer-only state is left out: upstream's `actorCnt` and `rectData` start as `null`
//! and only the renderer fills them, and its `notes` list has no getter (note messages are
//! in [`Ast::messages`] with [`LineType::Note`]). `getConfig()` returns configuration,
//! which [`crate::preprocess`] owns.
//!
//! No app consumes this yet: it replaces the placeholder `Ast`/`parse`, and
//! `Diagram::Sequence(Ast)` keeps its shape, so no app breaks.

mod ast;
mod db;
mod lexer;
mod metadata;
mod parser;
pub mod render;
#[cfg(test)]
mod tests;
#[cfg(test)]
#[path = "tests/render_tests.rs"]
mod render_tests;

pub use ast::{Actor, Ast, Box, LineType, Message, MessageText, Placement};
pub use parser::parse;
pub use render::{SequenceLayoutData, SequenceRenderError, draw, draw_ast, draw_layout_ast, render};
