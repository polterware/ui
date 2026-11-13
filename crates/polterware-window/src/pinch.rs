//! The trackpad's pinch. GPUI 0.2 drops AppKit's magnify events, so a local event monitor
//! reads them before GPUI sees them and hands each step's magnification to the app on its
//! own thread. The event still goes on to GPUI unchanged.

use gpui::App;

/// Calls `handler` with each pinch step's magnification (0.1 is 10% larger, negative is
/// smaller) for as long as the app runs. Installs one monitor per call; call it once.
#[cfg(target_os = "macos")]
pub fn on_pinch(cx: &mut App, handler: impl Fn(f32, &mut App) + 'static) {
    use block::ConcreteBlock;
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};

    /// `NSEventMaskMagnify`: `1 << NSEventTypeMagnify` (30).
    const MAGNIFY_MASK: u64 = 1 << 30;

    let (sender, receiver) = async_channel::unbounded::<f32>();
    let monitor = ConcreteBlock::new(move |event: *mut Object| -> *mut Object {
        if !event.is_null() {
            // SAFETY: AppKit calls the monitor on the main thread with a live magnify
            // event, whose `magnification` is a plain getter.
            let magnification: f64 = unsafe { msg_send![event, magnification] };
            let _ = sender.try_send(magnification as f32);
        }
        event
    })
    .copy();
    // SAFETY: the block lives as long as the app (it is leaked below) and AppKit copies
    // it; the call registers a monitor on the main thread, where GPUI runs the app.
    unsafe {
        let _: *mut Object = msg_send![
            class!(NSEvent),
            addLocalMonitorForEventsMatchingMask: MAGNIFY_MASK
            handler: &*monitor
        ];
    }
    std::mem::forget(monitor);
    cx.spawn(async move |cx| {
        while let Ok(step) = receiver.recv().await {
            if cx.update(|cx| handler(step, cx)).is_err() {
                break;
            }
        }
    })
    .detach();
}

#[cfg(not(target_os = "macos"))]
pub fn on_pinch(_cx: &mut App, _handler: impl Fn(f32, &mut App) + 'static) {}
