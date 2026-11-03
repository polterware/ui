//! `button.docs.tsx`: variants, tones, sizes, icon buttons, the texture variant, disabled,
//! and the link button, as Dispatch draws them (under `TextureFaceProvider`).

use gpui::{AnyElement, App, IntoElement, Window};
use polterware_ui::{Button, ButtonKind, ButtonSize, Icon};

use super::super::Example;

fn row(buttons: Vec<Button>) -> Vec<AnyElement> {
    buttons
        .into_iter()
        .map(IntoElement::into_any_element)
        .collect()
}

fn labeled(id: &'static str, kind: ButtonKind, label: &'static str) -> Button {
    Button::new(id, kind).label(label)
}

fn plus(id: &'static str, kind: ButtonKind, label: &'static str) -> Button {
    Button::new(id, kind)
        .size(ButtonSize::Icon)
        .icon(Icon::Plus)
        .tooltip(label)
}

fn variants(_: &mut Window, _: &mut App) -> Vec<AnyElement> {
    row(vec![
        labeled("variant-primary", ButtonKind::Primary, "Primary"),
        labeled("variant-secondary", ButtonKind::Secondary, "Secondary"),
        labeled("variant-brand", ButtonKind::Brand, "Brand"),
        labeled("variant-danger", ButtonKind::Danger, "Danger"),
        labeled("variant-ghost", ButtonKind::Ghost, "Ghost"),
    ])
}

fn tones(_: &mut Window, _: &mut App) -> Vec<AnyElement> {
    // Violet, teal, and pink have no texture tone, so they draw neutral, as in the app.
    row(vec![
        labeled("tone-neutral", ButtonKind::Secondary, "Neutral"),
        labeled("tone-success", ButtonKind::Success, "Success"),
        labeled("tone-warn", ButtonKind::Warn, "Warn"),
        labeled("tone-info", ButtonKind::Info, "Info"),
        labeled("tone-critical", ButtonKind::Danger, "Critical"),
        labeled("tone-violet", ButtonKind::Secondary, "Violet"),
        labeled("tone-teal", ButtonKind::Secondary, "Teal"),
        labeled("tone-pink", ButtonKind::Secondary, "Pink"),
    ])
}

fn sizes(_: &mut Window, _: &mut App) -> Vec<AnyElement> {
    row(vec![
        labeled("size-sm", ButtonKind::Secondary, "Small").size(ButtonSize::Sm),
        labeled("size-md", ButtonKind::Secondary, "Medium"),
    ])
}

fn icon(_: &mut Window, _: &mut App) -> Vec<AnyElement> {
    row(vec![plus("icon-add", ButtonKind::Ghost, "Add item")])
}

fn icon_sizes(_: &mut Window, _: &mut App) -> Vec<AnyElement> {
    row(vec![
        plus("icon-ghost", ButtonKind::Ghost, "Add ghost"),
        plus("icon-secondary", ButtonKind::Secondary, "Add secondary"),
        plus("icon-danger", ButtonKind::Danger, "Add danger"),
    ])
}

fn texture(_: &mut Window, _: &mut App) -> Vec<AnyElement> {
    row(vec![
        labeled("texture-neutral", ButtonKind::Secondary, "Neutral"),
        labeled("texture-brand", ButtonKind::Inverse, "Brand"),
        labeled("texture-danger", ButtonKind::Danger, "Danger"),
        labeled("texture-success", ButtonKind::Success, "Success"),
        labeled("texture-warn", ButtonKind::Warn, "Warn"),
        labeled("texture-info", ButtonKind::Info, "Info"),
        plus("texture-icon", ButtonKind::Secondary, "Add"),
        plus(
            "texture-icon-selected",
            ButtonKind::Secondary,
            "Filter (applied)",
        )
        .selected(true),
        labeled("texture-disabled", ButtonKind::Secondary, "Disabled").disabled(true),
    ])
}

fn disabled(_: &mut Window, _: &mut App) -> Vec<AnyElement> {
    row(vec![
        labeled("disabled-secondary", ButtonKind::Secondary, "Secondary").disabled(true),
        labeled("disabled-primary", ButtonKind::Primary, "Primary").disabled(true),
    ])
}

fn link(_: &mut Window, _: &mut App) -> Vec<AnyElement> {
    row(vec![labeled(
        "link-open",
        ButtonKind::Secondary,
        "Open site",
    )])
}

pub fn examples() -> Vec<Example> {
    let example = |title, render| Example {
        slug: "button",
        title,
        height: 120.0,
        render,
    };
    vec![
        example("Variants", variants),
        example("Tones", tones),
        example("Sizes", sizes),
        example("Icon button", icon),
        example("Icon size", icon_sizes),
        example("Texture variant", texture),
        example("Disabled", disabled),
        example("Link button", link),
    ]
}
