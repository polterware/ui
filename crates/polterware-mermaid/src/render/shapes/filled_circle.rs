//! Classic port of upstream `shapes/filledCircle.ts`.
//!
//! A 7px filled dot with no label. Classic rendering keeps rough.js's solid fill and
//! roughness 0; its fill is the theme's `nodeBorder` for both fill and stroke.
use super::{
    IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, get_node_classes, shape_style,
};
use crate::scene::{Color, EllipseShape, Group, Point, Rect, Size, SourceRef};

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    node.label.clear();

    // handDrawn: rough.js (later)
    let radius = 7.0;
    node.bounds = Rect::centered(Point::ZERO, Size::new(radius * 2.0, radius * 2.0));

    let border = ctx.theme.color("nodeBorder").unwrap_or("#9370DB");
    let mut style = shape_style();
    style.fill = Some(Color::new(border));
    style.stroke = Some(Color::new(border));
    style.stroke_width = 1.3;

    let mut group = Group::new()
        .with_class(get_node_classes(&node, None))
        .with_source(SourceRef::node(node.id.clone()));
    group.push(
        EllipseShape::new(Point::ZERO, radius, radius, style)
            .with_source(SourceRef::node(node.id.clone())),
    );

    node.intersect = IntersectKind::Circle;
    Ok(RenderedNode { node, group })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_upstream_classic_fixture() {
        crate::render::shapes::double_circle::fixture::check(
            include_str!("../testdata/shapes/filled_circle.json"),
            draw,
        );
    }
}
