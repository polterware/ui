//! `info` diagrams: a port of upstream mermaid 12.3.0 `packages/parser/src/language/info/`
//! (`info.langium`, `tokenBuilder.ts`) with the common rules it imports
//! (`packages/parser/src/language/common/common.langium` and `valueConverter.ts`), and of
//! `packages/mermaid/src/diagrams/info/` (`infoDb.ts`, `infoParser.ts`, `infoTypes.ts`).
//!
//! The grammar is a Langium grammar, so the lexer in `lexer.rs` reproduces the token order
//! Langium's `DefaultTokenBuilder` produces and the parser in `parser.rs` is the recursive
//! descent of the rules. [`Ast`] is the `Info` AST node the grammar infers, which is all
//! `infoParser.ts` produces: `title`, `accTitle`, and `accDescr`. The body starts with the
//! header line; frontmatter and `%%{init}%%` directives are already removed by
//! [`crate::preprocess`].
//!
//! # Where the port stops short of upstream
//!
//! * Upstream's database (`infoDb.ts`) holds only `getVersion`, and that value is
//!   build-injected (`injected.version`, with `-tiny` appended unless the bundle includes
//!   large features) rather than parsed; `infoParser.ts` never writes to the database. There
//!   is therefore no database state after parsing, and [`Ast`] carries no version. The version
//!   `infoRenderer.ts` draws is its `version` argument, which rendering owns.
//! * `showInfo` is an unassigned keyword, so Langium's generated `Info` node does not store
//!   it. `info` and `info showInfo` parse to the same [`Ast`].
//! * Langium reports lexer and parser errors without a position in the message and recovers by
//!   skipping characters; this crate's [`ParseError`] carries a
//!   [`Span`](crate::error::Span) instead and reports the first offending character, which is
//!   the same character upstream's message points at.
//!
//! The renderer is [`render`], ported from `infoRenderer.ts`; it draws the version text that
//! renderer appends, in the scene's 400x100 canvas.

mod ast;
mod lexer;
mod parser;
pub mod render;

pub use ast::Ast;

use crate::error::ParseError;

/// Parses the diagram body (see [`crate::preprocess`]).
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    parser::parse(body)
}

#[cfg(test)]
mod tests;
