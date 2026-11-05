//! `EchoRing`: the boot splash's echo pulse, the smaller ring Vitals shows while a host
//! connects (3px dots, muted peak, `--color-surface-selected` at rest), and the still ring of
//! reduced motion.

use gpui::{AnyElement, App, IntoElement, Window, div, prelude::*, px};
use polterware_ui::EchoRing;
use polterware_ui::theme::Theme;

use super::super::Example;

fn labeled(ring: EchoRing, label: &'static str, cx: &App) -> AnyElement {
    let theme = Theme::global(cx);
    div()
        .w(px(160.0))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(12.0))
        .child(ring)
        .child(
            div()
                .text_size(px(12.0))
                .text_color(theme.color_text_muted)
                .child(label),
        )
        .into_any_element()
}

fn rings(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let theme = *Theme::global(cx);
    vec![
        labeled(EchoRing::new("gallery-echo-splash"), "Boot splash", cx),
        labeled(
            EchoRing::new("gallery-echo-connecting")
                .dot(3.0)
                .peak(theme.color_text_muted)
                .rest(theme.color_surface_selected),
            "Connecting",
            cx,
        ),
        labeled(
            EchoRing::new("gallery-echo-still").still(true),
            "Reduced motion",
            cx,
        ),
    ]
}

pub fn examples() -> Vec<Example> {
    vec![Example {
        slug: "echo-ring",
        title: "Echo ring",
        height: 150.0,
        render: rings,
    }]
}
