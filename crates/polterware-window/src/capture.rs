//! The window's own pixels as a PNG, for screenshot tours and visual comparisons. It reads
//! the window's backing store, so it needs no Screen Recording permission.

use std::path::Path;

use gpui::Window;

/// A region of the window image, in device pixels: x, y, width, height.
pub type Crop = (u32, u32, u32, u32);

/// Saves the window's own pixels (no shadow or frame) as a PNG, cut to `crop` when given.
#[cfg(target_os = "macos")]
pub fn save_window_png(window: &Window, path: &Path, crop: Option<Crop>) -> Result<(), String> {
    use objc::runtime::Object;
    use objc::{msg_send, sel, sel_impl};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::ffi::c_void;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGPoint {
        x: f64,
        y: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGSize {
        width: f64,
        height: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGRect {
        origin: CGPoint,
        size: CGSize,
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        static CGRectNull: CGRect;
        fn CGWindowListCreateImage(
            bounds: CGRect,
            options: u32,
            window: u32,
            image_options: u32,
        ) -> *mut c_void;
        fn CGImageGetWidth(image: *mut c_void) -> usize;
        fn CGImageGetHeight(image: *mut c_void) -> usize;
        static kCGColorSpaceSRGB: *const c_void;
        fn CGColorSpaceCreateWithName(name: *const c_void) -> *mut c_void;
        fn CGColorSpaceRelease(space: *mut c_void);
        fn CGBitmapContextCreate(
            data: *mut c_void,
            width: usize,
            height: usize,
            bits_per_component: usize,
            bytes_per_row: usize,
            space: *mut c_void,
            bitmap_info: u32,
        ) -> *mut c_void;
        fn CGContextDrawImage(context: *mut c_void, rect: CGRect, image: *mut c_void);
        fn CGContextRelease(context: *mut c_void);
        fn CGImageRelease(image: *mut c_void);
    }
    const INCLUDING_WINDOW: u32 = 1 << 3;
    const BOUNDS_IGNORE_FRAMING: u32 = 1 << 0;
    const BEST_RESOLUTION: u32 = 1 << 3;
    const PREMULTIPLIED_LAST: u32 = 1;
    const BYTE_ORDER_32_BIG: u32 = 4 << 12;

    let handle = HasWindowHandle::window_handle(window).map_err(|e| e.to_string())?;
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return Err("not an AppKit window".into());
    };
    // SAFETY: main thread, live window; every CoreGraphics object created here
    // is released before returning.
    unsafe {
        let view = appkit.ns_view.as_ptr() as *mut Object;
        let ns_window: *mut Object = msg_send![view, window];
        if ns_window.is_null() {
            return Err("no NSWindow".into());
        }
        let number: isize = msg_send![ns_window, windowNumber];
        let image = CGWindowListCreateImage(
            CGRectNull,
            INCLUDING_WINDOW,
            number as u32,
            BOUNDS_IGNORE_FRAMING | BEST_RESOLUTION,
        );
        if image.is_null() {
            return Err("CGWindowListCreateImage returned nothing".into());
        }
        let width = CGImageGetWidth(image);
        let height = CGImageGetHeight(image);
        // The window server hands the image over in the display's color space (P3 on a
        // MacBook); drawing it into an sRGB context converts it, so captures compare with
        // the browser's sRGB screenshots value for value.
        let mut bytes = vec![0u8; width * height * 4];
        let space = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
        let context = CGBitmapContextCreate(
            bytes.as_mut_ptr() as *mut c_void,
            width,
            height,
            8,
            width * 4,
            space,
            PREMULTIPLIED_LAST | BYTE_ORDER_32_BIG,
        );
        CGColorSpaceRelease(space);
        if context.is_null() {
            CGImageRelease(image);
            return Err("no sRGB bitmap context".into());
        }
        let rect = CGRect {
            origin: CGPoint { x: 0.0, y: 0.0 },
            size: CGSize {
                width: width as f64,
                height: height as f64,
            },
        };
        CGContextDrawImage(context, rect, image);
        CGContextRelease(context);
        CGImageRelease(image);
        let mut rgba = Vec::with_capacity(width * height * 4);
        for px in bytes.chunks_exact(4) {
            let a = px[3] as u32;
            let un = |c: u8| {
                if a == 0 {
                    0
                } else {
                    ((c as u32 * 255 + a / 2) / a).min(255) as u8
                }
            };
            rgba.extend_from_slice(&[un(px[0]), un(px[1]), un(px[2]), px[3]]);
        }
        let (x, y, w, h) = crop.unwrap_or((0, 0, width as u32, height as u32));
        let w = w.min((width as u32).saturating_sub(x));
        let h = h.min((height as u32).saturating_sub(y));
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        for row in y..y + h {
            let start = (row as usize * width + x as usize) * 4;
            out.extend_from_slice(&rgba[start..start + w as usize * 4]);
        }
        image::save_buffer(path, &out, w, h, image::ColorType::Rgba8).map_err(|e| e.to_string())
    }
}

#[cfg(not(target_os = "macos"))]
pub fn save_window_png(_window: &Window, _path: &Path, _crop: Option<Crop>) -> Result<(), String> {
    Err("screenshots are macOS only".into())
}
