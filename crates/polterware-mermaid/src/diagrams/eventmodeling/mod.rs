//! `eventmodeling` diagrams: a port of upstream mermaid 12.3.0
//! `packages/parser/src/language/eventmodeling/` (`event-modeling.langium`,
//! `tokenBuilder.ts`, and the generated AST) and of the database semantics in
//! `packages/mermaid/src/diagrams/eventmodeling/` (`db.ts`, `parser.ts`, `types.ts`).
//!
//! The grammar is a Langium grammar, so the lexer in `lexer.rs` reproduces the token order
//! Langium's `DefaultTokenBuilder` produces and the parser in `parser.rs` is the recursive
//! descent of the rules. [`Ast`] holds what upstream's `setAst` stores in the event modeling
//! database: the `EventModel` AST (frames, data entities, model entities, notes, and
//! given/when/then blocks) plus the common title and accessibility fields `populateCommonDb`
//! copies in. The body starts with the header line; frontmatter and `%%{init}%%` directives are
//! already removed by [`crate::preprocess`].
//!
//! # Where the port stops short of upstream
//!
//! * Configuration setters are not owned by this crate, so `getConfig`,
//!   `DEFAULT_EVENTMODELING_CONFIG`, `setOptions`, `getOptions`, and the constant
//!   `getDirection` are absent. The theme's `em*` fills and strokes and the
//!   `EventModelingDiagramConfig` padding are read from [`crate::config::Config`] by the
//!   renderer.
//! * `EventModelingValidator` (`event-modeling-validator.ts`) is not ported: it runs in
//!   Langium's validation phase, which `mermaidAPI.parse` does not invoke, and it only adds
//!   diagnostics about which frame type may feed which. Those parser-package validation tests
//!   are skipped.
//! * `sanitizeText` is approximated: `script`/`style` elements are dropped and a stray `<` is
//!   escaped, but there is no DOMPurify (a DOM library).
//! * Langium reports lexer and parser errors as lists without this crate's position type; here
//!   the first error becomes a [`ParseError`] with a [`Span`](crate::error::Span). The
//!   duplicate-frame message keeps upstream's exact text, including `on line N`.
//! * Two upstream grammar quirks are kept: `EM_ID` precedes the accessibility terminals, so
//!   `title`, `accTitle`, and `accDescr` only match on the same line as a previous token; and
//!   the value converter keys on `TITLE`/`ACC_TITLE`/`ACC_DESCR`, not on `EM_*`, so their
//!   values reach the database raw (the common database still sanitizes them).
//! * `getState` and `renderer.ts` are ported in [`render`], which lays the boxes and swimlanes
//!   out and describes the drawing as a [`Scene`](crate::scene::Scene). The letter of
//!   `renderer.ts` is kept, including the `foreignObject` label, whose HTML becomes styled text
//!   lines in the scene.

mod ast;
mod db;
mod lexer;
mod parser;
pub mod render;

pub use ast::{
    Ast, DataEntity, Frame, FrameKind, Gwt, GwtStatement, ModelEntity, ModelEntityType, NoteEntity,
};

use crate::error::ParseError;

/// Parses the diagram body (see [`crate::preprocess`]).
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    let parsed = parser::parse(body)?;
    db::build(parsed)
}

#[cfg(test)]
mod tests;
