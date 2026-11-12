//! `class` diagrams: a hand port of upstream's
//! `packages/mermaid/src/diagrams/class/parser/classDiagram.jison` (lexer, start
//! conditions, and grammar rules) as recursive descent, and of the database those rules
//! fill, `classDb.ts`, with the shapes `classTypes.ts` declares.
//!
//! [`ast`] holds what the database's getters answer, [`lexer`] the `%lex` section,
//! [`parser`] the grammar and the database's mutators, [`layout_data`] the `getData()`
//! projection into the shared layout pipeline, [`render`] the `draw` entry, and [`tests`] the
//! ported upstream parser, database and renderer specs.

use crate::error::ParseError;

mod ast;
pub mod layout_data;
mod lexer;
mod parser;
pub mod render;
#[cfg(test)]
mod tests;

pub use ast::*;

/// Parses the diagram body, which starts with the `classDiagram` (or `classDiagram-v2`)
/// header line, as upstream's parser receives it after preprocessing.
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    parser::parse(body)
}
