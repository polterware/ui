//! `PageEmpty`: `EmptyBot` (the mascot in a mood, 64px) over an 18px semibold title at
//! `leading-tight`, centered in the space it fills (`flex h-full min-h-[200px] w-full p-8`,
//! then `m-auto flex-col items-center gap-2 text-center`). The title sits `mt-2` lower and a
//! body `mt-4` lower, as wide as it needs up to `max-w-3xl`.

pub use bot_avatar::MascotMood;
use gpui::{AnyElement, App, Div, FontWeight, SharedString, div, prelude::*, px};

use crate::bots::mascot;
use crate::theme::{Theme, tw};

/// `EmptyBot`'s default size.
const MASCOT: f32 = 64.0;
/// `max-w-3xl`, 48rem.
const BODY_MAX: f32 = 48.0 * 13.0;

pub fn page_empty(title: &str, mood: MascotMood, cx: &App) -> Div {
    page_empty_with(Some(title), mood, None, cx)
}

/// [`page_empty`] with an optional title and a body under it.
pub fn page_empty_with(
    title: Option<&str>,
    mood: MascotMood,
    body: Option<AnyElement>,
    cx: &App,
) -> Div {
    let theme = Theme::global(cx);
    let column = div()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(tw(2.0)))
        .text_center()
        .child(mascot(mood, MASCOT))
        .children(title.map(|title| {
            div()
                .mt(px(tw(2.0)))
                .text_size(px(18.0))
                .line_height(px(18.0 * 1.25))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.color_text_primary)
                .child(SharedString::from(title.to_string()))
        }))
        .children(body.map(|body| {
            div()
                .mt(px(tw(4.0)))
                .w_full()
                .max_w(px(BODY_MAX))
                .child(body)
        }));
    div()
        .flex_1()
        .size_full()
        .min_h(px(200.0))
        .flex()
        .items_center()
        .justify_center()
        .p(px(tw(8.0)))
        .child(column)
}
