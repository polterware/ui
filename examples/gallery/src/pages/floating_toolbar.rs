//! Electron PageToolbar floating over a document; the text scrolls under its controls.
use super::super::Example;
use gpui::{AnyElement, App, Context, IntoElement, Window, div, prelude::*, px};
use polterware_ui::{
    button::{Button, ButtonKind},
    floating_toolbar::{FloatingToolbar, floating_toolbar},
    theme::Theme,
};
fn page(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let state = window.use_keyed_state(
        "floating-toolbar-gallery",
        cx,
        |_, _: &mut Context<FloatingToolbar>| FloatingToolbar::new(44.0),
    );
    let inset = state.read(cx).height();
    let toolbar = div()
        .h(px(44.0))
        .p(px(8.0))
        .child(Button::new("floating-action", ButtonKind::Secondary).label("Toolbar"));
    let ink = Theme::global(cx).color_text_secondary;
    let content = div()
        .id("floating-document")
        .size_full()
        .overflow_y_scroll()
        .child(
            div()
                .pt(px(inset + 24.0))
                .px(px(24.0))
                .pb(px(24.0))
                .text_color(ink)
                .children((0..30).map(|index| {
                    div()
                        .h(px(30.0))
                        .child(format!("Document line {}", index + 1))
                })),
        );
    vec![
        div()
            .w(px(480.0))
            .h(px(240.0))
            .child(floating_toolbar(state, toolbar, content))
            .into_any_element(),
    ]
}
pub fn examples() -> Vec<Example> {
    vec![Example {
        slug: "floating-toolbar",
        title: "Document under floating toolbar",
        height: 280.0,
        render: page,
    }]
}
