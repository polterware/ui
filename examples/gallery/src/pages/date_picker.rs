//! `date-picker.docs.tsx`: empty, selected and disabled date fields; an open month grid.
use super::super::Example;
use gpui::{AnyElement, App, Context, IntoElement, Window, div, prelude::*};
use polterware_ui::date_picker::DatePicker;

fn field(
    id: &'static str,
    selected: bool,
    disabled: bool,
    expanded: bool,
    window: &mut Window,
    cx: &mut App,
) -> Vec<AnyElement> {
    let picker = window.use_keyed_state(id, cx, move |window, cx: &mut Context<DatePicker>| {
        let mut picker = DatePicker::new(cx).label("Choose date").disabled(disabled);
        if selected {
            picker.set_value(DatePicker::parse("2026-11-06"), cx);
        }
        if expanded {
            picker.open(window, cx);
        }
        picker
    });
    let overlay = DatePicker::overlay(&picker, window, cx);
    vec![div().child(picker).children(overlay).into_any_element()]
}
fn empty(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    field("date-empty", false, false, false, window, cx)
}
fn selected(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    field("date-selected", true, false, false, window, cx)
}
fn disabled(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    field("date-disabled", true, true, false, window, cx)
}
fn expanded(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    field("date-expanded", true, false, true, window, cx)
}
pub fn examples() -> Vec<Example> {
    vec![
        Example {
            slug: "date-picker",
            title: "Default",
            height: 110.0,
            render: empty,
        },
        Example {
            slug: "date-picker",
            title: "Selected date",
            height: 110.0,
            render: selected,
        },
        Example {
            slug: "date-picker",
            title: "Disabled",
            height: 110.0,
            render: disabled,
        },
        Example {
            slug: "date-picker",
            title: "Month grid",
            height: 410.0,
            render: expanded,
        },
    ]
}
