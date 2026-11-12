//! Classic port of upstream `shapes/forkJoin.ts`.
//!
//! A filled bar at least 70x10, centered on the node. Classic rendering keeps rough.js's
//! solid fill and roughness 0. The shape handler reads the graph direction from its render
//! options; this crate's [`ShapeContext`] carries no direction yet, so the port always takes
//! the top-to-bottom branch (the upstream default).
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, get_node_classes};
use crate::scene::{Color, Group, PathStyle, Point, Rect, RectShape, Size, SourceRef};

/// Upstream's default `state.padding`. `forkJoin` grows its intersection box by half of it
/// after measuring the bar; `ShapeContext` carries no diagram config, so the default is used.
const STATE_PADDING: f64 = 8.0;

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    node.label.clear();

    // handDrawn: rough.js (later)
    let width = 70.0_f64.max(node.width.unwrap_or(0.0));
    let height = 10.0_f64.max(node.height.unwrap_or(0.0));
    let x = -width / 2.0;
    let y = -height / 2.0;
    let rect = Rect::new(x, y, width, height);
    node.bounds = Rect::centered(
        Point::ZERO,
        Size::new(width + STATE_PADDING / 2.0, height + STATE_PADDING / 2.0),
    );

    let line = ctx.theme.color("lineColor").unwrap_or("#333333");
    let style = PathStyle {
        fill: Some(Color::new(line)),
        stroke: Some(Color::new(line)),
        stroke_width: 1.3,
        ..Default::default()
    };

    let mut group = Group::new()
        .with_class(get_node_classes(&node, None))
        .with_source(SourceRef::node(node.id.clone()));
    group.push(RectShape::new(rect, 0.0, style).with_source(SourceRef::node(node.id.clone())));

    node.intersect = IntersectKind::Rect;
    Ok(RenderedNode { node, group })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_upstream_classic_fixture() {
        crate::render::shapes::double_circle::fixture::check(
            include_str!("../testdata/shapes/fork_join.json"),
            draw,
        );
    }
}
