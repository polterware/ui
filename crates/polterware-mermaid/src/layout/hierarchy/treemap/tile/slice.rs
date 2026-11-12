//! Port of `src/treemap/slice.js` from `d3-hierarchy` 3.1.2: children are stacked as
//! vertical columns, left to right.

use super::ratio;
use crate::layout::hierarchy::{Hierarchy, NodeId};

/// `treemapSlice(parent, x0, y0, x1, y1)`.
pub(crate) fn slice<T>(
    hierarchy: &mut Hierarchy<T>,
    parent: NodeId,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
) {
    let (children, value) = {
        let node = hierarchy.node(parent);
        (node.children.clone().unwrap_or_default(), node.value)
    };
    slice_children(hierarchy, &children, value, x0, y0, x1, y1);
}

/// `treemapSlice(row, ...)` when `row` is a squarify row rather than a node.
pub(crate) fn slice_children<T>(
    hierarchy: &mut Hierarchy<T>,
    children: &[NodeId],
    value: Option<f64>,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
) {
    let k = ratio(value, y1 - y0);
    let mut y0 = y0;
    for &child in children {
        let y1 = y0 + hierarchy.node(child).value.unwrap_or(f64::NAN) * k;
        let node = hierarchy.node_mut(child);
        node.x0 = x0;
        node.x1 = x1;
        node.y0 = y0;
        node.y1 = y1;
        y0 = y1;
    }
}
