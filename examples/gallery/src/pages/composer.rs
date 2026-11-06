//! The Composer: the title field first, the description, three labelled fields (the last one
//! a wrapping value, long enough to take several lines), and the property pills (single choices and a set of tokens); then a pill's picker open; and the
//! confirmation on the same card, for a removal and for a plain change. Then the Composer
//! whole (`Composer`): a new record with its description, Context, Why, and Create more, and
//! an edit with its quiet action.

use gpui::{AnyElement, App, Context, Entity, IntoElement, SharedString, Window, div, prelude::*};
use polterware_ui::patterns::{
    Composer, ComposerDraft, ComposerSpec, ConfirmDialog, Property, PropertyOption, PropertyPills,
    composer_card, composer_field, composer_footer,
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

/// A Context of two domains, `picked` of them.
fn context(picked: &[&'static str]) -> Property {
    Property::tokens(
        "contexts",
        "Context",
        vec![
            PropertyOption::new("core", "Core").icon(Icon::Boxes),
            PropertyOption::new("web", "Web").icon(Icon::Boxes),
        ],
        picked.iter().map(|id| SharedString::from(*id)).collect(),
        "Whole project",
    )
    .loose()
    .icon(Icon::Layers)
}

fn whole(
    key: &'static str,
    spec: fn() -> ComposerSpec,
    picked: &'static [&'static str],
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let composer = window.use_keyed_state(key, cx, move |window, cx| {
        let mut composer = Composer::new(cx).inline();
        composer.open(spec(), vec![context(picked)], window, cx);
        composer
    });
    // The pills record where they drew, as the picker example's do.
    window.request_animation_frame();
    div().child(composer).into_any_element()
}

fn new_doc(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    vec![whole(
        "composer-whole-new",
        || {
            ComposerSpec::new("Doc title", "Create doc")
                .crumbs(["Polterware", "New doc"])
                .description("Write in Markdown…")
                .why("Optional. Recorded in Activity")
        },
        &[],
        window,
        cx,
    )]
}

fn edit_rule(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    vec![whole(
        "composer-whole-edit",
        || {
            ComposerSpec::new(
                "The API returns only fields declared in the contract",
                "Save rule",
            )
            .why("Optional. Recorded in Activity")
            .edit()
            .action("Remove rule")
            .initial(ComposerDraft {
                title: "Keep one writer per project".into(),
                ..ComposerDraft::default()
            })
        },
        &["core"],
        window,
        cx,
    )]
}

fn confirm(_: &mut Window, _: &mut App) -> Vec<AnyElement> {
    vec![
        ConfirmDialog::new(
            "gallery-confirm-kill",
            "Kill process 4182?",
            "This sends SIGTERM on the remote host.",
        )
        .recovery("The process stops only if the remote user is allowed to signal it.")
        .confirm_label("Kill")
        .danger()
        .into_any_element(),
        ConfirmDialog::new(
            "gallery-confirm-reset",
            "Reset the card layout?",
            "Every card goes back to its default size and place.",
        )
        .confirm_label("Reset")
        .into_any_element(),
    ]
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
        Example {
            slug: "composer-new",
            title: "The Composer whole: a new record",
            height: 380.0,
            render: new_doc,
        },
        Example {
            slug: "composer-edit",
            title: "The Composer whole: an edit with its action",
            height: 300.0,
            render: edit_rule,
        },
        Example {
            slug: "composer-confirm",
            title: "A confirmation, danger and plain",
            height: 640.0,
            render: confirm,
        },
    ]
}
