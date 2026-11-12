//! Port of `cose-base/src/CoSEGraphManager.js`.
//!
//! Upstream's `CoSEGraphManager` extends `LGraphManager` and adds nothing but a
//! back-reference to its layout, which the port's manager does not need: the arenas live
//! in the manager and the layout holds the manager.

pub use super::super::layout_base::l_graph_manager::LGraphManager;

/// Upstream's `CoSEGraphManager`, an [`LGraphManager`] without extra fields.
pub type CoSEGraphManager<V> = LGraphManager<V>;
