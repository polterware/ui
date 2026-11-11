//! `gantt` diagrams: a hand port of upstream mermaid 12.3.0 `packages/mermaid/src/diagrams/gantt/`.
//!
//! The grammar (`parser/gantt.jison`) is ported as a recursive-descent parser in
//! [`parser`] over the lexer start conditions in [`lexer`], and the semantics of
//! `ganttDb.js` live in [`db`]. [`Ast`] holds what upstream's database answers through its
//! getters after parsing: the format and marker statements, the accessibility texts, the
//! title, sections, the task records `getTasks` compiles, the links `click` statements made,
//! and the weekday/weekend settings. [`Ast::warnings`] keeps what upstream sends to
//! `log.warn`, because this crate has no logger.
//!
//! Dates and durations are dayjs's (`customParseFormat`, `isoWeek`, `advancedFormat`) in
//! [`date`], written with plain arithmetic: no date crate.
//!
//! Differences from upstream, all forced by the environment or listed with the module:
//! - There is no configuration object here (frontmatter and `%%{init}%%` are removed by
//!   [`crate::preprocess`]), so `displayMode` (only configuration sets it) is absent, a link
//!   keeps its URL unsanitized (a painter's concern), and the `securityLevel` checks that
//!   only affect DOM handlers are moot.
//! - Dates are wall-clock with no zone: `x` timestamps and explicit offsets read as UTC, and
//!   "today" is today UTC.
//! - `getTasks`' compilation runs during [`parse`], so the errors upstream raises when the
//!   renderer asks for the tasks are [`ParseError`]s here, with the task's span.
//! - `click` callbacks (`funs`, `bindFunctions`) are DOM-only and not modelled; the
//!   `clickable` class and the link are.
//! - Parse errors carry a [`Span`](crate::error::Span) (line and column).
//! - Week tokens (`w`, `ww`) and zone names (`z`) in a `dateFormat` are not implemented: a
//!   format using them parses as invalid.
//!
//! No layout or rendering: the scene work belongs to a later stage. No app consumes this
//! yet: it replaces the placeholder `Ast`/`parse`, and `Diagram::Gantt(Ast)` keeps its
//! shape, so no app breaks.

mod ast;
mod date;
mod db;
mod lexer;
mod parser;
#[cfg(test)]
mod tests;

use crate::error::ParseError;

pub use ast::{Ast, Link, Task};
pub use date::Date;

/// Parses the diagram body, which starts with the `gantt` header line, as upstream's parser
/// receives it after preprocessing.
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    parser::parse(body)
}
