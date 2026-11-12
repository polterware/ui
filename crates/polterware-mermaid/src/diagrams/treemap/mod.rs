//! `treemap` diagrams: a port of upstream mermaid 12.3.0
//! `packages/parser/src/language/treemap/` (`treemap.langium`, `tokenBuilder.ts`,
//! `valueConverter.ts`, `treemap-validator.ts`) and of the database semantics in
//! `packages/mermaid/src/diagrams/treemap/` (`db.ts`, `parser.ts`, `types.ts`, `utils.ts`)
//! plus `diagrams/common/`.
//!
//! The grammar is a Langium grammar, so the lexer in [`lexer`] reproduces the token order
//! Langium's `DefaultTokenBuilder` produces and the parser in [`parser`] is the recursive
//! descent of the rules. [`db`] ports `populate` and `buildHierarchy` as they fill the
//! database, and [`ast`] holds what its getters return: `getNodes`, `getRoot`, `getClasses`,
//! `getStylesForClass`, `getDiagramTitle`, `getAccTitle`, and `getAccDescription`, with the
//! `levels` map alongside. The body starts with the header line; frontmatter and `%%{init}%%`
//! directives are already removed by [`crate::preprocess`].
//!
//! # Where the port stops short of upstream
//!
//! * `renderer.ts` and `styles.ts` draw with plain d3; [`render`] ports `draw` and resolves the
//!   styles into the scene's paints. Configuration is owned by [`crate::config`], so the
//!   `TreemapDiagramConfig` defaults live there and `draw` reads them from [`crate::config::Config`].
//! * `treemap-validator.ts`'s `checkSingleRoot` is a document-validation check, not a parser
//!   check: mermaid's `parse` only rejects lexer and parser errors, so multiple root nodes
//!   parse here too, as the upstream parser spec expects.
//! * `sanitizeText` is approximated the same way the pie database does: `script`/`style`
//!   elements are dropped and a stray `<` is escaped, but there is no DOMPurify (a DOM
//!   library). This affects only the title and the accessibility strings.
//! * `TreemapTokenBuilder`'s keyword restriction is inert: `treemap` is one alternative of the
//!   `TREEMAP_KEYWORD` terminal, and Langium turns terminal keywords into regex terminals,
//!   not keyword tokens, so the generated keyword list is empty.
//! * Mermaid removes whole comment lines (`diagram-api/comments.ts`'s `cleanupComments`)
//!   before Langium sees the text, and [`crate::preprocess`] does the same. A body handed to
//!   the parser directly still carries them, so the lexer removes those lines itself, leading
//!   indentation included, and the parser skips the whitespace `cleanupComments` trims before
//!   the header. A comment that follows other text on its line is still a hidden
//!   `ML_COMMENT`, as in the grammar.
//! * `NUMBER2` goes through the `MyNumber` data type rule, so the custom converter's
//!   `parseFloat(input.replace(/,/g, ''))` result is stringified and parsed again by the
//!   `number` return type; the value is the same, and this port computes it directly.
//! * The `ClassDefStatement` row that replaces a `TreemapRow` in Langium's AST is modelled as
//!   a parsed-row variant; [`Ast`] only exposes what the database keeps from it.

mod ast;
mod db;
mod lexer;
mod parser;

pub mod render;

pub use ast::{Ast, StyleClass, TreemapNode};

use crate::error::ParseError;

/// Parses the diagram body (see [`crate::preprocess`]).
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    parser::parse(body)
}

#[cfg(test)]
mod tests;
