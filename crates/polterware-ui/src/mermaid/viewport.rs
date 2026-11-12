//! Where a diagram sits on screen: the pan and zoom over a [`Scene`](polterware_mermaid::scene::Scene).
//!
//! The viewport is the canvas's (`canvas::geometry::Viewport`): a scene point `p` lands at
//! `p * zoom + (x, y)` in the element's own pixels, y growing down. The bounds of the element
//! are added last, so event positions are turned into scene points by taking the element origin
//! off first. Unlike the canvas editor's 0.25–2.5 range, a diagram zooms out far enough to see a
//! whole large flowchart and in far enough to read a tiny label.

use gpui::{Bounds, Pixels, Point, point, px};

use polterware_mermaid::scene::{Point as ScenePoint, Rect};

/// The smallest zoom a diagram goes to (`0.05`, a twentieth).
pub const MIN_ZOOM: f32 = 0.05;
/// The largest zoom a diagram goes to (`8`, eight times).
pub const MAX_ZOOM: f32 = 8.0;

pub fn clamp_zoom(zoom: f32) -> f32 {
    if zoom.is_finite() {
        zoom.clamp(MIN_ZOOM, MAX_ZOOM)
    } else {
        1.0
    }
}

/// Where a scene sits in an element: a scene point `p` is drawn at
/// `p * zoom + (x, y)` element pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneViewport {
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
}

impl Default for SceneViewport {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            zoom: 1.0,
        }
    }
}

impl SceneViewport {
    pub fn new(x: f32, y: f32, zoom: f32) -> Self {
        Self {
            x,
            y,
            zoom: clamp_zoom(zoom),
        }
    }

    /// The scene point under an element-local pixel.
    pub fn to_scene(self, at: Point<Pixels>) -> ScenePoint {
        ScenePoint::new(
            (f32::from(at.x) - self.x) as f64 / self.zoom as f64,
            (f32::from(at.y) - self.y) as f64 / self.zoom as f64,
        )
    }

    /// The element-local pixel a scene point lands on.
    pub fn to_local(self, at: ScenePoint) -> Point<Pixels> {
        point(
            px(at.x as f32 * self.zoom + self.x),
            px(at.y as f32 * self.zoom + self.y),
        )
    }

    /// The window pixel a scene point lands on.
    pub fn to_window(self, at: ScenePoint, bounds: Bounds<Pixels>) -> Point<Pixels> {
        let local = self.to_local(at);
        point(bounds.origin.x + local.x, bounds.origin.y + local.y)
    }

    /// The window rectangle a scene rectangle covers.
    pub fn window_rect(self, rect: Rect, bounds: Bounds<Pixels>) -> Bounds<Pixels> {
        Bounds::new(
            self.to_window(ScenePoint::new(rect.x, rect.y), bounds),
            gpui::size(
                px(rect.width as f32 * self.zoom),
                px(rect.height as f32 * self.zoom),
            ),
        )
    }

    pub fn pan_by(self, dx: f32, dy: f32) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
            ..self
        }
    }

    /// Zooms keeping the scene point under `focus` (element-local pixels) fixed on screen.
    pub fn zoom_at(self, focus: Point<Pixels>, zoom: f32) -> Self {
        let zoom = clamp_zoom(zoom);
        let at = self.to_scene(focus);
        Self {
            zoom,
            x: f32::from(focus.x) - at.x as f32 * zoom,
            y: f32::from(focus.y) - at.y as f32 * zoom,
        }
    }

    /// Fits `view_box` in a `width` × `height` element, `padding` pixels in, never past 100%.
    /// This is the SVG view box's job in upstream (`setupViewPortForSVG.ts`): the whole drawing
    /// is visible when it opens.
    pub fn fit(view_box: Rect, width: f32, height: f32, padding: f32) -> Self {
        let view_width = view_box.width as f32;
        let view_height = view_box.height as f32;
        if !(view_width > 0.0 && view_height > 0.0 && width > 0.0 && height > 0.0) {
            return Self::default();
        }
        let padding = padding.max(0.0);
        let room_width = (width - padding * 2.0).max(1.0);
        let room_height = (height - padding * 2.0).max(1.0);
        let zoom = clamp_zoom(
            (room_width / view_width)
                .min(room_height / view_height)
                .min(1.0),
        );
        let center = view_box.center();
        Self {
            zoom,
            x: width / 2.0 - center.x as f32 * zoom,
            y: height / 2.0 - center.y as f32 * zoom,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use polterware_mermaid::scene::Point as ScenePoint;

    #[test]
    fn fit_centers_the_view_box_and_never_upscales() {
        let view_box = Rect::new(10.0, 20.0, 100.0, 50.0);
        let viewport = SceneViewport::fit(view_box, 400.0, 200.0, 20.0);
        assert_eq!(viewport.zoom, 1.0);
        let center = viewport.to_local(view_box.center());
        assert_eq!(f32::from(center.x), 200.0);
        assert_eq!(f32::from(center.y), 100.0);
    }

    #[test]
    fn fit_scales_down_a_view_box_that_does_not_fit() {
        let view_box = Rect::new(0.0, 0.0, 1000.0, 1000.0);
        let viewport = SceneViewport::fit(view_box, 300.0, 200.0, 10.0);
        assert!((viewport.zoom - 0.18).abs() < 1e-6);
        let center = viewport.to_local(view_box.center());
        assert!((f32::from(center.x) - 150.0).abs() < 1e-4);
        assert!((f32::from(center.y) - 100.0).abs() < 1e-4);
    }

    #[test]
    fn zooming_keeps_the_point_under_the_focus() {
        let viewport = SceneViewport::new(15.0, -8.0, 1.5);
        let focus = point(px(120.0), px(90.0));
        let before = viewport.to_scene(focus);
        let zoomed = viewport.zoom_at(focus, 3.0);
        let after = zoomed.to_scene(focus);
        assert!((before.x - after.x).abs() < 1e-6);
        assert!((before.y - after.y).abs() < 1e-6);
        assert_eq!(zoomed.zoom, 3.0);
    }

    #[test]
    fn panning_moves_scene_points_by_the_same_pixels() {
        let viewport = SceneViewport::new(0.0, 0.0, 2.0);
        let origin = ScenePoint::new(10.0, 20.0);
        let before = viewport.to_local(origin);
        let after = viewport.pan_by(7.0, -3.0).to_local(origin);
        assert_eq!(f32::from(after.x) - f32::from(before.x), 7.0);
        assert_eq!(f32::from(after.y) - f32::from(before.y), -3.0);
    }

    #[test]
    fn zoom_is_clamped() {
        assert_eq!(clamp_zoom(f32::NAN), 1.0);
        assert_eq!(clamp_zoom(0.0), MIN_ZOOM);
        assert_eq!(clamp_zoom(100.0), MAX_ZOOM);
    }
}
