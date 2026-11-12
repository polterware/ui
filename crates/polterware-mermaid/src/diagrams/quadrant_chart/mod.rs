//! `quadrant_chart` diagrams: a port of upstream mermaid 12.3.0
//! `packages/mermaid/src/diagrams/quadrant-chart/`.
//!
//! The lexer in `lexer.rs` ports the `%lex` block of `parser/quadrant.jison` (the `%x` modes,
//! the rule order, the case-insensitive flag, and jison's first-match tokenization); the parser
//! in `parser.rs` is the recursive descent of the `%%` grammar block; and `db.rs` ports
//! `quadrantDb.ts` as the grammar populates it, including the `parseStyles` validation and its
//! messages. [`Ast`] holds what the database has after parsing: the `QuadrantBuilderData` of
//! `quadrantBuilder.ts` (title, quadrant and axis labels, points, classes) and the common
//! accessibility getters.
//!
//! The body starts with the diagram's header line (`quadrantChart`); frontmatter and
//! `%%{init}%%` directives are already removed by [`crate::preprocess`]. [`parse`] returns the
//! populated [`Ast`] or the first error, with the line and column it was raised at.
//!
//! # Where the port stops short of upstream
//!
//! * A point's `x` and `y` stay the token text the grammar passed to `addPoint` (`"0.1"`), as
//!   the database holds it; `scaleLinear` turns it into a number in `getQuadrantPoints`, which
//!   the renderer ports. `radius` is an `f64`, `parseInt`'s JavaScript number.
//! * `getConfig` is not owned by this crate, so the builder's configuration and theme inputs
//!   are read from the [`crate::config::Config`] the caller passes to the renderer.
//! * `sanitizeText` is approximated (`common::sanitize`): `script`/`style` elements are dropped and a
//!   stray `<` is escaped, but there is no DOMPurify (a DOM library).
//! * The grammar's `text` objects carry a `type` (`'text'` or `'markdown'`); the database drops
//!   it, so it is not modelled.
//! * `section` in the `statement` rule, and the `NODE_STRING`, `DOWN`, `DEFAULT`, `UNIT`,
//!   `STYLE`, and `PCT` tokens, are unreachable upstream; they are not modelled.
//!
//! The renderer is [`render`], ported from `quadrantRenderer.ts` `draw` and from the geometry
//! `quadrantBuilder.ts` computes (`calculateSpace`, `getAxisLabels`, `getQuadrants`,
//! `getQuadrantPoints`, `getBorders`, `getTitle`, and `build`). It draws directly with d3 (no
//! unified pipeline), as upstream does.

mod ast;
mod db;
mod lexer;
mod parser;
pub mod render;

pub use ast::{Ast, ClassStyle, Point, Styles};

use crate::error::ParseError;

/// Parses the diagram body (see [`crate::preprocess`]).
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    let mut db = db::Db::default();
    let parsed = parser::parse(body, &mut db)?;
    Ok(db.build(parsed))
}

#[cfg(test)]
mod tests;
