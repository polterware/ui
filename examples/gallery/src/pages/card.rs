//! `card.docs.tsx`: the default ring Card (`p-3`) with a title and a line under it, and the
//! texture card and its compact face. The `Surface` host, nested, and floating panels have
//! no GPUI port.

use gpui::{AnyElement, App, FontWeight, IntoElement, Window, div, prelude::*, px};
use polterware_ui::theme::Theme;
use polterware_ui::{card, texture_card, texture_compact};

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

/// A title over a line, as both texture faces hold them.
fn lines(title: &'static str, line: &'static str, cx: &App) -> [AnyElement; 2] {
    let theme = *Theme::global(cx);
    [
        div()
            .relative()
            .font_weight(FontWeight::MEDIUM)
            .text_color(theme.color_text_primary)
            .child(title)
            .into_any_element(),
        div()
            .relative()
            .text_color(theme.color_text_secondary)
            .child(line)
            .into_any_element(),
    ]
}

fn textures(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    vec![
        texture_card(cx)
            .p(px(16.0))
            .w(px(280.0))
            .text_size(px(14.0))
            .line_height(px(20.0))
            .children(lines(
                "Checkout flow",
                "A canvas card on the Canvas list.",
                cx,
            ))
            .into_any_element(),
        texture_compact(cx)
            .px(px(12.0))
            .py(px(8.0))
            .text_size(px(13.0))
            .line_height(px(18.0))
            .children(lines("Compact", "A dock or a hint over a canvas.", cx))
            .into_any_element(),
    ]
}

pub fn examples() -> Vec<Example> {
    vec![
        Example {
            slug: "card",
            title: "Concentric rings (default)",
            height: 120.0,
            render: rings,
        },
        Example {
            slug: "card-texture",
            title: "Texture and compact texture",
            height: 140.0,
            render: textures,
        },
    ]
}
