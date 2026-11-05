//! The system's scroller style (`NSScroller.preferredScrollerStyle`): macOS draws overlay
//! scrollers, which take no room and show only while a view scrolls, unless the person asked
//! to always show scroll bars (or a mouse makes the system choose legacy ones). Chromium
//! follows the same preference, so the Electron app's scroll boxes do too.

use std::sync::atomic::{AtomicU8, Ordering};

/// 0 unread, 1 legacy, 2 overlay.
static STYLE: AtomicU8 = AtomicU8::new(0);

/// Whether scrollers overlay the content (`NSScrollerStyleOverlay`). Read once; a test or
/// an app may pin it with [`set_overlay_scrollers`].
pub fn overlay_scrollers() -> bool {
    match STYLE.load(Ordering::Relaxed) {
        1 => false,
        2 => true,
        _ => {
            let overlay = system_overlay();
            STYLE.store(if overlay { 2 } else { 1 }, Ordering::Relaxed);
            overlay
        }
    }
}

pub fn set_overlay_scrollers(overlay: bool) {
    STYLE.store(if overlay { 2 } else { 1 }, Ordering::Relaxed);
}

#[cfg(target_os = "macos")]
fn system_overlay() -> bool {
    use objc::{class, msg_send, sel, sel_impl};
    // SAFETY: a class method that returns an `NSScrollerStyle` (an NSInteger).
    let style: isize = unsafe { msg_send![class!(NSScroller), preferredScrollerStyle] };
    style == 1
}

#[cfg(not(target_os = "macos"))]
fn system_overlay() -> bool {
    false
}
