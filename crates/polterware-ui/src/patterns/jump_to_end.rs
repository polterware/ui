//! The button that jumps a long, growing log (a run's transcript, a chat) back to its end.
//!
//! It is the toolbar's secondary icon button (32px, the down arrow) floating over the log on
//! the float shadow. It shows only while the log is scrolled away from its end: the app
//! springs a `presence` from 0 to 1 as the reader leaves the end and back as they return,
//! and the button fades in while it rises 10px into place, and fades out as it sinks. Once
//! it is gone it draws nothing, so it never takes a click meant for the text.
//!
//! [`JUMP_TO_END_REST`] is how far from the end the log counts as at its end.

use gpui::{
    AnyElement, App, ElementId, IntoElement, ParentElement, SharedString, div, prelude::*, px,
};

use crate::Handler;
use crate::button::{Button, ButtonKind, ButtonSize};
use crate::icons::Icon;
use crate::material::float_drop;
use crate::theme::Theme;

/// Within this many px of its end, a log is at its end and the button leaves.
pub const JUMP_TO_END_REST: f32 = 48.0;
/// How far the button rises as it comes in.
const RISE: f32 = 10.0;

/// The button `presence` (0 to 1) of the way in, or nothing once it is gone. Place it at the
/// bottom center of the log, 16px up.
pub fn jump_to_end(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    presence: f32,
    on_click: Handler,
    cx: &App,
) -> Option<AnyElement> {
    let t = presence.clamp(0.0, 1.0);
    if t < 0.01 {
        return None;
    }
    let theme = Theme::global(cx);
    Some(
        div()
            .relative()
            .top(px((1.0 - t) * RISE))
            .opacity(t)
            .rounded_full()
            .shadow(float_drop(theme))
            .child(
                Button::new(id, ButtonKind::Secondary)
                    .size(ButtonSize::Icon)
                    .icon(Icon::ArrowDown)
                    .tooltip(label)
                    .on_click_rc(on_click),
            )
            .into_any_element(),
    )
}
