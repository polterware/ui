//! Port of `src/treemap/{squarify,binary,dice,slice,sliceDice,resquarify}.js` from
//! `d3-hierarchy` 3.1.2, selected the way upstream's `treemap.tile()` selects a function.
//!
//! Upstream stores a bare tiling function with an optional `ratio` property. Here the
//! choices are variants of [`Tile`]; [`Tile::Squarify`] and [`Tile::Resquarify`] carry the
//! default ratio, and [`Tile::SquarifyRatio`] and [`Tile::ResquarifyRatio`] a custom one,
//! clamped to at least `1` as upstream's `squarify.ratio` and `resquarify.ratio` do. A
//! custom tiling function is not supported: Mermaid's treemap only ever uses the default
//! squarify.
//!
//! Every function receives its parent as a [`NodeId`] and the rectangle to fill; it writes
//! `x0`, `y0`, `x1`, `y1` on the parent's children, exactly as `treemapDice` and
//! `treemapSlice` do, and the row helpers take an explicit child list and value so
//! `squarify` can lay out a row that is not a node.

mod binary;
mod dice;
mod resquarify;
mod slice;
mod slice_dice;
mod squarify;

use super::super::{Hierarchy, NodeId};

pub(crate) use squarify::SquarifyRows;

/// Upstream's `treemap.tile(...)` argument.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tile {
    /// `squarify` with the golden ratio, upstream's default.
    Squarify,
    /// `squarify.ratio(ratio)`. The ratio is clamped to at least `1` when used.
    SquarifyRatio(f64),
    /// `treemapBinary`.
    Binary,
    /// `treemapDice`: horizontal rows.
    Dice,
    /// `treemapSlice`: vertical columns.
    Slice,
    /// `treemapSliceDice`: slices at odd depths and dices at even ones.
    SliceDice,
    /// `resquarify` with the golden ratio.
    Resquarify,
    /// `resquarify.ratio(ratio)`. The ratio is clamped to at least `1` when used.
    ResquarifyRatio(f64),
}

impl Tile {
    /// Upstream's `squarify.ratio(ratio)`: the ratio is at least `1`, and a ratio that is
    /// not greater than `1` (including `NaN`) becomes `1`.
    pub fn squarify_ratio(ratio: f64) -> Self {
        Self::SquarifyRatio(clamp_ratio(ratio))
    }

    /// Upstream's `resquarify.ratio(ratio)`, with the same clamp.
    pub fn resquarify_ratio(ratio: f64) -> Self {
        Self::ResquarifyRatio(clamp_ratio(ratio))
    }

    /// Fills `parent`'s children into the rectangle, the way upstream calls `tile(parent,
    /// x0, y0, x1, y1)`.
    pub(crate) fn apply<T>(
        self,
        hierarchy: &mut Hierarchy<T>,
        parent: NodeId,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
    ) {
        match self {
            Self::Squarify => {
                squarify::squarify(hierarchy, parent, x0, y0, x1, y1, squarify::phi())
            }
            Self::SquarifyRatio(ratio) => {
                squarify::squarify(hierarchy, parent, x0, y0, x1, y1, clamp_ratio(ratio))
            }
            Self::Binary => binary::binary(hierarchy, parent, x0, y0, x1, y1),
            Self::Dice => dice::dice(hierarchy, parent, x0, y0, x1, y1),
            Self::Slice => slice::slice(hierarchy, parent, x0, y0, x1, y1),
            Self::SliceDice => slice_dice::slice_dice(hierarchy, parent, x0, y0, x1, y1),
            Self::Resquarify => {
                resquarify::resquarify(hierarchy, parent, x0, y0, x1, y1, squarify::phi())
            }
            Self::ResquarifyRatio(ratio) => {
                resquarify::resquarify(hierarchy, parent, x0, y0, x1, y1, clamp_ratio(ratio))
            }
        }
    }
}

/// Upstream's `(x = +x) > 1 ? x : 1`.
fn clamp_ratio(ratio: f64) -> f64 {
    if ratio > 1.0 { ratio } else { 1.0 }
}

/// JavaScript's `Math.max`: unlike `f64::max`, a `NaN` operand makes the result `NaN`.
pub(crate) fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.max(b)
    }
}

/// JavaScript truthiness of a node value: `undefined`, `0`, `-0`, and `NaN` are falsy.
pub(crate) fn truthy(value: Option<f64>) -> bool {
    value.is_some_and(|value| value != 0.0 && !value.is_nan())
}

/// Upstream's `value && numerator / value` in the places where the result is used as a
/// number: `undefined` becomes `NaN`, and `0` stays `0` (so children get zero-sized
/// rectangles instead of `NaN`s), exactly as JavaScript's `&&` does.
pub(crate) fn ratio(value: Option<f64>, numerator: f64) -> f64 {
    match value {
        None => f64::NAN,
        Some(value) => {
            if value == 0.0 {
                0.0
            } else {
                numerator / value
            }
        }
    }
}
