//! The canvas editor's floating docks (`.pw-canvas-dock` on `Card variant="texture"
//! texture="compact"`): editors group their tools in a dock over the canvas, not a crowded
//! header. A dock is a pill of the field gradient inside a 1px transparent frame, 3px in, its
//! controls 2px apart, lifted by `0 8px 24px --color-shadow-float` (which replaces the
//! compact card's own edge and drop). Groups are split by a 16px `--color-border-subtle` rule
//! 2px from each side; the zoom level reads in 11px secondary tabular figures, 38px wide.

use gpui::{App, Div, FontWeight, IntoElement, SharedString, div, prelude::*, px};

use crate::data_table::{sans, tabular};
use crate::material::{drop, face};
use crate::theme::Theme;

/// An empty dock; position it over the canvas and add its controls.
pub fn canvas_dock(cx: &App) -> Div {
    let theme = Theme::global(cx);
    div()
        .flex()
        .items_center()
        .gap(px(2.0))
        .p(px(4.0))
        .rounded_full()
        .bg(face(theme.field_fill_top, theme.field_fill_bottom))
        .shadow(vec![drop(8.0, 24.0, theme.color_shadow_float)])
}

/// A row of a dock's controls, 2px apart.
pub fn dock_group() -> Div {
    div().flex().items_center().gap(px(2.0))
}

/// The rule between a dock's groups.
pub fn dock_separator(cx: &App) -> impl IntoElement {
    div()
        .flex_none()
        .w(px(1.0))
        .h(px(16.0))
        .mx(px(2.0))
        .bg(Theme::global(cx).color_border_subtle)
}

/// The zoom level between a dock's zoom buttons, such as `100%`.
pub fn zoom_level(zoom: f32, cx: &App) -> impl IntoElement {
    let theme = Theme::global(cx);
    let label: SharedString = format!("{}%", (zoom * 100.0).round() as i32).into();
    div()
        .flex_none()
        .w(px(38.0))
        .flex()
        .justify_center()
        .text_size(px(11.0))
        .text_color(theme.color_text_secondary)
        .font(tabular(sans(theme, FontWeight::NORMAL)))
        .child(label)
}
