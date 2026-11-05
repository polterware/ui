//! Hot reload spike: with the `hot-reload` feature, `dx serve --hot-patch` sends patched code
//! to the running gallery through Subsecond, and every window draws again with it. Without the
//! feature, `call` is a plain call and nothing here is compiled in.

use gpui::App;

/// Runs `f` through Subsecond's jump table, so the newest patched version of the code it
/// reaches is what runs.
#[cfg(feature = "hot-reload")]
pub fn call<O>(f: impl FnMut() -> O) -> O {
    subsecond::call(f)
}

#[cfg(not(feature = "hot-reload"))]
#[inline(always)]
pub fn call<O>(mut f: impl FnMut() -> O) -> O {
    f()
}

/// Connects to the `dx` devserver and redraws every window when a patch lands.
#[cfg(feature = "hot-reload")]
pub fn init(cx: &mut App) {
    use futures::StreamExt as _;
    use std::sync::Arc;

    dioxus_devtools::connect_subsecond();
    let (tx, mut rx) = futures::channel::mpsc::unbounded::<()>();
    subsecond::register_handler(Arc::new(move || {
        let _ = tx.unbounded_send(());
    }));
    cx.spawn(async move |cx| {
        while rx.next().await.is_some() {
            eprintln!("[hot-reload] patch applied, redrawing");
            if cx.update(|cx| cx.refresh_windows()).is_err() {
                break;
            }
        }
    })
    .detach();
}

#[cfg(not(feature = "hot-reload"))]
pub fn init(_cx: &mut App) {}

// `dx`'s hot-patch link drops the frameworks `io-surface` asks for; name it again here.
#[cfg(all(feature = "hot-reload", target_os = "macos"))]
#[link(name = "IOSurface", kind = "framework")]
unsafe extern "C" {}
