//! Upstream `shapes/handDrawnShapeStyles.ts`; classic rendering uses these compiled CSS roles.
use std::collections::BTreeMap;

/// Resolves style declarations in order, with later declarations overriding earlier ones.
pub fn styles2_map(styles: &[String]) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    for style in styles {
        if let Some((key, value)) = style.split_once(':') {
            result.insert(key.trim().to_owned(), value.trim().to_owned());
        }
    }
    result
}

pub fn normalize_style_list(styles: Option<&str>) -> Vec<String> {
    styles
        .unwrap_or_default()
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

pub fn is_label_style(key: &str) -> bool {
    matches!(
        key,
        "color"
            | "font-size"
            | "font-family"
            | "font-weight"
            | "font-style"
            | "text-decoration"
            | "text-align"
            | "text-transform"
            | "line-height"
            | "letter-spacing"
            | "word-spacing"
            | "text-shadow"
            | "text-overflow"
            | "white-space"
            | "word-wrap"
            | "word-break"
            | "overflow-wrap"
            | "hyphens"
    )
}

pub fn stroke_dash_array(style: Option<&str>) -> [f64; 2] {
    let Some(style) = style else {
        return [0.0, 0.0];
    };
    let mut values = style
        .split_whitespace()
        .take(2)
        .map(|v| v.parse::<f64>().unwrap_or(0.0));
    let first = values.next().unwrap_or(0.0);
    [first, values.next().unwrap_or(first)]
}

use super::{Node, RenderedNode, ShapeContext, ShapeError};

// Compatibility entry for the current registry; this upstream module is a helper, not a shape.
pub fn draw(node: &Node, _ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    Err(ShapeError {
        message: format!("{} is a rendering helper, not a shape", node.shape),
    })
}
