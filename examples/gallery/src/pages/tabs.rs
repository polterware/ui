//! `Tabs` (`tabs.tsx`): a page toolbar's section tabs, such as Activity's All, Human, and
//! Agent, on the recessed track with the active pill gliding between them.

use gpui::{AnyElement, App, Context, IntoElement, Window};
use polterware_ui::Tabs;

use super::super::Example;

/// The picked tab, kept across frames so a click moves the pill.
struct Picked(usize);

fn tabs(
    id: &'static str,
    labels: &[&'static str],
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let picked = window.use_keyed_state(id, cx, |_, _: &mut Context<Picked>| Picked(0));
    let selected = picked.read(cx).0;
    let mut tabs = Tabs::new(id)
        .selected(selected)
        .on_select(move |index, _, cx| {
            picked.update(cx, |picked, cx| {
                picked.0 = index;
                cx.notify();
            });
        });
    for label in labels {
        tabs = tabs.tab(*label);
    }
    tabs.into_any_element()
}

fn actors(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    vec![
        tabs("tabs-actors", &["All", "Human", "Agent"], window, cx),
        tabs("tabs-layouts", &["Board", "List"], window, cx),
        tabs(
            "tabs-knowledge",
            &["Decisions", "Rules", "Docs"],
            window,
            cx,
        ),
    ]
}

pub fn examples() -> Vec<Example> {
    vec![Example {
        slug: "tabs",
        title: "Section tabs",
        height: 100.0,
        render: actors,
    }]
}
