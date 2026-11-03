//! Hover and press state for one control, eased on the wall clock like the CSS transitions
//! of `button.css` (`background-color` on the fast spring's duration, `color` on
//! `--duration-control`, the press inset on `--ease-enter`).

use gpui::{
    App, Context, ElementId, Entity, InteractiveElement, MouseButton, StatefulInteractiveElement,
    Window,
};

use crate::ValueHandler;
use crate::motion::{Curve, SpringTier, Tween};
use crate::theme::{Ease, Theme};

/// How the values of one control ease: the fill and the text color on hover, and the press.
#[derive(Clone, Copy, Debug)]
pub struct PointerTiming {
    pub fill: f32,
    pub color: f32,
    pub press_in: f32,
    pub press_out: f32,
    pub fill_ease: Ease,
    pub press_ease: Ease,
}

impl PointerTiming {
    /// `button.css`: fills on the fast spring's duration, color on `--duration-control`,
    /// the press inset on `--ease-enter`.
    pub fn control(theme: &Theme) -> Self {
        Self {
            fill: SpringTier::Fast.duration(),
            color: theme.duration_control,
            press_in: SpringTier::Fast.duration(),
            press_out: theme.duration_control,
            fill_ease: theme.ease_standard,
            press_ease: theme.ease_enter,
        }
    }

    /// `.pw-sidebar-item` and rows like it: color and background over `--duration-fast` on
    /// `--ease-standard`, the press (`:active`) included.
    pub fn item(theme: &Theme) -> Self {
        Self {
            fill: theme.duration_fast,
            color: theme.duration_fast,
            press_in: theme.duration_fast,
            press_out: theme.duration_fast,
            fill_ease: theme.ease_standard,
            press_ease: theme.ease_standard,
        }
    }
}

/// The hover and press values of one control, each 0 at rest and 1 when engaged.
pub struct PointerState {
    timing: PointerTiming,
    hovered: bool,
    pressed: bool,
    /// Background fills (`--spring-fast-duration`, `--ease-standard`).
    hover: Tween,
    /// Text and icon color (`--duration-control`, `--ease-standard`).
    hover_color: Tween,
    /// The press inset: in on the fast spring's duration, out on `--duration-control`,
    /// both on `--ease-enter`.
    press: Tween,
}

impl PointerState {
    fn new(timing: PointerTiming) -> Self {
        Self {
            timing,
            hovered: false,
            pressed: false,
            hover: Tween::new(0.0),
            hover_color: Tween::new(0.0),
            press: Tween::new(0.0),
        }
    }

    pub fn hovered(&self) -> bool {
        self.hovered
    }

    pub fn pressed(&self) -> bool {
        self.pressed
    }

    pub fn hover(&self) -> f32 {
        self.hover.value()
    }

    pub fn hover_color(&self) -> f32 {
        self.hover_color.value()
    }

    pub fn press(&self) -> f32 {
        self.press.value()
    }

    pub fn animating(&self) -> bool {
        self.hover.is_animating() || self.hover_color.is_animating() || self.press.is_animating()
    }

    fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
        let target = if hovered { 1.0 } else { 0.0 };
        let fill = Curve::Ease(self.timing.fill_ease);
        self.hover.animate_to(target, self.timing.fill, fill);
        self.hover_color.animate_to(target, self.timing.color, fill);
        if !hovered {
            self.set_pressed(false);
        }
    }

    fn set_pressed(&mut self, pressed: bool) {
        self.pressed = pressed;
        let (target, duration) = if pressed {
            (1.0, self.timing.press_in)
        } else {
            (0.0, self.timing.press_out)
        };
        self.press
            .animate_to(target, duration, Curve::Ease(self.timing.press_ease));
    }

    /// Settles every value at once (the gallery's still captures).
    pub fn force(&mut self, hovered: bool, pressed: bool) {
        self.hovered = hovered;
        self.pressed = pressed;
        let value = |on: bool| if on { 1.0 } else { 0.0 };
        self.hover.snap(value(hovered));
        self.hover_color.snap(value(hovered));
        self.press.snap(value(pressed));
    }
}

/// The pointer state kept for the element `id` while it renders in consecutive frames, on
/// the controls' timing. Keeps the window on the animation clock while a value moves.
pub fn use_pointer(
    id: impl Into<ElementId>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<PointerState> {
    let timing = PointerTiming::control(Theme::global(cx));
    use_pointer_timed(id, timing, window, cx)
}

/// `use_pointer` on another timing.
pub fn use_pointer_timed(
    id: impl Into<ElementId>,
    timing: PointerTiming,
    window: &mut Window,
    cx: &mut App,
) -> Entity<PointerState> {
    let state = window.use_keyed_state(id, cx, move |_, _: &mut Context<PointerState>| {
        PointerState::new(timing)
    });
    if state.read(cx).animating() {
        window.request_animation_frame();
    }
    state
}

/// Feeds hover and press into `state`. Disabled controls track nothing.
pub fn track<E>(element: E, state: &Entity<PointerState>, enabled: bool) -> E
where
    E: InteractiveElement + StatefulInteractiveElement,
{
    track_with_hover(element, state, enabled, None)
}

/// `track` that also calls `on_hover`: an element takes one hover listener, so a control
/// that needs its own passes it here.
pub fn track_with_hover<E>(
    element: E,
    state: &Entity<PointerState>,
    enabled: bool,
    on_hover: Option<ValueHandler<bool>>,
) -> E
where
    E: InteractiveElement + StatefulInteractiveElement,
{
    if !enabled {
        return element;
    }
    let hover = state.clone();
    let down = state.clone();
    let up = state.clone();
    let out = state.clone();
    element
        .on_hover(move |hovered, window, cx| {
            hover.update(cx, |state, cx| {
                state.set_hovered(*hovered);
                cx.notify();
            });
            if let Some(handler) = &on_hover {
                handler(*hovered, window, cx);
            }
        })
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            down.update(cx, |state, cx| {
                state.set_pressed(true);
                cx.notify();
            });
        })
        .on_mouse_up(MouseButton::Left, move |_, _, cx| {
            up.update(cx, |state, cx| {
                state.set_pressed(false);
                cx.notify();
            });
        })
        .on_mouse_up_out(MouseButton::Left, move |_, _, cx| {
            out.update(cx, |state, cx| {
                if state.pressed {
                    state.set_pressed(false);
                    cx.notify();
                }
            });
        })
}
