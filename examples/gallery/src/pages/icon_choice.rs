//! `IconChoice`: selectable ~64px image tiles with labels under them, as used for choosing an
//! app icon variant (Default, Ice, Fire) in settings.

use gpui::{AnyElement, App, Context, IntoElement, SharedString, Window};
use polterware_ui::IconChoice;

use super::super::Example;

const SAMPLE: &[u8] = include_bytes!("../../assets/sample.png");

/// The picked key, kept across frames so clicks and arrow keys move the accent ring.
struct Picked(SharedString);

fn variants(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let picked = window.use_keyed_state("gallery-icon-choice", cx, |_, _: &mut Context<Picked>| {
        Picked("default".into())
    });
    let selected = picked.read(cx).0.clone();
    vec![
        IconChoice::new("gallery-icon-choice")
            .option("default", "Default", SAMPLE)
            .option("ice", "Ice", SAMPLE)
            .option("fire", "Fire", SAMPLE)
            .selected(selected)
            .on_change(move |key, _, cx| {
                picked.update(cx, |picked, cx| {
                    picked.0 = key;
                    cx.notify();
                });
            })
            .into_any_element(),
    ]
}

fn disabled(_: &mut Window, _: &mut App) -> Vec<AnyElement> {
    vec![
        IconChoice::new("gallery-icon-choice-disabled")
            .option("default", "Default", SAMPLE)
            .option("ice", "Ice", SAMPLE)
            .option("fire", "Fire", SAMPLE)
            .selected("ice")
            .disabled(true)
            .into_any_element(),
    ]
}

pub fn examples() -> Vec<Example> {
    vec![
        Example {
            slug: "icon-choice",
            title: "App icon choice",
            height: 160.0,
            render: variants,
        },
        Example {
            slug: "icon-choice",
            title: "Disabled",
            height: 160.0,
            render: disabled,
        },
    ]
}
