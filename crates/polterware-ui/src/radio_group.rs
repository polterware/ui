//! Single-choice radio rows.

use gpui::{App, IntoElement, SharedString, div, prelude::*, px};

use crate::ValueHandler;
use crate::theme::Theme;

#[derive(Clone, Debug)]
pub struct RadioOption {
    pub label: SharedString,
}

pub fn radio_group(
    group_id: &'static str,
    options: &[RadioOption],
    selected: usize,
    on_select: ValueHandler<usize>,
    cx: &App,
) -> impl IntoElement {
    let theme = *Theme::global(cx);
    let mut column = div().flex().flex_col().gap(px(theme.space_1));
    for (index, option) in options.iter().enumerate() {
        let handler = on_select.clone();
        let on = index == selected;
        let mark = if on {
            theme.color_primary
        } else {
            theme.color_surface
        };
        let border = if on {
            theme.color_primary
        } else {
            theme.color_border_strong
        };
        column = column.child(
            div()
                .id((group_id, index))
                .h(px(theme.row_height))
                .px(px(theme.space_2))
                .flex()
                .items_center()
                .gap(px(theme.space_2))
                .rounded(px(theme.radius_sm))
                .cursor_pointer()
                .hover(move |style| style.bg(theme.color_surface_hover))
                .on_click(move |_, window, cx| handler(index, window, cx))
                .child(
                    div()
                        .size(px(14.0))
                        .flex_none()
                        .rounded_full()
                        .border_1()
                        .border_color(border)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(div().size(px(8.0)).rounded_full().bg(mark)),
                )
                .child(
                    div()
                        .text_size(px(theme.font_size_sm))
                        .text_color(theme.color_text_primary)
                        .child(option.label.clone()),
                ),
        );
    }
    column
}
