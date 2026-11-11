//! Port of upstream `shapes/flippedtriangle.ts` classic geometry.
use super::{
    IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, create_path_from_points, prepare,
    shape_style,
};
use crate::scene::{Item, PathShape, Point, Rect, Size, SourceRef};

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, label, text) = prepare(&node, ctx)?;
    // handDrawn: rough.js (later)
    let points = {
        let px = if node.look == "neo" {
            node.padding * 2.0
        } else {
            node.padding
        };
        if node.width.is_some() || node.height.is_some() {
            node.height = Some(node.height.unwrap_or(0.0).max(10.0));
            node.width = Some((node.width.unwrap_or(0.0) - px - px / 2.0).max(10.0));
        }
        let w = node.width.filter(|v| *v != 0.0).unwrap_or(text.width) + px;
        let h = node.height.filter(|v| *v != 0.0).unwrap_or(w + text.height);
        vec![
            Point::new(0.0, -h),
            Point::new(h, -h),
            Point::new(h / 2.0, 0.0),
        ]
    };
    let (min_x, max_x, min_y, max_y) = points.iter().fold(
        (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ),
        |(a, b, c, d), p| (a.min(p.x), b.max(p.x), c.min(p.y), d.max(p.y)),
    );
    let width = max_x - min_x;
    let height = max_y - min_y;
    node.bounds = Rect::centered(Point::ZERO, Size::new(width, height));
    let local = points
        .iter()
        .map(|p| Point::new(p.x - (min_x + max_x) / 2.0, p.y - (min_y + max_y) / 2.0))
        .collect::<Vec<_>>();
    group.push(
        PathShape::new(create_path_from_points(&local), shape_style())
            .with_source(SourceRef::node(node.id.clone())),
    );
    node.intersect = IntersectKind::Polygon(points);
    group.items.push(Item::Group(label));
    Ok(RenderedNode { node, group })
}
