//! Drawing again from inside a draw.
//!
//! A canvas that measures something in prepaint or paint (a scroll's overflow, a page's
//! width, a viewer's fit) often needs one more frame drawn with what it learned. GPUI drops a
//! `window.refresh()` asked for while it draws, so a measurement made there would wait for
//! the next unrelated event; on a still screen it waited forever. [`draw_again`] asks for the
//! next frame instead, which always comes.

use gpui::Window;

/// Draws the window once more on the next frame. Safe to call while drawing.
pub fn draw_again(window: &Window) {
    window.on_next_frame(|window, _| window.refresh());
}
