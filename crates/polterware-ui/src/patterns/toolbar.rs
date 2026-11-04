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

/// [`toolbar`] with a `center` held at the bar's middle whatever the two sides hold, as a
/// window title sits (a page's name, or the control that opens it). The center takes clicks;
/// the empty bar around it still drags the window.
pub fn toolbar_center<S: IntoElement, C: IntoElement, E: IntoElement>(
    start: S,
    center: C,
    end: E,
    cx: &App,
) -> impl IntoElement + use<S, C, E> {
    let theme = Theme::global(cx);
    div()
        .relative()
        .w_full()
        .flex_none()
        .child(toolbar(start, end, cx))
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(px(theme.space_2))
                .bottom(px(theme.space_1))
                .flex()
                .items_center()
                .justify_center()
                .child(div().flex_none().child(center)),
        )
}
