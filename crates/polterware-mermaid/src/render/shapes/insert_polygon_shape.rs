//! Upstream `shapes/insertPolygonShape.ts`.
use crate::scene::Point;

/// Returns the points used by upstream's first-child polygon and its SVG translation.
pub fn polygon(points: &[Point], width: f64, height: f64) -> (Vec<Point>, Point) {
    (points.to_vec(), Point::new(-width / 2.0, height / 2.0))
}

use super::{Node, RenderedNode, ShapeContext, ShapeError};

// Compatibility entry for the current registry; this upstream module is a helper, not a shape.
pub fn draw(node: &Node, _ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    Err(ShapeError {
        message: format!("{} is a rendering helper, not a shape", node.shape),
    })
}
