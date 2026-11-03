//! `progress-bar.docs.tsx`: the default bar, its tones, both sizes, the empty, unknown, and
//! complete values, and the controlled bar beside its Advance button, each `w-48` (192px).

use gpui::{AnyElement, App, Div, IntoElement, Window, div, prelude::*, px};
use polterware_ui::theme::Theme;
use polterware_ui::{Button, ButtonKind, ButtonSize, progress_bar, progress_bar_sm};

use super::super::Example;

/// `w-48`.
const WIDTH: f32 = 192.0;

/// `flex w-48 flex-col gap-3`.
fn column(bars: Vec<Div>) -> Vec<AnyElement> {
    vec![
        div()
            .w(px(WIDTH))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .children(bars)
            .into_any_element(),
    ]
}

fn default(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let accent = Theme::global(cx).color_accent;
    vec![
        div()
            .w(px(WIDTH))
            .child(progress_bar(0.68, accent, cx))
            .into_any_element(),
    ]
}

fn tones(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let theme = *Theme::global(cx);
    column(
        [
            theme.color_accent,
            theme.chart_blue,
            theme.chart_violet,
            theme.color_success,
            theme.color_warning,
            theme.color_danger,
        ]
        .into_iter()
        .map(|color| progress_bar(0.7, color, cx))
        .collect(),
    )
}

fn sizes(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let accent = Theme::global(cx).color_accent;
    column(vec![
        progress_bar_sm(0.35, accent, cx),
        progress_bar(0.82, accent, cx),
    ])
}

fn empty(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let accent = Theme::global(cx).color_accent;
    // An unknown value (`null`) draws the empty track, as 0 does.
    column(vec![
        progress_bar(0.0, accent, cx),
        progress_bar(0.0, accent, cx),
        progress_bar(1.0, accent, cx),
    ])
}

fn controlled(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let accent = Theme::global(cx).color_accent;
    vec![
        div()
            .w(px(WIDTH))
            .child(progress_bar(0.4, accent, cx))
            .into_any_element(),
        Button::new("progress-advance", ButtonKind::Secondary)
            .size(ButtonSize::Sm)
            .label("Advance")
            .into_any_element(),
    ]
}

pub fn examples() -> Vec<Example> {
    let example = |title, height, render| Example {
        slug: "progress-bar",
        title,
        height,
        render,
    };
    vec![
        example("Default", 120.0, default),
        example("Tones", 158.0, tones),
        example("Sizes", 120.0, sizes),
        example("Empty, unknown and complete", 120.0, empty),
        example("Controlled", 120.0, controlled),
    ]
}
