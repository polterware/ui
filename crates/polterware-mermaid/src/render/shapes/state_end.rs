//! Classic port of upstream `shapes/stateEnd.ts`.
//!
//! A 14px (at least) outer circle with a smaller filled inner circle, both inside the
//! `outer-path` group classic rendering emits. Upstream draws the inner circle with diameter
//! `width * 5 / 14`, so its radius is `width * 5 / 28`. The small-state drop-shadow filter
//! is a CSS effect with no scene equivalent and is omitted.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError};
use crate::scene::{Color, EllipseShape, Group, Item, PathStyle, Point, Rect, Size, SourceRef};

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();

    if node.width.is_some() || node.height.is_some() {
        if node.width.unwrap_or(0.0) < 14.0 {
            node.width = Some(14.0);
        }
        if node.height.unwrap_or(0.0) < 14.0 {
            node.height = Some(14.0);
        }
    }
    if node.width.is_none() {
        node.width = Some(14.0);
    }
    if node.height.is_none() {
        node.height = Some(14.0);
    }

    // Classic rendering always draws both circles through roughjs, even at `roughness: 0`.
    // Its 9-step spline bulges slightly past the nominal circle, so the DOM box
    // `updateNodeBounds` records is a hair wider than the ellipse this port paints. Layout
    // reads that measured box; the painted `bounds` below stays the nominal intersection box.
    let width = node.width.unwrap_or(14.0).max(14.0);
    let measured_width = rough_circle_width(width);
    node.width = Some(measured_width.max(width));
    node.height = Some(width);

    // handDrawn: rough.js (later)
    let painted_width = width;
    let outer_radius = painted_width / 2.0;
    let inner_radius = (painted_width * 5.0) / 28.0;
    node.bounds = Rect::centered(Point::ZERO, Size::new(painted_width, painted_width));

    let line = ctx.theme.color("lineColor").unwrap_or("#333333");
    let inner = ctx
        .theme
        .color("stateBorder")
        .or_else(|| ctx.theme.color("nodeBorder"))
        .unwrap_or("#9370DB");
    let background = ctx.theme.color("mainBkg").unwrap_or("#ECECFF");

    let outer_style = PathStyle {
        fill: Some(Color::new(background)),
        stroke: Some(Color::new(line)),
        stroke_width: 2.0,
        ..Default::default()
    };
    let inner_style = PathStyle {
        fill: Some(Color::new(inner)),
        stroke: Some(Color::new(inner)),
        stroke_width: 2.0,
        ..Default::default()
    };

    let mut outer = Group::new()
        .with_class("outer-path")
        .with_source(SourceRef::node(node.id.clone()));
    outer.push(
        EllipseShape::new(Point::ZERO, outer_radius, outer_radius, outer_style)
            .with_source(SourceRef::node(node.id.clone())),
    );
    outer.push(
        EllipseShape::new(Point::ZERO, inner_radius, inner_radius, inner_style)
            .with_source(SourceRef::node(node.id.clone())),
    );

    let mut group = Group::new()
        .with_class("node default")
        .with_source(SourceRef::node(node.id.clone()));
    group.items.push(Item::Group(outer));

    node.intersect = IntersectKind::Circle;
    Ok(RenderedNode { node, group })
}

/// The width rough.js's `_curve` gives a circle of diameter `width` at `roughness: 0`.
///
/// `ellipseWithParams` samples the circle every `2π / curveStepCount / 4` radians, then
/// `_curve` joins the samples with Catmull-Rom cubics; the spline overshoots the circle at
/// the quarter points, which is what makes the measured box wider than the painted one.
fn rough_circle_width(width: f64) -> f64 {
    let radius = width / 2.0;
    let psq =
        (std::f64::consts::PI * 2.0 * ((radius * radius + radius * radius) / 2.0).sqrt()).sqrt();
    let step_count = (9.0_f64).max((9.0 / 200.0_f64.sqrt()) * psq).ceil();
    let increment = std::f64::consts::PI * 2.0 / step_count / 4.0;

    let mut points = Vec::new();
    points.push((-increment, 0.0));
    let mut angle = 0.0;
    while angle <= std::f64::consts::PI * 2.0 {
        points.push((angle, 0.0));
        angle += increment;
    }
    points.push((0.0, 0.0));
    points.push((increment, 0.0));
    let points: Vec<Point> = points
        .into_iter()
        .map(|(angle, _)| Point::new(radius * angle.cos(), radius * angle.sin()))
        .collect();

    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    for index in 1..points.len().saturating_sub(2) {
        let point = points[index];
        let next = points[index + 1];
        let previous = points[index - 1];
        let following = points[index + 2];
        let control1 = Point::new(
            point.x + (next.x - previous.x) / 6.0,
            point.y + (next.y - previous.y) / 6.0,
        );
        let control2 = Point::new(
            next.x + (point.x - following.x) / 6.0,
            next.y + (point.y - following.y) / 6.0,
        );
        for value in cubic_extrema(point.x, control1.x, control2.x, next.x) {
            min_x = min_x.min(value);
            max_x = max_x.max(value);
        }
    }
    max_x - min_x
}

/// Every value a cubic Bézier reaches along one axis: its endpoints and interior extrema.
fn cubic_extrema(p0: f64, p1: f64, p2: f64, p3: f64) -> Vec<f64> {
    let at = |t: f64| {
        (1.0 - t).powi(3) * p0
            + 3.0 * (1.0 - t).powi(2) * t * p1
            + 3.0 * (1.0 - t) * t * t * p2
            + t * t * t * p3
    };
    let mut values = vec![p0, p3];
    let a = 3.0 * (p3 - 3.0 * p2 + 3.0 * p1 - p0);
    let b = 6.0 * (p2 - 2.0 * p1 + p0);
    let c = 3.0 * (p1 - p0);
    if a.abs() < 1e-12 {
        if b.abs() > 1e-12 {
            let t = -c / b;
            if t > 0.0 && t < 1.0 {
                values.push(at(t));
            }
        }
    } else {
        let discriminant = b * b - 4.0 * a * c;
        if discriminant >= 0.0 {
            let root = discriminant.sqrt();
            for t in [(-b + root) / (2.0 * a), (-b - root) / (2.0 * a)] {
                if t > 0.0 && t < 1.0 {
                    values.push(at(t));
                }
            }
        }
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_upstream_classic_fixture() {
        crate::render::shapes::double_circle::fixture::check(
            include_str!("../testdata/shapes/state_end.json"),
            draw,
        );
    }
}
