//! Classic port of upstream `shapes/anchor.ts`.
//!
//! A 1px-radius black dot with no stroke and no label, inside the `anchor` group classic
//! rendering emits. The node's own classes stay on the outer group.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, get_node_classes};
use crate::scene::{Color, EllipseShape, Group, Item, PathStyle, Point, Rect, Size, SourceRef};

pub fn draw(node: &Node, _ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    // handDrawn: rough.js (later)
    let radius = 1.0;
    node.bounds = Rect::centered(Point::ZERO, Size::new(radius * 2.0, radius * 2.0));

    let style = PathStyle {
        fill: Some(Color::new("black")),
        stroke: None,
        ..Default::default()
    };

    let mut anchor = Group::new()
        .with_class("anchor")
        .with_source(SourceRef::node(node.id.clone()));
    anchor.push(
        EllipseShape::new(Point::ZERO, radius, radius, style)
            .with_source(SourceRef::node(node.id.clone())),
    );

    let mut group = Group::new()
        .with_class(get_node_classes(&node, None))
        .with_source(SourceRef::node(node.id.clone()));
    group.items.push(Item::Group(anchor));

    node.intersect = IntersectKind::Circle;
    Ok(RenderedNode { node, group })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_upstream_classic_fixture() {
        crate::render::shapes::double_circle::fixture::check(
            include_str!("../testdata/shapes/anchor.json"),
            draw,
        );
    }
}
