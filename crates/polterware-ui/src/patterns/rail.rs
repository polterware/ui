//! Sidebar-style rail group and card.

use gpui::{App, Div, SharedString, div, prelude::*, px};

use crate::theme::Theme;

pub fn rail_group(title: impl Into<SharedString>, cx: &App) -> Div {
    let theme = Theme::global(cx);
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(theme.space_2))
        .child(
            div()
                .px(px(theme.space_1))
                .text_size(px(theme.font_size_xs))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme.color_text_muted)
                .child(title.into()),
        )
}

pub fn rail_card(cx: &App) -> Div {
    let theme = Theme::global(cx);
    div()
        .w_full()
        .p(px(theme.space_3))
        .rounded(px(theme.radius_card))
        .bg(theme.color_surface)
        .border_1()
        .border_color(theme.color_border)
}
