//! Tooltips. Pass [`tooltip`] to an element's `tooltip` slot for an opaque one-line label,
//! or [`rich_tooltip`] for one that explains a value: a [`tooltip_heading`], label and value
//! [`tooltip_row`]s, and a [`tooltip_note`], on the floating surface menus use, at a width
//! the caller picks so the heading's detail and the note wrap inside it.
//!
//! GPUI lays a tooltip out at its content's natural width, so text wraps only inside an
//! element of a set width: the panel's.

use std::rc::Rc;

use gpui::{
    AnyElement, AnyView, App, Context, Div, FontWeight, IntoElement, Render, SharedString, Window,
    div, prelude::*, px,
};

use crate::material::floating_panel;
use crate::theme::Theme;

pub struct TooltipView {
    label: SharedString,
}

impl Render for TooltipView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::global(cx);
        div()
            .px(px(theme.space_2))
            .py(px(theme.space_1))
            .rounded(px(theme.radius_sm))
            .bg(theme.color_surface_raised)
            .border_1()
            .border_color(theme.color_border)
            .text_size(px(theme.font_size_xs))
            .text_color(theme.color_text_primary)
            .child(self.label.clone())
    }
}

pub fn tooltip(
    label: impl Into<SharedString>,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let label = label.into();
    move |_window, cx| {
        cx.new(|_| TooltipView {
            label: label.clone(),
        })
        .into()
    }
}

/// The panel's padding, and the room between its blocks.
const PAD: f32 = 12.0;
const GAP: f32 = 8.0;

type Build = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

pub struct RichTooltipView {
    width: f32,
    build: Build,
}

impl Render for RichTooltipView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = (self.build.clone())(window, cx);
        tooltip_card(self.width, body, cx)
    }
}

/// A tooltip `width` wide whose body `build` makes each time it shows, on [`tooltip_card`].
pub fn rich_tooltip(
    width: f32,
    build: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let build: Build = Rc::new(build);
    move |_window, cx| {
        let build = build.clone();
        cx.new(|_| RichTooltipView { width, build }).into()
    }
}

/// The rich tooltip's panel, `width` wide: the menus' floating surface (radius, quiet edge,
/// float drop), 12px in, its blocks 8px apart in 12px text on 18px lines. Drawn on its own,
/// as a gallery does, it shows what the tooltip shows.
pub fn tooltip_card(width: f32, body: impl IntoElement, cx: &App) -> AnyElement {
    let theme = Theme::global(cx);
    let panel = div()
        .relative()
        .w(px(width))
        .bg(theme.color_surface_subtle)
        .p(px(PAD))
        .flex()
        .flex_col()
        .gap(px(GAP))
        .text_size(px(12.0))
        .line_height(px(18.0))
        .text_color(theme.color_text_primary)
        .child(body);
    floating_panel(panel, theme.radius_menu, theme).into_any_element()
}

/// The tooltip's title in 13px semibold, and its detail under it in secondary text.
pub fn tooltip_heading(
    title: impl Into<SharedString>,
    detail: Option<SharedString>,
    cx: &App,
) -> Div {
    let theme = Theme::global(cx);
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .child(
            div()
                .text_size(px(13.0))
                .line_height(px(18.0))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.into()),
        )
        .children(detail.map(|detail| {
            div()
                .w_full()
                .text_color(theme.color_text_secondary)
                .child(detail)
        }))
}

/// A label in secondary text at the left and its value at the right, on one line.
pub fn tooltip_row(label: impl Into<SharedString>, value: impl IntoElement, cx: &App) -> Div {
    let theme = Theme::global(cx);
    div()
        .w_full()
        .flex()
        .items_baseline()
        .justify_between()
        .gap(px(12.0))
        .child(
            div()
                .flex_none()
                .text_color(theme.color_text_secondary)
                .child(label.into()),
        )
        .child(
            div()
                .min_w(px(0.0))
                .flex()
                .justify_end()
                .text_right()
                .child(value),
        )
}

/// A closing line in muted text, above a hairline.
pub fn tooltip_note(text: impl Into<SharedString>, cx: &App) -> Div {
    let theme = Theme::global(cx);
    div()
        .w_full()
        .pt(px(GAP))
        .border_t_1()
        .border_color(theme.color_border_subtle)
        .text_size(px(11.5))
        .line_height(px(16.0))
        .text_color(theme.color_text_muted)
        .child(text.into())
}
