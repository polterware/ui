//! `select.docs.tsx`: the trigger showing the picked option, at the preview's full width as
//! the Solid trigger fills it. The GPUI trigger has no disabled state, so "Disabled" draws an
//! enabled trigger; the menus are not open in any example.

use std::rc::Rc;

use gpui::{AnyElement, App, IntoElement, Window};
use polterware_ui::select_trigger;

use super::super::Example;

fn trigger(id: &'static str, label: &'static str, cx: &App) -> Vec<AnyElement> {
    vec![select_trigger(id, label, None, Rc::new(|_, _| {}), cx).into_any_element()]
}

fn default(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    trigger("select-default", "Day", cx)
}

fn controlled(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    trigger("select-controlled", "Week", cx)
}

fn options(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    trigger("select-options", "Private", cx)
}

fn disabled(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    trigger("select-disabled", "Day", cx)
}

pub fn examples() -> Vec<Example> {
    let example = |title, render| Example {
        slug: "select",
        title,
        height: 120.0,
        render,
    };
    vec![
        example("Default", default),
        example("Controlled", controlled),
        example("Options with separators and disabled items", options),
        example("Disabled", disabled),
    ]
}
