//! `Popover` holding a `MenuList` (`popover.tsx`, `menu.tsx`, `menu-highlight.css`): the panel
//! a switcher or a short menu opens, such as Switch project or Help, as opposed to the
//! [`Menu`](crate::Menu) of a "…" button.
//!
//! It looks like the design system's: the `surface-panel-float` frame (the card fill,
//! `--radius-surface`, the quiet edge, the float drop and a 0 20px 56px
//! `--color-shadow-float` shadow) at its width, 6px from its trigger (`gutter: 6`). An
//! optional search leads it (`PopoverSearchField`: `px-3 py-2.5` over a
//! `--color-border-subtle` rule, a bare 13px field). The rows (`MenuList`, `p-2`) are
//! `MenuItem`s: `px-2.5 py-2`, `gap-2` between a leading icon or tile, the label in primary
//! ink, and, on the picked row, a 15px check in secondary ink; the picked row fills with
//! `--color-surface-selected` and the highlighted one with `--color-surface-hover`. A footer
//! (`border-t p-2`) holds more rows, such as New project, whose labels stay secondary. With
//! nothing to list it shows its empty line (`px-2 py-3`, 13px muted).
//!
//! It behaves like gpui-kit's `Popover` with a list: it springs in, takes the focus (its
//! search's, when it has one), ↑/↓ move the highlight over the rows and the footer, Enter
//! or a click runs a row and closes the panel, and Escape or a press outside closes it. It
//! hangs under its trigger, or over it when it does not fit under, through
//! [`window_anchor`](crate::patterns::window_anchor).

use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AnyElement, App, Context, Entity, FocusHandle, Focusable, IntoElement, KeyDownEvent, Render,
    SharedString, Window, div, prelude::*, px,
};

use crate::Handler;
use crate::hang::{SLIDE, hang};
use crate::icons::{Icon, icon_colored};
use crate::material::{drop, float_drop, floating_panel};
use crate::motion::{Animator, SpringTier, SpringValue};
use crate::text_input::TextInput;
use crate::theme::{Theme, tw};

/// The space between the panel and its trigger.
const GUTTER: f32 = 6.0;

/// What leads a row.
#[derive(Clone)]
pub enum RowLeading {
    /// A 14px icon in secondary ink.
    Icon(Icon),
    /// Any element, such as a project's folder tile.
    Element(Rc<dyn Fn(&App) -> AnyElement>),
}

/// One row: what leads it, its label, whether it is the picked one, and what it runs.
#[derive(Clone)]
pub struct ListRow {
    pub leading: Option<RowLeading>,
    pub label: SharedString,
    pub selected: bool,
    pub on_select: Handler,
}

impl ListRow {
    pub fn new(label: impl Into<SharedString>, on_select: Handler) -> Self {
        Self {
            leading: None,
            label: label.into(),
            selected: false,
            on_select,
        }
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.leading = Some(RowLeading::Icon(icon));
        self
    }

    pub fn leading(mut self, leading: impl Fn(&App) -> AnyElement + 'static) -> Self {
        self.leading = Some(RowLeading::Element(Rc::new(leading)));
        self
    }

    /// The picked row: the selected fill and the check at its end.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

pub struct ListPopover {
    focus: FocusHandle,
    open: bool,
    took_focus: bool,
    progress: SpringValue,
    animator: Animator,
    width: f32,
    text_size: f32,
    trigger_height: f32,
    search: Option<Entity<TextInput>>,
    rows: Vec<ListRow>,
    footer: Vec<ListRow>,
    empty: SharedString,
    /// Over the rows, then the footer's.
    highlighted: Option<usize>,
    /// How long a picked row stays on show before the panel closes.
    hold: Option<Duration>,
}

impl ListPopover {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            open: false,
            took_focus: false,
            progress: SpringValue::new(0.0, SpringTier::Fast),
            animator: Animator::new(),
            width: 320.0,
            text_size: 13.0,
            trigger_height: 28.0,
            search: None,
            rows: Vec::new(),
            footer: Vec::new(),
            empty: SharedString::default(),
            highlighted: None,
            hold: None,
        }
    }

    /// The panel's width (`width`, 320 by default).
    pub fn set_width(&mut self, width: f32) {
        self.width = width;
    }

    /// The height of the trigger it hangs from (28 by default).
    pub fn set_trigger_height(&mut self, height: f32) {
        self.trigger_height = height;
    }

    /// The rows' text size (`text-[13px]`, or the switchers' `text-[13.5px]`).
    pub fn set_text_size(&mut self, size: f32) {
        self.text_size = size;
    }

    /// The search field that leads the panel; the app filters the rows as it changes.
    pub fn set_search(&mut self, search: Option<Entity<TextInput>>) {
        self.search = search;
    }

    /// The rows. The highlight goes back to the first one.
    pub fn set_rows(&mut self, rows: Vec<ListRow>) {
        self.rows = rows;
        if self.highlighted.is_some() {
            self.highlighted = self.first_lit();
        }
    }

    pub fn set_footer(&mut self, footer: Vec<ListRow>) {
        self.footer = footer;
    }

    /// `SELECTION_ACK_MS`: a pick of one of the rows marks it and holds the panel open this
    /// long before it closes, so the check is seen to move. The footer's rows close at once.
    pub fn set_hold(&mut self, hold: Option<Duration>) {
        self.hold = hold;
    }

    /// The line shown when there are no rows, such as "No matching projects."
    pub fn set_empty(&mut self, empty: impl Into<SharedString>) {
        self.empty = empty.into();
    }

    /// Opening lights the first row, as the panel's first item takes the focus on the web.
    pub fn set_open(&mut self, open: bool) {
        self.open = open;
        self.progress.set_target(if open { 1.0 } else { 0.0 });
        self.took_focus = false;
        self.highlighted = if open { self.first_lit() } else { None };
    }

    /// Where the highlight starts: on the picked row, else the first.
    fn first_lit(&self) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| row.selected)
            .or((self.len() > 0).then_some(0))
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn highlighted(&self) -> Option<usize> {
        self.highlighted
    }

    fn len(&self) -> usize {
        self.rows.len() + self.footer.len()
    }

    fn row(&self, index: usize) -> Option<&ListRow> {
        self.rows.get(index).or_else(|| {
            index
                .checked_sub(self.rows.len())
                .and_then(|i| self.footer.get(i))
        })
    }

    fn step(&mut self, by: isize) {
        let len = self.len() as isize;
        if len == 0 {
            return;
        }
        let next = match self.highlighted {
            Some(index) => (index as isize + by).rem_euclid(len),
            None if by > 0 => 0,
            None => len - 1,
        };
        self.highlighted = Some(next as usize);
    }

    fn run(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(handler) = self.row(index).map(|row| row.on_select.clone()) else {
            return;
        };
        match self.hold.filter(|_| index < self.rows.len()) {
            Some(hold) => {
                for (at, row) in self.rows.iter_mut().enumerate() {
                    row.selected = at == index;
                }
                cx.notify();
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(hold).await;
                    let _ = this.update(cx, |this, cx| {
                        this.set_open(false);
                        cx.notify();
                    });
                })
                .detach();
            }
            None => {
                self.set_open(false);
                cx.notify();
            }
        }
        handler(window, cx);
    }

    fn row_element(
        &self,
        index: usize,
        row: &ListRow,
        footer: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = *Theme::global(cx);
        let lit = self.highlighted == Some(index);
        let ink = if footer && !lit {
            theme.color_text_secondary
        } else {
            theme.color_text_primary
        };
        let leading = row.leading.as_ref().map(|leading| match leading {
            RowLeading::Icon(icon) => {
                icon_colored(*icon, 14.0, theme.color_text_secondary).into_any_element()
            }
            RowLeading::Element(build) => build(cx),
        });
        let id = gpui::ElementId::NamedInteger("list-popover-row".into(), index as u64);
        div()
            .id(id.clone())
            .debug_selector(move || id.to_string())
            .relative()
            .w_full()
            .flex()
            .items_center()
            .gap(px(tw(2.0)))
            .px(px(tw(2.5)))
            .py(px(tw(2.0)))
            .rounded(px(theme.radius_item))
            .cursor_pointer()
            .text_size(px(self.text_size))
            .line_height(px(tw(1.0) * 6.0))
            .when(row.selected, |row| row.bg(theme.color_surface_selected))
            .when(lit && !row.selected, |row| {
                row.bg(theme.color_surface_hover)
            })
            .on_mouse_move(cx.listener(move |this, _, _, cx| {
                if this.highlighted != Some(index) {
                    this.highlighted = Some(index);
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _, window, cx| this.run(index, window, cx)))
            .children(leading.map(|leading| div().flex_none().child(leading)))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_color(ink)
                    .child(row.label.clone()),
            )
            .when(row.selected, |el| {
                el.child(div().flex_none().child(icon_colored(
                    Icon::Check,
                    15.0,
                    theme.color_text_secondary,
                )))
            })
            .into_any_element()
    }
}

impl Focusable for ListPopover {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for ListPopover {
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
            match &self.search {
                Some(search) => search.read(cx).focus_handle(cx).focus(window),
                None => self.focus.focus(window),
            }
            self.took_focus = true;
        }
        let theme = *Theme::global(cx);
        let t = self.progress.value().clamp(0.0, 1.0);

        let rows: Vec<AnyElement> = self
            .rows
            .clone()
            .iter()
            .enumerate()
            .map(|(index, row)| self.row_element(index, row, false, cx))
            .collect();
        let offset = self.rows.len();
        let footer: Vec<AnyElement> = self
            .footer
            .clone()
            .iter()
            .enumerate()
            .map(|(index, row)| self.row_element(offset + index, row, true, cx))
            .collect();
        let empty = rows.is_empty().then(|| {
            div()
                .px(px(tw(2.0)))
                .py(px(tw(3.0)))
                .text_size(px(13.0))
                .text_color(theme.color_text_muted)
                .child(self.empty.clone())
        });

        let mut surface = div()
            .relative()
            .w(px(self.width))
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(theme.color_surface_subtle)
            .text_color(theme.color_text_primary);
        if let Some(search) = &self.search {
            surface = surface.child(
                div()
                    .flex_none()
                    .px(px(tw(3.0)))
                    .py(px(tw(2.5)))
                    .border_b_1()
                    .border_color(theme.color_border_subtle)
                    .child(search.clone()),
            );
        }
        surface = surface.child(
            div()
                .flex()
                .flex_col()
                .p(px(tw(2.0)))
                .children(rows)
                .children(empty),
        );
        if !footer.is_empty() {
            surface = surface.child(
                div()
                    .flex()
                    .flex_col()
                    .p(px(tw(2.0)))
                    .border_t_1()
                    .border_color(theme.color_border_subtle)
                    .children(footer),
            );
        }
        let mut surface = floating_panel(surface, theme.radius_surface, &theme);
        let mut shadows = float_drop(&theme);
        shadows.push(drop(20.0, 56.0, theme.color_shadow_float));
        surface = surface.shadow(shadows);

        let panel = div()
            .key_context("ListPopover")
            .track_focus(&self.focus)
            .py(px(GUTTER))
            .opacity(t)
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                match event.keystroke.key.as_str() {
                    "up" => this.step(-1),
                    "down" => this.step(1),
                    "enter" => {
                        if let Some(index) = this.highlighted {
                            this.run(index, window, cx);
                        }
                    }
                    "escape" => this.set_open(false),
                    _ => return,
                }
                cx.stop_propagation();
                cx.notify();
            }))
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                if this.open {
                    this.set_open(false);
                    cx.notify();
                }
            }))
            .child(surface);
        // The gutter is the panel's own padding, on the side it takes.
        hang(self.trigger_height, (1.0 - t) * SLIDE, panel).into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use gpui::{Modifiers, TestAppContext, div, prelude::*, px, size};

    use super::{ListPopover, ListRow};
    use crate::test_support::{draw, open, settle};
    use crate::theme::tw;

    #[gpui::test]
    fn a_row_runs_and_closes_the_panel_and_keys_move_the_highlight(cx: &mut TestAppContext) {
        let chosen = Rc::new(Cell::new(usize::MAX));
        let popover = cx.new(ListPopover::new);
        let rows = |sink: Rc<Cell<usize>>| {
            (0..3)
                .map(|index| {
                    let sink = sink.clone();
                    ListRow::new(format!("Row {index}"), Rc::new(move |_, _| sink.set(index)))
                })
                .collect::<Vec<_>>()
        };
        popover.update(cx, |popover, _| {
            popover.set_width(260.0);
            popover.set_rows(rows(chosen.clone()));
            popover.set_open(true);
        });
        assert_eq!(popover.read_with(cx, |p, _| p.highlighted()), Some(0));
        let host = popover.clone();
        let mut cx = open(cx, size(600.0, 400.0), move |_, _| {
            div()
                .relative()
                .size_full()
                .child(
                    div()
                        .absolute()
                        .left(px(40.0))
                        .top(px(40.0))
                        .w(px(32.0))
                        .h(px(28.0))
                        .child(host.clone()),
                )
                .into_any_element()
        });
        settle(&mut cx);
        cx.simulate_keystrokes("down down enter");
        assert_eq!(chosen.get(), 2);
        assert!(!popover.read_with(&cx, |p, _| p.is_open()));

        // A click on a row runs it.
        cx.update(|_, cx| popover.update(cx, |p, _| p.set_open(true)));
        settle(&mut cx);
        draw(&mut cx);
        let first = cx
            .debug_bounds("list-popover-row-0")
            .expect("the first row draws");
        // It hangs 6px under its 28px trigger, the list's `p-2` before the row.
        assert_eq!(f32::from(first.top()), 40.0 + 28.0 + 6.0 + tw(2.0));
        cx.simulate_click(first.center(), Modifiers::none());
        assert_eq!(chosen.get(), 0);
    }
}
