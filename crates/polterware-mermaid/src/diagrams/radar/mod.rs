//! `radar` diagrams: a port of upstream mermaid 12.3.0
//! `packages/parser/src/language/radar/` (`radar.langium`, `tokenBuilder.ts`) with the common
//! rules it imports (`packages/parser/src/language/common/common.langium` and
//! `valueConverter.ts`), and of the database semantics in
//! `packages/mermaid/src/diagrams/radar/` (`db.ts`, `parser.ts`, `types.ts`).
//!
//! The grammar is a Langium grammar, so the lexer in `lexer.rs` reproduces the token order
//! Langium's `DefaultTokenBuilder` produces and the parser in `parser.rs` is the recursive
//! descent of the rules. [`Ast`] holds what upstream's radar database getters return after
//! parsing: `getAxes`, `getCurves`, `getOptions`, `getDiagramTitle`, `getAccTitle`, and
//! `getAccDescription`. The body starts with the header line; frontmatter and `%%{init}%%`
//! directives are already removed by [`crate::preprocess`].
//!
//! # Where the port stops short of upstream
//!
//! * Configuration is not owned by this crate, so `getConfig` and `DEFAULT_RADAR_CONFIG` are
//!   absent. That is why the `config` and theme sections of `radar.spec.ts` are not ported as
//!   configuration assertions; the three documents that carry a directive still parse, with
//!   the directive removed by preprocessing as it is in upstream's `preprocessDiagram`.
//! * `sanitizeText` is approximated: `script`/`style` elements are dropped and a stray `<` is
//!   escaped, but there is no DOMPurify (a DOM library). This affects only the title and the
//!   accessibility strings; axis and curve labels are not sanitized upstream either.
//! * The `MAX_TICKS` clamp is silent: upstream logs a warning before clamping `ticks` to 32,
//!   and this crate has no logger.
//! * Langium reports lexer and parser errors without a position in the message and recovers by
//!   skipping characters; this crate's [`ParseError`] carries a
//!   [`Span`](crate::error::Span) instead and stops at the first error. The two errors
//!   `db.ts` throws as plain `Error`s (`Axes must be populated before curves for reference
//!   entries` and `Missing entry for axis ...`) have no location upstream; this port attaches
//!   the curve name's span.
//! * Nothing here lays out or draws the graticule, axes, curves, legend, or title;
//!   `renderer.ts` and `styles.ts` are out of scope.

mod ast;
mod db;
mod lexer;
mod parser;
mod sanitize;

pub use ast::{Ast, Axis, Curve, Graticule, Options};

use crate::error::ParseError;

/// Parses the diagram body (see [`crate::preprocess`]).
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    db::build(parser::parse_document(body)?)
}

#[cfg(test)]
mod tests;
