//! A start page (Vitals' `HostHome`): the mascot over the main action, a button the column's
//! width, and the lists to reopen something from, in a 320px column 20px apart, given as
//! `PageEmpty`'s action (`mt-2`) and 12px lower again.
//!
//! The look is the old Solid app's: a [`start_section`] is its 12px muted label, left-aligned
//! (`0 8px 4px` in), over rows 2px apart; a [`StartRow`] is a 32px ghost button the column's
//! width, a 14px icon 8px before the title in primary ink, and its detail in 12px muted ink at
//! the row's end, with a 28px "…" at its right that shows while the pointer is on the row (and
//! while its menu is open), over the row's hover fill.
//!
//! It behaves like the Solid row: a click opens the item, and the "…" hands the app its
//! bounds, so a [`ContextMenu`](super::ContextMenu) opens under it
//! ([`open_under`](super::ContextMenu::open_under)).

use std::rc::Rc;

use gpui::{
    AnyElement, App, Bounds, ElementId, IntoElement, Pixels, SharedString, Window, canvas, div,
    prelude::*, px,
};

use super::page_empty::MascotMood;
use crate::Handler;
use crate::ellipsis::fade;
use crate::icons::{Icon, icon_colored};
use crate::theme::{Theme, tw};

use super::page_empty::page_empty_action;

pub type MoreHandler = Rc<dyn Fn(Bounds<Pixels>, &mut Window, &mut App)>;

/// The page: the `mood` mascot over `body` (the main action, then the sections).
pub fn start_page(mood: MascotMood, body: Vec<AnyElement>, cx: &App) -> AnyElement {
    let column = div()
        .w(px(320.0))
        .mt(px(12.0))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(20.0))
        .children(body)
        .into_any_element();
    page_empty_action(None, mood, column, cx).into_any_element()
}

/// A labelled list of rows; nothing when it has none.
pub fn start_section(
    label: impl Into<SharedString>,
    rows: Vec<AnyElement>,
    cx: &App,
) -> AnyElement {
    if rows.is_empty() {
        return div().into_any_element();
    }
    let theme = Theme::global(cx);
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .child(
            div()
                .px(px(8.0))
                .pb(px(4.0))
                .text_left()
                .text_size(px(12.0))
                .line_height(px(18.0))
                .text_color(theme.color_text_muted)
                .child(label.into()),
        )
        .children(rows)
        .into_any_element()
}

/// One row of a start section: see the module's docs.
#[derive(IntoElement)]
pub struct StartRow {
    id: ElementId,
    icon: Icon,
    title: SharedString,
    detail: Option<SharedString>,
    on_open: Option<Handler>,
    on_more: Option<MoreHandler>,
    menu_open: bool,
}

impl StartRow {
    pub fn new(id: impl Into<ElementId>, icon: Icon, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            icon,
            title: title.into(),
            detail: None,
            on_open: None,
            on_more: None,
            menu_open: false,
        }
    }

    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn on_open(mut self, handler: Handler) -> Self {
        self.on_open = Some(handler);
        self
    }

    /// The "…", which hands over its window bounds.
    pub fn on_more(mut self, handler: MoreHandler) -> Self {
        self.on_more = Some(handler);
        self
    }

    /// Its menu is open: the row keeps its fill and its "…".
    pub fn menu_open(mut self, open: bool) -> Self {
        self.menu_open = open;
        self
    }
}

impl RenderOnce for StartRow {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let hovered = window.use_keyed_state(self.id.clone(), cx, |_, _| false);
        let lit = *hovered.read(cx) || self.menu_open;
        let open = self.on_open;
        let more = self.on_more.map(|handler| {
            let bounds = window.use_keyed_state(
                SharedString::from(format!("{:?}-more", self.id)),
                cx,
                |_, _| None::<Bounds<Pixels>>,
            );
            let probe_bounds = bounds.clone();
            div()
                .id("more")
                .absolute()
                .right(px(4.0))
                .top(px(2.0))
                .size(px(28.0))
                .rounded(px(theme.radius_item))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .child(
                    canvas(
                        move |rect, _, cx| {
                            if probe_bounds.read(cx).as_ref() != Some(&rect) {
                                probe_bounds.update(cx, |slot, _| *slot = Some(rect));
                            }
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .inset_0(),
                )
                .child(icon_colored(
                    Icon::MoreHorizontal,
                    14.0,
                    if self.menu_open {
                        theme.color_text_primary
                    } else {
                        theme.color_text_muted
                    },
                ))
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    if let Some(rect) = *bounds.read(cx) {
                        handler(rect, window, cx);
                    }
                })
        });
        div()
            .id(self.id)
            .debug_selector(|| "start-row".into())
            .relative()
            .w_full()
            .h(px(32.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            // The ghost button's `px-3`, inside its 1px border.
            .pl(px(tw(3.0) + 1.0))
            .pr(px(if more.is_some() { 36.0 } else { 12.0 }))
            .rounded(px(16.0))
            .text_left()
            .cursor_pointer()
            .when(lit, |row| row.bg(theme.color_surface_hover))
            .on_hover(move |over, _, cx| {
                hovered.update(cx, |state, cx| {
                    *state = *over;
                    cx.notify();
                });
            })
            .when_some(open, |row, handler| {
                row.on_click(move |_, window, cx| handler(window, cx))
            })
            .child(
                div()
                    .debug_selector(|| "start-row-icon".into())
                    .flex_none()
                    .child(icon_colored(self.icon, 14.0, theme.color_text_secondary)),
            )
            // The title grows to push the detail to the row's end: gpui's layout drops the
            // row's gap beside an auto margin, so the detail takes no `margin-left: auto`.
            .child(
                div()
                    .debug_selector(|| "start-row-title".into())
                    .flex_grow()
                    .min_w(px(0.0))
                    .text_size(px(13.0))
                    .line_height(px(19.5))
                    .text_color(theme.color_text_primary)
                    .child(fade(self.title)),
            )
            .children(self.detail.map(|detail| {
                div()
                    .debug_selector(|| "start-row-detail".into())
                    .min_w(px(0.0))
                    .text_size(px(12.0))
                    .text_color(theme.color_text_muted)
                    .child(fade(detail))
            }))
            .when(lit, |row| row.children(more))
    }
}

#[cfg(test)]
mod tests {
    use gpui::{TestAppContext, div, prelude::*, px, size};

    use super::{MascotMood, StartRow, start_page, start_section};
    use crate::icons::Icon;
    use crate::test_support::{draw, open};

    #[gpui::test]
    fn a_row_sets_its_title_8px_past_its_icon(cx: &mut TestAppContext) {
        let mut cx = open(cx, size(800.0, 600.0), |_, cx| {
            let row = StartRow::new("row", Icon::Server, "alpine-test")
                .detail("vitals@127.0.0.1")
                .into_any_element();
            div()
                .size_full()
                .child(start_page(
                    MascotMood::Sleeping,
                    vec![start_section("Saved", vec![row], cx)],
                    cx,
                ))
                .into_any_element()
        });
        draw(&mut cx);
        let icon = cx.debug_bounds("start-row-icon").expect("the icon draws");
        let title = cx.debug_bounds("start-row-title").expect("the title draws");
        let detail = cx
            .debug_bounds("start-row-detail")
            .expect("the detail draws");
        let row = cx.debug_bounds("start-row").expect("the row draws");
        assert_eq!(icon.size.width, px(14.0));
        assert_eq!(title.origin.x - (icon.origin.x + icon.size.width), px(8.0));
        // The detail ends at the row's end, inside its 12px padding.
        assert_eq!(
            row.origin.x + row.size.width - (detail.origin.x + detail.size.width),
            px(12.0)
        );
    }
}
