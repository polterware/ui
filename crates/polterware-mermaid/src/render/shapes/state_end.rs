//! Classic port of upstream `shapes/stateEnd.ts`.
//!
//! A 14px (at least) outer circle with a smaller filled inner circle, both inside the
//! `outer-path` group classic rendering emits. Upstream draws the inner circle with diameter
//! `width * 5 / 14`, so its radius is `width * 5 / 28`. The small-state drop-shadow filter
//! is a CSS effect with no scene equivalent and is omitted.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError};
use crate::scene::{Color, EllipseShape, Group, Item, PathStyle, Point, Rect, Size, SourceRef};

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();

    if node.width.is_some() || node.height.is_some() {
        if node.width.unwrap_or(0.0) < 14.0 {
            node.width = Some(14.0);
        }
        if node.height.unwrap_or(0.0) < 14.0 {
            node.height = Some(14.0);
        }
    }
    if node.width.is_none() {
        node.width = Some(14.0);
    }
    if node.height.is_none() {
        node.height = Some(14.0);
    }

    // handDrawn: rough.js (later)
    let width = node.width.unwrap_or(14.0);
    let outer_radius = width / 2.0;
    let inner_radius = (width * 5.0) / 28.0;
    node.bounds = Rect::centered(Point::ZERO, Size::new(width, width));

    let line = ctx.theme.color("lineColor").unwrap_or("#333333");
    let inner = ctx
        .theme
        .color("stateBorder")
        .or_else(|| ctx.theme.color("nodeBorder"))
        .unwrap_or("#9370DB");
    let background = ctx.theme.color("mainBkg").unwrap_or("#ECECFF");

    let outer_style = PathStyle {
        fill: Some(Color::new(background)),
        stroke: Some(Color::new(line)),
        stroke_width: 2.0,
        ..Default::default()
    };
    let inner_style = PathStyle {
        fill: Some(Color::new(inner)),
        stroke: Some(Color::new(inner)),
        stroke_width: 2.0,
        ..Default::default()
    };

    let mut outer = Group::new()
        .with_class("outer-path")
        .with_source(SourceRef::node(node.id.clone()));
    outer.push(
        EllipseShape::new(Point::ZERO, outer_radius, outer_radius, outer_style)
            .with_source(SourceRef::node(node.id.clone())),
    );
    outer.push(
        EllipseShape::new(Point::ZERO, inner_radius, inner_radius, inner_style)
            .with_source(SourceRef::node(node.id.clone())),
    );

    let mut group = Group::new()
        .with_class("node default")
        .with_source(SourceRef::node(node.id.clone()));
    group.items.push(Item::Group(outer));

    node.intersect = IntersectKind::Circle;
    Ok(RenderedNode { node, group })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_upstream_classic_fixture() {
        crate::render::shapes::double_circle::fixture::check(
            include_str!("../testdata/shapes/state_end.json"),
            draw,
        );
    }
}
