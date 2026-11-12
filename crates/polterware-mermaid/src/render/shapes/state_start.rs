//! Classic port of upstream `shapes/stateStart.ts`.
//!
//! A 14px (at least) filled circle with no label. Classic rendering inserts a plain
//! `<circle class="state-start">`; the state styles paint it with `specialStateColor`. The
//! small-state drop-shadow filter is a CSS effect with no scene equivalent and is omitted.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError};
use crate::scene::{Color, EllipseShape, Group, PathStyle, Point, Rect, Size, SourceRef};

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
    let radius = width / 2.0;
    node.bounds = Rect::centered(Point::ZERO, Size::new(width, width));

    let fill = ctx
        .theme
        .color("specialStateColor")
        .or_else(|| ctx.theme.color("lineColor"))
        .unwrap_or("#333333");
    let style = PathStyle {
        fill: Some(Color::new(fill)),
        stroke: Some(Color::new(fill)),
        ..Default::default()
    };

    // `stateStart` always uses the fixed `node default` classes, not the node's classes.
    let mut group = Group::new()
        .with_class("node default")
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
            include_str!("../testdata/shapes/state_start.json"),
            draw,
        );
    }
}
