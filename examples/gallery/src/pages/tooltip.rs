//! The rich tooltip, drawn in place as it shows on hover: a score that explains itself with
//! a heading, label and value rows, and a closing note; and the same card on a hover, live.

use gpui::{AnyElement, App, FontWeight, IntoElement, SharedString, Window, div, prelude::*, px};
use polterware_ui::theme::Theme;
use polterware_ui::{mono, rich_tooltip, tooltip_card, tooltip_heading, tooltip_note, tooltip_row};

use super::super::Example;

const WIDTH: f32 = 280.0;

fn score_body(cx: &App) -> AnyElement {
    let code = |text: &str| {
        div()
            .font(mono(Theme::global(cx), FontWeight::NORMAL))
            .child(SharedString::from(text.to_string()))
    };
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(tooltip_heading(
            "Score 53",
            Some("The chance to pass a task of difficulty 0, on CursorBench's scale.".into()),
            cx,
        ))
        .child(tooltip_row("Skill \u{03b8}", code("0.12"), cx))
        .child(tooltip_row("From", "CursorBench 53% at high", cx))
        .child(tooltip_row(
            "Lower bound",
            code("\u{03b8} \u{2212} 0.30"),
            cx,
        ))
        .child(tooltip_note(
            "Catalog 2026-10-31 \u{00b7} CursorBench, ECI",
            cx,
        ))
        .into_any_element()
}

fn card(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    vec![tooltip_card(WIDTH, score_body(cx), cx)]
}

fn on_hover(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let theme = Theme::global(cx);
    vec![
        div()
            .id("gallery-rich-tooltip")
            .px(px(10.0))
            .py(px(4.0))
            .rounded(px(theme.radius_sm))
            .border_1()
            .border_color(theme.color_border)
            .text_size(px(12.5))
            .child("Hover for the score")
            .tooltip(rich_tooltip(WIDTH, |_, cx| score_body(cx)))
            .into_any_element(),
    ]
}

pub fn examples() -> Vec<Example> {
    let example = |title, height, render| Example {
        slug: "tooltip",
        title,
        height,
        render,
    };
    vec![
        example("Rich tooltip", 220.0, card),
        example("On hover", 120.0, on_hover),
    ]
}
