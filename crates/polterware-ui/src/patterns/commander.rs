//! Commander (⌘K): the command palette every app opens over its pages.
//!
//! It looks like the Solid app's `CommandPalette` (`@polterware/feature-commander`): the
//! Composer's frame (`surface-panel-float`, 44rem by 26rem, `--radius-dialog`), the search as
//! the Composer's title (18px semibold, `padding: 18px 24px 0`, clear of Close), and the
//! results under it (`mt-2.5 pt-1 pb-3`, 8px from the sides, `pw-scroll-fade` at both ends).
//! A group's heading is 10px medium uppercase in `--color-text-disabled` (`px-2 py-1`), a row
//! is a 20px `IconTile` and its 13px label (`px-2 py-1.5`), with its hint at the end in 11px
//! `--color-text-disabled`, and the one gliding highlight is the fluid hover fill.
//!
//! It behaves like gpui-kit's `Command`: the app hands it groups of [`CommanderItem`]s and
//! hears back [`CommanderEvent`]s; the query, the matches, and the highlight are its own.
//! Typing filters by label and keywords (a fuzzy match, best first inside each group) unless
//! the app answers the query itself ([`Commander::filterable`]); a new set of matches lights
//! the first enabled row; ↑/↓ move the highlight, wrapping and skipping disabled rows, and
//! scroll it into view; the pointer lights the row under it; Enter or a click confirms;
//! Escape clears a query first and only then cancels.
//!
//! Run mode follows `palette-body.tsx` and `commander.css`: once the host reads the query as
//! a run ([`Commander::set_run_mode`]), the results give way to the Composer's description
//! (Details, three rows), the run's property pills (`.commander-run-pills-region`), and the
//! footer's Start run, and the frame fits them instead of keeping its fixed height. Enter in
//! the title moves to Details; ⌘Enter (Ctrl+Enter) in either starts the run
//! ([`CommanderEvent::Run`]); a pill's pick is [`CommanderEvent::Property`]. The fields and
//! pickers keep gpui-kit's focus and keyboard behavior.
//!
//! GPUI clips to rectangles, so the results stop short of the frame's rounded bottom corners
//! and fade out above them instead of running under the corners.
//!
//! The rows are one flat `gpui::list` (gpui-kit's `VirtualList`): each item is a group's
//! heading or a command, and only the items in view, plus the list's overdraw, are built. The
//! highlight, the fades, and the scrollbar follow the row heights this module knows, so they
//! stay exact even before the list has measured its way to a row a jump skipped.

use std::rc::Rc;

use gpui::{
    AnyElement, App, Context, CursorStyle, Div, ElementId, Entity, EventEmitter, FocusHandle,
    Focusable, FontWeight, IntoElement, KeyBinding, KeyDownEvent, ListAlignment, ListOffset,
    ListState, MouseButton, Render, ScrollHandle, SharedString, Stateful, WeakEntity, Window,
    actions, canvas, div, linear_color_stop, linear_gradient, list, prelude::*, px,
};

use super::property_pills::{Property, PropertyPills, PropertyPillsEvent};
use crate::Handler;
use crate::button::{Button, ButtonKind, ButtonSize};
use crate::icon_button::icon_button;
use crate::icon_tile::{IconTile, TileTone};
use crate::icons::Icon;
use crate::material::{Edge, drop, edge, float_drop};
use crate::motion::{Animator, Spring, SpringTier, SpringValue};
use crate::scroll::{GUTTER, ScrollAxis, ScrollGeometry, ScrollSource, scrollbar};
use crate::text_input::{self, TextInput};
use crate::theme::Theme;

/// The palette's width and height when the window has room for them (44rem by 26rem).
pub const COMMANDER_WIDTH: f32 = 704.0;
pub const COMMANDER_HEIGHT: f32 = 416.0;
/// What the palette keeps clear of the window's edges when the window is small.
pub const COMMANDER_INSET: f32 = 40.0;

/// One result row (`py-1.5` around the 20px tile).
pub const COMMANDER_ROW_H: f32 = 32.0;
/// A group's heading (`py-1` around a 10px line).
pub const COMMANDER_HEADING_H: f32 = 24.0;
/// Between two groups (`mb-2`).
pub const COMMANDER_GROUP_GAP: f32 = 8.0;
/// The results' inset above the first row (`pt-1`) and below the last (`pb-3`).
pub const COMMANDER_LIST_TOP: f32 = 4.0;
pub const COMMANDER_LIST_BOTTOM: f32 = 12.0;

/// The results' inset from the frame's sides (`padding-inline: 0.5rem`).
const LIST_X: f32 = 8.0;
/// Between the search row and the results (`mt-2.5`).
const RESULTS_TOP: f32 = 10.0;
/// `--scroll-fade-size`.
const FADE: f32 = 24.0;
/// How far above the frame's bottom the results stop, so a row never reaches into the
/// rounded corners: at 10px up, the 30px corner is 7.6px in, inside the 8px side inset.
const CORNER_CLEAR: f32 = 10.0;
/// The row's tile (`IconTile size={20}` holding a 12px mark).
const TILE: f32 = 20.0;
const TILE_MARK: f32 = 12.0;

actions!(commander, [SelectUp, SelectDown, Confirm, Cancel]);

/// ↑, ↓, Enter, and Escape inside the palette. [`crate::init`] installs them.
pub fn register_key_bindings(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", SelectUp, Some("Commander")),
        KeyBinding::new("down", SelectDown, Some("Commander")),
        KeyBinding::new("enter", Confirm, Some("Commander")),
        KeyBinding::new("escape", Cancel, Some("Commander")),
    ]);
}

/// The palette's size in `window`: [`COMMANDER_WIDTH`] by [`COMMANDER_HEIGHT`], smaller in a
/// small window, never under 320 by 240.
pub fn commander_size(window: &Window) -> (f32, f32) {
    let viewport = window.viewport_size();
    let width = COMMANDER_WIDTH.min((f32::from(viewport.width) - COMMANDER_INSET).max(320.0));
    let height = COMMANDER_HEIGHT.min((f32::from(viewport.height) - COMMANDER_INSET).max(240.0));
    (width, height)
}

type MarkBuilder = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

/// What leads a row.
#[derive(Clone)]
pub enum CommanderMark {
    None,
    /// The 20px `IconTile` in a tone, as the Solid palette gives every command.
    Tile(Icon, TileTone),
    /// The app's own 20px mark, such as an agent's avatar.
    Custom(MarkBuilder),
}

/// One command.
#[derive(Clone)]
pub struct CommanderItem {
    id: SharedString,
    label: SharedString,
    keywords: Vec<SharedString>,
    mark: CommanderMark,
    hint: Option<SharedString>,
    disabled: bool,
}

impl CommanderItem {
    /// A command the palette reports by `id` and shows as `label`, with no mark.
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            keywords: Vec::new(),
            mark: CommanderMark::None,
            hint: None,
            disabled: false,
        }
    }

    /// Leads the row with `icon` on a tile in `tone`.
    pub fn tile(mut self, icon: Icon, tone: TileTone) -> Self {
        self.mark = CommanderMark::Tile(icon, tone);
        self
    }

    /// Leads the row with the app's own 20px mark.
    pub fn mark(mut self, build: impl Fn(&mut Window, &mut App) -> AnyElement + 'static) -> Self {
        self.mark = CommanderMark::Custom(Rc::new(build));
        self
    }

    /// More words the query matches, besides the label.
    pub fn keywords<I, S>(mut self, keywords: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<SharedString>,
    {
        self.keywords.extend(keywords.into_iter().map(Into::into));
        self
    }

    /// The quiet text at the row's end, such as its shortcut.
    pub fn hint(mut self, hint: impl Into<SharedString>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// A disabled command shows dimmed, and the highlight, the pointer, and Enter pass it by.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn id(&self) -> &SharedString {
        &self.id
    }

    pub fn label(&self) -> &SharedString {
        &self.label
    }

    /// The item's best score for `query` over its label and keywords, `None` when nothing
    /// matches.
    fn score(&self, query: &str) -> Option<i32> {
        std::iter::once(&self.label)
            .chain(self.keywords.iter())
            .filter_map(|text| fuzzy_score(text, query))
            .max()
    }
}

/// Commands under one heading. A group with no label has no heading.
#[derive(Clone, Default)]
pub struct CommanderGroup {
    label: Option<SharedString>,
    items: Vec<CommanderItem>,
}

impl CommanderGroup {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: Some(label.into()),
            items: Vec::new(),
        }
    }

    /// A group with no heading.
    pub fn untitled() -> Self {
        Self::default()
    }

    pub fn item(mut self, item: CommanderItem) -> Self {
        self.items.push(item);
        self
    }

    pub fn items(mut self, items: impl IntoIterator<Item = CommanderItem>) -> Self {
        self.items.extend(items);
        self
    }
}

/// What the palette tells its app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommanderEvent {
    /// The query changed (typed, pasted, or cleared).
    Query(String),
    /// The highlight moved to the command with this id.
    Select(SharedString),
    /// Enter or a click on the command with this id.
    Confirm(SharedString),
    /// Start the run with the title and details.
    Run { title: String, details: String },
    /// A run property changed.
    Property {
        id: SharedString,
        values: Vec<SharedString>,
    },
    /// Escape with an empty query, or Close: the app closes the palette.
    Cancel,
}

/// A row of the results: a group's heading, or a matched command (an index into `matched`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    Heading(usize),
    Item(usize),
}

/// The palette: its query field, its commands, what matches, and the highlight.
pub struct Commander {
    focus: FocusHandle,
    input: Entity<TextInput>,
    details: Entity<TextInput>,
    pills: Entity<PropertyPills>,
    run_mode: bool,
    /// The panel's glide between the results' fixed height and the run's content height
    /// (`morphPanel`): pending until the new mode's height is measured.
    morph_height: crate::motion::Tween,
    morph_pending: bool,
    /// The incoming content's fade, 0 to 1, which a run's pills and Start run also rise on.
    morph_in: crate::motion::Tween,
    /// The frame's height as last drawn, and its content's own height.
    drawn_h: Rc<std::cell::Cell<f32>>,
    natural_h: Rc<std::cell::Cell<f32>>,
    groups: Vec<CommanderGroup>,
    filterable: bool,
    empty_label: SharedString,
    footer: Option<AnyElement>,
    /// The matched commands, as (group, item) in the order they show.
    matched: Vec<(usize, usize)>,
    rows: Vec<Row>,
    selected: Option<usize>,
    applied_query: String,
    /// The ids `matched` showed last; a different set lights the first row again.
    matched_ids: Vec<SharedString>,
    reveal: bool,
    /// The results as a `gpui::list` (gpui-kit's `VirtualList`): one item per row, a group's
    /// heading or a command, so only the rows in view are built. The scrollbar, the fades,
    /// and the highlight read its scroll.
    list: ListState,
    /// The last (offset, overflow, height) the fades and the gutter were drawn for.
    seen: Rc<std::cell::Cell<(f32, f32, f32)>>,
    open: bool,
    /// What had focus when the palette opened, which it gets back on close: the field is
    /// gone once the palette has played out, and a window with nothing focused hears no
    /// shortcut, ⌘K among them, until something takes focus again.
    restore: Option<FocusHandle>,
    enter: SpringValue,
    press: SpringValue,
    indicator: Spring,
    animator: Animator,
}

impl EventEmitter<CommanderEvent> for Commander {}

impl Commander {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let weak = cx.entity().downgrade();
        let input = cx.new(|cx| {
            TextInput::new(cx)
                .bare()
                .title()
                .on_submit(move |_, window, cx| {
                    if let Some(this) = weak.upgrade() {
                        this.update(cx, |this, cx| this.confirm_selected(window, cx));
                    }
                })
        });
        let details = cx.new(|cx| {
            TextInput::new(cx)
                .description()
                .multiline()
                .placeholder("Details, files to touch, how to check it…")
        });
        let pills = cx.new(PropertyPills::new);
        cx.observe(&details, |_, _, cx| cx.notify()).detach();
        cx.observe(&pills, |_, _, cx| cx.notify()).detach();
        cx.subscribe(&pills, |_, _, event: &PropertyPillsEvent, cx| {
            let PropertyPillsEvent::Changed { id, values } = event;
            cx.emit(CommanderEvent::Property {
                id: id.clone(),
                values: values.clone(),
            });
        })
        .detach();
        cx.observe_in(&input, window, |this, _, _, cx| this.on_query_changed(cx))
            .detach();
        Self {
            focus: cx.focus_handle(),
            input,
            details,
            pills,
            run_mode: false,
            morph_height: crate::motion::Tween::new(0.0),
            morph_pending: false,
            morph_in: crate::motion::Tween::new(1.0),
            drawn_h: Rc::default(),
            natural_h: Rc::default(),
            groups: Vec::new(),
            filterable: true,
            empty_label: "No matches".into(),
            footer: None,
            matched: Vec::new(),
            rows: Vec::new(),
            selected: None,
            applied_query: String::new(),
            matched_ids: Vec::new(),
            reveal: false,
            list: ListState::new(0, ListAlignment::Top, px(COMMANDER_HEIGHT)),
            seen: Rc::new(std::cell::Cell::new((0.0, 0.0, 0.0))),
            open: false,
            restore: None,
            enter: SpringValue::new(0.0, SpringTier::Slow),
            press: SpringValue::new(0.0, SpringTier::Fast),
            indicator: Spring::selection(0.0),
            animator: Animator::new(),
        }
    }

    /// The search field's placeholder.
    pub fn set_placeholder(&mut self, placeholder: impl Into<SharedString>, cx: &mut App) {
        let placeholder = placeholder.into();
        self.input.update(cx, |input, cx| {
            input.set_placeholder(placeholder);
            cx.notify();
        });
    }

    /// What the results say when nothing matches. Default: "No matches".
    pub fn set_empty_label(&mut self, label: impl Into<SharedString>) {
        self.empty_label = label.into();
    }

    /// Off when the app answers the query itself (an async search): every command it gives
    /// shows, and it hears the query through [`CommanderEvent::Query`]. Default: on.
    pub fn set_filterable(&mut self, filterable: bool, cx: &mut Context<Self>) {
        self.filterable = filterable;
        self.update_matches(cx);
    }

    /// A row under the results, such as key hints. None by default.
    pub fn set_footer(&mut self, footer: Option<AnyElement>) {
        self.footer = footer;
    }

    /// Replaces the results with the run form. The host decides when a query is a run.
    pub fn set_run_mode(&mut self, run_mode: bool, cx: &mut Context<Self>) {
        // Only a switch on the open panel moves; its first paint snaps.
        if run_mode != self.run_mode && self.open && self.presence() > 0.99 {
            let from = self.drawn_h.get();
            if from > 0.5 {
                self.morph_height.snap(from);
                self.morph_pending = true;
                self.natural_h.set(0.0);
            }
            let moderate = SpringTier::Moderate;
            self.morph_in.snap(0.0);
            self.morph_in
                .animate_to(1.0, moderate.duration(), moderate.curve());
        }
        self.run_mode = run_mode;
        cx.notify();
    }

    /// Sets the run form's properties, in display order.
    pub fn set_run_properties(&mut self, properties: Vec<Property>, cx: &mut Context<Self>) {
        self.pills
            .update(cx, |pills, cx| pills.set_properties(properties, cx));
    }

    /// Gives the title field focus after a property is picked.
    pub fn focus_title(&self, window: &mut Window, cx: &App) {
        self.input.focus_handle(cx).focus(window);
    }

    /// The commands. The highlight stays on its command while that command still matches.
    pub fn set_groups(&mut self, groups: Vec<CommanderGroup>, cx: &mut Context<Self>) {
        self.groups = groups;
        self.update_matches(cx);
    }

    pub fn query(&self, cx: &App) -> String {
        self.input.read(cx).text().to_string()
    }

    pub fn set_query(&mut self, query: impl Into<String>, cx: &mut Context<Self>) {
        let query = query.into();
        self.input.update(cx, |input, cx| input.set_text(query, cx));
        self.on_query_changed(cx);
    }

    /// The highlighted command's id.
    pub fn selected_id(&self) -> Option<&SharedString> {
        self.selected
            .and_then(|index| self.item_at(index))
            .map(CommanderItem::id)
    }

    /// Opens with an empty query, the first row lit, and the list at its top; closing plays
    /// the palette out.
    pub fn set_open(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        if open == self.open {
            return;
        }
        self.open = open;
        if open {
            self.input.update(cx, |input, cx| input.set_text("", cx));
            self.details
                .update(cx, |details, cx| details.set_text("", cx));
            self.run_mode = false;
            self.applied_query.clear();
            self.matched_ids.clear();
            self.selected = None;
            self.update_matches(cx);
            self.list.scroll_to(ListOffset::default());
            self.press.snap(0.0);
            self.enter.snap(0.0);
            self.enter.set_target(1.0);
            self.enter.step(1.0 / 60.0);
            let field = self.input.read(cx).focus_handle(cx);
            self.restore = window
                .focused(cx)
                .filter(|focused| !field.contains(focused, window));
            field.focus(window);
        } else {
            self.enter.set_target(0.0);
            // Focus goes back only from the field: a click elsewhere since keeps its own.
            let field = self.input.read(cx).focus_handle(cx);
            let restore = self.restore.take();
            if let Some(handle) = restore
                && (field.contains_focused(window, cx)
                    || self.details.focus_handle(cx).contains_focused(window, cx))
            {
                handle.focus(window);
            }
        }
        cx.notify();
    }

    /// How far the palette is presented, 0 to 1, for the scrim behind it.
    pub fn presence(&self) -> f32 {
        self.enter.value().clamp(0.0, 1.0)
    }

    /// Whether the palette must stay mounted: open, or still playing out.
    pub fn is_presented(&self) -> bool {
        self.open || !self.enter.settled() || self.enter.value() > 0.001
    }

    fn item_at(&self, matched: usize) -> Option<&CommanderItem> {
        let (group, item) = *self.matched.get(matched)?;
        self.groups.get(group)?.items.get(item)
    }

    fn on_query_changed(&mut self, cx: &mut Context<Self>) {
        let query = self.query(cx);
        if query == self.applied_query {
            return;
        }
        self.applied_query = query.clone();
        self.update_matches(cx);
        cx.emit(CommanderEvent::Query(query));
    }

    /// Matches the query again. A new set of matches lights its first enabled command and
    /// goes back to the top; the same set keeps the highlight where it was.
    fn update_matches(&mut self, cx: &mut Context<Self>) {
        let query = self.applied_query.trim().to_string();
        let mut matched = Vec::new();
        let mut rows = Vec::new();
        for (group_index, group) in self.groups.iter().enumerate() {
            let mut scored: Vec<(usize, i32)> = group
                .items
                .iter()
                .enumerate()
                .filter_map(|(index, item)| {
                    if !self.filterable || query.is_empty() {
                        Some((index, 0))
                    } else {
                        item.score(&query).map(|score| (index, score))
                    }
                })
                .collect();
            if scored.is_empty() {
                continue;
            }
            // Best first; ties keep the app's order.
            scored.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));
            if group.label.is_some() {
                rows.push(Row::Heading(group_index));
            }
            for (index, _) in scored {
                rows.push(Row::Item(matched.len()));
                matched.push((group_index, index));
            }
        }
        let ids: Vec<SharedString> = matched
            .iter()
            .map(|(group, item)| self.groups[*group].items[*item].id.clone())
            .collect();
        let previous = self.selected_id().cloned();
        self.matched = matched;
        let previous_rows = std::mem::replace(&mut self.rows, rows);
        self.sync_rows(&previous_rows);
        if ids != self.matched_ids {
            self.matched_ids = ids;
            self.selected = self.first_enabled();
            self.list.scroll_to(ListOffset::default());
            self.reveal = false;
            if let Some(y) = self.selected.map(|index| self.row_y(index)) {
                self.indicator.snap(y);
            }
        } else {
            self.selected = previous
                .clone()
                .and_then(|id| self.matched_ids.iter().position(|seen| *seen == id))
                .or_else(|| self.first_enabled());
        }
        let now = self.selected_id().cloned();
        if now.is_some() && now != previous {
            cx.emit(CommanderEvent::Select(now.unwrap_or_default()));
        }
        cx.notify();
    }

    fn first_enabled(&self) -> Option<usize> {
        (0..self.matched.len())
            .find(|index| self.item_at(*index).is_some_and(|item| !item.disabled))
    }

    /// The y of matched command `index` inside the results column.
    fn row_y(&self, index: usize) -> f32 {
        let mut y = COMMANDER_LIST_TOP;
        let mut groups = 0;
        for row in &self.rows {
            match *row {
                Row::Heading(_) => {
                    if groups > 0 {
                        y += COMMANDER_GROUP_GAP;
                    }
                    groups += 1;
                    y += COMMANDER_HEADING_H;
                }
                Row::Item(item) => {
                    if item == index {
                        return y;
                    }
                    y += COMMANDER_ROW_H;
                }
            }
        }
        y
    }

    /// Moves the highlight `step` commands, wrapping and passing disabled ones by.
    fn select_by(&mut self, step: isize, cx: &mut Context<Self>) {
        let len = self.matched.len();
        if len == 0 {
            return;
        }
        let mut next = self.selected.unwrap_or(if step >= 0 { len - 1 } else { 0 });
        for _ in 0..len {
            next = (next as isize + step).rem_euclid(len as isize) as usize;
            if self.item_at(next).is_some_and(|item| !item.disabled) {
                self.reveal = true;
                self.select(next, cx);
                return;
            }
        }
    }

    fn select(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.selected == Some(index) {
            return;
        }
        self.selected = Some(index);
        if let Some(id) = self.selected_id().cloned() {
            cx.emit(CommanderEvent::Select(id));
        }
        cx.notify();
    }

    fn confirm(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(item) = self.item_at(index) else {
            return;
        };
        if item.disabled {
            return;
        }
        let id = item.id.clone();
        self.select(index, cx);
        cx.emit(CommanderEvent::Confirm(id));
    }

    fn confirm_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.run_mode {
            self.details.focus_handle(cx).focus(window);
            return;
        }
        if let Some(index) = self.selected {
            self.confirm(index, cx);
        }
    }

    fn submit_run(&mut self, cx: &mut Context<Self>) {
        let title = self.query(cx);
        if !title.trim().is_empty() {
            cx.emit(CommanderEvent::Run {
                title,
                details: self.details.read(cx).text().to_string(),
            });
        }
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        if !self.query(cx).is_empty() {
            self.set_query("", cx);
            return;
        }
        cx.emit(CommanderEvent::Cancel);
    }

    fn release_press(&mut self, cx: &mut Context<Self>) {
        if self.press.target() != 0.0 || !self.press.settled() {
            self.press.set_target(0.0);
            cx.notify();
        }
    }

    fn moving(&self) -> bool {
        !self.enter.settled() || !self.press.settled() || !self.indicator.is_settled()
    }

    /// Keeps the results list at one item per row. A new row structure replaces the items so
    /// no cached height stays on a row that changed shape (a heading that lost or gained its
    /// group's gap).
    fn sync_rows(&self, previous: &[Row]) {
        if self.rows != previous {
            let count = self.rows.len();
            self.list.splice(0..self.list.item_count(), count);
        }
    }

    /// Each row's height, index-aligned with `rows`: a command's [`COMMANDER_ROW_H`], or a
    /// heading's [`COMMANDER_HEADING_H`] plus [`COMMANDER_GROUP_GAP`] above every group after
    /// the first.
    fn row_heights(&self) -> Vec<f32> {
        let mut groups = 0;
        self.rows
            .iter()
            .map(|row| match *row {
                Row::Heading(_) => {
                    let height =
                        COMMANDER_HEADING_H + if groups > 0 { COMMANDER_GROUP_GAP } else { 0.0 };
                    groups += 1;
                    height
                }
                Row::Item(_) => COMMANDER_ROW_H,
            })
            .collect()
    }

    /// The position of matched command `index` in `rows`.
    fn row_of_item(&self, index: usize) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| matches!(row, Row::Item(item) if *item == index))
    }

    /// The results' scroll as (scrolled, content, viewport) heights. The rows' heights are
    /// known here, so the reading is exact even when the list has not measured the rows a jump
    /// skipped: `ListState`'s own pixel offset counts only what it has measured.
    fn list_extent(&self) -> (f32, f32, f32) {
        rows_extent(&self.list, &self.row_heights())
    }

    /// Scrolls the list so row `row` starts at `y` pixels into the rows, as [`ListOffset`]
    /// reads them.
    fn scroll_list_to(&self, y: f32) {
        scroll_rows_to(&self.list, &self.row_heights(), y);
    }

    /// Scrolls the least that shows the command `index` whole, clear of the fades' insets.
    fn reveal_row(&self, index: usize) {
        let Some(row) = self.row_of_item(index) else {
            return;
        };
        let (scrolled, _, view) = self.list_extent();
        if view < 1.0 {
            return;
        }
        let heights = self.row_heights();
        let top: f32 = heights[..row].iter().sum();
        let height = heights[row];
        let visible = (view - COMMANDER_LIST_TOP - (COMMANDER_LIST_BOTTOM - CORNER_CLEAR)).max(1.0);
        if top < scrolled {
            self.scroll_list_to(top);
        } else if top + height > scrolled + visible {
            self.scroll_list_to(top + height - visible);
        }
    }

    fn results(&mut self, overflows: bool, theme: &Theme, cx: &mut Context<Self>) -> Div {
        let gutter = if overflows { GUTTER } else { 0.0 };
        let pad = (
            COMMANDER_LIST_TOP,
            COMMANDER_LIST_BOTTOM - CORNER_CLEAR,
            LIST_X,
            gutter,
        );
        let mut column = div().relative().w_full().h_full().overflow_hidden();
        if self.matched.is_empty() {
            return column
                .pt(px(pad.0))
                .pb(px(pad.1))
                .pl(px(pad.2))
                .pr(px(pad.2 + pad.3))
                .child(
                    div()
                        .w_full()
                        .py(px(theme.space_4))
                        .text_center()
                        .text_size(px(theme.font_size_sm))
                        .text_color(theme.color_text_muted)
                        .child(self.empty_label.clone()),
                );
        }
        // The rows live in the list below, which scrolls under this column, so the one
        // highlight rides the list's scroll to stay on its row.
        let (scrolled, _, _) = self.list_extent();
        if self.selected.is_some() {
            let inset = self.press.value().clamp(0.0, 1.0);
            column = column.child(
                div()
                    .absolute()
                    .left(px(LIST_X + inset))
                    .right(px(LIST_X + gutter + inset))
                    .top(px(self.indicator.value() + inset - scrolled))
                    .h(px(COMMANDER_ROW_H - inset * 2.0))
                    .rounded(px(theme.radius_item))
                    .bg(theme.color_surface_hover),
            );
        }
        // One list item per row: what a heading shows, or a command, with the height the list
        // measures so `row_y` and the reveal line up.
        let mut groups = 0;
        let heights = self.row_heights();
        let rows: Vec<RowData> = self
            .rows
            .iter()
            .zip(heights)
            .map(|(row, height)| match *row {
                Row::Heading(group) => {
                    let label = self.groups[group].label.clone().unwrap_or_default();
                    let gap = if groups > 0 { COMMANDER_GROUP_GAP } else { 0.0 };
                    groups += 1;
                    RowData {
                        height,
                        gap,
                        label: Some(label.to_uppercase().into()),
                        item: None,
                        index: 0,
                    }
                }
                Row::Item(index) => RowData {
                    height,
                    gap: 0.0,
                    label: None,
                    item: self.item_at(index).cloned(),
                    index,
                },
            })
            .collect();
        let rows = Rc::new(rows);
        let selected = self.selected;
        let press = self.press.value().clamp(0.0, 1.0);
        let weak = cx.entity().downgrade();
        let theme = *theme;
        // `list` lays its items out across its whole width whatever its padding says, so each
        // item keeps the list's sides itself.
        let (side, end) = (pad.2, pad.2 + pad.3);
        let list = list(self.list.clone(), move |row, window, cx| {
            let Some(data) = rows.get(row) else {
                return div().into_any_element();
            };
            let Some(item) = &data.item else {
                return div()
                    .pl(px(side))
                    .pr(px(end))
                    .h(px(data.height))
                    .when(data.gap > 0.0, |heading| heading.pt(px(data.gap)))
                    .child(
                        div()
                            .h(px(COMMANDER_HEADING_H))
                            .px(px(theme.space_2))
                            .flex()
                            .items_center()
                            .text_size(px(10.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.color_text_disabled)
                            .child(data.label.clone().unwrap_or_default()),
                    )
                    .into_any_element();
            };
            div()
                .pl(px(side))
                .pr(px(end))
                .child(row_item(
                    item,
                    data.index,
                    selected == Some(data.index),
                    press,
                    &theme,
                    &weak,
                    window,
                    cx,
                ))
                .into_any_element()
        })
        .size_full()
        .pt(px(pad.0))
        .pb(px(pad.1));
        column.child(list)
    }
}

/// One item of the results list, as the list's row needs it: its height, the gap above a
/// later group's heading, and either a heading's label or a command.
struct RowData {
    height: f32,
    gap: f32,
    label: Option<SharedString>,
    item: Option<CommanderItem>,
    index: usize,
}

/// One command's row, with its hover and press moving the highlight and its click confirming.
#[allow(clippy::too_many_arguments)]
fn row_item(
    item: &CommanderItem,
    index: usize,
    active: bool,
    press: f32,
    theme: &Theme,
    weak: &WeakEntity<Commander>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let mark = match &item.mark {
        CommanderMark::None => None,
        CommanderMark::Tile(icon, tone) => Some(
            IconTile::new(*icon)
                .size(TILE, TILE_MARK)
                .tone(*tone)
                .into_any_element(),
        ),
        CommanderMark::Custom(build) => Some(build(window, cx)),
    };
    let hover = weak.clone();
    let down = weak.clone();
    let click = weak.clone();
    div()
        .id(("commander-row", index))
        .h(px(COMMANDER_ROW_H))
        .w_full()
        .px(px(theme.space_2))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(theme.space_2))
        .text_size(px(theme.font_size_md))
        .text_color(if active && !item.disabled {
            theme.color_text_primary
        } else if item.disabled {
            theme.color_text_disabled
        } else {
            theme.color_text_secondary
        })
        .opacity(if active { 1.0 - 0.18 * press } else { 1.0 })
        .when(!item.disabled, |row| {
            row.cursor(CursorStyle::PointingHand)
                .on_hover(move |hovered: &bool, _, cx| {
                    if *hovered {
                        let _ = hover.update(cx, |this, cx| {
                            this.reveal = false;
                            this.select(index, cx);
                        });
                    }
                })
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    let _ = down.update(cx, |this, cx| {
                        this.reveal = false;
                        this.select(index, cx);
                        this.press.set_target(1.0);
                        cx.notify();
                    });
                })
                .on_click(move |_, _, cx| {
                    let _ = click.update(cx, |this, cx| this.confirm(index, cx));
                })
        })
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap(px(theme.space_2))
                .children(mark.map(|mark| div().flex_none().child(mark)))
                .child(div().min_w(px(0.0)).truncate().child(item.label.clone())),
        )
        .children(item.hint.clone().map(|hint| {
            div()
                .flex_none()
                .text_size(px(theme.font_size_xs))
                .text_color(theme.color_text_disabled)
                .child(hint)
        }))
        .into_any_element()
}

/// The results' scroll as (scrolled, content, viewport) heights, from the rows' known
/// heights: the list reports only the rows it has measured, which undercounts after a jump.
fn rows_extent(list: &ListState, heights: &[f32]) -> (f32, f32, f32) {
    let top = list.logical_scroll_top();
    let scrolled = heights
        .iter()
        .take(top.item_ix.min(heights.len()))
        .sum::<f32>()
        + f32::from(top.offset_in_item);
    let content =
        COMMANDER_LIST_TOP + heights.iter().sum::<f32>() + (COMMANDER_LIST_BOTTOM - CORNER_CLEAR);
    let viewport = f32::from(list.viewport_bounds().size.height);
    (scrolled.max(0.0), content, viewport)
}

/// Scrolls `list` to `y` pixels into its rows, which start at 0.
fn scroll_rows_to(list: &ListState, heights: &[f32], y: f32) {
    let y = y.max(0.0);
    let Some(last) = heights.len().checked_sub(1) else {
        list.scroll_to(ListOffset::default());
        return;
    };
    let mut start = 0.0;
    for (index, height) in heights.iter().enumerate() {
        if index == last || start + height > y {
            list.scroll_to(ListOffset {
                item_ix: index,
                offset_in_item: px((y - start).max(0.0)),
            });
            return;
        }
        start += height;
    }
}

/// The results' scrollbar reads and moves the list through the rows' known heights, so its
/// thumb tracks a jump the list has not measured its way to yet.
#[derive(Clone)]
struct RowsScroll {
    state: ListState,
    heights: Rc<Vec<f32>>,
    scrolled: f32,
    max: f32,
}

impl ScrollSource for RowsScroll {
    fn scrolled(&self, _: ScrollAxis) -> f32 {
        self.scrolled
    }

    fn max_scroll(&self, _: ScrollAxis) -> f32 {
        self.max
    }

    fn scroll_to(&self, _: ScrollAxis, scrolled: f32) {
        scroll_rows_to(&self.state, &self.heights, scrolled);
    }
}

impl Focusable for Commander {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.read(cx).focus_handle(cx)
    }
}

impl Render for Commander {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::global(cx);
        if let Some(index) = self.selected {
            let y = self.row_y(index);
            self.indicator.set_target(y);
            if self.reveal {
                self.reveal_row(index);
                self.reveal = false;
            }
        }
        let dt = self.animator.dt(window, self.moving());
        if dt > 0.0 {
            self.enter.step(dt);
            self.press.step(dt);
            self.indicator.step(dt);
        }
        if self.moving() {
            let _ = self.animator.dt(window, true);
        }
        if self.open && window.focused(cx).is_none() {
            self.input.read(cx).focus_handle(cx).focus(window);
        }

        // The glide starts once the new mode's height is known: the results' frame at once,
        // the run's content after the frame it was first measured in.
        let (_, palette_height) = commander_size(window);
        if self.morph_pending {
            let target = if self.run_mode {
                self.natural_h.get()
            } else {
                palette_height
            };
            if target > 0.5 {
                let slow = SpringTier::Slow;
                if target < self.morph_height.value() {
                    self.morph_height
                        .animate_to(target, slow.exit_duration(), slow.exit_curve());
                } else {
                    self.morph_height
                        .animate_to(target, slow.duration(), slow.curve());
                }
                self.morph_pending = false;
            }
            window.request_animation_frame();
        }
        let glide = (self.morph_pending || self.morph_height.is_animating())
            .then(|| self.morph_height.value());
        if glide.is_some() || self.morph_in.is_animating() {
            window.request_animation_frame();
        }
        let content_in = self.morph_in.value().clamp(0.0, 1.0);
        let (offset, content, viewport) = self.list_extent();
        let max = (content - viewport).max(0.0);
        let overflows = max > 0.5;
        let results = self.results(overflows, &theme, cx);
        let bar = overflows.then(|| {
            scrollbar(
                "commander-scrollbar",
                &RowsScroll {
                    state: self.list.clone(),
                    heights: Rc::new(self.row_heights()),
                    scrolled: offset,
                    max,
                },
                ScrollAxis::Vertical,
                ScrollGeometry {
                    track: viewport,
                    viewport,
                    content: viewport + max,
                },
                window,
                cx,
            )
        });
        let weak = cx.entity().downgrade();
        let on_close: Handler = Rc::new(move |_, cx| {
            if let Some(this) = weak.upgrade() {
                this.update(cx, |_, cx| cx.emit(CommanderEvent::Cancel));
            }
        });
        let seen = self.seen.clone();
        let watch = self.list.clone();
        let heights = Rc::new(self.row_heights());
        // The fades and the gutter read the scroll of the frame before; draw again once the
        // offset, the overflow, or the height has moved.
        let watcher = canvas(
            move |_, window, _| {
                let (scrolled, content, viewport) = rows_extent(&watch, &heights);
                let now = (scrolled, content, viewport);
                let last = seen.get();
                if (now.0 - last.0).abs() > 0.5
                    || (now.1 - last.1).abs() > 0.5
                    || (now.2 - last.2).abs() > 0.5
                {
                    seen.set(now);
                    crate::frame::draw_again(window);
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_0();
        let fades = Fades {
            top: offset.clamp(0.0, FADE),
            bottom: (max - offset).clamp(0.0, FADE),
        };
        let rise = if self.run_mode {
            (1.0 - content_in) * 6.0
        } else {
            0.0
        };
        let (body, footer, bar, fades) = if self.run_mode {
            let description = self.details.clone();
            let body = div()
                .px(px(theme.space_6))
                .flex()
                .flex_col()
                .gap(px(10.0))
                .child(
                    div()
                        .id("commander-description")
                        .min_h(px(60.0))
                        .cursor_text()
                        .on_click(move |_, window, cx| description.focus_handle(cx).focus(window))
                        .child(self.details.clone()),
                )
                .child(
                    div()
                        .relative()
                        .top(px(rise))
                        .opacity(content_in)
                        .pt(px(4.0))
                        .child(self.pills.clone()),
                );
            let weak = cx.entity().downgrade();
            let submit = Button::new("commander-start-run", ButtonKind::Primary)
                .size(ButtonSize::Sm)
                .label("Start run")
                .on_click(move |_, cx| {
                    let _ = weak.update(cx, |this, cx| this.submit_run(cx));
                });
            (
                body.into_any_element(),
                Some(
                    div()
                        .relative()
                        .top(px(rise))
                        .opacity(content_in)
                        .flex()
                        .justify_end()
                        .child(submit)
                        .into_any_element(),
                ),
                None,
                Fades::default(),
            )
        } else {
            (
                div()
                    .size_full()
                    .opacity(content_in)
                    .child(results)
                    .into_any_element(),
                self.footer.take(),
                bar,
                fades,
            )
        };
        // The search row holds the field alone, in both modes (`.commander-search-row`).
        let input = self.input.clone();
        let overlay = self.pills.read(cx).overlay(cx);
        palette_frame(
            "commander-palette",
            commander_size(window),
            self.run_mode,
            Glide {
                height: glide,
                drawn: self.drawn_h.clone(),
                natural: self.natural_h.clone(),
            },
            self.presence(),
            input.into_any_element(),
            body,
            None,
            fades,
            bar,
            footer,
            on_close,
            &theme,
            cx,
        )
        .key_context("Commander")
        .track_focus(&self.focus)
        .capture_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
            let key = &event.keystroke;
            if this.run_mode
                && key.key == "enter"
                && (key.modifiers.platform || key.modifiers.control)
                && !key.modifiers.shift
                && !key.modifiers.alt
            {
                cx.stop_propagation();
                this.submit_run(cx);
            }
        }))
        .on_action(cx.listener(|this, _: &SelectUp, _, cx| this.select_by(-1, cx)))
        .on_action(cx.listener(|this, _: &SelectDown, _, cx| this.select_by(1, cx)))
        .on_action(cx.listener(|this, _: &Confirm, window, cx| this.confirm_selected(window, cx)))
        .on_action(cx.listener(|this, _: &Cancel, _, cx| this.cancel(cx)))
        // The field lets Escape through when it has nothing of its own to cancel.
        .on_action(cx.listener(|this, _: &text_input::Cancel, _, cx| this.cancel(cx)))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|this, _, _, cx| this.release_press(cx)),
        )
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(|this, _, _, cx| this.release_press(cx)),
        )
        .child(watcher)
        .children(overlay)
    }
}

/// How tall each end's fade is: `pw-scroll-fade` grows each one over the first 24px of
/// scroll away from its end.
#[derive(Clone, Copy, Default)]
struct Fades {
    top: f32,
    bottom: f32,
}

/// The palette's frame for an app that draws its own results: `input` in the search row with
/// Close at the right, `results` scrolling under it on `scroll`. `presence` (0 to 1) fades it
/// in and lifts it the last 8px. Prefer [`Commander`], which brings the results and the keys.
#[allow(clippy::too_many_arguments)]
pub fn commander_panel(
    id: impl Into<ElementId>,
    size: (f32, f32),
    presence: f32,
    input: impl IntoElement,
    results: impl IntoElement,
    scroll: &ScrollHandle,
    on_close: Handler,
    theme: &Theme,
    cx: &App,
) -> Stateful<Div> {
    palette_frame(
        id,
        size,
        false,
        Glide {
            height: None,
            drawn: Rc::default(),
            natural: Rc::default(),
        },
        presence,
        input.into_any_element(),
        results.into_any_element(),
        Some(scroll),
        Fades::default(),
        None,
        None,
        on_close,
        theme,
        cx,
    )
}

/// The frame's height while it glides between modes, and where it reports what it drew.
struct Glide {
    height: Option<f32>,
    drawn: Rc<std::cell::Cell<f32>>,
    natural: Rc<std::cell::Cell<f32>>,
}

#[allow(clippy::too_many_arguments)]
fn palette_frame(
    id: impl Into<ElementId>,
    (width, height): (f32, f32),
    run_mode: bool,
    glide: Glide,
    presence: f32,
    input: AnyElement,
    results: AnyElement,
    // The scroll `results` own, for `commander_panel`'s caller-drawn list; `None` when the
    // results bring their own (a `gpui::list`), which the frame then just clips.
    scroll: Option<&ScrollHandle>,
    fades: Fades,
    bar: Option<AnyElement>,
    footer: Option<AnyElement>,
    on_close: Handler,
    theme: &Theme,
    cx: &App,
) -> Stateful<Div> {
    let t = presence.clamp(0.0, 1.0);
    let radius = theme.radius_dialog;
    let mut shadow = float_drop(theme);
    shadow.push(drop(20.0, 56.0, theme.color_shadow_float));
    let surface = theme.color_surface_subtle;
    let mut clear = surface;
    clear.a = 0.0;
    let fade = |height: f32, to_bottom: bool| {
        let (from, to) = if to_bottom {
            (clear, surface)
        } else {
            (surface, clear)
        };
        // Over the rows' band only: a full-width fade would reach past the rounded corners.
        div()
            .absolute()
            .left(px(LIST_X))
            .right(px(LIST_X))
            .h(px(height))
            .bg(linear_gradient(
                180.0,
                linear_color_stop(from, 0.0),
                linear_color_stop(to, 1.0),
            ))
    };
    let has_footer = footer.is_some();
    // The frame's top and height as drawn, which the glide starts from.
    let frame_top: Rc<std::cell::Cell<f32>> = Rc::default();
    let measure = canvas(
        {
            let (drawn, top) = (glide.drawn.clone(), frame_top.clone());
            move |bounds, _, _| {
                top.set(f32::from(bounds.origin.y));
                drawn.set(f32::from(bounds.size.height));
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_full();
    div()
        .id(id)
        .occlude()
        .relative()
        .w(px(width))
        .map(|frame| match glide.height {
            // Mid-glide the frame clips what it has not reached yet.
            Some(glided) => frame.h(px(glided)).overflow_hidden(),
            None => frame.when(!run_mode, |frame| frame.h(px(height))),
        })
        .max_h(px(height))
        .mt(px((1.0 - t) * 8.0))
        .opacity(t)
        .flex()
        .flex_col()
        .rounded(px(radius))
        .bg(surface)
        .shadow(shadow)
        .text_color(theme.color_text_primary)
        .font_family(theme.font_sans)
        .cursor(CursorStyle::IBeam)
        .child(measure)
        .child(
            div()
                .relative()
                .flex_none()
                .pt(px(18.0))
                .pl(px(theme.space_6))
                .pr(px(theme.space_6 + theme.space_8))
                .flex()
                .items_center()
                .child(div().flex_1().min_w(px(0.0)).child(input))
                .child(
                    div()
                        .absolute()
                        .top(px(theme.space_4))
                        .right(px(theme.space_4))
                        .cursor(CursorStyle::PointingHand)
                        .child(icon_button(
                            "commander-close",
                            Icon::Close,
                            "Close",
                            on_close,
                            cx,
                        )),
                ),
        )
        .child(
            div()
                .relative()
                .map(|area| {
                    if run_mode {
                        // At its own height while the frame glides, so it can be measured.
                        area.flex_none()
                    } else {
                        area.flex_1().min_h(px(0.0))
                    }
                })
                .mt(px(RESULTS_TOP))
                .when(!has_footer, |area| area.mb(px(CORNER_CLEAR)))
                .child(
                    div()
                        .id("commander-results")
                        .w_full()
                        .when(!run_mode && scroll.is_none(), |body| {
                            body.h_full().overflow_hidden()
                        })
                        .when(!run_mode && scroll.is_some(), |body| {
                            body.h_full().overflow_y_scroll()
                        })
                        .when_some(scroll, |body, scroll| body.track_scroll(scroll))
                        .cursor(CursorStyle::PointingHand)
                        .child(results),
                )
                .when(fades.top > 0.5, |area| {
                    area.child(fade(fades.top, false).top(px(0.0)))
                })
                .when(fades.bottom > 0.5, |area| {
                    area.child(fade(fades.bottom, true).bottom(px(0.0)))
                })
                .children(bar.map(|bar| {
                    div()
                        .absolute()
                        .top(px(0.0))
                        .right(px(0.0))
                        .bottom(px(0.0))
                        .w(px(GUTTER))
                        .child(bar)
                })),
        )
        .children(footer.map(|footer| {
            div()
                .flex_none()
                .px(px(theme.space_6))
                .pt(px(theme.space_2))
                .pb(px(theme.space_4))
                .child(footer)
        }))
        // The content's end, whose distance from the frame's top is its own height.
        .child(
            canvas(
                {
                    let (natural, top) = (glide.natural.clone(), frame_top.clone());
                    move |bounds, _, _| {
                        natural.set(f32::from(bounds.origin.y) - top.get());
                    }
                },
                |_, _, _, _| {},
            )
            .flex_none()
            .w_full()
            .h(px(0.0)),
        )
        .child(edge(Edge::Quiet, radius, theme))
}

/// A case-insensitive subsequence score; an empty query matches everything at 0. Each
/// matched character scores 1, one right after the last 8 more, one at the start 6 more, and
/// one at a word's start 4 more, so a typed prefix wins.
pub fn fuzzy_score(text: &str, query: &str) -> Option<i32> {
    let needle = fold(query.trim());
    if needle.is_empty() {
        return Some(0);
    }
    let hay = fold(text);
    let mut score = 0_i32;
    let mut from = 0_usize;
    let mut previous: Option<usize> = None;
    for needle_ch in needle {
        let found = hay[from..]
            .iter()
            .position(|ch| *ch == needle_ch)
            .map(|offset| from + offset)?;
        score += 1;
        if previous == Some(found.saturating_sub(1)) {
            score += 8;
        }
        if found == 0 {
            score += 6;
        } else if !hay[found - 1].is_alphanumeric() {
            score += 4;
        }
        previous = Some(found);
        from = found + 1;
    }
    Some(score)
}

fn fold(text: &str) -> Vec<char> {
    text.chars().flat_map(|ch| ch.to_lowercase()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    type CommanderSlot = Rc<std::cell::RefCell<Option<Entity<Commander>>>>;

    fn groups() -> Vec<CommanderGroup> {
        vec![
            CommanderGroup::new("Navigation")
                .item(
                    CommanderItem::new("nav.stats", "Stats")
                        .tile(Icon::Activity, TileTone::Neutral),
                )
                .item(CommanderItem::new("nav.spend", "Spend").keywords(["usage", "cost"]))
                .item(CommanderItem::new("nav.off", "Offline").disabled(true)),
            CommanderGroup::new("Actions").item(CommanderItem::new("action.reload", "Reload")),
        ]
    }

    fn commander(cx: &mut TestAppContext) -> (Entity<Commander>, &mut gpui::VisualTestContext) {
        cx.update(crate::init);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut commander = Commander::new(window, cx);
            commander.set_groups(groups(), cx);
            commander
        });
        (view, cx)
    }

    #[test]
    fn fuzzy_prefers_a_typed_prefix() {
        assert_eq!(fuzzy_score("Stats", ""), Some(0));
        assert!(fuzzy_score("Stats", "st") > fuzzy_score("Settings", "st"));
        assert_eq!(fuzzy_score("Stats", "xyz"), None);
    }

    #[gpui::test]
    fn run_title_enter_focuses_details_and_modified_enter_submits(cx: &mut TestAppContext) {
        let (view, cx) = commander(cx);
        let events = Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = events.clone();
        cx.update(|window, cx| {
            cx.subscribe(&view, move |_, event: &CommanderEvent, _| {
                sink.borrow_mut().push(event.clone())
            })
            .detach();
            view.update(cx, |view, cx| {
                view.set_open(true, window, cx);
                view.set_run_mode(true, cx);
            });
        });
        cx.run_until_parked();
        cx.simulate_input("Title");
        cx.simulate_keystrokes("enter");
        cx.simulate_input("Details");
        cx.simulate_keystrokes("cmd-enter");
        assert!(events.borrow().contains(&CommanderEvent::Run {
            title: "Title".into(),
            details: "Details".into()
        }));
    }

    #[gpui::test]
    fn moving_wraps_and_skips_disabled(cx: &mut TestAppContext) {
        let (view, cx) = commander(cx);
        view.update(cx, |this, cx| {
            assert_eq!(this.selected_id().map(|id| id.as_ref()), Some("nav.stats"));
            this.select_by(1, cx);
            assert_eq!(this.selected_id().map(|id| id.as_ref()), Some("nav.spend"));
            this.select_by(1, cx);
            assert_eq!(
                this.selected_id().map(|id| id.as_ref()),
                Some("action.reload")
            );
            this.select_by(1, cx);
            assert_eq!(this.selected_id().map(|id| id.as_ref()), Some("nav.stats"));
            this.select_by(-1, cx);
            assert_eq!(
                this.selected_id().map(|id| id.as_ref()),
                Some("action.reload")
            );
        });
    }

    #[gpui::test]
    fn a_query_matches_keywords_and_relights_the_first_row(cx: &mut TestAppContext) {
        let (view, cx) = commander(cx);
        view.update(cx, |this, cx| {
            this.select_by(1, cx);
            this.set_query("usage", cx);
            assert_eq!(this.matched.len(), 1);
            assert_eq!(this.selected_id().map(|id| id.as_ref()), Some("nav.spend"));
            this.set_query("zzz", cx);
            assert!(this.matched.is_empty());
            assert_eq!(this.selected_id(), None);
        });
    }

    #[gpui::test]
    fn escape_clears_the_query_before_it_cancels(cx: &mut TestAppContext) {
        let (view, cx) = commander(cx);
        let events = Rc::new(std::cell::RefCell::new(Vec::new()));
        let seen = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&view, move |_, event: &CommanderEvent, _| {
                seen.borrow_mut().push(event.clone());
            })
            .detach();
        });
        view.update(cx, |this, cx| {
            this.set_query("re", cx);
            this.cancel(cx);
            assert_eq!(this.query(cx), "");
        });
        assert!(!events.borrow().contains(&CommanderEvent::Cancel));
        view.update(cx, |this, cx| this.cancel(cx));
        assert!(events.borrow().contains(&CommanderEvent::Cancel));
    }

    #[gpui::test]
    fn confirm_reports_the_lit_command_and_passes_disabled_ones_by(cx: &mut TestAppContext) {
        let (view, cx) = commander(cx);
        let events = Rc::new(std::cell::RefCell::new(Vec::new()));
        let seen = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&view, move |_, event: &CommanderEvent, _| {
                seen.borrow_mut().push(event.clone());
            })
            .detach();
        });
        view.update(cx, |this, cx| {
            this.confirm(2, cx);
            this.select_by(1, cx);
            this.confirm_selected_for_test(cx);
        });
        let events = events.borrow();
        assert!(!events.contains(&CommanderEvent::Confirm("nav.off".into())));
        assert!(events.contains(&CommanderEvent::Confirm("nav.spend".into())));
    }

    impl Commander {
        fn confirm_selected_for_test(&mut self, cx: &mut Context<Self>) {
            if let Some(index) = self.selected {
                self.confirm(index, cx);
            }
        }
    }

    #[gpui::test]
    fn row_y_steps_through_headings_and_gaps(cx: &mut TestAppContext) {
        let (view, cx) = commander(cx);
        view.update(cx, |this, _| {
            let first = COMMANDER_LIST_TOP + COMMANDER_HEADING_H;
            assert_eq!(this.row_y(0), first);
            assert_eq!(this.row_y(1), first + COMMANDER_ROW_H);
            assert_eq!(
                this.row_y(3),
                first + 3.0 * COMMANDER_ROW_H + COMMANDER_GROUP_GAP + COMMANDER_HEADING_H
            );
            // The list's row heights, which the reveal and the scrollbar read.
            assert_eq!(
                this.row_heights(),
                vec![
                    COMMANDER_HEADING_H,
                    COMMANDER_ROW_H,
                    COMMANDER_ROW_H,
                    COMMANDER_ROW_H,
                    COMMANDER_HEADING_H + COMMANDER_GROUP_GAP,
                    COMMANDER_ROW_H,
                ]
            );
        });
    }

    /// A palette open in a real window with `count` commands under one heading, counting every
    /// row mark built.
    fn open_palette(
        cx: &mut TestAppContext,
        count: usize,
    ) -> (
        CommanderSlot,
        Rc<std::cell::Cell<usize>>,
        gpui::VisualTestContext,
    ) {
        use crate::test_support::open;
        let built = Rc::new(std::cell::Cell::new(0_usize));
        let items: Vec<CommanderItem> = (0..count)
            .map(|index| {
                let built = built.clone();
                CommanderItem::new(format!("item.{index}"), format!("Item {index}")).mark(
                    move |_, _| {
                        built.set(built.get() + 1);
                        div().w(px(20.0)).h(px(20.0)).into_any_element()
                    },
                )
            })
            .collect();
        let slot: CommanderSlot = Rc::default();
        let made = slot.clone();
        let visual = open(cx, gpui::size(900.0, 700.0), move |window, cx| {
            let commander = made
                .borrow_mut()
                .get_or_insert_with(|| {
                    cx.new(|cx| {
                        let mut commander = Commander::new(window, cx);
                        commander
                            .set_groups(vec![CommanderGroup::new("All").items(items.clone())], cx);
                        commander.set_open(true, window, cx);
                        commander
                    })
                })
                .clone();
            commander.into_any_element()
        });
        (slot, built, visual)
    }

    /// A palette in a real window drawing `groups`, open, with its entity filled in once.
    fn open_groups(
        cx: &mut TestAppContext,
        groups: Vec<CommanderGroup>,
    ) -> (CommanderSlot, gpui::VisualTestContext) {
        use crate::test_support::open;
        let slot: CommanderSlot = Rc::default();
        let made = slot.clone();
        let visual = open(cx, gpui::size(900.0, 700.0), move |window, cx| {
            made.borrow_mut()
                .get_or_insert_with(|| {
                    cx.new(|cx| {
                        let mut commander = Commander::new(window, cx);
                        commander.set_groups(groups.clone(), cx);
                        commander.set_open(true, window, cx);
                        commander
                    })
                })
                .clone()
                .into_any_element()
        });
        (slot, visual)
    }

    #[gpui::test]
    fn a_later_groups_heading_keeps_its_gap_in_the_list(cx: &mut TestAppContext) {
        use crate::test_support::draw;
        let (slot, mut visual) = open_groups(cx, groups());
        draw(&mut visual);
        let commander = slot.borrow().clone().unwrap();
        commander.read_with(&visual, |commander, _| {
            // The rows: Navigation's heading, its three commands, Actions' heading, Reload.
            let last_item = commander.list.bounds_for_item(3).expect("Offline");
            let heading = commander.list.bounds_for_item(4).expect("Actions' heading");
            let reload = commander.list.bounds_for_item(5).expect("Reload");
            assert!(
                (f32::from(heading.size.height) - (COMMANDER_HEADING_H + COMMANDER_GROUP_GAP))
                    .abs()
                    < 0.5,
                "a later group's heading keeps its gap above it"
            );
            assert_eq!(
                heading.top(),
                reload.top() - px(COMMANDER_HEADING_H + COMMANDER_GROUP_GAP)
            );
            assert_eq!(last_item.top(), heading.top() - px(COMMANDER_ROW_H));
        });
    }

    #[gpui::test]
    fn hovering_a_listed_row_lights_it(cx: &mut TestAppContext) {
        use crate::test_support::draw;
        let (slot, mut visual) = open_groups(cx, groups());
        let commander = slot.borrow().clone().unwrap();
        let spend = commander.read_with(&visual, |commander, _| {
            commander.list.bounds_for_item(2).expect("Spend").center()
        });
        visual.simulate_mouse_move(spend, None, gpui::Modifiers::default());
        draw(&mut visual);
        commander.read_with(&visual, |commander, _| {
            assert_eq!(
                commander.selected_id().map(|id| id.as_ref()),
                Some("nav.spend")
            );
        });
    }

    #[gpui::test]
    fn a_long_palette_builds_only_the_rows_in_view(cx: &mut TestAppContext) {
        use crate::test_support::draw;
        let (slot, built, mut visual) = open_palette(cx, 400);
        draw(&mut visual);
        built.set(0);
        draw(&mut visual);
        let built = built.get();
        assert!(
            built > 0 && built < 80,
            "built {built} row marks for 400 commands"
        );
        let commander = slot.borrow().clone().unwrap();
        commander.read_with(&visual, |commander, _| {
            let count = commander.list.item_count();
            assert_eq!(count, 401, "a heading and every command");
            assert!(
                commander.list.bounds_for_item(count - 1).is_none(),
                "the last row is never laid out while it is out of view"
            );
            // The list lays the rows out at the heights `row_y` steps through, so the
            // highlight, the reveal, and the rows share one geometry.
            let heading = commander.list.bounds_for_item(0).expect("the heading");
            let first = commander
                .list
                .bounds_for_item(1)
                .expect("the first command");
            assert!((f32::from(heading.size.height) - COMMANDER_HEADING_H).abs() < 0.5);
            assert!((f32::from(first.size.height) - COMMANDER_ROW_H).abs() < 0.5);
            assert!((f32::from(first.top() - heading.top()) - COMMANDER_HEADING_H).abs() < 0.5);
        });
    }

    #[gpui::test]
    fn moving_past_the_view_scrolls_the_highlight_into_view(cx: &mut TestAppContext) {
        use crate::test_support::draw;
        let (slot, _built, mut visual) = open_palette(cx, 300);
        draw(&mut visual);
        let commander = slot.borrow().clone().unwrap();
        commander.update(&mut visual, |commander, cx| {
            commander.selected = Some(120);
            commander.reveal = true;
            cx.notify();
        });
        draw(&mut visual);
        commander.read_with(&visual, |commander, _| {
            let top = commander.list.logical_scroll_top();
            assert!(top.item_ix > 90, "scrolled to row {}", top.item_ix);
            let (scrolled, _, view) = commander.list_extent();
            let y = commander.row_y(120) - scrolled;
            assert!(view > 1.0, "the results were laid out");
            assert!(
                y >= 0.0 && y + COMMANDER_ROW_H <= view,
                "the lit row at {y} sits outside the {view} view"
            );
        });
    }
}
