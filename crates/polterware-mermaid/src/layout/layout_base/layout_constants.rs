//! Port of `layout-base/src/LayoutConstants.js`.
//!
//! Upstream's static constants. `LayoutConstants.ANIMATE` is referenced by
//! `Layout.runLayout` but never defined, so the `'during'` branch there never runs; the
//! port does not define it either.

/// Layout quality: 0 draft, 1 default, 2 proof.
pub const QUALITY: i32 = 1;

/// Whether layout creates bendpoints as needed. Defaults to `false`.
pub const DEFAULT_CREATE_BENDS_AS_NEEDED: bool = false;
/// Whether layout is incremental. Defaults to `false`.
pub const DEFAULT_INCREMENTAL: bool = false;
/// Whether the caller animates from the before to the after positions. Defaults to `true`.
pub const DEFAULT_ANIMATION_ON_LAYOUT: bool = true;
/// Whether the layout itself is animated. Defaults to `false`.
pub const DEFAULT_ANIMATION_DURING_LAYOUT: bool = false;
/// Iterations between two successive animations. Defaults to `50`.
pub const DEFAULT_ANIMATION_PERIOD: f64 = 50.0;
/// Whether leaf nodes have uniform sizes. Defaults to `false`.
pub const DEFAULT_UNIFORM_LEAF_NODE_SIZES: bool = false;

/// Margin a graph applies to the bounding rectangle of its contents, on all four sides.
pub const DEFAULT_GRAPH_MARGIN: f64 = 15.0;

/// Whether node dimensions include labels.
pub const NODE_DIMENSIONS_INCLUDE_LABELS: bool = false;

/// Default dimension of a non-compound node.
pub const SIMPLE_NODE_SIZE: f64 = 40.0;

/// Half of [`SIMPLE_NODE_SIZE`].
pub const SIMPLE_NODE_HALF_SIZE: f64 = SIMPLE_NODE_SIZE / 2.0;

/// Dimension of an empty compound node.
pub const EMPTY_COMPOUND_NODE_SIZE: f64 = 40.0;

/// Minimum length an edge takes during layout.
pub const MIN_EDGE_LENGTH: f64 = 1.0;

/// World boundary layout operates on.
pub const WORLD_BOUNDARY: f64 = 1_000_000.0;

/// World boundary random positioning uses.
pub const INITIAL_WORLD_BOUNDARY: f64 = WORLD_BOUNDARY / 1000.0;

/// X coordinate of the world center.
pub const WORLD_CENTER_X: f64 = 1200.0;

/// Y coordinate of the world center.
pub const WORLD_CENTER_Y: f64 = 900.0;
