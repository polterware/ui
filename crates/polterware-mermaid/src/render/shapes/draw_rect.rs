//! Upstream `shapes/drawRect.ts`; rough.js is intentionally omitted for classic look.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare, shape_style};
use crate::scene::{Item, Point, Rect, RectShape, Size, SourceRef};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RectOptions {
    pub label_padding_x: f64,
    pub label_padding_y: f64,
    pub rx: Option<f64>,
    pub ry: Option<f64>,
}

pub fn draw_rect(
    node: &Node,
    ctx: &ShapeContext<'_>,
    options: RectOptions,
) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, label, text_size) = prepare(&node, ctx)?;
    let width = (text_size.width + options.label_padding_x * 2.0).max(node.width.unwrap_or(0.0));
    let height = (text_size.height + options.label_padding_y * 2.0).max(node.height.unwrap_or(0.0));
    node.bounds = Rect::centered(Point::ZERO, Size::new(width, height));
    let mut rect = RectShape::new(node.bounds, options.rx.unwrap_or(node.rx), shape_style());
    rect.ry = options.ry.unwrap_or(node.ry);
    group.push(rect.with_source(SourceRef::node(node.id.clone())));
    group.items.push(Item::Group(label));
    node.intersect = IntersectKind::Rect;
    Ok(RenderedNode { node, group })
}

// Compatibility entry for the current registry; this upstream module is a helper, not a shape.
pub fn draw(node: &Node, _ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    Err(ShapeError {
        message: format!("{} is a rendering helper, not a shape", node.shape),
    })
}
