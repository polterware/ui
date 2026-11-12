//! Port of `src/treemap/sliceDice.js` from `d3-hierarchy` 3.1.2: slices at odd depths and
//! dices at even ones, so the grid alternates between columns and rows.

use super::{dice, slice};
use crate::layout::hierarchy::{Hierarchy, NodeId};

/// `treemapSliceDice(parent, x0, y0, x1, y1)`: `(parent.depth & 1 ? slice : dice)`.
pub(crate) fn slice_dice<T>(
    hierarchy: &mut Hierarchy<T>,
    parent: NodeId,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
) {
    if hierarchy.node(parent).depth & 1 == 1 {
        slice::slice(hierarchy, parent, x0, y0, x1, y1);
    } else {
        dice::dice(hierarchy, parent, x0, y0, x1, y1);
    }
}
