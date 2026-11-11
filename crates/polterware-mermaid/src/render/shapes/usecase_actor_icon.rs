//! Placeholder for upstream `shapes/usecaseActorIcon.ts`.
use super::{Node, RenderedNode, ShapeContext, ShapeError};

pub fn draw(node: &Node, _ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    Err(ShapeError {
        message: format!(
            "shape not ported: {} (rendering-elements/shapes/usecaseActorIcon.js)",
            node.shape
        ),
    })
}
