//! `PageEmpty`: `EmptyBot` (the mascot in a mood, 64px) over an 18px semibold title at
//! `leading-tight`, centered in the space it fills (`flex h-full min-h-[200px] w-full p-8`,
//! then `m-auto flex-col items-center gap-2 text-center`). The title sits `mt-2` lower, an
//! action `mt-2` lower, and a body `mt-4` lower, as wide as it needs up to `max-w-3xl`. [`list_empty`] is its small
//! form, for a list in a narrow column such as a sidebar.

pub use bot_avatar::MascotMood;
use gpui::{AnyElement, App, Div, FontWeight, SharedString, div, prelude::*, px};

use crate::bots::mascot;
use crate::theme::{Theme, tw};

/// `EmptyBot`'s default size.
const MASCOT: f32 = 64.0;
/// The mascot of a list's empty state.
const LIST_MASCOT: f32 = 40.0;
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
    page_empty_parts(title, mood, None, body, cx)
}

/// [`page_empty`] with an optional title and an action under it (`PageEmpty`'s `action`),
/// closer to the mascot than a body.
pub fn page_empty_action(
    title: Option<&str>,
    mood: MascotMood,
    action: AnyElement,
    cx: &App,
) -> Div {
    page_empty_parts(title, mood, Some(action), None, cx)
}

fn page_empty_parts(
    title: Option<&str>,
    mood: MascotMood,
    action: Option<AnyElement>,
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
        .children(action.map(|action| div().mt(px(tw(2.0))).child(action)))
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

/// A list's empty state, for a narrow column such as a sidebar: the mascot at 40px over one
/// muted line in the small size, centered, with room above and below. It says one thing: a
/// list that is empty because it could not load shows only that.
pub fn list_empty(text: impl Into<SharedString>, mood: MascotMood, cx: &App) -> Div {
    let theme = Theme::global(cx);
    div()
        .w_full()
        .py(px(tw(6.0)))
        .px(px(theme.space_2))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(theme.space_2))
        .text_center()
        .child(mascot(mood, LIST_MASCOT))
        .child(
            div()
                .text_size(px(theme.font_size_sm))
                .text_color(theme.color_text_muted)
                .child(text.into()),
        )
}
