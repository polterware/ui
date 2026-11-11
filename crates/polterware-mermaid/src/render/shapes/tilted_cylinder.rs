//! Port of upstream `shapes/tiltedCylinder.ts`; classic path matches the horizontal cylinder.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare, shape_style};
use crate::scene::{Group, Item, Path, PathShape, Point, Rect, Size, SourceRef, Transform};

// handDrawn: rough.js (later)
pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, mut label, text) = prepare(&node, ctx)?;
    let pad = if node.look == "neo" {
        12.0
    } else {
        node.padding / 2.0
    };
    let original_h = node.height.unwrap_or(0.0);
    if let Some(height) = node.height {
        node.height = Some((height - pad).max(5.0));
    }
    if let Some(width) = node.width {
        let old_ry = original_h / 2.0;
        let old_rx = old_ry / (2.5 + original_h / 50.0);
        node.width = Some((width - pad - old_rx * 3.0).max(10.0));
    }
    let h = node.height.unwrap_or(0.0).max(text.height) + pad;
    let ry = h / 2.0;
    let rx = ry / (2.5 + h / 50.0);
    let w = node.width.unwrap_or(0.0).max(text.width) + rx + pad;
    let bounds = Rect::centered(Point::ZERO, Size::new(w + 2.0 * rx, h));
    node.bounds = bounds;
    let left = -w / 2.0;
    let top = -h / 2.0;
    let mut path = Path::new();
    path.move_to(Point::new(left, top));
    path.arc_to(rx, ry, 0.0, false, true, Point::new(left, top + h));
    path.line_to(Point::new(left + w, top + h));
    path.arc_to(rx, ry, 0.0, false, true, Point::new(left + w, top));
    path.move_to(Point::new(left + w, top));
    path.arc_to(rx, ry, 0.0, false, false, Point::new(left + w, top + h));
    path.line_to(Point::new(left, top + h));
    let mut shape = Group::new().with_class("basic label-container outer-path");
    shape.push(PathShape::new(path, shape_style()).with_source(SourceRef::node(node.id.clone())));
    group.push(shape);
    label.transform = Transform::translate(-text.width / 2.0 - rx, -text.height / 2.0);
    group.push(Item::Group(label));
    node.intersect = IntersectKind::Polygon(tilted_outline(w, h, rx, 128));
    Ok(RenderedNode { node, group })
}

fn tilted_outline(width: f64, height: f64, rx: f64, steps: usize) -> Vec<Point> {
    let ry = height / 2.0;
    let mut points = Vec::with_capacity(steps * 2 + 2);
    for i in 0..=steps {
        let a = -std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * i as f64 / steps as f64;
        points.push(Point::new(-width / 2.0 - rx + rx * a.cos(), ry * a.sin()));
    }
    for i in 0..=steps {
        let a = std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * i as f64 / steps as f64;
        points.push(Point::new(width / 2.0 + rx + rx * a.cos(), ry * a.sin()));
    }
    points
}
