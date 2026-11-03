//! `PageToolbar` (`.pw-page-toolbar`): a page's first row, a plain bar with no title, border,
//! or fade. It is 44px tall with 8px 8px 4px padding: the leading group (8px gaps) takes the
//! free width, the actions (8px gaps) sit at its end, 12px apart. The empty part of the bar
//! drags the window.

use crate::window_drag::WindowDragRegion;
use gpui::{App, IntoElement, div, prelude::*, px};

use crate::theme::Theme;

pub fn toolbar<S: IntoElement, E: IntoElement>(
    start: S,
    end: E,
    cx: &App,
) -> impl IntoElement + use<S, E> {
    let theme = Theme::global(cx);
    div()
        .h(px(theme.page_header_height))
        .w_full()
        .flex_none()
        .pt(px(theme.space_2))
        .px(px(theme.space_2))
        .pb(px(theme.space_1))
        .flex()
        .items_center()
        .gap(px(theme.space_3))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .h_full()
                .flex()
                .items_center()
                .gap(px(theme.space_2))
                .child(start)
                .child(div().flex_1().h_full().window_drag_region()),
        )
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(theme.space_2))
                .child(end),
        )
}
