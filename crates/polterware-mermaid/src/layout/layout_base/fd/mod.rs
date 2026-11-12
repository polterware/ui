//! Port of the `fd/` folder of `layout-base` 1.0.2: the force-directed base that
//! cose-base extends. The files map one to one:
//!
//! - [`fd_layout_constants`] is `FDLayoutConstants.js`;
//! - [`fd_layout_node`] is `FDLayoutNode.js`, the fields `FDLayoutNode` adds to `LNode`;
//! - [`fd_layout_edge`] is `FDLayoutEdge.js`, the field `FDLayoutEdge` adds to `LEdge`;
//! - [`fd_layout`] is `FDLayout.js`, the layout itself.
//!
//! Upstream's classes inherit from [`super::layout`]'s `Layout`, `LNode`, and `LEdge`.
//! This port keeps the base objects in the manager's arenas and the fields the `fd/`
//! classes add in parallel arenas inside [`FDLayout`], indexed by the same [`super::l_node::NodeId`] and
//! [`super::l_edge::EdgeId`]. [`FDLayout::sync_arenas`] keeps the two in step; every mutating entry point
//! of [`FDLayout`] calls it.
//!
//! Upstream's `FDLayout` is a JavaScript constructor with no `layout()` method; the
//! concrete algorithm arrives from a subclass. Where a subclass overrides a method that
//! the base calls (`calcRepulsionRange`), the port applies the override right after the
//! base method, as [`super::layout::Layout`]'s hooks do.

pub mod fd_layout;
pub mod fd_layout_constants;
pub mod fd_layout_edge;
pub mod fd_layout_node;

pub use fd_layout::FDLayout;
pub use fd_layout_edge::FDLayoutEdge;
pub use fd_layout_node::FDLayoutNode;
