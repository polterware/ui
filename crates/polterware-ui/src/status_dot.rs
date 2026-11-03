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
