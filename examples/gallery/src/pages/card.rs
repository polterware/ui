//! `card.docs.tsx`: the default ring Card (`p-3`) with a title and a line under it. The
//! texture card and the `Surface` host, nested, and floating panels have no GPUI port.

use gpui::{AnyElement, App, FontWeight, IntoElement, Window, div, prelude::*, px};
use polterware_ui::card;
use polterware_ui::theme::Theme;

use super::super::Example;

fn rings(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let theme = *Theme::global(cx);
    vec![
        card(cx)
            .p(px(12.0))
            .text_size(px(14.0))
            .line_height(px(20.0))
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.color_text_primary)
                    .child("Project context"),
            )
            .child(
                div()
                    .text_color(theme.color_text_secondary)
                    .child("Evidence is attached to this decision."),
            )
            .into_any_element(),
    ]
}

pub fn examples() -> Vec<Example> {
    vec![Example {
        slug: "card",
        title: "Concentric rings (default)",
        height: 120.0,
        render: rings,
    }]
}
