//! Classic port of upstream `shapes/note.ts`.
//!
//! A rectangle sized from the label plus twice the padding, filled with `noteBkgColor` and
//! stroked with `noteBorderColor`. When labels are SVG text upstream sets `centerLabel`, so
//! the label group is centered on both axes; the rect carries the `basic label-container
//! outer-path` classes and the label `label noteLabel`, which the scene keeps on the groups
//! it can (rects have no class of their own).
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare};
use crate::scene::{Color, Item, PathStyle, Point, Rect, RectShape, Size, SourceRef, Transform};

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, mut label, text_size) = prepare(&node, ctx)?;
    label.class = Some("label noteLabel".to_string());
    label.transform = Transform::translate(-text_size.width / 2.0, -text_size.height / 2.0);

    let total_width = (text_size.width + node.padding * 2.0).max(node.width.unwrap_or(0.0));
    let total_height = (text_size.height + node.padding * 2.0).max(node.height.unwrap_or(0.0));
    node.bounds = Rect::centered(Point::ZERO, Size::new(total_width, total_height));

    // handDrawn: rough.js (later)
    let background = ctx.theme.color("noteBkgColor").unwrap_or("#fff5ad");
    let border = ctx.theme.color("noteBorderColor").unwrap_or("#9370DB");
    let style = PathStyle {
        fill: Some(Color::new(background)),
        stroke: Some(Color::new(border)),
        stroke_width: 1.3,
        ..Default::default()
    };

    group.push(
        RectShape::new(node.bounds, 0.0, style).with_source(SourceRef::node(node.id.clone())),
    );
    group.items.push(Item::Group(label));

    node.intersect = IntersectKind::Rect;
    Ok(RenderedNode { node, group })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_upstream_classic_fixture() {
        crate::render::shapes::double_circle::fixture::check(
            include_str!("../testdata/shapes/note.json"),
            draw,
        );
    }
}
