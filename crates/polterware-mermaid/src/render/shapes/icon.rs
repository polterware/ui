//! Port of upstream `shapes/icon.ts`.
//!
//! An icon node is an icon asset in a square container plus a label below it (or above it for
//! `node.pos === 't'`, which this crate's node has no field for, so the default below is
//! used). Upstream paints the container and the outer sizing box with `stroke: none` and
//! `fill: none/transparent`, so the visible mark is the icon itself; the classic geometry here
//! is the square, the outer box, and the attachment polygon.
//!
//! The crate's `Node` carries no icon name, so no `ImageRef` is emitted: a painter draws the
//! icon from its own data. The icon's measured box is the default 48×48 asset size.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare};
use crate::scene::{Group, Item, PathStyle, Point, Rect, RectShape, Size, SourceRef, Transform};

/// Upstream's `node.assetWidth ?? 48` and `node.assetHeight ?? 48`.
const ICON_SIZE: f64 = 48.0;
/// `labelHelper`'s background rectangle is the label box plus two pixels on every side.
const LABEL_BACKGROUND_PADDING: f64 = 2.0;
/// `const labelPadding = node.label ? 8 : 0`.
const LABEL_PADDING: f64 = 8.0;

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, label, text) = prepare(&node, ctx)?;
    group.class = Some("icon-shape default".into());
    let (width, height) = if node.icon.is_some() {
        let asset_width = node.asset_width.unwrap_or(ICON_SIZE);
        let asset_height = node.asset_height.unwrap_or(ICON_SIZE);
        let icon_size = asset_width.max(asset_height);
        (icon_size, icon_size)
    } else {
        (0.0, 0.0)
    };
    let bg_pad = if node.icon.is_some() || node.img.is_some() {
        LABEL_BACKGROUND_PADDING
    } else {
        0.0
    };
    let bbox_width = text.width + bg_pad * 2.0;
    let bbox_height = text.height + bg_pad * 2.0;
    let label_padding = if node.label.is_empty() {
        0.0
    } else {
        LABEL_PADDING
    };
    let outer_width = width.max(bbox_width);
    let outer_height = height + bbox_height + label_padding;
    if !outer_width.is_finite() || !outer_height.is_finite() {
        return Err(ShapeError {
            message: "invalid icon label dimensions".into(),
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
        RectShape::new(
            Rect::new(-width / 2.0, -height / 2.0, width, height),
            0.0,
            invisible_style(),
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
    let label_y = if top_label {
        -outer_height / 2.0
    } else {
        outer_height / 2.0 - bbox_height
    };
    label.transform = Transform::translate(-bbox_width / 2.0, label_y);
    group.items.push(Item::Group(label));

    node.bounds = Rect::centered(Point::ZERO, Size::new(outer_width, outer_height));
    node.intersect = if node.label.is_empty() {
        IntersectKind::Rect
    } else {
        IntersectKind::Polygon(attachment_polygon(
            width,
            height,
            outer_height,
            bbox_width,
            top_label,
            label_padding,
            bbox_height,
        ))
    };
    Ok(RenderedNode { node, group })
}

/// Upstream's labeled attachment polygon, including its `bbox.width / 2 / 2` inner step.
fn attachment_polygon(
    width: f64,
    height: f64,
    node_height: f64,
    bbox_width: f64,
    top_label: bool,
    label_padding: f64,
    bbox_height: f64,
) -> Vec<Point> {
    if top_label {
        vec![
            Point::new(-bbox_width / 2.0, -node_height / 2.0),
            Point::new(bbox_width / 2.0, -node_height / 2.0),
            Point::new(
                bbox_width / 2.0,
                -node_height / 2.0 + bbox_height + label_padding,
            ),
            Point::new(
                width / 2.0,
                -node_height / 2.0 + bbox_height + label_padding,
            ),
            Point::new(width / 2.0, node_height / 2.0),
            Point::new(-width / 2.0, node_height / 2.0),
            Point::new(
                -width / 2.0,
                -node_height / 2.0 + bbox_height + label_padding,
            ),
            Point::new(
                -bbox_width / 2.0,
                -node_height / 2.0 + bbox_height + label_padding,
            ),
        ]
    } else {
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
}

/// Both container boxes are `stroke: none` and unfilled/transparent in the classic options.
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
            serde_json::from_str(include_str!("../testdata/shapes/icon.json")).unwrap();
        for case in &fixture.cases {
            assert_case(case, draw);
        }
    }
}
