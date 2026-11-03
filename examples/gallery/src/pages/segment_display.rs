//! `segment-display.docs.tsx`: a percentage at the default 14px, and a clock at 32px.

use gpui::{AnyElement, App, IntoElement, Window};
use polterware_ui::segment_display;

use super::super::Example;

fn default(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    vec![segment_display("68%", 14.0, cx).into_any_element()]
}

fn clock(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    vec![segment_display("24:59", 32.0, cx).into_any_element()]
}

pub fn examples() -> Vec<Example> {
    let example = |title, render| Example {
        slug: "segment-display",
        title,
        height: 120.0,
        render,
    };
    vec![example("Default", default), example("Clock", clock)]
}
