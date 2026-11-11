//! `wardley` diagrams: a port of upstream mermaid 12.3.0
//! `packages/parser/src/language/wardley/` (`wardley.langium`, `valueConverter.ts`) with the
//! common rules it imports (`packages/parser/src/language/common/common.langium`,
//! `common/valueConverter.ts`, `common/tokenBuilder.ts`), and of the database semantics in
//! `packages/mermaid/src/diagrams/wardley/` (`wardleyParser.ts`, `wardleyDb.ts`,
//! `wardleyBuilder.ts`, `wardleyTypes.ts`, and the `populateCommonDb.ts` helper).
//!
//! The grammar is a Langium grammar, so the lexer in `lexer.rs` reproduces the keyword and
//! terminal order Langium's `DefaultTokenBuilder` produces, and the parser in `parser.rs` is
//! the recursive descent of the rules. [`Ast`] holds what upstream's database getters return
//! after parsing: `getWardleyData()` (the `WardleyBuildResult` of `WardleyBuilder.build()`)
//! plus `getDiagramTitle`, `getAccTitle`, and `getAccDescription`. The body starts with the
//! header line; frontmatter and `%%{init}%%` directives are already removed by
//! [`crate::preprocess`].
//!
//! # Where the port stops short of upstream
//!
//! * `getConfig` and `clear` are absent: this crate owns no configuration and builds one
//!   database per parse.
//! * `sanitizeText` is approximated in `common::sanitize` (no DOMPurify, a DOM library), exactly as
//!   the pie database approximates it. It touches only the title and accessibility strings.
//! * Nothing here lays out or draws the map; `wardleyRenderer.ts` and `styles.ts` are out of
//!   scope.
//! * Langium reports parser errors with Chevrotain's recovery and without a crate position;
//!   this port is strict and stops at the first syntax error, with a [`ParseError::span`].
//!   The database errors keep upstream's exact message text.
//! * The lexer omits `BOOLEAN`, `FLOAT`, and `NUMBER`, which the generated lexer can never
//!   reach because earlier terminals match their text first, and `YAML`/`DIRECTIVE`, which
//!   preprocessing removes. See `lexer.rs`.

mod ast;
mod db;
mod lexer;
mod parser;

pub use ast::{
    Accelerator, Annotation, Ast, Axes, Deaccelerator, Flow, Link, Node, Note, Pipeline, Point,
    Size, SourceStrategy, Trend,
};

use crate::error::ParseError;

/// Parses the diagram body (see [`crate::preprocess`]).
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    db::build(parser::parse(body)?)
}

#[cfg(test)]
mod tests;
