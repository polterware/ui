//! `data-table.docs.tsx`: the list in collapsible groups, with the selection column, the
//! active row's marker, and the selection toolbar, as Polterware's Inbox draws it. Click a
//! row to make it active (J and K then move it), tick boxes to see the toolbar, and click a
//! group's header to fold it. A second table has rows of their own heights: the runs with a
//! second line stand 46px tall, as Activity's do. Then a surface's archive (`ArchivedList`),
//! each row's "…" opening Restore and Delete permanently.

use std::collections::HashSet;
use std::rc::Rc;

use gpui::{
    AnyElement, App, Context, FontWeight, IntoElement, SharedString, Window, div, prelude::*, px,
};
use polterware_ui::patterns::{ArchivedItem, ArchivedList, selection_toolbar};
use polterware_ui::theme::{StatusTone, Theme};
use polterware_ui::{
    Button, ButtonKind, ButtonSize, Column, DataTable, Icon, IconTile, SelectionChange, TableGroup,
    TileTone, chip_rounded, primary_cell_ink, sans, text_width,
};

use super::super::Example;

/// The rows in group order: title, status, and whether it blocks.
const ROWS: [(&str, &str, bool); 5] = [
    ("Rename export snapshot", "Blocking", true),
    ("Wire search schema", "Blocking", true),
    ("Ship the event stream behind a flag?", "Blocking", true),
    ("Split ledger flag?", "Open", false),
    ("Add handoff index?", "Later", false),
];

#[derive(Default)]
struct Picks {
    selected: HashSet<usize>,
    active: Option<usize>,
}

fn grouped(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let theme = *Theme::global(cx);
    let picks = window.use_keyed_state("data-table-grouped", cx, |_, _: &mut Context<Picks>| {
        Picks {
            active: Some(0),
            ..Picks::default()
        }
    });
    let (selected, active) = {
        let picks = picks.read(cx);
        (picks.selected.clone(), picks.active)
    };
    let groups = vec![
        TableGroup::new("conflicts", "Conflicts", 0..2).icon(Icon::AlertDiamond),
        TableGroup::new("questions", "Questions", 2..5).icon(Icon::Scale),
    ];
    let font = sans(&theme, FontWeight::MEDIUM);
    let measure_font = font.clone();
    let select = picks.clone();
    let activate = picks.clone();
    let table = DataTable::new(
        "data-table-grouped-rows",
        vec![
            Column::new("Item").shrinks(),
            Column::new("Status").width(112.0),
        ],
        ROWS.len(),
        move |row, _, cx| {
            let theme = *Theme::global(cx);
            let (title, status, blocking) = ROWS[row.index];
            let icon = if row.index < 2 {
                Icon::AlertDiamond
            } else {
                Icon::Scale
            };
            let tone = if blocking {
                StatusTone::Danger
            } else {
                StatusTone::Muted
            };
            let ink = if status == "Later" {
                theme.color_text_muted
            } else {
                theme.color_text_primary
            };
            vec![
                primary_cell_ink(
                    IconTile::new(icon).size(24.0, 14.0).tone(if blocking {
                        TileTone::Security
                    } else {
                        TileTone::Neutral
                    }),
                    SharedString::from(title),
                    font.clone(),
                    ink,
                    None,
                ),
                chip_rounded(status, tone, 6.0, cx).into_any_element(),
            ]
        },
    )
    .measure(move |range, window, _| {
        let mut widest = [0.0_f32; 2];
        for (title, status, _) in ROWS.get(range).unwrap_or_default() {
            widest[0] = widest[0].max(34.0 + text_width(title, measure_font.clone(), 12.5, window));
            widest[1] =
                widest[1].max(20.0 + text_width(status, measure_font.clone(), 11.0, window));
        }
        widest.to_vec()
    })
    .groups(groups)
    .active(active)
    .selection(selected.iter().copied(), move |change, _, cx| {
        select.update(cx, |picks, cx| {
            match change {
                SelectionChange::Toggle(index) => {
                    if !picks.selected.remove(&index) {
                        picks.selected.insert(index);
                    }
                }
                SelectionChange::All(all) => {
                    picks.selected = if all {
                        (0..ROWS.len()).collect()
                    } else {
                        HashSet::new()
                    };
                }
            }
            cx.notify();
        });
    })
    .on_activate(move |index, _, cx| {
        activate.update(cx, |picks, cx| {
            picks.active = Some(index);
            cx.notify();
        });
    });

    let mut surface = div()
        .h(px(380.0))
        .w_full()
        .relative()
        .flex()
        .flex_col()
        .rounded(px(theme.radius_surface))
        .bg(theme.color_surface)
        .overflow_hidden()
        .child(table);
    if !selected.is_empty() {
        let clear = picks.clone();
        let actions = ["Later", "Ignore"]
            .into_iter()
            .map(|label| {
                Button::new(
                    SharedString::from(format!("data-table-bulk-{label}")),
                    ButtonKind::Secondary,
                )
                .size(ButtonSize::Sm)
                .label(label)
                .into_any_element()
            })
            .collect();
        surface = surface.child(selection_toolbar(
            selected.len(),
            actions,
            "No action fits every selected item",
            Rc::new(move |_, cx: &mut App| {
                clear.update(cx, |picks, cx| {
                    picks.selected.clear();
                    cx.notify();
                });
            }),
            cx,
        ));
    }
    vec![surface.into_any_element()]
}

/// Activity's changes: the summary, the run's second line when it has one, and the actor.
const CHANGES: [(&str, Option<&str>, &str); 6] = [
    (
        "Wire the export queue to the CLI",
        Some("Agent run (normal)"),
        "codex / normal",
    ),
    (
        "Tests failed in sync/worker_test.go",
        Some("Agent run (hard)"),
        "claude / hard",
    ),
    ("Update card-0065", None, "seed"),
    (
        "Split the sync worker",
        Some("Agent run (hard)"),
        "claude / hard",
    ),
    ("Update card-0090", None, "seed"),
    ("Update card-0075", None, "seed"),
];

/// A two-line cell in its 5px padding: 12.5px and 11.5px lines at 1.5.
const TWO_LINES: f32 = 10.0 + 12.5 * 1.5 + 11.5 * 1.5;

fn heights(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let theme = *Theme::global(cx);
    let active = window.use_keyed_state("data-table-heights", cx, |_, _| Some(0usize));
    let font = sans(&theme, FontWeight::NORMAL);
    let activate = active.clone();
    let table = DataTable::new(
        "data-table-heights-rows",
        vec![
            Column::new("Change").shrinks(),
            Column::new("Actor").width(160.0),
        ],
        CHANGES.len(),
        move |row, _, cx| {
            let theme = *Theme::global(cx);
            let (summary, run, actor) = CHANGES[row.index];
            vec![
                div()
                    .min_w(px(0.0))
                    .whitespace_nowrap()
                    .child(
                        div()
                            .text_size(px(12.5))
                            .line_height(px(18.75))
                            .child(summary),
                    )
                    .children(run.map(|run| {
                        div()
                            .text_size(px(11.5))
                            .line_height(px(17.25))
                            .text_color(theme.color_text_muted)
                            .child(run)
                    }))
                    .into_any_element(),
                div().child(actor).into_any_element(),
            ]
        },
    )
    .measure(move |range, window, _| {
        let mut widest = [0.0_f32; 2];
        for (summary, _, actor) in CHANGES.get(range).unwrap_or_default() {
            widest[0] = widest[0].max(text_width(summary, font.clone(), 12.5, window));
            widest[1] = widest[1].max(text_width(actor, font.clone(), 12.5, window));
        }
        widest.to_vec()
    })
    .row_heights(|row| {
        if CHANGES[row].1.is_some() {
            TWO_LINES
        } else {
            40.0
        }
    })
    .groups(vec![
        TableGroup::new("today", "Today", 0..3),
        TableGroup::new("yesterday", "Yesterday", 3..6),
    ])
    .active(*active.read(cx))
    .on_activate(move |index, _, cx| {
        activate.update(cx, |active, cx| {
            *active = Some(index);
            cx.notify();
        });
    });
    vec![
        div()
            .h(px(380.0))
            .w_full()
            .flex()
            .flex_col()
            .rounded(px(theme.radius_surface))
            .bg(theme.color_surface)
            .overflow_hidden()
            .child(table)
            .into_any_element(),
    ]
}

fn archive(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let list = window.use_keyed_state("gallery-archived", cx, |_, cx| {
        let mut list = ArchivedList::new("gallery-archived", Icon::FileText, cx);
        let item =
            |id: &'static str, title: &'static str, detail: &'static str, at: &'static str| {
                ArchivedItem {
                    id: id.into(),
                    title: title.into(),
                    detail: detail.into(),
                    archived_at: at.into(),
                }
            };
        list.set_items(
            vec![
                item("d1", "Release checklist", "release, runbook", "Nov 5, 2026"),
                item("d2", "Onboarding notes", "", "Nov 2, 2026"),
                item("d3", "Incident 0412", "incident", "Oct 28, 2026"),
            ],
            cx,
        );
        list
    });
    vec![div().w_full().h(px(180.0)).child(list).into_any_element()]
}

pub fn examples() -> Vec<Example> {
    vec![
        Example {
            slug: "data-table",
            title: "List rows in collapsible groups",
            height: 428.0,
            render: grouped,
        },
        Example {
            slug: "data-table-heights",
            title: "Rows of their own heights",
            height: 428.0,
            render: heights,
        },
        Example {
            slug: "archived-list",
            title: "An archive, each row restored or deleted",
            height: 240.0,
            render: archive,
        },
    ]
}
