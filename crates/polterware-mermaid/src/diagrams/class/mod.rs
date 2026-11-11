//! `class` diagrams: a hand port of upstream's
//! `packages/mermaid/src/diagrams/class/parser/classDiagram.jison` (lexer, start
//! conditions, and grammar rules) as recursive descent, and of the database those rules
//! fill, `classDb.ts`, with the shapes `classTypes.ts` declares.
//!
//! [`ast`] holds what the database's getters answer, [`lexer`] the `%lex` section,
//! [`parser`] the grammar and the database's mutators, and `tests` the ported upstream
//! parser and database specs.
//!
//! No layout or rendering: the scene work belongs to a later stage.

use crate::error::ParseError;

mod ast;
mod lexer;
mod parser;
#[cfg(test)]
mod tests;

pub use ast::*;

/// Parses the diagram body, which starts with the `classDiagram` (or `classDiagram-v2`)
/// header line, as upstream's parser receives it after preprocessing.
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    parser::parse(body)
}
