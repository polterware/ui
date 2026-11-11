//! `architecture` diagrams: a port of upstream mermaid 12.3.0
//! `packages/parser/src/language/architecture/` (`architecture.langium`, `arch.langium`,
//! `tokenBuilder.ts`, `valueConverter.ts`) with the common rules and terminals it imports
//! (`packages/parser/src/language/common/`), and of the database semantics in
//! `packages/mermaid/src/diagrams/architecture/` (`architectureDb.ts`, `architectureParser.ts`,
//! `architectureTypes.ts`) plus `diagrams/common/`.
//!
//! The grammar is a Langium grammar, so the lexer in [`lexer`] reproduces the token order
//! Langium's `DefaultTokenBuilder` produces and the parser in [`parser`] is the recursive
//! descent of the rules. [`db`] ports the database as `populateDb` fills it, and [`ast`] holds
//! what its getters return: `getServices`, `getJunctions`, `getNodes`, `getNode`, `getGroups`,
//! `getEdges`, `getLayoutHints`, `getDataStructures`, `getDiagramTitle`, `getAccTitle`, and
//! `getAccDescription`. The body starts with the header line; frontmatter and `%%{init}%%`
//! directives are already removed by [`crate::preprocess`].
//!
//! # Where the port stops short of upstream
//!
//! * Configuration is not owned by this crate, so `getConfig`, `getConfigField`, and the
//!   `DEFAULT_ARCHITECTURE_CONFIG` values (`randomize`, `nodeSeparation`,
//!   `idealEdgeLengthMultiplier`, `edgeElasticity`, `numIter`) are absent. That is why
//!   `architecture.spec.ts`'s `randomize config` and `fcose layout config` describes are not
//!   ported; the complex nested diagram they parse is still checked for its services, groups,
//!   and edges.
//! * `setDiagramId`, `getDiagramId`, `setElementForId`, and `getElementById` belong to the
//!   renderer and are absent; nothing here lays out or draws.
//! * `architectureSeed.ts` (a seeded `Math.random` for the renderer) and its spec are
//!   rendering, so they are not ported.
//! * `sanitizeText` is approximated the same way the pie database does: `script`/`style`
//!   elements are dropped and a stray `<` is escaped, but there is no DOMPurify (a DOM
//!   library). This affects only the title and the accessibility strings; service, junction,
//!   and group titles are not sanitized upstream either.
//! * `ArchitectureValueConverter`'s `ARCH_TEXT_ICON` branch is dead in this grammar: no rule
//!   or terminal carries that name, so an icon given as a `STRING` goes through Langium's
//!   default `STRING` conversion.
//! * `YAML` and `DIRECTIVE` are hidden terminals that preprocessing removes; `%%` comments are
//!   still skipped here, including a one-line `%%{...}%%` comment.
//! * Langium reports lexer and parser errors without a position in the message; this crate's
//!   [`ParseError`] carries a [`Span`](crate::error::Span) instead. Database error messages
//!   keep upstream's exact text, including the `TypeError` message upstream throws when an
//!   edge endpoint is a group.

mod ast;
mod db;
mod lexer;
mod parser;

pub use ast::{
    Alignment, AlignmentDirection, Ast, DataStructures, Direction, DirectionPair, Edge, Group,
    Junction, LayoutHint, Node, Service, group_alignment_key,
};

use crate::error::ParseError;

/// Parses the diagram body (see [`crate::preprocess`]) and populates the database.
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    parser::parse(body)
}

#[cfg(test)]
mod tests;
