//! The Dock icon of a build run from `cargo run`. Such a build has no bundle, so no
//! `Assets.car` and no icon of its own; the app hands over its icon's PNG, the Icon Composer
//! export on the macOS icon grid, and the Dock shows that. A bundled app keeps its compiled
//! Icon Composer icon, so this does nothing there.

/// Shows `png` as the app's Dock icon when the binary does not run from an `.app` bundle.
#[cfg(target_os = "macos")]
pub fn install_unbundled(png: &'static [u8]) {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};

    let bundled = std::env::current_exe()
        .is_ok_and(|exe| exe.to_string_lossy().contains(".app/Contents/MacOS/"));
    if bundled {
        return;
    }
    // SAFETY: plain AppKit calls on the main thread, where GPUI runs its startup; the NSData
    // copies the bytes and a null image is checked before it is used.
    unsafe {
        let data: *mut Object = msg_send![class!(NSData),
            dataWithBytes: png.as_ptr()
            length: png.len()];
        let image: *mut Object = msg_send![class!(NSImage), alloc];
        let image: *mut Object = msg_send![image, initWithData: data];
        if image.is_null() {
            return;
        }
        let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
        let () = msg_send![app, setApplicationIconImage: image];
    }
}

#[cfg(not(target_os = "macos"))]
pub fn install_unbundled(_png: &'static [u8]) {}
