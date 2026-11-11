//! `state` diagrams: a port of upstream mermaid 12.3.0 `packages/mermaid/src/diagrams/state/`.
//!
//! The grammar (`parser/stateDiagram.jison`) is ported by hand as a recursive-descent parser
//! in `parser.rs` over the lexer start conditions in `lexer.rs`, and the database semantics of
//! `stateDb.ts` (including `dataFetcher.ts`) live in `db.rs`. [`Ast`] holds what upstream's
//! `StateDB` exposes through its getters after parsing: `getStates`, `getRelations`,
//! `getClasses`, `getLinks`, `getDirection`, `getData` (nodes and edges), `getAccTitle`,
//! `getAccDescription`, and `getDiagramTitle`.
//!
//! Both headers the diagrams register (`stateDiagram` and `stateDiagram-v2`) are accepted; the
//! grammar returns the same `SD` token for either and mermaid 12.3.0 loads the version 2
//! database for both, so this port always applies the version 2 document translation.
//!
//! Differences from upstream, all forced by the environment:
//! - There is no configuration object here (frontmatter and `%%{init}%%` are removed by
//!   [`crate::preprocess`]), so `look`, `state.wrappingWidth`, and `state.minNodeWidth` are
//!   absent from the nodes and edges, as is `getData().config`; `getData().other` is always
//!   `{}` and is omitted. The edge arrow type is the one the default `neo` look chooses
//!   (`arrow_barb_neo`), and the note paddings are the documented config defaults.
//! - `sanitizeText`/`sanitizeUrl` (DOMPurify) are approximated: `script`/`style` elements are
//!   dropped with their content, a stray `<` is escaped, and `<br/>` is serialized as `<br>`.
//!   Full sanitizing belongs to the painter, which knows its HTML mode and security level.
//! - `docTranslator`'s trailing concurrency region gets the deterministic id `id-<n>` instead
//!   of `generateId()`'s random one; no getter exposes an ordering that depends on it.
//! - `getLinks` keys upstream by the parsed id statement object; [`Link`] keeps its id string.
//! - Parse errors carry a [`Span`](crate::error::Span) (line and column). The one database
//!   error (a group with more than one label) has none, because upstream's error is not
//!   tied to a line.
//!
//! Renderer-only state is left out: `funs`/`setupToolTips` bind DOM events, `rootDoc`'s raw
//! statement list has no getter, and `getConfig()` returns configuration this crate does not
//! own. No app consumes this yet: it replaces the placeholder `Ast`/`parse`, and
//! `Diagram::State(Ast)` keeps its shape, so no app breaks.

mod ast;
mod db;
mod lexer;
mod parser;
mod sanitize;
#[cfg(test)]
mod tests;

pub use ast::{
    Ast, Direction, Edge, EdgePattern, Link, Node, Note, NotePosition, Relation, RelationStatement,
    State, StateKind, StateStatement, Statement, StyleClass,
};

use crate::error::ParseError;

/// Parses the diagram body (see [`crate::preprocess`]).
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    let parsed = parser::parse_source(body)?;
    db::build(parsed.root_doc, parsed.acc_title, parsed.acc_description)
}
