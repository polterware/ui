//! Upstream `shapes/cloud.ts`; classic `basic label-container` path and rectangular intersection.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare, shape_style};
use crate::scene::{Group, Item, Path, PathShape, Point, Rect, Size, SourceRef};

#[allow(clippy::too_many_arguments)]
fn arc_points(
    x0: f64,
    y0: f64,
    mut rx: f64,
    mut ry: f64,
    fa: bool,
    fs: bool,
    x1: f64,
    y1: f64,
) -> Vec<Point> {
    if rx == 0.0 || ry == 0.0 {
        return vec![Point::new(x1, y1)];
    }
    rx = rx.abs();
    ry = ry.abs();
    let dx = (x0 - x1) / 2.0;
    let dy = (y0 - y1) / 2.0;
    let lambda = (dx * dx) / (rx * rx) + (dy * dy) / (ry * ry);
    if lambda > 1.0 {
        let sqrt_l = lambda.sqrt();
        rx *= sqrt_l;
        ry *= sqrt_l;
    }
    let sign = if fa == fs { -1.0 } else { 1.0 };
    let num = rx * rx * ry * ry - rx * rx * dy * dy - ry * ry * dx * dx;
    let sq = (num / (rx * rx * dy * dy + ry * ry * dx * dx)).max(0.0);
    let coef = sign * sq.sqrt();
    let cxp = coef * ((rx * dy) / ry);
    let cyp = coef * (-(ry * dx) / rx);
    let cx = cxp + (x0 + x1) / 2.0;
    let cy = cyp + (y0 + y1) / 2.0;

    let v1x = (dx - cxp) / rx;
    let v1y = (dy - cyp) / ry;
    let v2x = (-dx - cxp) / rx;
    let v2y = (-dy - cyp) / ry;

    let angle = |ux: f64, uy: f64, vx: f64, vy: f64| -> f64 {
        let sign = if ux * vy - uy * vx < 0.0 { -1.0 } else { 1.0 };
        let dot = ux * vx + uy * vy;
        let len = (ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt();
        sign * (dot / len).clamp(-1.0, 1.0).acos()
    };

    let theta1 = angle(1.0, 0.0, v1x, v1y);
    let mut dtheta = angle(v1x, v1y, v2x, v2y);
    if !fs && dtheta > 0.0 {
        dtheta -= 2.0 * std::f64::consts::PI;
    }
    if fs && dtheta < 0.0 {
        dtheta += 2.0 * std::f64::consts::PI;
    }

    let mut pts = vec![Point::new(x0, y0), Point::new(x1, y1)];
    let steps = 16;
    for s in 1..steps {
        let theta = theta1 + (dtheta * s as f64) / steps as f64;
        let px = rx * theta.cos() + cx;
        let py = ry * theta.sin() + cy;
        pts.push(Point::new(px, py));
    }
    pts
}

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, label, text) = prepare(&node, ctx)?;
    let pad = if node.look == "neo" {
        12.0
    } else {
        node.padding
    };
    let w = text.width + pad;
    let h = text.height + pad;
    if !w.is_finite() || !h.is_finite() {
        return Err(ShapeError {
            message: "invalid cloud label dimensions".into(),
        });
    }
    let r1 = 0.15 * w;
    let r2 = 0.25 * w;
    let r3 = 0.35 * w;
    let r4 = 0.2 * w;
    // Classic SVG path from upstream; handDrawn: rough.js (later)
    let mut path = Path::new();
    path.move_to(Point::new(0.0, 0.0));
    let arcs = [
        (r1, r1, false, true, 0.25 * w, -0.1 * w),
        (r3, r3, false, true, 0.4 * w, -0.1 * w),
        (r2, r2, false, true, 0.35 * w, 0.2 * w),
        (r1, r1, false, true, 0.15 * w, 0.35 * h),
        (r4, r4, false, true, -0.15 * w, 0.65 * h),
        (r2, r1, false, true, -0.25 * w, 0.15 * w),
        (r3, r3, false, true, -0.5 * w, 0.0),
        (r1, r1, false, true, -0.25 * w, -0.15 * w),
        (r1, r1, false, true, -0.1 * w, -0.35 * h),
        (r4, r4, false, true, 0.1 * w, -0.65 * h),
    ];
    let mut p = Point::ZERO;
    let mut all_points = vec![Point::ZERO];
    for (rx, ry, fa, fs, dx, dy) in arcs {
        let p_next = Point::new(p.x + dx, p.y + dy);
        all_points.extend(arc_points(p.x, p.y, rx, ry, fa, fs, p_next.x, p_next.y));
        path.arc_to(rx, ry, 0.0, fa, fs, p_next);
        p = p_next;
    }
    path.line_to(Point::new(0.0, 0.0));
    path.close();

    let (min_x, max_x, min_y, max_y) = all_points.iter().fold(
        (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ),
        |(min_x, max_x, min_y, max_y), pt| {
            (
                min_x.min(pt.x),
                max_x.max(pt.x),
                min_y.min(pt.y),
                max_y.max(pt.y),
            )
        },
    );
    let mut shape = Group::translated(-w / 2.0, -h / 2.0);
    shape.push(PathShape::new(path, shape_style()).with_source(SourceRef::node(node.id.clone())));
    group.push(shape);
    node.bounds = Rect::centered(Point::ZERO, Size::new(max_x - min_x, max_y - min_y));
    node.intersect = IntersectKind::Rect;
    let mut label = label;
    label.transform = crate::scene::Transform::translate(-text.width / 2.0, -text.height / 2.0);
    group.items.push(Item::Group(label));
    Ok(RenderedNode { node, group })
}
