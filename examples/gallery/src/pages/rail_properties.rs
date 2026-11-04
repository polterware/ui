//! `DetailPanel.Property` in a 360px rail card: short values, values that wrap over lines, a
//! mono id with no spaces, and a chip beside a long path.

use gpui::{AnyElement, App, IntoElement, Window, div, prelude::*, px};
use polterware_ui::theme::StatusTone;
use polterware_ui::{chip, rail_card, rail_properties, rail_property, wrap_text};

use super::super::Example;

fn text(value: &str) -> AnyElement {
    wrap_text(value.to_string()).into_any_element()
}

fn properties(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let worktree = div()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .items_start()
        .gap(px(4.0))
        .child(chip("ready", StatusTone::Ok, cx))
        .child(wrap_text("/Users/someone/codes/project/.lanes/core-e"))
        .into_any_element();
    let rows = rail_properties()
        .child(rail_property(
            "Run",
            text("cedf395c309c57bac0e65660f9c528af"),
            true,
            cx,
        ))
        .child(rail_property(
            "Duration",
            text("queue 1s / exec 9m"),
            false,
            cx,
        ))
        .child(rail_property(
            "Note",
            text("no file changes since the run started"),
            false,
            cx,
        ))
        .child(rail_property(
            "Policy",
            text("Tier, priced by the router model"),
            false,
            cx,
        ))
        .child(rail_property("Worktree", worktree, true, cx));
    vec![
        div()
            .w(px(360.0))
            .child(rail_card(cx).child(rows))
            .into_any_element(),
    ]
}

pub fn examples() -> Vec<Example> {
    vec![Example {
        slug: "rail-properties",
        title: "Values that wrap",
        height: 380.0,
        render: properties,
    }]
}
