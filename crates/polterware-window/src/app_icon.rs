//! Runtime app icon switching on macOS: updates the running Dock icon
//! (`NSApplication.applicationIconImage`) and, for an `.app` bundle, the Finder/Dock bundle
//! icon (`NSWorkspace`) so the choice persists while the app is closed.

#[cfg(any(target_os = "macos", test))]
#[derive(Debug, PartialEq, Eq)]
enum ApplyPlan<'a> {
    Set {
        bytes: &'a [u8],
        update_bundle: bool,
    },
    ClearBundle,
}

#[cfg(any(target_os = "macos", test))]
fn apply_plan<'a>(
    png: Option<&'a [u8]>,
    fallback_png: &'static [u8],
    bundled: bool,
) -> ApplyPlan<'a> {
    match (png, bundled) {
        (Some(bytes), update_bundle) => ApplyPlan::Set {
            bytes,
            update_bundle,
        },
        (None, false) => ApplyPlan::Set {
            bytes: fallback_png,
            update_bundle: false,
        },
        (None, true) => ApplyPlan::ClearBundle,
    }
}

/// Applies `png` as the app's icon, or resets to the default icon when `png` is `None`.
///
/// With `Some(png)`, sets `NSApplication.applicationIconImage` and, when running from an `.app`
/// bundle, also updates the bundle's Finder/Dock icon via `NSWorkspace`. With `None`, a bundled
/// app clears both overrides back to its compiled Icon Composer icon, and an unbundled build
/// shows `fallback_png` as [`crate::dock_icon::install_unbundled`] does. Failures (invalid PNG
/// bytes or a read-only bundle) are ignored silently.
#[cfg(target_os = "macos")]
pub fn apply(png: Option<&[u8]>, fallback_png: &'static [u8]) {
    use objc::rc::autoreleasepool;
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};

    let plan = apply_plan(png, fallback_png, crate::dock_icon::is_bundled());
    autoreleasepool(|| {
        // SAFETY: plain AppKit/Foundation calls on the main thread; every returned pointer is
        // checked for null before use, `NSData` copies the slice bytes, failures are ignored,
        // and any `NSImage` allocated here is released before returning.
        unsafe {
            let (image, update_bundle): (*mut Object, bool) = match plan {
                ApplyPlan::Set {
                    bytes,
                    update_bundle,
                } => {
                    let image = image_from_png(bytes);
                    if image.is_null() {
                        return;
                    }
                    (image, update_bundle)
                }
                ApplyPlan::ClearBundle => (std::ptr::null_mut(), true),
            };
            let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
            if !app.is_null() {
                let () = msg_send![app, setApplicationIconImage: image];
            }
            if update_bundle {
                set_bundle_icon(image);
            }
            if !image.is_null() {
                let () = msg_send![image, release];
            }
        }
    });
}

#[cfg(target_os = "macos")]
unsafe fn image_from_png(bytes: &[u8]) -> *mut objc::runtime::Object {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};

    if bytes.is_empty() {
        return std::ptr::null_mut();
    }
    // SAFETY: caller ensures a Foundation/AppKit autorelease pool and main-thread context;
    // `dataWithBytes:length:` copies `bytes`, and `initWithData:` returns null on invalid data.
    unsafe {
        let data: *mut Object = msg_send![class!(NSData),
            dataWithBytes: bytes.as_ptr()
            length: bytes.len()];
        if data.is_null() {
            return std::ptr::null_mut();
        }
        let image: *mut Object = msg_send![class!(NSImage), alloc];
        if image.is_null() {
            return std::ptr::null_mut();
        }
        msg_send![image, initWithData: data]
    }
}

#[cfg(target_os = "macos")]
unsafe fn set_bundle_icon(image: *mut objc::runtime::Object) {
    use objc::runtime::{BOOL, Object};
    use objc::{class, msg_send, sel, sel_impl};

    // SAFETY: plain AppKit/Foundation calls on the main thread; null bundle/path/workspace
    // pointers are checked before use and any `NO` result (e.g. a read-only bundle) is ignored.
    unsafe {
        let bundle: *mut Object = msg_send![class!(NSBundle), mainBundle];
        if bundle.is_null() {
            return;
        }
        let path: *mut Object = msg_send![bundle, bundlePath];
        if path.is_null() {
            return;
        }
        let workspace: *mut Object = msg_send![class!(NSWorkspace), sharedWorkspace];
        if workspace.is_null() {
            return;
        }
        let _: BOOL = msg_send![workspace, setIcon: image forFile: path options: 0usize];
    }
}

#[cfg(not(target_os = "macos"))]
pub fn apply(_png: Option<&[u8]>, _fallback_png: &'static [u8]) {}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{ApplyPlan, apply_plan};

    static FALLBACK: &[u8] = b"fallback";

    #[test]
    fn custom_icon_updates_bundle_only_when_bundled() {
        let custom = b"custom";
        assert_eq!(
            apply_plan(Some(custom), FALLBACK, false),
            ApplyPlan::Set {
                bytes: custom,
                update_bundle: false,
            }
        );
        assert_eq!(
            apply_plan(Some(custom), FALLBACK, true),
            ApplyPlan::Set {
                bytes: custom,
                update_bundle: true,
            }
        );
    }

    #[test]
    fn default_icon_clears_when_bundled_and_falls_back_when_unbundled() {
        assert_eq!(
            apply_plan(None, FALLBACK, false),
            ApplyPlan::Set {
                bytes: FALLBACK,
                update_bundle: false,
            }
        );
        assert_eq!(apply_plan(None, FALLBACK, true), ApplyPlan::ClearBundle);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn bundle_detection_matches_macos_app_executable_paths() {
        use crate::dock_icon::is_bundled_exe;

        assert!(is_bundled_exe(Some(Path::new(
            "/Applications/Dispatch.app/Contents/MacOS/dispatch"
        ))));
        assert!(!is_bundled_exe(Some(Path::new(
            "/Users/dev/target/debug/dispatch"
        ))));
        assert!(!is_bundled_exe(None));
    }
}
