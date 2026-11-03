//! The macOS window frame every Polterware app shares: a transparent window with the
//! traffic lights over the sidebar, the vibrancy material under the shell, and window drag
//! from the empty parts of the chrome.

pub mod capture;
pub mod vibrancy;
pub mod window_drag;

use gpui::{
    App, Bounds, Pixels, SharedString, Size, TitlebarOptions, WindowBackgroundAppearance,
    WindowBounds, WindowKind, WindowOptions, point, px,
};

pub use window_drag::WindowDragRegion;

/// Where the traffic lights sit, from the window's top left corner.
pub const TRAFFIC_LIGHTS: (f32, f32) = (18.0, 18.0);

/// The options of an app's main window: centered at `size`, no smaller than `min_size`,
/// transparent so the vibrancy under the content shows through the shell, with a
/// transparent title bar and the traffic lights at [`TRAFFIC_LIGHTS`].
pub fn window_options(
    title: impl Into<SharedString>,
    size: Size<Pixels>,
    min_size: Size<Pixels>,
    cx: &App,
) -> WindowOptions {
    let bounds = Bounds::centered(None, size, cx);
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(min_size),
        kind: WindowKind::Normal,
        window_background: WindowBackgroundAppearance::Transparent,
        titlebar: Some(TitlebarOptions {
            title: Some(title.into()),
            appears_transparent: true,
            traffic_light_position: Some(point(px(TRAFFIC_LIGHTS.0), px(TRAFFIC_LIGHTS.1))),
        }),
        ..Default::default()
    }
}
