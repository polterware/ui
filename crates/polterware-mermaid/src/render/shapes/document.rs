//! Port of upstream `shapes/document.ts`; its classic document edge is a cylinder-style path.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare, shape_style};
use crate::scene::{Group, Item, Path, PathShape, Point, Rect, Size, SourceRef};

// handDrawn: rough.js (later)
pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, label, text) = prepare(&node, ctx)?;
    let w = text.width + node.padding;
    let rx = w / 2.0;
    let ry = rx / (2.5 + w / 50.0);
    let h = text.height + ry + node.padding;
    let mut path = Path::new();
    let left = -w / 2.0;
    let top = -h / 2.0;
    path.move_to(Point::new(left, top));
    path.arc_to(rx, ry, 0.0, false, false, Point::new(left + w, top));
    path.arc_to(rx, ry, 0.0, false, false, Point::new(left, top));
    path.line_to(Point::new(left, top + h));
    path.arc_to(rx, ry, 0.0, false, false, Point::new(left + w, top + h));
    path.line_to(Point::new(left + w, top));
    let mut shape = Group::new().with_class("basic label-container");
    shape.push(PathShape::new(path, shape_style()).with_source(SourceRef::node(node.id.clone())));
    group.push(shape);
    group.push(Item::Group(label));
    node.bounds = Rect::centered(Point::ZERO, Size::new(w, h + 2.0 * ry));
    node.intersect = IntersectKind::Polygon(super::cylinder::cylinder_outline(w, h, ry, 128));
    Ok(RenderedNode { node, group })
}
