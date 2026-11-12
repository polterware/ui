//! Port of upstream `shapes/multiWaveEdgedRectangle.ts`; both outer and inset outlines are kept.
use super::{
    IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, generate_full_sine_wave_points,
    prepare, shape_style,
};
use crate::scene::{Group, Item, Path, PathShape, Point, Rect, Size, SourceRef, Transform};

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
    let adjust_h = node.width.is_none() && node.height.is_none();
    if !adjust_h {
        node.width = Some(node.width.unwrap_or(0.0) - pad_x * 2.0);
        node.height = Some(node.height.unwrap_or(0.0) - pad_y * 3.0);
    }
    let w = text.width.max(node.width.unwrap_or(0.0)) + pad_x * 2.0;
    let h = text.height.max(node.height.unwrap_or(0.0)) + pad_y * 3.0;
    let amp = if node.look == "neo" { h / 4.0 } else { h / 8.0 };
    let final_h = h + if adjust_h { amp / 2.0 } else { -amp / 2.0 };
    let x = -w / 2.0;
    let y = -final_h / 2.0;
    let off = 10.0;
    let wave = generate_full_sine_wave_points(
        x - off,
        y + final_h + off,
        x + w - off,
        y + final_h + off,
        amp,
        0.8,
    );
    let last = *wave.last().expect("sine wave contains sampled points");
    let outer_points = vec![
        Point::new(x - off, y + off),
        Point::new(x - off, y + final_h + off),
    ];
    let mut outer_points = outer_points;
    outer_points.extend(wave);
    outer_points.extend([
        Point::new(x + w - off, last.y - off),
        Point::new(x + w, last.y - off),
        Point::new(x + w, last.y - 2.0 * off),
        Point::new(x + w + off, last.y - 2.0 * off),
        Point::new(x + w + off, y - off),
        Point::new(x + off, y - off),
        Point::new(x + off, y),
        Point::new(x, y),
        Point::new(x, y + off),
    ]);
    let inner_points = [
        Point::new(x, y + off),
        Point::new(x + w - off, y + off),
        Point::new(x + w - off, last.y - off),
        Point::new(x + w, last.y - off),
        Point::new(x + w, y),
        Point::new(x, y),
    ];
    let mut shape = Group::new().with_class("basic label-container outer-path");
    shape.push(
        PathShape::new(super::create_path_from_points(&outer_points), shape_style())
            .with_source(SourceRef::node(node.id.clone())),
    );
    let mut inner = Path::polygon(inner_points);
    inner.close();
    shape.push(PathShape::new(inner, shape_style()).with_source(SourceRef::node(node.id.clone())));
    shape.transform = Transform::translate(0.0, -amp / 2.0);
    let (min_x, max_x, min_y, max_y) = outer_points.iter().fold(
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
    label.transform = Transform::translate(-text.width / 2.0 - off, -text.height / 2.0 + off - amp);
    group.push(Item::Group(label));
    node.intersect = IntersectKind::Polygon(outer_points);
    Ok(RenderedNode { node, group })
}
