//! Classic port of upstream `shapes/lightningBolt.ts`.
//!
//! A 35px (at least) bolt polygon, drawn in untransformed local coordinates, placed by the
//! `outer-path` group's `translate(-width/2, -height)` exactly as upstream does. The node
//! bounds therefore span two heights.
use super::{
    IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, create_path_from_points,
    get_node_classes, shape_style,
};
use crate::scene::{Color, Group, Item, PathShape, Point, Rect, Size, SourceRef, Transform};

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    node.label.clear();

    // handDrawn: rough.js (later)
    let width = 35.0_f64.max(node.width.unwrap_or(0.0));
    let height = 35.0_f64.max(node.height.unwrap_or(0.0));
    let gap = 7.0;

    let points = vec![
        Point::new(width, 0.0),
        Point::new(0.0, height + gap / 2.0),
        Point::new(width - 2.0 * gap, height + gap / 2.0),
        Point::new(0.0, 2.0 * height),
        Point::new(width, height - gap / 2.0),
        Point::new(2.0 * gap, height - gap / 2.0),
    ];

    node.bounds = Rect::centered(Point::ZERO, Size::new(width, 2.0 * height));

    let mut style = shape_style();
    style.fill = Some(Color::new(ctx.theme.color("mainBkg").unwrap_or("#ECECFF")));
    style.stroke = Some(Color::new(
        ctx.theme.color("nodeBorder").unwrap_or("#9370DB"),
    ));
    style.stroke_width = 1.3;

    let mut outer = Group::new()
        .with_class("outer-path")
        .with_transform(Transform::translate(-width / 2.0, -height))
        .with_source(SourceRef::node(node.id.clone()));
    outer.push(
        PathShape::new(create_path_from_points(&points), style)
            .with_source(SourceRef::node(node.id.clone())),
    );

    let mut group = Group::new()
        .with_class(get_node_classes(&node, None))
        .with_source(SourceRef::node(node.id.clone()));
    group.items.push(Item::Group(outer));

    node.intersect = IntersectKind::Polygon(points);
    Ok(RenderedNode { node, group })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_upstream_classic_fixture() {
        crate::render::shapes::double_circle::fixture::check(
            include_str!("../testdata/shapes/lightning_bolt.json"),
            draw,
        );
    }
}
