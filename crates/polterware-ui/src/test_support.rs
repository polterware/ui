//! What the shared controls' behavior tests share: a window on the test platform with the
//! theme and the key bindings the app installs, and a way to draw a frame on demand.

use std::time::Duration;

use gpui::{
    AnyElement, App, Bounds, Context, IntoElement, Render, Size, TestAppContext, VisualTestContext,
    Window, WindowBounds, WindowOptions, div, point, prelude::*, px, size,
};

use crate::theme::Theme;

type Build = Box<dyn Fn(&mut Window, &mut Context<Harness>) -> AnyElement>;

/// A root view that draws whatever `build` returns, filling the window.
pub struct Harness {
    build: Build,
}

impl Render for Harness {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = (self.build)(window, cx);
        div().size_full().child(content)
    }
}

/// Opens a `width` × `height` window whose root draws `build`, once the theme and the key
/// bindings are in place, and makes it the active window.
pub fn open(
    cx: &mut TestAppContext,
    window_size: Size<f32>,
    build: impl Fn(&mut Window, &mut Context<Harness>) -> AnyElement + 'static,
) -> VisualTestContext {
    let window = cx.update(|cx: &mut App| {
        cx.set_global(Theme::dark());
        super::register_key_bindings(cx);
        let bounds = Bounds::new(
            point(px(0.0), px(0.0)),
            size(px(window_size.width), px(window_size.height)),
        );
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            move |_, cx| {
                cx.new(|_| Harness {
                    build: Box::new(build),
                })
            },
        )
        .expect("the test window opens")
    });
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.update(|window, _| window.activate_window());
    draw(&mut cx);
    cx
}

/// Draws the next frame now.
pub fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear();
    });
    cx.run_until_parked();
}

/// Lets springs that run on the wall clock settle, drawing on the way.
pub fn settle(cx: &mut VisualTestContext) {
    for _ in 0..4 {
        draw(cx);
        std::thread::sleep(Duration::from_millis(120));
    }
    draw(cx);
}
