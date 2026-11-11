//! `packet` diagrams: a port of upstream mermaid 12.3.0 `packages/parser/src/language/packet/`
//! (`packet.langium`, `tokenBuilder.ts`, `module.ts`) with the common rules it imports
//! (`packages/parser/src/language/common/common.langium`, `tokenBuilder.ts`,
//! `valueConverter.ts`), and of the database semantics in
//! `packages/mermaid/src/diagrams/packet/` (`parser.ts`'s `populate` and `getNextFittingBlock`,
//! `db.ts`, `types.ts`).
//!
//! The grammar is a Langium grammar, so the lexer in `lexer.rs` reproduces the token order
//! Langium's `DefaultTokenBuilder` produces and the parser in `parser.rs` is the recursive
//! descent of the `Packet` and `PacketBlock` rules. [`Ast`] holds what upstream's `PacketDB`
//! getters return after parsing: `getPacket`, `getDiagramTitle`, `getAccTitle`, and
//! `getAccDescription`. The body starts with the header line; frontmatter and `%%{init}%%`
//! directives are already removed by [`crate::preprocess`].
//!
//! # Where the port stops short of upstream
//!
//! * Configuration is not owned by this crate, so `getConfig` and `DEFAULT_PACKET_CONFIG` are
//!   absent. `populate` splits blocks into rows by `getConfig().bitsPerRow`, so the rows here
//!   use the default, [`DEFAULT_BITS_PER_ROW`]; a configured `bitsPerRow` is not read. The
//!   other settings only feed the renderer.
//! * `INT` becomes an `i64`. JavaScript's `parseInt` keeps every digit, while a value that does
//!   not fit saturates (as `block/parser.rs` does), so a 20-digit bit index appears saturated in
//!   an error message.
//! * `sanitizeText` is approximated as `pie/sanitize.rs` approximates it: `script`/`style`
//!   elements are dropped and a stray `<` is escaped, but there is no DOMPurify (a DOM
//!   library). This affects only the title and the two accessibility strings.
//! * Nothing here lays out or draws the rows, blocks, labels, or bit numbers;
//!   `packet/renderer.ts` is out of scope.

mod ast;
mod db;
mod lexer;
mod parser;
mod sanitize;

pub use ast::{Ast, Block};

use crate::error::ParseError;

/// `DEFAULT_CONFIG.packet.bitsPerRow`: how many bits `populate` puts on one row.
pub const DEFAULT_BITS_PER_ROW: i64 = 32;

/// Parses the diagram body (see [`crate::preprocess`]).
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    parser::parse(body)
}

#[cfg(test)]
mod tests;
