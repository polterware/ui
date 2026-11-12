//! Port of `src/treemap/binary.js` from `d3-hierarchy` 3.1.2: the squarified "binary"
//! tiling that splits the rectangle in half by value, using a binary search over the
//! children's cumulative sums.
//!
//! Upstream throws when it is handed a parent with an empty child list (`nodes[0]` of
//! nothing); here the empty partition writes nothing. The children accessor cannot produce
//! an empty list, so the case only arises if a node's `children` is edited by hand.

use super::truthy;
use crate::layout::hierarchy::{Hierarchy, NodeId};

/// `treemapBinary(parent, x0, y0, x1, y1)`.
pub(crate) fn binary<T>(
    hierarchy: &mut Hierarchy<T>,
    parent: NodeId,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
) {
    let (nodes, value) = {
        let node = hierarchy.node(parent);
        (
            node.children.clone().unwrap_or_default(),
            node.value.unwrap_or(f64::NAN),
        )
    };

    let n = nodes.len();
    let mut sums = vec![0.0; n + 1];
    let mut sum = 0.0;
    for i in 0..n {
        sum += hierarchy.node(nodes[i]).value.unwrap_or(f64::NAN);
        sums[i + 1] = sum;
    }

    partition(hierarchy, &nodes, &sums, 0, n, value, x0, y0, x1, y1);
}

/// Upstream's `partition`, which is recursive and splits `[i, j)` by value.
#[allow(clippy::too_many_arguments)]
fn partition<T>(
    hierarchy: &mut Hierarchy<T>,
    nodes: &[NodeId],
    sums: &[f64],
    i: usize,
    j: usize,
    value: f64,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
) {
    if i + 1 >= j {
        let Some(&node_id) = nodes.get(i) else {
            return;
        };
        let node = hierarchy.node_mut(node_id);
        node.x0 = x0;
        node.y0 = y0;
        node.x1 = x1;
        node.y1 = y1;
        return;
    }

    let value_offset = sums[i];
    let value_target = value / 2.0 + value_offset;
    let mut k = i + 1;
    let mut hi = j - 1;

    while k < hi {
        let mid = (k + hi) >> 1;
        if sums[mid] < value_target {
            k = mid + 1;
        } else {
            hi = mid;
        }
    }

    if value_target - sums[k - 1] < sums[k] - value_target && i + 1 < k {
        k -= 1;
    }

    let value_left = sums[k] - value_offset;
    let value_right = value - value_left;

    if x1 - x0 > y1 - y0 {
        let xk = if truthy(Some(value)) {
            (x0 * value_right + x1 * value_left) / value
        } else {
            x1
        };
        partition(hierarchy, nodes, sums, i, k, value_left, x0, y0, xk, y1);
        partition(hierarchy, nodes, sums, k, j, value_right, xk, y0, x1, y1);
    } else {
        let yk = if truthy(Some(value)) {
            (y0 * value_right + y1 * value_left) / value
        } else {
            y1
        };
        partition(hierarchy, nodes, sums, i, k, value_left, x0, y0, x1, yk);
        partition(hierarchy, nodes, sums, k, j, value_right, x0, yk, x1, y1);
    }
}
