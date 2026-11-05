//! An app without a sidebar (Vitals): the title bar's strip of tabs, the start page under
//! it, and the small dialog a form or a question opens in.

use std::rc::Rc;

use gpui::{AnyElement, App, IntoElement, Window, div, prelude::*, px};
use polterware_ui::MascotMood;
use polterware_ui::button::{Button, ButtonKind};
use polterware_ui::icons::Icon;
use polterware_ui::patterns::{
    DialogSize, StartRow, TitleTab, dialog, field, form_stack, settings_row, start_page,
    start_section, title_add, title_card, title_divider, title_spacer, title_strip,
};
use polterware_ui::theme::Theme;

use super::super::Example;

fn noop() -> polterware_ui::Handler {
    Rc::new(|_, _| {})
}

/// Two open tabs, the second open, then "+" and the Settings card at the end.
fn strip(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let theme = *Theme::global(cx);
    let children = vec![
        title_divider(cx),
        TitleTab::new("gallery-tab-prod", "prod-1")
            .on_select(noop())
            .on_close(noop())
            .into_any_element(),
        TitleTab::new("gallery-tab-staging", "staging")
            .active(true)
            .on_select(noop())
            .on_close(noop())
            .into_any_element(),
        title_add("gallery-tab-add", false, noop(), window, cx),
        title_spacer(),
        title_card(
            "gallery-tab-settings",
            Icon::Settings,
            false,
            noop(),
            window,
            cx,
        ),
    ];
    vec![
        div()
            .w(px(860.0))
            .rounded(px(theme.radius_lg))
            .bg(theme.color_shell_background)
            .child(title_strip("gallery-strip", 0.0, children, cx))
            .into_any_element(),
    ]
}

/// The main action over Recents and Saved.
fn start(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let row = |id: &'static str, title: &'static str, detail: &'static str| {
        StartRow::new(id, Icon::Server, title)
            .detail(detail)
            .on_open(noop())
            .on_more(Rc::new(|_, _, _| {}))
            .into_any_element()
    };
    let body = vec![
        Button::new("gallery-start-connect", ButtonKind::Secondary)
            .label("Connect a host")
            .icon(Icon::Server)
            .icon_size(16.0)
            .full_width()
            .into_any_element(),
        start_section(
            "Recents",
            vec![row("gallery-start-prod", "prod-1", "root@10.0.0.4")],
            cx,
        ),
        start_section(
            "Saved",
            vec![row("gallery-start-lab", "lab", "erick@lab.local")],
            cx,
        ),
    ];
    vec![
        div()
            .w(px(860.0))
            .h(px(420.0))
            .child(start_page(MascotMood::Sleeping, body, cx))
            .into_any_element(),
    ]
}

/// `A11yDialog size="sm"`: a short form, its fields stacked, Cancel and the submit below.
fn small_dialog(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let input = |_: &mut Window, cx: &mut App, text: &'static str| {
        div()
            .h(px(32.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .rounded_full()
            .bg(Theme::global(cx).field_fill_top)
            .text_size(px(12.0))
            .child(text)
            .into_any_element()
    };
    let fields = vec![
        field("Host", input(window, cx, "192.168.1.10")).into_any_element(),
        field("User", input(window, cx, "root"))
            .error(Some("User is required"))
            .into_any_element(),
        settings_row("Save server").into_any_element(),
    ];
    vec![
        dialog("Connect", 1.0)
            .size(DialogSize::Sm)
            .on_close(noop())
            .footer([
                Button::new("gallery-dialog-cancel", ButtonKind::Danger)
                    .label("Cancel")
                    .into_any_element(),
                Button::new("gallery-dialog-submit", ButtonKind::Inverse)
                    .label("Connect")
                    .into_any_element(),
            ])
            .child(form_stack(fields))
            .into_any_element(),
    ]
}

pub fn examples() -> Vec<Example> {
    vec![
        Example {
            slug: "title-strip",
            title: "The title bar's tabs",
            height: 120.0,
            render: strip,
        },
        Example {
            slug: "start-page",
            title: "A start page",
            height: 480.0,
            render: start,
        },
        Example {
            slug: "dialog-sm",
            title: "A small dialog",
            height: 420.0,
            render: small_dialog,
        },
    ]
}
