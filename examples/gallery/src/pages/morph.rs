//! `morph.docs.tsx`: a button that grows into a small panel in place, Cancel standing where
//! the button was. Both start closed; click a trigger to open it, and Escape, a press outside,
//! or Cancel to close it.

use std::rc::Rc;

use gpui::{AnyElement, App, Context, Entity, IntoElement, Window, div, prelude::*, px};
use polterware_ui::patterns::Morph;
use polterware_ui::radio_group::{RadioOption, radio_group};
use polterware_ui::theme::Theme;
use polterware_ui::{Button, ButtonKind, ButtonSize, Icon};

use super::super::Example;

#[derive(Default)]
struct Panel {
    open: bool,
    picked: Option<usize>,
}

fn panel(id: &'static str, window: &mut Window, cx: &mut App) -> Entity<Panel> {
    window.use_keyed_state(id, cx, |_, _: &mut Context<Panel>| Panel::default())
}

fn set_open(state: &Entity<Panel>) -> impl Fn(bool, &mut Window, &mut App) + 'static {
    let state = state.clone();
    move |open, _, cx| {
        state.update(cx, |panel, cx| {
            panel.open = open;
            cx.notify();
        });
    }
}

fn trigger(id: &'static str, kind: ButtonKind, icon: Icon, label: &'static str) -> Button {
    Button::new(id, kind)
        .size(ButtonSize::Sm)
        .icon(icon)
        .label(label)
        .radius(8.0)
}

fn confirm(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let theme = *Theme::global(cx);
    let state = panel("morph-confirm", window, cx);
    let open = state.read(cx).open;
    let close = set_open(&state);
    let morph = Morph::new(
        "morph-confirm",
        trigger("morph-later", ButtonKind::Secondary, Icon::Clock, "Later")
            .tooltip("Set this aside. It stays in the Inbox, dimmed under Later."),
        "Move to Later?",
    )
    .open(open, set_open(&state))
    .body(
        div()
            .text_color(theme.color_text_secondary)
            .child("The item stays in the Inbox, dimmed under Later, until you come back to it."),
    )
    .action(
        Button::new("morph-later-confirm", ButtonKind::Primary)
            .size(ButtonSize::Sm)
            .label("Move to Later")
            .radius(8.0)
            .full_width()
            .on_click(move |window, cx| close(false, window, cx)),
    );
    vec![div().w(px(180.0)).flex().child(morph).into_any_element()]
}

const ANSWERS: [&str; 2] = ["Yes, behind a flag", "No, ship it to everyone"];

fn answer(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let state = panel("morph-answer", window, cx);
    let (open, picked) = {
        let panel = state.read(cx);
        (panel.open, panel.picked)
    };
    let options: Vec<RadioOption> = ANSWERS
        .iter()
        .map(|label| RadioOption {
            label: (*label).into(),
        })
        .collect();
    let pick = state.clone();
    let close = set_open(&state);
    let morph = Morph::new(
        "morph-answer",
        trigger("morph-decide", ButtonKind::Primary, Icon::Check, "Decide")
            .tooltip("Answer this question."),
        "Ship the event stream behind a flag?",
    )
    .open(open, set_open(&state))
    .body(radio_group(
        "morph-answer-options",
        &options,
        picked.unwrap_or(usize::MAX),
        Rc::new(move |index, _, cx: &mut App| {
            pick.update(cx, |panel, cx| {
                panel.picked = Some(index);
                cx.notify();
            });
        }),
        cx,
    ))
    .action(
        Button::new("morph-answer-record", ButtonKind::Primary)
            .size(ButtonSize::Sm)
            .label("Record decision")
            .radius(8.0)
            .full_width()
            .disabled(picked.is_none())
            .on_click(move |window, cx| close(false, window, cx)),
    );
    vec![div().w(px(180.0)).flex().child(morph).into_any_element()]
}

pub fn examples() -> Vec<Example> {
    let example = |title, render| Example {
        slug: "morph",
        title,
        height: 120.0,
        render,
    };
    vec![
        example("Confirm an action", confirm),
        example("Pick an answer", answer),
    ]
}
