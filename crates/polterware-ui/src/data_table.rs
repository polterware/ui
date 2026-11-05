//! `DataTable` (`table.tsx`, `table.css`) in its list density, virtualized.
//!
//! The table runs to its scroll's sides, so a page's padding alone sets its edges. Rows sit 4px
//! apart (`border-spacing: 0 4px`): a 36px header
//! (`padding: 4px 12px` around the 28px `.pw-data-table-column-label`, 11px/500 muted), then
//! 40px rows on a 44px pitch whose cells pad 5px 12px in 12.5px secondary text. Every
//! resizable column but the last shows its divider: a 1px `--color-border-strong` line from
//! 10px to 26px, centered on the column's right edge.
//!
//! Widths freeze once rows have a size, the way `distributeDataTableColumnWidths` does, and
//! freeze again when the scroll's width changes (unless [`DataTable::freeze_once`] keeps them),
//! until a column is resized by hand (a card
//! that settles after its first frame, a window resized): each
//! column's natural width is its widest content among the rows the virtualizer renders (the ones in
//! view and 8 more, or every row for [`DataTable::measure_every_row`]), the header's included and at least its explicit width, rounded as
//! `offsetWidth` rounds and capped at 320; the spare room shares out in proportion among the
//! resizable columns with no explicit width (the flex column, the last resizable one, weighs
//! double), each held to its minimum and maximum; and the flex column takes what the others leave,
//! at least 96, on every frame. When the columns would not fit, the [`Column::shrinks`] ones,
//! whose text fades, give up the difference in proportion to their room above 96 (or their
//! minimum), also on every frame, so the table follows the window; it runs wider than the
//! scroll and scrolls sideways only once they are all that narrow.
//! Both ways overflowing, the classic scrollbars take 12px gutters at the right and the bottom. The
//! header is the list's first item, so it scrolls away with the rows.
//!
//! A divider resizes its column, as gpui-kit's table does: an 8px band centered on the column's
//! edge takes the press, with the column-resize cursor, and the column follows the pointer from
//! its width at the press, held to its minimum and maximum, while the flex column takes up or
//! gives back the difference (the table scrolls sideways once the flex column is at its 96).
//! Under the pointer and while dragged, the divider brightens to `--color-text-muted`. A
//! double-click fits the column back to its natural width.
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

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    AnyElement, App, Context, DispatchPhase, ElementId, Entity, FocusHandle, Font, FontFeatures,
    FontWeight, IntoElement, KeyDownEvent, MouseButton, MouseMoveEvent, MouseUpEvent, Pixels,
    Point, Rgba, ScrollHandle, ScrollStrategy, SharedString, TextRun, UniformListScrollHandle,
    Window, canvas, div, font, prelude::*, px, uniform_list,
};

use super::checkbox::{checkbox_mark, checkbox_mark_mixed};
use super::ellipsis::fade as fade_line;
use super::icons::{Icon, icon_colored, icon_rotated};
use super::pointer::{track, use_pointer};
use super::scroll::{GUTTER as SCROLLBAR, ScrollAxis, ScrollGeometry, reserved_gutter, scrollbar};
use crate::ValueHandler;
use crate::motion::{Curve, SpringTier, Tween};
use crate::patterns::scroll_fade::{SCROLL_FADE, end_fade};
pub use crate::sort::{SortDirection, next_sort};
use crate::theme::{Ease, Theme, fade, mix};

/// The scroll's room on each side of the table: none, so the table's edges, and its hover
/// highlight's, sit where the page's padding puts them, in line with the toolbar's buttons.
const GUTTER: f32 = 0.0;
const SPACING: f32 = 4.0;
const HEADER_HEIGHT: f32 = 36.0;
const HEADER_LABEL_SIZE: f32 = 11.0;
/// A divider's hairline: 16px, centered in the header (10px to 26px of the 36px one).
const DIVIDER_LENGTH: f32 = 16.0;
const ROW_HEIGHT: f32 = 40.0;
/// The header block above the first row: its spacing, the 36px header, and the row gap.
const HEADER_PITCH: f32 = SPACING + HEADER_HEIGHT + SPACING;
const CELL_PAD_X: f32 = 12.0;
const LABEL_PAD_X: f32 = 8.0;
const MIN_WIDTH: f32 = 48.0;
const MAX_WIDTH: f32 = 360.0;
const FLEX_MIN_WIDTH: f32 = 96.0;
const MAX_NATURAL_WIDTH: f32 = 320.0;
/// How narrow a [`Column::shrinks`] column goes to fit the table in its scroll, unless its
/// minimum is wider.
pub const SHRINK_FLOOR: f32 = 96.0;
/// The rows the virtualizer renders past the ones in view.
const OVERSCAN: usize = 8;
const SORT_GAP: f32 = 5.0;
/// `.pw-data-table-selection-header` and `-cell`: 40px, the 14px box centered.
pub const SELECTION_WIDTH: f32 = 40.0;
const CHECK_SIZE: f32 = 14.0;
/// `.pw-data-table-group`: a 36px band, 4px in from the table's sides (`padding: 0 4px`).
const GROUP_HEIGHT: f32 = 36.0;
const GROUP_INSET: f32 = 4.0;
/// `.pw-disclosure-button`: the band's 32px collapse control.
const DISCLOSURE_WIDTH: f32 = 32.0;
/// The grab room on each side of a divider's hairline.
const HANDLE_PAD: f32 = 4.0;
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
    /// Its cells fade or cut their text, so it gives up width when the table is wider than
    /// its scroll, down to [`SHRINK_FLOOR`] or its minimum.
    pub shrinks: bool,
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
            shrinks: false,
        }
    }

    /// A column whose cells fade their text: it narrows before the table scrolls sideways.
    pub fn shrinks(mut self) -> Self {
        self.shrinks = true;
        self
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

    /// `resizable={false}`: no divider, and the flex column is the last one that resizes.
    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }
}

/// What a row's builder gets: the row, each cell's content width, and the row's text color.
pub struct RowCx {
    pub index: usize,
    pub widths: Rc<[f32]>,
    /// Secondary at rest, primary while the row is hovered.
    pub ink: Rgba,
    /// The pointer is on the row, for controls that show only then (a row's "…").
    pub hovered: bool,
}

/// One section of a grouped table (`grouping`): a header with the collapse control, the
/// group's icon, label, and count, then its rows, unless it is collapsed.
#[derive(Clone, Debug, PartialEq)]
pub struct TableGroup {
    /// Keeps the group's collapse across frames.
    pub key: SharedString,
    pub label: SharedString,
    pub icon: Option<Icon>,
    /// Its rows, in the table's row order; the groups tile the rows in order.
    pub rows: Range<usize>,
    /// The "+" at the header's end, shown while the pointer is on it (`onAdd`).
    pub add: Option<GroupAdd>,
}

/// A group's "+" (`onAdd`, `addLabel`): its label, the tooltip, and what it does.
#[derive(Clone)]
pub struct GroupAdd {
    pub label: SharedString,
    pub handler: crate::Handler,
}

impl std::fmt::Debug for GroupAdd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GroupAdd")
            .field("label", &self.label)
            .finish()
    }
}

impl PartialEq for GroupAdd {
    fn eq(&self, other: &Self) -> bool {
        self.label == other.label && Rc::ptr_eq(&self.handler, &other.handler)
    }
}

impl TableGroup {
    pub fn new(
        key: impl Into<SharedString>,
        label: impl Into<SharedString>,
        rows: Range<usize>,
    ) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            icon: None,
            rows,
            add: None,
        }
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// A "+" at the header's end, labelled `label` (`Add to <label>` on the web by default).
    pub fn on_add(mut self, label: impl Into<SharedString>, handler: crate::Handler) -> Self {
        self.add = Some(GroupAdd {
            label: label.into(),
            handler,
        });
        self
    }
}

/// What a press in the selection column, or X on the active row, asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionChange {
    /// The row joins the selection, or leaves it.
    Toggle(usize),
    /// The header's box: every row on show (`true`), or none.
    All(bool),
}

/// One item of the list under the header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Item {
    Group(usize),
    Row(usize),
}

/// The list under the header: each group's header, then its rows unless it is collapsed. An
/// ungrouped table is its rows.
fn layout_items(
    rows: usize,
    groups: &[TableGroup],
    collapsed: &HashSet<SharedString>,
) -> Vec<Item> {
    if groups.is_empty() {
        return (0..rows).map(Item::Row).collect();
    }
    let mut items = Vec::with_capacity(rows + groups.len());
    for (index, group) in groups.iter().enumerate() {
        items.push(Item::Group(index));
        if !collapsed.contains(&group.key) {
            items.extend(group.rows.clone().filter(|row| *row < rows).map(Item::Row));
        }
    }
    items
}

/// How far each item draws above its place on the uniform pitch: a group's band and its gap
/// stand `saving` shorter than a row's pitch, so every item after one moves up by that much.
fn group_lifts(items: &[Item], saving: f32) -> Vec<f32> {
    let mut groups = 0usize;
    items
        .iter()
        .map(|item| {
            let lift = groups as f32 * saving;
            if matches!(item, Item::Group(_)) {
                groups += 1;
            }
            lift
        })
        .collect()
}

/// Where each item starts below the first when rows take their own heights, and how tall all
/// of them stand: a row takes its height and the row gap, a group its band and the gap.
fn stack_items(items: &[Item], height: &dyn Fn(usize) -> f32) -> (Vec<f32>, f32) {
    let mut tops = Vec::with_capacity(items.len());
    let mut y = 0.0;
    for item in items {
        tops.push(y);
        y += match item {
            Item::Group(_) => GROUP_HEIGHT + SPACING,
            Item::Row(row) => height(*row) + SPACING,
        };
    }
    (tops, y)
}

/// The stacked items that reach into `start..end`, offsets below the first item's top.
fn stacked_range(tops: &[f32], start: f32, end: f32) -> Range<usize> {
    let first = tops.partition_point(|top| *top <= start).saturating_sub(1);
    let last = tops.partition_point(|top| *top < end);
    first..last.max(first)
}

/// Scrolls `handle` the least that shows `span` (offsets in the content) clear of chrome
/// `inset` tall and with the row gap under it, in a view of `(height, content height)`.
fn reveal(handle: &ScrollHandle, span: Range<f32>, inset: f32, (view, content): (f32, f32)) {
    let offset = -f32::from(handle.offset().y);
    let mut next = offset;
    if span.end + SPACING > next + view {
        next = span.end + SPACING - view;
    }
    if span.start < next + inset {
        next = span.start - inset;
    }
    let next = next.clamp(0.0, (content - view).max(0.0));
    if (next - offset).abs() > 0.5 {
        let x = handle.offset().x;
        handle.set_offset(gpui::point(x, px(-next)));
    }
}

type RowFn = Rc<dyn Fn(&RowCx, &mut Window, &mut App) -> Vec<AnyElement>>;
type HeightFn = Rc<dyn Fn(usize) -> f32>;
type MeasureFn = Rc<dyn Fn(Range<usize>, &mut Window, &mut App) -> Vec<f32>>;
type ActivateFn = ValueHandler<usize>;
type ContextFn = Rc<dyn Fn(usize, Point<Pixels>, &mut Window, &mut App)>;
type SortFn = ValueHandler<usize>;
type SelectFn = ValueHandler<SelectionChange>;

#[derive(IntoElement)]
pub struct DataTable {
    id: ElementId,
    columns: Vec<Column>,
    rows: usize,
    row_height: f32,
    heights: Option<HeightFn>,
    gutter: f32,
    row: RowFn,
    measure: Option<MeasureFn>,
    on_activate: Option<ActivateFn>,
    on_move: Option<ActivateFn>,
    on_context: Option<ContextFn>,
    marked: Option<usize>,
    sort: Option<(usize, SortDirection)>,
    on_sort: Option<SortFn>,
    empty: SharedString,
    inset_top: f32,
    fade: Option<Rgba>,
    groups: Vec<TableGroup>,
    selection: Option<(Rc<HashSet<usize>>, SelectFn)>,
    active: Option<usize>,
    header: (f32, f32),
    measure_all: bool,
    freeze_once: bool,
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
            heights: None,
            gutter: GUTTER,
            row: Rc::new(row),
            measure: None,
            on_activate: None,
            on_move: None,
            on_context: None,
            marked: None,
            sort: None,
            on_sort: None,
            empty: "Nothing here".into(),
            inset_top: 0.0,
            fade: None,
            groups: Vec::new(),
            selection: None,
            active: None,
            header: (HEADER_HEIGHT, HEADER_LABEL_SIZE),
            measure_all: false,
            freeze_once: false,
        }
    }

    /// The header's height and its labels' size, for a table whose header is not the list
    /// density's 36px of 11px labels (the Projects table's is 32px of 12px).
    pub fn header(mut self, height: f32, label_size: f32) -> Self {
        self.header = (height, label_size);
        self
    }

    /// `grouping`: the rows in sections, each under its header, which collapses it. The rows
    /// come in group order; groups with no rows are left out (`showEmpty: false`).
    pub fn groups(mut self, groups: Vec<TableGroup>) -> Self {
        self.groups = groups;
        self
    }

    /// `selectionMode="multiple"`: a 40px column of boxes before the others. The header's box
    /// picks every row on show, or none; a row's shows under the pointer and while picked,
    /// and the picked rows fill with `--color-surface-selected`. X picks the active row.
    pub fn selection(
        mut self,
        selected: impl IntoIterator<Item = usize>,
        on_change: impl Fn(SelectionChange, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.selection = Some((Rc::new(selected.into_iter().collect()), Rc::new(on_change)));
        self
    }

    /// `activeRowId` with `followFocus`: the open row, under a `--color-surface-raised` marker
    /// that travels between rows on the moderate spring. While the table has focus (a click
    /// on a row gives it), J or Down and K or Up move it to the next row on show, which
    /// [`Self::on_activate`] hears, and Enter activates it again.
    pub fn active(mut self, row: Option<usize>) -> Self {
        self.active = row;
        self
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

    /// `onActiveRowChange` apart from `onActivate`: J, K, Down, and Up move the marker to the
    /// row this hears, and only a click or Enter activates one. Without it, a move activates.
    pub fn on_move(mut self, handler: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_move = Some(Rc::new(handler));
        self
    }

    /// A right click on a row, with the window position it pressed: where a
    /// [`ContextMenu`](crate::patterns::ContextMenu) for the row opens.
    pub fn on_context(
        mut self,
        handler: impl Fn(usize, Point<Pixels>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_context = Some(Rc::new(handler));
        self
    }

    /// The row a menu acts on, which keeps the hover's fill while the menu is open
    /// (`.vitals-process-row--menu`).
    pub fn marked(mut self, row: Option<usize>) -> Self {
        self.marked = row;
        self
    }

    /// The rows' height. A list row (`density="list"`) is at least 40px and grows to its
    /// cells, as a 32px avatar in the cell's 5px padding makes 42; the default density sizes
    /// a row to its cells alone, as one line of 12.5px text makes 28.75.
    pub fn row_height(mut self, height: f32) -> Self {
        self.row_height = height;
        self
    }

    /// Rows of their own height, as a list row is at least 40px and grows to its cells (a
    /// two-line cell in its 5px padding makes 46): `height` gives each row's. The rows then
    /// stack in a scroll that builds only those near the view, where a table without it lays
    /// every row out on one pitch (`uniform_list`).
    pub fn row_heights(mut self, height: impl Fn(usize) -> f32 + 'static) -> Self {
        self.heights = Some(Rc::new(height));
        self
    }

    /// The default density: no gutters, so the table fills its scroll edge to edge. Its rows
    /// size to their cells, so give it a [`Self::row_height`] too.
    pub fn default_density(mut self) -> Self {
        self.gutter = 0.0;
        self
    }

    /// Room on each side of the table inside its scroll (`.pw-data-table-scroll`'s
    /// `padding-inline`, 12px in the web default), for a table that sits in a card's padding
    /// rather than a page's.
    pub fn gutters(mut self, inline: f32) -> Self {
        self.gutter = inline.max(0.0);
        self
    }

    /// Natural widths over every row, as a web table that is not `virtualized` (or is
    /// grouped) measures them, since it renders every row; the rows still draw virtualized.
    pub fn measure_every_row(mut self) -> Self {
        self.measure_all = true;
        self
    }

    /// The widths freeze once, at the first width the scroll lays out, and keep through later
    /// changes of it, as the web table's `columnsFrozen` does: the flex column takes what the
    /// others leave, at least 96, and no column shrinks, so a panel that narrows the scroll
    /// leaves the table running past it, scrolling sideways.
    pub fn freeze_once(mut self) -> Self {
        self.freeze_once = true;
        self
    }

    pub fn empty(mut self, text: impl Into<SharedString>) -> Self {
        self.empty = text.into();
        self
    }

    /// The page's chrome over the table's top (its toolbar, and the filter bar under it): at
    /// rest the header starts `inset` down, and the vertical scrollbar starts there, while the
    /// rows scroll up beneath the chrome, which fades them out on its trail
    /// ([`page_under`](crate::page_under) passes it).
    pub fn inset_top(mut self, inset: f32) -> Self {
        self.inset_top = inset.max(0.0);
        self
    }

    /// Fades the rows into `color`, the surface under the table, at each end with more to
    /// show, over as far as the scroll has moved from that end, up to 24px
    /// (`--scroll-fade-size`), so an end at rest is crisp. Under chrome
    /// ([`Self::inset_top`]) the top is the chrome's trail instead.
    pub fn fades(mut self, color: Rgba) -> Self {
        self.fade = Some(color);
        self
    }
}

/// A divider under drag: its column, the pointer's x at the press, and the column's width then.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Resize {
    column: usize,
    from_x: f32,
    from_width: f32,
}

/// The width a dragged divider gives its column: the width at the press plus how far the
/// pointer has travelled since, held to the column's minimum and maximum.
pub fn resized_width(column: &Column, from_width: f32, travel: f32) -> f32 {
    (from_width + travel)
        .clamp(column.min_width, column.max_width)
        .round()
}

/// The table's measured size, its frozen widths, its scrolls, and the travelling hover.
struct TableState {
    available: f32,
    height: f32,
    /// The widths frozen once rows had a size; the flex column's is worked out every frame.
    /// Dragging a divider rewrites its column's.
    frozen: Option<Vec<f32>>,
    /// The scroll's width the widths froze at.
    frozen_at: f32,
    /// Each column's natural width when the widths froze, which a double-click on its divider
    /// fits back to.
    natural: Vec<f32>,
    /// The columns resized by hand, which keep their width when the table fits its scroll.
    pinned: Vec<bool>,
    /// The widths the last frame drew, which a drag starts from.
    shown: Vec<f32>,
    resize: Option<Resize>,
    scroll: UniformListScrollHandle,
    x_scroll: ScrollHandle,
    hovered: Option<usize>,
    /// The row pitch of the last frame, which places the travelling hover.
    pitch: f32,
    y: Tween,
    /// The hover's height, which travels with it between rows of their own heights.
    h: Tween,
    opacity: Tween,
    ink: HashMap<usize, Tween>,
    /// Takes the keys that move the active row; a click on a row focuses it.
    focus: FocusHandle,
    /// The groups collapsed by their header, by key.
    collapsed: HashSet<SharedString>,
    /// The active marker's offset from the first row, once it has a row.
    active_y: Option<Tween>,
    /// The active row the caller last passed, and where the keys have moved it since: keys
    /// pressed before the next frame step on from there.
    active_prop: Option<usize>,
    cursor: Option<usize>,
}

impl TableState {
    fn new(focus: FocusHandle) -> Self {
        Self {
            available: 0.0,
            height: 0.0,
            frozen: None,
            frozen_at: 0.0,
            natural: Vec::new(),
            pinned: Vec::new(),
            shown: Vec::new(),
            resize: None,
            scroll: UniformListScrollHandle::new(),
            x_scroll: ScrollHandle::new(),
            hovered: None,
            pitch: ROW_HEIGHT + SPACING,
            y: Tween::new(0.0),
            h: Tween::new(ROW_HEIGHT),
            opacity: Tween::new(0.0),
            ink: HashMap::new(),
            focus,
            collapsed: HashSet::new(),
            active_y: None,
            active_prop: None,
            cursor: None,
        }
    }

    fn animating(&self) -> bool {
        self.y.is_animating()
            || self.h.is_animating()
            || self.opacity.is_animating()
            || self.ink.values().any(Tween::is_animating)
            || self.active_y.as_ref().is_some_and(Tween::is_animating)
    }

    /// Sends the active marker to `y`: it lands there at once the first time, then travels on
    /// the moderate spring; with no active row it leaves.
    fn place_active(&mut self, y: Option<f32>) {
        match (y, self.active_y.as_mut()) {
            (None, _) => self.active_y = None,
            (Some(y), None) => self.active_y = Some(Tween::new(y)),
            (Some(y), Some(tween)) => {
                if (tween.target() - y).abs() > 0.01 {
                    let moderate = SpringTier::Moderate;
                    tween.animate_to(y, moderate.duration(), moderate.curve());
                }
            }
        }
    }

    fn toggle_group(&mut self, key: &SharedString) {
        if !self.collapsed.remove(key) {
            self.collapsed.insert(key.clone());
        }
    }

    fn ink(&self, index: usize) -> f32 {
        self.ink.get(&index).map_or(0.0, Tween::value)
    }

    /// Row `index`, `y` below the first row's place and `height` tall, gains or loses the
    /// pointer.
    fn hover(&mut self, index: usize, hovered: bool, y: f32, height: f32, theme: &Theme) {
        let fast = SpringTier::Fast;
        let ink = self.ink.entry(index).or_insert_with(|| Tween::new(0.0));
        ink.animate_to(
            if hovered { 1.0 } else { 0.0 },
            theme.duration_fast,
            Curve::Ease(theme.ease_standard),
        );
        if hovered {
            if self.opacity.value() < 0.01 {
                self.y.snap(y);
                self.h.snap(height);
            } else {
                self.y.animate_to(y, fast.duration(), fast.curve());
                self.h.animate_to(height, fast.duration(), fast.curve());
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

/// This frame's widths from the frozen ones: when the columns would run past the scroll
/// with the flex column at its 96, the [`Column::shrinks`] columns give up the difference in
/// proportion to the room each has above its floor, and the flex column takes what is left.
/// A `pinned` column, one resized by hand, keeps its width. The table scrolls sideways only
/// once every shrinking column is at its floor.
pub fn fit_widths(
    frozen: &[f32],
    columns: &[Column],
    pinned: &[bool],
    available: f32,
    gutter: f32,
) -> Vec<f32> {
    let mut widths = frozen.to_vec();
    let flex = columns.iter().rposition(|column| column.resizable);
    let flex_need = if flex.is_some() { FLEX_MIN_WIDTH } else { 0.0 };
    let others: f32 = widths
        .iter()
        .enumerate()
        .filter(|(index, _)| Some(*index) != flex)
        .map(|(_, width)| width)
        .sum();
    let deficit = others + flex_need - (available - 2.0 * gutter);
    if deficit > 0.5 {
        let room = |index: usize, width: f32| -> f32 {
            let column = &columns[index];
            let shrinkable = column.shrinks
                && Some(index) != flex
                && !pinned.get(index).copied().unwrap_or(false);
            if shrinkable {
                (width - column.min_width.max(SHRINK_FLOOR)).max(0.0)
            } else {
                0.0
            }
        };
        let total: f32 = widths
            .iter()
            .enumerate()
            .map(|(index, &width)| room(index, width))
            .sum();
        if total > 0.0 {
            let take = deficit.min(total);
            for (index, width) in widths.iter_mut().enumerate() {
                let give = take * room(index, *width) / total;
                *width = (*width - give).floor();
            }
        }
    }
    fill_flex(&mut widths, columns, available, gutter);
    widths
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
    let ink = Theme::global(cx).color_text_primary;
    primary_cell_ink(leading, title, font, ink, trailing)
}

/// [`primary_cell`] with its title in `ink`, as a quieter row's (`.pw-inbox-row--deferred`
/// mutes its title).
pub fn primary_cell_ink(
    leading: impl IntoElement,
    title: impl Into<SharedString>,
    font: Font,
    ink: Rgba,
    trailing: Option<AnyElement>,
) -> AnyElement {
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
                .text_color(ink)
                .child(fade_line(title.into())),
        )
        .children(trailing.map(|trailing| div().flex_none().flex().items_center().child(trailing)))
        .into_any_element()
}

/// A header's natural width: its label (and the sort arrow when sorted) in the 28px label,
/// inside the cell's padding.
fn header_natural(
    column: &Column,
    sorted: bool,
    font: &Font,
    label_size: f32,
    window: &Window,
) -> f32 {
    let arrow = if sorted { SORT_GAP + SORT_ARROW } else { 0.0 };
    text_width(&column.title, font.clone(), label_size, window)
        + arrow
        + 2.0 * LABEL_PAD_X
        + 2.0 * CELL_PAD_X
}

/// How tall the rows and the header block stand in the scroll: the header and its spacing,
/// every row on its pitch, and the spacing under the last one.
/// Under chrome `inset` tall, the list leads with this many empty items and shifts up by what
/// they overshoot it, since every item of a uniform list takes the row `pitch`.
fn chrome_lead(inset: f32, pitch: f32) -> (usize, f32) {
    if inset <= 0.5 || pitch <= 0.0 {
        return (0, 0.0);
    }
    let spacers = (inset / pitch).ceil() as usize;
    (spacers, spacers as f32 * pitch - inset)
}

fn content_height(rows: usize, pitch: f32, header_pitch: f32) -> f32 {
    header_pitch + rows as f32 * pitch + SPACING
}

/// How tall a table of `rows` rows of `row_height` stands with nothing scrolling, for a host
/// that sizes it to its content.
pub fn table_height(rows: usize, row_height: f32) -> f32 {
    content_height(rows, row_height + SPACING, HEADER_PITCH)
}

impl RenderOnce for DataTable {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let state =
            window.use_keyed_state(self.id.clone(), cx, |_, cx: &mut Context<TableState>| {
                TableState::new(cx.focus_handle())
            });
        if state.read(cx).animating() {
            window.request_animation_frame();
        }
        let measure = {
            let state = state.clone();
            canvas(
                move |bounds, window, cx| {
                    let width = f32::from(bounds.size.width);
                    let height = f32::from(bounds.size.height);
                    let resized = state.update(cx, |state, cx| {
                        let resized = (state.available - width).abs() > 0.5
                            || (state.height - height).abs() > 0.5;
                        if resized {
                            state.available = width;
                            state.height = height;
                            cx.notify();
                        }
                        resized
                    });
                    // A notify while painting waits for the next event to draw: ask for the
                    // frame now, so the rows show the moment the table has its size.
                    if resized {
                        window.request_animation_frame();
                    }
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
                .pt(px(self.inset_top))
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
        let inset = self.inset_top;
        let collapsed = state.read(cx).collapsed.clone();
        let items: Rc<Vec<Item>> = Rc::new(layout_items(self.rows, &self.groups, &collapsed));
        let lifts: Rc<Vec<f32>> = Rc::new(group_lifts(
            &items,
            (pitch - (GROUP_HEIGHT + SPACING)).max(0.0),
        ));
        // Rows of their own heights stack, each item at its offset below the first.
        let heights = self.heights.clone();
        let stack: Option<Rc<(Vec<f32>, f32)>> = heights
            .as_ref()
            .map(|height| Rc::new(stack_items(&items, height.as_ref())));
        // A uniform list leads with spacer items under the chrome; a stack pads its top.
        let (spacers, shift) = if stack.is_some() {
            (0, 0.0)
        } else {
            chrome_lead(inset, pitch)
        };
        let lead = if stack.is_some() {
            inset
        } else {
            spacers as f32 * pitch
        };
        let row_height = self.row_height;
        let item_y: Rc<dyn Fn(usize) -> f32> = {
            let (stack, lifts) = (stack.clone(), lifts.clone());
            Rc::new(move |position| match &stack {
                Some(stack) => stack.0.get(position).copied().unwrap_or(0.0),
                None => position as f32 * pitch - lifts.get(position).copied().unwrap_or(0.0),
            })
        };
        let height_of: Rc<dyn Fn(usize) -> f32> = {
            let heights = heights.clone();
            Rc::new(move |row| heights.as_ref().map_or(row_height, |height| height(row)))
        };
        let shown_rows: Rc<Vec<usize>> = Rc::new(
            items
                .iter()
                .filter_map(|item| match item {
                    Item::Row(row) => Some(*row),
                    Item::Group(_) => None,
                })
                .collect(),
        );
        let selection_width = if self.selection.is_some() {
            SELECTION_WIDTH
        } else {
            0.0
        };
        let (header_height, label_size) = self.header;
        let header_pitch = SPACING + header_height + SPACING;
        let full_height = match &stack {
            Some(stack) => header_pitch + stack.1 + SPACING + inset,
            None => content_height(items.len(), pitch, header_pitch) + inset,
        };
        let tall = full_height > outer_height + 0.5;
        let available = outer_width - if tall { reserved_gutter() } else { 0.0 };
        let frozen = {
            let state = state.read(cx);
            let by_hand = state.pinned.iter().any(|pinned| *pinned);
            state
                .frozen
                .clone()
                .filter(|widths| widths.len() == self.columns.len())
                .filter(|_| {
                    by_hand || self.freeze_once || (state.frozen_at - available).abs() <= 0.5
                })
        };
        let widths = match frozen {
            Some(widths) => widths,
            None => {
                // Measured at content width over the rows the virtualizer renders, or all.
                let shown = if self.measure_all {
                    self.rows
                } else {
                    ((outer_height / pitch).ceil() as usize + OVERSCAN).min(self.rows)
                };
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
                        let header =
                            header_natural(column, sorted, &label_font, label_size, window);
                        let cell = content.get(index).copied().unwrap_or(0.0) + 2.0 * CELL_PAD_X;
                        header
                            .max(cell)
                            .max(column.width.unwrap_or(0.0))
                            .round()
                            .min(MAX_NATURAL_WIDTH)
                    })
                    .collect();
                let widths = column_widths(
                    &natural,
                    &self.columns,
                    available - selection_width,
                    self.gutter,
                );
                if outer_width > 0.0 {
                    state.update(cx, |state, _| {
                        state.frozen = Some(widths.clone());
                        state.frozen_at = available;
                        state.natural = natural;
                    });
                }
                widths
            }
        };
        // Frozen once, every column keeps its width as a hand-sized one does.
        let pinned = if self.freeze_once {
            vec![true; self.columns.len()]
        } else {
            state.read(cx).pinned.clone()
        };
        let widths = fit_widths(
            &widths,
            &self.columns,
            &pinned,
            available - selection_width,
            self.gutter,
        );
        if state.read(cx).shown != widths {
            state.update(cx, |state, _| state.shown = widths.clone());
        }
        let table_width: f32 = widths.iter().sum::<f32>() + selection_width;
        let gutter = self.gutter;
        let wide = table_width + 2.0 * gutter > available + 0.5;
        let view_height = outer_height - if wide { reserved_gutter() } else { 0.0 };
        let tall = full_height > view_height + 0.5;
        let view_width = outer_width - if tall { reserved_gutter() } else { 0.0 };
        let inner: Rc<[f32]> = widths
            .iter()
            .map(|width| (width - 2.0 * CELL_PAD_X).max(0.0))
            .collect();

        let (scroll, x_scroll, highlight_y, highlight_h, opacity) = {
            let state = state.read(cx);
            (
                state.scroll.clone(),
                state.x_scroll.clone(),
                state.y.value(),
                if stack.is_some() {
                    state.h.value()
                } else {
                    row_height
                },
                state.opacity.value(),
            )
        };
        let y_handle = scroll.0.borrow().base_handle.clone();
        let scroll_y = f32::from(y_handle.offset().y);
        let highlight = div()
            .absolute()
            .left(px(0.0))
            .top(px(lead + header_pitch + highlight_y + scroll_y))
            .w(px(table_width))
            .h(px(highlight_h))
            .rounded(px(theme.radius_item))
            .bg(fade(theme.color_surface_hover, opacity));
        // The active row's marker rides over the hover, under the cells.
        let active_y = self
            .active
            .and_then(|row| items.iter().position(|item| *item == Item::Row(row)))
            .map(|position| item_y(position));
        let active = self.active;
        let marker_h = active.map_or(row_height, |row| height_of(row));
        state.update(cx, |state, _| {
            state.place_active(active_y);
            if state.active_prop != active {
                state.active_prop = active;
                state.cursor = active;
            }
        });
        if state.read(cx).animating() {
            window.request_animation_frame();
        }
        let marker = state.read(cx).active_y.as_ref().map(|tween| {
            div()
                .absolute()
                .left(px(0.0))
                .top(px(lead + header_pitch + tween.value() + scroll_y))
                .w(px(table_width))
                .h(px(marker_h))
                .rounded(px(theme.radius_item))
                .bg(theme.color_surface_raised)
        });

        let row_builder = self.row.clone();
        let columns = Rc::new(self.columns);
        let drag_columns = columns.clone();
        let widths = Rc::new(widths);
        let on_activate = self.on_activate.clone();
        let on_context = self.on_context.clone();
        let marked = self.marked;
        let list_state = state.clone();
        let selection = self.selection.clone();
        let header_selection = selection.as_ref().map(|(selected, on_change)| {
            let picked = shown_rows
                .iter()
                .filter(|row| selected.contains(row))
                .count();
            let all = !shown_rows.is_empty() && picked == shown_rows.len();
            (all, picked > 0 && !all, on_change.clone())
        });
        let groups = Rc::new(self.groups.clone());
        let list_items = items.clone();
        let list_lifts = lifts.clone();
        let focus = state.read(cx).focus.clone();
        let row_focus = focus.clone();
        let (table_id, sort, on_sort) = (self.id.clone(), self.sort, self.on_sort.clone());
        // The header is the list's first item, so it scrolls away with the rows. Every item of
        // a uniform list takes the row pitch, so a pitch past the header's 44px lifts each row
        // by the rest; stacked items take their own heights and lift nothing.
        let stacked = stack.is_some();
        let lift = if stacked { 0.0 } else { pitch - header_pitch };
        let item_count = items.len() + 1 + spacers;
        let key_scroll = scroll.clone();
        let render_header = {
            let (id, columns, widths, state) = (
                table_id.clone(),
                columns.clone(),
                widths.clone(),
                list_state.clone(),
            );
            move |window: &mut Window, cx: &mut App| {
                header_row(
                    &id,
                    &columns,
                    &widths,
                    sort,
                    on_sort.clone(),
                    header_selection.clone(),
                    (header_height, label_size),
                    &state,
                    &theme,
                    window,
                    cx,
                )
            }
        };
        let render_item = {
            let (item_y, height_of) = (item_y.clone(), height_of.clone());
            move |position: usize, window: &mut Window, cx: &mut App| -> AnyElement {
                let Some(entry) = list_items.get(position).copied() else {
                    return div().h(px(pitch)).into_any_element();
                };
                let extra = if stacked {
                    0.0
                } else {
                    list_lifts.get(position).copied().unwrap_or(0.0)
                };
                let index = match entry {
                    Item::Row(index) => index,
                    Item::Group(group) => {
                        let Some(group) = groups.get(group) else {
                            return div().h(px(pitch)).into_any_element();
                        };
                        let collapsed = list_state.read(cx).collapsed.contains(&group.key);
                        return group_row(
                            &table_id,
                            position,
                            group,
                            collapsed,
                            if stacked {
                                GROUP_HEIGHT + SPACING
                            } else {
                                pitch
                            },
                            lift + extra,
                            table_width,
                            &list_state,
                            &theme,
                            window,
                            cx,
                        );
                    }
                };
                let ink = mix(
                    theme.color_text_secondary,
                    theme.color_text_primary,
                    list_state.read(cx).ink(index),
                );
                let row_cx = RowCx {
                    index,
                    widths: inner.clone(),
                    ink,
                    hovered: list_state.read(cx).hovered == Some(index),
                };
                let cells = row_builder(&row_cx, window, cx);
                let check = selection.as_ref().map(|(selected, on_change)| {
                    let picked = selected.contains(&index);
                    let hovered = list_state.read(cx).hovered == Some(index);
                    (picked, picked || hovered, on_change.clone())
                });
                body_row(
                    index,
                    height_of(index),
                    lift + extra,
                    item_y(position),
                    cells,
                    &columns,
                    &widths,
                    ink,
                    &list_state,
                    on_activate.clone(),
                    on_context.clone(),
                    marked == Some(index),
                    check,
                    &row_focus,
                    &theme,
                )
            }
        };
        let list = match &stack {
            None => {
                let mut list =
                    uniform_list(self.id.clone(), item_count, move |range, window, cx| {
                        range
                            .map(|item| {
                                if item < spacers {
                                    return div().h(px(pitch)).into_any_element();
                                }
                                let item = item - spacers;
                                if item == 0 {
                                    return div()
                                        .h(px(pitch))
                                        .pt(px(SPACING))
                                        .child(render_header(window, cx))
                                        .into_any_element();
                                }
                                render_item(item - 1, window, cx)
                            })
                            .collect()
                    })
                    .track_scroll(scroll)
                    .w(px(table_width))
                    .flex_1()
                    .min_h(px(0.0))
                    .pb(px((SPACING - lift).max(0.0)));
                list.style().restrict_scroll_to_axis = Some(true);
                list.into_any_element()
            }
            Some(stack) => {
                let (tops, total) = (&stack.0, stack.1);
                // The items from a view above to a view below what shows, so a scroll that
                // outruns a frame still finds rows drawn.
                let reach = view_height.max(ROW_HEIGHT * OVERSCAN as f32);
                let top = -scroll_y - inset - header_pitch;
                let range = stacked_range(tops, top - reach, top + view_height + reach);
                let above = tops.get(range.start).copied().unwrap_or(total);
                let below = total - tops.get(range.end).copied().unwrap_or(total);
                let header = render_header(window, cx);
                let built: Vec<AnyElement> = range
                    .map(|position| {
                        div()
                            .flex_none()
                            .child(render_item(position, window, cx))
                            .into_any_element()
                    })
                    .collect();
                let mut list = div()
                    .id((self.id.clone(), "rows"))
                    .w(px(table_width))
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .flex_col()
                    .overflow_y_scroll()
                    .track_scroll(&y_handle)
                    .child(div().flex_none().h(px(inset)))
                    .child(
                        div()
                            .flex_none()
                            .h(px(header_pitch))
                            .pt(px(SPACING))
                            .child(header),
                    )
                    .child(div().flex_none().h(px(above)))
                    .children(built)
                    .child(div().flex_none().h(px(below + SPACING)));
                list.style().restrict_scroll_to_axis = Some(true);
                list.into_any_element()
            }
        };

        let body = div()
            .relative()
            .flex_1()
            .min_h(px(0.0))
            .w(px(table_width))
            .when(shift > 0.0, |body| body.mt(px(-shift)))
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .overflow_hidden()
                    .child(highlight)
                    .children(marker),
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
        let resizing = state.read(cx).resize.is_some();
        let keys = {
            let shown_rows = shown_rows.clone();
            let items = items.clone();
            let on_activate = self.on_activate.clone();
            let on_move = self.on_move.clone().or_else(|| self.on_activate.clone());
            let on_select = self
                .selection
                .as_ref()
                .map(|(_, on_change)| on_change.clone());
            let key_state = state.clone();
            let (key_stack, key_handle, key_height) =
                (stack.clone(), y_handle.clone(), height_of.clone());
            move |event: &KeyDownEvent, window: &mut Window, cx: &mut App| {
                let keystroke = &event.keystroke;
                let mods = keystroke.modifiers;
                if mods.platform || mods.control || mods.alt || mods.function {
                    return;
                }
                let Some(active) = key_state.read(cx).cursor else {
                    return;
                };
                let step: isize = match keystroke.key.as_str() {
                    "down" | "j" => 1,
                    "up" | "k" => -1,
                    _ => 0,
                };
                if step != 0 {
                    cx.stop_propagation();
                    let Some(at) = shown_rows.iter().position(|row| *row == active) else {
                        return;
                    };
                    let next = at as isize + step;
                    if next < 0 || next as usize >= shown_rows.len() {
                        return;
                    }
                    let row = shown_rows[next as usize];
                    key_state.update(cx, |state, _| state.cursor = Some(row));
                    if let Some(position) = items.iter().position(|item| *item == Item::Row(row)) {
                        match &key_stack {
                            None => key_scroll.scroll_to_item(
                                spacers + 1 + position,
                                if step > 0 {
                                    ScrollStrategy::Bottom
                                } else {
                                    ScrollStrategy::Top
                                },
                            ),
                            Some(stack) => {
                                let top = inset + header_pitch + stack.0[position];
                                let view = (view_height, full_height);
                                reveal(&key_handle, top..top + key_height(row), inset, view);
                                window.refresh();
                            }
                        }
                    }
                    if let Some(handler) = &on_move {
                        handler(row, window, cx);
                    }
                    return;
                }
                match keystroke.key.as_str() {
                    "x" if !mods.shift => {
                        if let Some(select) = &on_select {
                            cx.stop_propagation();
                            select(SelectionChange::Toggle(active), window, cx);
                        }
                    }
                    "enter" => {
                        if let Some(handler) = &on_activate {
                            cx.stop_propagation();
                            handler(active, window, cx);
                        }
                    }
                    _ => {}
                }
            }
        };
        // The shifted list starts above the table: clip it to the table's box.
        let mut root = root
            .track_focus(&focus)
            .on_key_down(keys)
            .when(shift > 0.0, |root| root.overflow_hidden())
            .child(scroller);
        if resizing {
            root = root
                .cursor(gpui::CursorStyle::ResizeLeftRight)
                .child(resize_tracker(&state, &drag_columns));
        }
        if tall {
            let bar = scrollbar(
                (self.id.clone(), "y-bar"),
                &y_handle,
                ScrollAxis::Vertical,
                ScrollGeometry {
                    track: (view_height - inset).max(0.0),
                    viewport: view_height,
                    content: full_height,
                },
                window,
                cx,
            );
            // Under the chrome, the bar starts below it, clear of its controls.
            root = root.child(
                div()
                    .absolute()
                    .top(px(inset))
                    .right(px(0.0))
                    .w(px(SCROLLBAR))
                    .h(px((view_height - inset).max(0.0)))
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
        if let Some(color) = self.fade {
            let max = (full_height - view_height).max(0.0);
            let offset = (-scroll_y).clamp(0.0, max);
            let right = if tall { reserved_gutter() } else { 0.0 };
            let bottom = (max - offset).clamp(0.0, SCROLL_FADE);
            if bottom > 0.5 {
                root = root.child(
                    div()
                        .absolute()
                        .left(px(0.0))
                        .right(px(right))
                        .bottom(px(if wide { reserved_gutter() } else { 0.0 }))
                        .child(end_fade(color, bottom, false)),
                );
            }
            let top = offset.clamp(0.0, SCROLL_FADE);
            if inset <= 0.5 && top > 0.5 {
                root = root.child(
                    div()
                        .absolute()
                        .left(px(0.0))
                        .right(px(right))
                        .top(px(0.0))
                        .child(end_fade(color, top, true)),
                );
            }
            // The fades read the offset this frame rendered with; a scroll since, or the
            // clamp layout gives a wheel past an end, draws again (a cached view draws only
            // when notified).
            let watch = y_handle.clone();
            let view = window.current_view();
            root = root.child(
                canvas(
                    move |_, _, cx| {
                        if (f32::from(watch.offset().y) - scroll_y).abs() > 0.5 {
                            cx.notify(view);
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_0(),
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
    selection: Option<(bool, bool, SelectFn)>,
    (height, label_size): (f32, f32),
    state: &Entity<TableState>,
    theme: &Theme,
    window: &mut Window,
    cx: &mut App,
) -> gpui::Div {
    let dragged = state.read(cx).resize.map(|resize| resize.column);
    let last_resizable = columns.iter().rposition(|column| column.resizable);
    let mut row = div()
        .flex_none()
        .h(px(height))
        .flex()
        .text_size(px(label_size))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme.color_text_muted)
        .whitespace_nowrap();
    // The header's box always shows: picked when every row on show is, mixed when some are.
    if let Some((all, some, on_change)) = selection {
        let mark = if some {
            checkbox_mark_mixed(CHECK_SIZE, theme)
        } else {
            checkbox_mark(all, CHECK_SIZE, theme)
        };
        row = row.child(
            div()
                .flex_none()
                .w(px(SELECTION_WIDTH))
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .id(ElementId::Name(format!("{id}-select-all").into()))
                        .cursor_pointer()
                        .child(mark)
                        .on_click(move |_, window, cx| {
                            on_change(SelectionChange::All(!all), window, cx)
                        }),
                ),
        );
    }
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
            cell = cell.child(divider(
                id,
                index,
                dragged == Some(index),
                (height - DIVIDER_LENGTH) / 2.0,
                state,
                theme,
            ));
        }
        row = row.child(cell);
    }
    row
}

/// Column `index`'s divider: the 1px hairline from 10px to 26px on the column's right edge,
/// in an 8px band centered on it that takes the press. A press starts a resize from the
/// column's width; a double-click fits the column to its natural width.
fn divider(
    id: &ElementId,
    index: usize,
    dragged: bool,
    inset: f32,
    state: &Entity<TableState>,
    theme: &Theme,
) -> AnyElement {
    let group = SharedString::from(format!("{id}-divider-{index}"));
    let press_state = state.clone();
    let lit = theme.color_text_muted;
    div()
        .id(ElementId::NamedInteger(
            format!("{id}-divider").into(),
            index as u64,
        ))
        .group(group.clone())
        .absolute()
        .top(px(0.0))
        .bottom(px(0.0))
        .right(px(-HANDLE_PAD))
        .w(px(2.0 * HANDLE_PAD))
        .occlude()
        .cursor(gpui::CursorStyle::ResizeLeftRight)
        .on_mouse_down(MouseButton::Left, move |event, _, cx| {
            cx.stop_propagation();
            press_state.update(cx, |state, cx| {
                let natural = state.natural.get(index).copied();
                // The width on screen, which a narrow scroll may have taken under the frozen one.
                let Some(&width) = state.shown.get(index) else {
                    return;
                };
                let count = state.shown.len();
                let Some(widths) = state.frozen.as_mut() else {
                    return;
                };
                if index >= widths.len() {
                    return;
                }
                if event.click_count >= 2 {
                    widths[index] = natural.unwrap_or(width);
                    state.resize = None;
                } else {
                    widths[index] = width;
                    state.resize = Some(Resize {
                        column: index,
                        from_x: f32::from(event.position.x),
                        from_width: width,
                    });
                }
                // A column sized by hand keeps its width when the table fits its scroll.
                state.pinned.resize(count, false);
                state.pinned[index] = true;
                cx.notify();
            });
        })
        .child(
            div()
                .absolute()
                .top(px(inset))
                .bottom(px(inset))
                .left(px(HANDLE_PAD - 0.5))
                .w(px(1.0))
                .bg(if dragged {
                    lit
                } else {
                    theme.color_border_strong
                })
                .group_hover(group, move |hairline| hairline.bg(lit)),
        )
        .into_any_element()
}

/// While a divider is dragged: the window's pointer moves resize its column, wherever the
/// pointer goes, and the release ends the drag.
fn resize_tracker(state: &Entity<TableState>, columns: &Rc<Vec<Column>>) -> AnyElement {
    let move_state = state.clone();
    let up_state = state.clone();
    let columns = columns.clone();
    canvas(
        |_, _, _| {},
        move |_, _, window, _| {
            window.on_mouse_event({
                let state = move_state.clone();
                let columns = columns.clone();
                move |event: &MouseMoveEvent, phase, _, cx| {
                    if phase != DispatchPhase::Bubble {
                        return;
                    }
                    state.update(cx, |state, cx| {
                        let Some(resize) = state.resize else {
                            return;
                        };
                        if event.pressed_button != Some(MouseButton::Left) {
                            state.resize = None;
                            cx.notify();
                            return;
                        }
                        let (Some(column), Some(widths)) =
                            (columns.get(resize.column), state.frozen.as_mut())
                        else {
                            return;
                        };
                        let travel = f32::from(event.position.x) - resize.from_x;
                        let width = resized_width(column, resize.from_width, travel);
                        if let Some(slot) = widths.get_mut(resize.column)
                            && (*slot - width).abs() > 0.1
                        {
                            *slot = width;
                            cx.notify();
                        }
                    });
                }
            });
            window.on_mouse_event({
                let state = up_state.clone();
                move |event: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
                        state.update(cx, |state, cx| {
                            if state.resize.take().is_some() {
                                cx.notify();
                            }
                        });
                    }
                }
            });
        },
    )
    .absolute()
    .size_0()
    .into_any_element()
}

/// A group's header (`.pw-data-table-group`): a 36px `--color-surface-subtle` band 4px in
/// from the table's sides, holding the 32px collapse control (a muted 14px chevron, turned a
/// quarter while the group is open, primary under the pointer), 6px apart: the group's muted
/// 14px icon, its 13px/500 label, and its count in muted 12px tabular figures; a group that
/// adds shows its ghost "+" (14px) at the end while the pointer is on the header.
#[allow(clippy::too_many_arguments)]
fn group_row(
    id: &ElementId,
    position: usize,
    group: &TableGroup,
    collapsed: bool,
    pitch: f32,
    lift: f32,
    width: f32,
    state: &Entity<TableState>,
    theme: &Theme,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let toggle_id = ElementId::NamedInteger(format!("{id}-group").into(), position as u64);
    let pointer = use_pointer(toggle_id.clone(), window, cx);
    let hovered = pointer.read(cx).hovered();
    let chevron = if hovered {
        theme.color_text_primary
    } else {
        theme.color_text_muted
    };
    let key = group.key.clone();
    let toggle_state = state.clone();
    let toggle = div()
        .id(toggle_id)
        .flex_none()
        .w(px(DISCLOSURE_WIDTH))
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(theme.radius_control))
        .cursor_pointer()
        .child(icon_rotated(
            Icon::ChevronRight,
            14.0,
            chevron,
            if collapsed {
                0.0
            } else {
                std::f32::consts::FRAC_PI_2
            },
        ))
        .on_click(move |_, _, cx| {
            toggle_state.update(cx, |state, cx| {
                state.toggle_group(&key);
                cx.notify();
            });
        });
    let toggle = track(toggle, &pointer, true);
    let band_group = SharedString::from(format!("{id}-group-band-{position}"));
    let add = group.add.clone().map(|add| {
        div()
            .ml_auto()
            .flex_none()
            .invisible()
            .group_hover(band_group.clone(), |slot| slot.visible())
            .child(
                crate::icon_button::icon_button(
                    ElementId::NamedInteger(format!("{id}-group-add").into(), position as u64),
                    Icon::Plus,
                    add.label,
                    add.handler,
                    cx,
                )
                .icon_size(14.0),
            )
    });
    let band = div()
        .group(band_group)
        .size_full()
        .flex()
        .items_center()
        .gap(px(6.0))
        .pr(px(4.0))
        .rounded(px(theme.radius_item))
        .bg(theme.color_surface_subtle)
        .whitespace_nowrap()
        .child(toggle)
        .children(
            group
                .icon
                .map(|icon| icon_colored(icon, 14.0, theme.color_text_muted)),
        )
        .child(
            div()
                .text_size(px(13.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.color_text_primary)
                .child(group.label.clone()),
        )
        .child(
            div()
                .font(tabular(sans(theme, FontWeight::NORMAL)))
                .text_size(px(12.0))
                .text_color(theme.color_text_muted)
                .child(SharedString::from(group.rows.len().to_string())),
        )
        .children(add);
    div()
        .h(px(pitch))
        .child(
            div()
                .relative()
                .top(px(-lift))
                .w(px(width))
                .h(px(GROUP_HEIGHT))
                .px(px(GROUP_INSET))
                .child(band),
        )
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn body_row(
    index: usize,
    height: f32,
    lift: f32,
    y: f32,
    cells: Vec<AnyElement>,
    columns: &[Column],
    widths: &[f32],
    ink: Rgba,
    state: &Entity<TableState>,
    on_activate: Option<ActivateFn>,
    on_context: Option<ContextFn>,
    marked: bool,
    check: Option<(bool, bool, SelectFn)>,
    focus: &FocusHandle,
    theme: &Theme,
) -> AnyElement {
    let hover_state = state.clone();
    let hover_theme = *theme;
    let picked = check.as_ref().is_some_and(|(picked, _, _)| *picked);
    let mut row = div()
        .id(("row", index))
        .h(px(height))
        .flex()
        .text_size(px(12.5))
        .text_color(ink)
        .whitespace_nowrap()
        .on_hover(move |hovered, _, cx| {
            hover_state.update(cx, |state, cx| {
                state.hover(index, *hovered, y, height, &hover_theme);
                cx.notify();
            });
        });
    if let Some((picked, shown, on_change)) = check {
        let mut cell = div()
            .flex_none()
            .w(px(SELECTION_WIDTH))
            .h_full()
            .flex()
            .items_center()
            .justify_center();
        if shown {
            cell = cell.child(
                div()
                    .id(("row-select", index))
                    .cursor_pointer()
                    .child(checkbox_mark(picked, CHECK_SIZE, theme))
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        on_change(SelectionChange::Toggle(index), window, cx);
                    }),
            );
        }
        row = row.child(cell);
    }
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
        Some(handler) => {
            let focus = focus.clone();
            row.cursor_pointer().on_click(move |event, window, cx| {
                if event.standard_click() {
                    window.focus(&focus);
                    handler(index, window, cx);
                }
            })
        }
        None => row,
    };
    let row = match on_context {
        Some(handler) => row.on_mouse_down(MouseButton::Right, move |event, window, cx| {
            cx.stop_propagation();
            handler(index, event.position, window, cx);
        }),
        None => row,
    };
    let row = if picked {
        row.rounded(px(theme.radius_item))
            .bg(theme.color_surface_selected)
    } else if marked {
        row.rounded(px(theme.radius_item))
            .bg(theme.color_surface_hover)
    } else {
        row
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
    use std::cell::RefCell;
    use std::rc::Rc;

    use gpui::{
        IntoElement, Modifiers, MouseButton, MouseDownEvent, TestAppContext, div, point,
        prelude::*, px, size,
    };

    use std::collections::HashSet;

    use gpui::SharedString;

    use super::{
        CELL_PAD_X, Column, DataTable, GUTTER, HEADER_PITCH, Item, SELECTION_WIDTH,
        SelectionChange, TableGroup, chrome_lead, column_widths, fit_widths, group_lifts,
        layout_items, resized_width, reveal, stack_items, stacked_range,
    };
    use crate::test_support::{draw, open};

    #[test]
    fn chrome_leads_with_whole_rows_and_shifts_up_the_overshoot() {
        // No chrome: nothing leads.
        assert_eq!(chrome_lead(0.0, 44.0), (0, 0.0));
        // A 44px toolbar and 8px of padding over 44px rows: two rows, 36px above the top.
        assert_eq!(chrome_lead(52.0, 44.0), (2, 36.0));
        // Exactly one row tall: one row, no shift.
        assert_eq!(chrome_lead(44.0, 44.0), (1, 0.0));
    }

    #[test]
    fn shrinking_columns_give_up_room_before_the_table_scrolls() {
        let columns = vec![
            Column::new("Run").shrinks(),
            Column::new("Status"),
            Column::new("Agent").shrinks().min_width(120.0),
            Column::new("Started"),
        ];
        let frozen = [320.0, 110.0, 220.0, 100.0];
        // 600 wide: the others need 650 plus the flex column's 96, so 146 goes, 224 of room
        // above Run's 96 and 100 above Agent's 120 sharing it.
        let fitted = fit_widths(&frozen, &columns, &[], 600.0 + 2.0 * GUTTER, GUTTER);
        assert_eq!(fitted[0], (320.0 - 146.0 * 224.0 / 324.0_f32).floor());
        assert_eq!(fitted[1], 110.0);
        assert_eq!(fitted[2], (220.0 - 146.0 * 100.0 / 324.0_f32).floor());
        assert_eq!(fitted[3], 600.0 - fitted[0] - fitted[1] - fitted[2]);
        // Too narrow even at their floors: they stop there and the table scrolls.
        let narrow = fit_widths(&frozen, &columns, &[], 300.0, GUTTER);
        assert_eq!(&narrow[..3], &[96.0, 110.0, 120.0]);
        // A column resized by hand keeps its width.
        let pinned = fit_widths(&frozen, &columns, &[true], 600.0 + 2.0 * GUTTER, GUTTER);
        assert_eq!(pinned[0], 320.0);
        // Room to spare: nothing shrinks.
        let roomy = fit_widths(&frozen, &columns, &[], 1200.0, GUTTER);
        assert_eq!(&roomy[..3], &frozen[..3]);
    }

    #[test]
    fn a_drag_holds_the_column_to_its_bounds() {
        let column = Column::new("When").min_width(80.0);
        assert_eq!(resized_width(&column, 120.0, 30.4), 150.0);
        assert_eq!(resized_width(&column, 120.0, -100.0), 80.0);
        assert_eq!(resized_width(&column, 120.0, 900.0), 360.0);
    }

    #[gpui::test]
    fn dragging_a_divider_resizes_its_column_and_a_double_click_fits_it(cx: &mut TestAppContext) {
        // Each frame's cell widths, as the rows lay them out.
        let seen: Rc<RefCell<Vec<f32>>> = Rc::default();
        let sink = seen.clone();
        let mut cx = open(cx, size(800.0, 400.0), move |_, _| {
            let sink = sink.clone();
            let columns = vec![
                Column::new("Role").content(100.0),
                Column::new("When").content(100.0),
                Column::new("Source").content(60.0),
            ];
            div()
                .size_full()
                .child(DataTable::new("table", columns, 3, move |row, _, _| {
                    *sink.borrow_mut() = row.widths.to_vec();
                    (0..3).map(|_| div().into_any_element()).collect()
                }))
                .into_any_element()
        });
        draw(&mut cx);
        let width =
            |seen: &Rc<RefCell<Vec<f32>>>, index: usize| seen.borrow()[index] + 2.0 * CELL_PAD_X;
        let (role, when, source) = (width(&seen, 0), width(&seen, 1), width(&seen, 2));
        // The divider between Role and When, halfway down the 36px header under its 4px spacing.
        let at = point(px(GUTTER + role), px(22.0));
        cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(
            point(px(GUTTER + role + 40.0), px(22.0)),
            MouseButton::Left,
            Modifiers::none(),
        );
        cx.simulate_mouse_up(
            point(px(GUTTER + role + 40.0), px(22.0)),
            MouseButton::Left,
            Modifiers::none(),
        );
        draw(&mut cx);
        assert_eq!(width(&seen, 0), role + 40.0);
        assert_eq!(width(&seen, 1), when);
        // The flex column gives the room back, so the table still fills the scroll.
        assert_eq!(width(&seen, 2), source - 40.0);

        // A double-click on the moved divider fits Role back to its natural width.
        let moved = point(px(GUTTER + role + 40.0), px(22.0));
        cx.simulate_event(MouseDownEvent {
            position: moved,
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 2,
            first_mouse: false,
        });
        cx.simulate_mouse_up(moved, MouseButton::Left, Modifiers::none());
        draw(&mut cx);
        assert_eq!(width(&seen, 0), 124.0);
    }

    #[gpui::test]
    fn the_widths_share_out_again_when_the_scroll_narrows(cx: &mut TestAppContext) {
        let seen: Rc<RefCell<Vec<f32>>> = Rc::default();
        let wide = Rc::new(std::cell::Cell::new(800.0_f32));
        let (sink, width) = (seen.clone(), wide.clone());
        let mut cx = open(cx, size(900.0, 400.0), move |_, _| {
            let sink = sink.clone();
            let columns = vec![
                Column::new("Device").content(40.0),
                Column::new("Read").content(40.0),
                Column::new("Write").content(40.0),
                Column::new("Util").content(30.0),
            ];
            div()
                .w(px(width.get()))
                .h_full()
                .child(DataTable::new("table", columns, 3, move |row, _, _| {
                    *sink.borrow_mut() = row.widths.to_vec();
                    (0..4).map(|_| div().into_any_element()).collect()
                }))
                .into_any_element()
        });
        draw(&mut cx);
        let total = |seen: &Rc<RefCell<Vec<f32>>>| -> f32 {
            seen.borrow()
                .iter()
                .map(|width| width + 2.0 * CELL_PAD_X)
                .sum()
        };
        assert!((total(&seen) - 800.0).abs() < 1.0);
        // The card settles narrower: every column shares the room again, so the table fits.
        wide.set(500.0);
        cx.update(|window, _| window.refresh());
        draw(&mut cx);
        draw(&mut cx);
        assert!((total(&seen) - 500.0).abs() < 1.0, "{:?}", seen.borrow());
        assert!(seen.borrow()[0] + 2.0 * CELL_PAD_X < 200.0);
    }

    #[gpui::test]
    fn frozen_once_the_widths_keep_when_the_scroll_narrows(cx: &mut TestAppContext) {
        let seen: Rc<RefCell<Vec<f32>>> = Rc::default();
        let wide = Rc::new(std::cell::Cell::new(800.0_f32));
        let (sink, width) = (seen.clone(), wide.clone());
        let mut cx = open(cx, size(900.0, 400.0), move |_, _| {
            let sink = sink.clone();
            let columns = vec![
                Column::new("Change").content(120.0).shrinks(),
                Column::new("Actor").content(40.0),
                Column::new("When").content(40.0),
            ];
            div()
                .w(px(width.get()))
                .h_full()
                .child(
                    DataTable::new("table", columns, 3, move |row, _, _| {
                        *sink.borrow_mut() = row.widths.to_vec();
                        (0..3).map(|_| div().into_any_element()).collect()
                    })
                    .freeze_once(),
                )
                .into_any_element()
        });
        draw(&mut cx);
        let before = seen.borrow().clone();
        // A panel narrows the scroll: the columns keep their widths, the flex one its 96, and
        // the table runs past the scroll.
        wide.set(300.0);
        cx.update(|window, _| window.refresh());
        draw(&mut cx);
        draw(&mut cx);
        let after = seen.borrow().clone();
        assert_eq!(after[..2], before[..2], "{after:?}");
        assert_eq!(after[2] + 2.0 * CELL_PAD_X, super::FLEX_MIN_WIDTH);
    }

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
        assert_eq!(wide[2], 524.0 - 2.0 * GUTTER - 100.0 - 175.0);
        // A column grows no wider than 360.
        let capped = column_widths(&[141.0, 320.0, 96.0], &columns, 757.0, GUTTER);
        assert_eq!(capped[1], 360.0);
    }
    #[test]
    fn groups_lead_their_rows_and_a_collapsed_one_keeps_only_its_header() {
        let groups = vec![
            TableGroup::new("conflict", "Conflicts", 0..2),
            TableGroup::new("decision", "Questions", 2..5),
        ];
        let open = layout_items(5, &groups, &HashSet::new());
        assert_eq!(
            open,
            vec![
                Item::Group(0),
                Item::Row(0),
                Item::Row(1),
                Item::Group(1),
                Item::Row(2),
                Item::Row(3),
                Item::Row(4),
            ]
        );
        let collapsed: HashSet<SharedString> = ["conflict".into()].into_iter().collect();
        let folded = layout_items(5, &groups, &collapsed);
        assert_eq!(folded[..2], [Item::Group(0), Item::Group(1)]);
        assert_eq!(folded.len(), 5);
        // A group's 36px band and 4px gap stand 4px under a 44px row pitch: each item after
        // a group draws 4px higher.
        assert_eq!(
            group_lifts(&open, 4.0),
            vec![0.0, 4.0, 4.0, 4.0, 8.0, 8.0, 8.0]
        );
        // Ungrouped, the items are the rows.
        assert_eq!(
            layout_items(2, &[], &HashSet::new()),
            vec![Item::Row(0), Item::Row(1)]
        );
    }

    #[test]
    fn rows_of_their_own_heights_stack_with_the_gap_and_a_group_keeps_its_band() {
        let items = [Item::Group(0), Item::Row(0), Item::Row(1), Item::Row(2)];
        let (tops, total) = stack_items(&items, &|row| if row == 1 { 46.0 } else { 40.0 });
        // The band and its gap, then 44 for a 40px row and 50 for the 46px one.
        assert_eq!(tops, vec![0.0, 40.0, 84.0, 134.0]);
        assert_eq!(total, 178.0);
        // The items that reach into a span, from the one it starts in.
        assert_eq!(stacked_range(&tops, 50.0, 100.0), 1..3);
        assert_eq!(stacked_range(&tops, -100.0, 0.0), 0..0);
        assert_eq!(stacked_range(&tops, -100.0, 1.0), 0..1);
    }

    #[test]
    fn a_key_scroll_shows_the_row_clear_of_the_chrome() {
        let handle = gpui::ScrollHandle::new();
        // A row below the view comes up to its bottom, with the row gap under it.
        reveal(&handle, 500.0..546.0, 0.0, (400.0, 2000.0));
        assert_eq!(f32::from(handle.offset().y), -150.0);
        // A row under 60px of chrome comes down to just below it.
        reveal(&handle, 160.0..200.0, 60.0, (400.0, 2000.0));
        assert_eq!(f32::from(handle.offset().y), -100.0);
        // A row in view stays where it is.
        reveal(&handle, 300.0..340.0, 60.0, (400.0, 2000.0));
        assert_eq!(f32::from(handle.offset().y), -100.0);
        // Never past the content's end.
        reveal(&handle, 1990.0..2000.0, 0.0, (400.0, 2000.0));
        assert_eq!(f32::from(handle.offset().y), -1600.0);
    }

    #[gpui::test]
    fn rows_of_their_own_heights_build_only_those_near_the_view(cx: &mut TestAppContext) {
        let built: Rc<RefCell<HashSet<usize>>> = Rc::default();
        let sink = built.clone();
        let mut cx = open(cx, size(800.0, 400.0), move |_, _| {
            let sink = sink.clone();
            let columns = vec![Column::new("Change").content(100.0)];
            div()
                .size_full()
                .child(
                    DataTable::new("table", columns, 200, move |row, _, _| {
                        sink.borrow_mut().insert(row.index);
                        vec![div().into_any_element()]
                    })
                    .row_heights(|row| if row % 2 == 1 { 46.0 } else { 40.0 }),
                )
                .into_any_element()
        });
        draw(&mut cx);
        let built = built.borrow();
        assert!(built.contains(&0));
        assert!(built.len() < 40, "built {} of 200 rows", built.len());
    }

    #[gpui::test]
    fn the_selection_column_picks_rows_and_the_header_picks_all(cx: &mut TestAppContext) {
        let changes: Rc<RefCell<Vec<SelectionChange>>> = Rc::default();
        let sink = changes.clone();
        let mut cx = open(cx, size(600.0, 400.0), move |_, _| {
            let sink = sink.clone();
            div()
                .size_full()
                .child(
                    DataTable::new("picks", vec![Column::new("Item")], 3, |_, _, _| {
                        vec![div().into_any_element()]
                    })
                    .selection([1], move |change, _, _| sink.borrow_mut().push(change)),
                )
                .into_any_element()
        });
        draw(&mut cx);
        // The header's box, centered in the 40px column of the 36px header under its spacing.
        cx.simulate_click(
            point(px(SELECTION_WIDTH / 2.0), px(22.0)),
            Modifiers::none(),
        );
        // Row 1 is picked, so its box shows: the second row's middle.
        let second = HEADER_PITCH + 44.0 + 20.0;
        cx.simulate_click(
            point(px(SELECTION_WIDTH / 2.0), px(second)),
            Modifiers::none(),
        );
        assert_eq!(
            *changes.borrow(),
            vec![SelectionChange::All(true), SelectionChange::Toggle(1)]
        );
    }

    #[gpui::test]
    fn j_and_k_move_the_active_row_over_group_headers(cx: &mut TestAppContext) {
        let active: Rc<RefCell<Option<usize>>> = Rc::new(RefCell::new(Some(1)));
        let read = active.clone();
        let mut cx = open(cx, size(600.0, 400.0), move |_, _| {
            let write = read.clone();
            div()
                .size_full()
                .child(
                    DataTable::new("nav", vec![Column::new("Item")], 3, |_, _, _| {
                        vec![div().into_any_element()]
                    })
                    .groups(vec![
                        TableGroup::new("a", "A", 0..2),
                        TableGroup::new("b", "B", 2..3),
                    ])
                    .active(*read.borrow())
                    .on_activate(move |row, _, _| *write.borrow_mut() = Some(row)),
                )
                .into_any_element()
        });
        draw(&mut cx);
        // Row 1 sits under group A's header and row 0: a click there opens it and focuses the
        // table.
        let row_one = HEADER_PITCH + 40.0 + 44.0 + 20.0;
        cx.simulate_click(point(px(100.0), px(row_one)), Modifiers::none());
        assert_eq!(*active.borrow(), Some(1));
        draw(&mut cx);
        cx.simulate_keystrokes("j");
        assert_eq!(*active.borrow(), Some(2));
        draw(&mut cx);
        cx.simulate_keystrokes("k k");
        assert_eq!(*active.borrow(), Some(0));
    }

    #[gpui::test]
    fn a_move_handler_hears_j_and_k_and_only_a_click_or_enter_activates(cx: &mut TestAppContext) {
        let moved: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
        let opened: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
        let (move_log, open_log) = (moved.clone(), opened.clone());
        let mut cx = open(cx, size(600.0, 400.0), move |_, _| {
            let (move_log, open_log) = (move_log.clone(), open_log.clone());
            let active = move_log.borrow().last().copied().or(Some(0));
            div()
                .size_full()
                .child(
                    DataTable::new("moves", vec![Column::new("Item")], 3, |_, _, _| {
                        vec![div().into_any_element()]
                    })
                    .active(active)
                    .on_move(move |row, _, _| move_log.borrow_mut().push(row))
                    .on_activate(move |row, _, _| open_log.borrow_mut().push(row)),
                )
                .into_any_element()
        });
        draw(&mut cx);
        let first = HEADER_PITCH + 20.0;
        cx.simulate_click(point(px(100.0), px(first)), Modifiers::none());
        assert_eq!(*opened.borrow(), vec![0]);
        draw(&mut cx);
        cx.simulate_keystrokes("j");
        draw(&mut cx);
        cx.simulate_keystrokes("j");
        assert_eq!(*moved.borrow(), vec![1, 2]);
        assert_eq!(*opened.borrow(), vec![0]);
        draw(&mut cx);
        cx.simulate_keystrokes("enter");
        assert_eq!(*opened.borrow(), vec![0, 2]);
    }
}
