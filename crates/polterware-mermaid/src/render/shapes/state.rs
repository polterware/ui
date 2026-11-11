//! Placeholder for upstream `shapes/state.ts`.
use super::{Node, RenderedNode, ShapeContext, ShapeError};

pub fn draw(node: &Node, _ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    Err(ShapeError {
        message: format!(
            "shape not ported: {} (rendering-elements/shapes/state.js)",
            node.shape
        ),
    })
}
