//! A Knowledge document's parts: its Markdown read-only and edited in place (`MarkdownEditor
//! class="pw-md-editor--document"`), the page they sit on (`RailPage` with its properties
//! rail, a drawer over the text on the example's narrow page), its Context as tokens (`ContextPicker` on `TokenPicker`), the tags (`TagPicker`),
//! and the confirm that asks why (`requestReason`). The editor's tools are shown open, as the
//! Solid gallery cannot script them: the slash menu on the block the gutter's "+" opens, the
//! selection toolbar over selected text, and a code block edited in place.

use std::rc::Rc;

use gpui::{AnyElement, App, IntoElement, Window, div, prelude::*, px};
use polterware_ui::markdown::{MarkdownEditor, markdown_view};
use polterware_ui::patterns::{
    ConfirmDialog, DocumentPage, Property, PropertyOption, PropertyPills, RAIL_BUTTON_RADIUS,
    RailState, TagPicker, document_footer, document_group, document_group_body, document_title,
    rail_properties, rail_property,
};
use polterware_ui::theme::StatusTone;
use polterware_ui::{Button, ButtonKind, ButtonSize, Icon, TextInput, chip_rounded, wrap_text};

use super::super::Example;

const SAMPLE: &str = "## Context\n\nThe export queue runs **twice** when a retry lands *during* a flush; see `sync/worker.go` and [the incident](https://example.com).\n\n- Keep one writer per project\n- Retry with backoff\n  - capped at 30s\n1. Drain the queue\n2. Swap the worker\n\n- [x] Reproduce it\n- [ ] Ship the fix";
const BLOCKS: &str = "> A queue that runs twice is worse than one that runs late.\n\n```go\nfunc drain(q *Queue) {\n\tq.Flush()\n}\n```\n\n---\n\nConsequences follow.";

fn view(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    vec![
        div()
            .w(px(654.0))
            .child(markdown_view(SAMPLE, cx))
            .into_any_element(),
    ]
}

fn blocks(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    vec![
        div()
            .w(px(654.0))
            .child(markdown_view(BLOCKS, cx))
            .into_any_element(),
    ]
}

fn editor(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let editor = window.use_keyed_state("gallery-markdown-editor", cx, |_, cx| {
        MarkdownEditor::new("Click a block to edit its Markdown.\n\n- Enter continues a list\n- [ ] A task checks off from its box", cx)
    });
    let empty = window.use_keyed_state("gallery-markdown-empty", cx, |_, cx| {
        MarkdownEditor::new("", cx)
    });
    vec![
        div().w(px(420.0)).child(editor).into_any_element(),
        div().w(px(360.0)).child(empty).into_any_element(),
    ]
}

/// An editor on `source` that `start` opens a field in once, after its first frame.
fn opened(
    key: &'static str,
    source: &'static str,
    start: fn(&mut MarkdownEditor, &mut Window, &mut gpui::Context<MarkdownEditor>),
    window: &mut Window,
    cx: &mut App,
) -> gpui::Entity<MarkdownEditor> {
    let editor = window.use_keyed_state(key, cx, |_, cx| MarkdownEditor::new(source, cx));
    let started = window.use_keyed_state(
        gpui::ElementId::Name(format!("{key}-started").into()),
        cx,
        |_, _| false,
    );
    if !*started.read(cx) {
        started.update(cx, |started, _| *started = true);
        let editor = editor.clone();
        window.defer(cx, move |window, cx| {
            editor.update(cx, |editor, cx| start(editor, window, cx));
        });
    }
    editor
}

fn tools(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let slash = opened(
        "gallery-markdown-slash",
        "Keep one writer per project.\n\nThe queue drains before the swap.",
        |editor, window, cx| editor.insert_below(0, window, cx),
        window,
        cx,
    );
    // At the box's top, so the menu has its room below the caret.
    vec![div().w(px(560.0)).h_full().child(slash).into_any_element()]
}

fn toolbar(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let editor = opened(
        "gallery-markdown-toolbar",
        "Keep **project evidence** close to the work.\n\n- [x] Register sources",
        |editor, window, cx| {
            editor.edit_block(0, window, cx);
            editor.select(5..25, cx);
        },
        window,
        cx,
    );
    vec![
        div()
            .w(px(560.0))
            .pt(px(48.0))
            .child(editor)
            .into_any_element(),
    ]
}

fn code(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let editor = opened(
        "gallery-markdown-code",
        "Drain it first:\n\n```go\nfunc drain(q *Queue) {\n\tq.Flush()\n}\n```",
        |editor, window, cx| editor.edit_block(1, window, cx),
        window,
        cx,
    );
    vec![div().w(px(560.0)).child(editor).into_any_element()]
}

/// A Context of `picked` among two domains and a component.
fn context(
    key: &'static str,
    picked: &[&'static str],
    window: &mut Window,
    cx: &mut App,
) -> gpui::Entity<PropertyPills> {
    let picked: Vec<gpui::SharedString> = picked.iter().map(|id| (*id).into()).collect();
    window.use_keyed_state(key, cx, move |_, cx| {
        let mut pills = PropertyPills::new(cx);
        let options = vec![
            PropertyOption::new("core", "Core").icon(Icon::Boxes),
            PropertyOption::new("web", "Web").icon(Icon::Boxes),
            PropertyOption::new("api", "Api").icon(Icon::Blockchain01Icon),
        ];
        pills.set_properties(
            vec![
                Property::tokens("context", "Context", options, picked, "Whole project")
                    .loose()
                    .icon(Icon::Layers),
            ],
            cx,
        );
        pills
    })
}

fn page(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let scope = context("gallery-document-context", &[], window, cx);
    let tags = window.use_keyed_state("gallery-document-tags", cx, |_, cx| {
        let mut picker = TagPicker::new("gallery-document-tags", cx);
        picker.set_tags(vec!["runbook".into()], cx);
        picker
    });
    let rail = window.use_keyed_state("gallery-document-rail", cx, |_, _| RailState::new(true));
    // The example's 872px page is narrow: its drawer opens once, as its toggle opens it.
    let opened = window.use_keyed_state("gallery-document-drawer", cx, |_, _| false);
    if rail.read(cx).page().is_narrow() && !*opened.read(cx) {
        opened.update(cx, |opened, _| *opened = true);
        rail.update(cx, |rail, _| rail.toggle());
    }
    let status = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(6.0))
        .child(chip_rounded("Active", StatusTone::Ok, 6.0, cx))
        .child(
            Button::new("gallery-document-review", ButtonKind::Ghost)
                .size(ButtonSize::Sm)
                .label("Open review"),
        );
    let document = DocumentPage::new("gallery-document")
        .on_back(Rc::new(|_, _| {}))
        .rail_state(&rail)
        .rail(document_group(
            "Properties",
            Icon::InformationCircle,
            rail_properties()
                .child(rail_property("Status", status, false, cx))
                .child(rail_property(
                    "Reason",
                    wrap_text("It held up under the load test"),
                    false,
                    cx,
                ))
                .child(rail_property("Context", scope, false, cx)),
            cx,
        ))
        .rail(document_group(
            "Labels",
            Icon::Tag,
            document_group_body(tags),
            cx,
        ))
        .rail(document_footer(
            [
                Button::new("gallery-document-archive", ButtonKind::Secondary)
                    .size(ButtonSize::Sm)
                    .radius(RAIL_BUTTON_RADIUS)
                    .label("Archive")
                    .into_any_element(),
            ],
            cx,
        ))
        .child(document_title("Decision 0006: Validate brief audit", cx))
        .child(markdown_view(
            "The brief is audited before an agent reads it.\n\n- Rules first\n- Then the decisions",
            cx,
        ));
    vec![
        div()
            .w(px(872.0))
            .h(px(420.0))
            .child(document)
            .into_any_element(),
    ]
}

fn tags(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let picker = window.use_keyed_state("gallery-tag-picker", cx, |_, cx| {
        let mut picker = TagPicker::new("gallery-tag-picker", cx);
        picker.set_tags(vec!["runbook".into(), "release".into()], cx);
        picker.set_suggestions(vec![
            "audit".into(),
            "onboarding".into(),
            "release".into(),
            "runbook".into(),
        ]);
        picker
    });
    vec![div().w(px(360.0)).child(picker).into_any_element()]
}

fn contexts(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let whole = context("gallery-context-whole", &[], window, cx);
    let picked = context("gallery-context-picked", &["core", "api"], window, cx);
    vec![
        div().w(px(240.0)).child(whole).into_any_element(),
        div().w(px(240.0)).child(picked).into_any_element(),
    ]
}

/// Structure's head tokens: a Technology a typed name adds to, and Deploys whose tokens open
/// their Composer, lead out to their platform, and show their latest check.
fn head_tokens(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let pills = window.use_keyed_state("gallery-head-tokens", cx, |_, cx| {
        let mut pills = PropertyPills::new(cx);
        let techs = ["Go", "Postgres", "TypeScript"]
            .map(|tech| PropertyOption::new(tech, tech).icon(Icon::Cpu))
            .to_vec();
        let deploys = vec![
            PropertyOption::new("0", "Vercel · Production")
                .icon(Icon::Cloud)
                .hint("Live")
                .opens("Open in Vercel"),
            PropertyOption::new("1", "Fly.io · Staging")
                .icon(Icon::Cloud)
                .hint("Failed"),
        ];
        pills.set_properties(
            vec![
                Property::tokens("tech", "Technology", techs, vec!["Go".into()], "Technology")
                    .loose()
                    .creatable()
                    .icon(Icon::Cpu),
                Property::tokens(
                    "deploys",
                    "Deploys",
                    deploys,
                    vec!["0".into(), "1".into()],
                    "Where it runs",
                )
                .loose()
                .editable()
                .adds()
                .icon(Icon::Cloud),
            ],
            cx,
        );
        pills
    });
    let overlay = pills.read(cx).overlay(cx);
    vec![
        div()
            .w(px(520.0))
            .child(pills)
            .children(overlay)
            .into_any_element(),
    ]
}

fn reason(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let blank = window.use_keyed_state("gallery-reason-blank", cx, |_, cx| {
        TextInput::new(cx)
            .wrap()
            .placeholder("What should the review look at?")
    });
    let given = window.use_keyed_state("gallery-reason-given", cx, |_, cx| {
        let mut field = TextInput::new(cx).wrap();
        field.set_text("The cache it relies on is gone.", cx);
        field
    });
    let ask = |id: &'static str, field| {
        ConfirmDialog::new(
            id,
            "Open a review of “Decision 0006”?",
            "It stays under review until someone keeps or replaces it. Say what is wrong with it.",
        )
        .confirm_label("Open review")
        .reason(field)
        .into_any_element()
    };
    vec![
        ask("gallery-reason-blank", &blank),
        ask("gallery-reason-given", &given),
    ]
}

pub fn examples() -> Vec<Example> {
    vec![
        Example {
            slug: "markdown",
            title: "Markdown in the document style",
            height: 440.0,
            render: view,
        },
        Example {
            slug: "markdown-blocks",
            title: "A quote, a code block, and a rule",
            height: 280.0,
            render: blocks,
        },
        Example {
            slug: "markdown-editor",
            title: "Edited a block at a time, and empty",
            height: 260.0,
            render: editor,
        },
        Example {
            slug: "markdown-editor",
            title: "The slash menu on the block the gutter's plus opened",
            height: 420.0,
            render: tools,
        },
        Example {
            slug: "markdown-editor",
            title: "The selection toolbar over selected text",
            height: 220.0,
            render: toolbar,
        },
        Example {
            slug: "markdown-editor",
            title: "A code block edited in place",
            height: 240.0,
            render: code,
        },
        Example {
            slug: "document-page",
            title: "A narrow document page, its rail as a drawer",
            height: 470.0,
            render: page,
        },
        Example {
            slug: "tag-picker",
            title: "Tags, with Add tag",
            height: 120.0,
            render: tags,
        },
        Example {
            slug: "context-picker",
            title: "Context as tokens, the whole project and picked",
            height: 120.0,
            render: contexts,
        },
        Example {
            slug: "context-picker",
            title: "Tokens a name adds to, and tokens that edit and open",
            height: 140.0,
            render: head_tokens,
        },
        Example {
            slug: "confirm-reason",
            title: "A confirm that asks why, blank and answered",
            height: 300.0,
            render: reason,
        },
    ]
}
