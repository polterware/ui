//! `c4` diagrams: a port of upstream mermaid 12.3.0 `packages/mermaid/src/diagrams/c4/`
//! (`parser/c4Diagram.jison`, `c4Db.ts`, `c4Types.ts`, `c4Diagram.ts`, `c4Detector.ts`).
//!
//! The grammar is jison, so the lexer in `lexer.rs` reproduces the jison start conditions,
//! rule order, and keyword word boundaries, and the parser in `parser.rs` is the recursive
//! descent of the grammar's rules, calling the database methods the jison actions call. [`Ast`]
//! holds what upstream's `c4Db` getters return after parsing: `getC4Type`, `getTitle`,
//! `getC4ShapeArray`, `getC4Shape`, `getBoundaries`, `getRels`, `getC4ShapeInRow`,
//! `getC4BoundaryInRow`, and `getAccDescription`. The body starts with the header line;
//! frontmatter and `%%{init}%%` directives are already removed by [`crate::preprocess`].
//!
//! # Where the port stops short of upstream
//!
//! * Configuration is not owned by this crate, so `getConfig`, `setWrap`, and
//!   `DEFAULT_C4_CONFIG` are absent. `wrap` is therefore always `false`, which is what
//!   upstream's specs see before `init` runs.
//! * `sanitizeText` is approximated: `script`/`style` elements are dropped and a stray `<` is
//!   escaped, but there is no DOMPurify (a DOM library). Only the title, the C4 type, and the
//!   accessible description are sanitized.
//! * `start: direction` calls `yy.setDirection`, which `c4Db.ts` does not define, so upstream
//!   throws on a body that is only `direction TB`. This port records the direction in
//!   [`Ast::direction`] instead, and still rejects a `direction` token between statements,
//!   where the grammar has no rule for it.
//! * A named attribute that lands in a slot upstream assigns straight to a string (an alias,
//!   a relationship endpoint) stores an object there; this port keeps its value string. A key
//!   no C4 type declares is dropped.
//! * `parseInt`'s `NaN` becomes `None`, so a non-numeric offset stays unset instead of
//!   storing `NaN`.
//! * `getAccTitle` is never set by the C4 grammar (upstream's `mermaidAPI` spec notes this),
//!   so it is not stored.
//! * `LINETYPE`, `ARROWTYPE`, and `PLACEMENT` are renderer constants exported next to the
//!   database; they are not parse data and are not ported.
//! * Nothing here lays out or draws: `c4Renderer.ts`, `svgDraw.ts`, `c4ShapeAdapter.ts`, and
//!   `styles.js` are out of scope.

mod ast;
mod db;
mod lexer;
mod parser;

pub use ast::{Ast, Boundary, C4Element, Rel, Shape, Text, TextValue};

use crate::error::ParseError;

/// Parses the diagram body (see [`crate::preprocess`]).
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    parser::parse(body)
}

#[cfg(test)]
mod tests;
