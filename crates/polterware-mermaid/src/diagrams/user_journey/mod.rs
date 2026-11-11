//! `user_journey` diagrams: a hand port of upstream mermaid 12.3.0
//! `packages/mermaid/src/diagrams/user-journey/`, namely the lexer and grammar of
//! `parser/journey.jison` and the database semantics of `journeyDb.js` with the common
//! accessibility setters of `diagrams/common/commonDb.ts`.
//!
//! The grammar is a Jison LALR grammar, so `lexer` reproduces its start conditions and
//! Jison's rule selection (the first matching rule, not the longest: the grammar does not set
//! `%options flex`), and `parser` is the recursive descent of its rules. [`Ast`] holds what
//! upstream's `journeyDb` getters answer after parsing: `getDiagramTitle`, `getAccTitle`,
//! `getAccDescription`, `getSections`, `getTasks`, and `getActors`. The body starts with the
//! `journey` header line; frontmatter and `%%{init}%%` directives are already removed by
//! [`crate::preprocess`].
//!
//! # Where the port stops short of upstream
//!
//! * `getConfig` is configuration and is not ported.
//! * `sanitizeText` (DOMPurify, a DOM library) is not ported. The common setters keep the
//!   text they receive and apply only the whitespace replacements of `setAccTitle` and
//!   `setAccDescription`; sanitizing the title and accessibility strings belongs to the
//!   painter, as in the gantt and sequence ports.
//! * `addTaskOrg` is not ported: no grammar rule and no renderer calls it.
//! * `getTasks`' `compileTasks` loop is a no-op for this diagram (it reads a `processed`
//!   field nothing sets), so the task records are the raw ones. `getActors` reads the
//!   `tasks` array `getTasks` fills, so [`Ast::actors`] is `getActors` after `getTasks`, the
//!   order the specs call them in. Calling `getTasks` twice duplicates the records upstream;
//!   this crate's `Ast` is one snapshot.
//! * [`parse`] skips leading newlines: this crate's preprocess keeps blank lines where it
//!   removed frontmatter or directives, while upstream's parser receives text that starts
//!   with the header.
//! * Jison's end-of-input handling is reproduced, including the quirk that makes
//!   `journey\naccTitle:` a parse error upstream; see `lexer`'s module docs.
//! * Parse errors carry a [`Span`](crate::error::Span) (line and column); upstream reports a
//!   line and a caret.
//! * `styles.js`, `svgDraw.js`, `journeyRenderer.ts`, and `journeyDiagram.ts` are layout,
//!   styling, and rendering, out of scope here.
//!
//! No app consumes this yet: it replaces the placeholder `Ast`/`parse` and
//! `Diagram::UserJourney` keeps its shape, so no app breaks.

mod ast;
mod db;
mod lexer;
mod parser;

pub use ast::{Ast, Task};

use crate::error::ParseError;

/// Parses the diagram body, which starts with the `journey` header line, as upstream's
/// parser receives it after preprocessing.
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    parser::parse(body)
}

#[cfg(test)]
mod tests;
