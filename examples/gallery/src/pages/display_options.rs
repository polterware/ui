//! `display-options.docs.tsx`: the controls the filter menu includes after its fields.
use super::super::Example;
use gpui::{AnyElement, App, Context, IntoElement, Window, div, prelude::*, px};
use polterware_ui::display_options::{DisplayChoice, DisplayOptions, DisplaySettings};
fn panel(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let display = window.use_keyed_state(
        "display-options",
        cx,
        |_, cx: &mut Context<DisplayOptions>| {
            let mut display = DisplayOptions::new(cx);
            let choices = |pairs: &[(&str, &str)]| {
                pairs
                    .iter()
                    .map(|(v, l)| DisplayChoice::new(v.to_string(), l.to_string()))
                    .collect()
            };
            display.set_choices(
                choices(&[
                    ("none", "No grouping"),
                    ("stage", "Stage"),
                    ("milestone", "Milestone"),
                ]),
                choices(&[
                    ("updated", "Updated"),
                    ("title", "Title"),
                    ("stage", "Stage"),
                ]),
                choices(&[
                    ("slug", "Slug"),
                    ("context", "Context"),
                    ("stage", "Stage"),
                    ("tasks", "Tasks"),
                    ("milestone", "Milestone"),
                    ("updated", "Updated"),
                ]),
            );
            display.set_settings(
                DisplaySettings {
                    grouping: "stage".into(),
                    ordering: "updated".into(),
                    descending: true,
                    properties: vec![
                        "slug".into(),
                        "context".into(),
                        "stage".into(),
                        "tasks".into(),
                        "updated".into(),
                    ],
                },
                cx,
            );
            display
        },
    );
    let overlay = display.read(cx).overlay();
    vec![
        div()
            .w(px(320.0))
            .child(display)
            .child(overlay)
            .into_any_element(),
    ]
}
pub fn examples() -> Vec<Example> {
    vec![Example {
        slug: "display-options",
        title: "Grouping, ordering and properties",
        height: 280.0,
        render: panel,
    }]
}
