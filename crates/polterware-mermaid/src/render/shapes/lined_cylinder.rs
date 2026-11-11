//! Port of upstream `shapes/linedCylinder.ts`; classic path preserves its inner top line.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare, shape_style};
use crate::scene::{Group, Item, Path, PathShape, Point, Rect, Size, SourceRef, Transform};

// handDrawn: rough.js (later)
pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, mut label, text) = prepare(&node, ctx)?;
    let pad_x = if node.look == "neo" {
        16.0
    } else {
        node.padding
    };
    let pad_y = if node.look == "neo" {
        24.0
    } else {
        node.padding
    };
    let original_width = node.width.unwrap_or(0.0);
    if node.width.is_some() || node.height.is_some() {
        node.width = Some((original_width - pad_x).max(10.0));
        let old_rx = original_width / 2.0;
        let old_ry = old_rx / (2.5 + original_width / 50.0);
        node.height = Some((node.height.unwrap_or(0.0) - pad_y - old_ry * 3.0).max(10.0));
    }
    let w = node.width.filter(|w| *w != 0.0).unwrap_or(text.width) + pad_x * 2.0;
    let rx = w / 2.0;
    let ry = rx / (2.5 + w / 50.0);
    let h = node.height.filter(|h| *h != 0.0).unwrap_or(text.height) + ry + pad_y * 2.0;
    let bounds = Rect::centered(Point::ZERO, Size::new(w, h + 2.0 * ry));
    node.bounds = bounds;
    let left = -w / 2.0;
    let top = -h / 2.0;
    let outer_offset = h * 0.1;
    let mut path = Path::new();
    path.move_to(Point::new(left, top));
    path.arc_to(rx, ry, 0.0, false, false, Point::new(left + w, top));
    path.arc_to(rx, ry, 0.0, false, false, Point::new(left, top));
    path.line_to(Point::new(left, top + h));
    path.arc_to(rx, ry, 0.0, false, false, Point::new(left + w, top + h));
    path.line_to(Point::new(left + w, top));
    path.move_to(Point::new(left, top + outer_offset));
    path.arc_to(
        rx,
        ry,
        0.0,
        false,
        false,
        Point::new(left + w, top + outer_offset),
    );
    let mut shape = Group::new().with_class("basic label-container outer-path");
    shape.push(PathShape::new(path, shape_style()).with_source(SourceRef::node(node.id.clone())));
    group.push(shape);
    label.transform = Transform::translate(-text.width / 2.0, -text.height / 2.0 + ry);
    group.push(Item::Group(label));
    let points = super::cylinder::cylinder_outline(w, h, ry, 128);
    node.intersect = IntersectKind::Polygon(points);
    Ok(RenderedNode { node, group })
}
