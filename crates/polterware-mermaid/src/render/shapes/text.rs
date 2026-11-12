//! Classic port of upstream `shapes/text.ts`.
//!
//! A label plus `node.padding` on each side, with no outline radius. The label keeps
//! `labelHelper`'s SVG placement: its left edge at x=0 and its box vertically centered on the
//! node. Upstream gives the `<rect>` the `text` class; the scene carries rects without their
//! own class, so the role lives only in this doc comment.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare, shape_style};
use crate::scene::{Item, Point, Rect, RectShape, Size, SourceRef};

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, label, text_size) = prepare(&node, ctx)?;

    let total_width = (text_size.width + node.padding).max(node.width.unwrap_or(0.0));
    let total_height = (text_size.height + node.padding).max(node.height.unwrap_or(0.0));
    node.bounds = Rect::centered(Point::ZERO, Size::new(total_width, total_height));

    group.push(
        RectShape::new(node.bounds, 0.0, shape_style())
            .with_source(SourceRef::node(node.id.clone())),
    );
    group.items.push(Item::Group(label));

    node.intersect = IntersectKind::Rect;
    Ok(RenderedNode { node, group })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_upstream_classic_fixture() {
        crate::render::shapes::double_circle::fixture::check(
            include_str!("../testdata/shapes/text.json"),
            draw,
        );
    }
}
