//! Port of upstream `shapes/imageSquare.ts`.
//!
//! An image rectangle plus a label, with a transparent outer box that owns the node bounds.
//! Upstream sizes the image from the image's natural size or `assetWidth`/`assetHeight` and
//! wraps it at the flowchart wrapping width; the crate's `Node` carries neither the image
//! bytes nor its URL, so this port uses the documented defaults: a 48×48 asset and the default
//! 120px wrapping width. The image itself is not emitted (a painter draws it from its own
//! data); the classic geometry is the two rectangles and the attachment polygon.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, shape_style};
use crate::measure::Font;
use crate::render::labels::{LabelOptions, create_label};
use crate::scene::{Group, Item, PathStyle, Point, Rect, RectShape, Size, SourceRef, Transform};

/// `node.assetWidth ?? 48` and `node.assetHeight ?? 48`.
const ASSET_SIZE: f64 = 48.0;
/// `flowchart.wrappingWidth`'s default in mermaid 12.3.0.
const DEFAULT_WRAPPING_WIDTH: f64 = 120.0;
/// `labelHelper`'s background rectangle is the label box plus two pixels on every side.
const LABEL_BACKGROUND_PADDING: f64 = 2.0;
/// `const labelPadding = node.label ? 8 : 0`.
const LABEL_PADDING: f64 = 8.0;

fn text_size(label: &Group) -> Size {
    match label.items.first() {
        Some(Item::Text(text)) => text.size,
        _ => Size::ZERO,
    }
}

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let wrap_w = DEFAULT_WRAPPING_WIDTH.max(node.asset_width.unwrap_or(0.0));
    let font_size = ctx
        .theme
        .color("fontSize")
        .and_then(|s| s.trim_end_matches("px").parse::<f64>().ok())
        .or_else(|| ctx.theme.number("fontSize"))
        .unwrap_or(Font::default().size);
    let font_family = ctx
        .theme
        .color("fontFamily")
        .map(|s| s.to_string())
        .unwrap_or_else(|| Font::default().family);
    let font = Font {
        family: font_family,
        size: font_size,
        weight: crate::scene::NORMAL_WEIGHT,
        italic: false,
    };
    let label = create_label(
        &LabelOptions {
            text: node.label.clone(),
            font,
            markdown: false,
            width: wrap_w,
            centered: false,
            class: "label".into(),
            source: Some(SourceRef::node(node.id.clone())),
        },
        ctx.measure,
    )
    .map_err(|e| ShapeError {
        message: e.to_string(),
    })?;
    let text = text_size(&label);
    let mut group = Group::new()
        .with_class("image-shape default")
        .with_source(SourceRef::node(node.id.clone()));
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
    let (image_width, image_height) = if node.img.is_some() {
        let iw = if node.label.is_empty() {
            node.asset_width.unwrap_or(ASSET_SIZE)
        } else {
            wrap_w.max(node.asset_width.unwrap_or(ASSET_SIZE))
        };
        let ih = node.asset_height.unwrap_or(ASSET_SIZE);
        (iw, ih)
    } else {
        let iw = if node.label.is_empty() {
            node.asset_width.unwrap_or(0.0)
        } else {
            wrap_w
        };
        let ih = node.asset_height.unwrap_or(0.0);
        (iw, ih)
    };
    let outer_width = image_width.max(bbox_width);
    let outer_height = image_height + bbox_height + label_padding;
    if !outer_width.is_finite() || !outer_height.is_finite() {
        return Err(ShapeError {
            message: "invalid image label dimensions".into(),
        });
    }

    let top_label = node.pos.as_deref() == Some("t");
    let icon_offset_y = if top_label {
        bbox_height / 2.0 + label_padding / 2.0
    } else {
        -bbox_height / 2.0 - label_padding / 2.0
    };
    // handDrawn: rough.js (later)
    let mut image = Group::new().with_transform(Transform::translate(0.0, icon_offset_y));
    image.push(
        RectShape::new(
            Rect::new(
                -image_width / 2.0,
                -image_height / 2.0,
                image_width,
                image_height,
            ),
            0.0,
            shape_style(),
        )
        .with_source(SourceRef::node(node.id.clone())),
    );
    group.push(Item::Group(image));
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
        -image_height / 2.0 - bbox_height / 2.0 - label_padding / 2.0
    } else {
        image_height / 2.0 - bbox_height / 2.0 + label_padding / 2.0
    };
    label.transform = Transform::translate(-bbox_width / 2.0, label_y);
    group.items.push(Item::Group(label));

    node.bounds = Rect::centered(Point::ZERO, Size::new(outer_width, outer_height));
    node.intersect = if node.label.is_empty() {
        IntersectKind::Rect
    } else {
        IntersectKind::Polygon(attachment_polygon(
            image_width,
            image_height,
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
    image_width: f64,
    image_height: f64,
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
                image_width / 2.0,
                -node_height / 2.0 + bbox_height + label_padding,
            ),
            Point::new(image_width / 2.0, node_height / 2.0),
            Point::new(-image_width / 2.0, node_height / 2.0),
            Point::new(
                -image_width / 2.0,
                -node_height / 2.0 + bbox_height + label_padding,
            ),
            Point::new(
                -bbox_width / 2.0,
                -node_height / 2.0 + bbox_height + label_padding,
            ),
        ]
    } else {
        vec![
            Point::new(-image_width / 2.0, -node_height / 2.0),
            Point::new(image_width / 2.0, -node_height / 2.0),
            Point::new(image_width / 2.0, -node_height / 2.0 + image_height),
            Point::new(bbox_width / 2.0, -node_height / 2.0 + image_height),
            Point::new(bbox_width / 2.0 / 2.0, node_height / 2.0),
            Point::new(-bbox_width / 2.0, node_height / 2.0),
            Point::new(-bbox_width / 2.0, -node_height / 2.0 + image_height),
            Point::new(-image_width / 2.0, -node_height / 2.0 + image_height),
        ]
    }
}

/// The outer sizing box is `fill: none, stroke: none`.
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
            serde_json::from_str(include_str!("../testdata/shapes/image_square.json")).unwrap();
        for case in &fixture.cases {
            assert_case(case, draw);
        }
    }
}
