//! `railroad` diagrams: a port of upstream mermaid 12.3.0 `packages/parser/src/language/railroad/`
//! (`railroad.langium`, `tokenBuilder.ts`, `valueConverter.ts`) and of the database semantics in
//! `packages/mermaid/src/diagrams/railroad/` (`railroadDb.ts`, `parser/railroadParser.ts`,
//! `railroadTypes.ts`).
//!
//! The grammar is a Langium grammar, so the lexer in [`lexer`] reproduces the token order
//! Langium's `DefaultTokenBuilder` produces and the parser in [`parser`] is the recursive
//! descent of the rules. [`db`] ports the database as `populateDb` fills it, and [`ast`] holds
//! what its getters return: `getRules`, `getRule`, `getTitle`/`getDiagramTitle`,
//! `getAccTitle`, and `getAccDescription`. The body starts with the header line; frontmatter
//! and `%%{init}%%` directives are already removed by [`crate::preprocess`].
//!
//! # Where the port stops short of upstream
//!
//! * Configuration is not owned by this crate, so `getConfig`, `getRailroadConfig`, and
//!   `DEFAULT_RAILROAD_CONFIG` are absent.
//! * `sanitizeText` is approximated: `script`/`style` elements are dropped and a stray `<` is
//!   escaped, but there is no DOMPurify (a DOM library). This affects the title, the
//!   accessibility strings, rule names, and every terminal, nonterminal, and special text,
//!   which is what `sanitizeAstNode` covers upstream.
//! * `RailroadRule.comment` is part of `railroadTypes.ts` but never assigned by the grammar or
//!   `transformRule`, so it is omitted.
//! * `YAML` and `DIRECTIVE` are hidden terminals that preprocessing removes; `%%` and `/* */`
//!   comments are still skipped here, the latter including its multi-line form.
//! * Langium reports lexer and parser errors without a position in the message; this crate's
//!   [`ParseError`] carries a [`Span`](crate::error::Span) instead. A duplicate rule name is
//!   only logged upstream, so it is not an error here either.
//! * The header keyword is case-insensitive to the detector but exact in the lexer, exactly as
//!   upstream's `railroadDetector.ts` and `railroad.langium` have it.
//! * Only the `railroad-beta` grammar is ported. Upstream's `railroad-ebnf`,
//!   `railroad-abnf`, and `railroad-peg` are separate grammars in `railroad-ebnf/`,
//!   `railroad-abnf/`, and `railroad-peg/`.
//!
//! The renderer is [`render`], ported from `railroadRenderer.ts` with `styles.ts`' options,
//! drawing directly with d3-style appends and `getBBox()` measurement (no unified pipeline),
//! as upstream does.

mod ast;
mod db;
mod lexer;
mod parser;
pub mod render;

pub use ast::{Ast, Node, Rule};

use crate::error::ParseError;

/// Parses the diagram body (see [`crate::preprocess`]).
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    let parsed = parser::parse(body)?;
    Ok(db::build(parsed))
}

#[cfg(test)]
mod tests;
