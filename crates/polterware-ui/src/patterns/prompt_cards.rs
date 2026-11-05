//! `PromptCardGrid` on `TemplateCardGrid` (`prompt-card.tsx`, `template-card.tsx`,
//! `template-card.css`): cards that hand a ready-made prompt to the person's coding agent, so
//! an empty page fills from the real project instead of a fixed template.
//!
//! The grid is at most 44rem wide and centered, its cards up to 14rem each (`minmax(11.5rem,
//! 14rem)`), wrapping .75rem apart, under a label (`Copy a prompt for your agent`: 11px/500
//! muted, upper case, .04em apart). A card (`.pw-template-card` on `.card-texture`) is the
//! raised card face, .875rem in, its 28px icon tile over the title (13px/600, 1.3) and the
//! description (12px muted, 1.4), centered and .625rem apart. Under the pointer its lift grows
//! (`0 4px 12px`). The cards enter once, each 40ms after the one before (at most 240ms): they
//! rise 6px and fade in over 200ms on the enter ease (`cubic-bezier(0.16, 1, 0.3, 1)`).
//!
//! A press picks the card; the host copies its prompt and says so (`copyAgentPrompt`).
//!
//! [`template_card_grid`] is the same grid under its own label, or none
//! (`TemplateCardGrid`'s `label`), such as the environment templates an empty Environments
//! page offers.

use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    AnyElement, App, BoxShadow, ElementId, FontWeight, IntoElement, SharedString, Window, div,
    point, prelude::*, px,
};

use crate::icon_tile::{IconTile, TileTone};
use crate::icons::Icon;
use crate::material::raised_panel;
use crate::theme::{Ease, Theme, fade, rem};

/// `--ease-enter`.
const EASE_ENTER: Ease = Ease(0.16, 1.0, 0.3, 1.0);
/// `--duration-enter`.
const ENTER: Duration = Duration::from_millis(200);
/// Each card's delay after the one before, and the most any waits.
const STAGGER: Duration = Duration::from_millis(40);
const MOST_DELAY: Duration = Duration::from_millis(240);

/// One card: what it is called, what it does, and its tile.
#[derive(Clone, Debug)]
pub struct PromptCard {
    pub title: SharedString,
    pub description: SharedString,
    pub icon: Icon,
    pub tone: TileTone,
}

impl PromptCard {
    pub fn new(
        title: impl Into<SharedString>,
        description: impl Into<SharedString>,
        icon: Icon,
        tone: TileTone,
    ) -> Self {
        Self {
            title: title.into(),
            description: description.into(),
            icon,
            tone,
        }
    }
}

/// The card picked: its index.
pub type PickHandler = Rc<dyn Fn(usize, &mut Window, &mut App)>;

/// The grid of `cards` under its label, its entrance kept under `id`.
pub fn prompt_card_grid(
    id: impl Into<SharedString>,
    cards: Vec<PromptCard>,
    on_pick: PickHandler,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    template_card_grid(
        id,
        Some("Copy a prompt for your agent".into()),
        cards,
        on_pick,
        window,
        cx,
    )
}

/// The grid of `cards` under `label` (drawn in upper case), or under no label.
pub fn template_card_grid(
    id: impl Into<SharedString>,
    label: Option<SharedString>,
    cards: Vec<PromptCard>,
    on_pick: PickHandler,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = *Theme::global(cx);
    let id: SharedString = id.into();
    let started =
        window.use_keyed_state(ElementId::Name(format!("{id}-enter").into()), cx, |_, _| {
            Instant::now()
        });
    let elapsed = started.read(cx).elapsed();
    let unit = rem();
    let rows: Vec<AnyElement> = cards
        .into_iter()
        .enumerate()
        .map(|(index, card)| {
            let delay = (STAGGER * index as u32).min(MOST_DELAY);
            let t =
                (elapsed.saturating_sub(delay).as_secs_f32() / ENTER.as_secs_f32()).clamp(0.0, 1.0);
            let shown = EASE_ENTER.at(t);
            let pick = on_pick.clone();
            let lift = vec![BoxShadow {
                color: fade(theme.color_shadow_subtle, 1.0).into(),
                offset: point(px(0.0), px(4.0)),
                blur_radius: px(12.0),
                spread_radius: px(0.0),
            }];
            raised_panel(
                div()
                    .id(ElementId::NamedInteger(id.clone(), index as u64))
                    .relative()
                    .bg(theme.color_surface_subtle),
                theme.radius_card,
                &theme,
            )
            .w(px(14.0 * unit))
            .min_w(px(11.5 * unit))
            .p(px(0.875 * unit))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(0.625 * unit))
            .cursor_pointer()
            .opacity(shown)
            .top(px(6.0 * (1.0 - shown)))
            .hover(move |card| card.shadow(lift.clone()))
            .on_click(move |_, window, cx| pick(index, window, cx))
            .child(IconTile::new(card.icon).size(28.0, 16.0).tone(card.tone))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(0.25 * unit))
                    .text_center()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(13.0 * 1.3))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.color_text_primary)
                            .child(card.title),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(12.0 * 1.4))
                            .text_color(theme.color_text_muted)
                            .child(card.description),
                    ),
            )
            .into_any_element()
        })
        .collect();
    let last = STAGGER * rows.len().saturating_sub(1) as u32;
    if elapsed < last.min(MOST_DELAY) + ENTER {
        window.request_animation_frame();
    }
    div()
        .w_full()
        .max_w(px(44.0 * unit))
        .mx_auto()
        .flex()
        .flex_col()
        .gap(px(0.75 * unit))
        .children(label.map(|label| {
            div()
                .mt(px(0.25 * unit))
                .text_size(px(11.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.color_text_muted)
                .child(SharedString::from(label.to_uppercase()))
        }))
        .child(
            div()
                .w_full()
                .flex()
                .flex_wrap()
                .justify_center()
                .gap(px(0.75 * unit))
                .children(rows),
        )
        .into_any_element()
}
