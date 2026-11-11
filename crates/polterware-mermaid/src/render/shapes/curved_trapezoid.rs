//! Classic port of upstream `shapes/curvedTrapezoid.ts`.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare, shape_style};
use crate::scene::{Item, Path, PathShape, Point, Rect, SourceRef, Transform};

// TODO(dedupe): this shape-local renderer mirrors updateNodeBounds/createPathFromPoints.
fn finish(
    mut node: Node,
    mut group: crate::scene::Group,
    label: crate::scene::Group,
    points: Vec<Point>,
    label_offset: Point,
) -> Result<RenderedNode, ShapeError> {
    let min_x = points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let max_x = points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
    let min_y = points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    let max_y = points.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
    node.bounds = Rect::new(min_x, min_y, max_x - min_x, max_y - min_y);
    let path = Path::polygon(points.iter().copied());
    group.push(PathShape::new(path, shape_style()).with_source(SourceRef::node(node.id.clone())));
    let mut label = label;
    label.transform = Transform::translate(label_offset.x, label_offset.y);
    node.intersect = IntersectKind::Polygon(points);
    group.items.push(Item::Group(label));
    Ok(RenderedNode { node, group })
}

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let node = node.clone();
    let (group, label, b) = prepare(&node, ctx)?;
    let px = if node.look == "neo" {
        16.0
    } else {
        node.padding
    };
    let py = if node.look == "neo" {
        12.0
    } else {
        node.padding
    };
    let h = (b.height + py * 2.0)
        .max(node.height.unwrap_or(0.0))
        .max(5.0);
    let r = h / 2.0;
    let clearance = r - (r * r - (b.height / 2.0).powi(2)).max(0.0).sqrt();
    let w = (b.width + 2.0 * px + 2.0 * (h / 4.0).max(clearance))
        .max((b.width + px * 2.0) * 1.25)
        .max(node.width.unwrap_or(0.0))
        .max(20.0);
    let rw = w - r;
    let tw = h / 4.0;
    let mut pts = vec![
        Point::new(rw, 0.0),
        Point::new(tw, 0.0),
        Point::new(0.0, h / 2.0),
        Point::new(tw, h),
        Point::new(rw, h),
    ];
    for i in 0..50 {
        let a = (270.0 - 180.0 * i as f64 / 49.0).to_radians();
        pts.push(Point::new(-rw + r * a.cos(), -h / 2.0 + r * a.sin()));
    }
    let pts: Vec<_> = pts
        .into_iter()
        .map(|p| Point::new(p.x - w / 2.0, p.y - h / 2.0))
        .collect(); // handDrawn: rough.js (later)
    finish(node, group, label, pts, Point::ZERO)
}
