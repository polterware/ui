//! Port of upstream `shapes/hourglass.ts` classic geometry.
use super::{
    IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, create_path_from_points, prepare,
    shape_style,
};
use crate::scene::{PathShape, Point, Rect, Size, SourceRef};

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    node.label.clear();
    let (mut group, _, _) = prepare(&node, ctx)?;
    // handDrawn: rough.js (later)
    let points = {
        let w = node.width.unwrap_or(0.0).max(30.0);
        let h = node.height.unwrap_or(0.0).max(30.0);
        node.label.clear();
        vec![
            Point::new(0.0, 0.0),
            Point::new(w, 0.0),
            Point::new(0.0, h),
            Point::new(w, h),
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

    Ok(RenderedNode { node, group })
}
