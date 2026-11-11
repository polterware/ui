//! Classic port of upstream `shapes/card.ts`.
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
        28.0
    } else {
        node.padding
    };
    let py = if node.look == "neo" {
        24.0
    } else {
        node.padding
    };
    let notch = 12.0;
    let w = node.width.unwrap_or(b.width)
        + if node.look == "neo" {
            px * 2.0
        } else {
            px + notch
        };
    let h = node.height.unwrap_or(b.height) + if node.look == "neo" { py * 2.0 } else { py };
    let pts = vec![
        Point::new(-w / 2.0 + notch, -h / 2.0),
        Point::new(w / 2.0, -h / 2.0),
        Point::new(w / 2.0, h / 2.0),
        Point::new(-w / 2.0, h / 2.0),
        Point::new(-w / 2.0, -h / 2.0 + notch),
        Point::new(-w / 2.0 + notch, -h / 2.0),
    ]; // handDrawn: rough.js (later)
    finish(node, group, label, pts, Point::ZERO)
}
