//! Upstream `shapes/circle.ts`.
use super::{
    IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare_ignoring_min_width,
    shape_style,
};
use crate::scene::{EllipseShape, Item, Point, Rect, Size, SourceRef};

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, label, text_size) = prepare_ignoring_min_width(&node, ctx)?;
    let r = (text_size.width.powi(2) + text_size.height.powi(2)).sqrt() / 2.0
        + if node.look == "neo" {
            16.0
        } else {
            node.padding / 2.0
        };
    node.bounds = Rect::centered(Point::ZERO, Size::new(r * 2.0, r * 2.0));
    group.push(
        EllipseShape::new(Point::ZERO, r, r, shape_style())
            .with_source(SourceRef::node(node.id.clone())),
    );
    node.intersect = IntersectKind::Circle;
    group.items.push(Item::Group(label));
    Ok(RenderedNode { node, group })
}
