//! `cynefin` diagrams: a port of upstream mermaid 12.3.0
//! `packages/parser/src/language/cynefin/` (`cynefin.langium`, `tokenBuilder.ts`) with the
//! common rules it imports (`packages/parser/src/language/common/common.langium`,
//! `tokenBuilder.ts`, `valueConverter.ts`), and of the database semantics in
//! `packages/mermaid/src/diagrams/cynefin/` (`cynefinDb.ts`, `cynefinParser.ts`, `types.ts`)
//! plus `packages/mermaid/src/diagrams/common/populateCommonDb.ts` and `commonDb.ts`.
//!
//! The grammar is a Langium grammar, so the lexer in `lexer.rs` reproduces the token order
//! Langium's `DefaultTokenBuilder` produces and the parser in `parser.rs` is the recursive
//! descent of the rules, including the lookahead that tells a `DomainBlock` from a
//! `Transition`. [`Ast`] holds what upstream's `CynefinDB` getters return after parsing:
//! `getDomains`, `getTransitions`, `getDiagramTitle`, `getAccTitle`, and `getAccDescription`.
//! The body starts with the header line; frontmatter and `%%{init}%%` directives are already
//! removed by [`crate::preprocess`].
//!
//! [`render::render`] draws the map: it ports `cynefinRenderer.ts` `draw`, the geometry helpers
//! of `cynefinBoundaries.ts`, and the declarations of `styles.ts` into a
//! [`Scene`](crate::scene::Scene). The SVG element id upstream hashes for a default boundary
//! seed is [`render::DIAGRAM_ID`] here.
//!
//! # Where the port stops short of upstream
//!
//! * There is no module-level database singleton: upstream's `db` is cleared and populated by
//!   the renderer, while [`parse`] returns a fresh [`Ast`] per document.
//! * Configuration is not owned by this crate, so `getConfig` and `DEFAULT_CONFIG.cynefin` are
//!   absent. That is why the `getConfig` case of `cynefin.spec.ts` is not ported.
//! * A self-loop transition is skipped as `setTransitions` does, but the `log.warn` upstream
//!   emits for it has no logger here.
//! * `sanitizeText` is approximated (`common::sanitize`): `script`/`style` elements are dropped and
//!   a stray `<` is escaped, but there is no DOMPurify (a DOM library). This affects only the
//!   title and the accessibility strings; domain item labels are not sanitized upstream
//!   either.
//! * Langium reports lexer and parser errors in its own message shape; this crate's
//!   [`ParseError`](crate::error::ParseError) carries a [`Span`](crate::error::Span) instead.
//!   Upstream tokenizes the whole input before parsing, so it can report a lexer error far
//!   from a parser error; this lexer is driven by the parser. Both reject the same documents.

mod ast;
mod db;
mod lexer;
mod parser;
pub mod render;

pub use ast::{Ast, Domain, DomainName, Item, Transition};

use crate::error::ParseError;

/// Parses the diagram body (see [`crate::preprocess`]).
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    parser::parse(body).map(db::build)
}

#[cfg(test)]
mod tests;
