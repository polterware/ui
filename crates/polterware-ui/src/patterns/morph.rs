//! `Morph` (`morph.tsx`, `morph.css`): a button that turns into a small panel in place, for a
//! confirm or a short answer that belongs to one button ("Move to Later?"). App-wide
//! destructive confirms stay on [`ConfirmDialog`](super::ConfirmDialog).
//!
//! The trigger grows upward (downward, when there is more room under it than over it and the
//! panel does not fit above) into a `surface-panel-float` surface on the slow spring (back on
//! its exit tween, easing in): the surface wraps, 6px in, a card (`--color-surface-subtle`
//! with the quiet rings, 14px 14px 12px padding, `--radius-menu` less 6) holding the 13px/600
//! title, the body in 12.5px secondary on a 1.45 line, and the footer's actions full width
//! 6px apart, and under the card the trigger's own rect, where Cancel stands in the trigger's
//! shape, so the pointer
//! that opened the panel is already on it. The surface is at least 260px wide (6px wider than
//! the trigger on each side), starts at the trigger's start (the first of a row), end (the
//! last), or center, and stays 8px inside the window. The content fades in once the surface
//! has mostly grown. The trigger hides while the panel stands in for it. A panel taller than
//! its room keeps its title and footer and scrolls its body (`overflow-y: auto` on
//! `.pw-morph__body`), with the shared scrollbar ([`scroll_area`], after gpui-kit's
//! `Scrollbar`).
//!
//! Escape, a press outside, and Cancel dismiss it; the panel holds the keys while it is open,
//! so the page's shortcuts do not act behind it, and Enter runs [`Morph::on_enter`].

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    AnyElement, App, Bounds, Context, ElementId, FocusHandle, FontWeight, IntoElement,
    KeyDownEvent, MouseButton, Pixels, SharedString, Window, canvas, div, prelude::*, px,
};

use crate::overlay::deferred;
use crate::scroll::scroll_area;

use crate::button::{Button, ButtonKind, ButtonSize};
use crate::material::{Edge, drop, edge, float_drop};
use crate::motion::{Curve, SpringTier, Tween};
use crate::theme::{Ease, Theme};
use crate::{Handler, ValueHandler};

use super::shell::{AnchorSlot, window_layer};

/// What the panel keeps from the window's edges.
const EDGE: f32 = 8.0;
/// The surface's inset around the card and Cancel.
const INSET: f32 = 6.0;
/// The panel's width when the trigger is narrower.
pub const MORPH_WIDTH: f32 = 260.0;
/// Where the layer draws among the deferred ones: over the page's popovers.
const LAYER_PRIORITY: usize = 90;
/// Motion's `easeIn`.
const EASE_IN: Ease = Ease(0.42, 0.0, 1.0, 1.0);

/// Where the panel starts along the trigger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MorphAlign {
    Start,
    Center,
    End,
}

#[derive(IntoElement)]
pub struct Morph {
    id: ElementId,
    trigger: Button,
    open: bool,
    on_open_change: Option<ValueHandler<bool>>,
    title: SharedString,
    body: Option<AnyElement>,
    footer: Vec<AnyElement>,
    width: f32,
    align: MorphAlign,
    grow: bool,
    cancel_label: SharedString,
    on_enter: Option<Handler>,
}

impl Morph {
    /// A morph from `trigger`, a button without a click of its own: a click opens the panel.
    pub fn new(id: impl Into<ElementId>, trigger: Button, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            trigger,
            open: false,
            on_open_change: None,
            title: title.into(),
            body: None,
            footer: Vec::new(),
            width: MORPH_WIDTH,
            align: MorphAlign::Start,
            grow: false,
            cancel_label: "Cancel".into(),
            on_enter: None,
        }
    }

    /// The controlled open state, and what a click on the trigger, Cancel, Escape, or a press
    /// outside asks for.
    pub fn open(
        mut self,
        open: bool,
        on_open_change: impl Fn(bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.open = open;
        self.on_open_change = Some(Rc::new(on_open_change));
        self
    }

    /// `Morph.Body`: what happens, or the fields of a short answer.
    pub fn body(mut self, body: impl IntoElement) -> Self {
        self.body = Some(body.into_any_element());
        self
    }

    /// An action in `Morph.Footer`, full width under the body: usually the confirm.
    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.footer.push(action.into_any_element());
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Which side of the trigger the panel starts from: a row's first trigger starts on its
    /// start, its last on its end, any other on its center.
    pub fn align(mut self, align: MorphAlign) -> Self {
        self.align = align;
        self
    }

    /// The trigger takes its share of the row (`flex-1`).
    pub fn grow(mut self) -> Self {
        self.grow = true;
        self
    }

    pub fn cancel_label(mut self, label: impl Into<SharedString>) -> Self {
        self.cancel_label = label.into();
        self
    }

    /// Enter in the panel, as Enter on its autofocused confirm.
    pub fn on_enter(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_enter = Some(Rc::new(handler));
        self
    }
}

struct MorphState {
    slot: AnchorSlot,
    /// 0 on the trigger, 1 grown into the panel.
    progress: Tween,
    was_open: bool,
    focus: FocusHandle,
    /// The content's natural height, measured each frame it shows.
    natural: Rc<Cell<f32>>,
    /// The body's natural height, as tall as its content however short its scroll is.
    body: Rc<Cell<f32>>,
}

/// Where the panel stands: its rect, and whether it grows down from the trigger.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MorphRect {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
    pub down: bool,
}

/// The panel's rect around the trigger at `from`, in a `viewport`: `width` wide or 6px wider
/// than the trigger on each side, `natural` tall, kept 8px inside the window. It grows up, its
/// bottom 6px under the trigger's, unless it does not fit there and there is more room below:
/// then it grows down, its top 6px over the trigger's. Either way it is cut to its room.
pub fn morph_target(
    from: Bounds<Pixels>,
    natural: f32,
    width: f32,
    align: MorphAlign,
    viewport: (f32, f32),
) -> MorphRect {
    let (fx, fy) = (f32::from(from.origin.x), f32::from(from.origin.y));
    let (fw, fh) = (f32::from(from.size.width), f32::from(from.size.height));
    let panel_width = width.max(fw + INSET * 2.0).min(viewport.0 - EDGE * 2.0);
    let bottom = fy + fh + INSET;
    let top = fy - INSET;
    let room_up = (bottom - EDGE).max(0.0);
    let room_down = (viewport.1 - EDGE - top).max(0.0);
    let down = natural > room_up && room_down > room_up;
    let height = natural.min(if down { room_down } else { room_up }).max(0.0);
    let wanted = match align {
        MorphAlign::Start => fx - INSET,
        MorphAlign::End => fx + fw + INSET - panel_width,
        MorphAlign::Center => fx + fw / 2.0 - panel_width / 2.0,
    };
    let left = wanted.min(viewport.0 - panel_width - EDGE).max(EDGE);
    MorphRect {
        left,
        top: if down { top } else { bottom - height },
        width: panel_width,
        height,
        down,
    }
}

fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

impl RenderOnce for Morph {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        // Cancel stands where the trigger was, in its shape.
        let cancel_radius = self.trigger.radius_override();
        let state =
            window.use_keyed_state(self.id.clone(), cx, |_, cx: &mut Context<MorphState>| {
                MorphState {
                    slot: AnchorSlot::default(),
                    progress: Tween::new(0.0),
                    was_open: false,
                    focus: cx.focus_handle(),
                    natural: Rc::new(Cell::new(0.0)),
                    body: Rc::new(Cell::new(0.0)),
                }
            });
        let slow = SpringTier::Slow;
        let opened = state.update(cx, |state, _| {
            let opened = self.open && !state.was_open;
            if opened {
                state
                    .progress
                    .animate_to(1.0, slow.duration(), slow.curve());
            } else if !self.open && state.was_open {
                state
                    .progress
                    .animate_to(0.0, slow.exit_duration(), Curve::Ease(EASE_IN));
            }
            state.was_open = self.open;
            opened
        });
        let (slot, progress, animating, focus, natural, body_natural) = {
            let state = state.read(cx);
            (
                state.slot.clone(),
                state.progress.value(),
                state.progress.is_animating(),
                state.focus.clone(),
                state.natural.clone(),
                state.body.clone(),
            )
        };
        if animating {
            window.request_animation_frame();
        }
        if opened {
            window.focus(&focus);
        }
        let shown = self.open || animating || progress > 0.001;

        let request = self.on_open_change.clone();
        let open_it = request.clone();
        let mut trigger = self.trigger;
        if self.grow {
            trigger = trigger.full_width();
        }
        if let Some(open_it) = open_it {
            trigger = trigger.on_click(move |window, cx| open_it(true, window, cx));
        }
        let anchor = div()
            .debug_selector({
                let id = self.id.clone();
                move || format!("{id}-anchor")
            })
            .relative()
            .min_w(px(0.0))
            .when(self.grow, |anchor| anchor.flex_1())
            .when(shown, |anchor| anchor.invisible())
            .child(trigger)
            .child(slot.probe());

        let from = slot.get();
        let layer = from.filter(|_| shown).map(|from| {
            let viewport = window.viewport_size();
            let viewport = (f32::from(viewport.width), f32::from(viewport.height));
            let measured = natural.get();
            let fh = f32::from(from.size.height);
            let target = morph_target(
                from,
                if measured > 0.0 {
                    measured
                } else {
                    fh + INSET * 2.0
                },
                self.width,
                self.align,
                viewport,
            );
            let MorphRect {
                left,
                top,
                width,
                height,
                down,
            } = target;
            // What the panel's room cuts from its natural height comes off the body, which
            // scrolls the rest.
            let excess = (measured - height).max(0.0);
            let body_height = (body_natural.get() - excess).max(0.0);
            let t = progress.clamp(0.0, 1.0);
            let x = lerp(f32::from(from.origin.x), left, t);
            let y = lerp(f32::from(from.origin.y), top, t);
            let w = lerp(f32::from(from.size.width), width, t);
            let h = lerp(fh, height, t);
            let radius = lerp(fh / 2.0, theme.radius_menu, t);
            // The content fades in over the last part of the growth, and out with it.
            let alpha = ((t - 0.4) / 0.6).clamp(0.0, 1.0);

            let has_body = self.body.is_some();
            let measure = {
                let natural = natural.clone();
                let body_natural = body_natural.clone();
                canvas(
                    move |bounds, window, _| {
                        // The content as tall as it would be with its whole body showing.
                        let shown = if has_body { body_height } else { 0.0 };
                        let height = f32::from(bounds.size.height) - shown + body_natural.get();
                        if (natural.get() - height).abs() > 0.5 {
                            natural.set(height);
                            window.on_next_frame(|window, _| window.refresh());
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .size_full()
            };
            let card_radius = theme.radius_menu - INSET;
            let card = div()
                .relative()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .pt(px(14.0))
                .px(px(14.0))
                .pb(px(12.0))
                .rounded(px(card_radius))
                .bg(theme.color_surface_subtle)
                .child(
                    div()
                        .text_size(px(13.0))
                        .line_height(px(13.0 * 1.35))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.color_text_primary)
                        .child(self.title.clone()),
                )
                .children(self.body.map(|body| {
                    let body_natural = body_natural.clone();
                    let inner = div()
                        .relative()
                        .text_size(px(12.5))
                        .line_height(px(12.5 * 1.45))
                        .text_color(theme.color_text_secondary)
                        .child(body)
                        .child(
                            canvas(
                                move |bounds, window, _| {
                                    let height = f32::from(bounds.size.height);
                                    if (body_natural.get() - height).abs() > 0.5 {
                                        body_natural.set(height);
                                        window.on_next_frame(|window, _| window.refresh());
                                    }
                                },
                                |_, _, _, _| {},
                            )
                            .absolute()
                            .top(px(0.0))
                            .left(px(0.0))
                            .size_full(),
                        );
                    div().flex_none().h(px(body_height)).child(scroll_area(
                        (self.id.clone(), "body"),
                        inner,
                        window,
                        cx,
                    ))
                }))
                .when(!self.footer.is_empty(), |card| {
                    card.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .pt(px(6.0))
                            .children(self.footer),
                    )
                })
                .child(edge(Edge::Quiet, card_radius, &theme));
            let content = div()
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .w(px(width))
                .flex()
                .flex_col()
                .gap(px(INSET))
                .p(px(INSET))
                .opacity(alpha)
                .when(down, |content| content.child(div().flex_none().h(px(fh))))
                .child(card)
                .when(!down, |content| content.child(div().flex_none().h(px(fh))))
                .child(measure);
            let mut shadows = float_drop(&theme);
            shadows.push(drop(20.0, 56.0, theme.color_shadow_float));
            let panel = div()
                .id((self.id.clone(), "panel"))
                .debug_selector({
                    let id = self.id.clone();
                    move || format!("{id}-panel")
                })
                .absolute()
                .left(px(x))
                .top(px(y))
                .w(px(w))
                .h(px(h))
                .rounded(px(radius))
                .bg(theme.color_surface_subtle)
                .shadow(shadows)
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded(px(radius))
                        .overflow_hidden()
                        .child(content),
                )
                .child(edge(Edge::Quiet, radius, &theme));
            let cancel_request = request.clone();
            let mut cancel_button = Button::new((self.id.clone(), "cancel"), ButtonKind::Secondary)
                .size(ButtonSize::Sm)
                .label(self.cancel_label.clone())
                .full_width()
                .on_click(move |window, cx| {
                    if let Some(request) = &cancel_request {
                        request(false, window, cx);
                    }
                });
            if let Some(radius) = cancel_radius {
                cancel_button = cancel_button.radius(radius);
            }
            let cancel = div()
                .absolute()
                .left(from.origin.x)
                .top(from.origin.y)
                .w(from.size.width)
                .h(from.size.height)
                .opacity(alpha.max(if self.open { 1.0 } else { 0.0 }))
                .child(cancel_button);
            let dismiss = request.clone();
            let keys_request = request.clone();
            let on_enter = self.on_enter.clone();
            let layer = div()
                .id((self.id.clone(), "layer"))
                .track_focus(&focus)
                .key_context("Morph")
                .size_full()
                .on_key_down(move |event: &KeyDownEvent, window, cx| {
                    cx.stop_propagation();
                    match event.keystroke.key.as_str() {
                        "escape" => {
                            if let Some(request) = &keys_request {
                                request(false, window, cx);
                            }
                        }
                        "enter" => {
                            if let Some(on_enter) = &on_enter {
                                on_enter(window, cx);
                            }
                        }
                        _ => {}
                    }
                })
                .child(div().absolute().size_full().when(self.open, |backdrop| {
                    backdrop
                        .occlude()
                        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                            cx.stop_propagation();
                            if let Some(request) = &dismiss {
                                request(false, window, cx);
                            }
                        })
                }))
                .child(panel)
                .child(cancel);
            deferred(window_layer(layer))
                .with_priority(LAYER_PRIORITY)
                .into_any_element()
        });
        anchor.children(layer)
    }
}

#[cfg(test)]
mod tests {
    use gpui::{Bounds, TestAppContext, div, point, prelude::*, px, size};

    use super::{Morph, MorphAlign, morph_target};
    use crate::button::{Button, ButtonKind, ButtonSize};
    use crate::icons::Icon;
    use crate::test_support::{draw, open, settle};

    fn trigger(id: &'static str, label: &'static str) -> Button {
        Button::new(id, ButtonKind::Primary)
            .size(ButtonSize::Sm)
            .label(label)
            .icon(Icon::Inbox)
    }

    #[gpui::test]
    fn a_closed_morph_keeps_the_triggers_natural_width(cx: &mut TestAppContext) {
        let mut visual = open(cx, size(600.0, 300.0), |_, _| {
            div()
                .size_full()
                .flex()
                .items_start()
                .gap(px(8.0))
                .child(
                    Morph::new("morph", trigger("morph-trigger", "Decide"), "Decide")
                        .body(div().w_full().h(px(32.0)))
                        .action(
                            Button::new("morph-action", ButtonKind::Primary)
                                .label("Confirm")
                                .full_width(),
                        ),
                )
                .child(trigger("plain-trigger", "Decide"))
                .into_any_element()
        });
        draw(&mut visual);

        let anchor = visual
            .debug_bounds("morph-anchor")
            .expect("the morph anchor draws");
        let morph_trigger = visual
            .debug_bounds("morph-trigger")
            .expect("the morph trigger draws");
        let plain_trigger = visual
            .debug_bounds("plain-trigger")
            .expect("the plain trigger draws");
        assert_eq!(anchor.size.width, morph_trigger.size.width);
        assert_eq!(anchor.size.width, plain_trigger.size.width);
    }

    #[gpui::test]
    fn morphs_in_a_justified_row_keep_their_button_widths(cx: &mut TestAppContext) {
        let mut visual = open(cx, size(600.0, 300.0), |_, _| {
            div()
                .size_full()
                .flex()
                .justify_end()
                .gap(px(8.0))
                .child(Morph::new(
                    "first",
                    trigger("first-trigger", "Decide"),
                    "Decide",
                ))
                .child(Morph::new(
                    "second",
                    trigger("second-trigger", "Later"),
                    "Later",
                ))
                .into_any_element()
        });
        draw(&mut visual);

        let first = visual
            .debug_bounds("first-anchor")
            .expect("first anchor draws");
        let first_button = visual
            .debug_bounds("first-trigger")
            .expect("first trigger draws");
        let second = visual
            .debug_bounds("second-anchor")
            .expect("second anchor draws");
        let second_button = visual
            .debug_bounds("second-trigger")
            .expect("second trigger draws");
        assert_eq!(first.size.width, first_button.size.width);
        assert_eq!(second.size.width, second_button.size.width);
        assert_eq!(second.right(), px(600.0));
        assert_eq!(second.left() - first.right(), px(8.0));
    }

    #[test]
    fn the_panel_grows_up_from_the_trigger_and_stays_in_the_window() {
        let trigger = Bounds::new(point(px(100.0), px(500.0)), size(px(120.0), px(28.0)));
        // From the start: 6px before the trigger, its bottom 6px under the trigger's.
        let r = morph_target(trigger, 180.0, 260.0, MorphAlign::Start, (1200.0, 800.0));
        assert_eq!(
            (r.left, r.width, r.height, r.down),
            (94.0, 260.0, 180.0, false)
        );
        assert_eq!(r.top + r.height, 534.0);
        // From the end of a trigger near the window's right: held 8px inside.
        let near = Bounds::new(point(px(1100.0), px(500.0)), size(px(90.0), px(28.0)));
        let r = morph_target(near, 180.0, 260.0, MorphAlign::Start, (1200.0, 800.0));
        assert_eq!(r.left + r.width, 1192.0);
        // From the end: its end 6px after the trigger's, unless that leaves the window.
        let middle = Bounds::new(point(px(600.0), px(500.0)), size(px(120.0), px(28.0)));
        let r = morph_target(middle, 180.0, 260.0, MorphAlign::End, (1200.0, 800.0));
        assert_eq!(r.left, 600.0 + 120.0 + 6.0 - 260.0);
        let r = morph_target(trigger, 180.0, 260.0, MorphAlign::End, (1200.0, 800.0));
        assert_eq!(r.left, 8.0);
    }

    #[test]
    fn a_panel_without_room_above_grows_down_and_is_cut_to_its_room() {
        // A trigger near the top: 180px does not fit in the 54px over it.
        let high = Bounds::new(point(px(100.0), px(28.0)), size(px(120.0), px(28.0)));
        let r = morph_target(high, 180.0, 260.0, MorphAlign::Start, (1200.0, 800.0));
        assert!(r.down);
        assert_eq!((r.top, r.height), (22.0, 180.0));
        // Taller than the window's room below: cut 8px over the window's bottom.
        let r = morph_target(high, 2000.0, 260.0, MorphAlign::Start, (1200.0, 800.0));
        assert!(r.down);
        assert_eq!(r.top + r.height, 792.0);
        // Taller than either side, with more room above: up, cut 8px under the window's top.
        let low = Bounds::new(point(px(100.0), px(600.0)), size(px(120.0), px(28.0)));
        let r = morph_target(low, 2000.0, 260.0, MorphAlign::Start, (1200.0, 800.0));
        assert!(!r.down);
        assert_eq!((r.top, r.top + r.height), (8.0, 634.0));
    }

    #[gpui::test]
    fn a_tall_morph_near_the_top_grows_down_and_scrolls_its_body(cx: &mut TestAppContext) {
        let mut visual = open(cx, size(400.0, 400.0), |_, _| {
            div()
                .size_full()
                .pt(px(30.0))
                .pl(px(40.0))
                .child(
                    Morph::new("tall", trigger("tall-trigger", "Tasks"), "Tasks")
                        .open(true, |_, _, _| {})
                        .body(
                            div()
                                .debug_selector(|| "tall-body".into())
                                .w_full()
                                .h(px(900.0)),
                        ),
                )
                .into_any_element()
        });
        settle(&mut visual);

        let trigger = visual
            .debug_bounds("tall-anchor")
            .expect("the anchor draws");
        let panel = visual.debug_bounds("tall-panel").expect("the panel draws");
        // Down from 6px over the trigger, cut 8px over the window's bottom.
        assert_eq!(panel.top(), trigger.top() - px(6.0));
        assert_eq!(panel.bottom(), px(392.0));
        // The body keeps its height and scrolls inside the panel.
        let body = visual.debug_bounds("tall-body").expect("the body draws");
        assert_eq!(body.size.height, px(900.0));
    }
}
