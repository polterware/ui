//! Electron `PageToolbar` over `PageContent` (`structure.css`): a transparent toolbar
//! overlays the independently scrolling panes; each pane pads its content by the measured
//! toolbar height. Like gpui-kit's overlay placement, layout owns geometry rather than
//! estimating it from the number of controls. Notifications stop when the height settles.
use gpui::{
    AnyElement, App, Context, Entity, IntoElement, RenderOnce, Window, canvas, div, prelude::*,
};

/// The actual toolbar height, including wrapped filter clauses.
pub struct FloatingToolbar {
    height: f32,
}
impl FloatingToolbar {
    pub fn new(initial_height: f32) -> Self {
        Self {
            height: initial_height,
        }
    }
    pub fn height(&self) -> f32 {
        self.height
    }
    fn measure(&mut self, height: f32, cx: &mut Context<Self>) {
        if (height - self.height).abs() > 0.5 {
            self.height = height;
            cx.notify();
        }
    }
}

/// A toolbar floating over a full-height body. Hosts observe `state` and use its height
/// for each scroll's leading inset, so that content scrolls underneath the chrome.
pub fn floating_toolbar(
    state: Entity<FloatingToolbar>,
    toolbar: impl IntoElement,
    body: impl IntoElement,
) -> impl IntoElement {
    FloatingPage {
        state,
        toolbar: toolbar.into_any_element(),
        body: body.into_any_element(),
    }
}
#[derive(IntoElement)]
struct FloatingPage {
    state: Entity<FloatingToolbar>,
    toolbar: AnyElement,
    body: AnyElement,
}
impl RenderOnce for FloatingPage {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let state = self.state;
        div().relative().size_full().child(self.body).child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .w_full()
                .child(self.toolbar)
                .child(
                    canvas(
                        move |bounds, _, cx| {
                            state.update(cx, |state, cx| {
                                state.measure(f32::from(bounds.size.height), cx)
                            });
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                ),
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{TestAppContext, px, size};
    use std::{cell::Cell, rc::Rc};
    #[gpui::test]
    fn measures_wrapped_chrome_and_stops_notifying_when_unchanged(cx: &mut TestAppContext) {
        let height = Rc::new(Cell::new(44.0));
        let changes = Rc::new(Cell::new(0));
        let state = cx.update(|cx| cx.new(|_| FloatingToolbar::new(0.0)));
        let listener = changes.clone();
        cx.update(|cx| {
            cx.observe(&state, move |_, _| listener.set(listener.get() + 1))
                .detach();
        });
        let measured = state.clone();
        let wanted = height.clone();
        let mut visual = crate::test_support::open(cx, size(400.0, 300.0), move |_, _| {
            floating_toolbar(
                measured.clone(),
                div().h(px(wanted.get())),
                div().size_full(),
            )
            .into_any_element()
        });
        assert_eq!(visual.read(|cx| state.read(cx).height()), 44.0);
        height.set(80.0);
        crate::test_support::draw(&mut visual);
        assert_eq!(visual.read(|cx| state.read(cx).height()), 80.0);
        let count = changes.get();
        crate::test_support::draw(&mut visual);
        assert_eq!(
            changes.get(),
            count,
            "idle layout must not request another repaint"
        );
    }
}
