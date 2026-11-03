//! The window material behind the shell, as Electron's `vibrancy: "under-window"` with
//! `visualEffectState: "active"` draws it: an `NSVisualEffectView` with the
//! UnderWindowBackground material, blending what is behind the window, always active, under
//! GPUI's view. The window is transparent, so the shell glass (`--color-shell-glass`) the
//! root paints sits over the material, as it does over Electron's.
//!
//! GPUI's own `WindowBackgroundAppearance::Blurred` uses another material and strips its
//! tint and saturation, which reads lighter and flatter than Electron's sidebar.

use gpui::Window;

/// `NSVisualEffectMaterialUnderWindowBackground`.
#[cfg(target_os = "macos")]
const MATERIAL_UNDER_WINDOW: i64 = 21;
/// `NSVisualEffectBlendingModeBehindWindow`.
#[cfg(target_os = "macos")]
const BLENDING_BEHIND_WINDOW: i64 = 0;
/// `NSVisualEffectStateActive`.
#[cfg(target_os = "macos")]
const STATE_ACTIVE: i64 = 1;
/// `NSViewWidthSizable | NSViewHeightSizable`.
#[cfg(target_os = "macos")]
const SIZABLE: u64 = 2 | 16;
/// `NSWindowBelow`.
#[cfg(target_os = "macos")]
const BELOW: i64 = -1;

/// Adds the material under the window's content. Call once, on a window opened with
/// `WindowBackgroundAppearance::Transparent`.
#[cfg(target_os = "macos")]
pub fn install(window: &Window) {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Rect {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    }

    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return;
    };
    // SAFETY: the handle comes from the live GPUI window on the main thread. The calls are
    // plain AppKit setters on a view this function creates and hands to the window's content
    // view, which retains it; the local reference is released after.
    unsafe {
        let view = appkit.ns_view.as_ptr() as *mut Object;
        let ns_window: *mut Object = msg_send![view, window];
        if ns_window.is_null() {
            return;
        }
        let content: *mut Object = msg_send![ns_window, contentView];
        if content.is_null() {
            return;
        }
        let bounds: Rect = msg_send![content, bounds];
        let effect: *mut Object = msg_send![class!(NSVisualEffectView), alloc];
        let effect: *mut Object = msg_send![effect, initWithFrame: bounds];
        if effect.is_null() {
            return;
        }
        let _: () = msg_send![effect, setAutoresizingMask: SIZABLE];
        let _: () = msg_send![effect, setMaterial: MATERIAL_UNDER_WINDOW];
        let _: () = msg_send![effect, setBlendingMode: BLENDING_BEHIND_WINDOW];
        let _: () = msg_send![effect, setState: STATE_ACTIVE];
        let nil: *mut Object = std::ptr::null_mut();
        let _: () = msg_send![content, addSubview: effect positioned: BELOW relativeTo: nil];
        let _: () = msg_send![effect, release];
    }
}

#[cfg(not(target_os = "macos"))]
pub fn install(_window: &Window) {}
