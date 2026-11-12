//! Classic port of upstream `shapes/bowTieRect.ts`.
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
    let h = node.height.unwrap_or(b.height) + py;
    let ry = h / 2.0;
    let rx = ry / (2.5 + h / 50.0);
    let w = node.width.unwrap_or(b.width) + px * 2.0;
    let mut pts = vec![
        Point::new(w / 2.0 + rx / 2.0, -h / 2.0),
        Point::new(-w / 2.0 + rx / 2.0, -h / 2.0),
    ];
    for i in 0..20 {
        let a = -std::f64::consts::FRAC_PI_2 - (std::f64::consts::PI * i as f64 / 19.0);
        pts.push(Point::new(-w / 2.0 + rx / 2.0 + rx * a.cos(), ry * a.sin()));
    }
    pts.push(Point::new(w / 2.0 + rx / 2.0, h / 2.0));
    for i in 0..20 {
        let a = std::f64::consts::FRAC_PI_2 - (std::f64::consts::PI * i as f64 / 19.0);
        pts.push(Point::new(w / 2.0 + rx / 2.0 - rx * a.cos(), ry * a.sin()));
    }
    finish(node, group, label, pts, Point::new(0.0, -b.height / 2.0))
}
