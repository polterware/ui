//! `icon-tile.docs.tsx`: the chrome-mark tile in three tones and sizes, and the plain finish.
//! Brand marks, dimmed tiles, aurora covers, and the chrome reflection families have no
//! GPUI port, so those examples and the dimmed plain tile are left out.

use gpui::{AnyElement, App, IntoElement, Window};
use polterware_ui::{Icon, IconTile, TileTone};

use super::super::Example;

fn tones(_: &mut Window, _: &mut App) -> Vec<AnyElement> {
    vec![
        IconTile::new(Icon::Folder)
            .size(24.0, 14.0)
            .tone(TileTone::Architecture)
            .into_any_element(),
        IconTile::new(Icon::Database)
            .size(28.0, 16.0)
            .tone(TileTone::Data)
            .into_any_element(),
        IconTile::new(Icon::Shield)
            .size(36.0, 18.0)
            .tone(TileTone::Security)
            .into_any_element(),
    ]
}

fn plain(_: &mut Window, _: &mut App) -> Vec<AnyElement> {
    vec![
        IconTile::new(Icon::Folder)
            .size(24.0, 14.0)
            .plain()
            .into_any_element(),
        IconTile::new(Icon::Database)
            .size(28.0, 16.0)
            .plain()
            .into_any_element(),
        IconTile::new(Icon::Shield)
            .size(36.0, 18.0)
            .plain()
            .into_any_element(),
    ]
}

fn round(_: &mut Window, _: &mut App) -> Vec<AnyElement> {
    vec![
        IconTile::new(Icon::Bot)
            .size(20.0, 12.0)
            .round()
            .into_any_element(),
        IconTile::new(Icon::Database)
            .size(28.0, 16.0)
            .tone(TileTone::Data)
            .round()
            .into_any_element(),
    ]
}

fn service_marks(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    polterware_ui::service_mark::ALL
        .iter()
        .chain([&"google-cloud", &"unknown"])
        .map(|name| polterware_ui::service_mark::service_mark(name, 24.0, cx))
        .collect()
}

pub fn examples() -> Vec<Example> {
    let example = |title, render| Example {
        slug: "icon-tile",
        title,
        height: 120.0,
        render,
    };
    vec![
        example("Semantic tones", tones),
        example("Plain finish", plain),
        example("Round", round),
        example("Service marks", service_marks),
    ]
}
