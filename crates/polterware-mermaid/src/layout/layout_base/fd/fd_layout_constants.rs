//! Port of `layout-base/src/fd/FDLayoutConstants.js`.
//!
//! `FDLayoutConstants` copies every static of `LayoutConstants` and adds its own, so this
//! module re-exports [`super::super::layout_constants`] and adds the rest.
//!
//! Two properties exist in both: `MIN_EDGE_LENGTH` (the same `1`) and `ANIMATE` (which
//! `LayoutConstants` never defines). Only one definition of each is kept.

pub use super::super::layout_constants::*;

/// Maximum number of spring-embedder iterations.
pub const MAX_ITERATIONS: i64 = 2500;

/// Default ideal length of an edge.
pub const DEFAULT_EDGE_LENGTH: f64 = 50.0;
/// Default spring strength, upsteam's `DEFAULT_SPRING_STRENGTH`.
pub const DEFAULT_SPRING_STRENGTH: f64 = 0.45;
/// Default repulsion strength.
pub const DEFAULT_REPULSION_STRENGTH: f64 = 4500.0;
/// Default gravity strength.
pub const DEFAULT_GRAVITY_STRENGTH: f64 = 0.4;
/// Default gravity strength inside a compound.
pub const DEFAULT_COMPOUND_GRAVITY_STRENGTH: f64 = 1.0;
/// Default multiplier of the graph's estimated size that bounds gravity.
pub const DEFAULT_GRAVITY_RANGE_FACTOR: f64 = 3.8;
/// Default multiplier of the compound's estimated size that bounds gravity.
pub const DEFAULT_COMPOUND_GRAVITY_RANGE_FACTOR: f64 = 1.5;
/// Whether ideal edge lengths account for nesting.
pub const DEFAULT_USE_SMART_IDEAL_EDGE_LENGTH_CALCULATION: bool = true;
/// Whether repulsion uses the FR grid variant.
pub const DEFAULT_USE_SMART_REPULSION_RANGE_CALCULATION: bool = true;
/// Cooling factor of an incremental run.
pub const DEFAULT_COOLING_FACTOR_INCREMENTAL: f64 = 0.3;
/// Cooling adaptation factor for large graphs.
pub const COOLING_ADAPTATION_FACTOR: f64 = 0.33;
/// Node count above which the cooling factor adapts.
pub const ADAPTATION_LOWER_NODE_LIMIT: f64 = 1000.0;
/// Node count at which the cooling factor reaches its adapted value.
pub const ADAPTATION_UPPER_NODE_LIMIT: f64 = 5000.0;
/// Maximum displacement of a node per iteration, incremental runs.
pub const MAX_NODE_DISPLACEMENT_INCREMENTAL: f64 = 100.0;
/// Maximum displacement of a node per iteration, from scratch.
pub const MAX_NODE_DISPLACEMENT: f64 = MAX_NODE_DISPLACEMENT_INCREMENTAL * 3.0;
/// Distances below this are pushed away from zero.
pub const MIN_REPULSION_DIST: f64 = DEFAULT_EDGE_LENGTH / 10.0;
/// Iterations between two convergence checks.
pub const CONVERGENCE_CHECK_PERIOD: i64 = 100;
/// Ideal edge length grows by this factor for each level of nesting.
pub const PER_LEVEL_IDEAL_EDGE_LENGTH_FACTOR: f64 = 0.1;
/// Iterations between two grid updates.
pub const GRID_CALCULATION_CHECK_PERIOD: i64 = 10;
