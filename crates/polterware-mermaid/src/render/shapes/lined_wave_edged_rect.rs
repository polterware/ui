//! Placeholder for upstream `shapes/linedWaveEdgedRect.ts`.
use super::{Node, RenderedNode, ShapeContext, ShapeError};

pub fn draw(node: &Node, _ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    Err(ShapeError {
        message: format!(
            "shape not ported: {} (rendering-elements/shapes/linedWaveEdgedRect.js)",
            node.shape
        ),
    })
}
