//! Anchored opaque panel. Springs in, and closes on Escape or a click outside. It hangs from
//! the box it is rendered in (see [`hang`](super::hang)); a host whose box scrolls or clips
//! draws it outside that box, in a [`window_anchor`](crate::patterns::window_anchor).

use std::rc::Rc;

use gpui::{
    AnyElement, App, Context, FocusHandle, Focusable, IntoElement, Render, Window, div, prelude::*,
    px,
};

use super::Dismiss;
use super::hang::{SLIDE, hang};
use crate::motion::{Animator, SpringTier, SpringValue};
use crate::theme::Theme;

pub type PopoverPanel = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

pub struct Popover {
    focus: FocusHandle,
    open: bool,
    took_focus: bool,
    /// The trigger's width, when the panel lines up with its right edge.
    end: Option<f32>,
    progress: SpringValue,
    animator: Animator,
    panel: PopoverPanel,
}

impl Popover {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            open: false,
            took_focus: false,
            end: None,
            progress: SpringValue::new(0.0, SpringTier::Fast),
            animator: Animator::new(),
            panel: Rc::new(|_, _| div().into_any_element()),
        }
    }

    pub fn set_panel(&mut self, panel: PopoverPanel) {
        self.panel = panel;
    }

    /// Lines the panel's right edge up with a trigger `trigger_width` wide, for a trigger at
    /// the end of a row; `None` lines up their left edges, the default.
    pub fn set_align_end(&mut self, trigger_width: Option<f32>) {
        self.end = trigger_width;
    }

    pub fn set_open(&mut self, open: bool) {
        self.open = open;
        self.progress.set_target(if open { 1.0 } else { 0.0 });
        if !open {
            self.took_focus = false;
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }
}

impl Focusable for Popover {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Popover {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let visible = self.open || !self.progress.settled();
        let dt = self
            .animator
            .dt(window, visible && !self.progress.settled());
        if dt > 0.0 {
            self.progress.step(dt);
        }
        if !visible {
            return div().into_any_element();
        }
        if self.open && !self.took_focus {
            self.focus.focus(window);
            self.took_focus = true;
        }
        let theme = *Theme::global(cx);
        let t = self.progress.value().clamp(0.0, 1.0);
        let content = (self.panel)(window, cx);
        let panel = div()
            .occlude()
            .key_context("Popover")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Dismiss, _, cx| {
                this.set_open(false);
                cx.notify();
            }))
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                if this.open {
                    this.set_open(false);
                    cx.notify();
                }
            }))
            .opacity(t)
            .child(
                div()
                    .min_w(px(180.0))
                    .p(px(theme.space_2))
                    .rounded(px(theme.radius_lg))
                    .bg(theme.color_surface_raised)
                    .border_1()
                    .border_color(theme.color_border)
                    .text_color(theme.color_text_primary)
                    .child(content),
            );
        let hung = hang(theme.control_height_md, (1.0 - t) * SLIDE, panel);
        match self.end {
            Some(width) => hung.align_end(width).into_any_element(),
            None => hung.into_any_element(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use gpui::{
        Bounds, Entity, Focusable, Modifiers, Pixels, TestAppContext, VisualTestContext, canvas,
        div, point, prelude::*, px, size,
    };

    use super::Popover;
    use crate::test_support::{draw, open, settle};
    use crate::text_input::TextInput;

    const WINDOW: (f32, f32) = (600.0, 400.0);

    struct Fixture {
        popover: Entity<Popover>,
        field: Entity<TextInput>,
        /// The panel's content, as the last frame laid it out.
        panel: Rc<Cell<Option<Bounds<Pixels>>>>,
        cx: VisualTestContext,
    }

    /// A 120 × 32 trigger at `(x, y)` holding an open popover whose content is a 200 × 150
    /// box with a field in it.
    fn fixture(cx: &mut TestAppContext, x: f32, y: f32) -> Fixture {
        let popover = cx.new(Popover::new);
        let field = cx.new(TextInput::new);
        let panel: Rc<Cell<Option<Bounds<Pixels>>>> = Rc::default();
        let (seen, content) = (panel.clone(), field.clone());
        popover.update(cx, |popover, _| {
            popover.set_panel(Rc::new(move |_, _| {
                let seen = seen.clone();
                div()
                    .relative()
                    .w(px(200.0))
                    .h(px(150.0))
                    .child(content.clone())
                    .child(
                        canvas(move |bounds, _, _| seen.set(Some(bounds)), |_, _, _, _| {})
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full(),
                    )
                    .into_any_element()
            }));
        });
        let host = popover.clone();
        let mut cx = open(cx, size(WINDOW.0, WINDOW.1), move |_, _| {
            div()
                .relative()
                .size_full()
                .child(
                    div()
                        .absolute()
                        .left(px(x))
                        .top(px(y))
                        .w(px(120.0))
                        .h(px(32.0))
                        .child(host.clone()),
                )
                .into_any_element()
        });
        cx.update(|_, cx| {
            popover.update(cx, |popover, cx| {
                popover.set_open(true);
                cx.notify();
            })
        });
        settle(&mut cx);
        Fixture {
            popover,
            field,
            panel,
            cx,
        }
    }

    fn panel(fixture: &Fixture) -> Bounds<Pixels> {
        fixture.panel.get().expect("the panel drew")
    }

    fn is_open(fixture: &mut Fixture) -> bool {
        let popover = fixture.popover.clone();
        fixture.cx.update(|_, cx| popover.read(cx).is_open())
    }

    #[gpui::test]
    fn it_hangs_below_its_trigger_from_the_left_edge(cx: &mut TestAppContext) {
        let fixture = fixture(cx, 40.0, 40.0);
        // Inside the panel's 1px border and 8px padding, 32px under the trigger's top.
        assert_eq!(panel(&fixture).origin, point(px(49.0), px(81.0)));
    }

    #[gpui::test]
    fn near_the_corner_it_flips_above_and_stays_in_the_window(cx: &mut TestAppContext) {
        let fixture = fixture(cx, 520.0, 340.0);
        let bounds = panel(&fixture);
        assert!(
            bounds.bottom() <= px(340.0),
            "{bounds:?} is above the trigger"
        );
        assert!(
            bounds.right() <= px(WINDOW.0 - 8.0),
            "{bounds:?} keeps 8px from the right edge"
        );
    }

    #[gpui::test]
    fn escape_in_a_field_with_nothing_to_cancel_closes_it(cx: &mut TestAppContext) {
        let mut fixture = fixture(cx, 40.0, 40.0);
        let field = fixture.field.clone();
        fixture
            .cx
            .update(|window, cx| field.focus_handle(cx).focus(window));
        draw(&mut fixture.cx);
        fixture.cx.simulate_input("abc");
        assert!(is_open(&mut fixture));
        fixture.cx.simulate_keystrokes("escape");
        assert!(!is_open(&mut fixture));
    }

    #[gpui::test]
    fn a_press_outside_closes_it(cx: &mut TestAppContext) {
        let mut fixture = fixture(cx, 40.0, 40.0);
        fixture
            .cx
            .simulate_click(point(px(500.0), px(30.0)), Modifiers::none());
        assert!(!is_open(&mut fixture));
    }
}
