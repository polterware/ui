//! `pie` diagrams: a port of upstream mermaid 12.3.0 `packages/parser/src/language/pie/`
//! (`pie.langium`, `tokenBuilder.ts`, `valueConverter.ts`) with the common rules it imports
//! (`packages/parser/src/language/common/common.langium`), and of the database semantics in
//! `packages/mermaid/src/diagrams/pie/` (`pieDb.ts`, `pieParser.ts`, `pieTypes.ts`).
//!
//! The grammar is a Langium grammar, so the lexer in `lexer.rs` reproduces the token order
//! Langium's `DefaultTokenBuilder` produces and the parser in `parser.rs` is the recursive
//! descent of the rules. [`Ast`] holds what upstream's `PieDB` getters return after parsing:
//! `getSections`, `getShowData`, `getDiagramTitle`, `getAccTitle`, and `getAccDescription`.
//! The body starts with the header line; frontmatter and `%%{init}%%` directives are already
//! removed by [`crate::preprocess`].
//!
//! # Where the port stops short of upstream
//!
//! * Configuration is not owned by this crate, so `getConfig` and `DEFAULT_PIE_CONFIG` are
//!   absent. That is why the `config` describe of `pie.spec.ts` is not ported (its
//!   `setConfig`/`resetConfig` cases are `it.todo` upstream and the `getConfig` case compares
//!   against a config default).
//! * `sanitizeText` is approximated: `script`/`style` elements are dropped and a stray `<` is
//!   escaped, but there is no DOMPurify (a DOM library). This affects only the title and the
//!   accessibility strings; section labels are not sanitized upstream either.
//! * The pie value converter's `PIE_SECTION_LABEL` branch is dead in this grammar: the rule
//!   assigns `label=STRING`, so labels go through Langium's default `STRING` conversion
//!   (strip the quotes, interpret backslash escapes).
//! * The lexer models the tokens Langium's `getAllReachableRules` makes reachable from the
//!   `Pie` entry rule. The common terminals that no parser rule references (`BOOLEAN`,
//!   `FLOAT`, `INT`, `NUMBER`, `ID`) are therefore not tokens, exactly as upstream's generated
//!   lexer has it. `YAML` and `DIRECTIVE` are hidden terminals that preprocessing removes;
//!   `%%` comments are still skipped here, including a one-line `%%{...}%%`.
//! * Langium reports lexer and parser errors without a position in the message; this crate's
//!   [`ParseError`] carries a [`Span`](crate::error::Span) instead. The negative-value database
//!   error keeps upstream's exact message text.
//! * Nothing here lays out or draws the slice labels, legend, or arcs; `pieRenderer.ts` is out
//!   of scope.

mod ast;
mod db;
mod lexer;
mod parser;
mod sanitize;

pub use ast::{Ast, Section};

use crate::error::ParseError;

/// Parses the diagram body (see [`crate::preprocess`]).
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    parser::parse(body)
}

#[cfg(test)]
mod tests;
