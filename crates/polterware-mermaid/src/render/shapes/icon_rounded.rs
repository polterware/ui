//! Port of upstream `shapes/iconRounded.ts`.
//!
//! An icon asset in a five-radius rounded square sized from the asset plus the node's label
//! padding, a label below it, and a transparent outer box that owns the node bounds. The
//! square is filled and stroked with `mainBkg`, so the classic look is a solid rounded tile.
//! The tile group carries upstream's `icon-shape2` class.
//!
//! The crate's `Node` carries no icon name, so no `ImageRef` is emitted: a painter draws the
//! icon from its own data. The icon's measured box is the default 48×48 asset size.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare};
use crate::scene::{
    Color, Group, Item, Path, PathShape, PathStyle, Point, Rect, RectShape, Size, SourceRef,
    Transform,
};

/// Upstream's `node.assetWidth ?? 48` and `node.assetHeight ?? 48`.
const ICON_SIZE: f64 = 48.0;
/// `labelHelper`'s background rectangle is the label box plus two pixels on every side.
const LABEL_BACKGROUND_PADDING: f64 = 2.0;
/// `const labelPadding = node.label ? 8 : 0`.
const LABEL_PADDING: f64 = 8.0;
/// `createRoundedRectPathD(x, y, width, height, 5)`.
const CORNER_RADIUS: f64 = 5.0;

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, label, text) = prepare(&node, ctx)?;
    group.class = Some("icon-shape default".into());
    let half_padding = node.padding / 2.0;
    let label_padding = if node.label.is_empty() {
        0.0
    } else {
        LABEL_PADDING
    };
    let bbox_width = text.width + LABEL_BACKGROUND_PADDING * 2.0;
    let bbox_height = text.height + LABEL_BACKGROUND_PADDING * 2.0;
    let width = ICON_SIZE + half_padding * 2.0;
    let height = ICON_SIZE + half_padding * 2.0;
    let outer_width = width.max(bbox_width);
    let outer_height = height + bbox_height + label_padding;
    if !outer_width.is_finite() || !outer_height.is_finite() {
        return Err(ShapeError {
            message: "invalid icon rounded label dimensions".into(),
        });
    }

    let icon_rect = Rect::new(-width / 2.0, -height / 2.0, width, height);
    // handDrawn: rough.js (later)
    let mut icon = Group::new()
        .with_class("icon-shape2")
        .with_transform(Transform::translate(
            0.0,
            -bbox_height / 2.0 - label_padding / 2.0,
        ));
    icon.push(
        PathShape::new(
            Path::rect(icon_rect, CORNER_RADIUS, CORNER_RADIUS),
            disc_style(ctx),
        )
        .with_source(SourceRef::node(node.id.clone())),
    );
    group.push(Item::Group(icon));
    group.push(
        RectShape::new(
            Rect::centered(Point::ZERO, Size::new(outer_width, outer_height)),
            0.0,
            invisible_style(),
        )
        .with_source(SourceRef::node(node.id.clone())),
    );

    let mut label = label;
    label.transform = Transform::translate(-bbox_width / 2.0, outer_height / 2.0 - bbox_height);
    group.items.push(Item::Group(label));

    node.bounds = Rect::centered(Point::ZERO, Size::new(outer_width, outer_height));
    node.intersect = if node.label.is_empty() {
        IntersectKind::Rect
    } else {
        IntersectKind::Polygon(attachment_polygon(width, height, outer_height, bbox_width))
    };
    Ok(RenderedNode { node, group })
}

/// Upstream's labeled attachment polygon, including its `bbox.width / 2 / 2` inner step.
fn attachment_polygon(width: f64, height: f64, node_height: f64, bbox_width: f64) -> Vec<Point> {
    vec![
        Point::new(-width / 2.0, -node_height / 2.0),
        Point::new(width / 2.0, -node_height / 2.0),
        Point::new(width / 2.0, -node_height / 2.0 + height),
        Point::new(bbox_width / 2.0, -node_height / 2.0 + height),
        Point::new(bbox_width / 2.0 / 2.0, node_height / 2.0),
        Point::new(-bbox_width / 2.0, node_height / 2.0),
        Point::new(-bbox_width / 2.0, -node_height / 2.0 + height),
        Point::new(-width / 2.0, -node_height / 2.0 + height),
    ]
}

/// `options.stroke = fill ?? mainBkg` with `fill` from the node styles or `mainBkg`.
fn disc_style(ctx: &ShapeContext<'_>) -> PathStyle {
    let fill = Color::new(ctx.theme.color("mainBkg").unwrap_or("#ECECFF"));
    PathStyle {
        fill: Some(fill.clone()),
        stroke: Some(fill),
        ..Default::default()
    }
}

/// The outer sizing box is `fill: transparent, stroke: none`.
fn invisible_style() -> PathStyle {
    PathStyle {
        fill: None,
        stroke: None,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::shapes::label_image_utils::fixtures::{Fixture, assert_case};

    #[test]
    fn matches_upstream_classic_geometry() {
        let fixture: Fixture =
            serde_json::from_str(include_str!("../testdata/shapes/icon_rounded.json")).unwrap();
        for case in &fixture.cases {
            assert_case(case, draw);
        }
    }
}
