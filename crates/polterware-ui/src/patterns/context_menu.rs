//! `ContextMenu` (`ContextMenu.Root` with `anchorPoint` in the design system): a [`Menu`] that
//! opens where the pointer pressed, such as a right click on a table row, kept inside the
//! window. It looks like the [`Menu`] it holds.
//!
//! It behaves like gpui-kit's `ContextMenu`: opening it again at another point moves it there
//! with its new items; it closes on Escape, on a press outside, or once an item runs; and the
//! app learns what it acts on from the items it built, so nothing else needs to remember the
//! target. It sits in window coordinates wherever it is in the tree, and snaps inside the
//! window's edges; the app puts the entity last in its root (or defers it) so it draws over
//! everything, since the menu never defers itself.

use gpui::{
    AnchoredPositionMode, App, Context, Entity, IntoElement, Pixels, Point, Render, Window,
    anchored, div, point, prelude::*, px,
};

use crate::menu::{Menu, MenuItem};
use crate::theme::tw;

/// The space kept between the menu and the window's edges.
const EDGE: f32 = 8.0;
/// A menu row: 12px text on an 18px line, padded `py-2`.
fn row_height() -> f32 {
    18.0 + 2.0 * tw(2.0)
}

/// About how tall the menu of `items` draws, to keep it inside the window.
pub fn menu_height(items: &[MenuItem]) -> f32 {
    let rows: f32 = items
        .iter()
        .map(|item| match item {
            MenuItem::Separator => 2.0 * tw(1.0) + 1.0,
            _ => row_height(),
        })
        .sum();
    2.0 * tw(1.5) + rows + tw(1.0) * items.len().saturating_sub(1) as f32
}

/// Where a menu `size` big opens for a press at `at` in a window `window` big: at the point,
/// moved left or up as far as it needs to stay inside, never past the top-left edge.
pub fn place(at: (f32, f32), size: (f32, f32), window: (f32, f32)) -> (f32, f32) {
    let clamp = |at: f32, size: f32, room: f32| at.min(room - size - EDGE).max(EDGE.min(at));
    (clamp(at.0, size.0, window.0), clamp(at.1, size.1, window.1))
}

pub struct ContextMenu {
    menu: Entity<Menu>,
    at: (f32, f32),
    width: f32,
}

impl ContextMenu {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            menu: cx.new(Menu::new),
            at: (0.0, 0.0),
            width: 200.0,
        }
    }

    /// The menu's width; 200px by default.
    pub fn set_width(&mut self, width: f32) {
        self.width = width;
    }

    /// Opens `items` at `at`, a window position such as a mouse event's.
    pub fn open(
        &mut self,
        at: Point<Pixels>,
        items: Vec<MenuItem>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let viewport = window.viewport_size();
        self.at = place(
            (f32::from(at.x), f32::from(at.y)),
            (self.width, menu_height(&items)),
            (f32::from(viewport.width), f32::from(viewport.height)),
        );
        let width = self.width;
        self.menu.update(cx, |menu, cx| {
            menu.set_menu(items);
            menu.set_width(width);
            menu.set_open(true);
            cx.notify();
        });
        cx.notify();
    }

    /// Opens `items` under `trigger` (a button's window bounds) with their right edges lined
    /// up, 4px below it, as a `DropdownMenu` placed `bottom-end` opens.
    pub fn open_under(
        &mut self,
        trigger: gpui::Bounds<Pixels>,
        items: Vec<MenuItem>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let x = f32::from(trigger.origin.x + trigger.size.width) - self.width;
        let y = f32::from(trigger.origin.y + trigger.size.height) + 4.0;
        self.open(point(px(x), px(y)), items, window, cx);
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.menu.update(cx, |menu, cx| {
            if menu.is_open() {
                menu.set_open(false);
                cx.notify();
            }
        });
    }

    pub fn is_open(&self, cx: &App) -> bool {
        self.menu.read(cx).is_open()
    }

    /// Where it opened, for an app that marks what it acts on.
    pub fn position(&self) -> (f32, f32) {
        self.at
    }
}

impl Render for ContextMenu {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        anchored()
            .position_mode(AnchoredPositionMode::Window)
            .position(point(px(self.at.0), px(self.at.1)))
            .snap_to_window_with_margin(px(EDGE))
            .child(div().relative().child(self.menu.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Handler;
    use std::rc::Rc;

    #[test]
    fn the_menu_stays_inside_the_window() {
        let window = (800.0, 600.0);
        assert_eq!(
            place((100.0, 100.0), (200.0, 120.0), window),
            (100.0, 100.0)
        );
        assert_eq!(
            place((700.0, 100.0), (200.0, 120.0), window),
            (592.0, 100.0)
        );
        assert_eq!(
            place((100.0, 560.0), (200.0, 120.0), window),
            (100.0, 472.0)
        );
        // Bigger than the window: pinned to the top-left edge.
        assert_eq!(place((50.0, 50.0), (900.0, 700.0), window), (8.0, 8.0));
        assert_eq!(place((2.0, 2.0), (100.0, 100.0), window), (2.0, 2.0));
    }

    #[test]
    fn its_height_counts_rows_and_separators() {
        let noop: Handler = Rc::new(|_, _| {});
        let items = vec![
            MenuItem::action("Copy", None, noop.clone()),
            MenuItem::Separator,
            MenuItem::action("Kill", None, noop),
        ];
        let two_rows = menu_height(&items[..1]) + row_height();
        assert!(menu_height(&items) > two_rows);
    }
}
