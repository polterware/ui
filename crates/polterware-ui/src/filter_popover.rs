//! `FilterPopover`, `FilterOptionsPanel`, and `ActiveFilterBar` (`filter-popover.tsx`,
//! `filter-options-panel.tsx`, `active-filter-bar.tsx`) over [`crate::filter`].
//!
//! The root menu, 260 wide, opens 6px under its trigger: a search header (`px-3 py-2.5`, 13px,
//! the F key chip when the page gives it the shortcut) over a bottom hairline, then the fields
//! as 33px `MenuItem` rows in 13px secondary text: an `OptionTile`, the label, and a muted
//! chevron. One hover highlight glides between the rows and one selected marker sits on the
//! field whose values are open. Hovering a field opens its values 4px beside its row after
//! 120ms, on the left when the right has no room, and leaving closes them after 180ms; a click
//! opens them at once with their search focused. Typing in the root search lists the matching
//! fields and then `Field › Value` shortcuts.
//!
//! The values panel, 240 wide, lists 32px rows in 12px: a 14px checkbox that stays hidden until
//! the row is hovered unless checked (a radio for a single field), then the option's tile and
//! label. A label click picks only that value and closes the menu; a checkbox edits the pill
//! the first check created and keeps the menu open.
//!
//! The bar is a recessed track (radius 14, 2px in, 8px from the page) of segmented pills,
//! `[icon Field] [operator] [value] [×]`, then `+` and Clear. The pill's value opens the same
//! values panel with the checked rows first and the field's label as the placeholder; a
//! negatable field's operator opens a menu of is / is not.
//!
//! Every surface enters like `popup-motion.ts`: a fade over the fast spring's duration and a
//! 4px slide from its anchor on the fast spring, and it leaves on the fast exit tween. The
//! menus' slight vertical scale is left out, as GPUI cannot scale an element. Each surface is
//! drawn deferred, at the window position its anchor recorded in the last frame.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AnyElement, App, Bounds, Context, Corner, Entity, EventEmitter, Focusable, FontWeight,
    IntoElement, MouseButton, MouseDownEvent, Pixels, Point, Render, SharedString, Window,
    anchored, canvas, deferred, div, point, prelude::*, px,
};

use super::button::{Button, ButtonKind, ButtonSize};
use super::ellipsis::ellipsis;
use super::icon_tile::IconTile;
use super::icons::{Icon, icon_colored};
use super::material::{self, Edge, face};
use super::pointer::{PointerTiming, track, track_with_hover, use_pointer_timed};
use super::text_input::TextInput;
use super::tooltip::tooltip;
use crate::filter::{
    FilterKind, FilterSpec, Filters, SearchRow, operator_label, operators, option_sections,
    search_rows, value_summary,
};
use crate::motion::{Curve, SpringTier, Tween};
use crate::theme::{Ease, Theme, fade, mix};
use crate::{Handler, ValueHandler};

const ROOT_W: f32 = 260.0;
const PANEL_W: f32 = 240.0;
/// `min-w-32` on the operator menu.
const OPERATOR_MIN_W: f32 = 104.0;
/// `Popover`'s gutter, the submenu's, and Ark Menu's default.
const GUTTER: f32 = 6.0;
const SUB_GUTTER: f32 = 4.0;
const MENU_GUTTER: f32 = 8.0;
/// The window's overflow padding, and `max-h-[calc(100vh-64px)]`.
const WINDOW_MARGIN: f32 = 8.0;
const PANEL_INSET_Y: f32 = 64.0;
const SLIDE: f32 = 4.0;
/// The search header: `px-3 py-2.5` around a 19.5px line, over a 1px hairline.
const SEARCH_PAD_X: f32 = 9.75;
const SEARCH_PAD_Y: f32 = 8.125;
const SEARCH_LINE: f32 = 19.5;
const HEADER_H: f32 = SEARCH_PAD_Y * 2.0 + SEARCH_LINE + 1.0;
/// `p-1.5` around a list.
const LIST_PAD: f32 = 4.875;
/// A `MenuItem`: `py-2` around a 20px tile; `px-2.5`, `gap-2`.
const FIELD_ROW_H: f32 = 33.0;
const FIELD_ROW_PAD_X: f32 = 8.125;
const FIELD_ROW_GAP: f32 = 6.5;
/// A value row: `6px 8px` around a 20px tile.
const OPTION_ROW_H: f32 = 32.0;
/// A Dropdown row: `py-2` around an 18px line, `gap-1` apart.
const MENU_ROW_H: f32 = 31.0;
const MENU_ROW_GAP: f32 = 3.25;
/// The bar's pills: 4px apart, each a 22.4px segment row inside 2px of frame and face.
const PILL_RADIUS: f32 = 11.0;
const TRACK_RADIUS: f32 = 14.0;
const OPEN_DELAY: Duration = Duration::from_millis(120);
const CLOSE_DELAY: Duration = Duration::from_millis(180);
/// Motion's `easeOut` and `easeIn`.
const EASE_OUT: Ease = Ease(0.0, 0.0, 0.58, 1.0);
const EASE_IN: Ease = Ease(0.42, 0.0, 1.0, 1.0);

/// One field a page filters by: the model's spec and the icon its tile and pill show.
#[derive(Clone, Debug)]
pub struct FilterField {
    pub spec: FilterSpec,
    pub icon: Option<Icon>,
}

impl FilterField {
    pub fn new(spec: FilterSpec, icon: Option<Icon>) -> Self {
        Self { spec, icon }
    }
}

/// The clauses changed, or a surface the bar draws opened or closed.
pub struct FiltersChanged;

/// One surface's presence: `popup-motion.ts`'s fade and slide.
struct Popup {
    open: bool,
    alpha: Tween,
    slide: Tween,
}

impl Popup {
    fn new() -> Self {
        Self {
            open: false,
            alpha: Tween::new(0.0),
            slide: Tween::new(0.0),
        }
    }

    fn set(&mut self, open: bool) {
        if self.open == open {
            return;
        }
        self.open = open;
        let fast = SpringTier::Fast;
        if open {
            self.alpha
                .animate_to(1.0, fast.duration(), Curve::Ease(EASE_OUT));
            self.slide.animate_to(1.0, fast.duration(), fast.curve());
        } else {
            self.alpha
                .animate_to(0.0, fast.exit_duration(), Curve::Ease(EASE_IN));
            self.slide
                .animate_to(0.0, fast.exit_duration(), Curve::Ease(EASE_IN));
        }
    }

    fn visible(&self) -> bool {
        self.open || self.alpha.is_animating() || self.alpha.value() > 0.001
    }

    fn animating(&self) -> bool {
        self.alpha.is_animating() || self.slide.is_animating()
    }

    /// How far the surface still sits toward its anchor.
    fn offset(&self) -> f32 {
        SLIDE * (1.0 - self.slide.value().clamp(0.0, 1.0))
    }
}

/// `FluidHoverHighlight` in a menu list: one fill that snaps onto the first row it lights,
/// slides between rows on the fast spring, and fades out on the fast exit.
struct Glide {
    index: Option<usize>,
    y: Tween,
    alpha: Tween,
}

impl Glide {
    fn new() -> Self {
        Self {
            index: None,
            y: Tween::new(0.0),
            alpha: Tween::new(0.0),
        }
    }

    fn to(&mut self, index: Option<usize>, pitch: f32) {
        if self.index == index {
            return;
        }
        let fast = SpringTier::Fast;
        match index {
            Some(index) => {
                let y = index as f32 * pitch;
                if self.alpha.value() < 0.01 {
                    self.y.snap(y);
                } else {
                    self.y.animate_to(y, fast.duration(), fast.curve());
                }
                self.alpha
                    .animate_to(1.0, fast.duration(), Curve::Ease(EASE_OUT));
            }
            None => self
                .alpha
                .animate_to(0.0, fast.exit_duration(), Curve::Ease(EASE_IN)),
        }
        self.index = index;
    }

    fn leave(&mut self, index: usize) {
        if self.index == Some(index) {
            self.to(None, 0.0);
        }
    }

    fn reset(&mut self) {
        self.index = None;
        self.alpha.snap(0.0);
    }

    fn animating(&self) -> bool {
        self.y.is_animating() || self.alpha.is_animating()
    }

    fn fill(&self, height: f32, color: gpui::Rgba, theme: &Theme) -> Option<AnyElement> {
        let alpha = self.alpha.value();
        (alpha > 0.001).then(|| {
            div()
                .absolute()
                .left(px(0.0))
                .right(px(0.0))
                .top(px(self.y.value()))
                .h(px(height))
                .rounded(px(theme.radius_item))
                .bg(fade(color, alpha))
                .into_any_element()
        })
    }
}

/// Where the anchors and surfaces sat in the last frame, in window coordinates.
#[derive(Default)]
struct Geometry {
    trigger: Option<Bounds<Pixels>>,
    add: Option<Bounds<Pixels>>,
    root: Option<Bounds<Pixels>>,
    sub: Option<Bounds<Pixels>>,
    values: Option<Bounds<Pixels>>,
    operator: Option<Bounds<Pixels>>,
    pills: HashMap<String, Bounds<Pixels>>,
    operator_segments: HashMap<String, Bounds<Pixels>>,
    value_segments: HashMap<String, Bounds<Pixels>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Anchor {
    /// The toolbar's icon button: `bottom-end`.
    Trigger,
    /// The bar's `+`: `bottom-start`.
    Add,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Surface {
    Root,
    Sub,
    Values,
    Operator,
}

pub struct FilterPopover {
    id: SharedString,
    label: SharedString,
    shortcut: bool,
    fields: Vec<FilterField>,
    filters: Filters,
    root: Popup,
    anchor: Anchor,
    root_search: Entity<TextInput>,
    root_hover: Glide,
    root_selected: Glide,
    sub: Popup,
    /// The field whose values are open and its row in the root list; it stays while the
    /// panel fades out.
    sub_field: Option<(usize, usize)>,
    sub_search: Entity<TextInput>,
    /// The clause the open values edit, so further checks edit the same pill.
    session: Option<String>,
    timer: u64,
    values: Popup,
    values_clause: Option<String>,
    values_search: Entity<TextInput>,
    /// The values checked when the pill's panel opened, listed first.
    pinned: Vec<String>,
    operator: Popup,
    operator_clause: Option<String>,
    operator_hover: Glide,
    focus_next: Option<Surface>,
    geometry: Rc<RefCell<Geometry>>,
}

impl EventEmitter<FiltersChanged> for FilterPopover {}

impl FilterPopover {
    /// `label` is the trigger's name ("Run filters"); `shortcut` gives this control the F key.
    pub fn new(
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        shortcut: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let root_search = Self::search_input("Add filter…".into(), Surface::Root, cx);
        let sub_search = Self::search_input("Filter…".into(), Surface::Sub, cx);
        let values_search = Self::search_input("Filter…".into(), Surface::Values, cx);
        Self {
            id: id.into(),
            label: label.into(),
            shortcut,
            fields: Vec::new(),
            filters: Filters::default(),
            root: Popup::new(),
            anchor: Anchor::Trigger,
            root_search,
            root_hover: Glide::new(),
            root_selected: Glide::new(),
            sub: Popup::new(),
            sub_field: None,
            sub_search,
            session: None,
            timer: 0,
            values: Popup::new(),
            values_clause: None,
            values_search,
            pinned: Vec::new(),
            operator: Popup::new(),
            operator_clause: None,
            operator_hover: Glide::new(),
            focus_next: None,
            geometry: Rc::new(RefCell::new(Geometry::default())),
        }
    }

    /// A bare search whose Enter and Escape reach this control.
    fn search_input(
        placeholder: SharedString,
        surface: Surface,
        cx: &mut Context<Self>,
    ) -> Entity<TextInput> {
        let submit = cx.weak_entity();
        let cancel = cx.weak_entity();
        let input = cx.new(|cx| {
            TextInput::new(cx)
                .bare()
                .placeholder(placeholder)
                .on_submit(move |_, window, cx| {
                    let _ = submit.update(cx, |this, cx| this.submit(surface, window, cx));
                })
                .on_cancel(move |_, cx| {
                    let _ = cancel.update(cx, |this, cx| this.escape(surface, cx));
                })
        });
        cx.observe(&input, move |this, _, cx| this.query_changed(surface, cx))
            .detach();
        input
    }

    pub fn filters(&self) -> &Filters {
        &self.filters
    }

    pub fn fields(&self) -> &[FilterField] {
        &self.fields
    }

    /// The fields for the page's current data. Clauses whose field is gone stay, as the bar
    /// only draws the ones it knows.
    pub fn set_fields(&mut self, fields: Vec<FilterField>) {
        self.fields = fields;
        if let Some((field, _)) = self.sub_field
            && field >= self.fields.len()
        {
            self.sub_field = None;
            self.sub.set(false);
        }
    }

    pub fn is_open(&self) -> bool {
        self.root.open || self.values.open || self.operator.open
    }

    /// The F shortcut: the root menu under the toolbar's trigger, when that trigger is drawn.
    pub fn open_from_shortcut(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.shortcut || self.geometry.borrow().trigger.is_none() {
            return false;
        }
        if !self.root.open {
            self.open_root(Anchor::Trigger, cx);
        }
        true
    }

    /// Closes every surface, as Escape on each would.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.close_values();
        self.close_operator();
        self.close_root(cx);
    }

    /// Adds a pill for `field` holding `values`, as picking them from the menu would.
    pub fn add_filter(&mut self, field: &str, values: &[String], cx: &mut Context<Self>) {
        let Some(index) = self.field_index(field) else {
            return;
        };
        let spec = self.fields[index].spec.clone();
        self.filters.add(&spec, values);
        self.changed(cx);
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.filters.clear();
        self.close_values();
        self.close_operator();
        self.changed(cx);
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        cx.emit(FiltersChanged);
        cx.notify();
    }

    fn open_root(&mut self, anchor: Anchor, cx: &mut Context<Self>) {
        self.close_values();
        self.close_operator();
        self.anchor = anchor;
        self.root.set(true);
        self.root_hover.reset();
        self.root_selected.reset();
        self.focus_next = Some(Surface::Root);
        self.root_search
            .update(cx, |input, cx| input.set_text("", cx));
        self.changed(cx);
    }

    fn toggle_root(&mut self, anchor: Anchor, cx: &mut Context<Self>) {
        if self.root.open && self.anchor == anchor {
            self.close_root(cx);
        } else {
            self.open_root(anchor, cx);
        }
    }

    /// `closeRoot`: the menu, its open values, the session, and the query.
    fn close_root(&mut self, cx: &mut Context<Self>) {
        self.timer += 1;
        self.root.set(false);
        self.sub.set(false);
        self.session = None;
        self.root_hover.to(None, FIELD_ROW_H);
        self.root_search
            .update(cx, |input, cx| input.set_text("", cx));
        self.changed(cx);
    }

    fn close_values(&mut self) {
        self.values.set(false);
    }

    fn close_operator(&mut self) {
        self.operator.set(false);
    }

    fn query(&self, surface: Surface, cx: &App) -> String {
        let input = match surface {
            Surface::Root => &self.root_search,
            Surface::Sub => &self.sub_search,
            Surface::Values => &self.values_search,
            Surface::Operator => return String::new(),
        };
        input.read(cx).text().to_string()
    }

    fn query_changed(&mut self, surface: Surface, cx: &mut Context<Self>) {
        if surface == Surface::Root && !self.query(Surface::Root, cx).is_empty() {
            // `onInput`: typing closes the open values.
            self.timer += 1;
            self.sub.set(false);
        }
        cx.notify();
    }

    fn escape(&mut self, surface: Surface, cx: &mut Context<Self>) {
        match surface {
            Surface::Root | Surface::Sub => self.close_root(cx),
            Surface::Values => {
                self.close_values();
                self.changed(cx);
            }
            Surface::Operator => {
                self.close_operator();
                self.changed(cx);
            }
        }
    }

    /// Enter in a search: the root's first result, or the values' first option.
    fn submit(&mut self, surface: Surface, window: &mut Window, cx: &mut Context<Self>) {
        match surface {
            Surface::Root => {
                let query = self.query(Surface::Root, cx);
                let specs: Vec<&FilterSpec> = self.fields.iter().map(|field| &field.spec).collect();
                match search_rows(&specs, &query).first() {
                    Some(SearchRow::Field(field)) => self.open_field(*field, 0, true, cx),
                    Some(SearchRow::Value(field, option)) => {
                        let id = self.fields[*field].spec.options[*option].id.clone();
                        self.add_value(*field, &id, cx);
                    }
                    None => {}
                }
            }
            Surface::Sub => {
                let Some((field, _)) = self.sub_field else {
                    return;
                };
                let query = self.query(Surface::Sub, cx);
                let first = option_sections(&self.fields[field].spec, &query, &[], false)
                    .into_iter()
                    .flatten()
                    .next()
                    .map(|option| option.id.clone());
                if let Some(id) = first {
                    self.select_in_submenu(field, &id, cx);
                }
            }
            Surface::Values => {
                let Some((field, clause)) = self.values_target() else {
                    return;
                };
                let query = self.query(Surface::Values, cx);
                let first = option_sections(&self.fields[field].spec, &query, &self.pinned, true)
                    .into_iter()
                    .flatten()
                    .next()
                    .map(|option| option.id.clone());
                if let Some(id) = first {
                    self.filters.set_values(&clause, vec![id]);
                    self.close_values();
                    self.changed(cx);
                }
            }
            Surface::Operator => {}
        }
        let _ = window;
    }

    fn open_field(&mut self, field: usize, row: usize, focus: bool, cx: &mut Context<Self>) {
        self.timer += 1;
        if self.sub_field.map(|(open, _)| open) != Some(field) || !self.sub.open {
            self.session = None;
            self.sub_search
                .update(cx, |input, cx| input.set_text("", cx));
        }
        self.sub_field = Some((field, row));
        self.sub.set(true);
        if focus {
            self.focus_next = Some(Surface::Sub);
        }
        cx.notify();
    }

    fn schedule(&mut self, open: Option<(usize, usize)>, cx: &mut Context<Self>) {
        self.timer += 1;
        let timer = self.timer;
        let delay = if open.is_some() {
            OPEN_DELAY
        } else {
            CLOSE_DELAY
        };
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update(cx, |this, cx| {
                if this.timer != timer || !this.root.open {
                    return;
                }
                match open {
                    Some((field, row)) => this.open_field(field, row, false, cx),
                    None => {
                        this.sub.set(false);
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    fn hover_field(&mut self, field: usize, row: usize, hovered: bool, cx: &mut Context<Self>) {
        if hovered {
            self.root_hover.to(Some(row), FIELD_ROW_H);
            self.schedule(Some((field, row)), cx);
        } else {
            self.root_hover.leave(row);
            self.schedule(None, cx);
        }
        cx.notify();
    }

    fn hover_row(&mut self, row: usize, hovered: bool, cx: &mut Context<Self>) {
        if hovered {
            self.root_hover.to(Some(row), FIELD_ROW_H);
        } else {
            self.root_hover.leave(row);
        }
        cx.notify();
    }

    fn hover_sub(&mut self, hovered: bool, cx: &mut Context<Self>) {
        if hovered {
            // `clearTimers`: the pointer reached the values.
            self.timer += 1;
        } else if self.sub.open {
            self.schedule(None, cx);
        }
    }

    /// The clause the submenu edits: its session clause, or the field's only pill.
    fn edited_clause(&self, field: usize) -> Option<String> {
        let spec = &self.fields[field].spec;
        self.session
            .as_ref()
            .filter(|id| self.filters.clause(id).is_some())
            .cloned()
            .or_else(|| {
                self.filters
                    .single_clause(spec)
                    .map(|clause| clause.id.clone())
            })
    }

    /// `addValue`: a root shortcut adds just this value and closes.
    fn add_value(&mut self, field: usize, value: &str, cx: &mut Context<Self>) {
        let spec = self.fields[field].spec.clone();
        match self
            .filters
            .single_clause(&spec)
            .map(|clause| clause.id.clone())
        {
            Some(existing) => self.filters.set_values(&existing, vec![value.to_string()]),
            None => {
                self.filters.add(&spec, &[value.to_string()]);
            }
        }
        self.close_root(cx);
    }

    /// A label click in the submenu: only this value, then close.
    fn select_in_submenu(&mut self, field: usize, value: &str, cx: &mut Context<Self>) {
        let spec = self.fields[field].spec.clone();
        match self.edited_clause(field) {
            Some(clause) => self.filters.set_values(&clause, vec![value.to_string()]),
            None => {
                self.filters.add(&spec, &[value.to_string()]);
            }
        }
        self.close_root(cx);
    }

    /// A checkbox in the submenu: edit the session's pill and stay open.
    fn toggle_in_submenu(
        &mut self,
        field: usize,
        value: &str,
        checked: bool,
        cx: &mut Context<Self>,
    ) {
        let spec = self.fields[field].spec.clone();
        match self.edited_clause(field) {
            Some(clause) => {
                self.session = Some(clause.clone());
                self.filters.toggle(&clause, &spec, value, checked);
            }
            None if checked => {
                let id = self.filters.add(&spec, &[value.to_string()]);
                self.session = Some(id);
            }
            None => return,
        }
        self.changed(cx);
    }

    fn field_index(&self, field: &str) -> Option<usize> {
        self.fields.iter().position(|item| item.spec.id == field)
    }

    /// The field and clause of the open pill's values, while the clause still exists.
    fn values_target(&self) -> Option<(usize, String)> {
        let id = self.values_clause.as_ref()?;
        let clause = self.filters.clause(id)?;
        Some((self.field_index(&clause.field)?, id.clone()))
    }

    fn toggle_values(&mut self, clause: &str, cx: &mut Context<Self>) {
        if self.values.open && self.values_clause.as_deref() == Some(clause) {
            self.close_values();
        } else {
            self.close_root(cx);
            self.close_operator();
            self.pinned = self
                .filters
                .clause(clause)
                .map(|clause| clause.values.clone())
                .unwrap_or_default();
            self.values_clause = Some(clause.to_string());
            // The pill's search names its field.
            let label = self
                .values_target()
                .map(|(field, _)| SharedString::from(self.fields[field].spec.label.clone()))
                .unwrap_or_else(|| "Filter…".into());
            self.values_search = Self::search_input(label, Surface::Values, cx);
            self.values.set(true);
            self.focus_next = Some(Surface::Values);
        }
        self.changed(cx);
    }

    fn toggle_operator(&mut self, clause: &str, cx: &mut Context<Self>) {
        if self.operator.open && self.operator_clause.as_deref() == Some(clause) {
            self.close_operator();
        } else {
            self.close_root(cx);
            self.close_values();
            self.operator_clause = Some(clause.to_string());
            self.operator_hover.reset();
            self.operator.set(true);
        }
        self.changed(cx);
    }

    fn remove(&mut self, clause: &str, cx: &mut Context<Self>) {
        self.filters.remove(clause);
        if self.values_clause.as_deref() == Some(clause) {
            self.close_values();
        }
        if self.operator_clause.as_deref() == Some(clause) {
            self.close_operator();
        }
        self.changed(cx);
    }

    /// A press outside a surface closes it, unless it lands on its anchor or a surface that
    /// belongs with it (the root and its values hold each other open).
    fn press_outside(&mut self, surface: Surface, position: Point<Pixels>, cx: &mut Context<Self>) {
        let inside = |bounds: Option<Bounds<Pixels>>| bounds.is_some_and(|b| b.contains(&position));
        let keep = {
            let geometry = self.geometry.borrow();
            match surface {
                Surface::Root | Surface::Sub => {
                    inside(geometry.root)
                        || (self.sub.open && inside(geometry.sub))
                        || inside(geometry.trigger)
                        || inside(geometry.add)
                }
                Surface::Values => {
                    inside(geometry.values)
                        || self
                            .values_clause
                            .as_ref()
                            .is_some_and(|id| inside(geometry.value_segments.get(id).copied()))
                }
                Surface::Operator => {
                    inside(geometry.operator)
                        || self
                            .operator_clause
                            .as_ref()
                            .is_some_and(|id| inside(geometry.operator_segments.get(id).copied()))
                }
            }
        };
        if keep {
            return;
        }
        match surface {
            Surface::Root | Surface::Sub if self.root.open => self.close_root(cx),
            Surface::Values if self.values.open => {
                self.close_values();
                self.changed(cx);
            }
            Surface::Operator if self.operator.open => {
                self.close_operator();
                self.changed(cx);
            }
            _ => {}
        }
    }

    fn animating(&self) -> bool {
        self.root.animating()
            || self.sub.animating()
            || self.values.animating()
            || self.operator.animating()
            || self.root_hover.animating()
            || self.root_selected.animating()
            || self.operator_hover.animating()
    }
}

/// Records an element's window bounds into the shared geometry.
fn probe(
    geometry: &Rc<RefCell<Geometry>>,
    slot: impl Fn(&mut Geometry, Bounds<Pixels>) + 'static,
) -> impl IntoElement {
    let geometry = geometry.clone();
    canvas(
        move |bounds, _, _| slot(&mut geometry.borrow_mut(), bounds),
        |_, _, _, _| {},
    )
    .absolute()
    .top(px(0.0))
    .left(px(0.0))
    .size_full()
}

/// `.pw-filter-popover-kbd`: 1px 5px inside a 1px hairline, radius 4, 10px muted.
fn key_chip(label: &'static str, theme: &Theme) -> impl IntoElement {
    div()
        .flex_none()
        .px(px(5.0))
        .py(px(1.0))
        .border_1()
        .border_color(theme.color_border_subtle)
        .rounded(px(4.0))
        .text_size(px(10.0))
        .line_height(px(14.0))
        .text_color(theme.color_text_muted)
        .child(label)
}

/// A search header: `px-3 py-2.5` over the bottom hairline.
fn search_header(
    input: &Entity<TextInput>,
    key: Option<&'static str>,
    theme: &Theme,
) -> impl IntoElement {
    div()
        .flex_none()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(9.75))
        .px(px(SEARCH_PAD_X))
        .py(px(SEARCH_PAD_Y))
        .border_b_1()
        .border_color(theme.color_border_subtle)
        .child(div().flex_1().min_w(px(0.0)).child(input.clone()))
        .children(key.map(|key| key_chip(key, theme)))
}

/// A list's empty line: `px-2 py-3`, centered, 12px muted.
fn empty_line(text: &'static str, theme: &Theme) -> impl IntoElement {
    div()
        .px(px(6.5))
        .py(px(9.75))
        .text_center()
        .text_size(px(12.0))
        .line_height(px(18.0))
        .text_color(theme.color_text_muted)
        .child(text)
}

/// `.pw-checkbox__control` at 14px, radius 4: the field fill unchecked, the control face with
/// a 12px check when checked.
fn checkbox_mark(checked: bool, theme: &Theme) -> impl IntoElement {
    let fill = if checked {
        face(theme.material_face_top, theme.material_face_bottom)
    } else {
        face(theme.field_fill_top, theme.field_fill_bottom)
    };
    div()
        .relative()
        .size(px(14.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.0))
        .bg(fill)
        .shadow(material::control_drop(theme))
        .when(checked, |mark| {
            mark.child(icon_colored(Icon::Check, 12.0, theme.color_text_primary))
        })
        .child(material::edge(Edge::Strong, 4.0, theme))
}

/// `RadioMark`: a 16px round field control; checked, the control face and a 6px dot.
fn radio_mark(checked: bool, theme: &Theme) -> impl IntoElement {
    let fill = if checked {
        face(theme.material_face_top, theme.material_face_bottom)
    } else {
        face(theme.field_fill_top, theme.field_fill_bottom)
    };
    div()
        .relative()
        .size(px(16.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(fill)
        .shadow(material::control_drop(theme))
        .when(checked, |mark| {
            mark.child(
                div()
                    .size(px(6.0))
                    .rounded_full()
                    .bg(theme.color_text_primary),
            )
        })
        .child(material::edge(Edge::Strong, 8.0, theme))
}

/// The floating surface every panel shares (`surface-panel-float`): opaque, radius 16, the
/// float drop and the quiet edge. Call after its children, so the edge paints on top.
fn surface_panel(element: gpui::Stateful<gpui::Div>, theme: &Theme) -> gpui::Stateful<gpui::Div> {
    material::floating_panel(
        element.bg(theme.color_surface_subtle),
        theme.radius_menu,
        theme,
    )
}

impl FilterPopover {
    fn take_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(surface) = self.focus_next.take() else {
            return;
        };
        let input = match surface {
            Surface::Root => &self.root_search,
            Surface::Sub => &self.sub_search,
            Surface::Values => &self.values_search,
            Surface::Operator => return,
        };
        input.read(cx).focus_handle(cx).focus(window);
    }

    /// One root row: an `OptionTile`, its text, and the chevron of a field.
    #[allow(clippy::too_many_arguments)]
    fn menu_row(
        &self,
        key: String,
        row: usize,
        tile: IconTile,
        text: AnyElement,
        chevron: bool,
        selected: bool,
        on_hover: ValueHandler<bool>,
        on_click: Handler,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = *Theme::global(cx);
        let id = SharedString::from(format!("{}-{key}", self.id));
        let pointer = use_pointer_timed(id.clone(), PointerTiming::item(&theme), window, cx);
        let hover = pointer.read(cx).hover_color();
        let ink = if selected {
            theme.color_text_primary
        } else {
            mix(theme.color_text_secondary, theme.color_text_primary, hover)
        };
        let element = div()
            .id(id)
            .relative()
            .h(px(FIELD_ROW_H))
            .px(px(FIELD_ROW_PAD_X))
            .flex()
            .items_center()
            .gap(px(FIELD_ROW_GAP))
            .rounded(px(theme.radius_item))
            .cursor_pointer()
            .text_size(px(13.0))
            .line_height(px(19.5))
            .text_color(ink)
            .child(tile)
            .child(div().flex_1().min_w(px(0.0)).child(text))
            .when(chevron, |element| {
                element.child(icon_colored(
                    Icon::ChevronRight,
                    14.0,
                    theme.color_text_muted,
                ))
            })
            .on_click(move |_, window, cx| on_click(window, cx));
        let _ = row;
        track_with_hover(element, &pointer, true, Some(on_hover)).into_any_element()
    }

    fn root_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.root.visible() {
            return None;
        }
        let theme = *Theme::global(cx);
        let anchor = {
            let geometry = self.geometry.borrow();
            match self.anchor {
                Anchor::Trigger => geometry.trigger,
                Anchor::Add => geometry.add,
            }
        }?;
        let (corner, position) = match self.anchor {
            Anchor::Trigger => (
                Corner::TopRight,
                point(anchor.right(), anchor.bottom() + px(GUTTER)),
            ),
            Anchor::Add => (
                Corner::TopLeft,
                point(anchor.left(), anchor.bottom() + px(GUTTER)),
            ),
        };
        let entity = cx.entity();
        let query = self.query(Surface::Root, cx);
        let specs: Vec<&FilterSpec> = self.fields.iter().map(|field| &field.spec).collect();
        let open_field = self
            .sub_field
            .filter(|_| self.sub.open)
            .map(|(field, _)| field);

        // Each row: (field, option) and whether it is a field row.
        let rows: Vec<SearchRow> = if query.trim().is_empty() {
            (0..self.fields.len()).map(SearchRow::Field).collect()
        } else {
            search_rows(&specs, &query)
        };
        let searching = !query.trim().is_empty();
        let selected_row = rows.iter().enumerate().find_map(|(index, row)| match row {
            SearchRow::Field(field) if Some(*field) == open_field => Some(index),
            SearchRow::Value(..) if index == 0 => Some(index),
            _ => None,
        });
        self.root_selected.to(selected_row, FIELD_ROW_H);

        let mut list = div().relative().flex().flex_col();
        if let Some(fill) = self
            .root_hover
            .fill(FIELD_ROW_H, theme.color_surface_hover, &theme)
        {
            list = list.child(fill);
        }
        if let Some(fill) =
            self.root_selected
                .fill(FIELD_ROW_H, theme.color_surface_selected, &theme)
        {
            list = list.child(fill);
        }
        for (index, row) in rows.iter().enumerate() {
            let element = match *row {
                SearchRow::Field(field) => {
                    let spec = &self.fields[field].spec;
                    let tile = IconTile::option(self.fields[field].icon, &spec.label);
                    let hover_entity = entity.clone();
                    let click_entity = entity.clone();
                    self.menu_row(
                        format!("field-{field}"),
                        index,
                        tile,
                        ellipsis(SharedString::from(spec.label.clone())).into_any_element(),
                        true,
                        Some(field) == open_field,
                        Rc::new(move |hovered, _, cx| {
                            hover_entity
                                .update(cx, |this, cx| this.hover_field(field, index, hovered, cx));
                        }),
                        Rc::new(move |_, cx| {
                            click_entity
                                .update(cx, |this, cx| this.open_field(field, index, true, cx));
                        }),
                        window,
                        cx,
                    )
                }
                SearchRow::Value(field, option) => {
                    let spec = &self.fields[field].spec;
                    let item = &spec.options[option];
                    let tile = IconTile::option(self.fields[field].icon, &item.label);
                    let text = div()
                        .flex()
                        .min_w(px(0.0))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .child(
                            div()
                                .flex_none()
                                .text_color(theme.color_text_muted)
                                .child(SharedString::from(format!("{} › ", spec.label))),
                        )
                        .child(
                            div()
                                .min_w(px(0.0))
                                .child(ellipsis(SharedString::from(item.label.clone()))),
                        )
                        .into_any_element();
                    let value = item.id.clone();
                    let hover_entity = entity.clone();
                    let click_entity = entity.clone();
                    self.menu_row(
                        format!("value-{field}-{option}"),
                        index,
                        tile,
                        text,
                        false,
                        index == 0,
                        Rc::new(move |hovered, _, cx| {
                            hover_entity.update(cx, |this, cx| this.hover_row(index, hovered, cx));
                        }),
                        Rc::new(move |_, cx| {
                            click_entity.update(cx, |this, cx| this.add_value(field, &value, cx));
                        }),
                        window,
                        cx,
                    )
                }
            };
            list = list.child(element);
        }
        let body = if rows.is_empty() {
            empty_line(
                if searching {
                    "No filters found."
                } else {
                    "No filters available."
                },
                &theme,
            )
            .into_any_element()
        } else {
            div().p(px(LIST_PAD)).child(list).into_any_element()
        };
        let alpha = self.root.alpha.value().clamp(0.0, 1.0);
        let offset = self.root.offset();
        let geometry = self.geometry.clone();
        let viewport = f32::from(window.viewport_size().height);
        let panel = surface_panel(
            div()
                .id(SharedString::from(format!("{}-root", self.id)))
                .relative()
                .top(px(-offset))
                .w(px(ROOT_W))
                .max_h(px((viewport - PANEL_INSET_Y).max(120.0)))
                .flex()
                .flex_col()
                .occlude()
                .opacity(alpha)
                .text_color(theme.color_text_primary)
                .on_mouse_down_out(cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    this.press_outside(Surface::Root, event.position, cx)
                }))
                .child(search_header(
                    &self.root_search,
                    self.shortcut.then_some("F"),
                    &theme,
                ))
                .child(
                    div()
                        .id(SharedString::from(format!("{}-root-list", self.id)))
                        .min_h(px(0.0))
                        .overflow_y_scroll()
                        .child(body),
                )
                .child(probe(&geometry, |geometry, bounds| {
                    geometry.root = Some(bounds)
                })),
            &theme,
        );
        Some(
            deferred(
                anchored()
                    .position(position)
                    .anchor(corner)
                    .snap_to_window_with_margin(px(WINDOW_MARGIN))
                    .child(panel),
            )
            .with_priority(6)
            .into_any_element(),
        )
    }

    /// `FilterOptionsPanel` for the submenu or a pill's values.
    #[allow(clippy::too_many_arguments)]
    fn options_panel(
        &self,
        surface: Surface,
        field: usize,
        values: &[String],
        checked_first: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let theme = *Theme::global(cx);
        let entity = cx.entity();
        let search = match surface {
            Surface::Sub => &self.sub_search,
            _ => &self.values_search,
        };
        let query = self.query(surface, cx);
        let spec = &self.fields[field].spec;
        let multi = spec.kind == FilterKind::Multi;
        let pinned: &[String] = if checked_first { &self.pinned } else { &[] };
        let sections = option_sections(spec, &query, pinned, checked_first);
        let first = sections
            .first()
            .and_then(|section| section.first())
            .map(|option| option.id.clone());
        let mut list = div().p(px(LIST_PAD)).flex().flex_col();
        if sections.iter().all(|section| section.is_empty()) {
            list = div().child(empty_line("No matches.", &theme));
        }
        for (section_index, section) in sections.iter().enumerate() {
            if section_index > 0 {
                list = list.child(div().my(px(3.25)).h(px(1.0)).bg(theme.color_border_subtle));
            }
            for option in section {
                let checked = values.contains(&option.id);
                let active = !query.trim().is_empty() && first.as_deref() == Some(&option.id);
                let id = SharedString::from(format!(
                    "{}-{surface:?}-{}-{}",
                    self.id, spec.id, option.id
                ));
                let pointer =
                    use_pointer_timed(id.clone(), PointerTiming::item(&theme), window, cx);
                let (hovered, hover, hover_color) = {
                    let state = pointer.read(cx);
                    (state.hovered(), state.hover(), state.hover_color())
                };
                let fill = if active { 1.0 } else { hover };
                let ink = if checked || active {
                    theme.color_text_primary
                } else {
                    mix(
                        theme.color_text_secondary,
                        theme.color_text_primary,
                        hover_color,
                    )
                };
                let value = option.id.clone();
                let mut row = div()
                    .id(id.clone())
                    .relative()
                    .h(px(OPTION_ROW_H))
                    .flex()
                    .items_center()
                    .rounded(px(theme.radius_item))
                    .bg(fade(theme.color_surface_hover, fill))
                    .text_color(ink);
                if multi {
                    let toggle_entity = entity.clone();
                    let toggle_value = value.clone();
                    row = row.child(
                        div()
                            .id(SharedString::from(format!("{id}-check")))
                            .flex_none()
                            .pl(px(8.0))
                            .pr(px(2.0))
                            .cursor_pointer()
                            .on_click(move |_, _, cx| {
                                let value = toggle_value.clone();
                                toggle_entity.update(cx, |this, cx| {
                                    this.toggle_option(surface, field, &value, !checked, cx)
                                });
                            })
                            .child(if checked || hovered {
                                checkbox_mark(checked, &theme).into_any_element()
                            } else {
                                div().size(px(14.0)).into_any_element()
                            }),
                    );
                }
                let select_entity = entity.clone();
                let body = div()
                    .id(SharedString::from(format!("{id}-body")))
                    .flex_1()
                    .min_w(px(0.0))
                    .h_full()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(8.0))
                    .rounded(px(theme.radius_item))
                    .cursor_pointer()
                    .text_size(px(12.0))
                    .line_height(px(18.0))
                    .on_click(move |_, _, cx| {
                        let value = value.clone();
                        select_entity.update(cx, |this, cx| {
                            this.select_option(surface, field, &value, cx)
                        });
                    })
                    .when(!multi, |body| body.child(radio_mark(checked, &theme)))
                    .child(IconTile::option(None, &option.label))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .when(!multi, |label| label.flex_1())
                            .child(ellipsis(SharedString::from(option.label.clone()))),
                    )
                    .children(option.trailing.clone().map(|trailing| {
                        div()
                            .flex_none()
                            .when(multi, |trailing| trailing.flex_1())
                            .text_size(px(11.0))
                            .text_color(theme.color_text_muted)
                            .child(SharedString::from(trailing))
                    }));
                row = row.child(body);
                list = list.child(track(row, &pointer, true));
            }
        }
        let viewport = f32::from(window.viewport_size().height);
        div()
            .relative()
            .w(px(PANEL_W))
            .max_h(px((viewport - PANEL_INSET_Y).max(120.0)))
            .flex()
            .flex_col()
            .occlude()
            .text_color(theme.color_text_primary)
            .child(search_header(search, None, &theme))
            .child(
                div()
                    .id(SharedString::from(format!("{}-{surface:?}-list", self.id)))
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .child(list),
            )
    }

    fn toggle_option(
        &mut self,
        surface: Surface,
        field: usize,
        value: &str,
        checked: bool,
        cx: &mut Context<Self>,
    ) {
        match surface {
            Surface::Sub => self.toggle_in_submenu(field, value, checked, cx),
            _ => {
                let Some((field, clause)) = self.values_target() else {
                    return;
                };
                let spec = self.fields[field].spec.clone();
                self.filters.toggle(&clause, &spec, value, checked);
                if self.filters.clause(&clause).is_none() {
                    self.close_values();
                }
                self.changed(cx);
            }
        }
    }

    fn select_option(
        &mut self,
        surface: Surface,
        field: usize,
        value: &str,
        cx: &mut Context<Self>,
    ) {
        match surface {
            Surface::Sub => self.select_in_submenu(field, value, cx),
            _ => {
                let Some((_, clause)) = self.values_target() else {
                    return;
                };
                self.filters.set_values(&clause, vec![value.to_string()]);
                self.close_values();
                self.changed(cx);
            }
        }
    }

    fn sub_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.sub.visible() || !self.root.visible() {
            return None;
        }
        let (field, row) = self.sub_field?;
        if field >= self.fields.len() {
            return None;
        }
        let theme = *Theme::global(cx);
        let root = self.geometry.borrow().root?;
        let row_top = f32::from(root.top()) + HEADER_H + LIST_PAD + row as f32 * FIELD_ROW_H;
        let row_left = f32::from(root.left()) + LIST_PAD;
        let row_right = f32::from(root.right()) - LIST_PAD;
        let viewport = f32::from(window.viewport_size().width);
        let right = row_right + SUB_GUTTER;
        // `right-start`, flipped to `left-start` when the window has no room.
        let (x, toward) = if right + PANEL_W <= viewport - WINDOW_MARGIN {
            (right, -1.0)
        } else {
            (row_left - SUB_GUTTER - PANEL_W, 1.0)
        };
        let values = self
            .edited_clause(field)
            .and_then(|id| self.filters.clause(&id).map(|clause| clause.values.clone()))
            .unwrap_or_default();
        let alpha = self.sub.alpha.value().clamp(0.0, 1.0);
        let offset = self.sub.offset() * toward;
        let geometry = self.geometry.clone();
        let entity = cx.entity();
        let panel = surface_panel(
            self.options_panel(Surface::Sub, field, &values, false, window, cx)
                .id(SharedString::from(format!("{}-sub", self.id)))
                .left(px(offset))
                .opacity(alpha)
                .on_hover(move |hovered, _, cx| {
                    entity.update(cx, |this, cx| this.hover_sub(*hovered, cx));
                })
                .on_mouse_down_out(cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    this.press_outside(Surface::Sub, event.position, cx)
                }))
                .child(probe(&geometry, |geometry, bounds| {
                    geometry.sub = Some(bounds)
                })),
            &theme,
        );
        Some(
            deferred(
                anchored()
                    .position(point(px(x), px(row_top)))
                    .anchor(Corner::TopLeft)
                    .snap_to_window_with_margin(px(WINDOW_MARGIN))
                    .child(panel),
            )
            .with_priority(7)
            .into_any_element(),
        )
    }

    fn values_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.values.visible() {
            return None;
        }
        let (field, clause) = self.values_target()?;
        let pill = self.geometry.borrow().pills.get(&clause).copied()?;
        let theme = *Theme::global(cx);
        let values = self
            .filters
            .clause(&clause)
            .map(|clause| clause.values.clone())
            .unwrap_or_default();
        let alpha = self.values.alpha.value().clamp(0.0, 1.0);
        let offset = self.values.offset();
        let geometry = self.geometry.clone();
        let panel = surface_panel(
            self.options_panel(Surface::Values, field, &values, true, window, cx)
                .id(SharedString::from(format!("{}-values", self.id)))
                .top(px(-offset))
                .opacity(alpha)
                .on_mouse_down_out(cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    this.press_outside(Surface::Values, event.position, cx)
                }))
                .child(probe(&geometry, |geometry, bounds| {
                    geometry.values = Some(bounds)
                })),
            &theme,
        );
        Some(
            deferred(
                anchored()
                    .position(point(pill.left(), pill.bottom() + px(GUTTER)))
                    .anchor(Corner::TopLeft)
                    .snap_to_window_with_margin(px(WINDOW_MARGIN))
                    .child(panel),
            )
            .with_priority(6)
            .into_any_element(),
        )
    }

    fn operator_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.operator.visible() {
            return None;
        }
        let id = self.operator_clause.clone()?;
        let clause = self.filters.clause(&id)?.clone();
        let field = self.field_index(&clause.field)?;
        let segment = self.geometry.borrow().operator_segments.get(&id).copied()?;
        let theme = *Theme::global(cx);
        let entity = cx.entity();
        let spec = &self.fields[field].spec;
        let choices = operators(spec, clause.values.len());
        let selected = choices
            .iter()
            .position(|operator| operator.id == clause.operator);
        let pitch = MENU_ROW_H + MENU_ROW_GAP;
        let mut list = div().relative().flex().flex_col().gap(px(MENU_ROW_GAP));
        if let Some(fill) = self
            .operator_hover
            .fill(MENU_ROW_H, theme.color_surface_hover, &theme)
        {
            list = list.child(fill);
        }
        if let Some(index) = selected {
            list = list.child(
                div()
                    .absolute()
                    .left(px(0.0))
                    .right(px(0.0))
                    .top(px(index as f32 * pitch))
                    .h(px(MENU_ROW_H))
                    .rounded(px(theme.radius_item))
                    .bg(theme.color_surface_selected),
            );
        }
        for (index, operator) in choices.iter().enumerate() {
            let row_id = SharedString::from(format!("{}-operator-{index}", self.id));
            let pointer =
                use_pointer_timed(row_id.clone(), PointerTiming::item(&theme), window, cx);
            let hover_color = pointer.read(cx).hover_color();
            let ink = if Some(index) == selected {
                theme.color_text_primary
            } else {
                mix(
                    theme.color_text_secondary,
                    theme.color_text_primary,
                    hover_color,
                )
            };
            let hover_entity = entity.clone();
            let pick_entity = entity.clone();
            let clause_id = id.clone();
            let operator_id = operator.id;
            let row = div()
                .id(row_id)
                .relative()
                .h(px(MENU_ROW_H))
                .px(px(8.125))
                .flex()
                .items_center()
                .gap(px(6.5))
                .rounded(px(theme.radius_item))
                .cursor_pointer()
                .text_size(px(12.0))
                .line_height(px(18.0))
                .text_color(ink)
                .child(operator.label)
                .on_click(move |_, _, cx| {
                    pick_entity.update(cx, |this, cx| {
                        this.filters.set_operator(&clause_id, operator_id);
                        this.close_operator();
                        this.changed(cx);
                    });
                });
            list = list.child(track_with_hover(
                row,
                &pointer,
                true,
                Some(Rc::new(move |hovered, _, cx| {
                    hover_entity.update(cx, |this, cx| {
                        if hovered {
                            this.operator_hover.to(Some(index), pitch);
                        } else {
                            this.operator_hover.leave(index);
                        }
                        cx.notify();
                    });
                })),
            ));
        }
        let alpha = self.operator.alpha.value().clamp(0.0, 1.0);
        let offset = self.operator.offset();
        let geometry = self.geometry.clone();
        let panel = surface_panel(
            div()
                .id(SharedString::from(format!("{}-operator", self.id)))
                .relative()
                .top(px(-offset))
                .min_w(px(OPERATOR_MIN_W))
                .p(px(LIST_PAD))
                .occlude()
                .opacity(alpha)
                .on_mouse_down_out(cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    this.press_outside(Surface::Operator, event.position, cx)
                }))
                .child(list)
                .child(probe(&geometry, |geometry, bounds| {
                    geometry.operator = Some(bounds)
                })),
            &theme,
        );
        Some(
            deferred(
                anchored()
                    .position(point(segment.left(), segment.bottom() + px(MENU_GUTTER)))
                    .anchor(Corner::TopLeft)
                    .snap_to_window_with_margin(px(WINDOW_MARGIN))
                    .child(panel),
            )
            .with_priority(6)
            .into_any_element(),
        )
    }
}

impl Render for FilterPopover {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.take_focus(window, cx);
        if !self.root.visible() {
            self.sub_field = None;
        }
        let entity = cx.entity();
        let trigger = Button::new(
            SharedString::from(format!("{}-trigger", self.id)),
            ButtonKind::Secondary,
        )
        .size(ButtonSize::Icon)
        .icon(Icon::ListFilter)
        .tooltip(self.label.clone())
        .selected(!self.filters.is_empty())
        .on_click(move |_, cx| {
            entity.update(cx, |this, cx| this.toggle_root(Anchor::Trigger, cx));
        });
        let geometry = self.geometry.clone();
        let root = self.root_panel(window, cx);
        let sub = self.sub_panel(window, cx);
        let values = self.values_panel(window, cx);
        let operator = self.operator_panel(window, cx);
        if self.animating() {
            window.request_animation_frame();
        }
        div()
            .relative()
            .flex_none()
            .child(trigger)
            .child(probe(&geometry, |geometry, bounds| {
                geometry.trigger = Some(bounds)
            }))
            .children(root)
            .children(sub)
            .children(values)
            .children(operator)
    }
}

/// A flat round control of the bar (`variant="ghost"`): the hover fill and secondary text that
/// turns primary.
fn ghost(
    id: SharedString,
    window: &mut Window,
    cx: &mut App,
) -> (
    gpui::Stateful<gpui::Div>,
    Entity<super::pointer::PointerState>,
    gpui::Rgba,
) {
    let theme = *Theme::global(cx);
    let pointer = use_pointer_timed(id.clone(), PointerTiming::control(&theme), window, cx);
    let (hover, hover_color, press) = {
        let state = pointer.read(cx);
        (state.hover(), state.hover_color(), state.press())
    };
    let fill = mix(theme.color_surface_hover, theme.color_surface_active, press);
    let ink = mix(
        theme.color_text_secondary,
        theme.color_text_primary,
        hover_color,
    );
    let element = div()
        .id(id)
        .relative()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .cursor_pointer()
        .child(
            div()
                .absolute()
                .inset(px(press))
                .rounded_full()
                .bg(fade(fill, hover.max(press))),
        );
    (element, pointer, ink)
}

impl FilterPopover {
    /// `ActiveFilterBar`: the pills, `+`, and Clear, or nothing while no clause filters.
    pub fn bar(entity: &Entity<Self>, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        let theme = *Theme::global(cx);
        let this = entity.read(cx);
        let clauses: Vec<_> = this
            .filters
            .clauses
            .iter()
            .filter_map(|clause| {
                this.field_index(&clause.field)
                    .map(|field| (clause.clone(), field))
            })
            .collect();
        if clauses.is_empty() {
            return None;
        }
        let id = this.id.clone();
        let geometry = this.geometry.clone();
        let values_open = this
            .values
            .open
            .then(|| this.values_clause.clone())
            .flatten();
        let operator_open = this
            .operator
            .open
            .then(|| this.operator_clause.clone())
            .flatten();
        let add_open = this.root.open && this.anchor == Anchor::Add;
        let fields = this.fields.clone();

        let mut pills = div()
            .flex()
            .flex_1()
            .min_w(px(0.0))
            .flex_wrap()
            .items_center()
            .gap(px(4.0));
        for (clause, field) in clauses {
            let spec = &fields[field].spec;
            let segment_id =
                |part: &str| SharedString::from(format!("{id}-pill-{}-{part}", clause.id));
            let hovered_fill =
                |pointer: &Entity<super::pointer::PointerState>, open: bool, cx: &App| {
                    if open { 1.0 } else { pointer.read(cx).hover() }
                };

            // [icon Field]
            let field_segment = div()
                .flex()
                .min_w(px(0.0))
                .items_center()
                .gap(px(5.0))
                .pl(px(10.0))
                .pr(px(8.0))
                .py(px(4.0))
                .children(
                    fields[field]
                        .icon
                        .map(|name| icon_colored(name, 13.0, theme.color_text_muted)),
                )
                .child(
                    div()
                        .min_w(px(0.0))
                        .child(ellipsis(SharedString::from(spec.label.clone()))),
                );

            // [operator]
            let label = operator_label(spec, &clause);
            let negatable = operators(spec, clause.values.len()).len() > 1;
            let operator_segment = if negatable {
                let pointer = use_pointer_timed(
                    segment_id("operator"),
                    PointerTiming::item(&theme),
                    window,
                    cx,
                );
                let open = operator_open.as_deref() == Some(clause.id.as_str());
                let fill = hovered_fill(&pointer, open, cx);
                let hover_color = if open {
                    1.0
                } else {
                    pointer.read(cx).hover_color()
                };
                let target = entity.clone();
                let clause_id = clause.id.clone();
                let probe_id = clause.id.clone();
                track(
                    div()
                        .id(segment_id("operator"))
                        .relative()
                        .flex()
                        .items_center()
                        .px(px(8.0))
                        .py(px(4.0))
                        .border_l_1()
                        .border_color(theme.color_border_subtle)
                        .bg(fade(theme.color_surface_hover, fill))
                        .cursor_pointer()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(mix(
                            theme.color_text_muted,
                            theme.color_text_primary,
                            hover_color,
                        ))
                        .child(label)
                        .child(probe(&geometry, move |geometry, bounds| {
                            geometry.operator_segments.insert(probe_id.clone(), bounds);
                        }))
                        .on_click(move |_, _, cx| {
                            target.update(cx, |this, cx| this.toggle_operator(&clause_id, cx));
                        }),
                    &pointer,
                    true,
                )
                .into_any_element()
            } else {
                div()
                    .flex()
                    .items_center()
                    .px(px(8.0))
                    .py(px(4.0))
                    .border_l_1()
                    .border_color(theme.color_border_subtle)
                    .font_weight(FontWeight::NORMAL)
                    .text_color(theme.color_text_muted)
                    .child(label)
                    .into_any_element()
            };

            // [value]
            let value_pointer =
                use_pointer_timed(segment_id("value"), PointerTiming::item(&theme), window, cx);
            let value_open = values_open.as_deref() == Some(clause.id.as_str());
            let value_fill = hovered_fill(&value_pointer, value_open, cx);
            let value_target = entity.clone();
            let value_clause = clause.id.clone();
            let value_probe = clause.id.clone();
            let value_segment = track(
                div()
                    .id(segment_id("value"))
                    .relative()
                    .flex()
                    .min_w(px(0.0))
                    .items_center()
                    .gap(px(5.0))
                    .px(px(8.0))
                    .py(px(4.0))
                    .border_l_1()
                    .border_color(theme.color_border_subtle)
                    .bg(fade(theme.color_surface_hover, value_fill))
                    .cursor_pointer()
                    .child(
                        div()
                            .min_w(px(0.0))
                            .child(ellipsis(SharedString::from(value_summary(spec, &clause)))),
                    )
                    .child(probe(&geometry, move |geometry, bounds| {
                        geometry.value_segments.insert(value_probe.clone(), bounds);
                    }))
                    .on_click(move |_, _, cx| {
                        value_target.update(cx, |this, cx| this.toggle_values(&value_clause, cx));
                    }),
                &value_pointer,
                true,
            );

            // [×]
            let remove_pointer = use_pointer_timed(
                segment_id("remove"),
                PointerTiming::item(&theme),
                window,
                cx,
            );
            let (remove_fill, remove_ink) = {
                let state = remove_pointer.read(cx);
                (state.hover(), state.hover_color())
            };
            let remove_target = entity.clone();
            let remove_clause = clause.id.clone();
            let remove_segment = track(
                div()
                    .id(segment_id("remove"))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .px(px(6.0))
                    .border_l_1()
                    .border_color(theme.color_border_subtle)
                    .rounded_r(px(10.0))
                    .bg(fade(theme.color_surface_hover, remove_fill))
                    .cursor_pointer()
                    .tooltip(tooltip(format!("Remove {} filter", spec.label)))
                    .child(icon_colored(
                        Icon::Close,
                        12.0,
                        mix(theme.color_text_muted, theme.color_text_primary, remove_ink),
                    ))
                    .on_click(move |_, _, cx| {
                        remove_target.update(cx, |this, cx| this.remove(&remove_clause, cx));
                    }),
                &remove_pointer,
                true,
            );

            let pill_probe = clause.id.clone();
            pills = pills.child(
                div()
                    .relative()
                    .flex()
                    .max_w_full()
                    .p(px(2.0))
                    .rounded(px(PILL_RADIUS))
                    .bg(face(theme.face_neutral.top, theme.face_neutral.bottom))
                    .shadow(material::control_drop(&theme))
                    .text_size(px(12.0))
                    .line_height(px(14.4))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.color_text_primary)
                    .child(field_segment)
                    .child(operator_segment)
                    .child(value_segment)
                    .child(remove_segment)
                    .child(probe(&geometry, move |geometry, bounds| {
                        geometry.pills.insert(pill_probe.clone(), bounds);
                    }))
                    .child(material::edge(Edge::Strong, PILL_RADIUS, &theme)),
            );
        }

        // `+`: 24px, the root menu at `bottom-start`.
        let (add, add_pointer, add_ink) =
            ghost(SharedString::from(format!("{id}-add")), window, cx);
        let add_target = entity.clone();
        let add = track(
            add.size(px(24.0))
                .when(add_open, |add| add.bg(theme.color_surface_hover))
                .tooltip(tooltip("Add filter"))
                .child(icon_colored(Icon::Plus, 14.0, add_ink))
                .child(probe(&geometry, |geometry, bounds| {
                    geometry.add = Some(bounds)
                }))
                .on_click(move |_, _, cx| {
                    add_target.update(cx, |this, cx| this.toggle_root(Anchor::Add, cx));
                }),
            &add_pointer,
            true,
        );
        pills = pills.child(add);

        // Clear: a ghost pill, `px-2.5 py-1`, 11px.
        let (clear, clear_pointer, clear_ink) =
            ghost(SharedString::from(format!("{id}-clear")), window, cx);
        let clear_target = entity.clone();
        let clear = track(
            clear
                .px(px(8.125))
                .py(px(3.25))
                .text_size(px(11.0))
                .line_height(px(16.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(clear_ink)
                .child(div().relative().child("Clear"))
                .on_click(move |_, _, cx| {
                    clear_target.update(cx, |this, cx| this.clear(cx));
                }),
            &clear_pointer,
            true,
        );

        Some(
            div()
                .relative()
                .m(px(8.0))
                .p(px(2.0))
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(2.0))
                .rounded(px(TRACK_RADIUS))
                .bg(face(theme.field_fill_top, theme.field_fill_bottom))
                .shadow(material::control_drop(&theme))
                .child(pills)
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(2.0))
                        .mr(px(2.0))
                        .child(clear),
                )
                .child(material::edge(Edge::Strong, TRACK_RADIUS, &theme))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .into_any_element(),
        )
    }
}
