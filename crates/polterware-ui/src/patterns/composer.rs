//! The Composer's frame: the dialog every new or edited record opens in. The title comes
//! first, then the fields the app adds, then the actions at the end. It opens on a
//! [`modal_layer`](super::modal_layer).

use gpui::{AnyElement, Div, FontWeight, MouseButton, SharedString, div, prelude::*, px};

use crate::theme::Theme;

/// The Composer's width.
pub const COMPOSER_WIDTH: f32 = 520.0;
/// The tallest the Composer grows before its fields scroll.
pub const COMPOSER_MAX_HEIGHT: f32 = 640.0;

/// The Composer card with its `title`. `presence` (0 to 1) fades it in and `offset_y` is the
/// rise still to go (8px when it starts opening, 0 when it is open). Add the fields with
/// `child`, then [`composer_actions`].
pub fn composer_card(
    title: impl Into<SharedString>,
    presence: f32,
    offset_y: f32,
    theme: &Theme,
) -> Div {
    div()
        .w(px(COMPOSER_WIDTH))
        .max_h(px(COMPOSER_MAX_HEIGHT))
        .flex()
        .flex_col()
        .gap(px(theme.space_3))
        .p(px(theme.space_5))
        .rounded(px(theme.radius_dialog))
        .bg(theme.color_surface_raised)
        .border_1()
        .border_color(theme.color_border)
        .mt(px(offset_y))
        .opacity(presence.clamp(0.0, 1.0))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .text_size(px(theme.font_size_lg))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.into()),
        )
}

/// The Composer's last row: its buttons at the end, in order (the primary one last).
pub fn composer_actions(actions: impl IntoIterator<Item = AnyElement>, theme: &Theme) -> Div {
    div()
        .flex()
        .justify_end()
        .gap(px(theme.space_2))
        .children(actions)
}
