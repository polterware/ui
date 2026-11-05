//! `Peek` (`peek.tsx`, `peek.css`): a quick look at a record without leaving where you are,
//! laid out like the Composer, in a large dialog with no chrome of its own.
//!
//! The content (`.pw-peek`, 16px 24px 22px in) is a column 10px apart with no dividers: the
//! top line (at least 28px, its tools pulled 10px out) holds an optional tile, the kind in
//! 12px weight-550 muted ink, and at its end Expand and Close as 14px ghost icon buttons 2px
//! apart; then the 18px semibold title in primary ink (line-height 1.35); then the read-only
//! properties wrapped 6px apart; then the summary (6px further down), a column 16px apart in
//! 13px secondary ink on a 1.5 line. A property chip ([`peek_chip`]) is
//! `--control-height-sm` tall, 10px in, on `--radius-control` and `--color-surface-subtle`,
//! with its 13px muted icon 6px before its 12px secondary label, which truncates. A section
//! ([`peek_section`]) is its 12px weight-550 muted title over its rows, 6px apart. The footer
//! holds the record's actions, as the dialog's.
//!
//! It behaves like gpui-kit's `Dialog` with a custom body: the app owns whether it is open,
//! draws it over a [`modal_layer`](super::modal_layer), and closes it on Escape; Expand and
//! ⌘Enter open the record's page ([`Peek::on_open`]).

use gpui::{
    AnyElement, App, Div, FontWeight, IntoElement, KeyDownEvent, RenderOnce, SharedString, Window,
    div, prelude::*, px, relative,
};

use super::dialog::{DialogPose, DialogSize, dialog};
use crate::Handler;
use crate::icon_button::icon_button;
use crate::icons::{Icon, icon_colored};
use crate::theme::Theme;

/// A peek titled `title` at `pose`; `kind` names what it is ("Card").
pub fn peek(
    title: impl Into<SharedString>,
    kind: impl Into<SharedString>,
    pose: DialogPose,
) -> Peek {
    Peek {
        title: title.into(),
        kind: kind.into(),
        pose,
        icon: None,
        properties: Vec::new(),
        summary: Vec::new(),
        actions: Vec::new(),
        on_close: None,
        on_open: None,
    }
}

#[derive(IntoElement)]
pub struct Peek {
    title: SharedString,
    kind: SharedString,
    pose: DialogPose,
    icon: Option<AnyElement>,
    properties: Vec<AnyElement>,
    summary: Vec<AnyElement>,
    actions: Vec<AnyElement>,
    on_close: Option<Handler>,
    on_open: Option<Handler>,
}

impl Peek {
    /// The tile before the kind, such as the record's icon tile.
    pub fn icon(mut self, icon: impl IntoElement) -> Self {
        self.icon = Some(icon.into_any_element());
        self
    }

    /// The read-only chips under the title.
    pub fn properties(mut self, properties: impl IntoIterator<Item = AnyElement>) -> Self {
        self.properties.extend(properties);
        self
    }

    /// The record's actions, in the footer.
    pub fn actions(mut self, actions: impl IntoIterator<Item = AnyElement>) -> Self {
        self.actions.extend(actions);
        self
    }

    /// Close, and Space on the summary.
    pub fn on_close(mut self, handler: Handler) -> Self {
        self.on_close = Some(handler);
        self
    }

    /// Expand and ⌘Enter: opens the record's page.
    pub fn on_open(mut self, handler: Handler) -> Self {
        self.on_open = Some(handler);
        self
    }
}

impl ParentElement for Peek {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.summary.extend(elements);
    }
}

impl RenderOnce for Peek {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let mut tools = div()
            .flex_none()
            .ml_auto()
            .flex()
            .items_center()
            .gap(px(2.0));
        if let Some(open) = self.on_open.clone() {
            tools = tools.child(
                icon_button("peek-expand", Icon::ExpandIcon, "Expand", open, cx).icon_size(14.0),
            );
        }
        if let Some(close) = self.on_close.clone() {
            tools = tools
                .child(icon_button("peek-close", Icon::Close, "Close", close, cx).icon_size(14.0));
        }
        let top = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .min_h(px(28.0))
            .mr(px(-10.0))
            .children(self.icon)
            .child(
                div()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_size(px(12.0))
                    .font_weight(FontWeight(550.0))
                    .text_color(theme.color_text_muted)
                    .child(self.kind),
            )
            .child(tools);
        let title = div()
            .text_size(px(18.0))
            .line_height(relative(1.35))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme.color_text_primary)
            .child(self.title.clone());
        let properties = (!self.properties.is_empty()).then(|| {
            div()
                .flex()
                .flex_wrap()
                .gap(px(6.0))
                .children(self.properties)
        });
        let summary = (!self.summary.is_empty()).then(|| {
            div()
                .flex()
                .flex_col()
                .gap(px(16.0))
                .pt(px(6.0))
                .text_size(px(13.0))
                .line_height(relative(1.5))
                .text_color(theme.color_text_secondary)
                .children(self.summary)
        });
        let (on_close, on_open) = (self.on_close.clone(), self.on_open.clone());
        let content = div()
            .id("peek")
            .pt(px(16.0))
            .px(px(24.0))
            .pb(px(22.0))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .min_w(px(0.0))
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                let keys = &event.keystroke;
                if keys.key == "space" && !keys.modifiers.modified() {
                    if let Some(close) = &on_close {
                        cx.stop_propagation();
                        close(window, cx);
                    }
                } else if keys.key == "enter"
                    && (keys.modifiers.platform || keys.modifiers.control)
                    && let Some(open) = &on_open
                {
                    cx.stop_propagation();
                    open(window, cx);
                }
            })
            .child(top)
            .child(title)
            .children(properties)
            .children(summary);
        dialog(self.title, self.pose.opacity)
            .size(DialogSize::Lg)
            .flush()
            .pose(self.pose)
            .footer(self.actions)
            .child(content)
    }
}

/// `PeekChip`: a read-only property, with an optional icon; `mono` sets its label in the
/// monospace face, as a slug's.
pub fn peek_chip(icon: Option<Icon>, label: impl Into<SharedString>, mono: bool, cx: &App) -> Div {
    let theme = Theme::global(cx);
    div()
        .flex_none()
        .max_w_full()
        .flex()
        .items_center()
        .gap(px(6.0))
        .min_h(px(theme.control_height_sm))
        .px(px(10.0))
        .rounded(px(theme.radius_control))
        .bg(theme.color_surface_subtle)
        .text_size(px(12.0))
        .text_color(theme.color_text_secondary)
        .children(icon.map(|icon| icon_colored(icon, 13.0, theme.color_text_muted)))
        .child(
            div()
                .min_w(px(0.0))
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .when(mono, |label| label.font_family(theme.font_mono))
                .child(label.into()),
        )
}

/// `PeekSection`: a labelled group of the summary, such as Open tasks.
pub fn peek_section(
    title: impl Into<SharedString>,
    rows: impl IntoIterator<Item = AnyElement>,
    cx: &App,
) -> Div {
    let theme = Theme::global(cx);
    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(
            div()
                .text_size(px(12.0))
                .font_weight(FontWeight(550.0))
                .text_color(theme.color_text_muted)
                .child(title.into()),
        )
        .children(rows)
}
