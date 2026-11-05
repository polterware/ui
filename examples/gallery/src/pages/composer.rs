//! The Composer: the title field first, the description, three labelled fields (the last one
//! a wrapping value, long enough to take several lines), and the property pills (single
//! choices and a set of tokens); then a pill's picker open.

use gpui::{AnyElement, App, Context, Entity, IntoElement, SharedString, Window, div, prelude::*};
use polterware_ui::patterns::{
    Property, PropertyOption, PropertyPills, composer_card, composer_field, composer_footer,
};
use polterware_ui::theme::Theme;
use polterware_ui::{ButtonKind, Icon, TextInput, button};

use super::super::Example;

struct Fields {
    title: Entity<TextInput>,
    description: Entity<TextInput>,
    when: Entity<TextInput>,
    notes: Entity<TextInput>,
    pills: Entity<PropertyPills>,
}

fn options(pairs: &[(&'static str, &'static str)]) -> Vec<PropertyOption> {
    pairs
        .iter()
        .map(|(value, label)| PropertyOption::new(*value, *label))
        .collect()
}

fn fields(id: &'static str, picking: bool, window: &mut Window, cx: &mut App) -> Fields {
    let input = |key: &'static str, build: fn(TextInput) -> TextInput, text: &'static str| {
        move |window: &mut Window, cx: &mut App| {
            window.use_keyed_state(
                SharedString::from(format!("{id}-{key}")),
                cx,
                move |_, cx: &mut Context<TextInput>| {
                    let mut input = build(TextInput::new(cx));
                    input.set_text(text, cx);
                    input
                },
            )
        }
    };
    let title = input("title", |input| input.bare().title(), "reviewer")(window, cx);
    let description = input(
        "description",
        |input| input.description(),
        "Review a diff for bugs before it merges.",
    )(window, cx);
    let when = input(
        "when",
        |input| input.inline().placeholder("The tasks it fits"),
        "",
    )(window, cx);
    let notes = input(
        "notes",
        |input| {
            input
                .inline()
                .wrap()
                .placeholder("What the reviewer should look for")
        },
        "Read the whole diff before commenting. Look for bugs, missing tests, and risky changes, \
         say what to fix first and why it matters, and keep the report short enough to read in \
         one pass.",
    )(window, cx);
    let pills = window.use_keyed_state(
        SharedString::from(format!("{id}-pills")),
        cx,
        move |window, cx| {
            let mut pills = PropertyPills::new(cx);
            pills.set_properties(
                vec![
                    Property::single(
                        "tier",
                        "Lowest tier",
                        options(&[("", "Any tier"), ("easy", "Easy"), ("hard", "Hard")]),
                        "hard",
                    ),
                    Property::single(
                        "access",
                        "Access",
                        options(&[("edit", "Can change files"), ("readonly", "Read only")]),
                        "readonly",
                    ),
                    Property::tokens(
                        "prefer",
                        "Prefer",
                        options(&[
                            ("codex", "codex"),
                            ("claude", "claude"),
                            ("cursor", "cursor"),
                        ]),
                        vec!["claude".into()],
                        "No preference",
                    )
                    .icon(Icon::Bot),
                ],
                cx,
            );
            if picking {
                pills.toggle(0, window, cx);
            }
            pills
        },
    );
    Fields {
        title,
        description,
        when,
        notes,
        pills,
    }
}

fn composer(id: &'static str, picking: bool, window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = *Theme::global(cx);
    let fields = fields(id, picking, window, cx);
    // The pills record where they drew, so a picker opened on the first frame finds its pill
    // on the next one.
    window.request_animation_frame();
    let overlay = fields.pills.read(cx).overlay(cx);
    div()
        .child(
            composer_card("New role", 1.0, 0.0, &theme)
                .on_close(std::rc::Rc::new(|_, _| {}))
                .child(fields.title)
                .child(fields.description)
                .child(composer_field("When to use", fields.when, &theme))
                .child(composer_field("Instructions", fields.notes, &theme))
                .child(div().pt_1().child(fields.pills))
                .footer(composer_footer(
                    [],
                    [button(
                        "composer-submit",
                        "Create role",
                        ButtonKind::Primary,
                        std::rc::Rc::new(|_, _| {}),
                        cx,
                    )
                    .into_any_element()],
                    &theme,
                ))
                .overlay(overlay),
        )
        .into_any_element()
}

fn default(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    vec![composer("composer-default", false, window, cx)]
}

fn picking(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    vec![composer("composer-picking", true, window, cx)]
}

pub fn examples() -> Vec<Example> {
    vec![
        Example {
            slug: "composer",
            title: "Title, description, fields (one wrapping), and pills",
            height: 440.0,
            render: default,
        },
        Example {
            slug: "composer-picker",
            title: "A pill's picker open",
            height: 560.0,
            render: picking,
        },
    ]
}
