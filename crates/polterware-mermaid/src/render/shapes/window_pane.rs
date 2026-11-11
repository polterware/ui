//! Classic port of upstream `shapes/windowPane.ts`.
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
    let mut node = node.clone();
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
    let d = 10.0;
    if node.width.is_some() || node.height.is_some() {
        node.width = Some((node.width.unwrap_or(0.0) - 2.0 * px - d).max(10.0));
        node.height = Some((node.height.unwrap_or(0.0) - 2.0 * py - d).max(10.0));
    }
    let w = node.width.unwrap_or(b.width) + 2.0 * px;
    let h = node.height.unwrap_or(b.height) + 2.0 * py;
    let x = -w / 2.0;
    let y = -h / 2.0;
    let pts = vec![
        Point::new(x - d, y - d),
        Point::new(x + w, y - d),
        Point::new(x + w, y + h),
        Point::new(x - d, y + h),
    ]; // handDrawn: rough.js (later)
    finish(
        node,
        group,
        label,
        pts,
        Point::new(d / 2.0 - b.width / 2.0, d / 2.0 - b.height / 2.0),
    )
}
