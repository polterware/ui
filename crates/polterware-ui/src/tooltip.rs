//! Opaque tooltip label. Pass [`tooltip`] to an element's `tooltip` slot.

use gpui::{AnyView, App, Context, IntoElement, Render, SharedString, Window, div, prelude::*, px};

use crate::theme::Theme;

pub struct TooltipView {
    label: SharedString,
}

impl Render for TooltipView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::global(cx);
        div()
            .px(px(theme.space_2))
            .py(px(theme.space_1))
            .rounded(px(theme.radius_sm))
            .bg(theme.color_surface_raised)
            .border_1()
            .border_color(theme.color_border)
            .text_size(px(theme.font_size_xs))
            .text_color(theme.color_text_primary)
            .child(self.label.clone())
    }
}

pub fn tooltip(
    label: impl Into<SharedString>,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let label = label.into();
    move |_window, cx| {
        cx.new(|_| TooltipView {
            label: label.clone(),
        })
        .into()
    }
}
