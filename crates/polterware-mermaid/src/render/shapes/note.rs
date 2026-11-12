//! Classic port of upstream `shapes/note.ts`.
//!
//! A rectangle sized from the label plus twice the padding, filled with `noteBkgColor` and
//! stroked with `noteBorderColor`. When labels are SVG text upstream sets `centerLabel`, so
//! the label group is centered on both axes; the rect carries the `basic label-container
//! outer-path` classes and the label `label noteLabel`, which the scene keeps on the groups
//! it can (rects have no class of their own).
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, get_node_classes};
use crate::measure::Font;
use crate::render::labels::{LabelOptions, create_label};
use crate::scene::{
    Color, Group, Item, NORMAL_WEIGHT, PathStyle, Point, Rect, RectShape, Size, SourceRef,
    Transform,
};

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let source = SourceRef::node(node.id.clone());
    let mut group = Group::new()
        .with_class(get_node_classes(&node, None))
        .with_source(source.clone());
    let font_size = ctx
        .theme
        .color("fontSize")
        .and_then(|s| s.trim_end_matches("px").parse::<f64>().ok())
        .or_else(|| ctx.theme.number("fontSize"))
        .unwrap_or(12.0);
    let font_family = ctx.theme.color("fontFamily").unwrap_or("Arial").to_string();
    let font = Font {
        family: font_family,
        size: font_size,
        weight: NORMAL_WEIGHT,
        italic: false,
    };
    // `labelHelper`: markdown notes wrap at the explicit width, the node's wrapping width,
    // or the diagram's `wrappingWidth` (which the layout data copies onto the note).
    let width = node.width.or(node.wrapping_width).unwrap_or(f64::INFINITY);
    let mut label = create_label(
        &LabelOptions {
            text: node.label.clone(),
            font,
            markdown: node.label_type.as_deref() == Some("markdown"),
            width,
            centered: false,
            class: "label".into(),
            source: Some(source.clone()),
        },
        ctx.measure,
    )
    .map_err(|error| ShapeError {
        message: error.to_string(),
    })?;
    let text_size = match label.items.first() {
        Some(Item::Text(text)) => text.size,
        _ => Size::ZERO,
    };
    // `labelHelper`'s `withMinWidth`: a label narrower than the diagram's minimum is
    // widened (and stays centred), unless the node has an explicit width.
    let min_width = if node.label.is_empty() || node.width.is_some() {
        0.0
    } else {
        node.min_width.unwrap_or(0.0)
    };
    let text_size = Size::new(text_size.width.max(min_width), text_size.height);
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
