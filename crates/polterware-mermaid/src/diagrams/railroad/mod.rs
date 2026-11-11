//! `railroad` diagrams. Port of upstream `packages/mermaid/src/diagrams/` (see crate docs).

use crate::error::ParseError;

/// The parsed diagram: what upstream's diagram database holds after parsing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ast {}

/// Parses the diagram body (see [`crate::preprocess`]).
pub fn parse(_body: &str) -> Result<Ast, ParseError> {
    Err(ParseError::unsupported("railroad"))
}
