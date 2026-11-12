//! Port of `layout-base/src/LGraphObject.js`.
//!
//! The base of [`super::l_node::LNode`], [`super::l_edge::LEdge`], and
//! [`super::l_graph::LGraph`]: the v-level object an l-level object stands for. Nothing
//! in layout-base reads it; [`super::layout::Layout::update`] hands it back to the
//! caller, upstream's `Updatable` callback.

/// Upstream's `LGraphObject`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LGraphObject<V> {
    /// The v-level object this l-level object stands for, upstream's `vGraphObject`.
    pub v_graph_object: Option<V>,
}

impl<V> Default for LGraphObject<V> {
    fn default() -> Self {
        Self {
            v_graph_object: None,
        }
    }
}

impl<V> LGraphObject<V> {
    /// Wraps a v-level object.
    pub fn new(v_graph_object: Option<V>) -> Self {
        Self { v_graph_object }
    }
}
