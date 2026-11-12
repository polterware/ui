//! Classic port of upstream `shapes/choice.ts`.
//!
//! A diamond of the larger of 28px and the node width, drawn from the four midpoint points
//! `createPathFromPoints` joins. The node keeps upstream's 28x28 bounds even when the drawn
//! diamond is larger.
use super::{
    IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, create_path_from_points,
    get_node_classes, shape_style,
};
use crate::scene::{Color, Group, PathShape, Point, Rect, Size, SourceRef};

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    node.label.clear();

    // handDrawn: rough.js (later)
    let s = 28.0_f64.max(node.width.unwrap_or(0.0));
    let points = vec![
        Point::new(0.0, s / 2.0),
        Point::new(s / 2.0, 0.0),
        Point::new(0.0, -s / 2.0),
        Point::new(-s / 2.0, 0.0),
    ];

    let mut style = shape_style();
    style.fill = Some(Color::new(ctx.theme.color("mainBkg").unwrap_or("#ECECFF")));
    style.stroke = Some(Color::new(
        ctx.theme.color("nodeBorder").unwrap_or("#9370DB"),
    ));
    style.stroke_width = 1.3;

    let mut group = Group::new()
        .with_class(get_node_classes(&node, None))
        .with_source(SourceRef::node(node.id.clone()));
    group.push(
        PathShape::new(create_path_from_points(&points), style)
            .with_source(SourceRef::node(node.id.clone())),
    );

    node.width = Some(28.0);
    node.height = Some(28.0);
    node.bounds = Rect::centered(Point::ZERO, Size::new(28.0, 28.0));
    node.intersect = IntersectKind::Polygon(points);
    Ok(RenderedNode { node, group })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_upstream_classic_fixture() {
        crate::render::shapes::double_circle::fixture::check(
            include_str!("../testdata/shapes/choice.json"),
            draw,
        );
    }
}
