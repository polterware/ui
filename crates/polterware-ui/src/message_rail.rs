//! `MessageRail`: a thin column of short horizontal bars, one per message of a conversation,
//! that indexes it. Hovering a bar grows and brightens it on the moderate spring and opens a
//! card beside it (to its left) with the message's first lines; clicking one tells the
//! owner which message to reveal. The bar of the message in view can be marked active.
//!
//! It looks like the message index along the edge of Codex's conversation view, on the
//! library's tokens (`color_text_*`, the tooltip card of [`crate::tooltip`]); it behaves
//! like a one-axis anchor list (gpui-kit's `Anchor`): items with a stable id, one active,
//! a callback with the picked index. An empty list has nothing to index, so the rail draws
//! nothing; one or more items show.
//!
//! The rail is a fixed-width column ([`RAIL_WIDTH`]) as tall as its bars; the caller places
//! it (absolute, vertically centered, clear of a scrollbar). Its hover springs live in the
//! window's keyed state under the rail's id, and it asks for frames only while one moves.

use std::rc::Rc;

use gpui::{
    App, ElementId, IntoElement, RenderOnce, SharedString, Window, deferred, div, prelude::*, px,
};

use crate::ValueHandler;
use crate::motion::{Animator, SpringTier, SpringValue};
use crate::theme::{Theme, mix};
use crate::tooltip::tooltip_card;

/// The column's width, which is also each bar's hit area.
pub const RAIL_WIDTH: f32 = 24.0;
/// The hit area's height: bars sit this far apart, centre to centre.
const ITEM_HEIGHT: f32 = 12.0;
/// The bar's thickness.
const BAR_HEIGHT: f32 = 2.0;
/// The bar's length at rest and while the pointer is over it.
const BAR_REST: f32 = 10.0;
const BAR_HOVER: f32 = 16.0;
/// The space between the bar and the column's right edge.
const BAR_INSET: f32 = 4.0;
/// The card's width, its distance from the column, and how far it slides in from.
const CARD_WIDTH: f32 = 264.0;
const CARD_GAP: f32 = 6.0;
const CARD_SLIDE: f32 = 6.0;
/// The most lines and characters per line a card keeps of a message.
const SUMMARY_LINES: usize = 3;
const SUMMARY_LINE_CHARS: usize = 160;

/// One message the rail indexes: a stable id (a bar keeps its hover across changes of the
/// list) and the text its card summarizes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageRailItem {
    pub id: SharedString,
    pub label: SharedString,
}

impl MessageRailItem {
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
    }
}

/// Whether a rail of `count` messages shows: an empty list has nothing to index.
pub fn message_rail_visible(count: usize) -> bool {
    count >= 1
}

/// The bar's length at hover progress `t` (0 at rest, 1 hovered).
pub fn message_rail_bar_width(t: f32) -> f32 {
    BAR_REST + (BAR_HOVER - BAR_REST) * t.clamp(0.0, 1.0)
}

/// What a card shows of a message: its first non-blank lines, trimmed, each cut at a
/// readable length, and an ellipsis where something was left out.
pub fn message_rail_summary(text: &str) -> String {
    let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
    let mut kept: Vec<String> = Vec::new();
    for line in lines.by_ref().take(SUMMARY_LINES) {
        let mut chars = line.chars();
        let mut cut: String = chars.by_ref().take(SUMMARY_LINE_CHARS).collect();
        if chars.next().is_some() {
            cut.push('…');
        }
        kept.push(cut);
    }
    if lines.next().is_some()
        && let Some(last) = kept.last_mut()
        && !last.ends_with('…')
    {
        last.push('…');
    }
    kept.join("\n")
}

/// The hover springs, one per message, and the message under the pointer.
struct RailState {
    hovered: Option<SharedString>,
    springs: Vec<(SharedString, SpringValue)>,
    animator: Animator,
}

impl RailState {
    /// Follows the list of ids, aims each spring at hover, and steps them one frame; each
    /// message's progress comes back, in order.
    fn frame(&mut self, ids: &[SharedString], window: &mut Window) -> Vec<f32> {
        let mut springs = Vec::with_capacity(ids.len());
        for id in ids {
            let spring = match self.springs.iter().position(|(held, _)| held == id) {
                Some(at) => self.springs.swap_remove(at).1,
                None => SpringValue::new(0.0, SpringTier::Moderate),
            };
            springs.push((id.clone(), spring));
        }
        self.springs = springs;
        for (id, spring) in &mut self.springs {
            let target = if self.hovered.as_ref() == Some(id) {
                1.0
            } else {
                0.0
            };
            if (spring.target() - target).abs() > f32::EPSILON {
                spring.set_target(target);
            }
        }
        let moving = self.springs.iter().any(|(_, spring)| !spring.settled());
        let dt = self.animator.dt(window, moving);
        if dt > 0.0 {
            for (_, spring) in &mut self.springs {
                spring.step(dt);
            }
        }
        self.springs
            .iter()
            .map(|(_, spring)| spring.value())
            .collect()
    }
}

#[derive(IntoElement)]
pub struct MessageRail {
    id: ElementId,
    items: Vec<MessageRailItem>,
    active: Option<usize>,
    on_select: Option<ValueHandler<usize>>,
}

impl MessageRail {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            items: Vec::new(),
            active: None,
            on_select: None,
        }
    }

    /// The messages, oldest first.
    pub fn items(mut self, items: Vec<MessageRailItem>) -> Self {
        self.items = items;
        self
    }

    /// The message in view, whose bar takes a brighter tone at rest.
    pub fn active(mut self, active: Option<usize>) -> Self {
        self.active = active;
        self
    }

    /// Called with the index of the bar that was clicked.
    pub fn on_select(mut self, handler: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for MessageRail {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let column = div().w(px(RAIL_WIDTH)).flex_none();
        if !message_rail_visible(self.items.len()) {
            return column;
        }
        let theme = *Theme::global(cx);
        let state = window.use_keyed_state((self.id.clone(), "state"), cx, |_, _| RailState {
            hovered: None,
            springs: Vec::new(),
            animator: Animator::new(),
        });
        let ids: Vec<SharedString> = self.items.iter().map(|item| item.id.clone()).collect();
        let progress = state.update(cx, |state, _| state.frame(&ids, window));
        let mut column = column.flex().flex_col();
        for (index, item) in self.items.into_iter().enumerate() {
            let t = progress.get(index).copied().unwrap_or(0.0);
            let rest = if self.active == Some(index) {
                theme.color_text_secondary
            } else {
                theme.color_text_disabled
            };
            let ink = mix(rest, theme.color_text_primary, t);
            let id = ElementId::NamedInteger(format!("{}-item", self.id).into(), index as u64);
            let bar = div()
                .debug_selector(move || format!("message-rail-{index}-bar"))
                .w(px(message_rail_bar_width(t)))
                .h(px(BAR_HEIGHT))
                .rounded_full()
                .bg(ink);
            let mut cell = div()
                .id(id)
                .debug_selector(move || format!("message-rail-{index}"))
                .relative()
                .w_full()
                .h(px(ITEM_HEIGHT))
                .pr(px(BAR_INSET))
                .flex()
                .items_center()
                .justify_end()
                .cursor_pointer()
                .on_hover({
                    let state = state.clone();
                    let item_id = item.id.clone();
                    move |hovered, _, cx| {
                        state.update(cx, |state, cx| {
                            if *hovered {
                                state.hovered = Some(item_id.clone());
                            } else if state.hovered.as_ref() == Some(&item_id) {
                                state.hovered = None;
                            }
                            cx.notify();
                        });
                    }
                })
                .child(bar);
            if let Some(handler) = self.on_select.clone() {
                cell = cell.on_click(move |_, window, cx| handler(index, window, cx));
            }
            if t > 0.001 {
                cell = cell.child(deferred(card(&item, t, cx)).with_priority(1));
            }
            column = column.child(cell);
        }
        column
    }
}

/// The card beside the bar, vertically centered on it: it fades and slides in with the
/// bar's hover progress `t`.
fn card(item: &MessageRailItem, t: f32, cx: &App) -> impl IntoElement {
    let body = div()
        .w_full()
        .line_clamp(SUMMARY_LINES)
        .text_ellipsis()
        .child(SharedString::from(message_rail_summary(&item.label)));
    div()
        .absolute()
        .top_0()
        .h(px(ITEM_HEIGHT))
        .w(px(CARD_WIDTH))
        .right(px(RAIL_WIDTH + CARD_GAP + (1.0 - t) * CARD_SLIDE))
        .flex()
        .items_center()
        .opacity(t)
        .child(tooltip_card(CARD_WIDTH, body, cx))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gpui::{Modifiers, TestAppContext, VisualTestContext, div, point, prelude::*, size};

    use super::*;
    use crate::test_support::{draw, open, settle};

    fn items(count: usize) -> Vec<MessageRailItem> {
        (0..count)
            .map(|index| MessageRailItem::new(format!("m{index}"), format!("Message {index}")))
            .collect()
    }

    fn open_rail(
        cx: &mut TestAppContext,
        count: usize,
        picked: Rc<RefCell<Vec<usize>>>,
    ) -> VisualTestContext {
        open(cx, size(300.0, 200.0), move |_, _| {
            let picked = picked.clone();
            div()
                .size_full()
                .flex()
                .justify_end()
                .child(
                    MessageRail::new("rail")
                        .items(items(count))
                        .on_select(move |index, _, _| picked.borrow_mut().push(index)),
                )
                .into_any_element()
        })
    }

    fn center(cx: &mut VisualTestContext, selector: &'static str) -> gpui::Point<gpui::Pixels> {
        cx.debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} is drawn"))
            .center()
    }

    #[test]
    fn an_empty_rail_has_nothing_to_index() {
        assert!(!message_rail_visible(0));
        assert!(message_rail_visible(1));
        assert!(message_rail_visible(2));
    }

    #[test]
    fn the_bar_grows_with_hover_progress() {
        assert_eq!(message_rail_bar_width(0.0), BAR_REST);
        assert_eq!(message_rail_bar_width(1.0), BAR_HOVER);
        assert_eq!(message_rail_bar_width(7.0), BAR_HOVER);
        assert!(message_rail_bar_width(0.5) > BAR_REST);
    }

    #[test]
    fn the_summary_keeps_the_first_lines_and_marks_what_it_drops() {
        assert_eq!(
            message_rail_summary("  one \n\n two\nthree"),
            "one\ntwo\nthree"
        );
        assert_eq!(message_rail_summary("a\nb\nc\nd"), "a\nb\nc…");
        let long = "x".repeat(SUMMARY_LINE_CHARS + 5);
        let summary = message_rail_summary(&long);
        assert_eq!(summary.chars().count(), SUMMARY_LINE_CHARS + 1);
        assert!(summary.ends_with('…'));
        assert_eq!(message_rail_summary("   \n "), "");
    }

    #[gpui::test]
    fn a_click_selects_the_bar_it_hits(cx: &mut TestAppContext) {
        let picked = Rc::new(RefCell::new(Vec::new()));
        let mut cx = open_rail(cx, 3, picked.clone());
        let second = center(&mut cx, "message-rail-1");
        cx.simulate_click(second, Modifiers::none());
        let third = center(&mut cx, "message-rail-2");
        cx.simulate_click(third, Modifiers::none());
        assert_eq!(*picked.borrow(), vec![1, 2]);
    }

    #[gpui::test]
    fn hovering_a_bar_grows_it_and_the_others_stay(cx: &mut TestAppContext) {
        let mut cx = open_rail(cx, 3, Rc::new(RefCell::new(Vec::new())));
        const BARS: [&str; 3] = [
            "message-rail-0-bar",
            "message-rail-1-bar",
            "message-rail-2-bar",
        ];
        let width = |cx: &mut VisualTestContext, index: usize| {
            f32::from(cx.debug_bounds(BARS[index]).unwrap().size.width)
        };
        assert_eq!(width(&mut cx, 0), BAR_REST);
        let first = center(&mut cx, "message-rail-0");
        cx.simulate_mouse_move(first, None, Modifiers::none());
        settle(&mut cx);
        assert!((width(&mut cx, 0) - BAR_HOVER).abs() < 0.5);
        assert_eq!(width(&mut cx, 1), BAR_REST);
        cx.simulate_mouse_move(point(px(2.0), px(2.0)), None, Modifiers::none());
        settle(&mut cx);
        assert_eq!(width(&mut cx, 0), BAR_REST);
    }

    #[gpui::test]
    fn zero_messages_draw_no_rail(cx: &mut TestAppContext) {
        let mut cx = open_rail(cx, 0, Rc::new(RefCell::new(Vec::new())));
        draw(&mut cx);
        assert!(cx.debug_bounds("message-rail-0").is_none());
    }

    #[gpui::test]
    fn one_message_draws_a_rail_bar(cx: &mut TestAppContext) {
        let picked = Rc::new(RefCell::new(Vec::new()));
        let mut cx = open_rail(cx, 1, picked.clone());
        draw(&mut cx);
        let bar = center(&mut cx, "message-rail-0");
        cx.simulate_click(bar, Modifiers::none());
        assert_eq!(*picked.borrow(), vec![0]);
    }
}
