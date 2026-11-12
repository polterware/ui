//! Port of `d3-sankey` 0.12.3 `src/sankeyLinkHorizontal.js`: a horizontal
//! `linkHorizontal()` from d3-shape 1.3.7, whose source is `[source.x1, link.y0]`
//! and target is `[target.x0, link.y1]`.
//!
//! d3-shape draws it with `curveHorizontal` (d3-shape 1.3.7 `src/link/index.js`):
//! a move to the source, then one cubic Bézier whose control points share
//! `(x0 + x1) / 2`. d3-path 1.0.9 concatenates the numbers, so the result is an SVG
//! path string with JavaScript number formatting.

use super::number::format_number;
use super::{SankeyGraph, SankeyLink};

/// The `d` attribute of a link.
pub(crate) fn link_path(graph: &SankeyGraph, link: &SankeyLink) -> String {
    let source = &graph.nodes[link.source];
    let target = &graph.nodes[link.target];
    let x0 = source.x1;
    let y0 = link.y0;
    let x1 = target.x0;
    let y1 = link.y1;
    let middle = (x0 + x1) / 2.0;
    let number = format_number;
    format!(
        "M{},{}C{},{},{},{},{},{}",
        number(x0),
        number(y0),
        number(middle),
        number(y0),
        number(middle),
        number(y1),
        number(x1),
        number(y1)
    )
}
