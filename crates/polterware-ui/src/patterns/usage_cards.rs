//! Usage summaries as cards: [`StatTile`], the totals row's figure; [`UsageCard`], one
//! consumer's spend with its parts and an optional limit meter; and [`card_grid`], the
//! responsive grid they sit in.
//!
//! The look is the design system's `Card` (`.card-panel`, [`card`]) with the metric layout of
//! the old Solid stat cards: a 12px muted label over a tabular figure. The grid behaves like
//! CSS `repeat(auto-fill, minmax(<min>, 1fr))`, which taffy's grid cannot express: it reads
//! its width from the frame before and picks as many equal columns as fit. The cards are
//! listed a row at a time, so a grid longer than a screen only builds the rows in view.

use gpui::{
    AnyElement, App, ElementId, FontWeight, IntoElement, ListAlignment, ListOffset, ListState,
    Rgba, SharedString, Window, canvas, div, list, prelude::*, px,
};

use std::cell::RefCell;
use std::rc::Rc;

use crate::Handler;
use crate::card::card;
use crate::data_table::{sans, tabular};
use crate::ellipsis::fade;
use crate::progress_bar::{bar_legend, progress_bar_sm, stacked_bar};
use crate::theme::Theme;
use crate::tooltip::tooltip;

/// A summary figure: an optional leading mark (an `IconTile`), then its label in 12px muted
/// ink over the value in 20px/500 tabular figures, on a card, with an optional caption and a
/// tooltip over the value.
#[derive(IntoElement)]
pub struct StatTile {
    id: SharedString,
    leading: Option<AnyElement>,
    label: SharedString,
    value: SharedString,
    caption: Option<SharedString>,
    tooltip: Option<SharedString>,
}

impl StatTile {
    pub fn new(
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        value: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            leading: None,
            label: label.into(),
            value: value.into(),
            caption: None,
            tooltip: None,
        }
    }

    /// A mark before the figure, centred on it: an `IconTile`.
    pub fn leading(mut self, leading: impl IntoElement) -> Self {
        self.leading = Some(leading.into_any_element());
        self
    }

    /// A 12px secondary line under the value.
    pub fn caption(mut self, caption: impl Into<SharedString>) -> Self {
        self.caption = Some(caption.into());
        self
    }

    /// What the value sums, shown on hover.
    pub fn tooltip(mut self, text: impl Into<SharedString>) -> Self {
        self.tooltip = Some(text.into());
        self
    }
}

impl RenderOnce for StatTile {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let mut value = div()
            .id(ElementId::Name(self.id.clone()))
            .min_w(px(0.0))
            .font(tabular(sans(&theme, FontWeight::MEDIUM)))
            .text_size(px(20.0))
            .line_height(px(26.0))
            .text_color(theme.color_text_primary)
            .child(fade(self.value));
        if let Some(text) = self.tooltip {
            value = value.tooltip(tooltip(text));
        }
        let text = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(theme.color_text_muted)
                    .child(fade(self.label)),
            )
            .child(value)
            .when_some(self.caption, |text, caption| {
                text.child(
                    div()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(theme.color_text_secondary)
                        .child(fade(caption)),
                )
            });
        card(cx)
            .min_w(px(0.0))
            .flex()
            .items_center()
            .gap(px(12.0))
            .px(px(14.0))
            .py(px(12.0))
            .children(self.leading)
            .child(text)
    }
}

/// The limit at the foot of a [`UsageCard`]: a label and a value over a small bar, and under
/// it a muted detail (when it resets) with an optional badge (a status chip) at its end.
/// With no fraction the bar is left out: the label says why.
pub struct UsageMeter {
    label: SharedString,
    value: Option<SharedString>,
    detail: Option<SharedString>,
    fraction: Option<f32>,
    color: Option<Rgba>,
    badge: Option<AnyElement>,
    tooltip: Option<SharedString>,
}

impl UsageMeter {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            value: None,
            detail: None,
            fraction: None,
            color: None,
            badge: None,
            tooltip: None,
        }
    }

    pub fn value(mut self, value: impl Into<SharedString>) -> Self {
        self.value = Some(value.into());
        self
    }

    /// A muted line under the bar.
    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        let detail = detail.into();
        self.detail = (!detail.is_empty()).then_some(detail);
        self
    }

    /// How full the bar is, 0 to 1, and its color.
    pub fn fraction(mut self, fraction: Option<f32>, color: Rgba) -> Self {
        self.fraction = fraction;
        self.color = Some(color);
        self
    }

    pub fn badge(mut self, badge: impl IntoElement) -> Self {
        self.badge = Some(badge.into_any_element());
        self
    }

    /// Where the figure comes from, shown on hover over the label.
    pub fn tooltip(mut self, text: impl Into<SharedString>) -> Self {
        self.tooltip = Some(text.into());
        self
    }
}

/// One consumer's usage on a card, in three rows spread from top to bottom (`justify-between`)
/// so the cards of a grid row line up: a leading mark (an avatar) with its name and an aside
/// (its provider); the headline figure over its caption; and at the foot the figure's parts
/// as a [`stacked_bar`] over its [`bar_legend`], then an optional [`UsageMeter`].
#[derive(IntoElement)]
pub struct UsageCard {
    id: SharedString,
    leading: Option<AnyElement>,
    title: SharedString,
    aside: Option<SharedString>,
    figure: SharedString,
    figure_tooltip: Option<SharedString>,
    caption: Option<SharedString>,
    caption_tooltip: Option<SharedString>,
    parts: Vec<(f32, Rgba, SharedString)>,
    meter: Option<UsageMeter>,
    on_click: Option<Handler>,
}

impl UsageCard {
    pub fn new(
        id: impl Into<SharedString>,
        title: impl Into<SharedString>,
        figure: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            leading: None,
            title: title.into(),
            aside: None,
            figure: figure.into(),
            figure_tooltip: None,
            caption: None,
            caption_tooltip: None,
            parts: Vec::new(),
            meter: None,
            on_click: None,
        }
    }

    pub fn leading(mut self, leading: impl IntoElement) -> Self {
        self.leading = Some(leading.into_any_element());
        self
    }

    /// Muted text at the header's end.
    pub fn aside(mut self, aside: impl Into<SharedString>) -> Self {
        let aside = aside.into();
        self.aside = (!aside.is_empty()).then_some(aside);
        self
    }

    pub fn figure_tooltip(mut self, text: impl Into<SharedString>) -> Self {
        self.figure_tooltip = Some(text.into());
        self
    }

    pub fn caption(mut self, caption: impl Into<SharedString>) -> Self {
        self.caption = Some(caption.into());
        self
    }

    pub fn caption_tooltip(mut self, text: impl Into<SharedString>) -> Self {
        self.caption_tooltip = Some(text.into());
        self
    }

    /// A part of the figure: its weight, its color, and its legend label.
    pub fn part(mut self, weight: f32, color: Rgba, label: impl Into<SharedString>) -> Self {
        self.parts.push((weight, color, label.into()));
        self
    }

    pub fn meter(mut self, meter: UsageMeter) -> Self {
        self.meter = Some(meter);
        self
    }

    /// Makes the card a button: a click anywhere on it calls `handler`, the pointer is a
    /// hand over it, and its face lightens under the pointer.
    pub fn on_click(mut self, handler: Handler) -> Self {
        self.on_click = Some(handler);
        self
    }
}

impl RenderOnce for UsageCard {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let small = |color: Rgba| {
            div()
                .min_w(px(0.0))
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(color)
        };
        let id = |part: &str| ElementId::Name(format!("{}-{part}", self.id).into());

        let header = div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .min_w(px(0.0))
            .children(self.leading)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .text_size(px(13.0))
                    .line_height(px(18.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.color_text_primary)
                    .child(fade(self.title)),
            )
            .when_some(self.aside, |header, aside| {
                header.child(small(theme.color_text_muted).flex_none().child(aside))
            });

        let mut figure = div()
            .id(id("figure"))
            .min_w(px(0.0))
            .font(tabular(sans(&theme, FontWeight::MEDIUM)))
            .text_size(px(24.0))
            .line_height(px(30.0))
            .text_color(theme.color_text_primary)
            .child(fade(self.figure));
        if let Some(text) = self.figure_tooltip {
            figure = figure.tooltip(tooltip(text));
        }

        let caption = self.caption.map(|caption| {
            let line = small(theme.color_text_secondary)
                .id(id("caption"))
                .font(tabular(sans(&theme, FontWeight::NORMAL)))
                .child(fade(caption));
            match self.caption_tooltip {
                Some(text) => line.tooltip(tooltip(text)),
                None => line,
            }
        });

        let parts = (!self.parts.is_empty()).then(|| {
            let weights: Vec<(f32, Rgba)> = self
                .parts
                .iter()
                .map(|(weight, color, _)| (*weight, *color))
                .collect();
            let legend: Vec<(Rgba, SharedString)> = self
                .parts
                .iter()
                .map(|(_, color, label)| (*color, label.clone()))
                .collect();
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(stacked_bar(&weights, cx))
                .child(bar_legend(&legend, cx))
        });

        let meter = self.meter.map(|meter| {
            let mut label = small(theme.color_text_secondary)
                .id(id("meter"))
                .flex_1()
                .child(fade(meter.label));
            if let Some(text) = meter.tooltip {
                label = label.tooltip(tooltip(text));
            }
            let color = meter.color.unwrap_or(theme.color_accent);
            let bar = meter
                .fraction
                .map(|fraction| progress_bar_sm(fraction, color, cx));
            let foot = (meter.detail.is_some() || meter.badge.is_some()).then(|| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .min_h(px(22.0))
                    .child(
                        small(theme.color_text_muted)
                            .flex_1()
                            .children(meter.detail.map(fade)),
                    )
                    .children(meter.badge)
            });
            div()
                .mt(px(12.0))
                .pt(px(10.0))
                .border_t_1()
                .border_color(theme.color_border_subtle)
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(label)
                        .when_some(meter.value, |row, value| {
                            row.child(
                                small(theme.color_text_primary)
                                    .flex_none()
                                    .font(tabular(sans(&theme, FontWeight::MEDIUM)))
                                    .child(value),
                            )
                        }),
                )
                .children(bar)
                .children(foot)
        });

        let middle = div()
            .flex()
            .flex_col()
            .min_w(px(0.0))
            .child(figure)
            .children(caption);
        let foot = (parts.is_some() || meter.is_some()).then(|| {
            div()
                .flex()
                .flex_col()
                .min_w(px(0.0))
                .children(parts)
                .children(meter)
        });
        card(cx)
            .id(ElementId::Name(self.id.clone()))
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .justify_between()
            .gap(px(12.0))
            .px(px(14.0))
            .py(px(12.0))
            .when_some(self.on_click, |card, handler| {
                card.cursor_pointer()
                    .hover(|style| style.bg(theme.color_surface_hover))
                    .on_click(move |_, window, cx| handler(window, cx))
            })
            .child(header)
            .child(middle)
            .children(foot)
    }
}

/// What [`card_grid`] keeps between frames: the width it last laid out at, the shape its whole
/// height was measured at, and the rows' [`ListState`].
struct GridState {
    width: f32,
    viewport_width: f32,
    /// The grid's whole height at `measured`'s width, or 0 until a frame has drawn it whole.
    content_h: f32,
    measured: Option<MeasuredGrid>,
    list: ListState,
    /// The number of rows the list holds; `None` until it was first given rows.
    listed: Option<usize>,
}

/// The shape [`GridState::content_h`] belongs to.
#[derive(Clone, Copy)]
struct MeasuredGrid {
    width: f32,
    columns: u16,
    rows: usize,
}

impl GridState {
    fn new() -> Self {
        Self {
            width: 0.0,
            viewport_width: 0.0,
            content_h: 0.0,
            measured: None,
            list: ListState::new(0, ListAlignment::Top, px(GRID_OVERDRAW)),
            listed: None,
        }
    }
}

/// How far past the viewport the grid's list measures rows, so a fast scroll finds the next
/// row ready.
const GRID_OVERDRAW: f32 = 240.0;

/// The cards in as many equal columns of at least `min_width` as the available width allows,
/// `gap` apart both ways; tiles fill their column evenly.
/// Layout determines the column count from the available width in the current frame without
/// lagging behind window resizing, avoiding jumps and stutter during window edge drag.
///
/// The cards are rows of `columns` inside a `gpui::list` (gpui-kit's `VirtualList`), so a grid
/// longer than a screen builds only the rows in view. The list takes the grid's own height
/// while the cards fit on screen, so a short grid draws exactly as a plain CSS grid did:
/// the same widths, gaps, and heights. Past a screenful it caps at the window's height and
/// scrolls its own rows — a `gpui::list` is its own viewport, which is the closest gpui draws
/// to virtualizing a block inside a page that scrolls.
pub fn card_grid(
    id: impl Into<ElementId>,
    min_width: f32,
    gap: f32,
    guess: u16,
    cards: Vec<AnyElement>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let id = id.into();
    let state = window.use_keyed_state(id.clone(), cx, |_, _| GridState::new());
    let current_viewport = f32::from(window.viewport_size().width);
    let (stored_width, stored_viewport) = {
        let grid = state.read(cx);
        (grid.width, grid.viewport_width)
    };
    let width = if stored_width > 0.0 {
        (stored_width + (current_viewport - stored_viewport)).max(min_width)
    } else {
        let sidebar = Theme::global(cx).sidebar_width;
        if sidebar > 0.0 && current_viewport > sidebar + 26.0 {
            current_viewport - sidebar - 26.0
        } else if current_viewport > 32.0 {
            current_viewport - 32.0
        } else {
            min_width * f32::from(guess.max(1)) + gap * f32::from(guess.saturating_sub(1))
        }
    };
    let cols = grid_columns(width, min_width, gap);
    let rows: Vec<Vec<AnyElement>> = {
        let mut rows = Vec::with_capacity(cards.len().div_ceil(usize::from(cols)));
        let mut cards = cards.into_iter();
        loop {
            let row: Vec<AnyElement> = cards.by_ref().take(usize::from(cols)).collect();
            if row.is_empty() {
                break;
            }
            rows.push(row);
        }
        rows
    };
    let row_count = rows.len();
    let probe = canvas(
        {
            let state = state.clone();
            move |bounds, window, cx| {
                let seen = f32::from(bounds.size.width);
                let prev_cols = grid_columns(state.read(cx).width, min_width, gap);
                let new_cols = grid_columns(seen, min_width, gap);
                state.update(cx, |grid, _| {
                    grid.width = seen;
                    grid.viewport_width = current_viewport;
                });
                if prev_cols != new_cols {
                    crate::frame::draw_again(window);
                }
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .w_full()
    .h_0();

    let measured = {
        let grid = state.read(cx);
        grid.content_h > 0.0
            && grid.measured.is_some_and(|shape| {
                (shape.width - width).abs() <= 0.5
                    && shape.columns == cols
                    && shape.rows == row_count
            })
    };
    if !measured {
        // One frame draws the grid whole so its height is known (and the cards look exactly
        // as they always have); the next lists its rows.
        let grid = div()
            .w_full()
            .grid()
            .grid_cols(cols)
            .gap(px(gap))
            .children(rows.into_iter().flatten());
        let tracker = canvas(
            {
                let state = state.clone();
                move |bounds, window, cx| {
                    let (height, seen) =
                        (f32::from(bounds.size.height), f32::from(bounds.size.width));
                    let shape = MeasuredGrid {
                        width: seen,
                        columns: cols,
                        rows: row_count,
                    };
                    let changed = {
                        let grid = state.read(cx);
                        (grid.content_h - height).abs() > 0.5
                            || grid.measured.is_none_or(|seen| {
                                seen.width != shape.width
                                    || seen.columns != shape.columns
                                    || seen.rows != shape.rows
                            })
                    };
                    if changed {
                        state.update(cx, |grid, cx| {
                            grid.content_h = height;
                            grid.width = seen;
                            grid.measured = Some(shape);
                            cx.notify();
                        });
                        crate::frame::draw_again(window);
                    }
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .top_0()
        .left_0()
        .w_full()
        .h_full();
        return div()
            .relative()
            .w_full()
            .child(probe)
            .child(tracker)
            .child(grid)
            .into_any_element();
    }

    // The list's rows: one grid of `cols` cards each, `gap` apart, the last row without the
    // trailing gap so the grid's height matches the one measured above.
    let listed = state.read(cx).listed;
    let list_state = state.read(cx).list.clone();
    if listed != Some(row_count) {
        let anchor = list_state.logical_scroll_top();
        list_state.splice(0..list_state.item_count(), row_count);
        list_state.scroll_to(ListOffset {
            item_ix: anchor.item_ix.min(row_count.saturating_sub(1)),
            offset_in_item: anchor.offset_in_item,
        });
        state.update(cx, |grid, _| grid.listed = Some(row_count));
    }
    let content_h = state.read(cx).content_h;
    let height = content_h.min(f32::from(window.viewport_size().height));
    let rows: Vec<Rc<RefCell<Option<Vec<AnyElement>>>>> = rows
        .into_iter()
        .map(|row| Rc::new(RefCell::new(Some(row))))
        .collect();
    let count = rows.len();
    let list = list(list_state, move |index, _window, _cx| {
        let Some(slot) = rows.get(index) else {
            return div().into_any_element();
        };
        let Some(cards) = slot.borrow_mut().take() else {
            return div().into_any_element();
        };
        div()
            .w_full()
            .grid()
            .grid_cols(cols)
            .gap(px(gap))
            .when(index + 1 < count, |row| row.pb(px(gap)))
            .children(cards)
            .into_any_element()
    })
    .w_full()
    .h(px(height));
    div()
        .relative()
        .w_full()
        .child(probe)
        .child(list)
        .into_any_element()
}

/// How many columns of at least `min_width`, `gap` apart, fit in `width`: one at least.
pub fn grid_columns(width: f32, min_width: f32, gap: f32) -> u16 {
    if !(width.is_finite() && min_width > 0.0) {
        return 1;
    }
    let fit = ((width + gap) / (min_width + gap)).floor();
    fit.clamp(1.0, f32::from(u16::MAX)) as u16
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gpui::{
        AnyElement, Bounds, IntoElement, Pixels, TestAppContext, canvas, div, prelude::*, px, size,
    };

    use super::{card_grid, grid_columns};
    use crate::test_support::{draw, open};

    type Painted = Rc<RefCell<Vec<Bounds<Pixels>>>>;

    fn card(painted: &Painted, height: f32) -> AnyElement {
        let painted = painted.clone();
        div()
            .w_full()
            .h(px(height))
            .child(
                canvas(
                    move |bounds, _, _| painted.borrow_mut().push(bounds),
                    |_, _, _, _| {},
                )
                .size_full(),
            )
            .into_any_element()
    }

    /// Clears the recording at the start of each frame, so a draw that runs several frames
    /// leaves only the last frame's cards.
    fn frame_clear(painted: &Painted) -> AnyElement {
        let painted = painted.clone();
        canvas(move |_, _, _| painted.borrow_mut().clear(), |_, _, _, _| {})
            .absolute()
            .top_0()
            .left_0()
            .size_0()
            .into_any_element()
    }

    /// A 720px column in a window whose content width guesses to 720 (the theme's sidebar and
    /// the 26px inset taken out of the viewport), so the first frame already holds the right
    /// column count.
    fn grid_window(
        cx: &mut TestAppContext,
        painted: Painted,
        count: usize,
    ) -> gpui::VisualTestContext {
        let window_width = 720.0 + crate::theme::Theme::dark().sidebar_width + 26.0;
        open(cx, size(window_width, 700.0), move |window, cx| {
            let cards = (0..count).map(|_| card(&painted, 100.0)).collect();
            div()
                .w(px(720.0))
                .child(frame_clear(&painted))
                .child(card_grid("gallery-grid", 220.0, 10.0, 3, cards, window, cx))
                .into_any_element()
        })
    }

    #[test]
    fn the_grid_fits_as_many_columns_as_the_width_holds() {
        assert_eq!(grid_columns(0.0, 260.0, 12.0), 1);
        assert_eq!(grid_columns(259.0, 260.0, 12.0), 1);
        assert_eq!(grid_columns(532.0, 260.0, 12.0), 2);
        assert_eq!(grid_columns(531.0, 260.0, 12.0), 1);
        assert_eq!(grid_columns(1100.0, 260.0, 12.0), 4);
        assert_eq!(grid_columns(f32::NAN, 260.0, 12.0), 1);
    }

    #[gpui::test]
    fn a_short_grid_lays_its_cards_out_exactly_as_the_plain_grid_did(cx: &mut TestAppContext) {
        let painted: Painted = Rc::default();
        let mut visual = grid_window(cx, painted.clone(), 3);
        let whole = painted.borrow().clone();
        assert_eq!(whole.len(), 3, "three cards in one row");
        // The plain grid's geometry: 720 wide, 10 apart, three equal columns of 100px cards.
        let width = (720.0 - 2.0 * 10.0) / 3.0;
        assert!((f32::from(whole[0].size.width) - width).abs() < 0.5);
        assert!((f32::from(whole[0].size.height) - 100.0).abs() < 0.5);
        for pair in whole.windows(2) {
            assert!(
                (f32::from(pair[1].origin.x) - f32::from(pair[0].origin.x) - width - 10.0).abs()
                    < 0.5
            );
            assert_eq!(pair[0].origin.y, pair[1].origin.y);
        }
        // A second frame — the rows now in a list — puts every card back in the same place.
        draw(&mut visual);
        let listed = painted.borrow().clone();
        assert_eq!(listed.len(), 3);
        for (whole, listed) in whole.iter().zip(&listed) {
            assert!((f32::from(whole.origin.x) - f32::from(listed.origin.x)).abs() < 0.5);
            assert!((f32::from(whole.origin.y) - f32::from(listed.origin.y)).abs() < 0.5);
            assert!((f32::from(whole.size.width) - f32::from(listed.size.width)).abs() < 0.5);
            assert!((f32::from(whole.size.height) - f32::from(listed.size.height)).abs() < 0.5);
        }
    }

    #[gpui::test]
    fn a_long_grid_builds_only_the_rows_in_view(cx: &mut TestAppContext) {
        let painted: Painted = Rc::default();
        let mut visual = grid_window(cx, painted.clone(), 200);
        let settled = painted.borrow().len();
        assert!(settled > 0, "the cards in view are drawn");
        assert!(settled < 60, "painted {settled} of 200 cards");
        draw(&mut visual);
        let listed = painted.borrow().len();
        assert!(listed > 0, "the cards in view are drawn");
        assert!(listed < 60, "painted {listed} of 200 cards");
    }
}
