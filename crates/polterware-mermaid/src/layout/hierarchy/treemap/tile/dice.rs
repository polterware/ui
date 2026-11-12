//! Port of `src/treemap/dice.js` from `d3-hierarchy` 3.1.2: children are stacked as
//! horizontal rows, top to bottom.

use super::ratio;
use crate::layout::hierarchy::{Hierarchy, NodeId};

/// `treemapDice(parent, x0, y0, x1, y1)`.
pub(crate) fn dice<T>(
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
    dice_children(hierarchy, &children, value, x0, y0, x1, y1);
}

/// `treemapDice(row, ...)` when `row` is a squarify row rather than a node: the same
/// arithmetic over an explicit child list and value.
pub(crate) fn dice_children<T>(
    hierarchy: &mut Hierarchy<T>,
    children: &[NodeId],
    value: Option<f64>,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
) {
    let k = ratio(value, x1 - x0);
    let mut x0 = x0;
    for &child in children {
        let x1 = x0 + hierarchy.node(child).value.unwrap_or(f64::NAN) * k;
        let node = hierarchy.node_mut(child);
        node.y0 = y0;
        node.y1 = y1;
        node.x0 = x0;
        node.x1 = x1;
        x0 = x1;
    }
}
