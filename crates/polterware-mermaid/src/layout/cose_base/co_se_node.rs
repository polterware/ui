//! Port of `cose-base/src/CoSENode.js`.
//!
//! Upstream's `CoSENode` extends `FDLayoutNode` and adds no fields: its `move` is the only
//! behavior of the class, and it reads the layout's cooling factor, maximum displacement,
//! and total displacement, plus the node's child graph. Since the only caller is
//! `FDLayout.prototype.moveNodes`, which layout-base cannot point at cose-base, the port
//! keeps that method as
//! [`FDLayout::node_move`](super::super::layout_base::fd::fd_layout::FDLayout::node_move) and
//! the child propagation as
//! [`FDLayout::propagate_displacement_to_children`](super::super::layout_base::fd::fd_layout::FDLayout::propagate_displacement_to_children)
//! in `fd_layout.rs`.
//!
//! The `pred`/`next`/`processed` accessors upstream are not ported: they read globals that
//! are never declared (`pred1`, `pred2`, `next`, `processed`), so every one of them throws
//! a `ReferenceError`, and nothing calls them.

pub use super::super::layout_base::fd::fd_layout_node::FDLayoutNode;

/// Upstream's `CoSENode`, a [`FDLayoutNode`] without extra fields.
pub type CoSENode = FDLayoutNode;
