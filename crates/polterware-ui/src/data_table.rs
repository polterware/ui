//! `DataTable` (`table.tsx`, `table.css`) in its list density, virtualized.
//!
//! The scroll keeps 12px gutters (none in the default density, whose rows size to their cells
//! alone). Rows sit 4px apart (`border-spacing: 0 4px`): a 36px header
//! (`padding: 4px 12px` around the 28px `.pw-data-table-column-label`, 11px/500 muted), then
//! 40px rows on a 44px pitch whose cells pad 5px 12px in 12.5px secondary text. Every
//! resizable column but the last shows its divider: a 1px `--color-border-strong` line from
//! 10px to 26px, centered on the column's right edge.
//!
//! Widths freeze once rows have a size, the way `distributeDataTableColumnWidths` does: each
//! column's natural width is its widest content among the rows the virtualizer renders (the ones in
//! view and 8 more), the header's included and at least its explicit width, rounded as
//! `offsetWidth` rounds and capped at 320; the spare room shares out in proportion among the
//! resizable columns with no explicit width (the flex column, the last resizable one, weighs
//! double), each held to its minimum and maximum; and the flex column takes what the others leave,
//! at least 96, on every frame. The table may run wider than the scroll and then scrolls sideways.
//! Both ways overflowing, the classic scrollbars take 12px gutters at the right and the bottom. The
//! header is the list's first item, so it scrolls away with the rows. Dragging a divider is not
//! ported.
//!
//! A sortable header is `.pw-data-table-sort-trigger`: a 28px pill padded 8px, its label and,
//! sorted, a 13px arrow 5px after it; under the pointer it fills with `--color-surface-hover`
//! in secondary text, sorted with `--color-surface-selected` in primary text, both at once.
//!
//! The hover is `FluidHoverHighlight`: one `--color-surface-hover` highlight behind the rows.
//! Entering the table it snaps onto the row and fades in on `spring.fast`'s duration (easeOut);
//! moving to another row it slides there on `spring.fast`; leaving it fades out over the fast
//! exit (0.06s, easeIn). The hovered row's text turns primary over `--duration-fast`.
//!
//! [`fade_text`] is `.pw-inline-fade`: a cell's text that overflows fades out over its last
//! 24px. GPUI cannot mask text, so each glyph in that stretch takes its own alpha.

use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    AnyElement, App, Context, ElementId, Entity, Font, FontFeatures, FontWeight, IntoElement, Rgba,
    ScrollHandle, SharedString, TextRun, UniformListScrollHandle, Window, canvas, div, font,
    prelude::*, px, uniform_list,
};

use super::ellipsis::fade as fade_line;
use super::icons::{Icon, icon_colored};
use super::pointer::{track, use_pointer};
use super::scroll::{GUTTER as SCROLLBAR, ScrollAxis, ScrollGeometry, scrollbar};
use crate::ValueHandler;
use crate::motion::{Curve, SpringTier, Tween};
pub use crate::sort::{SortDirection, next_sort};
use crate::theme::{Ease, Theme, fade, mix};

const GUTTER: f32 = 12.0;
const SPACING: f32 = 4.0;
const HEADER_HEIGHT: f32 = 36.0;
const ROW_HEIGHT: f32 = 40.0;
/// The header block above the first row: its spacing, the 36px header, and the row gap.
const HEADER_PITCH: f32 = SPACING + HEADER_HEIGHT + SPACING;
const CELL_PAD_X: f32 = 12.0;
const LABEL_PAD_X: f32 = 8.0;
const MIN_WIDTH: f32 = 48.0;
const MAX_WIDTH: f32 = 360.0;
const FLEX_MIN_WIDTH: f32 = 96.0;
const MAX_NATURAL_WIDTH: f32 = 320.0;
/// The rows the virtualizer renders past the ones in view.
const OVERSCAN: usize = 8;
const SORT_GAP: f32 = 5.0;
const SORT_ARROW: f32 = 13.0;
/// Motion's `easeOut` and `easeIn`.
const EASE_OUT: Ease = Ease(0.0, 0.0, 0.58, 1.0);
const EASE_IN: Ease = Ease(0.42, 0.0, 1.0, 1.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Start,
    End,
}

/// One `DataTable.ColumnHeader`.
#[derive(Clone, Debug)]
pub struct Column {
    pub title: SharedString,
    pub align: Align,
    /// The widest cell's content, without the cell's padding.
    pub content: f32,
    /// An explicit `width`: the column keeps its natural width when spare room shares out.
    pub fixed: bool,
    /// The explicit width in pixels, which the natural width does not go under.
    pub width: Option<f32>,
    pub min_width: f32,
    pub max_width: f32,
    pub resizable: bool,
    pub sortable: bool,
}

impl Column {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            align: Align::Start,
            content: 0.0,
            fixed: false,
            width: None,
            min_width: MIN_WIDTH,
            max_width: MAX_WIDTH,
            resizable: true,
            sortable: false,
        }
    }

    /// `width="96px"`.
    pub fn width(mut self, width: f32) -> Self {
        self.fixed = true;
        self.width = Some(width);
        self
    }

    /// `minWidth`.
    pub fn min_width(mut self, width: f32) -> Self {
        self.min_width = width;
        self
    }

    /// A header that sorts the rows: `sortBy`.
    pub fn sortable(mut self) -> Self {
        self.sortable = true;
        self
    }

    /// `align="right"`.
    pub fn end(mut self) -> Self {
        self.align = Align::End;
        self
    }

    pub fn content(mut self, width: f32) -> Self {
        self.content = width;
        self
    }

    pub fn fixed(mut self) -> Self {
        self.fixed = true;
        self
    }
}

/// What a row's builder gets: the row, each cell's content width, and the row's text color.
pub struct RowCx {
    pub index: usize,
    pub widths: Rc<[f32]>,
    /// Secondary at rest, primary while the row is hovered.
    pub ink: Rgba,
}

type RowFn = Rc<dyn Fn(&RowCx, &mut Window, &mut App) -> Vec<AnyElement>>;
type MeasureFn = Rc<dyn Fn(Range<usize>, &mut Window, &mut App) -> Vec<f32>>;
type ActivateFn = ValueHandler<usize>;
type SortFn = ValueHandler<usize>;

#[derive(IntoElement)]
pub struct DataTable {
    id: ElementId,
    columns: Vec<Column>,
    rows: usize,
    row_height: f32,
    gutter: f32,
    row: RowFn,
    measure: Option<MeasureFn>,
    on_activate: Option<ActivateFn>,
    sort: Option<(usize, SortDirection)>,
    on_sort: Option<SortFn>,
    empty: SharedString,
}

impl DataTable {
    pub fn new(
        id: impl Into<ElementId>,
        columns: Vec<Column>,
        rows: usize,
        row: impl Fn(&RowCx, &mut Window, &mut App) -> Vec<AnyElement> + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            columns,
            rows,
            row_height: ROW_HEIGHT,
            gutter: GUTTER,
            row: Rc::new(row),
            measure: None,
            on_activate: None,
            sort: None,
            on_sort: None,
            empty: "Nothing here".into(),
        }
    }

    /// Each column's widest content over the rows in `range`, measured when the widths
    /// freeze (for text, which needs the window). Overrides [`Column::content`].
    pub fn measure(
        mut self,
        measure: impl Fn(Range<usize>, &mut Window, &mut App) -> Vec<f32> + 'static,
    ) -> Self {
        self.measure = Some(Rc::new(measure));
        self
    }

    /// The column the rows are sorted by, which its header shows, and the click on a sortable
    /// header. The caller sorts the rows.
    pub fn sort(
        mut self,
        sort: Option<(usize, SortDirection)>,
        on_sort: impl Fn(usize, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.sort = sort;
        self.on_sort = Some(Rc::new(on_sort));
        self
    }

    /// `onActivate`: a click on the row.
    pub fn on_activate(mut self, handler: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_activate = Some(Rc::new(handler));
        self
    }

    /// The rows' height. A list row (`density="list"`) is at least 40px and grows to its
    /// cells, as a 32px avatar in the cell's 5px padding makes 42; the default density sizes
    /// a row to its cells alone, as one line of 12.5px text makes 28.75.
    pub fn row_height(mut self, height: f32) -> Self {
        self.row_height = height;
        self
    }

    /// The default density: no gutters, so the table fills its scroll edge to edge. Its rows
    /// size to their cells, so give it a [`Self::row_height`] too.
    pub fn default_density(mut self) -> Self {
        self.gutter = 0.0;
        self
    }

    pub fn empty(mut self, text: impl Into<SharedString>) -> Self {
        self.empty = text.into();
        self
    }
}

/// The table's measured size, its frozen widths, its scrolls, and the travelling hover.
struct TableState {
    available: f32,
    height: f32,
    /// The widths frozen once rows had a size; the flex column's is worked out every frame.
    frozen: Option<Vec<f32>>,
    scroll: UniformListScrollHandle,
    x_scroll: ScrollHandle,
    hovered: Option<usize>,
    /// The row pitch of the last frame, which places the travelling hover.
    pitch: f32,
    y: Tween,
    opacity: Tween,
    ink: HashMap<usize, Tween>,
}

impl TableState {
    fn new() -> Self {
        Self {
            available: 0.0,
            height: 0.0,
            frozen: None,
            scroll: UniformListScrollHandle::new(),
            x_scroll: ScrollHandle::new(),
            hovered: None,
            pitch: ROW_HEIGHT + SPACING,
            y: Tween::new(0.0),
            opacity: Tween::new(0.0),
            ink: HashMap::new(),
        }
    }

    fn animating(&self) -> bool {
        self.y.is_animating()
            || self.opacity.is_animating()
            || self.ink.values().any(Tween::is_animating)
    }

    fn ink(&self, index: usize) -> f32 {
        self.ink.get(&index).map_or(0.0, Tween::value)
    }

    fn hover(&mut self, index: usize, hovered: bool, theme: &Theme) {
        let fast = SpringTier::Fast;
        let ink = self.ink.entry(index).or_insert_with(|| Tween::new(0.0));
        ink.animate_to(
            if hovered { 1.0 } else { 0.0 },
            theme.duration_fast,
            Curve::Ease(theme.ease_standard),
        );
        let y = index as f32 * self.pitch;
        if hovered {
            if self.opacity.value() < 0.01 {
                self.y.snap(y);
            } else {
                self.y.animate_to(y, fast.duration(), fast.curve());
            }
            self.opacity
                .animate_to(1.0, fast.duration(), Curve::Ease(EASE_OUT));
            self.hovered = Some(index);
        } else if self.hovered == Some(index) {
            self.hovered = None;
            self.opacity
                .animate_to(0.0, fast.exit_duration(), Curve::Ease(EASE_IN));
        }
        self.ink
            .retain(|key, tween| *key == index || tween.is_animating() || tween.value() > 0.0);
    }
}

/// `distributeDataTableColumnWidths`, then the flex column's share of what is left.
/// `available` is the scroll's width, gutters included.
pub fn column_widths(natural: &[f32], columns: &[Column], available: f32, gutter: f32) -> Vec<f32> {
    let flex = columns.iter().rposition(|column| column.resizable);
    let weight = |index: usize| -> f32 {
        if Some(index) == flex {
            2.0
        } else if columns[index].resizable && !columns[index].fixed {
            1.0
        } else {
            0.0
        }
    };
    let total: f32 = natural.iter().sum();
    let weighted: f32 = natural
        .iter()
        .enumerate()
        .map(|(index, width)| width * weight(index))
        .sum();
    let spare = (available - total).max(0.0);
    let mut widths: Vec<f32> = natural
        .iter()
        .enumerate()
        .map(|(index, &width)| {
            let grown = if weighted > 0.0 {
                width + spare * width * weight(index) / weighted
            } else {
                width
            };
            let column = &columns[index];
            if column.resizable {
                grown.clamp(column.min_width, column.max_width).round()
            } else {
                grown.round()
            }
        })
        .collect();
    fill_flex(&mut widths, columns, available, gutter);
    widths
}

/// The flex column's width: what the others leave of the scroll inside its gutters, at
/// least 96.
fn fill_flex(widths: &mut [f32], columns: &[Column], available: f32, gutter: f32) {
    let flex = columns.iter().rposition(|column| column.resizable);
    if let Some(flex) = flex {
        let others: f32 = widths
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != flex)
            .map(|(_, width)| width)
            .sum();
        // The table fills the scroll inside its gutters.
        widths[flex] = (available - 2.0 * gutter - others).max(FLEX_MIN_WIDTH);
    }
}

/// The width of one line of `text`.
pub fn text_width(text: &str, font: Font, size: f32, window: &Window) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    let run = TextRun {
        len: text.len(),
        font,
        color: gpui::black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let layout = window
        .text_system()
        .layout_line(text, px(size), &[run], None);
    f32::from(layout.width)
}

/// A font of the theme's sans family at `weight`.
pub fn sans(theme: &Theme, weight: FontWeight) -> Font {
    Font {
        weight,
        ..font(theme.font_sans)
    }
}

/// `font` with `tabular-nums`: every figure as wide as the others.
pub fn tabular(font: Font) -> Font {
    Font {
        features: FontFeatures(Arc::new(vec![("tnum".into(), 1)])),
        ..font
    }
}

/// A font of the theme's mono family at `weight`.
pub fn mono(theme: &Theme, weight: FontWeight) -> Font {
    Font {
        weight,
        ..font(theme.font_mono)
    }
}

/// `.pw-data-table-cell-text.pw-inline-fade`: one line in `width`, fading out over its last
/// 24px when it does not fit.
pub fn fade_text(
    text: impl Into<SharedString>,
    font: Font,
    size: f32,
    color: Rgba,
    width: f32,
) -> AnyElement {
    div()
        .w(px(width.max(0.0)))
        .min_w(px(0.0))
        .overflow_hidden()
        .whitespace_nowrap()
        .font(font)
        .text_size(px(size))
        .text_color(color)
        .child(fade_line(text.into()))
        .into_any_element()
}

/// `DataTable.PrimaryCell`: a leading element, 10px, then the title, which takes the free
/// width in primary 12.5px/500 and fades when it does not fit, then a trailing element.
pub fn primary_cell(
    leading: impl IntoElement,
    title: impl Into<SharedString>,
    font: Font,
    trailing: Option<AnyElement>,
    cx: &App,
) -> AnyElement {
    let theme = Theme::global(cx);
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(px(10.0))
        .min_w(px(0.0))
        .child(div().flex_none().flex().items_center().child(leading))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .font(font)
                .text_size(px(12.5))
                .text_color(theme.color_text_primary)
                .child(fade_line(title)),
        )
        .children(trailing.map(|trailing| div().flex_none().flex().items_center().child(trailing)))
        .into_any_element()
}

/// A header's natural width: its label (and the sort arrow when sorted) in the 28px label,
/// inside the cell's padding.
fn header_natural(column: &Column, sorted: bool, font: &Font, window: &Window) -> f32 {
    let arrow = if sorted { SORT_GAP + SORT_ARROW } else { 0.0 };
    text_width(&column.title, font.clone(), 11.0, window)
        + arrow
        + 2.0 * LABEL_PAD_X
        + 2.0 * CELL_PAD_X
}

/// How tall the rows and the header block stand in the scroll: the header and its spacing,
/// every row on its pitch, and the spacing under the last one.
fn content_height(rows: usize, pitch: f32) -> f32 {
    HEADER_PITCH + rows as f32 * pitch + SPACING
}

/// How tall a table of `rows` rows of `row_height` stands with nothing scrolling, for a host
/// that sizes it to its content.
pub fn table_height(rows: usize, row_height: f32) -> f32 {
    content_height(rows, row_height + SPACING)
}

impl RenderOnce for DataTable {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let state =
            window.use_keyed_state(self.id.clone(), cx, |_, _: &mut Context<TableState>| {
                TableState::new()
            });
        if state.read(cx).animating() {
            window.request_animation_frame();
        }
        let measure = {
            let state = state.clone();
            canvas(
                move |bounds, _, cx| {
                    let width = f32::from(bounds.size.width);
                    let height = f32::from(bounds.size.height);
                    state.update(cx, |state, cx| {
                        if (state.available - width).abs() > 0.5
                            || (state.height - height).abs() > 0.5
                        {
                            state.available = width;
                            state.height = height;
                            cx.notify();
                        }
                    });
                },
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0()
        };
        let root = div()
            .relative()
            .size_full()
            .min_h(px(0.0))
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .child(measure);
        if self.rows == 0 {
            return root
                .child(
                    div()
                        .flex_1()
                        .min_h(px(240.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(12.5))
                        .text_color(theme.color_text_muted)
                        .child(self.empty),
                )
                .into_any_element();
        }

        let (outer_width, outer_height) = {
            let state = state.read(cx);
            (state.available, state.height)
        };
        let pitch = self.row_height + SPACING;
        if (state.read(cx).pitch - pitch).abs() > 0.01 {
            state.update(cx, |state, _| state.pitch = pitch);
        }
        let full_height = content_height(self.rows, pitch);
        let tall = full_height > outer_height + 0.5;
        let available = outer_width - if tall { SCROLLBAR } else { 0.0 };
        let frozen = state
            .read(cx)
            .frozen
            .clone()
            .filter(|widths| widths.len() == self.columns.len());
        let mut widths = match frozen {
            Some(widths) => widths,
            None => {
                // Measured at content width over the rows the virtualizer renders.
                let shown = ((outer_height / pitch).ceil() as usize + OVERSCAN).min(self.rows);
                let content: Vec<f32> = match &self.measure {
                    Some(measure) => measure(0..shown.max(1), window, cx),
                    None => self.columns.iter().map(|column| column.content).collect(),
                };
                let label_font = sans(&theme, FontWeight::MEDIUM);
                let natural: Vec<f32> = self
                    .columns
                    .iter()
                    .enumerate()
                    .map(|(index, column)| {
                        let sorted = self.sort.is_some_and(|(active, _)| active == index);
                        let header = header_natural(column, sorted, &label_font, window);
                        let cell = content.get(index).copied().unwrap_or(0.0) + 2.0 * CELL_PAD_X;
                        header
                            .max(cell)
                            .max(column.width.unwrap_or(0.0))
                            .round()
                            .min(MAX_NATURAL_WIDTH)
                    })
                    .collect();
                let widths = column_widths(&natural, &self.columns, available, self.gutter);
                if outer_width > 0.0 {
                    state.update(cx, |state, _| state.frozen = Some(widths.clone()));
                }
                widths
            }
        };
        fill_flex(&mut widths, &self.columns, available, self.gutter);
        let table_width: f32 = widths.iter().sum();
        let gutter = self.gutter;
        let wide = table_width + 2.0 * gutter > available + 0.5;
        let view_height = outer_height - if wide { SCROLLBAR } else { 0.0 };
        let tall = full_height > view_height + 0.5;
        let view_width = outer_width - if tall { SCROLLBAR } else { 0.0 };
        let inner: Rc<[f32]> = widths
            .iter()
            .map(|width| (width - 2.0 * CELL_PAD_X).max(0.0))
            .collect();

        let (scroll, x_scroll, highlight_y, opacity) = {
            let state = state.read(cx);
            (
                state.scroll.clone(),
                state.x_scroll.clone(),
                state.y.value(),
                state.opacity.value(),
            )
        };
        let y_handle = scroll.0.borrow().base_handle.clone();
        let scroll_y = f32::from(y_handle.offset().y);
        let highlight = div()
            .absolute()
            .left(px(0.0))
            .top(px(HEADER_PITCH + highlight_y + scroll_y))
            .w(px(table_width))
            .h(px(self.row_height))
            .rounded(px(theme.radius_item))
            .bg(fade(theme.color_surface_hover, opacity));

        let row_builder = self.row.clone();
        let columns = Rc::new(self.columns);
        let widths = Rc::new(widths);
        let on_activate = self.on_activate.clone();
        let list_state = state.clone();
        let (table_id, sort, on_sort) = (self.id.clone(), self.sort, self.on_sort.clone());
        // The header is the list's first item, so it scrolls away with the rows. Every item
        // takes the row pitch, so a pitch past the header's 44px lifts each row by the rest.
        let row_height = self.row_height;
        let lift = pitch - HEADER_PITCH;
        let mut list = uniform_list(self.id.clone(), self.rows + 1, move |range, window, cx| {
            range
                .map(|item| {
                    if item == 0 {
                        return div()
                            .h(px(pitch))
                            .pt(px(SPACING))
                            .child(header_row(
                                &table_id,
                                &columns,
                                &widths,
                                sort,
                                on_sort.clone(),
                                &theme,
                                window,
                                cx,
                            ))
                            .into_any_element();
                    }
                    let index = item - 1;
                    let ink = mix(
                        theme.color_text_secondary,
                        theme.color_text_primary,
                        list_state.read(cx).ink(index),
                    );
                    let row_cx = RowCx {
                        index,
                        widths: inner.clone(),
                        ink,
                    };
                    let cells = row_builder(&row_cx, window, cx);
                    body_row(
                        index,
                        row_height,
                        lift,
                        cells,
                        &columns,
                        &widths,
                        ink,
                        &list_state,
                        on_activate.clone(),
                        &theme,
                    )
                })
                .collect()
        })
        .track_scroll(scroll)
        .w(px(table_width))
        .flex_1()
        .min_h(px(0.0))
        .pb(px((SPACING - lift).max(0.0)));
        list.style().restrict_scroll_to_axis = Some(true);

        let body = div()
            .relative()
            .flex_1()
            .min_h(px(0.0))
            .w(px(table_width))
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .overflow_hidden()
                    .child(highlight),
            )
            .child(div().size_full().flex().flex_col().child(list));

        let content = div()
            .h_full()
            .flex_none()
            .w(px(table_width + 2.0 * gutter))
            .px(px(gutter))
            .flex()
            .flex_col()
            .child(body);

        let mut scroller = div()
            .id((self.id.clone(), "scroll"))
            .flex_none()
            .w(px(view_width.max(0.0)))
            .h(px(view_height.max(0.0)))
            .overflow_x_scroll()
            .track_scroll(&x_scroll)
            .child(content);
        scroller.style().restrict_scroll_to_axis = Some(true);
        let mut root = root.child(scroller);
        if tall {
            let bar = scrollbar(
                (self.id.clone(), "y-bar"),
                &y_handle,
                ScrollAxis::Vertical,
                ScrollGeometry {
                    track: view_height,
                    viewport: view_height,
                    content: full_height,
                },
                window,
                cx,
            );
            root = root.child(
                div()
                    .absolute()
                    .top(px(0.0))
                    .right(px(0.0))
                    .w(px(SCROLLBAR))
                    .h(px(view_height.max(0.0)))
                    .child(bar),
            );
        }
        if wide {
            let bar = scrollbar(
                (self.id.clone(), "x-bar"),
                &x_scroll,
                ScrollAxis::Horizontal,
                ScrollGeometry {
                    track: view_width,
                    viewport: view_width,
                    content: table_width + 2.0 * gutter,
                },
                window,
                cx,
            );
            root = root.child(
                div()
                    .absolute()
                    .left(px(0.0))
                    .bottom(px(0.0))
                    .h(px(SCROLLBAR))
                    .w(px(view_width.max(0.0)))
                    .child(bar),
            );
        }
        root.into_any_element()
    }
}

#[allow(clippy::too_many_arguments)]
fn header_row(
    id: &ElementId,
    columns: &[Column],
    widths: &[f32],
    sort: Option<(usize, SortDirection)>,
    on_sort: Option<SortFn>,
    theme: &Theme,
    window: &mut Window,
    cx: &mut App,
) -> gpui::Div {
    let last_resizable = columns.iter().rposition(|column| column.resizable);
    let mut row = div()
        .flex_none()
        .h(px(HEADER_HEIGHT))
        .flex()
        .text_size(px(11.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme.color_text_muted)
        .whitespace_nowrap();
    for (index, column) in columns.iter().enumerate() {
        let label: AnyElement = match (&on_sort, column.sortable) {
            (Some(on_sort), true) => {
                let direction = sort
                    .filter(|(active, _)| *active == index)
                    .map(|(_, direction)| direction);
                let trigger_id = ElementId::NamedInteger(format!("{id}-sort").into(), index as u64);
                let pointer = use_pointer(trigger_id.clone(), window, cx);
                let hovered = pointer.read(cx).hovered();
                let (bg, ink) = if hovered {
                    (Some(theme.color_surface_hover), theme.color_text_secondary)
                } else if direction.is_some() {
                    (Some(theme.color_surface_selected), theme.color_text_primary)
                } else {
                    (None, theme.color_text_muted)
                };
                let on_sort = on_sort.clone();
                let trigger = div()
                    .id(trigger_id)
                    .h(px(28.0))
                    .px(px(LABEL_PAD_X))
                    .flex()
                    .items_center()
                    .gap(px(SORT_GAP))
                    .min_w(px(0.0))
                    .rounded_full()
                    .when_some(bg, |trigger, bg| trigger.bg(bg))
                    .text_color(ink)
                    .cursor_pointer()
                    .child(
                        div()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .child(column.title.clone()),
                    )
                    .when_some(direction, |trigger, direction| {
                        let arrow = match direction {
                            SortDirection::Ascending => Icon::ArrowUp,
                            SortDirection::Descending => Icon::ArrowDown,
                        };
                        trigger.child(
                            div()
                                .flex_none()
                                .child(icon_colored(arrow, SORT_ARROW, ink)),
                        )
                    })
                    .on_click(move |_, window, cx| on_sort(index, window, cx));
                track(trigger, &pointer, true).into_any_element()
            }
            _ => div()
                .h(px(28.0))
                .px(px(LABEL_PAD_X))
                .flex()
                .items_center()
                .overflow_hidden()
                .child(column.title.clone())
                .into_any_element(),
        };
        let mut cell = div()
            .relative()
            .flex_none()
            .w(px(widths[index]))
            .h_full()
            .py(px(4.0))
            .px(px(CELL_PAD_X))
            .flex()
            .items_center()
            .child(label);
        if column.align == Align::End {
            cell = cell.justify_end();
        }
        if column.resizable && Some(index) != last_resizable {
            cell = cell.child(
                div()
                    .absolute()
                    .top(px(10.0))
                    .bottom(px(10.0))
                    .right(px(-0.5))
                    .w(px(1.0))
                    .bg(theme.color_border_strong),
            );
        }
        row = row.child(cell);
    }
    row
}

#[allow(clippy::too_many_arguments)]
fn body_row(
    index: usize,
    height: f32,
    lift: f32,
    cells: Vec<AnyElement>,
    columns: &[Column],
    widths: &[f32],
    ink: Rgba,
    state: &Entity<TableState>,
    on_activate: Option<ActivateFn>,
    theme: &Theme,
) -> AnyElement {
    let hover_state = state.clone();
    let hover_theme = *theme;
    let mut row = div()
        .id(("row", index))
        .h(px(height))
        .flex()
        .text_size(px(12.5))
        .text_color(ink)
        .whitespace_nowrap()
        .on_hover(move |hovered, _, cx| {
            hover_state.update(cx, |state, cx| {
                state.hover(index, *hovered, &hover_theme);
                cx.notify();
            });
        });
    for (column_index, cell) in cells.into_iter().enumerate() {
        let Some(column) = columns.get(column_index) else {
            break;
        };
        let mut slot = div()
            .flex_none()
            .w(px(widths[column_index]))
            .h_full()
            .px(px(CELL_PAD_X))
            .py(px(5.0))
            .flex()
            .items_center()
            .overflow_hidden()
            .child(cell);
        if column.align == Align::End {
            slot = slot.justify_end();
        }
        row = row.child(slot);
    }
    let row = match on_activate {
        Some(handler) => row.cursor_pointer().on_click(move |event, window, cx| {
            if event.standard_click() {
                handler(index, window, cx);
            }
        }),
        None => row,
    };
    // `uniform_list` lays each item out as a root, whose own offset Taffy ignores, so the
    // lift moves the row inside its item.
    div()
        .h(px(height + SPACING))
        .child(row.relative().top(px(-lift)))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{Column, GUTTER, column_widths};

    #[test]
    fn spare_room_shares_out_by_weight_and_the_flex_column_takes_the_rest() {
        let columns = vec![
            Column::new("Role").fixed(),
            Column::new("When"),
            Column::new("Source"),
        ];
        // No spare room: every column keeps its natural width, the flex one its minimum.
        let tight = column_widths(&[141.0, 320.0, 80.0], &columns, 400.0, GUTTER);
        assert_eq!(tight, vec![141.0, 320.0, 96.0]);
        // 224px spare over the weights 0, 1, and 2: When grows by 224·100/300.
        let wide = column_widths(&[100.0, 100.0, 100.0], &columns, 524.0, GUTTER);
        assert_eq!(wide[0], 100.0);
        assert_eq!(wide[1], 175.0);
        assert_eq!(wide[2], 524.0 - 24.0 - 100.0 - 175.0);
        // A column grows no wider than 360.
        let capped = column_widths(&[141.0, 320.0, 96.0], &columns, 757.0, GUTTER);
        assert_eq!(capped[1], 360.0);
    }
}
