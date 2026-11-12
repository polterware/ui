//! Classic port of upstream `shapes/state.ts`.
//!
//! The shape is a `drawRect` rounded rectangle: radius 3 for `neo` and 5 for classic, with
//! the `flowchart-node` class. Upstream's `state.ts` passes no `labelPaddingX`/`labelPaddingY`
//! to `drawRect`, which makes the classic rectangle's width, height and origin `NaN`; this
//! port passes `node.padding`, the padding the other `drawRect` shapes use, so state nodes
//! draw. The label is measured and placed exactly as `drawRect` does.
use super::draw_rect::{RectOptions, draw_rect};
use super::{Node, RenderedNode, ShapeContext, ShapeError};

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    // handDrawn: rough.js (later)
    let radius = if node.look == "neo" { 3.0 } else { 5.0 };
    draw_rect(
        node,
        ctx,
        RectOptions {
            label_padding_x: node.padding,
            label_padding_y: node.padding,
            rx: Some(radius),
            ry: Some(radius),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_upstream_classic_fixture() {
        crate::render::shapes::double_circle::fixture::check(
            include_str!("../testdata/shapes/state.json"),
            draw,
        );
    }
}
