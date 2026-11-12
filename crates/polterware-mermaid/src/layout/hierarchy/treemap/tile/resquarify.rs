//! Port of `src/treemap/resquarify.js` from `d3-hierarchy` 3.1.2: squarify once, then keep
//! the rows across layouts and only rescale them, so a live treemap's cells keep their
//! positions when values change.
//!
//! Upstream keeps the rows on the parent node as `_squarify`; this port keeps them in
//! [`crate::layout::hierarchy::Node`]'s `squarify` field, which [`Tile`](super::Tile) can
//! reuse on the next [`crate::layout::hierarchy::Treemap::layout`]. Rows are reused only
//! when the stored ratio is exactly the requested one, as upstream's `rows.ratio === ratio`.

use super::squarify::{SquarifyRows, squarify_ratio};
use super::{dice, slice, truthy};
use crate::layout::hierarchy::{Hierarchy, NodeId};

/// `resquarify(parent, x0, y0, x1, y1)`.
pub(crate) fn resquarify<T>(
    hierarchy: &mut Hierarchy<T>,
    parent: NodeId,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    ratio: f64,
) {
    let rows = hierarchy
        .node(parent)
        .squarify
        .clone()
        .filter(|rows| rows.ratio == ratio);

    let Some(rows) = rows else {
        let rows = squarify_ratio(hierarchy, parent, x0, y0, x1, y1, ratio);
        hierarchy.node_mut(parent).squarify = Some(rows);
        return;
    };

    let SquarifyRows {
        ratio: stored_ratio,
        rows,
    } = rows;
    let mut value = hierarchy.node(parent).value.unwrap_or(f64::NAN);
    let mut x0 = x0;
    let mut y0 = y0;
    let mut updated = Vec::with_capacity(rows.len());

    for mut row in rows {
        let mut row_value = 0.0;
        for &child in &row.children {
            row_value += hierarchy.node(child).value.unwrap_or(f64::NAN);
        }
        row.value = row_value;

        if row.dice {
            let next_y0 = if truthy(Some(value)) {
                y0 + (y1 - y0) * row.value / value
            } else {
                y1
            };
            dice::dice_children(
                hierarchy,
                &row.children,
                Some(row.value),
                x0,
                y0,
                x1,
                next_y0,
            );
            y0 = next_y0;
        } else {
            let next_x0 = if truthy(Some(value)) {
                x0 + (x1 - x0) * row.value / value
            } else {
                x1
            };
            slice::slice_children(
                hierarchy,
                &row.children,
                Some(row.value),
                x0,
                y0,
                next_x0,
                y1,
            );
            x0 = next_x0;
        }

        value -= row_value;
        updated.push(row);
    }

    hierarchy.node_mut(parent).squarify = Some(SquarifyRows {
        ratio: stored_ratio,
        rows: updated,
    });
}
