//! `dialog.docs.tsx`'s motion: a dialog drawn a quarter of the way into its spring (its frame
//! at 99% and the fade most of the way in) beside one at rest, and a button that opens a
//! dialog on the slow spring over the scrim and closes it on the quicker tween.

use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{AnyElement, App, IntoElement, Window, div, prelude::*, px};
use polterware_ui::button::{Button, ButtonKind};
use polterware_ui::patterns::{DialogPose, DialogPresence, DialogSize, modal_layer};
use polterware_ui::text_input::TextInput;
use polterware_ui::theme::Theme;
use polterware_ui::{dialog, field, form_stack};

use super::super::Example;

fn body(key: &'static str, window: &mut Window, cx: &mut App) -> AnyElement {
    let host = window.use_keyed_state(key, cx, |_, cx| {
        TextInput::new(cx).placeholder("192.168.1.10")
    });
    form_stack([field("Host", host).into_any_element()]).into_any_element()
}

fn poses(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    // Where `DialogPresence` has a dialog 60ms after it opens.
    let mut presence = DialogPresence::new();
    presence.set_open(true);
    let quarter = presence.pose_at(Instant::now() + Duration::from_millis(60));
    [
        (quarter, "Opening", "gallery-dialog-opening"),
        (DialogPose::SHOWN, "Open", "gallery-dialog-open"),
    ]
    .into_iter()
    .map(|(pose, title, key)| {
        div()
            .w(px(420.0))
            .flex()
            .justify_center()
            .child(
                dialog(title, 1.0)
                    .size(DialogSize::Sm)
                    .pose(pose)
                    .description(format!(
                        "Opacity {:.2}, frame at {:.3}",
                        pose.opacity, pose.scale
                    ))
                    .child(body(key, window, cx)),
            )
            .into_any_element()
    })
    .collect()
}

fn open_close(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let theme = *Theme::global(cx);
    let presence =
        window.use_keyed_state("gallery-dialog-presence", cx, |_, _| DialogPresence::new());
    let open = presence.clone();
    let mut out = vec![
        Button::new("gallery-dialog-open", ButtonKind::Secondary)
            .label("Open dialog")
            .on_click(move |_, cx| {
                open.update(cx, |presence, cx| {
                    presence.set_open(true);
                    cx.notify();
                });
            })
            .into_any_element(),
    ];
    let shown = *presence.read(cx);
    if shown.is_present() {
        let pose = shown.frame(window);
        let close = |presence: &gpui::Entity<DialogPresence>| {
            let presence = presence.clone();
            Rc::new(move |_: &mut Window, cx: &mut App| {
                presence.update(cx, |presence, cx| {
                    presence.set_open(false);
                    cx.notify();
                });
            })
        };
        let card = dialog("Connect", 1.0)
            .size(DialogSize::Sm)
            .pose(pose)
            .on_close(close(&presence))
            .footer([Button::new("gallery-dialog-done", ButtonKind::Inverse)
                .label("Done")
                .on_click_rc(close(&presence))
                .into_any_element()])
            .child(body("gallery-dialog-live", window, cx));
        out.push(modal_layer(
            pose.opacity,
            shown.is_open(),
            close(&presence),
            card,
            &theme,
        ));
    }
    out
}

pub fn examples() -> Vec<Example> {
    vec![
        Example {
            slug: "dialog",
            title: "Opening on the slow spring",
            height: 300.0,
            render: poses,
        },
        Example {
            slug: "dialog",
            title: "Open and close",
            height: 110.0,
            render: open_close,
        },
    ]
}
