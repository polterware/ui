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
//! GPUI clips to rectangles, so the results stop short of the frame's rounded bottom corners
//! and fade out above them instead of running under the corners.

use std::rc::Rc;

use gpui::{
    AnyElement, App, Context, CursorStyle, Div, ElementId, Entity, EventEmitter, FocusHandle,
    Focusable, FontWeight, IntoElement, KeyBinding, MouseButton, Render, ScrollHandle,
    SharedString, Stateful, Window, actions, canvas, div, linear_color_stop, linear_gradient,
    point, prelude::*, px,
};

use crate::Handler;
use crate::icon_button::icon_button;
use crate::icon_tile::{IconTile, TileTone};
use crate::icons::Icon;
use crate::material::{Edge, drop, edge, float_drop};
use crate::motion::{Animator, Spring, SpringTier, SpringValue};
use crate::scroll::{GUTTER, ScrollAxis, ScrollGeometry, scrollbar};
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
    scroll: ScrollHandle,
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
        cx.observe_in(&input, window, |this, _, _, cx| this.on_query_changed(cx))
            .detach();
        Self {
            focus: cx.focus_handle(),
            input,
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
            scroll: ScrollHandle::new(),
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
            self.applied_query.clear();
            self.matched_ids.clear();
            self.selected = None;
            self.update_matches(cx);
            self.scroll.set_offset(point(px(0.0), px(0.0)));
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
                && field.contains_focused(window, cx)
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
        self.rows = rows;
        if ids != self.matched_ids {
            self.matched_ids = ids;
            self.selected = self.first_enabled();
            self.scroll.set_offset(point(px(0.0), px(0.0)));
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

    fn confirm_selected(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.selected {
            self.confirm(index, cx);
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

    /// Scrolls the least that shows row `y` whole, clear of the fades' insets.
    fn reveal_row(&self, y: f32) {
        let view = f32::from(self.scroll.bounds().size.height);
        if view < 1.0 {
            return;
        }
        let top = -f32::from(self.scroll.offset().y);
        let first = y - COMMANDER_LIST_TOP;
        let last = y + COMMANDER_ROW_H + COMMANDER_LIST_BOTTOM;
        if first < top {
            self.scroll.set_offset(point(px(0.0), px(-first.max(0.0))));
        } else if last > top + view {
            self.scroll.set_offset(point(px(0.0), px(-(last - view))));
        }
    }

    fn results(
        &mut self,
        overflows: bool,
        theme: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let gutter = if overflows { GUTTER } else { 0.0 };
        let mut column = div()
            .relative()
            .w_full()
            .pt(px(COMMANDER_LIST_TOP))
            .pb(px(COMMANDER_LIST_BOTTOM - CORNER_CLEAR))
            .pl(px(LIST_X))
            .pr(px(LIST_X + gutter));
        if self.matched.is_empty() {
            return column.child(
                div()
                    .w_full()
                    .py(px(theme.space_4))
                    .text_center()
                    .text_size(px(theme.font_size_sm))
                    .text_color(theme.color_text_muted)
                    .child(self.empty_label.clone()),
            );
        }
        if self.selected.is_some() {
            let inset = self.press.value().clamp(0.0, 1.0);
            column = column.child(
                div()
                    .absolute()
                    .left(px(LIST_X + inset))
                    .right(px(LIST_X + gutter + inset))
                    .top(px(self.indicator.value() + inset))
                    .h(px(COMMANDER_ROW_H - inset * 2.0))
                    .rounded(px(theme.radius_item))
                    .bg(theme.color_surface_hover),
            );
        }
        let mut groups = 0;
        let rows = self.rows.clone();
        for row in rows {
            match row {
                Row::Heading(group) => {
                    let label = self.groups[group].label.clone().unwrap_or_default();
                    column = column.child(
                        div()
                            .when(groups > 0, |heading| heading.mt(px(COMMANDER_GROUP_GAP)))
                            .h(px(COMMANDER_HEADING_H))
                            .px(px(theme.space_2))
                            .flex()
                            .items_center()
                            .text_size(px(10.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.color_text_disabled)
                            .child(label.to_uppercase()),
                    );
                    groups += 1;
                }
                Row::Item(index) => {
                    let row = self.row(index, theme, window, cx);
                    column = column.child(row);
                }
            }
        }
        column
    }

    fn row(
        &mut self,
        index: usize,
        theme: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let item = self
            .item_at(index)
            .cloned()
            .unwrap_or_else(|| CommanderItem::new("", ""));
        let active = self.selected == Some(index);
        let press = self.press.value().clamp(0.0, 1.0);
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
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        if *hovered {
                            this.reveal = false;
                            this.select(index, cx);
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            this.reveal = false;
                            this.select(index, cx);
                            this.press.set_target(1.0);
                            cx.notify();
                        }),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.confirm(index, cx)))
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
                self.reveal_row(y);
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
        if self.open && !self.focus_handle(cx).contains_focused(window, cx) {
            self.input.read(cx).focus_handle(cx).focus(window);
        }

        let offset = -f32::from(self.scroll.offset().y);
        let max = f32::from(self.scroll.max_offset().height);
        let viewport = f32::from(self.scroll.bounds().size.height);
        let overflows = max > 0.5;
        let results = self.results(overflows, &theme, window, cx);
        let bar = overflows.then(|| {
            scrollbar(
                "commander-scrollbar",
                &self.scroll,
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
        let watch = self.scroll.clone();
        // The fades and the gutter read the scroll of the frame before; draw again once the
        // offset, the overflow, or the height has moved.
        let watcher = canvas(
            move |_, window, _| {
                let now = (
                    -f32::from(watch.offset().y),
                    f32::from(watch.max_offset().height),
                    f32::from(watch.bounds().size.height),
                );
                let last = seen.get();
                if (now.0 - last.0).abs() > 0.5
                    || (now.1 - last.1).abs() > 0.5
                    || (now.2 - last.2).abs() > 0.5
                {
                    seen.set(now);
                    window.refresh();
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
        let footer = self.footer.take();

        palette_frame(
            "commander-palette",
            commander_size(window),
            self.presence(),
            self.input.clone().into_any_element(),
            results.into_any_element(),
            &self.scroll,
            fades,
            bar,
            footer,
            on_close,
            &theme,
            cx,
        )
        .key_context("Commander")
        .track_focus(&self.focus)
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
        presence,
        input.into_any_element(),
        results.into_any_element(),
        scroll,
        Fades::default(),
        None,
        None,
        on_close,
        theme,
        cx,
    )
}

#[allow(clippy::too_many_arguments)]
fn palette_frame(
    id: impl Into<ElementId>,
    (width, height): (f32, f32),
    presence: f32,
    input: AnyElement,
    results: AnyElement,
    scroll: &ScrollHandle,
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
    div()
        .id(id)
        .occlude()
        .relative()
        .w(px(width))
        .h(px(height))
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
                .flex_1()
                .min_h(px(0.0))
                .mt(px(RESULTS_TOP))
                .when(!has_footer, |area| area.mb(px(CORNER_CLEAR)))
                .child(
                    div()
                        .id("commander-results")
                        .size_full()
                        .overflow_y_scroll()
                        .track_scroll(scroll)
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
        });
    }
}
