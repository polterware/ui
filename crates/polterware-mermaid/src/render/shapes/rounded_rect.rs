//! Upstream `shapes/roundedRect.ts`.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare, shape_style};
use crate::scene::{Item, Point, Rect, RectShape, Size, SourceRef};

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, label, text_size) = prepare(&node, ctx)?;
    let width = (text_size.width + node.padding * 2.0).max(node.width.unwrap_or(0.0));
    let height = (text_size.height + node.padding * 2.0).max(node.height.unwrap_or(0.0));
    node.bounds = Rect::centered(Point::ZERO, Size::new(width, height));
    let radius = ctx.theme.number("radius").unwrap_or(5.0);
    group.push(
        RectShape::new(node.bounds, radius, shape_style())
            .with_source(SourceRef::node(node.id.clone())),
    );
    node.intersect = IntersectKind::Rect;
    group.items.push(Item::Group(label));
    Ok(RenderedNode { node, group })
}
