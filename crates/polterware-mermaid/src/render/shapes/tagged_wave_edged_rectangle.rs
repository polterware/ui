//! Port of upstream `shapes/taggedWaveEdgedRectangle.ts`; the tag overlays the paper edge.
use super::{
    IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, generate_full_sine_wave_points,
    prepare, shape_style,
};
use crate::scene::{Group, Item, PathShape, Point, Rect, Size, SourceRef, Transform};

// handDrawn: rough.js (later)
pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, mut label, text) = prepare(&node, ctx)?;
    let pad = node.padding;
    let w = (text.width + pad * 2.0).max(node.width.unwrap_or(0.0));
    let h = (text.height + pad * 2.0).max(node.height.unwrap_or(0.0));
    let amp = h / 8.0;
    let tag_w = 0.2 * w;
    let tag_h = 0.2 * h;
    let final_h = h + amp;
    let left = -w / 2.0 - w * 0.05;
    let right = w / 2.0 + w * 0.05;
    let mut points = vec![Point::new(left, final_h / 2.0)];
    points.extend(generate_full_sine_wave_points(
        left,
        final_h / 2.0,
        right,
        final_h / 2.0,
        amp,
        0.8,
    ));
    points.extend([
        Point::new(right, -final_h / 2.0),
        Point::new(left, -final_h / 2.0),
    ]);
    let x = -w / 2.0 + w * 0.05;
    let y = -final_h / 2.0 - tag_h * 0.4;
    let tag_points = [
        Point::new(x + w - tag_w, (y + h) * 1.3),
        Point::new(x + w, y + h - tag_h),
        Point::new(x + w, (y + h) * 0.9),
    ];
    let mut tag_points = tag_points.to_vec();
    tag_points.extend(generate_full_sine_wave_points(
        x + w,
        (y + h) * 1.25,
        x + w - tag_w,
        (y + h) * 1.3,
        -h * 0.02,
        0.5,
    ));
    let mut shape = Group::new().with_class("basic label-container outer-path");
    shape.push(
        PathShape::new(super::create_path_from_points(&points), shape_style())
            .with_source(SourceRef::node(node.id.clone())),
    );
    shape.push(
        PathShape::new(super::create_path_from_points(&tag_points), shape_style())
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
    label.transform = Transform::translate(-w / 2.0 + pad, -h / 2.0 + pad - amp / 2.0);
    group.push(Item::Group(label));
    node.intersect = IntersectKind::Polygon(points);
    Ok(RenderedNode { node, group })
}
