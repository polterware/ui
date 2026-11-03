//! `kbd.docs.tsx`: one key, and a shortcut of two keys 4px apart.

use gpui::{AnyElement, App, IntoElement, Window, div, prelude::*, px};
use polterware_ui::kbd;
use polterware_ui::theme::Theme;

use super::super::Example;

fn single(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    vec![kbd("Esc", Theme::global(cx)).into_any_element()]
}

fn shortcut(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let theme = Theme::global(cx);
    vec![
        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .child(kbd("⌘", theme))
            .child(kbd("K", theme))
            .into_any_element(),
    ]
}

pub fn examples() -> Vec<Example> {
    let example = |title, render| Example {
        slug: "keyboard-key",
        title,
        height: 120.0,
        render,
    };
    vec![example("Single key", single), example("Shortcut", shortcut)]
}
