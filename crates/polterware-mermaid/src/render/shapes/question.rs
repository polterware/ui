//! Upstream `shapes/question.ts`.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare, shape_style};
use crate::scene::{Item, Path, PathShape, Point, Rect, Size, SourceRef};

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, label, text_size) = prepare(&node, ctx)?;
    let s = text_size.width + text_size.height + node.padding;
    node.bounds = Rect::centered(Point::ZERO, Size::new(s, s));
    let points = vec![
        Point::new(s / 2.0, 0.0),
        Point::new(s, s / 2.0),
        Point::new(s / 2.0, s),
        Point::new(0.0, s / 2.0),
    ];
    let local: Vec<Point> = points
        .iter()
        .map(|p| p.translated(-s / 2.0, -s / 2.0))
        .collect();
    group.push(
        PathShape::new(Path::polygon(local), shape_style())
            .with_source(SourceRef::node(node.id.clone())),
    );
    node.intersect = IntersectKind::Polygon(points);
    group.items.push(Item::Group(label));
    Ok(RenderedNode { node, group })
}
