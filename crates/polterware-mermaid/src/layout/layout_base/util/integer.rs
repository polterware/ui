//! Port of `layout-base/src/util/Integer.js`.
//!
//! Upstream's Java-style bounds. Layout arithmetic is `f64` (JavaScript numbers), so
//! the constants are `f64` too. Upstream uses them as sentinels: [`MAX_VALUE`] for
//! "no bound yet" and [`MIN_VALUE`] for "size not computed".

/// `Integer.MAX_VALUE`.
pub const MAX_VALUE: f64 = 2_147_483_647.0;

/// `Integer.MIN_VALUE`.
pub const MIN_VALUE: f64 = -2_147_483_648.0;
