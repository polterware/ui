//! The settings dialog: grouped sections in the rail, the open one lit, and its panel with
//! Close, the heading, and the page's sections as cards (`settings_section`), each with its
//! tile, title, description, an optional action, and its rows; then a picker opened from a
//! row's "+", hung in the dialog's `overlay` and lined up with the "+"'s right edge.

use std::rc::Rc;

use gpui::{AnyElement, App, IntoElement, Window, div, prelude::*, px};
use polterware_ui::hang::hang;
use polterware_ui::patterns::{
    AnchorSlot, SETTINGS_SECTION_GAP, SettingsNavItem, settings_dialog, settings_section,
    window_anchor,
};
use polterware_ui::theme::{StatusTone, Theme};
use polterware_ui::{Button, ButtonKind, ButtonSize, Icon, TileTone, chip, icon_button};

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

/// A row whose "+" has a picker open: the panel hangs in the dialog's overlay, over the rows
/// after it, its right edge on the "+"'s.
fn picker(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let theme = *Theme::global(cx);
    let anchor = window
        .use_keyed_state("gallery-settings-anchor", cx, |_, _| AnchorSlot::default())
        .read(cx)
        .clone();
    let plus = div().relative().child(anchor.probe()).child(icon_button(
        "gallery-add",
        Icon::Plus,
        "Add",
        Rc::new(|_, _| {}),
        cx,
    ));
    let rule = |label: &'static str, end: AnyElement| {
        div()
            .flex()
            .items_center()
            .justify_between()
            .h(px(44.0))
            .child(div().text_size(px(13.0)).child(label))
            .child(end)
            .into_any_element()
    };
    let any = |cx: &App| {
        div()
            .text_size(px(11.5))
            .text_color(Theme::global(cx).color_text_muted)
            .child("Any")
            .into_any_element()
    };
    let panel = div()
        .occlude()
        .w(px(220.0))
        .p(px(theme.space_2))
        .rounded(px(theme.radius_lg))
        .bg(theme.color_surface_raised)
        .border_1()
        .border_color(theme.color_border)
        .flex()
        .flex_col()
        .children(["anthropic", "google", "openai"].map(|name| {
            div()
                .h(px(theme.control_height_md))
                .px(px(theme.space_2))
                .flex()
                .items_center()
                .text_size(px(theme.font_size_sm))
                .child(name)
        }));
    let mut dialog = settings_dialog(1.0)
        .items(vec![
            SettingsNavItem::new("policy", "Routing policy").icon(Icon::Sliders),
        ])
        .active("policy")
        .on_select(Rc::new(|_, _, _| {}))
        .on_close(Rc::new(|_, _| {}))
        .child(
            settings_section("Allow")
                .description("When a list has values, runs use only those.")
                .icon(Icon::FirewallIcon, TileTone::Security)
                .child(rule("Providers", plus.into_any_element()))
                .child(rule("Models", any(cx)))
                .child(rule("Agents", any(cx))),
        );
    if let Some(bounds) = anchor.get() {
        dialog = dialog.overlay(window_anchor(
            bounds,
            hang(f32::from(bounds.size.height) + 4.0, 0.0, panel).align_end(28.0),
        ));
    }
    vec![div().w(px(860.0)).child(dialog).into_any_element()]
}

pub fn examples() -> Vec<Example> {
    vec![
        Example {
            slug: "settings-dialog",
            title: "Sections and the open one",
            height: 560.0,
            render: dialog,
        },
        Example {
            slug: "settings-dialog-picker",
            title: "A picker over the rows after it",
            height: 420.0,
            render: picker,
        },
    ]
}
