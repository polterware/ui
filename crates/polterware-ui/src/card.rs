//! `Card` (`.card-panel`): the raised host surface, `--color-surface-subtle` with the quiet
//! edge and the raised drop at `--radius-card`.
//!
//! `Card variant="texture"` (`.card-texture-framed`, `materials.css`): a framed face inside a
//! 1px `--field-frame` border (transparent in the dark theme). [`texture_card`] is the large
//! content surface: the face from `--color-surface` to 85% of it over `--color-surface-subtle`
//! at the card radius, with the card rings (the quiet edge) and a `0 1px 2px` drop.
//! [`texture_compact`] (`texture="compact"`) is the field gradient (`--field-fill-top` to
//! `--field-fill-bottom`) at 12px with the control's edge and drop (`--material-control`).

use gpui::{App, Div, Rgba, div, prelude::*, px};

use super::material::{Edge, control_drop, drop, edge, face, raised_panel};
use crate::theme::Theme;

pub fn card(cx: &App) -> Div {
    let theme = Theme::global(cx);
    raised_panel(
        div().relative().bg(theme.color_surface_subtle),
        theme.radius_card,
        theme,
    )
}

/// `color-mix(in srgb, a <share>, b)`.
fn mix(a: Rgba, b: Rgba, share: f32) -> Rgba {
    Rgba {
        r: a.r * share + b.r * (1.0 - share),
        g: a.g * share + b.g * (1.0 - share),
        b: a.b * share + b.b * (1.0 - share),
        a: a.a * share + b.a * (1.0 - share),
    }
}

/// `Card variant="texture"`: the framed content face. Add children after it; the edge is its
/// first child, under them.
pub fn texture_card(cx: &App) -> Div {
    let theme = Theme::global(cx);
    let radius = theme.radius_card;
    div()
        .relative()
        .rounded(px(radius))
        .bg(face(
            theme.color_surface,
            mix(theme.color_surface, theme.color_surface_subtle, 0.85),
        ))
        .shadow(vec![drop(1.0, 2.0, theme.material_drop)])
        .child(edge(Edge::Quiet, radius, theme))
}

/// `Card variant="texture" texture="compact"`: the field gradient at 12px with the control's
/// edge and drop.
pub fn texture_compact(cx: &App) -> Div {
    let theme = Theme::global(cx);
    div()
        .relative()
        .rounded(px(12.0))
        .bg(face(theme.field_fill_top, theme.field_fill_bottom))
        .shadow(control_drop(theme))
        .child(edge(Edge::Strong, 12.0, theme))
}
