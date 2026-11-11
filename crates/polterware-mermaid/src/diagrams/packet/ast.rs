//! The packet database after parsing: what upstream's `PacketDB` getters return (`db.ts`
//! `getPacket`, `getDiagramTitle`, `getAccTitle`, `getAccDescription`), once
//! `packet/parser.ts`'s `populate` has laid the blocks out in rows.
//!
//! Upstream holds `PacketWord[]`, an array of rows, each a `Required<PacketBlock>[]`
//! (`packet/types.ts`); here that is `packet: Vec<Vec<Block>>` with every field set.
//! `getConfig` is left out because this crate owns no configuration.

/// One block of a packet row, every field set (`Required<PacketBlock>` upstream).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Block {
    /// The first bit of the block (inclusive).
    pub start: i64,
    /// The last bit of the block (inclusive).
    pub end: i64,
    /// The block's bit count. `getNextFittingBlock` sets it to the difference of the two ends
    /// without adding one, so a block split across rows carries a count one short of its bits;
    /// the port keeps that, as the upstream snapshots do.
    pub bits: i64,
    /// The block's label.
    pub label: String,
}

/// What upstream's packet database holds after parsing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ast {
    /// `getPacket()`: the rows, in order, each block left to right.
    pub packet: Vec<Vec<Block>>,
    /// `getDiagramTitle()`; empty when the diagram has no title.
    pub diagram_title: String,
    /// `getAccTitle()`; empty when the diagram has no accessible title.
    pub acc_title: String,
    /// `getAccDescription()`; empty when the diagram has no accessible description.
    pub acc_description: String,
}
