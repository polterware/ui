//! Port of upstream `shapes/bang.ts`.
//!
//! The classic look is one closed SVG path built from fourteen relative arcs, class
//! `basic label-container`, shifted by half its effective size. The label sits above the
//! shape's centre. Attachment uses `intersect.rect`, exactly as upstream. rough.js is only
//! the handDrawn stroke.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare, shape_style};
use crate::scene::{
    Group, Item, Path, PathCommand, PathShape, Point, Rect, Size, SourceRef, Transform,
};

/// Upstream keeps the label in the upper half.
const LABEL_WIDTH_DIVISOR: f64 = 2.0;

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, label, text) = prepare(&node, ctx)?;
    let half_padding = node.padding / 2.0;
    let w = text.width + 10.0 * half_padding;
    let h = text.height + 8.0 * half_padding;
    let r = 0.15 * w;

    let min_width = text.width + 20.0;
    let min_height = text.height + 20.0;
    let effective_width = w.max(min_width);
    let effective_height = h.max(min_height);
    if !effective_width.is_finite() || !effective_height.is_finite() {
        return Err(ShapeError {
            message: "invalid bang label dimensions".into(),
        });
    }

    let mut label = label;
    label.transform = Transform::translate(
        -text.width / LABEL_WIDTH_DIVISOR,
        -text.height / LABEL_WIDTH_DIVISOR,
    );
    group.items.push(Item::Group(label));

    let arcs = [
        (r, r, effective_width * 0.25, -effective_height * 0.1),
        (r, r, effective_width * 0.25, 0.0),
        (r, r, effective_width * 0.25, 0.0),
        (r, r, effective_width * 0.25, effective_height * 0.1),
        (r, r, effective_width * 0.15, effective_height * 0.33),
        (r * 0.8, r * 0.8, 0.0, effective_height * 0.34),
        (r, r, -effective_width * 0.15, effective_height * 0.33),
        (r, r, -effective_width * 0.25, effective_height * 0.15),
        (r, r, -effective_width * 0.25, 0.0),
        (r, r, -effective_width * 0.25, 0.0),
        (r, r, -effective_width * 0.25, -effective_height * 0.15),
        (r, r, -effective_width * 0.1, -effective_height * 0.33),
        (r * 0.8, r * 0.8, 0.0, -effective_height * 0.34),
        (r, r, effective_width * 0.1, -effective_height * 0.33),
    ];

    let mut current = Point::ZERO;
    let mut path = Path::new();
    path.move_to(current);
    for (rx, ry, dx, dy) in arcs {
        current = Point::new(current.x + dx, current.y + dy);
        // Upstream writes `a${r},${r} 1 0,0 ...`: the `1` lands in the x-axis-rotation slot,
        // and large-arc/sweep are both 0.
        path.arc_to(rx, ry, 1.0, false, false, current);
    }
    path.line_to(Point::new(0.0, current.y));
    path.line_to(Point::ZERO);
    path.close();

    let bounds = path_bounds(&path).unwrap_or(Rect::ZERO);

    // handDrawn: rough.js (later)
    let mut shape = Group::new()
        .with_class("basic label-container")
        .with_transform(Transform::translate(
            -effective_width / 2.0,
            -effective_height / 2.0,
        ));
    shape.push(PathShape::new(path, shape_style()).with_source(SourceRef::node(node.id.clone())));
    group.push(Item::Group(shape));

    node.bounds = Rect::centered(Point::ZERO, Size::new(bounds.width, bounds.height));
    node.intersect = IntersectKind::Rect;
    Ok(RenderedNode { node, group })
}

// TODO(dedupe): exact SVG path bounds for the shapes whose node size comes from `getBBox`.
fn path_bounds(path: &Path) -> Option<Rect> {
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut current = Point::ZERO;
    let mut start = Point::ZERO;
    let mut include = |point: Point| {
        min_x = min_x.min(point.x);
        max_x = max_x.max(point.x);
        min_y = min_y.min(point.y);
        max_y = max_y.max(point.y);
    };
    for command in &path.commands {
        match *command {
            PathCommand::MoveTo { x, y } => {
                current = Point::new(x, y);
                start = current;
                include(current);
            }
            PathCommand::LineTo { x, y } => {
                current = Point::new(x, y);
                include(current);
            }
            PathCommand::ArcTo {
                rx,
                ry,
                rotation,
                large_arc,
                sweep,
                x,
                y,
            } => {
                let to = Point::new(x, y);
                for point in sample_arc(
                    current,
                    Point::new(rx, ry),
                    rotation,
                    (large_arc, sweep),
                    to,
                    512,
                ) {
                    include(point);
                }
                current = to;
            }
            PathCommand::CubicTo {
                c1x,
                c1y,
                c2x,
                c2y,
                x,
                y,
            } => {
                for point in sample_cubic(
                    current,
                    Point::new(c1x, c1y),
                    Point::new(c2x, c2y),
                    Point::new(x, y),
                    256,
                ) {
                    include(point);
                }
                current = Point::new(x, y);
            }
            PathCommand::QuadTo { cx, cy, x, y } => {
                let control = Point::new(cx, cy);
                let to = Point::new(x, y);
                let reflected = Point::new(2.0 * to.x - control.x, 2.0 * to.y - control.y);
                for point in sample_cubic(current, control, reflected, to, 256) {
                    include(point);
                }
                current = to;
            }
            PathCommand::Close => {
                current = start;
            }
        }
    }
    if min_x.is_finite() {
        Some(Rect::new(min_x, min_y, max_x - min_x, max_y - min_y))
    } else {
        None
    }
}

// TODO(dedupe): mirror of the fixture generator's SVG arc sampler.
fn sample_arc(
    from: Point,
    radius: Point,
    rotation_degrees: f64,
    (large_arc, sweep): (bool, bool),
    to: Point,
    steps: usize,
) -> Vec<Point> {
    let (rx, ry) = (radius.x, radius.y);
    let phi = rotation_degrees.to_radians();
    let (sin_phi, cos_phi) = phi.sin_cos();
    let dx2 = (from.x - to.x) / 2.0;
    let dy2 = (from.y - to.y) / 2.0;
    let x1p = cos_phi * dx2 + sin_phi * dy2;
    let y1p = -sin_phi * dx2 + cos_phi * dy2;
    let mut rx = rx.abs();
    let mut ry = ry.abs();
    let mut rx2 = rx * rx;
    let mut ry2 = ry * ry;
    let lambda = x1p * x1p / rx2 + y1p * y1p / ry2;
    if lambda > 1.0 {
        let scale = lambda.sqrt();
        rx *= scale;
        ry *= scale;
        rx2 = rx * rx;
        ry2 = ry * ry;
    }
    let sign = if large_arc != sweep { 1.0 } else { -1.0 };
    let numerator = rx2 * ry2 - rx2 * y1p * y1p - ry2 * x1p * x1p;
    let denominator = rx2 * y1p * y1p + ry2 * x1p * x1p;
    if denominator == 0.0 {
        return vec![from, to];
    }
    let coefficient = sign * (numerator / denominator).max(0.0).sqrt();
    let cxp = coefficient * rx * y1p / ry;
    let cyp = -coefficient * ry * x1p / rx;
    let cx = cos_phi * cxp - sin_phi * cyp + (from.x + to.x) / 2.0;
    let cy = sin_phi * cxp + cos_phi * cyp + (from.y + to.y) / 2.0;

    let angle = |ux: f64, uy: f64, vx: f64, vy: f64| {
        let mut cosine =
            (ux * vx + uy * vy) / ((ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt());
        cosine = cosine.clamp(-1.0, 1.0);
        let mut result = cosine.acos();
        if ux * vy - uy * vx < 0.0 {
            result = -result;
        }
        result
    };
    let ux = (x1p - cxp) / rx;
    let uy = (y1p - cyp) / ry;
    let vx = (-x1p - cxp) / rx;
    let vy = (-y1p - cyp) / ry;
    let theta1 = angle(1.0, 0.0, ux, uy);
    let mut delta = angle(ux, uy, vx, vy);
    if !sweep && delta > 0.0 {
        delta -= std::f64::consts::TAU;
    } else if sweep && delta < 0.0 {
        delta += std::f64::consts::TAU;
    }
    let mut points = Vec::with_capacity(steps + 1);
    for index in 0..=steps {
        let theta = theta1 + delta * index as f64 / steps as f64;
        let (sin_theta, cos_theta) = theta.sin_cos();
        points.push(Point::new(
            cx + rx * cos_theta * cos_phi - ry * sin_theta * sin_phi,
            cy + rx * cos_theta * sin_phi + ry * sin_theta * cos_phi,
        ));
    }
    points
}

fn sample_cubic(
    from: Point,
    control1: Point,
    control2: Point,
    to: Point,
    steps: usize,
) -> Vec<Point> {
    let mut points = Vec::with_capacity(steps + 1);
    for index in 0..=steps {
        let t = index as f64 / steps as f64;
        let u = 1.0 - t;
        points.push(Point::new(
            u * u * u * from.x
                + 3.0 * u * u * t * control1.x
                + 3.0 * u * t * t * control2.x
                + t * t * t * to.x,
            u * u * u * from.y
                + 3.0 * u * u * t * control1.y
                + 3.0 * u * t * t * control2.y
                + t * t * t * to.y,
        ));
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::shapes::label_image_utils::fixtures::{Fixture, assert_case};

    #[test]
    fn matches_upstream_classic_geometry() {
        let fixture: Fixture =
            serde_json::from_str(include_str!("../testdata/shapes/bang.json")).unwrap();
        for case in &fixture.cases {
            assert_case(case, draw);
        }
    }
}
