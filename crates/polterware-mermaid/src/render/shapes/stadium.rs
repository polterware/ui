//! Upstream `shapes/stadium.ts`.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare, shape_style};
use crate::scene::{Item, Path, PathShape, Point, Rect, Size, SourceRef};

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, label, text_size) = prepare(&node, ctx)?;
    let padding = if node.look == "neo" {
        (40.0, 24.0)
    } else {
        (node.padding, node.padding)
    };
    let height = text_size.height + padding.1;
    let width = text_size.width + height / 4.0 + padding.0;
    node.bounds = Rect::centered(Point::ZERO, Size::new(width, height));
    let h = height;
    let r = h / 2.0;
    let x = -width / 2.0;
    let y = -height / 2.0;
    let right = -x;
    let mut path = Path::new();
    path.move_to(Point::new(x + r, y))
        .line_to(Point::new(right - r, y))
        .arc_to(r, r, 0.0, false, true, Point::new(right - r, y + h))
        .line_to(Point::new(x + r, y + h))
        .arc_to(r, r, 0.0, false, true, Point::new(x + r, y))
        .close();
    group.push(PathShape::new(path, shape_style()).with_source(SourceRef::node(node.id.clone())));
    let mut polygon = vec![Point::new(r, 0.0), Point::new(width - r, 0.0)];
    polygon.extend((0..=24).map(|i| {
        let angle = (-90.0 + i as f64 * 180.0 / 24.0).to_radians();
        Point::new(width - r + r * angle.cos(), r + r * angle.sin())
    }));
    polygon.push(Point::new(r, height));
    polygon.extend((0..=24).map(|i| {
        let angle = (90.0 + i as f64 * 180.0 / 24.0).to_radians();
        Point::new(r + r * angle.cos(), r + r * angle.sin())
    }));
    node.intersect = IntersectKind::Polygon(polygon);
    group.items.push(Item::Group(label));
    Ok(RenderedNode { node, group })
}
