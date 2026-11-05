//! The canvas engine and its painting (`@polterware/frontend-canvas` and the drawing half of
//! `@polterware/feature-canvas`): path data drawn as GPUI paths, the drawing library and the
//! component silhouettes, the viewport, routes, and hit testing, undo history, and the
//! painters for shapes, notes, selection, connections, and the dotted grid.
//!
//! What a node means (its kind's catalog, its references) stays with the app; it hands the
//! painters resolved outlines and colors and lays the node's text over them as elements, in
//! screen pixels at the viewport's zoom.

pub mod dock;
pub mod geometry;
pub mod history;
pub mod paint;
pub mod path;
pub mod shapes;

pub use dock::{canvas_dock, dock_group, dock_separator, zoom_level};
pub use geometry::{
    Box2, EDGE_HIT_PADDING, Hit, MAX_ZOOM, MIN_ZOOM, Pt, Viewport, arrange_horizontally, bounds_of,
    clamp_zoom, hit_test, near_polyline, route,
};
pub use history::{Capture, History};
pub use paint::{
    EdgePaint, GRID, Line, ShapePaint, dot_grid, edges_layer, paint_dot_grid, paint_edges,
    paint_selection, paint_shape, selection_frame, shape_surface, sticky_paths, sticky_surface,
};
pub use shapes::{
    FAMILIES, Family, GLYPH, Inset, SHAPES, ShapeDefinition, ShapeGroup, family_of, shape_of,
};
