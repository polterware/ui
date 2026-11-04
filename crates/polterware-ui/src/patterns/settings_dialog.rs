//! The settings dialog every app shares, over the page it was opened from. It opens on a
//! [`modal_layer`](super::modal_layer).
//!
//! It looks like the Solid design system's `SettingsDialog`: the `A11yDialog` frame (55rem by
//! `min(78vh, 760px)`, narrower than the web's `xl` 72rem, which read as too wide beside the
//! window; `--radius-dialog`, on `--color-app-background`) holding,
//! 8px in and 8px apart, the sections' rail (208px, 144 in a narrow window, 8px in, rows 4px
//! apart) and the active section on an opaque panel (`surface-panel`, the dialog's radius
//! less the inset). A rail row is 32px, `0 10px` in at `--radius-control`, its 14px icon and
//! 12.5px label secondary, primary on `--color-surface-selected` when it is the open section;
//! a group's quiet heading (11px medium muted) leads the rows that share it. The panel draws
//! Close (the secondary icon button, on the control material) 20px in from its top right,
//! the section's 16px semibold heading (and its actions) 24px above the content, and scrolls
//! the content `24px 32px 32px` in; scrolled, the content fades out under Close.
//!
//! It behaves like gpui-kit's `Settings` (its `SettingPage`s in a sidebar): the app owns
//! which section is open and hears a pick through [`SettingsDialog::on_select`]; the panel's
//! scroll is its own and starts at the top for each section the app opens.

use std::rc::Rc;

use gpui::{
    AnyElement, App, CursorStyle, FontWeight, IntoElement, MouseButton, ParentElement, RenderOnce,
    SharedString, Window, div, point, prelude::*, px,
};

use crate::Handler;
use crate::button::{Button, ButtonKind, ButtonSize};
use crate::icons::{Icon, icon_colored};
use crate::material::{Edge, drop, edge, float_drop};
use crate::scroll::{GUTTER, ScrollAxis, ScrollGeometry, scrollbar, use_scroll};
use crate::theme::Theme;

use super::scroll_fade::toolbar_trail;

/// The dialog's width (55rem) and height when the window has room for them.
pub const SETTINGS_WIDTH: f32 = 880.0;
pub const SETTINGS_HEIGHT: f32 = 760.0;
/// The sections' rail, and its width once the dialog is under 720px.
const NAV_WIDTH: f32 = 208.0;
const NAV_NARROW: f32 = 144.0;
/// The inset around and between the rail and the panel.
const INSET: f32 = 8.0;
/// What the dialog keeps clear of the window's edges.
const WINDOW_INSET: f32 = 40.0;
/// The trail over the panel's top once its content scrolls: Close's row and a little more.
const TRAIL: f32 = 56.0;

/// One section in the rail.
#[derive(Clone)]
pub struct SettingsNavItem {
    id: SharedString,
    label: SharedString,
    icon: Option<Icon>,
    group: Option<SharedString>,
}

impl SettingsNavItem {
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            group: None,
        }
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Items that share a group are listed under one quiet heading, in order.
    pub fn group(mut self, group: impl Into<SharedString>) -> Self {
        self.group = Some(group.into());
        self
    }
}

/// Called with the id of the section picked in the rail.
pub type SettingsSelect = Rc<dyn Fn(&SharedString, &mut Window, &mut App)>;

/// The dialog, `presence` (0 to 1) of the way in.
pub fn settings_dialog(presence: f32) -> SettingsDialog {
    SettingsDialog {
        presence,
        items: Vec::new(),
        active: SharedString::default(),
        on_select: None,
        heading: None,
        actions: None,
        on_close: None,
        content: Vec::new(),
    }
}

#[derive(IntoElement)]
pub struct SettingsDialog {
    presence: f32,
    items: Vec<SettingsNavItem>,
    active: SharedString,
    on_select: Option<SettingsSelect>,
    heading: Option<SharedString>,
    actions: Option<AnyElement>,
    on_close: Option<Handler>,
    content: Vec<AnyElement>,
}

impl SettingsDialog {
    pub fn items(mut self, items: Vec<SettingsNavItem>) -> Self {
        self.items = items;
        self
    }

    /// The open section's id.
    pub fn active(mut self, id: impl Into<SharedString>) -> Self {
        self.active = id.into();
        self
    }

    pub fn on_select(mut self, handler: SettingsSelect) -> Self {
        self.on_select = Some(handler);
        self
    }

    /// The panel's heading; the open section's label by default.
    pub fn heading(mut self, heading: impl Into<SharedString>) -> Self {
        self.heading = Some(heading.into());
        self
    }

    /// Controls beside the heading, such as an Add button.
    pub fn actions(mut self, actions: impl IntoElement) -> Self {
        self.actions = Some(actions.into_any_element());
        self
    }

    /// Close at the panel's top right.
    pub fn on_close(mut self, handler: Handler) -> Self {
        self.on_close = Some(handler);
        self
    }
}

impl ParentElement for SettingsDialog {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.content.extend(elements);
    }
}

/// The dialog's size in a `viewport`: [`SETTINGS_WIDTH`] by [`SETTINGS_HEIGHT`] (at most 78%
/// of the window's height), smaller in a small window, never under 480 by 320.
pub fn settings_size(window: &Window) -> (f32, f32) {
    let viewport = window.viewport_size();
    let (width, height) = (f32::from(viewport.width), f32::from(viewport.height));
    (
        SETTINGS_WIDTH.min(width - WINDOW_INSET).max(480.0),
        SETTINGS_HEIGHT
            .min(height * 0.78)
            .min(height - WINDOW_INSET)
            .max(320.0),
    )
}

impl RenderOnce for SettingsDialog {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let t = self.presence.clamp(0.0, 1.0);
        let (width, height) = settings_size(window);
        let nav_width = if width < 720.0 { NAV_NARROW } else { NAV_WIDTH };
        let radius = theme.radius_dialog;
        let mut shadow = float_drop(&theme);
        shadow.push(drop(20.0, 56.0, theme.color_shadow_float));

        // The panel scrolls back to its top when another section opens.
        let scroll = use_scroll("settings-content", window, cx);
        let handle = scroll.read(cx).handle().clone();
        let seen = window.use_keyed_state("settings-content-section", cx, |_, _| {
            SharedString::default()
        });
        if *seen.read(cx) != self.active {
            handle.set_offset(point(px(0.0), px(0.0)));
            let active = self.active.clone();
            seen.update(cx, |seen, _| *seen = active);
        }
        let max = f32::from(handle.max_offset().height);
        let view_h = f32::from(handle.bounds().size.height);
        // Scrolled away from the top, the content goes soft under Close instead of being cut
        // at the panel's edge: the toolbar's trail, as tall as the scroll has gone, up to 56.
        let scrolled = (-f32::from(handle.offset().y)).clamp(0.0, TRAIL);
        let bar = (max > 0.5).then(|| {
            scrollbar(
                "settings-content-bar",
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

        let mut nav = div()
            .id("settings-nav")
            .w(px(nav_width))
            .flex_none()
            .h_full()
            .overflow_y_scroll()
            .p(px(INSET))
            .flex()
            .flex_col()
            .gap(px(4.0));
        let mut last_group: Option<SharedString> = None;
        for (index, item) in self.items.iter().enumerate() {
            if item.group.is_some() && item.group != last_group {
                nav = nav.child(
                    div()
                        .px(px(10.0))
                        .pb(px(2.0))
                        .when(index > 0, |heading| heading.pt(px(12.0)))
                        .text_size(px(11.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.color_text_muted)
                        .child(item.group.clone().unwrap_or_default()),
                );
            }
            last_group = item.group.clone();
            let on = item.id == self.active;
            let ink = if on {
                theme.color_text_primary
            } else {
                theme.color_text_secondary
            };
            let select = self.on_select.clone();
            let id = item.id.clone();
            nav = nav.child(
                div()
                    .id(("settings-nav-item", index))
                    .h(px(32.0))
                    .w_full()
                    .flex_none()
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(theme.space_2))
                    .rounded(px(theme.radius_control))
                    .cursor_pointer()
                    .text_size(px(12.5))
                    .text_color(ink)
                    .map(|row| {
                        if on {
                            row.bg(theme.color_surface_selected)
                        } else {
                            row.hover(move |row| {
                                row.bg(theme.color_surface_hover)
                                    .text_color(theme.color_text_primary)
                            })
                        }
                    })
                    .on_click(move |_, window, cx| {
                        if let Some(select) = &select {
                            select(&id, window, cx);
                        }
                    })
                    .children(item.icon.map(|name| icon_colored(name, 14.0, ink)))
                    .child(div().truncate().child(item.label.clone())),
            );
        }

        let heading = self.heading.clone().or_else(|| {
            self.items
                .iter()
                .find(|item| item.id == self.active)
                .map(|item| item.label.clone())
        });
        let panel_radius = radius - INSET;
        let close = self.on_close.map(|handler| {
            div()
                .absolute()
                .top(px(20.0))
                .right(px(20.0))
                .cursor(CursorStyle::PointingHand)
                // The control material, as the toolbars' icon buttons wear it, so Close reads
                // as a button over the panel and not as a bare glyph.
                .child(
                    Button::new("settings-close", ButtonKind::Secondary)
                        .size(ButtonSize::Icon)
                        .icon(Icon::Close)
                        .tooltip("Close settings")
                        .on_click_rc(handler),
                )
        });
        let panel = div()
            .relative()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .rounded(px(panel_radius))
            .bg(theme.color_surface)
            .overflow_hidden()
            .child(
                div()
                    .id("settings-content")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&handle)
                    .child(
                        div()
                            .w_full()
                            .pt(px(24.0))
                            .px(px(32.0))
                            .pb(px(32.0))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .mb(px(24.0))
                                    .pr(px(40.0))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(12.0))
                                    .child(
                                        div()
                                            .text_size(px(16.0))
                                            .line_height(px(24.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(theme.color_text_primary)
                                            .child(heading.unwrap_or_default()),
                                    )
                                    .children(self.actions),
                            )
                            .child(div().flex().flex_col().gap(px(24.0)).children(self.content)),
                    ),
            )
            .when(scrolled > 0.5, |panel| {
                panel.child(
                    div()
                        .absolute()
                        .top(px(0.0))
                        // Clear of the rounded corners and of the scrollbar.
                        .left(px(panel_radius))
                        .right(px(GUTTER + panel_radius))
                        .child(toolbar_trail(theme.color_surface, scrolled)),
                )
            })
            .children(bar.map(|bar| {
                div()
                    .absolute()
                    .top(px(0.0))
                    .right(px(0.0))
                    .bottom(px(0.0))
                    .w(px(GUTTER))
                    .child(bar)
            }))
            .children(close);

        div()
            .occlude()
            .relative()
            .w(px(width))
            .h(px(height))
            .mt(px((1.0 - t) * 8.0))
            .opacity(t)
            .flex()
            .gap(px(INSET))
            .p(px(INSET))
            .rounded(px(radius))
            .bg(theme.color_app_background)
            .shadow(shadow)
            .text_color(theme.color_text_primary)
            .font_family(theme.font_sans)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(nav)
            .child(panel)
            .child(edge(Edge::Quiet, radius, &theme))
    }
}
