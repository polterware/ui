//! Small status dot, in the tone the app gives its status (see [`crate::theme::status_tone`]).

use gpui::{IntoElement, div, prelude::*, px};

use crate::theme::status_color;

pub fn status_dot(status: &str) -> impl IntoElement {
    div()
        .size(px(7.0))
        .flex_none()
        .rounded_full()
        .bg(status_color(status))
}

/// `StatusDot` with its label (`status.tsx`): the tone's 6px dot (`h-1.5 w-1.5` on the 13px
/// root) and the label in `text-xs` secondary ink, `gap-1.5` apart.
pub fn status_dot_label(
    tone: crate::theme::StatusTone,
    label: impl Into<gpui::SharedString>,
    cx: &gpui::App,
) -> gpui::Div {
    use crate::theme::{Theme, tw};
    let theme = Theme::global(cx);
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(tw(1.5)))
        .text_size(px(tw(3.0)))
        .text_color(theme.color_text_secondary)
        .whitespace_nowrap()
        .child(
            div()
                .size(px(tw(1.5)))
                .flex_none()
                .rounded_full()
                .bg(crate::theme::tone_color(theme, tone)),
        )
        .child(label.into())
}
