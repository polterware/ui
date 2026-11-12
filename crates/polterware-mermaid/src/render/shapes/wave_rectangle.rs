//! Port of upstream `shapes/waveRectangle.ts`; the paper tape uses both sampled waves.
use super::{
    IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, generate_full_sine_wave_points,
    prepare, shape_style,
};
use crate::scene::{Group, Item, PathShape, Point, Rect, Size, SourceRef};

// handDrawn: rough.js (later)
pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, label, text) = prepare(&node, ctx)?;
    let pad_x = if node.look == "neo" {
        16.0
    } else {
        node.padding
    };
    let pad_y = if node.look == "neo" {
        20.0
    } else {
        node.padding
    };
    if node.width.is_some() || node.height.is_some() {
        let width = node.width.unwrap_or(0.0).max(20.0);
        let height = node.height.unwrap_or(0.0).max(10.0);
        let amplitude = (height * 0.2).min(height / 4.0);
        node.height = Some((height - pad_y - amplitude * (20.0 / 9.0)).ceil());
        node.width = Some(width - pad_x * 2.0);
    }
    let w = node.width.filter(|v| *v != 0.0).unwrap_or(text.width) + pad_x * 2.0;
    let h = node.height.filter(|v| *v != 0.0).unwrap_or(text.height) + pad_y;
    let amp = h / 8.0;
    let final_h = h + 2.0 * amp;
    let mut points = vec![Point::new(-w / 2.0, final_h / 2.0)];
    points.extend(generate_full_sine_wave_points(
        -w / 2.0,
        final_h / 2.0,
        w / 2.0,
        final_h / 2.0,
        amp,
        1.0,
    ));
    points.push(Point::new(w / 2.0, -final_h / 2.0));
    points.extend(generate_full_sine_wave_points(
        w / 2.0,
        -final_h / 2.0,
        -w / 2.0,
        -final_h / 2.0,
        amp,
        -1.0,
    ));
    let mut shape = Group::new().with_class("basic label-container");
    shape.push(
        PathShape::new(super::create_path_from_points(&points), shape_style())
            .with_source(SourceRef::node(node.id.clone())),
    );
    let min_y = points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    let max_y = points.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
    node.bounds = Rect::centered(Point::ZERO, Size::new(w, max_y - min_y));
    group.push(shape);
    group.push(Item::Group(label));
    node.intersect = IntersectKind::Polygon(points);
    Ok(RenderedNode { node, group })
}
