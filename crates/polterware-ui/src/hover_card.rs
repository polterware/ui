//! Hover card. The panel springs in while the pointer is over the trigger.

use std::rc::Rc;

use gpui::{AnyElement, App, Context, IntoElement, Render, Window, div, prelude::*, px};

use crate::motion::{Animator, SpringTier, SpringValue};
use crate::theme::Theme;

pub type HoverBuilder = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

pub struct HoverCard {
    open: bool,
    progress: SpringValue,
    animator: Animator,
    trigger: HoverBuilder,
    body: HoverBuilder,
}

impl HoverCard {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let _ = cx;
        let empty: HoverBuilder = Rc::new(|_, _| div().into_any_element());
        Self {
            open: false,
            progress: SpringValue::new(0.0, SpringTier::Fast),
            animator: Animator::new(),
            trigger: empty.clone(),
            body: empty,
        }
    }

    pub fn set_trigger(&mut self, trigger: HoverBuilder) {
        self.trigger = trigger;
    }

    pub fn set_body(&mut self, body: HoverBuilder) {
        self.body = body;
    }
}

impl Render for HoverCard {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let visible = self.open || !self.progress.settled();
        let dt = self
            .animator
            .dt(window, visible && !self.progress.settled());
        if dt > 0.0 {
            self.progress.step(dt);
        }
        let theme = *Theme::global(cx);
        let t = self.progress.value().clamp(0.0, 1.0);
        let trigger = (self.trigger)(window, cx);
        let mut root = div()
            .id(("hover-card", cx.entity_id()))
            .relative()
            .on_hover(cx.listener(|this, hovered, _, cx| {
                this.open = *hovered;
                this.progress.set_target(if *hovered { 1.0 } else { 0.0 });
                cx.notify();
            }))
            .child(trigger);
        if visible {
            let body = (self.body)(window, cx);
            root = root.child(
                div()
                    .absolute()
                    .top(px(theme.control_height_md + (1.0 - t) * 4.0))
                    .left(px(0.0))
                    .occlude()
                    .opacity(t)
                    .child(
                        div()
                            .min_w(px(180.0))
                            .p(px(theme.space_3))
                            .rounded(px(theme.radius_lg))
                            .bg(theme.color_surface_raised)
                            .border_1()
                            .border_color(theme.color_border)
                            .text_color(theme.color_text_primary)
                            .child(body),
                    ),
            );
        }
        root
    }
}
