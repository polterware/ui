//! Port of `cose-base/src/CoSEGraph.js`.
//!
//! Upstream's `CoSEGraph` extends `LGraph` and adds nothing. The port keeps [`LGraph`],
//! whose manager owns the arena, and names it here.

pub use super::super::layout_base::l_graph::LGraph;

/// Upstream's `CoSEGraph`, an [`LGraph`] without extra fields.
pub type CoSEGraph<V> = LGraph<V>;
