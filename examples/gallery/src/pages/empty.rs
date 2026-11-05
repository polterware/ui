//! `PageEmpty` and its small form for a list: the mascot in each mood over what the page or
//! the list says.

use std::rc::Rc;

use gpui::{AnyElement, App, IntoElement, Window, div, prelude::*, px};
use polterware_ui::icon_tile::TileTone;
use polterware_ui::patterns::{PromptCard, prompt_card_grid, template_card_grid};
use polterware_ui::theme::Theme;
use polterware_ui::{Icon, MascotMood, list_empty, page_empty, page_empty_with};

use super::super::Example;

fn page(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    [
        ("No runs yet", MascotMood::Sleeping),
        ("Could not load the runs", MascotMood::Sad),
        ("No run matches", MascotMood::Lost),
    ]
    .into_iter()
    .map(|(title, mood)| {
        div()
            .w(px(260.0))
            .h(px(240.0))
            .child(page_empty(title, mood, cx))
            .into_any_element()
    })
    .collect()
}

/// `PromptCardGrid` under the mascot, as an empty Structure offers it: a press copies the
/// card's prompt.
fn prompts(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let cards = vec![
        PromptCard::new(
            "Map the project",
            "Propose domains and components from the repo.",
            Icon::Boxes,
            TileTone::Architecture,
        ),
        PromptCard::new(
            "Register deploys",
            "Link components to where they run per environment.",
            Icon::Blockchain01Icon,
            TileTone::Infrastructure,
        ),
    ];
    let grid = prompt_card_grid("gallery-prompts", cards, Rc::new(|_, _, _| {}), window, cx);
    vec![
        div()
            .w(px(600.0))
            .child(page_empty_with(
                Some("No structure yet"),
                MascotMood::Sleeping,
                Some(grid),
                cx,
            ))
            .into_any_element(),
    ]
}

/// `TemplateCardGrid` with no label, as an empty Environments page offers its templates: a
/// press creates one.
fn templates(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let cards = [
        (
            "Development",
            "Relaxed local development environment.",
            TileTone::Execution,
        ),
        (
            "Staging",
            "Standard pre-production environment.",
            TileTone::Infrastructure,
        ),
        (
            "Production",
            "Strict production environment.",
            TileTone::Security,
        ),
    ]
    .into_iter()
    .map(|(title, description, tone)| PromptCard::new(title, description, Icon::Server, tone))
    .collect();
    let grid = template_card_grid(
        "gallery-templates",
        None,
        cards,
        Rc::new(|_, _, _| {}),
        window,
        cx,
    );
    vec![
        div()
            .w(px(720.0))
            .child(page_empty_with(
                Some("No environments"),
                MascotMood::Sleeping,
                Some(grid),
                cx,
            ))
            .into_any_element(),
    ]
}

/// In a sidebar's width, under its section heading.
fn list(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let theme = *Theme::global(cx);
    [
        ("No runs to show.", MascotMood::Sleeping),
        ("Could not refresh runs and sections.", MascotMood::Sad),
    ]
    .into_iter()
    .map(|(text, mood)| {
        div()
            .w(px(240.0))
            .p(px(theme.space_2))
            .rounded(px(theme.radius_lg))
            .bg(theme.color_surface_subtle)
            .child(
                div()
                    .pl(px(theme.space_2))
                    .text_size(px(theme.font_size_sm))
                    .text_color(theme.color_text_muted)
                    .child("Runs"),
            )
            .child(list_empty(text, mood, cx))
            .into_any_element()
    })
    .collect()
}

pub fn examples() -> Vec<Example> {
    vec![
        Example {
            slug: "empty",
            title: "A page in each mood",
            height: 300.0,
            render: page,
        },
        Example {
            slug: "empty",
            title: "With prompts for an agent",
            height: 420.0,
            render: prompts,
        },
        Example {
            slug: "empty",
            title: "With templates and no label",
            height: 400.0,
            render: templates,
        },
        Example {
            slug: "empty-list",
            title: "A list in a sidebar",
            height: 220.0,
            render: list,
        },
    ]
}
