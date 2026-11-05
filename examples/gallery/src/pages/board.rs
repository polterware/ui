//! `work-board.tsx`: cards in stage columns. Drag a card's head 4px to lift it, drop it on
//! another column to move it there, or press Escape to put it back. The second board refuses
//! every move, so a dropped card glides back to its column.

use std::rc::Rc;

use gpui::{AnyElement, App, Context, Entity, IntoElement, Window, div, prelude::*, px};
use polterware_ui::patterns::{BoardCard, BoardColumn, BoardMove, board};
use polterware_ui::theme::Theme;
use polterware_ui::{Icon, IconTile};

use super::super::Example;

const STAGES: [(&str, &str, Icon); 5] = [
    ("shape", "Idea", Icon::Pencil),
    ("plan", "Planned", Icon::Route),
    ("build", "In progress", Icon::Hammer),
    ("verify", "In review", Icon::ShieldCheck),
    ("delivered", "Done", Icon::Check),
];

struct Cards {
    /// Each card's title, slug, and stage, in the store's order.
    cards: Vec<(&'static str, &'static str, &'static str)>,
}

fn cards(id: &'static str, window: &mut Window, cx: &mut App) -> Entity<Cards> {
    window.use_keyed_state(id, cx, |_, _: &mut Context<Cards>| Cards {
        cards: vec![
            ("Export queue", "export-queue", "shape"),
            ("Retry failed syncs", "retry-failed-syncs", "plan"),
            ("Audit the token store", "audit-token-store", "build"),
            ("Offline drafts", "offline-drafts", "build"),
            ("Ship the event stream", "event-stream", "delivered"),
        ],
    })
}

fn stage_board(
    id: &'static str,
    refuse: bool,
    window: &mut Window,
    cx: &mut App,
) -> Vec<AnyElement> {
    let state = cards(id, window, cx);
    let theme = *Theme::global(cx);
    let tints = [
        theme.chart_amber,
        theme.chart_blue,
        theme.chart_cyan,
        theme.chart_violet,
        theme.chart_green,
    ];
    let listed = state.read(cx).cards.clone();
    let mut built = board(id);
    for ((key, label, icon), tint) in STAGES.into_iter().zip(tints) {
        let column = listed
            .iter()
            .enumerate()
            .filter(|(_, (_, _, stage))| *stage == key)
            .map(|(order, (title, slug, _))| BoardCard::new(*slug, *title, *slug, order));
        built = built.column(
            BoardColumn::new(key, label, IconTile::new(icon).tint(tint))
                .cards(column)
                .on_add(Rc::new(|_, _| {})),
        );
    }
    let moves = state.clone();
    let built = built.on_move(Rc::new(move |change: BoardMove, _, cx: &mut App| {
        if refuse {
            change.settle(false, cx);
            return;
        }
        moves.update(cx, |state, cx| {
            if let Some(card) = state
                .cards
                .iter_mut()
                .find(|card| card.1 == change.card.as_ref())
                && let Some((stage, _, _)) = STAGES
                    .iter()
                    .find(|(key, _, _)| *key == change.column.as_ref())
            {
                card.2 = stage;
            }
            cx.notify();
        });
        change.settle(true, cx);
    }));
    vec![
        div()
            .w_full()
            .h(px(300.0))
            .child(built.build(window, cx))
            .into_any_element(),
    ]
}

fn moves(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    stage_board("board-moves", false, window, cx)
}

fn refused(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    stage_board("board-refused", true, window, cx)
}

pub fn examples() -> Vec<Example> {
    let example = |title, render| Example {
        slug: "board",
        title,
        height: 360.0,
        render,
    };
    vec![
        example("Move cards between stages", moves),
        example("A refused move glides back", refused),
    ]
}
