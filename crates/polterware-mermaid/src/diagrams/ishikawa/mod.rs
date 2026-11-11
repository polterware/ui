//! `ishikawa` diagrams: a port of upstream mermaid 12.3.0
//! `packages/mermaid/src/diagrams/ishikawa/` (`parser/ishikawa.jison`, `ishikawaDb.ts`,
//! `ishikawaTypes.ts`, `ishikawaDiagram.ts`, `ishikawaDetector.ts`).
//!
//! The grammar is a jison grammar, so the lexer in `lexer.rs` reproduces the `%lex` section
//! (its rule order, `%options case-insensitive`, and the non-flex first match) and the parser
//! in `parser.rs` is the recursive descent of the productions, `ishikawa.jison`'s
//! `yy.addNode` calls included. [`Ast`] holds what upstream's `IshikawaDB` getters return
//! after parsing: `getRoot` and `getDiagramTitle`.
//!
//! The body starts with the header line (`ishikawa` or `ishikawa-beta`, case-insensitively);
//! frontmatter and `%%{init}%%` directives are already removed by [`crate::preprocess`],
//! while `%%` comment lines are kept and the lexer skips them as upstream's grammar does.
//!
//! # Where the port stops short of upstream
//!
//! * Configuration is not owned by this crate, so `getConfig` is absent and the `%lex`
//!   options are read from the grammar rather than from a config.
//! * `sanitizeText` is approximated (`sanitize.rs`): `script`/`style` elements are dropped
//!   and a stray `<` is escaped, but there is no DOMPurify (a DOM library). This affects the
//!   labels `addNode` stores and the diagram title derived from the root.
//! * `getAccTitle` and `getAccDescription` are not part of [`Ast`]: `ishikawa.jison` never
//!   calls their setters, so they are always empty.
//! * A parse error carries a [`Span`](crate::error::Span) (line and column); upstream's
//!   jison reports a line and a token name in its message instead.
//! * Nothing here lays out or draws the fishbone: `ishikawaRenderer.ts`, `ishikawaStyles.ts`,
//!   and the renderer and styles `ishikawaDiagram.ts` registers are out of scope.
//!
//! The upstream spec ported as tests is `ishikawa.spec.ts`; what its port skips is listed in
//! `tests.rs`.
//!
//! No app consumes this yet: it replaces the placeholder `Ast`/`parse`, and
//! `Diagram::Ishikawa(Ast)` keeps its shape, so no app breaks.

mod ast;
mod db;
mod lexer;
mod parser;
mod sanitize;

pub use ast::{Ast, IshikawaNode};

use crate::error::ParseError;

/// Parses the diagram body (see [`crate::preprocess`]).
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    parser::parse(body)
}

#[cfg(test)]
mod tests;
