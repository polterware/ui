//! Port of `src/treemap/squarify.js` from `d3-hierarchy` 3.1.2, the default tiling: it
//! packs children into rows whose cells stay as square as possible, choosing the orientation
//! that makes the worst aspect ratio smallest.
//!
//! `squarifyRatio` returns the rows it laid out (upstream's `rows` array); [`Tile::Resquarify`]
//! stores them on the parent and reuses them on the next layout. [`squarify`] is upstream's
//! default export: same work, rows dropped.

use super::{dice, js_max, slice, truthy};
use crate::layout::hierarchy::{Hierarchy, NodeId};

/// Upstream's `phi`, the golden ratio, the default squarify ratio.
pub(crate) fn phi() -> f64 {
    (1.0 + 5.0_f64.sqrt()) / 2.0
}

/// One row of upstream's `rows` array: the children it holds and the band it occupies.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SquarifyRow {
    /// Upstream's `row.value`, the children's summed value (recomputed by resquarify).
    pub value: f64,
    /// Upstream's `row.dice`: `true` when the row is a horizontal band.
    pub dice: bool,
    /// Upstream's `row.children`, as node ids.
    pub children: Vec<NodeId>,
}

/// Upstream's `rows` array, and the ratio it was built for (resquarify's `rows.ratio`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SquarifyRows {
    pub(crate) ratio: f64,
    pub(crate) rows: Vec<SquarifyRow>,
}

/// Upstream's default `squarify(parent, x0, y0, x1, y1)`.
pub(crate) fn squarify<T>(
    hierarchy: &mut Hierarchy<T>,
    parent: NodeId,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    ratio: f64,
) {
    squarify_ratio(hierarchy, parent, x0, y0, x1, y1, ratio);
}

/// Upstream's `squarifyRatio(ratio, parent, x0, y0, x1, y1)`.
pub(crate) fn squarify_ratio<T>(
    hierarchy: &mut Hierarchy<T>,
    parent: NodeId,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    ratio: f64,
) -> SquarifyRows {
    let nodes = hierarchy.node(parent).children.clone().unwrap_or_default();
    let n = nodes.len();
    let mut value = hierarchy.node(parent).value.unwrap_or(f64::NAN);
    let mut rows = Vec::new();
    let mut x0 = x0;
    let mut y0 = y0;
    let mut i0 = 0;
    let mut i1 = 0;

    while i0 < n {
        let dx = x1 - x0;
        let dy = y1 - y0;

        // Find the next non-empty node: `do sumValue = nodes[i1++].value; while (!sumValue
        // && i1 < n);`.
        let mut sum_value;
        loop {
            sum_value = hierarchy.node(nodes[i1]).value.unwrap_or(f64::NAN);
            i1 += 1;
            if truthy(Some(sum_value)) || i1 >= n {
                break;
            }
        }
        let mut min_value = sum_value;
        let mut max_value = sum_value;
        let alpha = js_max(dy / dx, dx / dy) / (value * ratio);
        let mut beta = sum_value * sum_value * alpha;
        let mut min_ratio = js_max(max_value / beta, beta / min_value);

        // Keep adding nodes while the aspect ratio maintains or improves.
        while i1 < n {
            let node_value = hierarchy.node(nodes[i1]).value.unwrap_or(f64::NAN);
            sum_value += node_value;
            if node_value < min_value {
                min_value = node_value;
            }
            if node_value > max_value {
                max_value = node_value;
            }
            beta = sum_value * sum_value * alpha;
            let new_ratio = js_max(max_value / beta, beta / min_value);
            if new_ratio > min_ratio {
                sum_value -= node_value;
                break;
            }
            min_ratio = new_ratio;
            i1 += 1;
        }

        let row_children = nodes[i0..i1].to_vec();
        let dice_row = dx < dy;
        if dice_row {
            let next_y0 = if truthy(Some(value)) {
                y0 + dy * sum_value / value
            } else {
                y1
            };
            dice::dice_children(
                hierarchy,
                &row_children,
                Some(sum_value),
                x0,
                y0,
                x1,
                next_y0,
            );
            y0 = next_y0;
        } else {
            let next_x0 = if truthy(Some(value)) {
                x0 + dx * sum_value / value
            } else {
                x1
            };
            slice::slice_children(
                hierarchy,
                &row_children,
                Some(sum_value),
                x0,
                y0,
                next_x0,
                y1,
            );
            x0 = next_x0;
        }

        rows.push(SquarifyRow {
            value: sum_value,
            dice: dice_row,
            children: row_children,
        });
        value -= sum_value;
        i0 = i1;
    }

    SquarifyRows { ratio, rows }
}
