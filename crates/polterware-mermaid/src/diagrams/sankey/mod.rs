//! `sankey` diagrams: a port of upstream mermaid 12.3.0 `packages/mermaid/src/diagrams/sankey/`
//! — the lexer and grammar of `parser/sankey.jison` (jison), the database semantics of
//! `sankeyDB.ts`, the input preparation of `sankeyUtils.ts` (`prepareTextForParsing`), and the
//! comment removal the mermaid pipeline applies before parsing (`diagram-api/comments.ts`
//! `cleanupComments`).
//!
//! The lexer in `lexer.rs` reproduces the jison start conditions and rule order; the parser in
//! `parser.rs` is the recursive descent of the grammar. [`Ast`] holds what upstream's database
//! getters answer after parsing (`getNodes`, `getLinks`, `getGraph`). The body starts with the
//! header line (`sankey` or `sankey-beta`); frontmatter and `%%{init}%%` directives are already
//! removed by [`crate::preprocess`]. Nothing here lays out or draws.
//!
//! # Where the port stops short of upstream
//!
//! * Configuration is not owned by this crate, so `sankeyDB.ts`'s `getConfig` and the
//!   `DEFAULT_SANKEY_CONFIG` defaults are absent. The renderer's config reads
//!   (`showValues`, `linkColor`, `nodeAlignment`, `labelStyle`, `nodeWidth`, `nodePadding`,
//!   `nodeColors`) are not needed to parse.
//! * `sanitizeText` is approximated: `script`/`style` elements are dropped, a `<` that opens no
//!   tag is escaped, and `<br/>`/`<br />` serialize as `<br>`, but there is no DOMPurify (a DOM
//!   library). Node ids are the only strings this database sanitizes.
//! * `sankeyUtils.ts`'s `prepareTextForParsing` is not applied as a text rewrite: its final
//!   `trim` is the lexer's leading-whitespace skip plus the parser's end trim, its newline-run
//!   collapse is the parser's acceptance of several `NEWLINE` tokens where the grammar has one,
//!   and the lexer collapses a newline run inside a quoted field. A quoted field that spans a
//!   run of blank lines therefore keeps the several newlines as one (upstream also collapses
//!   them) but their byte offsets, and every span, still address the body the caller passed.
//! * Comment lines are removed by the mermaid pipeline, not by the grammar.
//!   [`crate::preprocess`] blanks them in [`crate::preprocess::Source::body`], and this module
//!   skips one it is handed directly, keeping its bytes so spans still address the body.
//! * Parse errors carry a [`Span`](crate::error::Span); jison reports a line and an excerpt.
//!
//! No app breaks: `Diagram::Sankey(Ast)` keeps its variant and the placeholder `Ast` had no
//! consumers. `sankeyRenderer.ts` and `styles.js` are out of scope.

mod ast;
mod db;
mod lexer;
mod parser;

pub use ast::{Ast, Link, Node};

use crate::error::ParseError;

/// Parses the diagram body (see [`crate::preprocess`]).
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    parser::parse(body)
}

#[cfg(test)]
mod tests;
