//! Port of upstream `shapes/linedWaveEdgedRect.ts`; includes its left binding and wavy edge.
use super::{
    IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, generate_full_sine_wave_points,
    prepare, shape_style,
};
use crate::scene::{Group, Item, PathShape, Point, Rect, Size, SourceRef, Transform};

// handDrawn: rough.js (later)
pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, mut label, text) = prepare(&node, ctx)?;
    let pad_x = if node.look == "neo" {
        16.0
    } else {
        node.padding
    };
    let pad_y = if node.look == "neo" {
        12.0
    } else {
        node.padding
    };
    if node.width.is_some() || node.height.is_some() {
        node.width = Some((node.width.unwrap_or(0.0) * 10.0 / 11.0 - pad_x * 2.0).max(10.0));
        node.height = Some((node.height.unwrap_or(0.0) - pad_y * 2.0).max(10.0));
    }
    let w = node.width.filter(|v| *v != 0.0).unwrap_or(text.width) + pad_x * 2.0;
    let h = node.height.filter(|v| *v != 0.0).unwrap_or(text.height) + pad_y * 2.0;
    let amp = if node.look == "neo" { h / 4.0 } else { h / 8.0 };
    let final_h = h + amp;
    let x_left = -w / 2.0 - w * 0.05;
    let x_right = w / 2.0 + w * 0.05;
    let mut points = vec![
        Point::new(x_left, -final_h / 2.0),
        Point::new(x_left, final_h / 2.0),
    ];
    points.extend(generate_full_sine_wave_points(
        x_left,
        final_h / 2.0,
        x_right,
        final_h / 2.0,
        amp,
        0.8,
    ));
    points.extend([
        Point::new(x_right, -final_h / 2.0),
        Point::new(x_left, -final_h / 2.0),
        Point::new(-w / 2.0, -final_h / 2.0),
        Point::new(-w / 2.0, final_h * 0.55),
        Point::new(-w / 2.0, -final_h / 2.0),
    ]);
    let mut shape = Group::new().with_class("basic label-container outer-path");
    shape.push(
        PathShape::new(super::create_path_from_points(&points), shape_style())
            .with_source(SourceRef::node(node.id.clone())),
    );
    shape.transform = Transform::translate(0.0, -amp / 2.0);
    let (min_x, max_x, min_y, max_y) = points.iter().fold(
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
    node.bounds = Rect::centered(Point::ZERO, Size::new(max_x - min_x, max_y - min_y));
    group.push(shape);
    label.transform = Transform::translate(
        -w / 2.0 + pad_x + ((w / 2.0) * 0.1) / 2.0,
        -h / 2.0 + pad_y - amp,
    );
    group.push(Item::Group(label));
    node.intersect = IntersectKind::Polygon(points);
    Ok(RenderedNode { node, group })
}
