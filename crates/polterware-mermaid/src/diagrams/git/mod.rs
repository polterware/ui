//! `git` diagrams: a hand port of upstream mermaid 12.3.0's Langium grammar and database.
//!
//! The grammar is `packages/parser/src/language/gitGraph/gitGraph.langium` with
//! `reference.langium` and the `common/langium` rules it imports, and its token builder
//! (`tokenBuilder.ts`); [`lexer`] reproduces the tokens Langium's `DefaultTokenBuilder`
//! produces and [`parser`] is the recursive descent of the rules. [`db`] ports the database in
//! `packages/mermaid/src/diagrams/git/gitGraphAst.ts` as `gitGraphParser.ts` populates it, and
//! [`ast`] holds what its getters return.
//!
//! The body starts with the diagram's header line (`gitGraph`, `gitGraph:`, or
//! `gitGraph <direction>:`); frontmatter and `%%{init}%%` directives are already removed by
//! [`crate::preprocess`]. [`parse`] keeps upstream's signature: it returns the populated
//! [`Ast`] or the first error, with the line and column of the token it was raised at.
//!
//! [`render`] ports `gitGraphRenderer.ts`: the branch lanes, the commit positions, the
//! bullets, the rotated commit labels, the tags, the arrows and the title, as a
//! [`Scene`](crate::scene::Scene).

use crate::error::ParseError;

mod ast;
mod db;
mod lexer;
mod parser;
pub mod render;

pub use ast::{Ast, Branch, Commit, CommitType, Direction};

/// Parses the diagram body (see [`crate::preprocess`]).
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    parser::parse(body)
}

#[cfg(test)]
mod tests;
