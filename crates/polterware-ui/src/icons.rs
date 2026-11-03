//! Icon elements over the embedded Hugeicons marks of `polterware-assets`, and the asset
//! source an app installs with `Application::with_assets`.

use std::borrow::Cow;

use gpui::prelude::*;
use gpui::{AssetSource, Hsla, SharedString, px, svg};
pub use polterware_assets::Icon;
use polterware_assets::IconSource;

use crate::theme::Theme;

/// Every asset the controls draw: the icons and the segment display's glyphs. An app with
/// assets of its own puts them first and falls back on this one.
#[derive(Clone, Copy, Debug, Default)]
pub struct IconAssets;

impl AssetSource for IconAssets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        if path.starts_with("segments/") {
            return Ok(
                super::segment_display::glyph_svg(path).map(|svg| Cow::Owned(svg.into_bytes()))
            );
        }
        IconSource.load(path)
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        IconSource.list(path)
    }
}

/// Paints an icon in the primary text color.
pub fn icon(name: Icon, size_px: f32, cx: &gpui::App) -> gpui::AnyElement {
    icon_colored(name, size_px, Theme::global(cx).color_text_primary)
}

/// Paints an icon in `color`, turned by `radians` about its center (a CSS `rotate()`).
pub fn icon_rotated(
    name: Icon,
    size_px: f32,
    color: impl Into<Hsla>,
    radians: f32,
) -> gpui::AnyElement {
    svg()
        .path(name.asset_path())
        .size(px(size_px))
        .flex_none()
        .text_color(color.into())
        .with_transformation(gpui::Transformation::rotate(gpui::radians(radians)))
        .into_any_element()
}

/// Paints an icon in `color` with its strokes at `stroke` instead of 1.5.
pub fn icon_stroked(
    name: Icon,
    size_px: f32,
    color: impl Into<Hsla>,
    stroke: f32,
) -> gpui::AnyElement {
    svg()
        .path(name.asset_path_stroked(stroke))
        .size(px(size_px))
        .flex_none()
        .text_color(color.into())
        .into_any_element()
}

/// Paints an icon in `color` (the design system's `currentColor`).
pub fn icon_colored(name: Icon, size_px: f32, color: impl Into<Hsla>) -> gpui::AnyElement {
    svg()
        .path(name.asset_path())
        .size(px(size_px))
        .flex_none()
        .text_color(color.into())
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{Icon, IconAssets};
    use gpui::AssetSource;

    #[test]
    fn icons_and_segments_load_through_one_source() {
        let assets = IconAssets;
        assert!(assets.load(&Icon::Bot.asset_path()).unwrap().is_some());
        assert!(assets.load("icons/missing.svg").unwrap().is_none());
    }
}
