//! Port of upstream `shapes/slopedrectangle.ts` classic geometry.
use super::{
    IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, create_path_from_points, prepare,
    shape_style,
};
use crate::scene::{Item, PathShape, Point, Rect, Size, SourceRef};

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, label, text) = prepare(&node, ctx)?;
    // handDrawn: rough.js (later)
    let (px, py) = if node.look == "neo" {
        (16.0, 12.0)
    } else {
        (node.padding, node.padding)
    };
    if node.width.is_some() || node.height.is_some() {
        node.width = Some((node.width.unwrap_or(0.0) - px * 2.0).max(10.0));
        node.height = Some((node.height.unwrap_or(0.0) / 1.5 - py * 2.0).max(10.0));
    }
    let w = node.width.filter(|v| *v != 0.0).unwrap_or(text.width) + px * 2.0;
    let h = node.height.filter(|v| *v != 0.0).unwrap_or(text.height) + py * 2.0;
    let points = vec![
        Point::new(-w / 2.0, -h / 4.0),
        Point::new(-w / 2.0, 3.0 * h / 4.0),
        Point::new(w / 2.0, 3.0 * h / 4.0),
        Point::new(w / 2.0, -3.0 * h / 4.0),
    ];
    let width = w;
    let height = 1.5 * h;
    node.bounds = Rect::centered(Point::ZERO, Size::new(width, height));
    group.push(
        PathShape::new(create_path_from_points(&points), shape_style())
            .with_source(SourceRef::node(node.id.clone())),
    );
    node.intersect = IntersectKind::Polygon(points);
    let mut label = label;
    label.transform = crate::scene::Transform::translate(-w / 2.0 + px, -h / 4.0 + py);
    group.items.push(Item::Group(label));
    Ok(RenderedNode { node, group })
}
