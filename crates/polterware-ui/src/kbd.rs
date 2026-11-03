//! `Kbd` (`.pw-kbd`): a key chip, 16px high with 4px sides and radius 4, on the material face
//! (`--material-face-top` to `--material-face-bottom`) with the well's 0.5px ring, in the
//! monospace at 10px, weight 550, primary. GPUI has no letter spacing, so the CSS's 0.02em
//! (0.2px) tracking is left out.

use gpui::{FontWeight, SharedString, div, prelude::*, px};

use super::material::{Edge, edge, face};
use crate::theme::Theme;

pub fn kbd(label: impl Into<SharedString>, theme: &Theme) -> gpui::Div {
    div()
        .relative()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .min_h(px(16.0))
        .px(px(4.0))
        .rounded(px(4.0))
        .bg(face(theme.material_face_top, theme.material_face_bottom))
        .font_family(theme.font_mono)
        .text_size(px(10.0))
        .font_weight(FontWeight(550.0))
        .line_height(px(10.0))
        .text_color(theme.color_text_primary)
        .child(label.into())
        .child(edge(Edge::Well, 4.0, theme))
}
