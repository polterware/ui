//! Port of `d3-sankey` 0.12.3 (MIT, Copyright 2016 Mike Bostock), the layout
//! Mermaid 12.3.0's sankey renderer calls: `src/` in
//! `research/mermaid-port/upstream/mermaid/node_modules/.pnpm/d3-sankey@0.12.3`
//! (`sankey.js`, `align.js`, `sankeyLinkHorizontal.js`, `constant.js`).
//!
//! The files map one to one: [`generator`] holds the generator and its pipeline
//! (`computeNodeLinks` through `computeLinkBreadths`), [`align`] the four
//! `nodeAlign` functions, [`link_horizontal`] the link path over d3-shape 1.3.7's
//! `link()` and d3-path 1.0.9's `Path`, and [`constant`] upstream's helper.
//! [`number`] formats numbers the way JavaScript's `String(number)` does, because
//! d3-path builds the `d` attribute by concatenating numbers.
//!
//! The API is typed where upstream's is plain JavaScript objects: nodes are
//! [`SankeyNodeInput`] (id, optional `fixedValue`), links are [`SankeyLinkInput`]
//! (source id, target id, value), and [`Sankey::layout`] returns a [`SankeyGraph`]
//! with upstream's fields (`x0`, `x1`, `y0`, `y1`, `value`, `depth`, `height`,
//! `layer`; links' `y0`, `y1`, `width`) plus the link path of
//! `sankeyLinkHorizontal`. Link endpoints are node indices in [`SankeyLink`], and
//! every array keeps upstream's iteration order.
//!
//! Not ported, because no caller reaches them: `nodeId` (nodes always carry their
//! id), `nodeSort` and `linkSort` (Mermaid sets neither, so both stay `undefined`
//! and every sort is `ascendingBreadth` by link index), `update` (recomputing link
//! breadths only matters to callers that edit a laid-out graph, which the renderer
//! does not), and the `sankey.size`/`extent` getters (`extent` is a field).
//!
//! What upstream throws, layout returns as [`SankeyError`]: a missing node id, a
//! cycle, and an empty graph (upstream then builds `new Array(NaN)`). A graph with
//! a single layer has `(x - 1) = 0` in `kx`, so upstream writes `NaN` into
//! `x0`/`x1` and this port does the same; the fixtures carry it as `null`, as
//! `JSON.stringify` does.
//!
//! Differential fixtures generated with Node against the copy Mermaid depends on
//! live in `testdata/` and are compared by `differential.rs` within `1e-6`, link
//! paths included.

pub mod align;
pub mod constant;
#[cfg(test)]
mod differential;
pub mod generator;
pub mod link_horizontal;
pub mod number;

pub use generator::{
    Sankey, SankeyAlignment, SankeyError, SankeyGraph, SankeyLink, SankeyLinkInput, SankeyNode,
    SankeyNodeInput,
};
