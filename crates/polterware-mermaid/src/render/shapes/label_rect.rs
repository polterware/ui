//! Classic port of upstream `shapes/labelRect.ts`.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare, shape_style};
use crate::scene::{Item, Path, PathShape, Point, Rect, SourceRef, Transform};

// TODO(dedupe): this shape-local renderer mirrors updateNodeBounds/createPathFromPoints.
fn box_points(w: f64, h: f64) -> Vec<Point> {
    vec![
        Point::new(-w / 2.0, -h / 2.0),
        Point::new(w / 2.0, -h / 2.0),
        Point::new(w / 2.0, h / 2.0),
        Point::new(-w / 2.0, h / 2.0),
    ]
}

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
    let (mut group, label, b) = prepare(&node, ctx)?;
    group.class = Some("label edgeLabel".into());
    let pts = box_points(0.1, 0.1);
    finish(
        node,
        group,
        label,
        pts,
        Point::new(-b.width / 2.0, -b.height / 2.0),
    )
}
