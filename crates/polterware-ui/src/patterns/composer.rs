//! The Composer: the dialog every new or edited record opens in. It opens on a
//! [`modal_layer`](super::modal_layer).
//!
//! It looks like the Solid design system's `Composer`, Linear's create dialog: the
//! `A11yDialog` frame at size `lg` (`surface-panel-float`, 44rem, `--radius-dialog`) with its
//! flush chrome. There is no heading: the record's title field leads (18px semibold, clear of
//! Close at the top right), then its description, the app's fields, and the property pills,
//! on the dialog's surface with space (10px), not rules, between them (`padding: 18px 24px
//! 8px`). The footer (`px-7 pt-3 pb-6`) holds a quiet action at its start and the submit at
//! its end. [`composer_card`] with [`ComposerCard::heading`] draws the plain dialog instead,
//! its title as a 16px heading over the fields.
//!
//! The footer stays put and the body scrolls once it outgrows the card, which never grows
//! past [`COMPOSER_MAX_HEIGHT`] or the window less 40px. Fields keep their own height: they
//! never shrink to fit. A picker opened from the body is drawn in [`ComposerCard::overlay`],
//! outside the scrolling body that would clip it.

use gpui::{
    AnyElement, App, CursorStyle, FontWeight, IntoElement, MouseButton, ParentElement, RenderOnce,
    SharedString, Window, div, prelude::*, px,
};

use crate::Handler;
use crate::icon_button::icon_button;
use crate::icons::Icon;
use crate::material::{Edge, drop, edge, float_drop};
use crate::scroll::{GUTTER, ScrollAxis, ScrollGeometry, scrollbar, use_scroll};
use crate::theme::Theme;

/// The Composer's width (`lg`: 44rem).
pub const COMPOSER_WIDTH: f32 = 704.0;
/// The tallest the Composer grows before its body scrolls (`max-height: min(85vh, 760px)`).
pub const COMPOSER_MAX_HEIGHT: f32 = 760.0;
/// What the Composer keeps clear of the window's edges.
const WINDOW_INSET: f32 = 40.0;
/// Between the title, the description, the fields, and the pills.
const BODY_GAP: f32 = 10.0;

/// The Composer card named `title`. `presence` (0 to 1) fades it in and `offset_y` is the
/// rise still to go (8px when it starts opening, 0 when it is open). Add the body with
/// `child` (the title field first), and the footer with [`ComposerCard::footer`] and
/// [`composer_footer`].
pub fn composer_card(
    title: impl Into<SharedString>,
    presence: f32,
    offset_y: f32,
    _theme: &Theme,
) -> ComposerCard {
    ComposerCard {
        title: title.into(),
        heading: false,
        presence,
        offset_y,
        fields: Vec::new(),
        footer: None,
        on_close: None,
        overlay: None,
    }
}

#[derive(IntoElement)]
pub struct ComposerCard {
    title: SharedString,
    heading: bool,
    presence: f32,
    offset_y: f32,
    fields: Vec<AnyElement>,
    footer: Option<AnyElement>,
    on_close: Option<Handler>,
    overlay: Option<AnyElement>,
}

impl ComposerCard {
    /// Draws the title as the dialog's 16px heading (`A11yDialog`'s default chrome), for a
    /// dialog whose body has no title field, such as a confirmation.
    pub fn heading(mut self) -> Self {
        self.heading = true;
        self
    }

    /// The row under the body that does not scroll: see [`composer_footer`].
    pub fn footer(mut self, footer: impl IntoElement) -> Self {
        self.footer = Some(footer.into_any_element());
        self
    }

    /// Close at the top right, a 28px ghost icon button 16px in.
    pub fn on_close(mut self, handler: Handler) -> Self {
        self.on_close = Some(handler);
        self
    }

    /// Drawn over the card, outside its scrolling body: an open picker, such as
    /// [`super::PropertyPills::overlay`].
    pub fn overlay(mut self, overlay: Option<AnyElement>) -> Self {
        self.overlay = overlay;
        self
    }
}

impl ParentElement for ComposerCard {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.fields.extend(elements);
    }
}

impl RenderOnce for ComposerCard {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let viewport = window.viewport_size();
        let max_h = COMPOSER_MAX_HEIGHT
            .min(f32::from(viewport.height) * 0.85)
            .min(f32::from(viewport.height) - WINDOW_INSET)
            .max(240.0);
        let width = COMPOSER_WIDTH
            .min(f32::from(viewport.width) - WINDOW_INSET)
            .max(320.0);
        let scroll = use_scroll("composer-fields", window, cx);
        let handle = scroll.read(cx).handle().clone();
        let max = f32::from(handle.max_offset().height);
        let view_h = f32::from(handle.bounds().size.height);
        let overflows = max > 0.5;
        let bar = overflows.then(|| {
            scrollbar(
                "composer-fields-bar",
                &handle,
                ScrollAxis::Vertical,
                ScrollGeometry {
                    track: view_h,
                    viewport: view_h,
                    content: view_h + max,
                },
                window,
                cx,
            )
        });
        let has_footer = self.footer.is_some();
        let heading = self.heading;
        let radius = theme.radius_dialog;
        let mut shadow = float_drop(&theme);
        shadow.push(drop(20.0, 56.0, theme.color_shadow_float));
        // The body's inset: `18px 24px 8px` under the flush chrome, `px-7 py-4` under a
        // heading.
        let (pad_x, pad_top) = if self.heading {
            (28.0, 16.0)
        } else {
            (24.0, 18.0)
        };
        let close = self.on_close.map(|handler| {
            div()
                .absolute()
                .top(px(theme.space_4))
                .right(px(theme.space_4))
                .cursor(CursorStyle::PointingHand)
                .child(icon_button(
                    "composer-close",
                    Icon::Close,
                    "Close",
                    handler,
                    cx,
                ))
        });
        div()
            .occlude()
            .relative()
            .w(px(width))
            .max_h(px(max_h))
            .flex()
            .flex_col()
            .rounded(px(radius))
            .bg(theme.color_surface_subtle)
            .shadow(shadow)
            .text_color(theme.color_text_primary)
            .font_family(theme.font_sans)
            .mt(px(self.offset_y))
            .opacity(self.presence.clamp(0.0, 1.0))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .when(self.heading, |card| {
                card.child(
                    div()
                        .flex_none()
                        .px(px(28.0))
                        .pt(px(24.0))
                        .pb(px(8.0))
                        .pr(px(28.0 + 32.0))
                        .text_size(px(16.0))
                        .line_height(px(24.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(self.title.clone()),
                )
            })
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .id("composer-fields")
                            .flex_1()
                            .min_h(px(0.0))
                            .overflow_y_scroll()
                            .track_scroll(&handle)
                            .child(
                                div()
                                    .w_full()
                                    .px(px(pad_x))
                                    .pt(px(pad_top))
                                    .pb(px(if has_footer { 8.0 } else { 24.0 }))
                                    .when(overflows, |body| body.pr(px(pad_x + GUTTER)))
                                    .flex()
                                    .flex_col()
                                    .gap(px(BODY_GAP))
                                    .text_size(px(13.0))
                                    .text_color(theme.color_text_secondary)
                                    .children(self.fields.into_iter().enumerate().map(
                                        |(index, field)| {
                                            div()
                                                .flex_none()
                                                // The title field keeps clear of Close.
                                                .when(index == 0 && !heading, |field| {
                                                    field.pr(px(32.0))
                                                })
                                                .child(field)
                                        },
                                    )),
                            ),
                    )
                    .children(bar.map(|bar| {
                        div()
                            .absolute()
                            .top(px(0.0))
                            .right(px(0.0))
                            .bottom(px(0.0))
                            .w(px(GUTTER))
                            .child(bar)
                    })),
            )
            .children(self.footer.map(|footer| {
                div()
                    .flex_none()
                    .px(px(28.0))
                    .pt(px(12.0))
                    .pb(px(24.0))
                    .child(footer)
            }))
            .children(close)
            .child(edge(Edge::Quiet, radius, &theme))
            .children(self.overlay)
    }
}

/// The Composer's footer (`.pw-dialog-footer`): `start` (a quiet action, such as Remove)
/// at its start and `end` (the buttons, the submit last) at its end, 8px apart.
pub fn composer_footer(
    start: impl IntoIterator<Item = AnyElement>,
    end: impl IntoIterator<Item = AnyElement>,
    theme: &Theme,
) -> gpui::Div {
    div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(theme.space_2))
        .child(div().flex().items_center().gap(px(12.0)).children(start))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(theme.space_2))
                .children(end),
        )
}

/// The Composer's last row with its buttons at the end, in order (the primary one last).
pub fn composer_actions(actions: impl IntoIterator<Item = AnyElement>, theme: &Theme) -> gpui::Div {
    composer_footer([], actions, theme)
}

/// A short labelled field (`.pw-composer__field`): the 12.5px muted label and the value side
/// by side in a 32px row on the subtle fill, 12px in, at `--radius-field`: a field to type in,
/// squarer than the pill buttons beside it. Give it a
/// [`TextInput::inline`](crate::text_input::TextInput::inline).
pub fn composer_field(
    label: impl Into<SharedString>,
    value: impl IntoElement,
    theme: &Theme,
) -> gpui::Div {
    div()
        .w_full()
        .min_h(px(32.0))
        .px(px(12.0))
        .flex()
        .items_center()
        .gap(px(10.0))
        .rounded(px(theme.radius_field))
        .bg(theme.color_surface_hover)
        .text_size(px(12.5))
        .child(
            div()
                .flex_none()
                .font_weight(FontWeight(550.0))
                .text_color(theme.color_text_muted)
                .child(label.into()),
        )
        .child(div().flex_1().min_w(px(0.0)).child(value))
}
