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

/// Upstream `generateArcPoints`: 20 points along the ellipse between the two endpoints,
/// multiplying the point offset by the node's `radius`. The Rust port keeps the exact
/// parameterization because the polygon's `intersect` result depends on the samples.
fn generate_arc_points(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    rx: f64,
    ry: f64,
    clockwise: bool,
) -> Vec<Point> {
    let num_points = 20;
    let mid_x = (x1 + x2) / 2.0;
    let mid_y = (y1 + y2) / 2.0;
    let angle = (y2 - y1).atan2(x2 - x1);
    let transformed_x = (x2 - x1) / 2.0 / rx;
    let transformed_y = (y2 - y1) / 2.0 / ry;
    let distance = (transformed_x * transformed_x + transformed_y * transformed_y).sqrt();
    let scaled_center_distance = (1.0 - distance * distance).sqrt();
    let direction = if clockwise { -1.0 } else { 1.0 };
    let center_x = mid_x + scaled_center_distance * ry * angle.sin() * direction;
    let center_y = mid_y - scaled_center_distance * rx * angle.cos() * direction;
    let start_angle = ((y1 - center_y) / ry).atan2((x1 - center_x) / rx);
    let end_angle = ((y2 - center_y) / ry).atan2((x2 - center_x) / rx);
    let mut angle_range = end_angle - start_angle;
    if clockwise && angle_range < 0.0 {
        angle_range += std::f64::consts::TAU;
    }
    if !clockwise && angle_range > 0.0 {
        angle_range -= std::f64::consts::TAU;
    }
    (0..num_points)
        .map(|i| {
            let t = i as f64 / (num_points - 1) as f64;
            let angle = start_angle + t * angle_range;
            Point::new(center_x + rx * angle.cos(), center_y + ry * angle.sin())
        })
        .collect()
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
        Point::new(w / 2.0, -h / 2.0),
        Point::new(-w / 2.0, -h / 2.0),
    ];
    pts.extend(generate_arc_points(
        -w / 2.0,
        -h / 2.0,
        -w / 2.0,
        h / 2.0,
        rx,
        ry,
        false,
    ));
    pts.push(Point::new(w / 2.0, h / 2.0));
    pts.extend(generate_arc_points(
        w / 2.0,
        h / 2.0,
        w / 2.0,
        -h / 2.0,
        rx,
        ry,
        true,
    ));
    // Upstream draws the path with `transform="translate(rx / 2, 0)"`, but its
    // `intersect.polygon` receives the untranslated `points` and re-centres them from
    // `node.width`, so the hit test sits a hair inside the painted path.
    let hit_points = pts.clone();
    for point in &mut pts {
        point.x += rx / 2.0;
    }
    let mut out = finish(node, group, label, pts, Point::new(0.0, -b.height / 2.0))?;
    // Upstream's `node.x` stays at the origin while the path is translated by `rx / 2`;
    // `intersect.polygon` centres the untranslated points on `node.x`, so the hit test is
    // offset from the painted path by exactly that half radius.
    let (width, height) = (out.node.bounds.width, out.node.bounds.height);
    out.node.bounds = Rect::new(-width / 2.0, -height / 2.0, width, height);
    out.node.intersect = IntersectKind::Polygon(hit_points);
    Ok(out)
}
