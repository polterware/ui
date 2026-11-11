//! Upstream `shapes/squareRect.ts`.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare, shape_style};
use crate::scene::{Item, Point, Rect, RectShape, Size, SourceRef};

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, label, text_size) = prepare(&node, ctx)?;
    let padding = if node.look == "neo" {
        (16.0, 12.0)
    } else {
        (node.padding * 2.0, node.padding)
    };
    let width = (text_size.width + padding.0 * 2.0).max(node.width.unwrap_or(0.0));
    let height = (text_size.height + padding.1 * 2.0).max(node.height.unwrap_or(0.0));
    node.bounds = Rect::centered(Point::ZERO, Size::new(width, height));
    group.push(
        RectShape::new(node.bounds, 0.0, shape_style())
            .with_source(SourceRef::node(node.id.clone())),
    );
    node.intersect = IntersectKind::Rect;
    group.items.push(Item::Group(label));
    Ok(RenderedNode { node, group })
}
