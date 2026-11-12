//! Port of `layout-base/src/fd/FDLayoutEdge.js`.
//!
//! Upstream's `FDLayoutEdge` extends `LEdge` with the ideal length the spring of this edge
//! pulls towards. As with [`super::fd_layout_node`], the `LEdge` half lives in the
//! manager's arena and this struct holds the extra field, in a parallel arena indexed by
//! the same [`super::super::l_edge::EdgeId`].

use super::fd_layout_constants as constants;

/// The force-directed half of an edge, upstream's `FDLayoutEdge`.
#[derive(Clone, Debug, PartialEq)]
pub struct FDLayoutEdge {
    /// The length the spring pulls towards, upstream's `idealLength`.
    pub ideal_length: f64,
}

impl Default for FDLayoutEdge {
    fn default() -> Self {
        Self {
            ideal_length: constants::DEFAULT_EDGE_LENGTH,
        }
    }
}

impl FDLayoutEdge {
    /// An edge with the default ideal length, upstream's constructor.
    pub fn new() -> Self {
        Self::default()
    }
}
