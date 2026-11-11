//! Upstream `shapes/roundedRectPath.ts`.

/// Serializes the classic SVG rounded rectangle path using upstream's command order.
pub fn create_rounded_rect_path_d(x: f64, y: f64, width: f64, height: f64, radius: f64) -> String {
    format!(
        "M {} {} H {} A {} {} 0 0 1 {} {} V {} A {} {} 0 0 1 {} {} H {} A {} {} 0 0 1 {} {} V {} A {} {} 0 0 1 {} {} Z",
        x + radius,
        y,
        x + width - radius,
        radius,
        radius,
        x + width,
        y + radius,
        y + height - radius,
        radius,
        radius,
        x + width - radius,
        y + height,
        x + radius,
        radius,
        radius,
        x,
        y + height - radius,
        y + radius,
        radius,
        radius,
        x + radius,
        y
    )
}

use super::{Node, RenderedNode, ShapeContext, ShapeError};

// Compatibility entry for the current registry; this upstream module is a helper, not a shape.
pub fn draw(node: &Node, _ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    Err(ShapeError {
        message: format!("{} is a rendering helper, not a shape", node.shape),
    })
}
