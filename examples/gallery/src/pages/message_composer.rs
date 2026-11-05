//! The message composer: empty with its placeholder, grown to a few lines, and past its cap,
//! where the field scrolls inside the card and fades at its ends.

use gpui::{
    AnyElement, App, Context, FontWeight, IntoElement, ScrollHandle, SharedString, Window, div,
    prelude::*, px,
};
use polterware_ui::TextInput;
use polterware_ui::patterns::message_composer;

use super::super::Example;

const WIDTH: f32 = 560.0;

const FEW: &str = "The test still fails on CI.\nRun it again with the race detector on,\nand tell me which goroutine holds the lock.";

const LONG: &str = "Line one of a long message.\nLine two.\nLine three.\nLine four.\nLine five.\nLine six.\nLine seven.\nLine eight.\nLine nine.\nLine ten.";

fn composer(id: &'static str, text: &'static str, window: &mut Window, cx: &mut App) -> AnyElement {
    let input = window.use_keyed_state(
        SharedString::from(format!("{id}-input")),
        cx,
        move |_, cx: &mut Context<TextInput>| {
            let mut input = TextInput::new(cx)
                .bare()
                .prose(13.5, 20.0, FontWeight::NORMAL)
                .chat()
                .placeholder("Send a message to the agent");
            input.set_text(text, cx);
            input
        },
    );
    let scroll = window
        .use_keyed_state(
            SharedString::from(format!("{id}-scroll")),
            cx,
            |_, _: &mut Context<ScrollHandle>| ScrollHandle::new(),
        )
        .read(cx)
        .clone();
    div()
        .w(px(WIDTH))
        .child(message_composer(id, &input, &scroll).max_height(100.0))
        .into_any_element()
}

fn empty(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    vec![composer("message-composer-empty", "", window, cx)]
}

fn few(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    vec![composer("message-composer-few", FEW, window, cx)]
}

fn capped(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    vec![composer("message-composer-capped", LONG, window, cx)]
}

pub fn examples() -> Vec<Example> {
    vec![
        Example {
            slug: "message-composer",
            title: "Empty, with its placeholder",
            height: 120.0,
            render: empty,
        },
        Example {
            slug: "message-composer-lines",
            title: "Grown to its lines",
            height: 180.0,
            render: few,
        },
        Example {
            slug: "message-composer-capped",
            title: "Past its cap: the field scrolls inside the card",
            height: 220.0,
            render: capped,
        },
    ]
}
