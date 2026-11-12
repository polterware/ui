//! Classic port of upstream `shapes/rectWithTitle.ts`.
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
    // `rectWithTitle` builds its title through `createLabel`, which passes
    // `width: Number.POSITIVE_INFINITY` to `createText`: the title never wraps, ignoring the
    // diagram's `wrappingWidth`.
    let mut title_node = node.clone();
    title_node.wrapping_width = None;
    let (group, label, b) = prepare(&title_node, ctx)?;
    let p = node.padding;
    let w = (b.width + p).max(node.width.unwrap_or(0.0));
    let h = (b.height + p).max(node.height.unwrap_or(0.0));
    let mut out = finish(node, group, label, box_points(w, h), Point::ZERO)?;
    let mut divider = Path::new();
    divider
        .move_to(Point::new(-w / 2.0, 0.0))
        .line_to(Point::new(w / 2.0, 0.0));
    out.group.push(
        PathShape::new(divider, shape_style()).with_source(SourceRef::node(out.node.id.clone())),
    );
    Ok(out)
}
