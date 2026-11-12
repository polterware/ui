//! The GPUI painter for Mermaid scenes: `polterware-mermaid` lays a diagram out into a
//! [`Scene`](polterware_mermaid::scene::Scene), and this module draws it with GPUI 0.2.2.
//! Build a [`MermaidView`] with `cx.new(|_| MermaidView::new(scene))` and put it in the tree.
//!
//! What the painter covers: groups with translate, scale, rotate, and skew transforms and
//! rectangular clips; paths with fill (nonzero and even-odd) and stroke, `stroke-dasharray`
//! through GPUI's `dash_array`, line joins and markers placed on the path's start and end
//! tangent; rectangles as corner-rounded quads when axis-aligned and as paths otherwise;
//! ellipses as paths; text shaped and painted by the window's text system in the theme's fonts;
//! and images through a resolver the app supplies. The view's background, a label with no
//! color, and `currentColor` fall back to `polterware-theme` tokens.
//!
//! What GPUI 0.2.2 cannot do, and how the painter takes the closest look: a stroke has no
//! selectable line cap (butt, GPUI's default), text cannot rotate (a rotated run is placed along
//! its angle with upright glyphs; only the swimlane title rotates upstream), and an image has no
//! per-paint opacity (the resolver's bytes are painted as they are). Content masks clip to
//! rectangles, which is what a scene's group clip already is.
//!
//! The view itself adds pan (drag on empty space, scroll), zoom (secondary-modifier scroll, the
//! pinch form GPUI 0.2.2 exposes), fit to the view box, and hit testing that returns the
//! [`SourceRef`](polterware_mermaid::scene::SourceRef) of the topmost node or edge.

mod color;
mod geometry;
mod hit;
mod measure;
mod paint;
mod view;
mod viewport;

pub use hit::{SceneHit, hit_test};
pub use measure::SceneMeasure;
pub use paint::{ImageResolver, paint_scene, paint_selection};
pub use view::{MermaidEvent, MermaidView};
pub use viewport::{MAX_ZOOM, MIN_ZOOM, SceneViewport, clamp_zoom};
