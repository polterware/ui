//! `peek.docs.tsx`: a quick look at a card, laid out like the Composer: its kind and tile,
//! its title, its properties as chips, then its intent and open tasks. Expand would open the
//! card's page.

use std::rc::Rc;

use gpui::{AnyElement, App, IntoElement, Window, div, prelude::*, px};
use polterware_ui::patterns::{DialogPose, peek, peek_chip, peek_section};
use polterware_ui::theme::{StatusTone, Theme};
use polterware_ui::{Icon, IconTile, chip_rounded};

use super::super::Example;

fn card(_window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let theme = *Theme::global(cx);
    let task = |number: u32, title: &'static str| {
        div()
            .text_color(theme.color_text_primary)
            .child(
                div()
                    .mr(px(8.0))
                    .font_family(theme.font_mono)
                    .text_size(px(12.0))
                    .text_color(theme.color_text_muted)
                    .child(format!("T{number}")),
            )
            .child(title)
            .flex()
            .into_any_element()
    };
    vec![
        peek("Export queue", "Card", DialogPose::SHOWN)
            .icon(IconTile::new(Icon::FolderKanban).size(24.0, 14.0))
            .on_open(Rc::new(|_, _| {}))
            .on_close(Rc::new(|_, _| {}))
            .properties([
                chip_rounded("In progress", StatusTone::Ok, 6.0, cx).into_any_element(),
                peek_chip(None, "export-queue", true, cx).into_any_element(),
                peek_chip(Some(Icon::Layers), "app/exports", false, cx).into_any_element(),
            ])
            .child(div().child(
                "Queue exports so a large one never blocks the window, and say when each is ready.",
            ))
            .child(peek_section(
                "Open tasks",
                [
                    task(2, "Retry a failed export once"),
                    task(3, "Show the queue in Activity"),
                ],
                cx,
            ))
            .into_any_element(),
    ]
}

pub fn examples() -> Vec<Example> {
    vec![Example {
        slug: "peek",
        title: "A card at a glance",
        height: 420.0,
        render: card,
    }]
}
