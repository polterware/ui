//! Upstream `shapes/labelImageUtils.ts`.

/// Describes whether a label tree contains non-whitespace text apart from image elements.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LabelContent<'a> {
    Text(&'a str),
    Image,
    Other(Vec<LabelContent<'a>>),
}

pub fn has_text_besides_images(content: &LabelContent<'_>) -> bool {
    match content {
        LabelContent::Text(text) => !text.trim().is_empty(),
        LabelContent::Image => false,
        LabelContent::Other(children) => children.iter().any(has_text_besides_images),
    }
}

/// Image sizing selected by `configureLabelImages` after load/error completion.
pub fn image_width(no_text: bool, font_size: f64) -> ImageWidth {
    if no_text {
        ImageWidth::Fixed(5.0 * font_size)
    } else {
        ImageWidth::Container
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImageWidth {
    Fixed(f64),
    Container,
}

/// The image styles written by upstream `configureLabelImages` once an image loads or fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageLayout {
    pub display: &'static str,
    pub flex_direction: &'static str,
    pub width_percent: Option<u8>,
    pub min_width_px: Option<u32>,
    pub max_width_px: Option<u32>,
}

/// Resolves the CSS decisions made after image completion. A DOM adapter applies this value.
pub fn configured_image_layout(no_text: bool, font_size: f64) -> ImageLayout {
    match image_width(no_text, font_size) {
        ImageWidth::Fixed(width) => ImageLayout {
            display: "flex",
            flex_direction: "column",
            width_percent: None,
            min_width_px: Some(width.round() as u32),
            max_width_px: Some(width.round() as u32),
        },
        ImageWidth::Container => ImageLayout {
            display: "flex",
            flex_direction: "column",
            width_percent: Some(100),
            min_width_px: None,
            max_width_px: None,
        },
    }
}

use super::{Node, RenderedNode, ShapeContext, ShapeError};

// Compatibility entry for the current registry; this upstream module is a helper, not a shape.
pub fn draw(node: &Node, _ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    Err(ShapeError {
        message: format!("{} is a rendering helper, not a shape", node.shape),
    })
}
