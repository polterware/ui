//! Port of `cose-base/src/CoSEConstants.js`.
//!
//! `CoSEConstants` copies every static of `FDLayoutConstants` and adds its own, so this
//! module re-exports [`super::super::layout_base::fd::fd_layout_constants`] and adds the
//! rest.
//!
//! cose-bilkent mutates several of these constants per run (`DEFAULT_REPULSION_STRENGTH`,
//! `DEFAULT_EDGE_LENGTH`, the paddings, ...). A Rust library cannot own process-wide
//! mutable constants, so the port keeps them here and copies the ones a run may change
//! into the layout fields of the same name, which a caller sets before `run_layout`.

pub use super::super::layout_base::fd::fd_layout_constants::*;

/// Whether the layout runs multi-level scaling, which this port does not implement.
pub const DEFAULT_USE_MULTI_LEVEL_SCALING: bool = false;

/// Default radial separation of the trees of a forest layout.
pub const DEFAULT_RADIAL_SEPARATION: f64 = DEFAULT_EDGE_LENGTH;

/// Space between two trees of a forest layout.
pub const DEFAULT_COMPONENT_SEPERATION: f64 = 60.0;

/// Whether zero-degree members and compound members are tiled.
pub const TILE: bool = true;

/// Vertical space between two rows of tiled nodes.
pub const TILING_PADDING_VERTICAL: f64 = 10.0;

/// Horizontal space between two nodes of a tiled row.
pub const TILING_PADDING_HORIZONTAL: f64 = 10.0;

/// Whether tree reduction runs in incremental mode.
pub const TREE_REDUCTION_ON_INCREMENTAL: bool = false;

// Constants of cose-base 2.2.0 (the base of cytoscape-fcose 2.2.0). The v1 values above
// stay as they are so cose-bilkent keeps its results.

/// Whether tree reduction runs in incremental mode in 2.2.0. Its value is `DEFAULT_INCREMENTAL`, which is `false`.
pub const TREE_REDUCTION_ON_INCREMENTAL_V2: bool = true;

/// Whether the constraint handler transforms the draft layout (Procrustes) before
/// enforcing the constraints.
pub const TRANSFORM_ON_CONSTRAINT_HANDLING: bool = true;

/// Whether the constraint handler enforces fixed node, alignment, and relative placement
/// constraints on the draft layout.
pub const ENFORCE_CONSTRAINTS: bool = true;

/// Whether the layout runs the spring embedder. `false` applies constraints only, which
/// cytoscape-fcose uses for the incremental layout.
pub const APPLY_LAYOUT: bool = true;

/// Whether constrained movement is relaxed by shuffling the constrained nodes instead of
/// averaging the components of the relative placement graph.
pub const RELAX_MOVEMENT_ON_CONSTRAINTS: bool = true;

/// Whether the layout is a pure incremental layout on top of the previous positions
/// rather than a randomized one. 2.2.0 sets it to `DEFAULT_INCREMENTAL`, which is
/// `false`.
pub const PURE_INCREMENTAL: bool = false;
