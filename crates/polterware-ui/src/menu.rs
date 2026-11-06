//! `DropdownMenu` (`@polterware/frontend-design-system`): an anchored menu of actions, radio
//! groups, separators, and submenus.
//!
//! It looks like the Solid `MenuSurface` on `.menu-panel`: the card fill, `--radius-menu`, the
//! quiet edge and the float drop, `p-1.5` with rows `gap-1` apart at 12px. A row is `px-2.5
//! py-2` at `--radius-item` in `--color-text-secondary`; the highlighted one takes
//! `--color-surface-hover` and the primary text, as does a picked choice. A choice leads with
//! its 14px control: a radio item (one of a group) the round [`radio_mark`], a check item
//! (any number of a group) the square [`checkbox_mark`]. A submenu's row ends in a chevron
//! and opens its own panel beside the menu. A separator is a `--color-border-subtle` rule
//! `my-1`.
//!
//! It behaves like gpui-kit's `PopupMenu`: ↑/↓ move the highlight, wrapping and passing
//! separators by; → or Enter on a submenu opens it and highlights its first row, ← or Escape
//! closes it; Enter or a click runs an item and closes the menu, except a check item, which
//! toggles and leaves the menu open for the next one; the pointer highlights the
//! row under it and opens a submenu by hovering; Escape or a press outside closes the menu.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{
    App, Bounds, Context, DispatchPhase, FocusHandle, Focusable, IntoElement, KeyBinding,
    MouseMoveEvent, Pixels, Render, SharedString, Window, actions, canvas, div, prelude::*, px,
    relative,
};

use super::Dismiss;
use super::aim::{AIM_DELAY, MenuAim};
use super::checkbox::checkbox_mark;
use super::hang::{SLIDE, hang};
use super::icons::{Icon, icon, icon_colored};
use super::radio_group::radio_mark;
use crate::Handler;
use crate::material::floating_panel;
use crate::motion::{Animator, SpringTier, SpringValue};
use crate::theme::{Theme, tw};

actions!(
    menu,
    [SelectPrev, SelectNext, Confirm, OpenSubmenu, CloseSubmenu]
);

/// The menu's keys, inside its `Menu` context. [`crate::init`] installs them.
pub fn register_key_bindings(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", SelectPrev, Some("Menu")),
        KeyBinding::new("down", SelectNext, Some("Menu")),
        KeyBinding::new("enter", Confirm, Some("Menu")),
        KeyBinding::new("right", OpenSubmenu, Some("Menu")),
        KeyBinding::new("left", CloseSubmenu, Some("Menu")),
    ]);
}

/// A plain action: its label, an optional leading icon, and whether it reads as danger.
#[derive(Clone)]
pub struct MenuEntry {
    pub label: SharedString,
    pub icon: Option<Icon>,
    pub danger: bool,
    pub on_select: Handler,
}

/// One row of a menu.
#[derive(Clone)]
pub enum MenuItem {
    Action(MenuEntry),
    /// A choice of a radio group, checked when it is the picked one.
    Radio {
        label: SharedString,
        checked: bool,
        on_select: Handler,
    },
    /// A choice among many, checked while it is on. Picking it toggles it and keeps the menu
    /// open.
    Check {
        label: SharedString,
        checked: bool,
        on_select: Handler,
    },
    Separator,
    Submenu {
        label: SharedString,
        icon: Option<Icon>,
        items: Vec<MenuItem>,
    },
}

impl MenuItem {
    pub fn action(label: impl Into<SharedString>, icon: Option<Icon>, on_select: Handler) -> Self {
        MenuItem::Action(MenuEntry {
            label: label.into(),
            icon,
            danger: false,
            on_select,
        })
    }

    /// [`Self::action`] in danger ink, for a removal or a stop.
    pub fn danger(label: impl Into<SharedString>, icon: Option<Icon>, on_select: Handler) -> Self {
        MenuItem::Action(MenuEntry {
            label: label.into(),
            icon,
            danger: true,
            on_select,
        })
    }

    pub fn radio(label: impl Into<SharedString>, checked: bool, on_select: Handler) -> Self {
        MenuItem::Radio {
            label: label.into(),
            checked,
            on_select,
        }
    }

    pub fn check(label: impl Into<SharedString>, checked: bool, on_select: Handler) -> Self {
        MenuItem::Check {
            label: label.into(),
            checked,
            on_select,
        }
    }

    pub fn submenu(
        label: impl Into<SharedString>,
        icon: Option<Icon>,
        items: Vec<MenuItem>,
    ) -> Self {
        MenuItem::Submenu {
            label: label.into(),
            icon,
            items,
        }
    }

    fn selectable(&self) -> bool {
        !matches!(self, MenuItem::Separator)
    }
}

/// The default width of a panel.
const MIN_WIDTH: f32 = 180.0;
/// How far a submenu overlaps its parent panel's edge.
const SUBMENU_GAP: f32 = 4.0;

pub struct Menu {
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    focus: FocusHandle,
    open: bool,
    took_focus: bool,
    progress: SpringValue,
    animator: Animator,
    items: Vec<MenuItem>,
    width: f32,
    highlighted: Option<usize>,
    /// The open submenu (an index into `items`) and the highlight inside it.
    submenu: Option<usize>,
    sub_highlighted: Option<usize>,
    /// Whether the keys move inside the open submenu.
    in_submenu: bool,
    /// Where the open submenu was drawn. It hangs past the menu's own bounds, so a press on
    /// it is not a press outside the menu.
    sub_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The pointer's way to the open submenu: a row it crosses waits for it to rest.
    aim: Rc<RefCell<MenuAim>>,
    /// Counts the waits, so a newer hover drops an older one.
    pending: u64,
}

impl Menu {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            bounds: Rc::new(Cell::new(None)),
            focus: cx.focus_handle(),
            open: false,
            took_focus: false,
            progress: SpringValue::new(0.0, SpringTier::Fast),
            animator: Animator::new(),
            items: Vec::new(),
            width: MIN_WIDTH,
            highlighted: None,
            submenu: None,
            sub_highlighted: None,
            in_submenu: false,
            sub_bounds: Rc::new(Cell::new(None)),
            aim: Rc::new(RefCell::new(MenuAim::default())),
            pending: 0,
        }
    }

    /// Plain actions.
    pub fn set_items(&mut self, items: Vec<MenuEntry>) {
        self.set_menu(items.into_iter().map(MenuItem::Action).collect());
    }

    /// Any rows: actions, radio items, separators, and submenus.
    pub fn set_menu(&mut self, items: Vec<MenuItem>) {
        self.items = items;
        self.highlighted = None;
        self.close_submenu();
    }

    /// The panel's width, such as a `min-w-*` the Solid menu set; 0 or less goes back to the
    /// default 180px.
    pub fn set_width(&mut self, width: f32) {
        self.width = if width > 0.0 { width } else { MIN_WIDTH };
    }

    pub fn set_open(&mut self, open: bool) {
        self.open = open;
        self.progress.set_target(if open { 1.0 } else { 0.0 });
        if !open {
            self.took_focus = false;
            self.highlighted = None;
            self.close_submenu();
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The root panel's last drawn bounds, for an owning popover's outside-press handling.
    pub fn bounds(&self) -> Option<Bounds<Pixels>> {
        self.open.then(|| self.bounds.get()).flatten()
    }

    /// The plain actions it holds, in order, as a test or a host reads them.
    pub fn entries(&self) -> Vec<MenuEntry> {
        self.items
            .iter()
            .filter_map(|item| match item {
                MenuItem::Action(entry) => Some(entry.clone()),
                _ => None,
            })
            .collect()
    }

    /// Lights row `index`, opening it when it is a submenu, as the pointer resting on it does.
    pub fn highlight(&mut self, index: usize) {
        self.in_submenu = false;
        self.highlighted = Some(index);
        if matches!(self.items.get(index), Some(MenuItem::Submenu { .. })) {
            self.open_submenu(index, false);
        } else {
            self.close_submenu();
        }
    }

    fn close_submenu(&mut self) {
        self.submenu = None;
        self.sub_highlighted = None;
        self.in_submenu = false;
        self.sub_bounds.set(None);
    }

    /// The pointer on top row `index`: it lights, and opens its submenu or closes the open
    /// one, unless the pointer is only crossing it on its way to the open submenu; then it
    /// waits until the pointer rests or turns away.
    fn hover_top(&mut self, index: usize, is_submenu: bool, cx: &mut Context<Self>) {
        self.pending += 1;
        let crossing = self.submenu.is_some_and(|open| open != index) && {
            let mut aim = self.aim.borrow_mut();
            aim.set_target(self.sub_bounds.get());
            aim.aiming()
        };
        if !crossing {
            self.take_row(index, is_submenu);
            return;
        }
        let pending = self.pending;
        let moves = self.aim.borrow().moves();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(AIM_DELAY).await;
            let _ = this.update(cx, |this, cx| {
                if this.pending != pending || !this.open {
                    return;
                }
                let aim = this.aim.borrow();
                let on_its_way = aim.moves() != moves && aim.aiming();
                drop(aim);
                if on_its_way {
                    this.hover_top(index, is_submenu, cx);
                } else {
                    this.take_row(index, is_submenu);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn take_row(&mut self, index: usize, is_submenu: bool) {
        self.highlighted = Some(index);
        self.in_submenu = false;
        if is_submenu {
            self.open_submenu(index, false);
        } else {
            self.close_submenu();
        }
    }

    fn submenu_items(&self) -> &[MenuItem] {
        match self.submenu.and_then(|index| self.items.get(index)) {
            Some(MenuItem::Submenu { items, .. }) => items,
            _ => &[],
        }
    }

    fn step(&mut self, step: isize) {
        if self.in_submenu {
            let next = next_selectable(self.submenu_items(), self.sub_highlighted, step);
            self.sub_highlighted = next;
        } else {
            self.highlighted = next_selectable(&self.items, self.highlighted, step);
            self.close_submenu();
        }
    }

    fn open_submenu(&mut self, index: usize, enter: bool) {
        if !matches!(self.items.get(index), Some(MenuItem::Submenu { .. })) {
            return;
        }
        self.highlighted = Some(index);
        if self.submenu != Some(index) {
            self.submenu = Some(index);
            self.sub_highlighted = None;
        }
        if enter {
            self.in_submenu = true;
            self.sub_highlighted = next_selectable(self.submenu_items(), None, 1);
        }
    }

    /// Runs `item`: an action or a radio choice runs and closes the menu; a submenu opens.
    ///
    /// The handler runs once this press is done with the menu, not inside it: a handler is
    /// free to update the menu (close it, refill it) or the view that owns it, which from
    /// inside the menu's own update would update an entity already being updated.
    fn run(&mut self, item: &MenuItem, window: &mut Window, cx: &mut Context<Self>) {
        let handler = match item {
            MenuItem::Action(entry) => entry.on_select.clone(),
            MenuItem::Radio { on_select, .. } => on_select.clone(),
            MenuItem::Check {
                label, on_select, ..
            } => {
                // A check item toggles in place: the menu stays open for the next one.
                let label = label.clone();
                let handler = on_select.clone();
                toggle_check(&mut self.items, &label);
                cx.notify();
                window.defer(cx, move |window, cx| handler(window, cx));
                return;
            }
            MenuItem::Separator | MenuItem::Submenu { .. } => return,
        };
        self.set_open(false);
        cx.notify();
        window.defer(cx, move |window, cx| handler(window, cx));
    }

    fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.in_submenu {
            if let Some(item) = self
                .sub_highlighted
                .and_then(|index| self.submenu_items().get(index))
                .cloned()
            {
                self.run(&item, window, cx);
            }
            return;
        }
        let Some(index) = self.highlighted else {
            return;
        };
        match self.items.get(index).cloned() {
            Some(MenuItem::Submenu { .. }) => self.open_submenu(index, true),
            Some(item) => self.run(&item, window, cx),
            None => {}
        }
    }

    fn panel(
        &self,
        items: &[MenuItem],
        level: Level,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let highlighted = match level {
            Level::Top => self.highlighted,
            Level::Sub => self.sub_highlighted,
        };
        let mut list = div()
            .relative()
            .w(px(self.width))
            .bg(theme.color_surface_subtle)
            .p(px(tw(1.5)))
            .flex()
            .flex_col()
            .gap(px(tw(1.0)))
            .text_size(px(12.0))
            .line_height(px(18.0));
        for (index, item) in items.iter().enumerate() {
            let lit = highlighted == Some(index);
            list = list.child(self.row(index, item, lit, level, theme, cx));
        }
        floating_panel(list, theme.radius_menu, theme)
    }

    fn row(
        &self,
        index: usize,
        item: &MenuItem,
        lit: bool,
        level: Level,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        if matches!(item, MenuItem::Separator) {
            return div()
                .my(px(tw(1.0)))
                .h(px(1.0))
                .w_full()
                .bg(theme.color_border_subtle)
                .into_any_element();
        }
        let (label, leading, trailing, danger, checked) = match item {
            MenuItem::Action(entry) => (
                entry.label.clone(),
                entry.icon.map(Lead::Icon),
                None,
                entry.danger,
                false,
            ),
            MenuItem::Radio { label, checked, .. } => (
                label.clone(),
                Some(Lead::Radio(*checked)),
                None,
                false,
                *checked,
            ),
            MenuItem::Check { label, checked, .. } => (
                label.clone(),
                Some(Lead::Check(*checked)),
                None,
                false,
                *checked,
            ),
            MenuItem::Submenu { label, icon, .. } => (
                label.clone(),
                icon.map(Lead::Icon),
                Some(Icon::ChevronRight),
                false,
                false,
            ),
            MenuItem::Separator => unreachable!(),
        };
        let ink = if danger {
            theme.color_danger
        } else if lit || checked {
            theme.color_text_primary
        } else {
            theme.color_text_secondary
        };
        let id = match level {
            Level::Top => ("menu-item", index),
            Level::Sub => ("menu-sub-item", index),
        };
        let is_submenu = matches!(item, MenuItem::Submenu { .. });
        let item = item.clone();
        // Tests find a row as `menu-item-<index>` (`menu-sub-item-<index>` in a submenu).
        let selector = format!("{}-{}", id.0, id.1);
        let mut row = div()
            .id(id)
            .debug_selector(move || selector)
            .relative()
            .w_full()
            .px(px(tw(2.5)))
            .py(px(tw(2.0)))
            .flex()
            .items_center()
            .gap(px(theme.space_2))
            .rounded(px(theme.radius_item))
            .cursor_pointer()
            .text_color(ink)
            .when(lit, |row| row.bg(theme.color_surface_hover))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if !*hovered {
                    return;
                }
                match level {
                    Level::Top => this.hover_top(index, is_submenu, cx),
                    Level::Sub => {
                        this.pending += 1;
                        this.in_submenu = true;
                        this.sub_highlighted = Some(index);
                    }
                }
                cx.notify();
            }))
            .on_click(cx.listener(move |this, _, window, cx| {
                if let MenuItem::Submenu { .. } = item {
                    this.open_submenu(index, false);
                    cx.notify();
                } else {
                    this.run(&item, window, cx);
                }
            }));
        match leading {
            Some(Lead::Icon(name)) => row = row.child(icon_colored(name, 14.0, ink)),
            Some(Lead::Radio(on)) => row = row.child(radio_mark(on, 14.0, theme)),
            Some(Lead::Check(on)) => row = row.child(checkbox_mark(on, 14.0, theme)),
            None => {}
        }
        row = row.child(div().flex_1().min_w(px(0.0)).truncate().child(label));
        if let Some(name) = trailing {
            row = row.child(div().size(px(14.0)).flex_none().child(icon(name, 14.0, cx)));
        }
        if level == Level::Top && is_submenu && self.submenu == Some(index) {
            let sub = self.panel(self.submenu_items(), Level::Sub, theme, cx);
            let seen = self.sub_bounds.clone();
            row = row.child(
                div()
                    .absolute()
                    .top(px(-tw(1.5)))
                    .left(relative(1.0))
                    .ml(px(tw(1.5) + SUBMENU_GAP))
                    .child(sub)
                    .child(
                        canvas(move |bounds, _, _| seen.set(Some(bounds)), |_, _, _, _| {})
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full(),
                    ),
            );
        }
        row.into_any_element()
    }
}

/// What leads a row: its icon, or a choice's control.
#[derive(Clone, Copy)]
enum Lead {
    Icon(Icon),
    Radio(bool),
    Check(bool),
}

/// Flips the check item named `label`, at the top or in a submenu, as its handler flips the
/// value it shows.
fn toggle_check(items: &mut [MenuItem], label: &SharedString) {
    for item in items {
        match item {
            MenuItem::Check {
                label: name,
                checked,
                ..
            } if name == label => *checked = !*checked,
            MenuItem::Submenu { items, .. } => toggle_check(items, label),
            _ => {}
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Level {
    Top,
    Sub,
}

/// The next row `step` away from `from` that is not a separator, wrapping; from nothing, the
/// first going down and the last going up.
fn next_selectable(items: &[MenuItem], from: Option<usize>, step: isize) -> Option<usize> {
    let len = items.len();
    if len == 0 {
        return None;
    }
    let mut next = from.unwrap_or(if step >= 0 { len - 1 } else { 0 });
    for _ in 0..len {
        next = (next as isize + step).rem_euclid(len as isize) as usize;
        if items[next].selectable() {
            return Some(next);
        }
    }
    None
}

impl Focusable for Menu {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Menu {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let visible = self.open || !self.progress.settled();
        let dt = self
            .animator
            .dt(window, visible && !self.progress.settled());
        if dt > 0.0 {
            self.progress.step(dt);
        }
        if !visible {
            return div().into_any_element();
        }
        if self.open && !self.took_focus {
            self.focus.focus(window);
            self.took_focus = true;
        }
        let theme = *Theme::global(cx);
        let t = self.progress.value().clamp(0.0, 1.0);
        let items = self.items.clone();
        let bounds = self.bounds.clone();
        let panel = div()
            .relative()
            .child(
                canvas(move |rect, _, _| bounds.set(Some(rect)), |_, _, _, _| {})
                    .absolute()
                    .inset_0(),
            )
            .occlude()
            .key_context("Menu")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Dismiss, _, cx| {
                if this.in_submenu || this.submenu.is_some() {
                    this.close_submenu();
                } else {
                    this.set_open(false);
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &SelectPrev, _, cx| {
                this.step(-1);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &SelectNext, _, cx| {
                this.step(1);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Confirm, window, cx| this.confirm(window, cx)))
            .on_action(cx.listener(|this, _: &OpenSubmenu, _, cx| {
                if let Some(index) = this.highlighted
                    && !this.in_submenu
                {
                    this.open_submenu(index, true);
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &CloseSubmenu, _, cx| {
                if this.in_submenu {
                    this.close_submenu();
                    cx.notify();
                }
            }))
            .on_mouse_down_out(cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                let on_submenu = this.submenu.is_some()
                    && this
                        .sub_bounds
                        .get()
                        .is_some_and(|bounds| bounds.contains(&event.position));
                if this.open && !on_submenu {
                    this.set_open(false);
                    cx.notify();
                }
            }))
            .opacity(t)
            .child(self.panel(&items, Level::Top, &theme, cx))
            .when(self.submenu.is_some(), |panel| {
                let aim = self.aim.clone();
                panel.child(
                    canvas(
                        |_, _, _| {},
                        move |_, _, window, _| {
                            let aim = aim.clone();
                            window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, _| {
                                if phase == DispatchPhase::Capture {
                                    aim.borrow_mut().moved(event.position);
                                }
                            });
                        },
                    )
                    .absolute()
                    .size_0(),
                )
            });
        hang(theme.control_height_md, (1.0 - t) * SLIDE, panel).into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use gpui::{Modifiers, TestAppContext, div, point, prelude::*, px, size};

    use std::time::Duration;

    use super::{Menu, MenuEntry, MenuItem, next_selectable};
    use crate::aim::AIM_DELAY;
    use crate::test_support::{draw, open, settle};
    use crate::theme::{Theme, tw};

    fn noop() -> crate::Handler {
        Rc::new(|_, _| {})
    }

    #[gpui::test]
    fn choosing_an_item_runs_it_and_closes_the_menu(cx: &mut TestAppContext) {
        let chosen = Rc::new(Cell::new(0));
        let menu = cx.new(Menu::new);
        let sink = chosen.clone();
        menu.update(cx, |menu, _| {
            menu.set_items(vec![MenuEntry {
                label: "Open".into(),
                icon: None,
                danger: false,
                on_select: Rc::new(move |_, _| sink.set(sink.get() + 1)),
            }]);
        });
        let host = menu.clone();
        let mut cx = open(cx, size(600.0, 400.0), move |_, _| {
            div()
                .relative()
                .size_full()
                .child(
                    div()
                        .absolute()
                        .left(px(40.0))
                        .top(px(40.0))
                        .w(px(120.0))
                        .h(px(32.0))
                        .child(host.clone()),
                )
                .into_any_element()
        });
        cx.update(|_, cx| {
            menu.update(cx, |menu, cx| {
                menu.set_open(true);
                cx.notify();
            })
        });
        settle(&mut cx);
        // Under the trigger: past the panel's padding, mid the first row.
        let theme = cx.update(|_, cx| *Theme::global(cx));
        let y = 40.0 + theme.control_height_md + tw(1.5) + 10.0;
        cx.simulate_click(point(px(60.0), px(y)), Modifiers::none());
        assert_eq!(chosen.get(), 1);
        assert!(!cx.update(|_, cx| menu.read(cx).is_open()));
    }

    #[gpui::test]
    fn a_row_crossed_on_the_way_to_the_submenu_waits_for_the_pointer_to_rest(
        cx: &mut TestAppContext,
    ) {
        let menu = cx.new(Menu::new);
        menu.update(cx, |menu, _| {
            menu.set_menu(vec![
                MenuItem::submenu(
                    "Group by",
                    None,
                    vec![
                        MenuItem::radio("Day", true, noop()),
                        MenuItem::radio("Week", false, noop()),
                        MenuItem::radio("Model", false, noop()),
                    ],
                ),
                MenuItem::action("Reset", None, noop()),
                MenuItem::action("View all", None, noop()),
            ]);
        });
        let host = menu.clone();
        let mut cx = open(cx, size(700.0, 500.0), move |_, _| {
            div()
                .relative()
                .size_full()
                .child(
                    div()
                        .absolute()
                        .left(px(40.0))
                        .top(px(40.0))
                        .w(px(120.0))
                        .h(px(32.0))
                        .child(host.clone()),
                )
                .into_any_element()
        });
        cx.update(|_, cx| {
            menu.update(cx, |menu, cx| {
                menu.set_open(true);
                cx.notify();
            })
        });
        settle(&mut cx);
        let theme = cx.update(|_, cx| *Theme::global(cx));
        let first = 40.0 + theme.control_height_md + tw(1.5) + 10.0;
        let submenu = |cx: &mut gpui::VisualTestContext| cx.update(|_, cx| menu.read(cx).submenu);
        // Resting on Group by opens its submenu.
        cx.simulate_mouse_move(point(px(60.0), px(first)), None, Modifiers::none());
        settle(&mut cx);
        assert_eq!(submenu(&mut cx), Some(0));
        let sub = cx
            .update(|_, cx| menu.read(cx).sub_bounds.get())
            .expect("the submenu was drawn");
        let pitch = 32.0;
        // On the way to the submenu's lower rows the pointer crosses Reset: it stays open.
        cx.simulate_mouse_move(point(px(70.0), px(first + 2.0)), None, Modifiers::none());
        cx.simulate_mouse_move(point(px(150.0), px(first + pitch)), None, Modifiers::none());
        draw(&mut cx);
        assert!(
            f32::from(sub.left()) > 150.0,
            "the submenu opens to the right"
        );
        assert_eq!(submenu(&mut cx), Some(0), "crossing Reset keeps it open");
        // Resting on Reset, it takes over once the wait is out.
        cx.executor()
            .advance_clock(AIM_DELAY + Duration::from_millis(20));
        cx.run_until_parked();
        assert_eq!(submenu(&mut cx), None);
        assert_eq!(cx.update(|_, cx| menu.read(cx).highlighted), Some(1));
    }

    #[test]
    fn the_highlight_wraps_and_passes_separators_by() {
        let items = vec![
            MenuItem::radio("A", true, noop()),
            MenuItem::Separator,
            MenuItem::radio("B", false, noop()),
        ];
        assert_eq!(next_selectable(&items, None, 1), Some(0));
        assert_eq!(next_selectable(&items, Some(0), 1), Some(2));
        assert_eq!(next_selectable(&items, Some(2), 1), Some(0));
        assert_eq!(next_selectable(&items, None, -1), Some(2));
        assert_eq!(next_selectable(&[MenuItem::Separator], None, 1), None);
    }

    #[gpui::test]
    fn an_item_may_update_its_own_menu(cx: &mut TestAppContext) {
        // A handler that closes or refills the menu it was picked from, as an app's "New
        // section" closes the menu before it opens its editor.
        let menu = cx.new(Menu::new);
        let ran = Rc::new(Cell::new(0));
        let sink = ran.clone();
        let weak = menu.downgrade();
        menu.update(cx, |menu, _| {
            menu.set_items(vec![MenuEntry {
                label: "Open".into(),
                icon: None,
                danger: false,
                on_select: Rc::new(move |_, cx| {
                    sink.set(sink.get() + 1);
                    let _ = weak.update(cx, |menu, cx| {
                        menu.set_items(Vec::new());
                        menu.set_open(false);
                        cx.notify();
                    });
                }),
            }]);
        });
        let host = menu.clone();
        let mut cx = open(cx, size(600.0, 400.0), move |_, _| {
            div()
                .relative()
                .size_full()
                .child(
                    div()
                        .absolute()
                        .left(px(40.0))
                        .top(px(40.0))
                        .w(px(120.0))
                        .h(px(32.0))
                        .child(host.clone()),
                )
                .into_any_element()
        });
        cx.update(|_, cx| {
            menu.update(cx, |menu, cx| {
                menu.set_open(true);
                cx.notify();
            })
        });
        settle(&mut cx);
        let theme = cx.update(|_, cx| *Theme::global(cx));
        let y = 40.0 + theme.control_height_md + tw(1.5) + 10.0;
        cx.simulate_click(point(px(60.0), px(y)), Modifiers::none());
        settle(&mut cx);
        assert_eq!(ran.get(), 1);
        assert!(!cx.update(|_, cx| menu.read(cx).is_open()));
    }

    #[gpui::test]
    fn a_click_on_a_submenu_never_reaches_the_page_under_it(cx: &mut TestAppContext) {
        let chosen = Rc::new(Cell::new(0));
        let behind = Rc::new(Cell::new(0));
        let menu = cx.new(Menu::new);
        let sink = chosen.clone();
        menu.update(cx, |menu, _| {
            menu.set_menu(vec![MenuItem::submenu(
                "Group by",
                None,
                vec![MenuItem::radio(
                    "Agent",
                    false,
                    Rc::new(move |_, _| sink.set(sink.get() + 1)),
                )],
            )]);
        });
        let host = menu.clone();
        let page = behind.clone();
        let mut cx = open(cx, size(800.0, 400.0), move |_, _| {
            let page = page.clone();
            div()
                .relative()
                .size_full()
                // A row of the page, under where the submenu opens.
                .child(
                    div()
                        .id("page-row")
                        .absolute()
                        .left(px(0.0))
                        .top(px(0.0))
                        .size_full()
                        .on_click(move |_, _, _| page.set(page.get() + 1)),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(40.0))
                        .top(px(40.0))
                        .w(px(120.0))
                        .h(px(32.0))
                        .child(host.clone()),
                )
                .into_any_element()
        });
        cx.update(|_, cx| {
            menu.update(cx, |menu, cx| {
                menu.set_open(true);
                menu.open_submenu(0, false);
                cx.notify();
            })
        });
        settle(&mut cx);
        // Beside the menu, mid the submenu's first row, which lines up with its parent row.
        let theme = cx.update(|_, cx| *Theme::global(cx));
        let y = 40.0 + theme.control_height_md + tw(1.5) + 10.0;
        let x = 40.0 + super::MIN_WIDTH + super::SUBMENU_GAP + 60.0;
        cx.simulate_click(point(px(x), px(y)), Modifiers::none());
        assert_eq!(chosen.get(), 1);
        assert_eq!(behind.get(), 0);
    }

    #[gpui::test]
    fn right_opens_a_submenu_on_its_first_row_and_left_closes_it(cx: &mut TestAppContext) {
        let menu = cx.new(Menu::new);
        menu.update(cx, |menu, _| {
            menu.set_menu(vec![
                MenuItem::submenu(
                    "Group by",
                    None,
                    vec![MenuItem::radio("Agent", false, noop())],
                ),
                MenuItem::action("Reset", None, noop()),
            ]);
            menu.step(1);
            assert_eq!(menu.highlighted, Some(0));
            menu.open_submenu(0, true);
            assert_eq!(menu.submenu, Some(0));
            assert_eq!(menu.sub_highlighted, Some(0));
            assert!(menu.in_submenu);
            menu.close_submenu();
            assert_eq!(menu.submenu, None);
            menu.step(1);
            assert_eq!(menu.highlighted, Some(1));
        });
    }
}
