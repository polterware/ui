//! The settings dialog: grouped sections in the rail, the open one lit, and its panel with
//! Close, the heading, and the page's sections as cards (`settings_section`), each with its
//! tile, title, description, an optional action, and its rows.

use std::rc::Rc;

use gpui::{AnyElement, App, IntoElement, Window, div, prelude::*, px};
use polterware_ui::patterns::{
    SETTINGS_SECTION_GAP, SettingsNavItem, settings_dialog, settings_section,
};
use polterware_ui::theme::{StatusTone, Theme};
use polterware_ui::{Button, ButtonKind, ButtonSize, Icon, TileTone, chip};

use super::super::Example;

fn row(label: &'static str, path: &'static str, cx: &App) -> AnyElement {
    let theme = Theme::global(cx);
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(16.0))
        .child(
            div()
                .flex()
                .flex_col()
                .child(div().text_size(px(13.0)).child(label))
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(theme.color_text_muted)
                        .child(path),
                ),
        )
        .child(chip("Current", StatusTone::Ok, cx))
        .into_any_element()
}

fn dialog(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let items = vec![
        SettingsNavItem::new("agents", "Agents")
            .icon(Icon::Settings)
            .group("Dispatch"),
        SettingsNavItem::new("classifier", "Classifier")
            .icon(Icon::AiBrain01Icon)
            .group("Dispatch"),
        SettingsNavItem::new("policy", "Routing policy")
            .icon(Icon::Sliders)
            .group("Dispatch"),
        SettingsNavItem::new("about", "About").group("App"),
    ];
    vec![
        div()
            .w(px(860.0))
            .child(
                settings_dialog(1.0)
                    .items(items)
                    .active("agents")
                    .on_select(Rc::new(|_, _, _| {}))
                    .on_close(Rc::new(|_, _| {}))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(SETTINGS_SECTION_GAP))
                            .child(
                                settings_section("Command line")
                                    .description("Agents delegate by running dispatch.")
                                    .icon(Icon::SquareTerminal, TileTone::Execution)
                                    .child(row("dispatch", "~/.local/bin/dispatch", cx)),
                            )
                            .child(
                                settings_section("Skill")
                                    .description("Teaches each agent when to delegate.")
                                    .icon(Icon::BookOpen, TileTone::Knowledge)
                                    .action(
                                        Button::new("install-all", ButtonKind::Secondary)
                                            .size(ButtonSize::Sm)
                                            .label("Install for all"),
                                    )
                                    .child(row(
                                        "Claude Code",
                                        "~/.claude/skills/dispatch/SKILL.md",
                                        cx,
                                    ))
                                    .child(row("Codex", "~/.agents/skills/dispatch/SKILL.md", cx)),
                            ),
                    ),
            )
            .into_any_element(),
    ]
}

pub fn examples() -> Vec<Example> {
    vec![Example {
        slug: "settings-dialog",
        title: "Sections and the open one",
        height: 560.0,
        render: dialog,
    }]
}
