//! `timeline` diagrams: a hand port of upstream mermaid 12.3.0
//! `packages/mermaid/src/diagrams/timeline/`: the jison grammar in `parser/timeline.jison`
//! (lexer start conditions, tokens, and rules) as a recursive-descent parser, and the
//! database semantics of `timelineDb.js` with the `common/commonDb.ts` getters it forwards
//! to.
//!
//! [`lexer`] reproduces the `%lex` section rule by rule in jison's first-match order,
//! including its exclusive start conditions and the word boundaries jison-lex adds. [`parser`]
//! walks the grammar and applies the actions, and [`db`] keeps the state those actions
//! mutate. [`Ast`] holds what upstream's getters return after parsing: `getDirection`,
//! `getSections`, `getTasks`, `getDiagramTitle`, `getAccTitle`, and `getAccDescription`.
//! The body starts with the header line; frontmatter and `%%{init}%%` directives are already
//! removed by [`crate::preprocess`].
//!
//! # Where the port stops short of upstream
//!
//! * `sanitizeText` is a no-op: upstream runs DOMPurify, which this crate cannot. The
//!   trims and replacements the common database applies after it are kept.
//! * Upstream's `addEvent` throws a `TypeError` when an event has no task; here the same
//!   diagram is a [`ParseError`] carrying the event's span.
//! * Parse errors carry a [`Span`](crate::error::Span) (line and column); jison reports a
//!   line and a caret instead.
//! * The lexer's two end-of-input tokens are kept distinct (`EOF` and the parser's `$end`),
//!   because upstream's lexer fails a diagram whose last token is a zero-width accessibility
//!   value (`accTitle:` with nothing after it). That failure is preserved.
//! * Leading blank lines are skipped before the header. Upstream's preprocessing deletes the
//!   frontmatter and directive lines, while [`crate::preprocess`] keeps them as empty lines so
//!   spans keep their line numbers; skipping them is what makes a frontmatter diagram parse.
//! * Nothing here lays out or draws the timeline; `timelineRenderer.ts`,
//!   `timelineRendererVertical.ts`, `svgDraw.js`, and `styles.js` are out of scope.
//!
//! No app consumes this yet: it replaces the placeholder `Ast`/`parse`, and
//! `Diagram::Timeline(Ast)` keeps its variant, so no app breaks.

mod ast;
mod db;
mod lexer;
mod parser;
#[cfg(test)]
mod tests;

pub use ast::{Ast, Direction, Task};

use crate::error::ParseError;

/// Parses the diagram body, which starts with the `timeline` header line, as upstream's
/// parser receives it after preprocessing.
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    parser::parse(body)
}
