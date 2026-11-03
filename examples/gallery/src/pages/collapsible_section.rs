//! `collapsible-section.docs.tsx`: open sections with and without an icon or a count, and a
//! closed one.

use std::rc::Rc;

use gpui::{AnyElement, App, Context, IntoElement, SharedString, Window, div, prelude::*, px};
use polterware_ui::theme::Theme;
use polterware_ui::{CollapsibleSection, Icon};

use super::super::Example;

#[allow(clippy::too_many_arguments)]
fn section(
    id: &'static str,
    title: &'static str,
    icon: bool,
    aside: Option<&'static str>,
    open: bool,
    text: &'static str,
    window: &mut Window,
    cx: &mut App,
) -> Vec<AnyElement> {
    let section = window.use_keyed_state(id, cx, move |_, cx: &mut Context<CollapsibleSection>| {
        let mut section = CollapsibleSection::new(title, cx);
        if icon {
            section.set_plain_icon(Icon::Folder);
        }
        section.set_aside(aside.map(SharedString::from));
        section.set_body(Rc::new(move |_, cx| {
            div()
                .text_size(px(14.0))
                .line_height(px(20.0))
                .text_color(Theme::global(cx).color_text_secondary)
                .child(text)
                .into_any_element()
        }));
        section.set_open(open, cx);
        section
    });
    // The section fills its width, so it takes the preview's whole line, its header and body
    // starting at the left like the inline Solid section.
    vec![div().w_full().child(section).into_any_element()]
}

fn default(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    section(
        "section-default",
        "Evidence",
        false,
        None,
        true,
        "Three sources support this fact.",
        window,
        cx,
    )
}

fn with_icon(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    section(
        "section-icon",
        "Project files",
        true,
        None,
        true,
        "Source imports are ready.",
        window,
        cx,
    )
}

fn with_count(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    section(
        "section-count",
        "Cards",
        true,
        Some("3"),
        true,
        "Three cards cite this component.",
        window,
        cx,
    )
}

fn closed(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    section(
        "section-closed",
        "Advanced settings",
        false,
        None,
        false,
        "Open this section to reveal its content.",
        window,
        cx,
    )
}

pub fn examples() -> Vec<Example> {
    let example = |title, height, render| Example {
        slug: "collapsible-section",
        title,
        height,
        render,
    };
    vec![
        example("Default", 122.9, default),
        example("With icon", 122.9, with_icon),
        example("With a count", 122.9, with_count),
        example("Initially closed", 120.0, closed),
    ]
}
