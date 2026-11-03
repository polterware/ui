//! Anchored menu. Opaque panel, spring in, Escape and click-outside close it.

use gpui::{
    App, Context, FocusHandle, Focusable, IntoElement, Render, SharedString, Window, div,
    prelude::*, px,
};

use super::Dismiss;
use super::hang::{SLIDE, hang};
use super::icons::{Icon, icon};
use crate::Handler;
use crate::motion::{Animator, SpringTier, SpringValue};
use crate::theme::Theme;

#[derive(Clone)]
pub struct MenuEntry {
    pub label: SharedString,
    pub icon: Option<Icon>,
    pub danger: bool,
    pub on_select: Handler,
}

pub struct Menu {
    focus: FocusHandle,
    open: bool,
    took_focus: bool,
    progress: SpringValue,
    animator: Animator,
    items: Vec<MenuEntry>,
}

impl Menu {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            open: false,
            took_focus: false,
            progress: SpringValue::new(0.0, SpringTier::Fast),
            animator: Animator::new(),
            items: Vec::new(),
        }
    }

    pub fn set_items(&mut self, items: Vec<MenuEntry>) {
        self.items = items;
    }

    pub fn set_open(&mut self, open: bool) {
        self.open = open;
        self.progress.set_target(if open { 1.0 } else { 0.0 });
        if !open {
            self.took_focus = false;
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }
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
        let mut list = div().flex().flex_col().py(px(theme.space_1));
        for (index, item) in self.items.iter().enumerate() {
            let handler = item.on_select.clone();
            let fg = if item.danger {
                theme.color_danger
            } else {
                theme.color_text_primary
            };
            let mut row = div()
                .id(("menu-item", index))
                .h(px(theme.control_height_md))
                .px(px(theme.space_3))
                .flex()
                .items_center()
                .gap(px(theme.space_2))
                .rounded(px(theme.radius_sm))
                .cursor_pointer()
                .hover(|style| style.bg(theme.color_surface_hover))
                .on_click(cx.listener(move |this, _, window, cx| {
                    handler(window, cx);
                    this.set_open(false);
                    cx.notify();
                }));
            if let Some(name) = item.icon {
                row = row.child(icon(name, 14.0, cx));
            }
            row = row.child(
                div()
                    .text_size(px(theme.font_size_sm))
                    .text_color(fg)
                    .child(item.label.clone()),
            );
            list = list.child(row);
        }
        let panel = div()
            .occlude()
            .key_context("Menu")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Dismiss, _, cx| {
                this.set_open(false);
                cx.notify();
            }))
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                if this.open {
                    this.set_open(false);
                    cx.notify();
                }
            }))
            .opacity(t)
            .child(
                div()
                    .min_w(px(180.0))
                    .rounded(px(theme.radius_lg))
                    .bg(theme.color_surface_raised)
                    .border_1()
                    .border_color(theme.color_border)
                    .child(list),
            );
        hang(theme.control_height_md, (1.0 - t) * SLIDE, panel).into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use gpui::{Modifiers, TestAppContext, div, point, prelude::*, px, size};

    use super::{Menu, MenuEntry};
    use crate::test_support::{open, settle};
    use crate::theme::Theme;

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
        // Under the trigger: past the panel's border and the list's top padding, mid item.
        let theme = cx.update(|_, cx| *Theme::global(cx));
        let y =
            40.0 + theme.control_height_md + 1.0 + theme.space_1 + theme.control_height_md / 2.0;
        cx.simulate_click(point(px(60.0), px(y)), Modifiers::none());
        assert_eq!(chosen.get(), 1);
        assert!(!cx.update(|_, cx| menu.read(cx).is_open()));
    }
}
