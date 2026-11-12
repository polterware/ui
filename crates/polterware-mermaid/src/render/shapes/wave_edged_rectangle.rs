//! Port of upstream `shapes/waveEdgedRectangle.ts`; the outline follows its sampled path.
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
    let adjust_height = node.width.is_none() && node.height.is_none();
    if node.width.is_some() || node.height.is_some() {
        node.width = Some((node.width.unwrap_or(0.0) - pad_x * 2.0).max(10.0));
        node.height = Some((node.height.unwrap_or(0.0) - pad_y * 2.0).max(10.0));
    }
    let w = node.width.filter(|v| *v != 0.0).unwrap_or(text.width) + pad_x * 2.0;
    let h = node.height.filter(|v| *v != 0.0).unwrap_or(text.height) + pad_y * 2.0;
    let amp = if node.look == "neo" { h / 4.0 } else { h / 8.0 };
    let final_h = h + if adjust_height { amp } else { -amp };
    let extra = ((14.0 - w).max(0.0)) / 2.0;
    let left = -w / 2.0 - extra;
    let right = w / 2.0 + extra;
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
    let shape = PathShape::new(super::create_path_from_points(&points), shape_style())
        .with_source(SourceRef::node(node.id.clone()));
    node.bounds = Rect::centered(Point::ZERO, Size::new(right - left, final_h + amp));
    let mut shape_group = Group::new().with_class("basic label-container outer-path");
    shape_group.push(shape);
    shape_group.transform = Transform::translate(0.0, -amp / 2.0);
    group.push(shape_group);
    label.transform = Transform::translate(-w / 2.0 + pad_x, -h / 2.0 + pad_y - amp);
    group.push(Item::Group(label));
    node.intersect = IntersectKind::Polygon(points);
    Ok(RenderedNode { node, group })
}
