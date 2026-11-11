//! Upstream `shapes/bpmnIcons.ts`; classic scene ports can consume the original SVG glyph data.
use super::{Node, RenderedNode, ShapeContext, ShapeError};

pub const ICONS: &[(&str, &str)] = &[
    (
        "message",
        r#"<path fill="none" stroke="currentColor" stroke-width="1.6" d="M3 6h18v12H3z"/><path class="bpmn-glyph-fold" fill="none" stroke="currentColor" stroke-width="1.6" d="m3 6 9 7 9-7"/>"#,
    ),
    (
        "timer",
        r#"<circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" stroke-width="1.6"/><path fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" d="M12 7v5l3.5 2.5"/>"#,
    ),
    (
        "error",
        r#"<path fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" d="M4 21 9.5 6 13 12.5 20 4 14.5 19 11 12.5Z"/>"#,
    ),
    (
        "escalation",
        r#"<path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round" d="m12 3 8 18-8-7-8 7z"/>"#,
    ),
    (
        "signal",
        r#"<path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round" d="M12 4 22 20H2z"/>"#,
    ),
    (
        "conditional",
        r#"<path fill="none" stroke="currentColor" stroke-width="1.6" d="M4 4h16v16H4z"/><path stroke="currentColor" stroke-width="1.6" stroke-linecap="round" d="M7 9h10M7 12.5h10M7 16h10"/>"#,
    ),
];
pub fn icon_svg(name: &str) -> Option<&'static str> {
    ICONS
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, svg)| *svg)
}

// Compatibility entry for the current registry; this upstream module is a helper, not a shape.
pub fn draw(node: &Node, _ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    Err(ShapeError {
        message: format!("{} is a rendering helper, not a shape", node.shape),
    })
}
