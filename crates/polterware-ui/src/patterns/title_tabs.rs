//! The window's title bar as a strip of tabs, for an app with no sidebar (Vitals'
//! `.vitals-tabs`): it drags the window, clears the traffic lights, and holds a divider, the
//! open tabs, a "+" that goes back to the start page, and a card button at its end (Settings).
//!
//! The look is the old Solid app's, on the lit material: a [`TitleTab`] is a 24px pill on the
//! near-flat face with the control drop and the fading rim, its label in secondary ink and a
//! 16px close at its end that shows only under the pointer; hovered, the face lifts to
//! `#383838 → #333333`, and the open tab reads `#3c3c3c → #363636` in primary ink.
//! [`title_add`] is the same face as a 24px circle with a 14px plus, sunk to a flat `#2b2b2b`
//! while the start page is open; [`title_card`] is a 30px circle with a 12px icon, the same
//! way. [`title_divider`] is the 16px `--color-border-strong` rule 8px past the lights.
//!
//! It behaves like gpui-kit's `TabBar` in what it leaves to the app: the app owns the tabs,
//! which is open, and what a close asks; the strip only draws and reports clicks.

use gpui::{
    AnyElement, App, ElementId, IntoElement, Rgba, SharedString, Window, div, prelude::*, px,
};

use crate::Handler;
use crate::icons::{Icon, icon_colored};
use crate::material::{face, rim_fade};
use crate::theme::{Theme, hex};
use crate::window::window_drag::WindowDragRegion;

/// Where the traffic lights end (`--titlebar-traffic-inset`), from the window's left edge.
pub const TITLEBAR_TRAFFIC_INSET: f32 = 78.0;

fn faces(active: bool, hovered: bool, theme: &Theme) -> (Rgba, Rgba) {
    if active {
        (hex(0x3c3c3c), hex(0x363636))
    } else if hovered {
        (hex(0x383838), hex(0x333333))
    } else {
        (theme.material_face_top, theme.material_face_bottom)
    }
}

/// The strip: `children` in a row 8px apart, as tall as the title bar, starting past the
/// traffic lights when the strip begins `inset` from the window's left edge (the frame's
/// padding).
pub fn title_strip(
    id: impl Into<ElementId>,
    inset: f32,
    children: impl IntoIterator<Item = AnyElement>,
    cx: &App,
) -> impl IntoElement {
    let theme = Theme::global(cx);
    div()
        .id(id)
        .flex_none()
        .w_full()
        .min_w(px(0.0))
        .h(px(theme.titlebar_height))
        .flex()
        .items_center()
        .gap(px(8.0))
        .pl(px((TITLEBAR_TRAFFIC_INSET - inset).max(0.0)))
        .pr(px(4.0))
        .overflow_hidden()
        .window_drag_region()
        .children(children)
}

/// The rule between the traffic lights and the tabs.
pub fn title_divider(cx: &App) -> AnyElement {
    let theme = Theme::global(cx);
    div()
        .flex_none()
        .w(px(1.0))
        .h(px(16.0))
        .ml(px(8.0))
        .mr(px(4.0))
        .bg(theme.color_border_strong)
        .into_any_element()
}

/// Space that pushes what follows to the strip's end.
pub fn title_spacer() -> AnyElement {
    div().flex_1().into_any_element()
}

/// One open tab: see the module's docs.
#[derive(IntoElement)]
pub struct TitleTab {
    id: ElementId,
    label: SharedString,
    active: bool,
    on_select: Option<Handler>,
    on_close: Option<Handler>,
}

impl TitleTab {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            active: false,
            on_select: None,
            on_close: None,
        }
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub fn on_select(mut self, handler: Handler) -> Self {
        self.on_select = Some(handler);
        self
    }

    /// Its close, named "Close <label>".
    pub fn on_close(mut self, handler: Handler) -> Self {
        self.on_close = Some(handler);
        self
    }
}

impl RenderOnce for TitleTab {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let hovered = window.use_keyed_state(self.id.clone(), cx, |_, _| false);
        let hover = *hovered.read(cx);
        let (top, bottom) = faces(self.active, hover, &theme);
        let ink = if self.active || hover {
            theme.color_text_primary
        } else {
            theme.color_text_secondary
        };
        let select = self.on_select;
        let close = self.on_close.map(|handler| {
            let close_hover = window.use_keyed_state(
                SharedString::from(format!("{:?}-close", self.id)),
                cx,
                |_, _| false,
            );
            let over = *close_hover.read(cx);
            div()
                .id("close")
                .flex_none()
                .size(px(16.0))
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .window_drag_control()
                .when(over, |close| close.bg(theme.color_surface_hover))
                .on_hover(move |over, _, cx| {
                    close_hover.update(cx, |state, cx| {
                        *state = *over;
                        cx.notify();
                    });
                })
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    handler(window, cx);
                })
                .child(icon_colored(
                    Icon::X,
                    12.0,
                    if over {
                        theme.color_text_primary
                    } else if hover {
                        theme.color_text_muted
                    } else {
                        Rgba {
                            a: 0.0,
                            ..theme.color_text_muted
                        }
                    },
                ))
        });
        div()
            .id(self.id)
            .relative()
            .flex_none()
            .h(px(24.0))
            .flex()
            .items_center()
            .gap(px(2.0))
            .pl(px(12.0))
            .pr(px(if close.is_some() { 6.0 } else { 12.0 }))
            .rounded_full()
            .bg(face(top, bottom))
            .shadow(crate::material::control_drop(&theme))
            .text_size(px(13.0))
            .line_height(px(16.0))
            .text_color(ink)
            .whitespace_nowrap()
            .cursor_pointer()
            .window_drag_control()
            .on_hover(move |over, _, cx| {
                hovered.update(cx, |state, cx| {
                    *state = *over;
                    cx.notify();
                });
            })
            .when_some(select, |tab, handler| {
                tab.on_click(move |_, window, cx| handler(window, cx))
            })
            .child(div().pr(px(4.0)).child(self.label))
            .children(close)
            .child(rim_fade(12.0, &theme))
    }
}

/// A round button on the strip's material: `size` across with an `icon_size` icon, sunk while
/// `active`.
fn round(
    id: ElementId,
    icon: Icon,
    (size, icon_size): (f32, f32),
    active: bool,
    on_click: Handler,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = *Theme::global(cx);
    let hovered = window.use_keyed_state(id.clone(), cx, |_, _| false);
    let hover = *hovered.read(cx);
    let (top, bottom) = if active {
        (hex(0x2b2b2b), hex(0x2b2b2b))
    } else {
        faces(false, hover, &theme)
    };
    let ink = if active || hover {
        theme.color_text_primary
    } else {
        theme.color_text_secondary
    };
    div()
        .id(id)
        .relative()
        .flex_none()
        .size(px(size))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(face(top, bottom))
        .shadow(crate::material::control_drop(&theme))
        .cursor_pointer()
        .window_drag_control()
        .on_hover(move |over, _, cx| {
            hovered.update(cx, |state, cx| {
                *state = *over;
                cx.notify();
            });
        })
        .on_click(move |_, window, cx| on_click(window, cx))
        .child(icon_colored(icon, icon_size, ink))
        .child(rim_fade(size / 2.0, &theme))
        .into_any_element()
}

/// The "+" after the tabs (`.vitals-tab-new`), sunk while the start page is open.
pub fn title_add(
    id: impl Into<ElementId>,
    active: bool,
    on_click: Handler,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    round(
        id.into(),
        Icon::Plus,
        (24.0, 14.0),
        active,
        on_click,
        window,
        cx,
    )
}

/// A card button (`.vitals-settings-card`, `.vitals-layout-trigger`): a 30px circle with a
/// 12px icon, sunk while what it opens is open.
pub fn title_card(
    id: impl Into<ElementId>,
    icon: Icon,
    active: bool,
    on_click: Handler,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    round(id.into(), icon, (30.0, 12.0), active, on_click, window, cx)
}
