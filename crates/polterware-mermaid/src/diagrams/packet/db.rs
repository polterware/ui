//! The packet database's writing side, ported from `packet/parser.ts`'s `populate` and
//! `getNextFittingBlock`, `db.ts`'s `pushWord`, and `diagrams/common/populateCommonDb.ts`.
//!
//! `packetParser.ts` parses the whole document first and then populates the database, so the
//! order of the writes matches `populate`: the accessibility fields and the title, then each
//! block in grammar order. Blocks are laid out left to right and a block longer than the rest of
//! its row is split at the row boundary, with the remainder continuing on the next row.

use super::DEFAULT_BITS_PER_ROW;
use super::ast::{Ast, Block};
use super::parser::ParsedPacket;
use super::sanitize;
use crate::error::ParseError;

/// `maxPacketSize`: `populate` stops pushing rows at this count, so hostile input cannot make
/// the database grow without bound.
const MAX_PACKET_SIZE: usize = 10_000;

/// `populate`: applies the parsed document to a fresh database.
pub(crate) fn build(parsed: ParsedPacket) -> Result<Ast, ParseError> {
    let mut ast = Ast::default();
    if let Some(value) = parsed.acc_description.filter(|value| !value.is_empty()) {
        ast.acc_description = sanitize::set_acc_description(&value);
    }
    if let Some(value) = parsed.acc_title.filter(|value| !value.is_empty()) {
        ast.acc_title = sanitize::set_acc_title(&value);
    }
    if let Some(value) = parsed.title.filter(|value| !value.is_empty()) {
        ast.diagram_title = sanitize::sanitize_text(&value);
    }

    let mut last_bit: i64 = -1;
    let mut word: Vec<Block> = Vec::new();
    let mut row: i64 = 1;
    for block in parsed.blocks {
        if let (Some(start), Some(end)) = (block.start, block.end)
            && end < start
        {
            return Err(ParseError::at(
                format!("Packet block {start} - {end} is invalid. End must be greater than start."),
                block.span,
            ));
        }
        let next_bit = last_bit.saturating_add(1);
        let start = block.start.unwrap_or(next_bit);
        if start != next_bit {
            return Err(ParseError::at(
                format!(
                    "Packet block {start} - {} is not contiguous. It should start from {next_bit}.",
                    block.end.unwrap_or(start)
                ),
                block.span,
            ));
        }
        if block.bits == Some(0) {
            return Err(ParseError::at(
                format!("Packet block {start} is invalid. Cannot have a zero bit field."),
                block.span,
            ));
        }
        let end = block.end.unwrap_or_else(|| {
            start
                .saturating_add(block.bits.unwrap_or(1))
                .saturating_sub(1)
        });
        let bits = block
            .bits
            .unwrap_or_else(|| end.saturating_sub(start).saturating_add(1));
        last_bit = end;

        let mut fitting = Block {
            start,
            end,
            bits,
            label: block.label,
        };
        while word.len() as i64 <= DEFAULT_BITS_PER_ROW + 1 && ast.packet.len() < MAX_PACKET_SIZE {
            let (first, next) = get_next_fitting_block(&fitting, row);
            let ends_row = first.end.saturating_add(1) == row.saturating_mul(DEFAULT_BITS_PER_ROW);
            word.push(first);
            if ends_row {
                ast.packet.push(std::mem::take(&mut word));
                row = row.saturating_add(1);
            }
            match next {
                Some(tail) => fitting = tail,
                None => break,
            }
        }
    }
    push_word(&mut ast, word);
    Ok(ast)
}

/// `db.pushWord`: an empty word is not a row.
fn push_word(ast: &mut Ast, word: Vec<Block>) {
    if !word.is_empty() {
        ast.packet.push(word);
    }
}

/// `getNextFittingBlock`: the part of `block` that fits in `row`, and the remainder to continue
/// with, or `None` when the whole block fits.
///
/// Upstream's two "should have been set during first phase" throws are unrepresentable here:
/// [`build`] sets `start` and `end` before the first call, and a remainder always starts where
/// the part before it ended. Its `start > end` throw is unreachable for the same reason, and its
/// `bits` fields keep the missing `+ 1` (see [`Block::bits`]).
fn get_next_fitting_block(block: &Block, row: i64) -> (Block, Option<Block>) {
    if block.end.saturating_add(1) <= row.saturating_mul(DEFAULT_BITS_PER_ROW) {
        return (block.clone(), None);
    }
    let row_end = row.saturating_mul(DEFAULT_BITS_PER_ROW).saturating_sub(1);
    let row_start = row.saturating_mul(DEFAULT_BITS_PER_ROW);
    (
        Block {
            start: block.start,
            end: row_end,
            bits: row_end.saturating_sub(block.start),
            label: block.label.clone(),
        },
        Some(Block {
            start: row_start,
            end: block.end,
            bits: block.end.saturating_sub(row_start),
            label: block.label.clone(),
        }),
    )
}
