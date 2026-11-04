//! Property pills: the row of choices under a Composer's title, each a pill that opens a
//! picker.
//!
//! They look like the Solid design system's `PropertyPills` and `PropertyPicker`. A pill is
//! a small texture button (`size="sm" variant="texture"`, 6px apart, wrapping) showing the
//! picked option, or the property's name while nothing is picked. A property that holds
//! several values (`TokenPicker`) is one group on a field-face track, as `ActiveFilterBar`
//! holds its pills: its icon and name, each value as a pill that removes it (or the empty
//! label while none is picked), then a `+`, so two lists of the same options read apart; its
//! picker's rows are checkboxes that toggle and keep it open. The picker is a 240px floating
//! panel: a bare search field (`Set lowest tier…`, `10px 12px` in, a subtle rule under it),
//! then the options (`6px` in, 2px apart, at most 260px tall). An option row is 32px
//! (40 with a hint) and `0 10px` in: a 14px radio mark, the 20px option tile (its icon, or
//! the label's initial), the 13px label with its hint in 11px muted under it, and the
//! option's digit at the end while the search is empty.
//!
//! The picker behaves like gpui-kit's `Select` with search on: it opens on the picked option,
//! typing filters by label and hint, ↑/↓ move the highlight (wrapping) and the pointer lights
//! the row under it, Enter or a click picks and closes, and Escape or a click outside closes
//! without a change and clears the search for the next time. As in the Solid picker, a digit
//! typed into the empty search picks that option.
//!
//! The pills can sit inside a scrolling body, which clips. So the open picker is not drawn
//! with them: the host draws [`PropertyPills::overlay`] outside that body (the Composer card
//! has a slot for it), and the picker hangs from its pill in window coordinates.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    AnyElement, App, Bounds, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    KeyBinding, Pixels, Render, SharedString, Window, actions, canvas, div, prelude::*, px,
};

use super::shell::window_layer;
use crate::button::{Button, ButtonKind, ButtonSize};
use crate::checkbox::checkbox_mark;
use crate::hang::{SLIDE, hang};
use crate::icon_tile::IconTile;
use crate::icons::{Icon, icon_colored};
use crate::material::{self, Edge, face, floating_panel};
use crate::motion::{Animator, SpringTier, SpringValue};
use crate::radio_group::radio_mark;
use crate::text_input::{self, TextInput};
use crate::theme::Theme;

/// The picker's width (`width ?? 240`).
pub const PICKER_WIDTH: f32 = 240.0;
/// The tallest the options grow before they scroll.
const OPTIONS_MAX_H: f32 = 260.0;
/// Between two pills.
const PILL_GAP: f32 = 6.0;
/// A token group's track: its 28px pills and 2px around them.
const GROUP_RADIUS: f32 = 16.0;
/// A click on the pill whose picker a press outside just closed toggles it shut, not open
/// again: the press closes it first, then the click lands.
const REOPEN_GUARD: Duration = Duration::from_millis(250);

actions!(property_picker, [SelectPrev, SelectNext, Confirm, Cancel]);

/// ↑, ↓, Enter, and Escape inside the picker. [`crate::init`] installs them.
pub fn register_key_bindings(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", SelectPrev, Some("PropertyPicker")),
        KeyBinding::new("down", SelectNext, Some("PropertyPicker")),
        KeyBinding::new("enter", Confirm, Some("PropertyPicker")),
        KeyBinding::new("escape", Cancel, Some("PropertyPicker")),
    ]);
}

/// Draws the app's own mark at the size it is given (20px in the picker, 14px on a pill).
type MarkBuilder = Rc<dyn Fn(f32, &mut Window, &mut App) -> AnyElement>;

/// One value a property can take.
#[derive(Clone)]
pub struct PropertyOption {
    value: SharedString,
    label: SharedString,
    icon: Option<Icon>,
    mark: Option<MarkBuilder>,
    hint: Option<SharedString>,
}

impl PropertyOption {
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            icon: None,
            mark: None,
            hint: None,
        }
    }

    /// The icon in the option's tile and on its pill.
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// The app's own mark in place of the tile, such as an agent's avatar, drawn at the
    /// size the builder is given: 20px in the picker, 14px on a pill.
    pub fn mark(
        mut self,
        build: impl Fn(f32, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.mark = Some(Rc::new(build));
        self
    }

    /// A muted second line, which the search matches too.
    pub fn hint(mut self, hint: impl Into<SharedString>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn value(&self) -> &SharedString {
        &self.value
    }

    pub fn label(&self) -> &SharedString {
        &self.label
    }

    fn matches(&self, needle: &str) -> bool {
        needle.is_empty()
            || self.label.to_lowercase().contains(needle)
            || self
                .hint
                .as_ref()
                .is_some_and(|hint| hint.to_lowercase().contains(needle))
    }
}

#[derive(Clone)]
enum Kind {
    Single(SharedString),
    Tokens {
        values: Vec<SharedString>,
        empty: SharedString,
    },
}

/// One property: a single choice, or a set of values edited as tokens.
#[derive(Clone)]
pub struct Property {
    id: SharedString,
    label: SharedString,
    icon: Option<Icon>,
    options: Vec<PropertyOption>,
    kind: Kind,
    width: f32,
}

impl Property {
    /// One choice among `options`, `value` picked. The pill shows the picked option's label,
    /// or `label` when `value` is none of them.
    pub fn single(
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        options: Vec<PropertyOption>,
        value: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            options,
            kind: Kind::Single(value.into()),
            width: PICKER_WIDTH,
        }
    }

    /// Any number of `options`, `values` picked in order; `empty` says what none means,
    /// such as "No preference".
    pub fn tokens(
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        options: Vec<PropertyOption>,
        values: Vec<SharedString>,
        empty: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            options,
            kind: Kind::Tokens {
                values,
                empty: empty.into(),
            },
            width: PICKER_WIDTH,
        }
    }

    /// The icon on the pill while its option has none.
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// The picker's width, for options with a second line.
    pub fn picker_width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn id(&self) -> &SharedString {
        &self.id
    }

    /// The picked values: one for a single choice.
    pub fn values(&self) -> Vec<SharedString> {
        match &self.kind {
            Kind::Single(value) => vec![value.clone()],
            Kind::Tokens { values, .. } => values.clone(),
        }
    }

    fn option(&self, value: &str) -> Option<&PropertyOption> {
        self.options
            .iter()
            .find(|option| option.value.as_ref() == value)
    }
}

/// What the picker tells its host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PropertyPickerEvent {
    /// The option with this value was picked; the picker closes after it.
    Pick(SharedString),
    /// A checkbox of a picker of several values: this value is now checked or not. The
    /// picker stays open for the next one.
    Toggle(SharedString, bool),
    /// The picker closed.
    Close,
}

/// The picker: its search, its options, what matches, and the highlight.
pub struct PropertyPicker {
    focus: FocusHandle,
    input: Entity<TextInput>,
    label: SharedString,
    options: Vec<PropertyOption>,
    value: Option<SharedString>,
    /// The checked values while it picks several (checkboxes that keep it open), else none.
    checked: Option<Vec<SharedString>>,
    width: f32,
    trigger_height: f32,
    /// Indices into `options`, in order, that match the search.
    matched: Vec<usize>,
    active: Option<usize>,
    applied_query: String,
    open: bool,
    progress: SpringValue,
    animator: Animator,
}

impl EventEmitter<PropertyPickerEvent> for PropertyPicker {}

impl PropertyPicker {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let weak = cx.entity().downgrade();
        let input = cx.new(|cx| {
            TextInput::new(cx).bare().on_submit(move |_, _, cx| {
                if let Some(this) = weak.upgrade() {
                    this.update(cx, |this, cx| this.confirm(cx));
                }
            })
        });
        cx.observe(&input, |this, _, cx| this.on_query_changed(cx))
            .detach();
        Self {
            focus: cx.focus_handle(),
            input,
            label: SharedString::default(),
            options: Vec::new(),
            value: None,
            checked: None,
            width: PICKER_WIDTH,
            trigger_height: 28.0,
            matched: Vec::new(),
            active: None,
            applied_query: String::new(),
            open: false,
            progress: SpringValue::new(0.0, SpringTier::Fast),
            animator: Animator::new(),
        }
    }

    /// What it picks: `label` names the search (`Set {label}…`), `value` is the picked
    /// option (none for tokens), and the panel is `width` wide.
    pub fn set_options(
        &mut self,
        label: impl Into<SharedString>,
        options: Vec<PropertyOption>,
        value: Option<SharedString>,
        width: f32,
        cx: &mut Context<Self>,
    ) {
        self.label = label.into();
        self.options = options;
        self.value = value;
        self.checked = None;
        self.width = width;
        let placeholder = format!("Set {}…", self.label.to_lowercase());
        self.input.update(cx, |input, cx| {
            input.set_placeholder(placeholder);
            cx.notify();
        });
        self.update_matches();
        cx.notify();
    }

    /// Picks several values, `checked` checked: each row is a checkbox, and a click or Enter
    /// toggles it and leaves the picker open, as gpui-kit's multiple `Select` does. A press
    /// outside or on its trigger closes it. Call after [`Self::set_options`].
    pub fn set_checked(&mut self, checked: Vec<SharedString>, cx: &mut Context<Self>) {
        self.checked = Some(checked);
        cx.notify();
    }

    /// The height of the control it hangs from.
    pub fn set_trigger_height(&mut self, height: f32) {
        self.trigger_height = height;
    }

    /// Opens with an empty search and the picked option lit, the search focused.
    pub fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = true;
        self.clear_query(cx);
        self.active = self
            .value
            .as_ref()
            .and_then(|value| {
                self.matched
                    .iter()
                    .position(|index| self.options[*index].value == *value)
            })
            .or((!self.matched.is_empty()).then_some(0));
        self.progress.set_target(1.0);
        self.input.read(cx).focus_handle(cx).focus(window);
        cx.notify();
    }

    /// Closes without a change; the next open starts from an empty search.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        if !self.open {
            return;
        }
        self.open = false;
        self.progress.set_target(0.0);
        self.clear_query(cx);
        cx.emit(PropertyPickerEvent::Close);
        cx.notify();
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Whether it must stay drawn: open, or still playing out.
    pub fn is_presented(&self) -> bool {
        self.open || !self.progress.settled() || self.progress.value() > 0.001
    }

    pub fn query(&self, cx: &App) -> String {
        self.input.read(cx).text().to_string()
    }

    pub fn set_query(&mut self, query: impl Into<String>, cx: &mut Context<Self>) {
        let query = query.into();
        self.input.update(cx, |input, cx| input.set_text(query, cx));
        self.on_query_changed(cx);
    }

    /// The lit option's value.
    pub fn active_value(&self) -> Option<&SharedString> {
        self.active
            .and_then(|row| self.matched.get(row))
            .map(|index| &self.options[*index].value)
    }

    fn clear_query(&mut self, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| input.set_text("", cx));
        self.applied_query.clear();
        self.update_matches();
    }

    fn on_query_changed(&mut self, cx: &mut Context<Self>) {
        let query = self.query(cx);
        if query == self.applied_query {
            return;
        }
        // A digit typed into the empty search picks that option.
        if self.applied_query.is_empty()
            && let Some(digit) = single_digit(&query)
            && digit <= self.matched.len()
        {
            self.choose(digit - 1, cx);
            return;
        }
        self.applied_query = query;
        self.update_matches();
        self.active = (!self.matched.is_empty()).then_some(0);
        cx.notify();
    }

    fn update_matches(&mut self) {
        let needle = self.applied_query.trim().to_lowercase();
        self.matched = (0..self.options.len())
            .filter(|index| self.options[*index].matches(&needle))
            .collect();
        if self.active.is_some_and(|row| row >= self.matched.len()) {
            self.active = (!self.matched.is_empty()).then_some(0);
        }
    }

    fn step(&mut self, step: isize, cx: &mut Context<Self>) {
        let len = self.matched.len();
        if len == 0 {
            return;
        }
        let from = self.active.unwrap_or(if step >= 0 { len - 1 } else { 0 });
        self.active = Some((from as isize + step).rem_euclid(len as isize) as usize);
        cx.notify();
    }

    fn confirm(&mut self, cx: &mut Context<Self>) {
        if let Some(row) = self.active {
            self.choose(row, cx);
        }
    }

    fn choose(&mut self, row: usize, cx: &mut Context<Self>) {
        let Some(index) = self.matched.get(row) else {
            return;
        };
        let value = self.options[*index].value.clone();
        if let Some(checked) = self.checked.as_mut() {
            let on = !checked.contains(&value);
            if on {
                checked.push(value.clone());
            } else {
                checked.retain(|picked| picked != &value);
            }
            // A digit's shortcut leaves the digit in the search; the next one starts clean.
            if self.applied_query.is_empty() && single_digit(&self.query(cx)).is_some() {
                self.clear_query(cx);
            }
            cx.emit(PropertyPickerEvent::Toggle(value, on));
            cx.notify();
            return;
        }
        self.value = Some(value.clone());
        cx.emit(PropertyPickerEvent::Pick(value));
        self.close(cx);
    }

    fn row(
        &self,
        row: usize,
        option: &PropertyOption,
        theme: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let lit = self.active == Some(row);
        let mark = match &self.checked {
            Some(checked) => checkbox_mark(checked.contains(&option.value), 14.0, theme),
            None => radio_mark(self.value.as_ref() == Some(&option.value), 14.0, theme),
        };
        let tile = match &option.mark {
            Some(build) => div()
                .flex_none()
                .child(build(20.0, window, cx))
                .into_any_element(),
            None => IconTile::option(option.icon, &option.label).into_any_element(),
        };
        let mut text = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .child(div().truncate().child(option.label.clone()));
        if let Some(hint) = &option.hint {
            text = text.child(
                div()
                    .truncate()
                    .text_size(px(11.0))
                    .line_height(px(16.5))
                    .text_color(theme.color_text_muted)
                    .child(hint.clone()),
            );
        }
        div()
            .id(("property-option", row))
            .w_full()
            .min_h(px(if option.hint.is_some() { 40.0 } else { 32.0 }))
            .px(px(10.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(px(theme.radius_item))
            .cursor_pointer()
            .text_color(if lit {
                theme.color_text_primary
            } else {
                theme.color_text_secondary
            })
            .when(lit, |row| row.bg(theme.color_surface_hover))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered && this.active != Some(row) {
                    this.active = Some(row);
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _, _, cx| this.choose(row, cx)))
            .child(mark)
            .child(tile)
            .child(text)
            .when(self.applied_query.is_empty() && row < 9, |line| {
                line.child(
                    div()
                        .flex_none()
                        .text_size(px(11.0))
                        .text_color(theme.color_text_muted)
                        .child((row + 1).to_string()),
                )
            })
            .into_any_element()
    }
}

fn single_digit(text: &str) -> Option<usize> {
    let mut chars = text.chars();
    let first = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    first
        .to_digit(10)
        .filter(|digit| (1..=9).contains(digit))
        .map(|digit| digit as usize)
}

impl Focusable for PropertyPicker {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for PropertyPicker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dt = self
            .animator
            .dt(window, self.is_presented() && !self.progress.settled());
        if dt > 0.0 {
            self.progress.step(dt);
        }
        if !self.is_presented() {
            return div().into_any_element();
        }
        let theme = *Theme::global(cx);
        let t = self.progress.value().clamp(0.0, 1.0);
        let rows: Vec<AnyElement> = self
            .matched
            .clone()
            .into_iter()
            .enumerate()
            .map(|(row, index)| {
                let option = self.options[index].clone();
                self.row(row, &option, &theme, window, cx)
            })
            .collect();
        let empty = rows.is_empty();
        let list = div()
            .id("property-options")
            .max_h(px(OPTIONS_MAX_H))
            .overflow_y_scroll()
            .p(px(6.0))
            .flex()
            .flex_col()
            .gap(px(2.0))
            .children(rows)
            .when(empty, |list| {
                list.child(
                    div()
                        .px(px(10.0))
                        .py(px(8.0))
                        .text_size(px(12.0))
                        .text_color(theme.color_text_muted)
                        .child("No match."),
                )
            });
        let body = div()
            .relative()
            .w(px(self.width))
            .bg(theme.color_surface_subtle)
            .flex()
            .flex_col()
            .text_size(px(13.0))
            .line_height(px(19.5))
            .child(
                div()
                    .px(px(12.0))
                    .py(px(10.0))
                    .border_b_1()
                    .border_color(theme.color_border_subtle)
                    .child(self.input.clone()),
            )
            .child(list);
        let panel = div()
            .occlude()
            .key_context("PropertyPicker")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &SelectPrev, _, cx| this.step(-1, cx)))
            .on_action(cx.listener(|this, _: &SelectNext, _, cx| this.step(1, cx)))
            .on_action(cx.listener(|this, _: &Confirm, _, cx| this.confirm(cx)))
            .on_action(cx.listener(|this, _: &Cancel, _, cx| this.close(cx)))
            .on_action(cx.listener(|this, _: &text_input::Cancel, _, cx| this.close(cx)))
            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.close(cx)))
            .opacity(t)
            .child(floating_panel(body, theme.radius_menu, &theme));
        hang(self.trigger_height + 4.0, (1.0 - t) * SLIDE, panel).into_any_element()
    }
}

/// What the pills tell their host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PropertyPillsEvent {
    /// The property with this id now holds these values (one for a single choice).
    Changed {
        id: SharedString,
        values: Vec<SharedString>,
    },
}

/// The row of pills and the one picker they share.
pub struct PropertyPills {
    properties: Vec<Property>,
    picker: Entity<PropertyPicker>,
    /// The property whose picker is open, or was last.
    opened: Option<usize>,
    /// Each property's pill (its add pill for tokens), as the last frame laid it out.
    anchors: Rc<RefCell<Vec<Option<Bounds<Pixels>>>>>,
    closed_at: Option<(usize, Instant)>,
}

impl EventEmitter<PropertyPillsEvent> for PropertyPills {}

impl PropertyPills {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let picker = cx.new(PropertyPicker::new);
        cx.subscribe(&picker, |this, _, event, cx| match event {
            PropertyPickerEvent::Pick(value) => this.picked(value.clone(), cx),
            PropertyPickerEvent::Toggle(value, on) => this.toggled(value.clone(), *on, cx),
            PropertyPickerEvent::Close => {
                if let Some(index) = this.opened {
                    this.closed_at = Some((index, Instant::now()));
                }
                cx.notify();
            }
        })
        .detach();
        Self {
            properties: Vec::new(),
            picker,
            opened: None,
            anchors: Rc::default(),
            closed_at: None,
        }
    }

    /// The properties, in order. An open picker closes if its property is gone.
    pub fn set_properties(&mut self, properties: Vec<Property>, cx: &mut Context<Self>) {
        self.properties = properties;
        self.anchors
            .borrow_mut()
            .resize(self.properties.len(), None);
        // An open checkbox picker shows what its property holds now, which its host may
        // have changed in answer to the last toggle.
        if let Some(Kind::Tokens { values, .. }) = self
            .opened
            .and_then(|index| self.properties.get(index))
            .map(|property| &property.kind)
            && self.picker.read(cx).is_open()
        {
            let values = values.clone();
            self.picker
                .update(cx, |picker, cx| picker.set_checked(values, cx));
        }
        if let Some(index) = self.opened
            && index >= self.properties.len()
        {
            self.picker.update(cx, |picker, cx| picker.close(cx));
            self.opened = None;
        }
        cx.notify();
    }

    pub fn properties(&self) -> &[Property] {
        &self.properties
    }

    /// Whether a picker is open.
    pub fn is_picking(&self, cx: &App) -> bool {
        self.picker.read(cx).is_open()
    }

    /// The open picker, hung from its pill. Draw it outside any scrolling body that holds
    /// the pills; it is none while no picker shows.
    pub fn overlay(&self, cx: &App) -> Option<AnyElement> {
        if !self.picker.read(cx).is_presented() {
            return None;
        }
        let anchor = (*self.anchors.borrow().get(self.opened?)?)?;
        Some(window_layer(
            div().size_full().child(
                div()
                    .absolute()
                    .left(anchor.origin.x)
                    .top(anchor.origin.y)
                    .w(anchor.size.width)
                    .h(anchor.size.height)
                    .child(self.picker.clone()),
            ),
        ))
    }

    /// Opens the picker of property `index`, or closes it when it is the open one.
    pub fn toggle(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let open = self.picker.read(cx).is_open();
        if open && self.opened == Some(index) {
            self.picker.update(cx, |picker, cx| picker.close(cx));
            return;
        }
        if !open
            && let Some((closed, at)) = self.closed_at
            && closed == index
            && at.elapsed() < REOPEN_GUARD
        {
            return;
        }
        let Some(property) = self.properties.get(index) else {
            return;
        };
        let (value, checked) = match &property.kind {
            Kind::Single(value) => (Some(value.clone()), None),
            Kind::Tokens { values, .. } => (None, Some(values.clone())),
        };
        let (label, options, width) = (
            property.label.clone(),
            property.options.clone(),
            property.width,
        );
        let height = self
            .anchors
            .borrow()
            .get(index)
            .copied()
            .flatten()
            .map_or(28.0, |bounds| f32::from(bounds.size.height));
        self.opened = Some(index);
        self.picker.update(cx, |picker, cx| {
            picker.set_options(label, options, value, width, cx);
            if let Some(checked) = checked {
                picker.set_checked(checked, cx);
            }
            picker.set_trigger_height(height);
            picker.open(window, cx);
        });
        cx.notify();
    }

    fn picked(&mut self, value: SharedString, cx: &mut Context<Self>) {
        let Some(property) = self.opened.and_then(|index| self.properties.get_mut(index)) else {
            return;
        };
        match &mut property.kind {
            Kind::Single(current) => *current = value,
            Kind::Tokens { values, .. } => {
                if !values.contains(&value) {
                    values.push(value);
                }
            }
        }
        let event = PropertyPillsEvent::Changed {
            id: property.id.clone(),
            values: property.values(),
        };
        cx.emit(event);
        cx.notify();
    }

    fn toggled(&mut self, value: SharedString, on: bool, cx: &mut Context<Self>) {
        let Some(property) = self.opened.and_then(|index| self.properties.get_mut(index)) else {
            return;
        };
        if let Kind::Tokens { values, .. } = &mut property.kind {
            values.retain(|picked| picked != &value);
            if on {
                values.push(value);
            }
        }
        let event = PropertyPillsEvent::Changed {
            id: property.id.clone(),
            values: property.values(),
        };
        cx.emit(event);
        cx.notify();
    }

    fn remove(&mut self, index: usize, value: &SharedString, cx: &mut Context<Self>) {
        let Some(property) = self.properties.get_mut(index) else {
            return;
        };
        if let Kind::Tokens { values, .. } = &mut property.kind {
            values.retain(|picked| picked != value);
        }
        let event = PropertyPillsEvent::Changed {
            id: property.id.clone(),
            values: property.values(),
        };
        cx.emit(event);
        cx.notify();
    }

    /// `pill` with the box it laid out in recorded as property `index`'s anchor.
    fn anchored(&self, index: usize, pill: impl IntoElement) -> AnyElement {
        let anchors = self.anchors.clone();
        div()
            .relative()
            .flex_none()
            .whitespace_nowrap()
            .child(pill)
            .child(
                canvas(
                    move |bounds, _, _| {
                        if let Some(slot) = anchors.borrow_mut().get_mut(index) {
                            *slot = Some(bounds);
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .into_any_element()
    }
}

/// A pill's 14px mark: the option's own, else its icon.
fn pill_mark(
    option: Option<&PropertyOption>,
    window: &mut Window,
    cx: &mut App,
) -> Option<AnyElement> {
    let option = option?;
    option.mark.as_ref().map(|build| {
        div()
            .flex_none()
            .child(build(14.0, window, cx))
            .into_any_element()
    })
}

impl Render for PropertyPills {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let view = cx.entity().downgrade();
        let mut row = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(PILL_GAP));
        for (index, property) in self.properties.clone().into_iter().enumerate() {
            let toggle = {
                let view = view.clone();
                move |window: &mut Window, cx: &mut App| {
                    if let Some(view) = view.upgrade() {
                        view.update(cx, |this, cx| this.toggle(index, window, cx));
                    }
                }
            };
            match &property.kind {
                Kind::Single(value) => {
                    let option = property.option(value);
                    let label =
                        option.map_or(property.label.clone(), |option| option.label.clone());
                    let mut pill = Button::new(("property-pill", index), ButtonKind::Secondary)
                        .size(ButtonSize::Sm)
                        .label(label)
                        .on_click(toggle);
                    if let Some(mark) = pill_mark(option, window, cx) {
                        pill = pill.leading(mark);
                    } else if let Some(name) =
                        option.and_then(|option| option.icon).or(property.icon)
                    {
                        pill = pill.icon(name);
                    }
                    row = row.child(self.anchored(index, pill));
                }
                Kind::Tokens { values, empty } => {
                    // One group per property, as `ActiveFilterBar`'s track holds its pills:
                    // the property's icon and name, its values as pills that remove them, then
                    // `+`, so two lists of the same options read apart.
                    let mut group =
                        div()
                            .relative()
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(2.0))
                            .p(px(2.0))
                            .rounded(px(GROUP_RADIUS))
                            .bg(face(theme.field_fill_top, theme.field_fill_bottom))
                            .shadow(material::control_drop(&theme))
                            .child(
                                div()
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .gap(px(5.0))
                                    .pl(px(10.0))
                                    .pr(px(6.0))
                                    .text_size(px(12.0))
                                    .text_color(theme.color_text_muted)
                                    .whitespace_nowrap()
                                    .children(property.icon.map(|name| {
                                        icon_colored(name, 13.0, theme.color_text_muted)
                                    }))
                                    .child(property.label.clone()),
                            );
                    if values.is_empty() {
                        group = group.child(
                            div()
                                .flex_none()
                                .px(px(6.0))
                                .text_size(px(12.0))
                                .text_color(theme.color_text_secondary)
                                .whitespace_nowrap()
                                .child(empty.clone()),
                        );
                    }
                    for (slot, value) in values.iter().enumerate() {
                        let option = property.option(value);
                        let label = option.map_or(value.clone(), |option| option.label.clone());
                        let remove = {
                            let view = view.clone();
                            let value = value.clone();
                            move |_: &mut Window, cx: &mut App| {
                                if let Some(view) = view.upgrade() {
                                    view.update(cx, |this, cx| this.remove(index, &value, cx));
                                }
                            }
                        };
                        let mut token = Button::new(
                            ("property-token", index * 1000 + slot),
                            ButtonKind::Secondary,
                        )
                        .size(ButtonSize::Sm)
                        .label(label.clone())
                        .tooltip(format!("Remove {label}"))
                        .trailing(icon_colored(Icon::X, 12.0, theme.color_text_muted))
                        .on_click(remove);
                        if let Some(mark) = pill_mark(option, window, cx) {
                            token = token.leading(mark);
                        } else if let Some(name) = option.and_then(|option| option.icon) {
                            token = token.icon(name);
                        }
                        group = group.child(div().flex_none().whitespace_nowrap().child(token));
                    }
                    let remaining = !property.options.is_empty();
                    let add = Button::new(("property-add", index), ButtonKind::Ghost)
                        .size(ButtonSize::Icon)
                        .icon(Icon::Plus)
                        .tooltip(format!("Add to {}", property.label.to_lowercase()))
                        .disabled(!remaining)
                        .on_click(toggle);
                    group = group.child(self.anchored(index, add)).child(material::edge(
                        Edge::Strong,
                        GROUP_RADIUS,
                        &theme,
                    ));
                    row = row.child(group);
                }
            }
        }
        row
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gpui::{Entity, SharedString, TestAppContext, VisualTestContext};

    use super::{
        Property, PropertyOption, PropertyPicker, PropertyPickerEvent, PropertyPills,
        PropertyPillsEvent,
    };

    fn tiers() -> Vec<PropertyOption> {
        vec![
            PropertyOption::new("", "Any tier"),
            PropertyOption::new("easy", "Easy"),
            PropertyOption::new("normal", "Normal").hint("most tasks"),
            PropertyOption::new("hard", "Hard"),
        ]
    }

    fn picker(cx: &mut TestAppContext) -> (Entity<PropertyPicker>, &mut VisualTestContext) {
        cx.update(crate::init);
        let (view, cx) = cx.add_window_view(|_, cx| PropertyPicker::new(cx));
        view.update_in(cx, |this, window, cx| {
            this.set_options("Lowest tier", tiers(), Some("normal".into()), 240.0, cx);
            this.open(window, cx);
        });
        (view, cx)
    }

    fn events(
        view: &Entity<PropertyPicker>,
        cx: &mut VisualTestContext,
    ) -> Rc<RefCell<Vec<PropertyPickerEvent>>> {
        let seen: Rc<RefCell<Vec<PropertyPickerEvent>>> = Rc::default();
        let sink = seen.clone();
        cx.update(|_, cx| {
            cx.subscribe(view, move |_, event: &PropertyPickerEvent, _| {
                sink.borrow_mut().push(event.clone());
            })
            .detach();
        });
        seen
    }

    #[gpui::test]
    fn it_opens_on_the_picked_option_and_wraps(cx: &mut TestAppContext) {
        let (view, cx) = picker(cx);
        view.update(cx, |this, cx| {
            assert_eq!(this.active_value().map(|v| v.as_ref()), Some("normal"));
            this.step(1, cx);
            assert_eq!(this.active_value().map(|v| v.as_ref()), Some("hard"));
            this.step(1, cx);
            assert_eq!(this.active_value().map(|v| v.as_ref()), Some(""));
            this.step(-1, cx);
            assert_eq!(this.active_value().map(|v| v.as_ref()), Some("hard"));
        });
    }

    #[gpui::test]
    fn search_matches_labels_and_hints(cx: &mut TestAppContext) {
        let (view, cx) = picker(cx);
        view.update(cx, |this, cx| {
            this.set_query("most", cx);
            assert_eq!(this.matched.len(), 1);
            assert_eq!(this.active_value().map(|v| v.as_ref()), Some("normal"));
            this.set_query("zzz", cx);
            assert!(this.matched.is_empty());
            assert_eq!(this.active_value(), None);
        });
    }

    #[gpui::test]
    fn a_digit_picks_and_escape_closes_without_a_change(cx: &mut TestAppContext) {
        let (view, cx) = picker(cx);
        let seen = events(&view, cx);
        view.update(cx, |this, cx| this.set_query("2", cx));
        assert_eq!(
            seen.borrow().as_slice(),
            [
                PropertyPickerEvent::Pick("easy".into()),
                PropertyPickerEvent::Close
            ]
        );
        view.update_in(cx, |this, window, cx| {
            assert!(!this.is_open());
            assert_eq!(this.query(cx), "");
            this.open(window, cx);
            this.set_query("ha", cx);
            this.close(cx);
            this.open(window, cx);
            assert_eq!(this.query(cx), "", "a closed search starts empty again");
            assert_eq!(this.active_value().map(|v| v.as_ref()), Some("easy"));
        });
        assert_eq!(seen.borrow().len(), 3);
    }

    #[gpui::test]
    fn pills_report_a_pick_and_a_removed_token(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (view, cx) = cx.add_window_view(|_, cx| PropertyPills::new(cx));
        let seen: Rc<RefCell<Vec<PropertyPillsEvent>>> = Rc::default();
        let sink = seen.clone();
        cx.update(|_, cx| {
            cx.subscribe(&view, move |_, event: &PropertyPillsEvent, _| {
                sink.borrow_mut().push(event.clone());
            })
            .detach();
        });
        let agents = vec![
            PropertyOption::new("codex", "codex"),
            PropertyOption::new("claude", "claude"),
        ];
        // Events reach their subscribers once each update ends, as each click does.
        view.update_in(cx, |this, window, cx| {
            this.set_properties(
                vec![
                    Property::single("tier", "Lowest tier", tiers(), ""),
                    Property::tokens("prefer", "Prefer", agents, vec![], "No preference"),
                ],
                cx,
            );
            this.toggle(0, window, cx);
            assert!(this.is_picking(cx));
            this.picker
                .update(cx, |picker, cx| picker.set_query("4", cx));
        });
        view.update_in(cx, |this, window, cx| {
            assert!(!this.is_picking(cx));
            assert_eq!(
                this.properties[0].values(),
                vec![SharedString::from("hard")]
            );
            this.closed_at = None;
            this.toggle(1, window, cx);
            this.picker.update(cx, |picker, cx| {
                assert_eq!(picker.matched.len(), 2);
                picker.confirm(cx);
            });
            // A checkbox picker stays open for the next value.
            assert!(this.is_picking(cx));
        });
        view.update(cx, |this, cx| {
            assert_eq!(
                this.properties[1].values(),
                vec![SharedString::from("codex")]
            );
            this.remove(1, &"codex".into(), cx);
        });
        assert_eq!(
            seen.borrow().as_slice(),
            [
                PropertyPillsEvent::Changed {
                    id: "tier".into(),
                    values: vec!["hard".into()]
                },
                PropertyPillsEvent::Changed {
                    id: "prefer".into(),
                    values: vec!["codex".into()]
                },
                PropertyPillsEvent::Changed {
                    id: "prefer".into(),
                    values: vec![]
                },
            ]
        );
    }
}
