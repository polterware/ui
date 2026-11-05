//! `DetailPanel` (`detail-panel.css`, `side-panel.tsx`): the floating panel a list opens its
//! active record in, beside the list rather than over a scrim. The frame is
//! `surface-panel-float` (`--card-fill`, the quiet edge, the float drop and a 0 20px 56px
//! `--color-shadow-float` shadow, `--radius-surface`), 400px wide, 8px in from the page's
//! right, top (below a toolbar over the page), and bottom. It takes the presses and the
//! wheel over its face, so the list under it keeps neither.
//!
//! Its header (`data-expandable="false"`) pads 12px 12px 8px 20px: the title in 14px/600 on a
//! 30px line with its chips 6px after it, wrapping to the next line when they do not fit, and
//! Close details at the end; with no chips the title is on its 1.35 line, centred with Close
//! on the 28px row; a [`DetailPanel::leading`] icon sits in a 16px slot 8px before
//! the title, centred on it when it is larger, as a 20px bot is. The body scrolls, padded 16px
//! 20px 24px; its sections ([`detail_section`]) stand 24px apart, each a 24px icon tile and a
//! 13px title 8px after it, then its content 10px under them. The rail's sections
//! ([`DetailPanel::rail`], `DetailPanel.Rail`) follow the main ones 12px under them, the
//! record's gap, since the 400px panel stacks the two. [`detail_properties`] lists label and value rows: a
//! 110px muted 12px label, 12px apart from a 12.5px primary value at the end, 28px tall and
//! 8px apart. The footer, when there is one, pads 12px 20px 20px and holds its actions 6px
//! apart.
//!
//! As `dismissOnOutsideClick` asks, a click outside the frame tells the host
//! ([`DetailPanel::on_outside_click`]) after it has done what it does there, so the host can
//! close the panel unless that click opened a record in it, as a row (an opener) does.

use gpui::{
    AnyElement, App, ElementId, FontWeight, IntoElement, MouseButton, Pixels, Point, SharedString,
    Window, div, prelude::*, px, relative,
};

use crate::Handler;
use crate::data_table::{sans, tabular};
use crate::icon_button::icon_button;
use crate::icon_tile::IconTile;
use crate::icons::Icon;
use crate::material::{drop, floating_panel};
use crate::scroll::scroll_area;
use crate::theme::Theme;

/// `--detail-panel-width`.
pub const DETAIL_PANEL_WIDTH: f32 = 400.0;
/// The frame's inset from the page's edges.
pub const DETAIL_PANEL_INSET: f32 = 8.0;
const SECTION_GAP: f32 = 24.0;
/// `.pw-detail-panel__record`'s gap, between the main column and the rail stacked under it.
const RECORD_GAP: f32 = 12.0;
/// `.pw-detail-panel__header-icon`.
const HEADER_ICON: f32 = 16.0;
/// The header's row without chips: Close details' 28px.
const HEADER_ROW: f32 = 28.0;
const PROPERTY_LABEL_WIDTH: f32 = 110.0;

#[derive(IntoElement)]
pub struct DetailPanel {
    id: ElementId,
    title: SharedString,
    leading: Option<AnyElement>,
    aside: Vec<AnyElement>,
    sections: Vec<AnyElement>,
    rail: Vec<AnyElement>,
    footer: Option<AnyElement>,
    on_close: Option<Handler>,
    on_outside: Option<Handler>,
    top: f32,
}

impl DetailPanel {
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            leading: None,
            aside: Vec::new(),
            sections: Vec::new(),
            rail: Vec::new(),
            footer: None,
            on_close: None,
            on_outside: None,
            top: 0.0,
        }
    }

    /// `DetailPanel.Title`'s icon: a mark before the title, such as the actor's bot, 8px
    /// from it.
    pub fn leading(mut self, element: impl IntoElement) -> Self {
        self.leading = Some(element.into_any_element());
        self
    }

    /// An element after the title, such as a `StatusChip`.
    pub fn aside(mut self, element: impl IntoElement) -> Self {
        self.aside.push(element.into_any_element());
        self
    }

    /// A body section, in order: usually a [`detail_section`].
    pub fn section(mut self, section: impl IntoElement) -> Self {
        self.sections.push(section.into_any_element());
        self
    }

    /// `DetailPanel.Footer`: the record's actions, under the body.
    /// A section of the rail (`DetailPanel.Rail`), such as Properties, under the main ones.
    pub fn rail(mut self, section: impl IntoElement) -> Self {
        self.rail.push(section.into_any_element());
        self
    }

    pub fn footer(mut self, footer: impl IntoElement) -> Self {
        self.footer = Some(footer.into_any_element());
        self
    }

    /// Close details: the header's last control.
    pub fn on_close(mut self, handler: Handler) -> Self {
        self.on_close = Some(handler);
        self
    }

    /// Called once a click outside the frame has run its own action: the host closes the
    /// panel unless that click opened a record, such as a row of the list.
    pub fn on_outside_click(mut self, handler: Handler) -> Self {
        self.on_outside = Some(handler);
        self
    }

    /// `--pw-toolbar-overlay`: how far a toolbar floating over the page reaches into it, so
    /// the frame starts under its controls.
    pub fn top(mut self, top: f32) -> Self {
        self.top = top;
        self
    }
}

impl RenderOnce for DetailPanel {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        // Chips make the title `--inline`, on a 30px line; alone it keeps its 1.35 line.
        let inline = !self.aside.is_empty();
        let title = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_x(px(6.0))
            .min_w(px(0.0))
            .when(!inline, |title| title.min_h(px(HEADER_ROW)))
            .children(self.leading.map(|leading| {
                div()
                    .flex_none()
                    .size(px(HEADER_ICON))
                    .mr(px(2.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(div().flex_none().child(leading))
            }))
            .child(
                div()
                    .min_w(px(0.0))
                    .text_size(px(14.0))
                    .line_height(if inline {
                        px(30.0).into()
                    } else {
                        relative(1.35)
                    })
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.color_text_primary)
                    .child(self.title),
            )
            .children(self.aside);
        let close = self.on_close.map(|on_close| {
            icon_button(
                (self.id.clone(), "close"),
                Icon::Close,
                "Close details",
                on_close,
                cx,
            )
        });
        let header = div()
            .flex_none()
            .flex()
            .when(inline, |header| header.items_start())
            .when(!inline, |header| header.items_center())
            .gap(px(8.0))
            .pt(px(12.0))
            .pr(px(12.0))
            .pb(px(8.0))
            .pl(px(20.0))
            .child(div().flex_1().min_w(px(0.0)).child(title))
            .children(close.map(|close| {
                div()
                    .flex_none()
                    .when(inline, |close| close.pt(px(1.0)))
                    .child(close)
            }));
        let mut main = div()
            .w_full()
            .flex()
            .flex_col()
            .pt(px(16.0))
            .px(px(20.0))
            .pb(px(24.0));
        let has_main = !self.sections.is_empty();
        for (index, section) in self.sections.into_iter().enumerate() {
            main = main.child(
                div()
                    .when(index > 0, |section| section.mt(px(SECTION_GAP)))
                    .child(section),
            );
        }
        for (index, section) in self.rail.into_iter().enumerate() {
            let gap = if index > 0 {
                SECTION_GAP
            } else if has_main {
                RECORD_GAP
            } else {
                0.0
            };
            main = main.child(div().mt(px(gap)).child(section));
        }
        let body = div().flex_1().min_h(px(0.0)).child(scroll_area(
            (self.id.clone(), "body"),
            main,
            window,
            cx,
        ));
        let footer = self.footer.map(|footer| {
            div()
                .flex_none()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(6.0))
                .pt(px(12.0))
                .px(px(20.0))
                .pb(px(20.0))
                .child(footer)
        });
        let frame = div()
            .id(self.id.clone())
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.color_surface_subtle)
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(header)
            .child(body)
            .children(footer);
        let frame = match self.on_outside {
            Some(on_outside) => {
                // Only a click counts: a press outside released within 3px of where it began.
                let press =
                    window.use_keyed_state((self.id.clone(), "outside-press"), cx, |_, _| {
                        None::<Point<Pixels>>
                    });
                let down = press.clone();
                frame
                    .on_mouse_down_out(move |event, _, cx| {
                        if event.button == MouseButton::Left {
                            down.update(cx, |press, _| *press = Some(event.position));
                        }
                    })
                    .on_mouse_up_out(MouseButton::Left, move |event, window, cx| {
                        let Some(start) = press.update(cx, |press, _| press.take()) else {
                            return;
                        };
                        let travel = event.position - start;
                        if f32::from(travel.x).hypot(f32::from(travel.y)) >= 3.0 {
                            return;
                        }
                        let on_outside = on_outside.clone();
                        // After the click's own listeners, which may open another record.
                        window.defer(cx, move |window, cx| on_outside(window, cx));
                    })
            }
            None => frame,
        };
        let mut frame = floating_panel(frame, theme.radius_surface, &theme);
        let mut shadows = crate::material::float_drop(&theme);
        shadows.push(drop(20.0, 56.0, theme.color_shadow_float));
        frame = frame.shadow(shadows);
        div()
            .absolute()
            .top(px(DETAIL_PANEL_INSET + self.top))
            .right(px(DETAIL_PANEL_INSET))
            .bottom(px(DETAIL_PANEL_INSET))
            .w(px(DETAIL_PANEL_WIDTH))
            .child(frame)
    }
}

/// `DetailPanel.Section`: a 24px neutral icon tile and the title in 13px primary, 8px apart,
/// then `content` 10px under them.
pub fn detail_section(
    title: impl Into<SharedString>,
    icon: Icon,
    content: impl IntoElement,
    cx: &App,
) -> gpui::Div {
    section(title.into(), icon, None, content.into_any_element(), cx)
}

/// [`detail_section`] with `aside` at the end of its title row (the section's `aside`), such
/// as an icon button.
pub fn detail_section_with_aside(
    title: impl Into<SharedString>,
    icon: Icon,
    aside: impl IntoElement,
    content: impl IntoElement,
    cx: &App,
) -> gpui::Div {
    section(
        title.into(),
        icon,
        Some(aside.into_any_element()),
        content.into_any_element(),
        cx,
    )
}

fn section(
    title: SharedString,
    icon: Icon,
    aside: Option<AnyElement>,
    content: AnyElement,
    cx: &App,
) -> gpui::Div {
    let theme = Theme::global(cx);
    // `.pw-detail-panel__section-heading`: at least 20px, the label and the aside apart.
    let label = div()
        .min_w(px(0.0))
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(IconTile::new(icon).size(24.0, 14.0))
        .child(
            div()
                .text_size(px(13.0))
                .line_height(px(13.0 * 1.35))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.color_text_primary)
                .child(title),
        );
    div()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .min_w(px(0.0))
        .child(
            div()
                .min_w(px(0.0))
                .min_h(px(20.0))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .child(label)
                .children(aside),
        )
        .child(content)
}

/// `DetailPanel.Properties`: the rows 8px apart.
pub fn detail_properties(rows: impl IntoIterator<Item = AnyElement>) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .min_w(px(0.0))
        .children(rows)
}

/// `DetailPanel.Property`: the label in a 110px muted 12px column, then the value at the end
/// in 12.5px primary, wrapping anywhere.
pub fn detail_property(
    label: impl Into<SharedString>,
    value: impl IntoElement,
    cx: &App,
) -> AnyElement {
    let theme = Theme::global(cx);
    div()
        .min_h(px(28.0))
        .flex()
        .items_center()
        .gap(px(12.0))
        .min_w(px(0.0))
        .child(
            div()
                .flex_none()
                .w(px(PROPERTY_LABEL_WIDTH))
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_size(px(12.0))
                .text_color(theme.color_text_muted)
                .child(label.into()),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_wrap()
                .justify_end()
                .items_center()
                .gap(px(4.0))
                .text_size(px(12.5))
                .text_color(theme.color_text_primary)
                .child(value),
        )
        .into_any_element()
}

/// `.pw-detail-panel__text`: a paragraph in 12.5px secondary on a 1.55 line.
pub fn detail_text(text: impl Into<SharedString>, cx: &App) -> gpui::Div {
    let theme = Theme::global(cx);
    div()
        .text_size(px(12.5))
        .line_height(px(12.5 * 1.55))
        .text_color(theme.color_text_secondary)
        .child(text.into())
}

/// A numbered list, as a record's options read: each item's two-digit number in muted tabular
/// figures, 8px before it, the items 8px apart.
pub fn detail_numbered_list<S: Into<SharedString>>(
    items: impl IntoIterator<Item = S>,
    cx: &App,
) -> gpui::Div {
    let theme = Theme::global(cx);
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .text_size(px(12.5))
        .line_height(px(18.0))
        .text_color(theme.color_text_primary)
        .children(items.into_iter().enumerate().map(|(index, item)| {
            div()
                .flex()
                .items_start()
                .gap(px(8.0))
                .min_w(px(0.0))
                .child(
                    div()
                        .flex_none()
                        .font(tabular(sans(theme, FontWeight::NORMAL)))
                        .text_color(theme.color_text_muted)
                        .child(SharedString::from(format!("{:02}", index + 1))),
                )
                .child(div().flex_1().min_w(px(0.0)).child(item.into()))
        }))
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use gpui::{Modifiers, TestAppContext, prelude::*, size};

    use super::DetailPanel;
    use crate::test_support::{draw, open};

    #[gpui::test]
    fn the_panel_floats_8px_in_from_the_right_and_closes(cx: &mut TestAppContext) {
        let closed = Rc::new(Cell::new(0));
        let count = closed.clone();
        let mut cx = open(cx, size(1000.0, 600.0), move |_, _| {
            let count = count.clone();
            gpui::div()
                .relative()
                .size_full()
                .child(
                    DetailPanel::new("panel", "Rename export snapshot")
                        .on_close(Rc::new(move |_, _| count.set(count.get() + 1))),
                )
                .into_any_element()
        });
        draw(&mut cx);
        let close = cx.debug_bounds("panel-close").expect("Close details draws");
        // The header's 12px end padding inside the panel's 8px inset; with no chips, Close
        // stands on the header's 28px row under its 12px top.
        assert_eq!(f32::from(close.right()), 1000.0 - 8.0 - 12.0);
        assert_eq!(f32::from(close.top()), 8.0 + 12.0);
        cx.simulate_click(close.center(), Modifiers::none());
        assert_eq!(closed.get(), 1);
    }

    #[gpui::test]
    fn chips_put_the_title_on_its_30px_line(cx: &mut TestAppContext) {
        let mut cx = open(cx, size(1000.0, 600.0), |_, _| {
            gpui::div()
                .relative()
                .size_full()
                .child(
                    DetailPanel::new("panel", "Rename export snapshot")
                        .aside(gpui::div().id("chip").w(gpui::px(40.0)).h(gpui::px(20.0)))
                        .on_close(Rc::new(|_, _| {})),
                )
                .into_any_element()
        });
        draw(&mut cx);
        let close = cx.debug_bounds("panel-close").expect("Close details draws");
        // Centred on the 30px line, a pixel under the header's 12px top.
        assert_eq!(f32::from(close.top()), 8.0 + 12.0 + 1.0);
    }

    #[gpui::test]
    fn a_section_aside_ends_its_title_row(cx: &mut TestAppContext) {
        let mut cx = open(cx, size(300.0, 200.0), |_, cx| {
            gpui::div()
                .w(gpui::px(300.0))
                .child(super::detail_section_with_aside(
                    "Run summary",
                    crate::icons::Icon::FileText,
                    gpui::div()
                        .debug_selector(|| "aside".into())
                        .size(gpui::px(26.0)),
                    gpui::div()
                        .debug_selector(|| "body".into())
                        .h(gpui::px(10.0)),
                    cx,
                ))
                .into_any_element()
        });
        draw(&mut cx);
        let aside = cx.debug_bounds("aside").expect("the aside draws");
        assert_eq!(f32::from(aside.right()), 300.0);
        assert_eq!(f32::from(aside.top()), 0.0);
        // The row grows past its 20px to the aside; the body stays 10px under it.
        let body = cx.debug_bounds("body").expect("the body draws");
        assert_eq!(f32::from(body.top()), 26.0 + 10.0);
    }
}
