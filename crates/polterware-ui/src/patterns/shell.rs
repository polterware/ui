//! The app frame's surfaces: the main surface inset over the shell, and the layer a modal
//! (Commander, a Composer) opens on.

use gpui::{Div, IntoElement, MouseButton, div, prelude::*, px};

use crate::Handler;
use crate::material::{Edge, edge, face, raised_drop};
use crate::theme::{Theme, hex};

/// `MainSurfaceFrame`: the inset card over the shell, `linear-gradient(to bottom,
/// --color-surface, #1c1c1c)` with the raised drop, its quiet lit edge drawn above the page.
/// GPUI clips to rectangles, so the page keeps its corners clear itself.
pub fn main_surface(page: impl IntoElement, theme: &Theme) -> Div {
    div()
        .relative()
        .size_full()
        .rounded(px(theme.radius_surface))
        .bg(face(theme.color_surface, hex(0x1c1c1c)))
        .shadow(raised_drop(theme))
        .child(
            div()
                .size_full()
                .rounded(px(theme.radius_surface))
                .overflow_hidden()
                .child(page),
        )
        .child(edge(Edge::Quiet, theme.radius_surface, theme))
}

/// A full-window layer with the overlay scrim at `presence` (0 to 1) and `content` centered
/// on it. While `interactive`, the layer takes the pointer and a press on the scrim calls
/// `on_dismiss`; a press on the content stays on the content.
pub fn modal_layer(
    presence: f32,
    interactive: bool,
    on_dismiss: Handler,
    content: impl IntoElement,
    theme: &Theme,
) -> Div {
    let mut scrim = theme.color_overlay;
    scrim.a *= presence.clamp(0.0, 1.0);
    div()
        .absolute()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .absolute()
                .size_full()
                .bg(scrim)
                .when(interactive, |backdrop| {
                    backdrop
                        .occlude()
                        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                            cx.stop_propagation();
                            on_dismiss(window, cx);
                        })
                }),
        )
        .child(
            div()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(content),
        )
}
