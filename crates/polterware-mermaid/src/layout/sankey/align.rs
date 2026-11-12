//! Port of `d3-sankey` 0.12.3 `src/align.js`, the four `nodeAlign` functions.
//!
//! Each returns the column a node belongs to as a JavaScript number; the caller
//! floors and clamps it, as `computeNodeLayers` does.

use super::generator::{Graph, Node};

/// `sankeyLeft`: the node's depth.
pub(crate) fn left(node: &Node) -> f64 {
    node.depth
}

/// `sankeyRight`: `n - 1 - node.height`.
pub(crate) fn right(node: &Node, n: f64) -> f64 {
    n - 1.0 - node.height
}

/// `sankeyJustify`: a source node goes to the last column, the rest keep their
/// depth.
pub(crate) fn justify(node: &Node, n: f64) -> f64 {
    if !node.source_links.is_empty() {
        node.depth
    } else {
        n - 1.0
    }
}

/// `sankeyCenter`: a node with incoming links keeps its depth; a node with only
/// outgoing links sits one column before its first target; an isolated node stays
/// at zero.
pub(crate) fn center(graph: &Graph, node: &Node) -> f64 {
    if !node.target_links.is_empty() {
        return node.depth;
    }
    if !node.source_links.is_empty() {
        // `min(node.sourceLinks, targetDepth)`.
        let mut min: Option<f64> = None;
        for &link in &node.source_links {
            let depth = graph.nodes[graph.links[link].target].depth;
            if depth.is_nan() {
                continue;
            }
            if min.is_none_or(|min| min > depth) {
                min = Some(depth);
            }
        }
        return min.unwrap_or(f64::NAN) - 1.0;
    }
    0.0
}
