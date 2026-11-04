//! `PageEmpty` and its small form for a list: the mascot in each mood over what the page or
//! the list says.

use gpui::{AnyElement, App, IntoElement, Window, div, prelude::*, px};
use polterware_ui::theme::Theme;
use polterware_ui::{MascotMood, list_empty, page_empty};

use super::super::Example;

fn page(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    [
        ("No runs yet", MascotMood::Sleeping),
        ("Could not load the runs", MascotMood::Sad),
        ("No run matches", MascotMood::Lost),
    ]
    .into_iter()
    .map(|(title, mood)| {
        div()
            .w(px(260.0))
            .h(px(240.0))
            .child(page_empty(title, mood, cx))
            .into_any_element()
    })
    .collect()
}

/// In a sidebar's width, under its section heading.
fn list(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let theme = *Theme::global(cx);
    [
        ("No runs to show.", MascotMood::Sleeping),
        ("Could not refresh runs and sections.", MascotMood::Sad),
    ]
    .into_iter()
    .map(|(text, mood)| {
        div()
            .w(px(240.0))
            .p(px(theme.space_2))
            .rounded(px(theme.radius_lg))
            .bg(theme.color_surface_subtle)
            .child(
                div()
                    .pl(px(theme.space_2))
                    .text_size(px(theme.font_size_sm))
                    .text_color(theme.color_text_muted)
                    .child("Runs"),
            )
            .child(list_empty(text, mood, cx))
            .into_any_element()
    })
    .collect()
}

pub fn examples() -> Vec<Example> {
    vec![
        Example {
            slug: "empty",
            title: "A page in each mood",
            height: 300.0,
            render: page,
        },
        Example {
            slug: "empty-list",
            title: "A list in a sidebar",
            height: 220.0,
            render: list,
        },
    ]
}
