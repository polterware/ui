//! `MessageRail`: the thin index of a conversation's messages along the edge of its view.
//! The preview sets one beside a stand-in transcript: hover a bar for the message's first
//! lines, click one to mark it as the message in view.

use gpui::{AnyElement, App, Context, IntoElement, Window, div, prelude::*, px};
use polterware_ui::theme::Theme;
use polterware_ui::{MESSAGE_RAIL_WIDTH, MessageRail, MessageRailItem};

use super::super::Example;

/// The message in view, kept across frames so a click moves the mark.
struct Viewing(usize);

fn messages(count: usize) -> Vec<MessageRailItem> {
    let texts = [
        "Add a message index rail to the run page.\nOne bar per message I sent.\nClicking a bar scrolls to it.\nHovering shows its text.",
        "Make the bars a little wider on hover.",
        "Now keep it clear of the scrollbar.",
        "Thanks, ship it.",
    ];
    (0..count)
        .map(|index| {
            MessageRailItem::new(
                format!("message-{index}"),
                texts[index % texts.len()].to_string(),
            )
        })
        .collect()
}

fn rail(id: &'static str, count: usize, window: &mut Window, cx: &mut App) -> AnyElement {
    let viewing = window.use_keyed_state(id, cx, |_, _: &mut Context<Viewing>| Viewing(0));
    let active = viewing.read(cx).0.min(count.saturating_sub(1));
    let theme = *Theme::global(cx);
    div()
        .relative()
        .w_full()
        .h(px(150.0))
        .rounded(px(theme.radius_card))
        .border_1()
        .border_color(theme.color_border)
        .bg(theme.color_surface)
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .right(px(theme.space_2))
                .w(px(MESSAGE_RAIL_WIDTH))
                .flex()
                .items_center()
                .child(
                    MessageRail::new(id)
                        .items(messages(count))
                        .active(Some(active))
                        .on_select(move |index, _, cx| {
                            viewing.update(cx, |viewing, cx| {
                                viewing.0 = index;
                                cx.notify();
                            });
                        }),
                ),
        )
        .into_any_element()
}

fn several(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    vec![rail("message-rail-several", 4, window, cx)]
}

fn single(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    vec![rail("message-rail-single", 1, window, cx)]
}

pub fn examples() -> Vec<Example> {
    let example = |title, render| Example {
        slug: "message-rail",
        title,
        height: 190.0,
        render,
    };
    vec![
        example("Message rail", several),
        example("One message (hidden)", single),
    ]
}
