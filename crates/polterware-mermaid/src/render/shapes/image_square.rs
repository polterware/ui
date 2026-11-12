//! Port of upstream `shapes/imageSquare.ts`.
//!
//! An image rectangle plus a label, with a transparent outer box that owns the node bounds.
//! Upstream sizes the image from the image's natural size or `assetWidth`/`assetHeight` and
//! wraps it at the flowchart wrapping width; the crate's `Node` carries neither the image
//! bytes nor its URL, so this port uses the documented defaults: a 48×48 asset and the default
//! 120px wrapping width. The image itself is not emitted (a painter draws it from its own
//! data); the classic geometry is the two rectangles and the attachment polygon.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare, shape_style};
use crate::scene::{Group, Item, PathStyle, Point, Rect, RectShape, Size, SourceRef, Transform};

/// `node.assetWidth ?? 48` and `node.assetHeight ?? 48`.
const ASSET_SIZE: f64 = 48.0;
/// `flowchart.wrappingWidth`'s default in mermaid 12.3.0.
const DEFAULT_WRAPPING_WIDTH: f64 = 120.0;
/// `labelHelper`'s background rectangle is the label box plus two pixels on every side.
const LABEL_BACKGROUND_PADDING: f64 = 2.0;
/// `const labelPadding = node.label ? 8 : 0`.
const LABEL_PADDING: f64 = 8.0;

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, label, text) = prepare(&node, ctx)?;
    group.class = Some("image-shape default".into());
    let label_padding = if node.label.is_empty() {
        0.0
    } else {
        LABEL_PADDING
    };
    let bbox_width = text.width + LABEL_BACKGROUND_PADDING * 2.0;
    let bbox_height = text.height + LABEL_BACKGROUND_PADDING * 2.0;
    let image_width = if node.label.is_empty() {
        ASSET_SIZE
    } else {
        DEFAULT_WRAPPING_WIDTH.max(ASSET_SIZE)
    };
    let image_height = ASSET_SIZE;
    let outer_width = image_width.max(bbox_width);
    let outer_height = image_height + bbox_height + label_padding;
    if !outer_width.is_finite() || !outer_height.is_finite() {
        return Err(ShapeError {
            message: "invalid image label dimensions".into(),
        });
    }

    // handDrawn: rough.js (later)
    let mut image = Group::new().with_transform(Transform::translate(
        0.0,
        -bbox_height / 2.0 - label_padding / 2.0,
    ));
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
    label.transform = Transform::translate(
        -bbox_width / 2.0,
        image_height / 2.0 - bbox_height / 2.0 + label_padding / 2.0,
    );
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
) -> Vec<Point> {
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
