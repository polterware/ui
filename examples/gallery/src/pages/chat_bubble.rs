//! `ChatBubble`: the user's own message, short, long and collapsed, and the same one expanded.

use gpui::{AnyElement, App, IntoElement, Window, div, prelude::*, px};
use polterware_ui::ChatBubble;

use super::super::Example;

const WIDTH: f32 = 520.0;

const LONG: &str = "Please go through the failing runs from last night and group them by cause.\n\
For each group, say which step failed first.\n\
Keep the summary under ten lines.\n\
Then open a follow-up for the group with the most runs.\n\
Leave the flaky ones out for now.";

fn row(bubble: ChatBubble) -> AnyElement {
    div().w(px(WIDTH)).child(bubble).into_any_element()
}

fn short(_: &mut Window, _: &mut App) -> Vec<AnyElement> {
    vec![row(ChatBubble::new(
        "chat-short",
        "Can you check why the build is red?",
    )
    .meta_label("You · via Claude Code")
    .time("10:42")
    .on_copy(|_, _| {}))]
}

fn long(expanded: bool) -> ChatBubble {
    let id = if expanded {
        "chat-expanded"
    } else {
        "chat-collapsed"
    };
    ChatBubble::new(id, LONG)
        .meta_label("You · via Claude Code")
        .time("10:43")
        .on_copy(|_, _| {})
        .collapse_after(2)
        .expanded(expanded)
        .on_toggle(|_, _, _| {})
}

fn collapsed(_: &mut Window, _: &mut App) -> Vec<AnyElement> {
    vec![row(long(false))]
}

fn expanded(_: &mut Window, _: &mut App) -> Vec<AnyElement> {
    vec![row(long(true))]
}

pub fn examples() -> Vec<Example> {
    vec![
        Example {
            slug: "chat-bubble",
            title: "A short message",
            height: 120.0,
            render: short,
        },
        Example {
            slug: "chat-bubble-collapsed",
            title: "A long message, collapsed",
            height: 140.0,
            render: collapsed,
        },
        Example {
            slug: "chat-bubble-expanded",
            title: "The same message, expanded",
            height: 220.0,
            render: expanded,
        },
    ]
}
