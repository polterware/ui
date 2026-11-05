//! `Switch` (`.pw-switch`): a 38×22 track with radius 6, a grey gradient with the inset well
//! ring, the info blue when on. The squared 16px thumb (radius 3, a white face with a white
//! rim) sits 3px in. Hover stretches it 2px and press 4px, toward the middle, and press also
//! squashes it 4px; its x, y, width, and height spring on `spring.moderate`, as `switch.tsx`
//! drives them. The track's color eases over `--duration-fast`. A click toggles it; dragging
//! the thumb across does not (Ark's drag-to-toggle is not ported). [`Switch::small`] is
//! `size="sm"`: a 30×18 track at radius 4, a 12px thumb at radius 2, 3px in, stretching 2px
//! on hover and 3px on press, which squashes it 3px.
//!
//! [`switch_row`] is `SwitchRow` (`.pw-switch-row`): the label, then the switch, in a row
//! that pads 6px 8px at `--radius-item` and fills with `--color-surface-hover` on hover. The
//! row is the switch's root: hovering it stretches the thumb and a click on it toggles.
//! [`switch_card`] is that row on the card's raised material, a pill as wide as its label, for
//! a switch that stands in a toolbar.

use std::rc::Rc;

use gpui::{
    App, Context, ElementId, Entity, IntoElement, MouseButton, Render, SharedString, Window, div,
    prelude::*, px,
};

use super::material::{self, Edge, face};
use super::pointer::{PointerTiming, track_with_hover, use_pointer_timed};
use crate::ValueHandler;
use crate::motion::{Animator, Curve, SpringTier, SpringValue, Tween};
use crate::theme::{Theme, hex, mix, white};

/// A size's track and thumb (`METRICS` in `switch.tsx`, the radii from `switch.css`).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Metrics {
    track_w: f32,
    track_h: f32,
    track_radius: f32,
    thumb: f32,
    thumb_radius: f32,
    offset: f32,
    hover_extend: f32,
    press_extend: f32,
    squash: f32,
}

const DEFAULT: Metrics = Metrics {
    track_w: 38.0,
    track_h: 22.0,
    track_radius: 6.0,
    thumb: 16.0,
    thumb_radius: 3.0,
    offset: 3.0,
    hover_extend: 2.0,
    press_extend: 4.0,
    squash: 4.0,
};

const SMALL: Metrics = Metrics {
    track_w: 30.0,
    track_h: 18.0,
    track_radius: 4.0,
    thumb: 12.0,
    thumb_radius: 2.0,
    offset: 3.0,
    hover_extend: 2.0,
    press_extend: 3.0,
    squash: 3.0,
};

pub struct Switch {
    on: bool,
    metrics: Metrics,
    hovered: bool,
    /// The pointer is over the `SwitchRow` that holds it.
    row_hovered: bool,
    pressed: bool,
    x: SpringValue,
    y: SpringValue,
    width: SpringValue,
    height: SpringValue,
    hover: Tween,
    animator: Animator,
    on_change: Option<ValueHandler<bool>>,
}

fn rest_x(metrics: &Metrics, on: bool, extend: f32) -> f32 {
    // On, the thumb stretches leftward so it stays flush with the right edge.
    if on {
        metrics.track_w - metrics.offset - metrics.thumb - extend
    } else {
        metrics.offset
    }
}

impl Switch {
    pub fn new(on: bool, cx: &mut Context<Self>) -> Self {
        let _ = cx;
        let metrics = DEFAULT;
        Self {
            on,
            metrics,
            hovered: false,
            row_hovered: false,
            pressed: false,
            x: SpringValue::new(rest_x(&metrics, on, 0.0), SpringTier::Moderate),
            y: SpringValue::new(metrics.offset, SpringTier::Moderate),
            width: SpringValue::new(metrics.thumb, SpringTier::Moderate),
            height: SpringValue::new(metrics.thumb, SpringTier::Moderate),
            hover: Tween::new(0.0),
            animator: Animator::new(),
            on_change: None,
        }
    }

    /// `size="sm"`, as the Composer's Create more is: see the module's docs.
    pub fn small(mut self) -> Self {
        let metrics = SMALL;
        self.metrics = metrics;
        self.x = SpringValue::new(rest_x(&metrics, self.on, 0.0), SpringTier::Moderate);
        self.y = SpringValue::new(metrics.offset, SpringTier::Moderate);
        self.width = SpringValue::new(metrics.thumb, SpringTier::Moderate);
        self.height = SpringValue::new(metrics.thumb, SpringTier::Moderate);
        self
    }

    pub fn on_change(
        mut self,
        callback: impl Fn(bool, &mut Window, &mut gpui::App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(callback));
        self
    }

    pub fn is_on(&self) -> bool {
        self.on
    }

    pub fn set_on(&mut self, on: bool) {
        self.on = on;
        self.retarget();
    }

    /// Flips it and tells `on_change`, as a click does.
    pub fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.on = !self.on;
        self.retarget();
        if let Some(callback) = self.on_change.clone() {
            callback(self.on, window, cx);
        }
        cx.notify();
    }

    fn set_hover(&mut self, theme: &Theme) {
        let target = if self.hovered || self.row_hovered {
            1.0
        } else {
            0.0
        };
        self.hover.animate_to(
            target,
            theme.duration_fast,
            Curve::Ease(theme.ease_standard),
        );
        if target == 0.0 {
            self.pressed = false;
        }
        self.retarget();
    }

    fn retarget(&mut self) {
        let metrics = self.metrics;
        let extend = if self.pressed {
            metrics.press_extend
        } else if self.hovered || self.row_hovered {
            metrics.hover_extend
        } else {
            0.0
        };
        let squeeze = if self.pressed { metrics.squash } else { 0.0 };
        self.x.set_target(rest_x(&metrics, self.on, extend));
        self.y.set_target(metrics.offset + squeeze / 2.0);
        self.width.set_target(metrics.thumb + extend);
        self.height.set_target(metrics.thumb - squeeze);
    }

    fn moving(&self) -> bool {
        !self.x.settled()
            || !self.y.settled()
            || !self.width.settled()
            || !self.height.settled()
            || self.hover.is_animating()
    }
}

impl Render for Switch {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dt = self.animator.dt(window, self.moving());
        if dt > 0.0 {
            self.x.step(dt);
            self.y.step(dt);
            self.width.step(dt);
            self.height.step(dt);
        }
        let theme = *Theme::global(cx);
        let hover = self.hover.value();
        let info = theme.color_info;
        let (top, bottom) = if self.on {
            (
                mix(info, white(1.0), 0.08 * hover),
                mix(info, white(1.0), 0.12 + 0.08 * hover),
            )
        } else {
            (
                mix(hex(0x3a3a3a), hex(0x424242), hover),
                mix(hex(0x4a4a4a), hex(0x525252), hover),
            )
        };
        let thumb_bottom = if self.on {
            hex(0xeeeeee)
        } else {
            hex(0xe4e4e4)
        };
        let metrics = self.metrics;
        let radius = metrics.track_radius;
        div()
            .id(("switch", cx.entity_id()))
            .relative()
            .flex_none()
            .w(px(metrics.track_w))
            .h(px(metrics.track_h))
            .rounded(px(radius))
            .bg(face(top, bottom))
            .cursor_pointer()
            .child(material::edge(Edge::Well, radius, &theme))
            .child(
                div()
                    .absolute()
                    .left(px(self.x.value()))
                    .top(px(self.y.value()))
                    .w(px(self.width.value().max(0.0)))
                    .h(px(self.height.value().max(0.0)))
                    .rounded(px(metrics.thumb_radius))
                    .bg(face(hex(0xffffff), thumb_bottom))
                    .border(px(0.5))
                    .border_color(hex(0xffffff))
                    .shadow(vec![material::drop(0.5, 0.5, theme.material_drop_strong)]),
            )
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                this.hovered = *hovered;
                this.set_hover(&theme);
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.pressed = true;
                    this.retarget();
                    cx.notify();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.pressed = false;
                    this.retarget();
                    cx.notify();
                }),
            )
            .on_click(cx.listener(|this, _, window, cx| {
                // The row around it would toggle it again.
                cx.stop_propagation();
                this.toggle(window, cx);
            }))
    }
}

/// `SwitchRow`: `label` and the switch in one 34px row.
pub fn switch_row(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    switch: &Entity<Switch>,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    labelled(id, label, switch, false, window, cx)
}

/// [`switch_row`] on the card's raised material, as a pill as wide as its label: a toolbar's
/// switch, standing as one control beside its buttons.
pub fn switch_card(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    switch: &Entity<Switch>,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    labelled(id, label, switch, true, window, cx)
}

fn labelled(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    switch: &Entity<Switch>,
    card: bool,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let theme = *Theme::global(cx);
    let id = id.into();
    let pointer = use_pointer_timed(id.clone(), PointerTiming::item(&theme), window, cx);
    let hover = pointer.read(cx).hover();
    let on = switch.read(cx).is_on();
    let ink = if on {
        theme.color_text_primary
    } else {
        theme.color_text_secondary
    };
    let hover_switch = switch.clone();
    let click_switch = switch.clone();
    let selector = id.clone();
    let row = div()
        .id(id)
        // Tests find a switch row by its id (a no-op outside gpui's test support).
        .debug_selector(move || selector.to_string())
        .relative()
        .flex()
        .items_center()
        .gap(px(8.0))
        .py(px(6.0))
        .text_size(px(12.0))
        .line_height(px(18.0))
        .text_color(ink)
        .cursor_pointer();
    let row = if card {
        // A pill: the track's height and the row's padding, round at both ends.
        let radius = (switch.read(cx).metrics.track_h + 12.0) / 2.0;
        material::raised_panel(
            row.flex_none().pl(px(14.0)).pr(px(6.0)).bg(mix(
                theme.color_surface_subtle,
                theme.color_surface_hover,
                hover,
            )),
            radius,
            &theme,
        )
        .child(div().flex_none().child(label.into()))
    } else {
        row.w_full()
            .px(px(8.0))
            .rounded(px(theme.radius_item))
            .bg(crate::theme::fade(theme.color_surface_hover, hover))
            .child(div().flex_1().min_w(px(0.0)).child(label.into()))
    };
    let row = row.child(switch.clone()).on_click(move |_, window, cx| {
        click_switch.update(cx, |switch, cx| switch.toggle(window, cx));
    });
    // An element takes one hover listener, so the switch's goes through the pointer's.
    let on_hover: ValueHandler<bool> = Rc::new(move |hovered, _, cx| {
        hover_switch.update(cx, |switch, cx| {
            switch.row_hovered = hovered;
            switch.set_hover(&theme);
            cx.notify();
        });
    });
    track_with_hover(row, &pointer, true, Some(on_hover))
}
