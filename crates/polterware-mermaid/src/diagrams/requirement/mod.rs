//! `requirement` diagrams: a port of upstream mermaid 12.3.0
//! `packages/mermaid/src/diagrams/requirement/` (`parser/requirementDiagram.jison`,
//! `requirementDb.ts`, and `types.ts`).
//!
//! The grammar is jison, so [`lexer`] reproduces the `%lex` rules and their start
//! conditions in rule order (first match wins, case-insensitive, `\b` after literal
//! keywords), and [`parser`] is a recursive descent of the grammar production by
//! production. [`db`] ports `RequirementDB`'s mutators into [`Ast`], which holds what the
//! database's getters return: `getRequirements`, `getElements`, `getRelationships`,
//! `getClasses`, `getDirection`, `getAccTitle`, and `getAccDescription`.
//!
//! The body starts with the header line; frontmatter and `%%{init}%%` directives are already
//! removed by [`crate::preprocess`]. `%%` and `#` comments are still skipped by this
//! diagram's own lexer.
//!
//! # Where the port stops short of upstream
//!
//! * `getData` is the renderer's projection, and this crate neither lays out nor draws, so
//!   its nodes, edges, `look`, and `config` are not modelled. That is why the
//!   `requirementDb.spec.ts` case that reads `getData().edges` is not ported; the raw
//!   relationships are in [`Ast::relationships`].
//! * The common database's diagram title is not here: the lexer has a `title` rule but no
//!   production consumes its token, so it is a parse error upstream as well.
//! * `sanitizeText` (DOMPurify) is not applied to the accessibility strings; sanitizing
//!   belongs to the painter, which knows its security level.
//! * A parse or lexical error carries a [`Span`](crate::error::Span) (line and column);
//!   upstream reports a jison line and token name instead. At end of input jison's
//!   explicit `EOF` terminal only exists in the initial state, so a style, string, or
//!   accessibility block left open at end of input is an error; this port keeps that.
//! * The jison lexer's empty-match loop in the `string` and `acc_descr_multiline`
//!   conditions at end of input terminates upstream only through the `done` flag; here it is
//!   an error, with the same parse result.
//! * Blank lines, and the line breaks comments leave behind, are skipped before the header.
//!   Upstream deletes the frontmatter block and comment lines before parsing, while
//!   [`crate::preprocess`] blanks their lines so spans and editor lines agree; the skip makes
//!   the two agree.

mod ast;
mod db;
mod lexer;
mod parser;
#[cfg(test)]
mod tests;

pub use ast::{
    Ast, Direction, Element, Relation, RelationshipType, Requirement, RequirementClass,
    RequirementType, RiskLevel, VerifyType,
};

use crate::error::ParseError;

/// Parses the diagram body (see [`crate::preprocess`]).
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    parser::parse(body)
}
