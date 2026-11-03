//! Commander's frame (⌘K): the centered opaque palette with its input row and the scrolling
//! results under it. The app owns the query, the list, and the keys; the frame is the same in
//! every app.

use gpui::{
    App, CursorStyle, Div, ElementId, IntoElement, ScrollHandle, Stateful, Window, div, prelude::*,
    px,
};

use crate::Handler;
use crate::icon_button::icon_button;
use crate::icons::Icon;
use crate::theme::Theme;

/// The palette's width and height when the window has room for them.
pub const COMMANDER_WIDTH: f32 = 704.0;
pub const COMMANDER_HEIGHT: f32 = 416.0;
/// What the palette keeps clear of the window's edges when the window is small.
pub const COMMANDER_INSET: f32 = 40.0;

/// The palette's size in `window`: [`COMMANDER_WIDTH`] by [`COMMANDER_HEIGHT`], smaller in a
/// small window, never under 320 by 240.
pub fn commander_size(window: &Window) -> (f32, f32) {
    let viewport = window.viewport_size();
    let width = COMMANDER_WIDTH.min((f32::from(viewport.width) - COMMANDER_INSET).max(320.0));
    let height = COMMANDER_HEIGHT.min((f32::from(viewport.height) - COMMANDER_INSET).max(240.0));
    (width, height)
}

/// The palette: `input` in its top row with Close at the right, `results` scrolling under it
/// on `scroll`. `presence` (0 to 1) fades it in and lifts it the last 8px. The caller adds
/// its key context, focus, and actions to the returned element.
#[allow(clippy::too_many_arguments)]
pub fn commander_panel(
    id: impl Into<ElementId>,
    (width, height): (f32, f32),
    presence: f32,
    input: impl IntoElement,
    results: impl IntoElement,
    scroll: &ScrollHandle,
    on_close: Handler,
    theme: &Theme,
    cx: &App,
) -> Stateful<Div> {
    let t = presence.clamp(0.0, 1.0);
    div()
        .id(id)
        .occlude()
        .w(px(width))
        .h(px(height))
        .mt(px((1.0 - t) * 8.0))
        .opacity(t)
        .flex()
        .flex_col()
        .overflow_hidden()
        .rounded(px(theme.radius_dialog))
        .bg(theme.color_surface_subtle)
        .shadow_xl()
        .text_color(theme.color_text_primary)
        .font_family(theme.font_sans)
        .cursor(CursorStyle::IBeam)
        .child(
            div()
                .relative()
                .flex_none()
                .pt(px(theme.space_5))
                .px(px(theme.space_6))
                .flex()
                .items_center()
                .child(input)
                .child(
                    div()
                        .absolute()
                        .top(px(theme.space_4))
                        .right(px(theme.space_4))
                        .cursor(CursorStyle::PointingHand)
                        .child(icon_button(
                            "commander-close",
                            Icon::Close,
                            "Close",
                            on_close,
                            cx,
                        )),
                ),
        )
        .child(
            div()
                .id("commander-results")
                .flex_1()
                .min_h(px(0.0))
                .pt(px(theme.space_2))
                .overflow_y_scroll()
                .track_scroll(scroll)
                .cursor(CursorStyle::PointingHand)
                .child(results),
        )
}
