//! `usecase` diagrams: a port of upstream mermaid 12.3.0
//! `packages/mermaid/src/diagrams/usecase/`.
//!
//! The diagram is parsed by Chevrotain, not by a jison or Langium grammar. [`lexer`] reproduces
//! the token table and modes of `parser/usecase.tokens.ts` (including `LabelText`/`Word`
//! categories and `longer_alt` precedence), [`parser`] is the recursive descent of
//! `parser/usecase.parser.ts`, and [`db`] ports the visitor and model builder
//! (`parser/usecase.visitor.ts`, `parser/usecaseModelBuilder.ts`) plus the JSON reader
//! (`parser/usecaseJson.ts`). [`Ast`] holds what upstream's `UsecaseDB` getters return after
//! parsing: `getActors`, `getUseCases`, `getSystemBoundaries`, `getRelationships`, `getNotes`,
//! `getJsonNodes`, `getClassDefs`, `getDirection`, `getAccTitle`, `getAccDescription`, the
//! counters and symbol table of `UsecaseFields`, and `getAST`'s normalized graph AST.
//! The body starts with the header line; frontmatter and `%%{init}%%` directives are already
//! removed by [`crate::preprocess`].
//!
//! # Where the port stops short of upstream
//!
//! * Configuration is not owned by this crate, so `getConfig`, `getData` (the renderer
//!   contract), `DEFAULT_USECASE_CONFIG`, and every layout or font value derived from them are
//!   absent. `usecase.spec.ts`'s `getData` and `P4 LayoutData contract` describes and
//!   `usecaseDb.spec.ts`'s oval-clearance case are therefore not ported.
//! * `sanitizeText` (DOMPurify) is not available; the accessible title and description are
//!   still trimmed and re-wrapped exactly as `commonDb.ts` does. Labels and stereotypes are
//!   not sanitized upstream either, and the renderer owns escaping.
//! * Parse errors carry a [`Span`](crate::error::Span); database messages keep upstream's
//!   exact text, including the `at line …, column …` suffix and the previous declaration's
//!   location.
//! * The graph AST is held as Rust data; upstream's `JSON.parse(JSON.stringify(ast))`
//!   round-trip assertion is not ported because the structs are not serialized.
//! * `usecaseRenderer.ts`'s accessible names, DOM attributes, font custom properties,
//!   title insertion and annotations are the renderer's; [`render`] ports the geometry and
//!   the stylesheet's edge and label styling. `usecase-colorIndex.spec.ts`,
//!   `usecaseRoleColors.spec.ts`, and `usecase.docs.spec.ts` cover colours and
//!   documentation, so they are out of scope.
//! * The Chevrotain lexer has three modes; this port keeps them. A lexer error names the
//!   offending character rather than Chevrotain's `Unable to tokenize` diagnostic.
//! * `preprocess` removes frontmatter and `%%` comment lines and keeps a directive's line
//!   breaks, while upstream hands this parser the cleaned text. Leading blank and comment
//!   lines are skipped before the header so the crate entry point accepts both shapes.

mod ast;
mod db;
pub mod layout_data;
mod lexer;
mod parser;
pub mod render;

pub use ast::{
    Actor, ActorType, Animation, ArrowType, Ast, BoundaryType, ClassDef, Direction, EdgeOccurrence,
    GraphAst, GraphEdge, GraphGroup, GraphHeader, GraphNode, GraphStatement, LabelType,
    MetadataOccurrence, NodeOccurrence, Relationship, RelationshipType, StatementKind,
    SystemBoundary, UseCase, UseCaseShape, UsecaseJsonNode, UsecaseNote, UsecaseSymbolKind,
};

use crate::error::ParseError;

/// Parses the diagram body (see [`crate::preprocess`]).
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    parser::parse(body)
}

#[cfg(test)]
mod tests;
