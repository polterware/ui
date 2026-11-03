//! `switch.docs.tsx`: the controlled switch and the switch row. The GPUI switch has one
//! size, no disabled state, and no trailing slot in its row, so "Sizes" draws the default
//! switch twice, "Row" leaves out "Weekly", and "Disabled" draws an enabled row.

use gpui::{AnyElement, App, Context, IntoElement, Window, div, prelude::*};
use polterware_ui::{Switch, switch_row};

use super::super::Example;

fn held(id: &'static str, on: bool, window: &mut Window, cx: &mut App) -> gpui::Entity<Switch> {
    window.use_keyed_state(id, cx, move |_, cx: &mut Context<Switch>| {
        Switch::new(on, cx)
    })
}

fn controlled(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    vec![held("switch-controlled", true, window, cx).into_any_element()]
}

fn sizes(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    vec![
        held("switch-compact", true, window, cx).into_any_element(),
        held("switch-default", true, window, cx).into_any_element(),
    ]
}

/// `SwitchRow` fills its preview's width.
fn row(
    id: &'static str,
    label: &'static str,
    on: bool,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let switch = held(id, on, window, cx);
    div()
        .w_full()
        .child(switch_row(
            gpui::SharedString::from(format!("{id}-row")),
            label,
            &switch,
            window,
            cx,
        ))
        .into_any_element()
}

fn switch_rows(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    vec![row(
        "switch-row",
        "Show verification reminders",
        false,
        window,
        cx,
    )]
}

fn disabled(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    vec![row(
        "switch-disabled",
        "Managed by policy",
        true,
        window,
        cx,
    )]
}

pub fn examples() -> Vec<Example> {
    let example = |title, render| Example {
        slug: "switch",
        title,
        height: 120.0,
        render,
    };
    vec![
        example("Controlled", controlled),
        example("Sizes", sizes),
        example("Row", switch_rows),
        example("Disabled", disabled),
    ]
}
