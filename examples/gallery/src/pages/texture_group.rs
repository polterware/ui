//! `TextureButtonGroup` (`title-bar.tsx`'s history pill) and `CountBadge` (`count-badge.tsx`):
//! Go back and Go forward on one texture pill, a face each, the disabled one sunk; and the
//! count a sidebar row or a group header ends on, in its tones.

use gpui::{AnyElement, App, IntoElement, Window, div, prelude::*, px};
use polterware_ui::{
    Button, ButtonKind, ButtonSize, CountBadge, CountTone, Icon, TextureButtonGroup,
    TextureGroupItem,
};

use super::super::Example;

fn history(_: &mut Window, _: &mut App) -> Vec<AnyElement> {
    let pill = |id: &'static str, back: bool, forward: bool| {
        TextureButtonGroup::new(id)
            .item(TextureGroupItem::new("Go back", Icon::ChevronLeft).disabled(!back))
            .item(TextureGroupItem::new("Go forward", Icon::ChevronRight).disabled(!forward))
            .into_any_element()
    };
    vec![
        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .child(pill("texture-history", true, false))
            .child(
                Button::new("texture-home", ButtonKind::Secondary)
                    .size(ButtonSize::Icon)
                    .icon(Icon::Home01Icon),
            )
            .into_any_element(),
        pill("texture-both", true, true),
        pill("texture-none", false, false),
    ]
}

fn counts(_: &mut Window, _: &mut App) -> Vec<AnyElement> {
    let tones = [
        (3, CountTone::Neutral),
        (12, CountTone::Info),
        (4, CountTone::Warning),
        (9, CountTone::Success),
        (14, CountTone::Danger),
    ];
    let mut badges: Vec<AnyElement> = tones
        .iter()
        .map(|(count, tone)| CountBadge::new(*count).tone(*tone).into_any_element())
        .collect();
    badges.push(
        CountBadge::new(14)
            .sidebar()
            .tone(CountTone::Danger)
            .into_any_element(),
    );
    badges.push(CountBadge::new(128).sidebar().into_any_element());
    badges
}

pub fn examples() -> Vec<Example> {
    vec![
        Example {
            slug: "texture-group",
            title: "History buttons",
            height: 100.0,
            render: history,
        },
        Example {
            slug: "count-badge",
            title: "Counts in their tones",
            height: 100.0,
            render: counts,
        },
    ]
}
