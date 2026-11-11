//! Upstream `shapes/c4LabelHelper.ts`.
use super::{Node, RenderedNode, ShapeContext, ShapeError};
use crate::measure::{TextOptions, shape_text};
use crate::scene::Size;

pub const SECTION_GAP: f64 = 3.0;
pub const MIN_WRAP_WIDTH: f64 = 32.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LabelSection {
    pub text: String,
    pub class: &'static str,
}

pub fn sections(name: &str, stereotype: Option<&str>, description: &[String]) -> Vec<LabelSection> {
    let mut out = Vec::new();
    for (text, class) in std::iter::once((name, "c4-name"))
        .chain(stereotype.into_iter().map(|s| (s, "c4-type")))
        .chain(description.iter().map(|s| (s.as_str(), "c4-descr")))
    {
        if !text.is_empty() {
            out.push(LabelSection {
                text: text.to_owned(),
                class,
            });
        }
    }
    out
}

pub fn wrap_width(node: &Node, fallback: f64) -> f64 {
    node.width
        .map(|w| (w - 2.0 * node.padding).max(MIN_WRAP_WIDTH))
        .unwrap_or(fallback)
}

pub fn measure_sections(
    sections: &[LabelSection],
    ctx: &ShapeContext<'_>,
    width: f64,
) -> Result<Size, crate::measure::TextError> {
    let mut total = Size::ZERO;
    for section in sections {
        let text = shape_text(
            &section.text,
            &TextOptions {
                width,
                markdown: false,
                min_width: 0.0,
            },
            &crate::measure::Font::default(),
            ctx.measure,
        )?;
        let size = text.size;
        total.width = total.width.max(size.width);
        total.height += size.height;
    }
    if !sections.is_empty() {
        total.height += SECTION_GAP * (sections.len() - 1) as f64;
    }
    Ok(total)
}

// Compatibility entry for the current registry; this upstream module is a helper, not a shape.
pub fn draw(node: &Node, _ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    Err(ShapeError {
        message: format!("{} is a rendering helper, not a shape", node.shape),
    })
}
