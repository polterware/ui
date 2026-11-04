//! Window dragging from any surface, the way the Electron app drags from its
//! sidebar and toolbars.
//!
//! gpui 0.2.2 leaves `Window::start_window_move` empty on macOS and only the
//! native titlebar strip moves a frameless window, so a drag region asks AppKit
//! directly: `-[NSWindow performWindowDragWithEvent:]` with the current mouse
//! event. The drag starts once the pointer moves a few pixels with the button
//! down, so a press that ends as a click still reaches buttons inside the
//! region. A double click on the region's empty area follows the system's
//! titlebar preference (zoom or minimize); on a control inside it, one marked
//! [`WindowDragRegion::window_drag_control`], it is the control's, as a double
//! click on a toolbar button never zooms a native window.

use std::cell::Cell;

use gpui::{InteractiveElement, MouseButton, Pixels, Point, Window, px};

const DRAG_SLOP: f32 = 4.0;

#[derive(Default)]
struct PendingDrag {
    origin: Option<Point<Pixels>>,
}

thread_local! {
    static PENDING: std::cell::RefCell<PendingDrag> = std::cell::RefCell::new(PendingDrag::default());
    /// Where a control inside a region was last pressed. A control's listener runs before
    /// the region's (the bubble phase goes from the innermost element out), so the region
    /// reads the press it is handling.
    static CLAIMED: Cell<Option<Point<Pixels>>> = const { Cell::new(None) };
}

/// A control took the press at `position`.
fn claim(position: Point<Pixels>) {
    CLAIMED.with(|claimed| claimed.set(Some(position)));
}

/// Whether a control took the press at `position`, forgetting any claim either way, so a
/// claim the region never read cannot hold over a later press.
fn take_claim(position: Point<Pixels>) -> bool {
    CLAIMED.with(|claimed| claimed.take()) == Some(position)
}

/// Adds window dragging to an element's empty area.
pub trait WindowDragRegion: InteractiveElement + Sized {
    fn window_drag_region(self) -> Self {
        self.on_mouse_down(MouseButton::Left, |event, window, _cx| {
            let on_a_control = take_claim(event.position);
            if event.click_count >= 2 {
                PENDING.with(|p| p.borrow_mut().origin = None);
                if !on_a_control {
                    window.titlebar_double_click();
                }
                return;
            }
            PENDING.with(|p| p.borrow_mut().origin = Some(event.position));
        })
        .on_mouse_up(MouseButton::Left, |_, _, _| {
            PENDING.with(|p| p.borrow_mut().origin = None);
        })
        .on_mouse_move(|event, window, cx| {
            if event.pressed_button != Some(MouseButton::Left) || cx.has_active_drag() {
                PENDING.with(|p| p.borrow_mut().origin = None);
                return;
            }
            let Some(origin) = PENDING.with(|p| p.borrow().origin) else {
                return;
            };
            let moved = event.position - origin;
            if moved.x.abs() < px(DRAG_SLOP) && moved.y.abs() < px(DRAG_SLOP) {
                return;
            }
            PENDING.with(|p| p.borrow_mut().origin = None);
            start_native_drag(window);
        })
    }

    /// Marks a control inside a drag region (a button, a row, a section's heading): a double
    /// click on it is its own and never zooms the window. A press that moves still drags the
    /// window from it, as from the region around it. Outside a region it does nothing.
    fn window_drag_control(self) -> Self {
        self.on_mouse_down(MouseButton::Left, |event, _, _| claim(event.position))
    }
}

impl<E: InteractiveElement + Sized> WindowDragRegion for E {}

#[cfg(target_os = "macos")]
fn start_native_drag(window: &Window) {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return;
    };
    // SAFETY: the handle comes from the live gpui window on the main thread;
    // `window` and `currentEvent` are plain AppKit getters, and the drag runs
    // AppKit's own tracking loop for the current mouse event.
    unsafe {
        let view = appkit.ns_view.as_ptr() as *mut Object;
        let ns_window: *mut Object = msg_send![view, window];
        if ns_window.is_null() {
            return;
        }
        let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
        let event: *mut Object = msg_send![app, currentEvent];
        if event.is_null() {
            return;
        }
        let _: () = msg_send![ns_window, performWindowDragWithEvent: event];
    }
}

#[cfg(not(target_os = "macos"))]
fn start_native_drag(window: &Window) {
    window.start_window_move();
}

#[cfg(test)]
mod tests {
    use gpui::{point, px};

    use super::{claim, take_claim};

    #[test]
    fn a_press_a_control_took_is_not_the_regions() {
        let on = point(px(10.0), px(20.0));
        claim(on);
        assert!(take_claim(on), "the control's press");
        // Read once: the next press at the same spot is the region's unless claimed again.
        assert!(!take_claim(on));
    }

    #[test]
    fn a_claim_the_region_never_read_does_not_hold_over() {
        claim(point(px(10.0), px(20.0)));
        // A press elsewhere, on the region's empty area, clears it.
        assert!(!take_claim(point(px(200.0), px(20.0))));
        assert!(!take_claim(point(px(10.0), px(20.0))));
    }
}
