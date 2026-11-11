//! `venn` diagrams: a port of upstream mermaid 12.3.0 `packages/mermaid/src/diagrams/venn/`
//! — the lexer and grammar of `parser/venn.jison` (jison), the database semantics of
//! `vennDB.ts`, the data shapes of `vennTypes.ts`, and the parser wiring of `vennDiagram.ts`.
//!
//! The lexer in `lexer.rs` reproduces the jison start condition `bol`, its rule order and
//! `%options case-insensitive`; the parser in `parser.rs` is the recursive descent of the
//! grammar. [`Ast`] holds what upstream's database getters answer after parsing:
//! `getDiagramTitle`, `getSubsetData`, `getTextData`, and `getStyleData`. The body starts with
//! the header line (`venn-beta`); frontmatter and `%%{init}%%` directives are already removed by
//! [`crate::preprocess`]. Nothing here lays out or draws.
//!
//! # Where the port stops short of upstream
//!
//! * Configuration is not owned by this crate, so `vennDB.ts`'s `getConfig` and
//!   `DEFAULT_CONFIG.venn` defaults are absent. The renderer's `styles.ts` and
//!   `vennRenderer.ts` are out of scope, and so are their specs (`vennRenderer.spec.ts` and
//!   `vennPalette.spec.ts`).
//! * `sanitizeText` is approximated: `script`/`style` elements are dropped, a `<` that opens no
//!   tag is escaped, and `<br/>`/`<br />` serialize as `<br>`, but there is no DOMPurify (a DOM
//!   library). The diagram title is the only string this database sanitizes.
//! * `vennDetector.ts`'s detector lives in [`crate::diagrams::detect`] and already matches
//!   upstream's `/^\s*venn-beta/`; the lexer is case-insensitive like the jison grammar, so a
//!   caller that parsed `VENN-BETA` directly would pass where detection would not.
//! * Parse errors carry a [`Span`](crate::error::Span); jison reports a line and an excerpt. The
//!   database's three semantic errors keep upstream's exact message text.
//! * `getCurrentSets` and `getIndentMode` are parser state, not parsed data, so they are absent
//!   from [`Ast`]; `clear()` is a fresh `Ast` per parse.
//!
//! `Diagram::Venn(Ast)` keeps its variant, and the placeholder `Ast` had no consumers, so no
//! app breaks.

mod ast;
mod db;
mod lexer;
mod parser;

pub use ast::{Ast, StyleEntry, Subset, TextNode};

use crate::error::ParseError;

/// Parses the diagram body (see [`crate::preprocess`]).
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    parser::parse(body)
}

#[cfg(test)]
mod tests;
