//! `DataTable.SelectionToolbar` (`table.css`): while rows are picked, a toolbar floats 12px
//! over the bottom of the table, centered: "N selected" in 11px/600 primary, the actions every
//! picked row takes (6px from the summary, 4px apart), or, when none fits all of them, a muted
//! note, and Clear selection, a 28px muted X that turns primary on a hover fill. The bar is
//! at least 40px tall on `--color-surface-raised` at `--radius-surface`, padded 6px 8px 6px
//! 12px.

use gpui::{AnyElement, App, FontWeight, IntoElement, SharedString, div, prelude::*, px};

use crate::Handler;
use crate::icon_button::icon_button;
use crate::icons::Icon;
use crate::material::float_drop;
use crate::theme::Theme;

/// The bar for `count` picked rows. Place it last in a `relative()` box over the table: it
/// fills the box's bottom edge, 12px in, and takes only the presses on itself.
pub fn selection_toolbar(
    count: usize,
    actions: Vec<AnyElement>,
    empty_note: impl Into<SharedString>,
    on_clear: Handler,
    cx: &App,
) -> AnyElement {
    let theme = Theme::global(cx);
    let body: AnyElement = if actions.is_empty() {
        div()
            .text_size(px(11.0))
            .text_color(theme.color_text_muted)
            .whitespace_nowrap()
            .child(empty_note.into())
            .into_any_element()
    } else {
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_center()
            .gap(px(4.0))
            .children(actions)
            .into_any_element()
    };
    let bar = div()
        .id("selection-toolbar")
        .occlude()
        .min_h(px(40.0))
        .max_w(px(920.0))
        .flex()
        .flex_wrap()
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .pt(px(6.0))
        .pb(px(6.0))
        .pl(px(12.0))
        .pr(px(8.0))
        .rounded(px(theme.radius_surface))
        .bg(theme.color_surface_raised)
        .shadow(float_drop(theme))
        .text_color(theme.color_text_secondary)
        .child(
            div()
                .flex_none()
                .pr(px(4.0))
                .text_size(px(11.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.color_text_primary)
                .whitespace_nowrap()
                .child(SharedString::from(format!("{count} selected"))),
        )
        .child(body)
        .child(icon_button(
            "selection-clear",
            Icon::Close,
            "Clear selection",
            on_clear,
            cx,
        ));
    div()
        .absolute()
        .left(px(12.0))
        .right(px(12.0))
        .bottom(px(12.0))
        .flex()
        .justify_center()
        .child(bar)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use gpui::{Modifiers, TestAppContext, point, prelude::*, px, size};

    use super::selection_toolbar;
    use crate::test_support::{draw, open};

    #[gpui::test]
    fn the_bar_sits_12px_over_the_bottom_and_keeps_its_presses(cx: &mut TestAppContext) {
        let cleared = Rc::new(Cell::new(0));
        let pressed = Rc::new(Cell::new(0));
        let (clear, press) = (cleared.clone(), pressed.clone());
        let mut cx = open(cx, size(600.0, 300.0), move |_, cx| {
            let (clear, press) = (clear.clone(), press.clone());
            gpui::div()
                .id("page")
                .relative()
                .size_full()
                .on_click(move |_, _, _| press.set(press.get() + 1))
                .child(selection_toolbar(
                    2,
                    Vec::new(),
                    "No action fits every selected item",
                    Rc::new(move |_, _| clear.set(clear.get() + 1)),
                    cx,
                ))
                .into_any_element()
        });
        draw(&mut cx);
        let button = cx
            .debug_bounds("selection-clear")
            .expect("Clear selection draws");
        // The 40px bar ends 12px over the bottom; the 28px button sits 6px in.
        assert_eq!(f32::from(button.bottom()), 300.0 - 12.0 - 6.0);
        cx.simulate_click(button.center(), Modifiers::none());
        assert_eq!((cleared.get(), pressed.get()), (1, 0));
        cx.simulate_click(point(px(300.0), px(100.0)), Modifiers::none());
        assert_eq!(pressed.get(), 1);
    }
}
