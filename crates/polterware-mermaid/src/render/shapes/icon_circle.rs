//! Port of upstream `shapes/iconCircle.ts`.
//!
//! An icon asset in a circle whose diameter fits the icon diagonally (`max(width, height) *
//! √2`) plus upstream's twenty-pixel padding, a label below it, and a transparent outer box
//! that owns the node bounds. Both the fill and the stroke are `mainBkg`, so the classic look
//! is a solid disc. Attachment is `intersect.rect` on the outer box.
//!
//! The crate's `Node` carries no icon name, so no `ImageRef` is emitted: a painter draws the
//! icon from its own data. The icon's measured box is the default 48×48 asset size.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare};
use crate::scene::{
    Color, EllipseShape, Group, Item, PathStyle, Point, Rect, RectShape, Size, SourceRef, Transform,
};

/// Upstream's `node.assetWidth ?? 48` and `node.assetHeight ?? 48`.
const ICON_SIZE: f64 = 48.0;
/// `labelHelper`'s background rectangle is the label box plus two pixels on every side.
const LABEL_BACKGROUND_PADDING: f64 = 2.0;
/// `const labelPadding = node.label ? 8 : 0`.
const LABEL_PADDING: f64 = 8.0;
/// `const padding = 20`.
const CIRCLE_PADDING: f64 = 20.0;

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, label, text) = prepare(&node, ctx)?;
    group.class = Some("icon-shape default".into());
    let label_padding = if node.label.is_empty() {
        0.0
    } else {
        LABEL_PADDING
    };
    let bg_pad = if node.icon.is_some() || node.img.is_some() {
        LABEL_BACKGROUND_PADDING
    } else {
        0.0
    };
    let bbox_width = text.width + bg_pad * 2.0;
    let bbox_height = text.height + bg_pad * 2.0;
    let (icon_width, icon_height) = if node.icon.is_some() {
        (
            node.asset_width.unwrap_or(ICON_SIZE),
            node.asset_height.unwrap_or(ICON_SIZE),
        )
    } else {
        (0.0, 0.0)
    };

    let diameter = icon_width.max(icon_height) * std::f64::consts::SQRT_2 + CIRCLE_PADDING * 2.0;
    let outer_width = diameter.max(bbox_width);
    let outer_height = diameter + bbox_height + label_padding;
    if !outer_width.is_finite() || !outer_height.is_finite() {
        return Err(ShapeError {
            message: "invalid icon circle label dimensions".into(),
        });
    }

    let top_label = node.pos.as_deref() == Some("t");
    let icon_offset_y = if top_label {
        bbox_height / 2.0 + label_padding / 2.0
    } else {
        -bbox_height / 2.0 - label_padding / 2.0
    };
    // handDrawn: rough.js (later)
    let mut icon = Group::new().with_transform(Transform::translate(0.0, icon_offset_y));
    icon.push(
        EllipseShape::new(Point::ZERO, diameter / 2.0, diameter / 2.0, disc_style(ctx))
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
    let label_y = if top_label {
        -outer_height / 2.0
    } else {
        outer_height / 2.0 - bbox_height
    };
    label.transform = Transform::translate(-bbox_width / 2.0, label_y);
    group.items.push(Item::Group(label));

    node.bounds = Rect::centered(Point::ZERO, Size::new(outer_width, outer_height));
    node.intersect = IntersectKind::Rect;
    Ok(RenderedNode { node, group })
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
            serde_json::from_str(include_str!("../testdata/shapes/icon_circle.json")).unwrap();
        for case in &fixture.cases {
            assert_case(case, draw);
        }
    }
}
