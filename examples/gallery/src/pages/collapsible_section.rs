//! `collapsible-section.docs.tsx`: open sections with and without an icon or a count, a
//! closed one, and one with actions at its heading's end.

use std::rc::Rc;

use gpui::{AnyElement, App, Context, IntoElement, SharedString, Window, div, prelude::*, px};
use polterware_ui::theme::Theme;
use polterware_ui::{Button, ButtonKind, ButtonSize, CollapsibleSection, Icon};

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

fn with_actions(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let section = window.use_keyed_state(
        "section-actions",
        cx,
        |_, cx: &mut Context<CollapsibleSection>| {
            let mut section = CollapsibleSection::new("Runs per day", cx);
            section.set_plain_icon(Icon::CalendarClock);
            section.set_actions(Some(Rc::new(|_, _| {
                Button::new("section-action", ButtonKind::Secondary)
                    .size(ButtonSize::Sm)
                    .label("Last 7 days")
                    .into_any_element()
            })));
            section.set_body(Rc::new(|_, cx| {
                div()
                    .text_size(px(14.0))
                    .line_height(px(20.0))
                    .text_color(Theme::global(cx).color_text_secondary)
                    .child("A click on the action does not fold the section.")
                    .into_any_element()
            }));
            section
        },
    );
    vec![div().w_full().child(section).into_any_element()]
}

/// A document's section, as Structure's record sections are set: the larger title, 12px in,
/// with a `+` at its end.
fn document(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let section = window.use_keyed_state(
        "section-document",
        cx,
        |_, cx: &mut Context<CollapsibleSection>| {
            let mut section = CollapsibleSection::new("Rules", cx);
            section.set_large_title(true);
            section.set_actions(Some(Rc::new(|_, _| {
                Button::new("section-document-link", ButtonKind::Ghost)
                    .size(ButtonSize::Icon)
                    .icon(Icon::Plus)
                    .tooltip("Link rule")
                    .into_any_element()
            })));
            section.set_body(Rc::new(|_, cx| {
                div()
                    .px(px(12.0))
                    .text_size(px(12.5))
                    .text_color(Theme::global(cx).color_text_secondary)
                    .child("Keep one writer per project")
                    .into_any_element()
            }));
            section
        },
    );
    vec![div().w_full().child(section).into_any_element()]
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
        example("With actions", 130.0, with_actions),
        example("A document's section", 130.0, document),
    ]
}
