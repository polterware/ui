//! Port of upstream `shapes/datastore.ts`; classic look uses a dashed rectangle boundary.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare, shape_style};
use crate::scene::{Item, Path, PathShape, Point, Rect, Size, SourceRef};

// handDrawn: rough.js (later)
pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, label, text) = prepare(&node, ctx)?;
    let pad_x = node.padding * 2.0;
    let pad_y = node.padding;
    let width = (text.width + pad_x * 2.0).max(node.width.unwrap_or(0.0));
    let height = (text.height + pad_y * 2.0).max(node.height.unwrap_or(0.0));
    let bounds = Rect::centered(Point::ZERO, Size::new(width, height));
    let mut style = shape_style();
    style.dash = vec![width, height];
    let mut shape = Path::rect(bounds, 0.0, 0.0);
    shape.close();
    group.push(PathShape::new(shape, style).with_source(SourceRef::node(node.id.clone())));
    group.push(Item::Group(label));
    node.bounds = bounds;
    node.intersect = IntersectKind::Rect;
    Ok(RenderedNode { node, group })
}
