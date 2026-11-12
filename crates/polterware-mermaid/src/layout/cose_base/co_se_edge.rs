//! Port of `cose-base/src/CoSEEdge.js`.
//!
//! Upstream's `CoSEEdge` extends `FDLayoutEdge` and adds nothing. The port keeps
//! [`FDLayoutEdge`] and names it here; an edge's nodes live in the graph manager's arena
//! and its ideal length in [`super::super::layout_base::fd::FDLayout`].

pub use super::super::layout_base::fd::fd_layout_edge::FDLayoutEdge;

/// Upstream's `CoSEEdge`, a [`FDLayoutEdge`] without extra fields.
pub type CoSEEdge = FDLayoutEdge;
