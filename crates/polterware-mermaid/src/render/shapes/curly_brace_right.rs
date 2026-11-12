//! Port of upstream `shapes/curlyBraceRight.ts`.
//!
//! Mirror of `curlyBraceLeft`: an open brace polyline plus a bounds rectangle inside a
//! `<g class="text">` shifted left by the brace radius. Upstream's local circle sampler for
//! this shape does not negate its points, unlike the left and double braces, so the local
//! sampler below follows it. Intersection uses the rectangle points.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare};
use crate::scene::{
    Color, Group, Item, Path, PathShape, PathStyle, Point, Rect, Size, SourceRef, Transform,
};

// TODO(dedupe): upstream's per-shape `generateCirclePoints` without the negation.
fn circle_points(cx: f64, cy: f64, r: f64, count: usize, start: f64, end: f64) -> Vec<Point> {
    if count == 0 {
        return Vec::new();
    }
    let step = if count == 1 {
        0.0
    } else {
        (end - start).to_radians() / (count - 1) as f64
    };
    let start = start.to_radians();
    (0..count)
        .map(|index| {
            let angle = start + index as f64 * step;
            Point::new(cx + r * angle.cos(), cy + r * angle.sin())
        })
        .collect()
}

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, label, text) = prepare(&node, ctx)?;
    let label_padding_x = if node.look == "neo" {
        18.0
    } else {
        node.padding
    };
    let label_padding_y = if node.look == "neo" {
        12.0
    } else {
        node.padding
    };
    let w = text.width
        + if node.look == "neo" {
            label_padding_x * 2.0
        } else {
            label_padding_x
        };
    let h = text.height
        + if node.look == "neo" {
            label_padding_y * 2.0
        } else {
            label_padding_y
        };
    if !w.is_finite() || !h.is_finite() {
        return Err(ShapeError {
            message: "invalid curly brace label dimensions".into(),
        });
    }
    let radius = (h * 0.1).max(5.0);

    let mut points = Vec::new();
    points.extend(circle_points(w / 2.0, -h / 2.0, radius, 20, -90.0, 0.0));
    points.push(Point::new(w / 2.0 + radius, -radius));
    points.extend(circle_points(
        w / 2.0 + radius * 2.0,
        -radius,
        radius,
        20,
        -180.0,
        -270.0,
    ));
    points.extend(circle_points(
        w / 2.0 + radius * 2.0,
        radius,
        radius,
        20,
        -90.0,
        -180.0,
    ));
    points.push(Point::new(w / 2.0 + radius, h / 2.0));
    points.extend(circle_points(w / 2.0, h / 2.0, radius, 20, 0.0, 90.0));

    let mut rect_points = Vec::new();
    rect_points.push(Point::new(-w / 2.0, -h / 2.0 - radius));
    rect_points.push(Point::new(w / 2.0, -h / 2.0 - radius));
    rect_points.extend(circle_points(w / 2.0, -h / 2.0, radius, 20, -90.0, 0.0));
    rect_points.push(Point::new(w / 2.0 + radius, -radius));
    rect_points.extend(circle_points(
        w / 2.0 + radius * 2.0,
        -radius,
        radius,
        20,
        -180.0,
        -270.0,
    ));
    rect_points.extend(circle_points(
        w / 2.0 + radius * 2.0,
        radius,
        radius,
        20,
        -90.0,
        -180.0,
    ));
    rect_points.push(Point::new(w / 2.0 + radius, h / 2.0));
    rect_points.extend(circle_points(w / 2.0, h / 2.0, radius, 20, 0.0, 90.0));
    rect_points.push(Point::new(w / 2.0, h / 2.0 + radius));
    rect_points.push(Point::new(-w / 2.0, h / 2.0 + radius));

    // handDrawn: rough.js (later)
    let mut shape = Group::new()
        .with_class("text")
        .with_transform(Transform::translate(-radius, 0.0));
    shape.push(
        PathShape::new(Path::polyline(points.iter().copied()), outline_style(ctx))
            .with_source(SourceRef::node(node.id.clone())),
    );
    shape.push(
        PathShape::new(
            Path::polygon(rect_points.iter().copied()),
            invisible_style(),
        )
        .with_source(SourceRef::node(node.id.clone())),
    );
    group.push(Item::Group(shape));

    let mut label = label;
    label.transform =
        Transform::translate(-w / 2.0 + node.padding / 2.0, -h / 2.0 + node.padding / 2.0);
    group.items.push(Item::Group(label));

    let min_x = points
        .iter()
        .chain(rect_points.iter())
        .map(|point| point.x)
        .fold(f64::INFINITY, f64::min);
    let max_x = points
        .iter()
        .chain(rect_points.iter())
        .map(|point| point.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = points
        .iter()
        .chain(rect_points.iter())
        .map(|point| point.y)
        .fold(f64::INFINITY, f64::min);
    let max_y = points
        .iter()
        .chain(rect_points.iter())
        .map(|point| point.y)
        .fold(f64::NEG_INFINITY, f64::max);
    node.bounds = Rect::centered(Point::ZERO, Size::new(max_x - min_x, max_y - min_y));
    node.intersect = IntersectKind::Polygon(rect_points);
    Ok(RenderedNode { node, group })
}

/// The brace is an open path: `userNodeOverrides(node, { fill: 'none' })`.
fn outline_style(ctx: &ShapeContext<'_>) -> PathStyle {
    PathStyle {
        fill: None,
        stroke: Some(Color::new(
            ctx.theme.color("nodeBorder").unwrap_or("#9370DB"),
        )),
        ..Default::default()
    }
}

/// The bounds-only rectangle is painted with `stroke-opacity: 0`.
fn invisible_style() -> PathStyle {
    PathStyle {
        fill: None,
        stroke: None,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::shapes::label_image_utils::fixtures::{Fixture, assert_case};

    #[test]
    fn matches_upstream_classic_geometry() {
        let fixture: Fixture =
            serde_json::from_str(include_str!("../testdata/shapes/curly_brace_right.json"))
                .unwrap();
        for case in &fixture.cases {
            assert_case(case, draw);
        }
    }
}
