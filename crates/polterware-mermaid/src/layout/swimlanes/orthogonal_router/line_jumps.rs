//! Port of the layout side of upstream mermaid 12.3.0
//! `rendering-util/rendering-elements/lineJump.ts`, as `adjustLayout.ts` runs it for the
//! swimlanes layout: every routed polyline crossing becomes an arc (`jumpStyle: 'arc'`) or a
//! visible break (`jumpStyle: 'gap'`) on the line that hops.
//!
//! Upstream reads the geometry back from each rendered path's `data-points` and rewrites the
//! SVG `d` in place, because the router leaves the last word on geometry to that paint pass.
//! This crate draws from layout data, so the same figures are computed here from the routed
//! edges and handed to the renderer, which rewrites the path it paints; the arc geometry and
//! the SVG command formatting stay in `render::edges` (the port of `lineJump.ts`). The hop
//! radius is the `6` `adjustLayout.ts` passes.

use crate::layout::swimlanes::SwimlaneError;
use crate::render::edges::{
    EdgeGeom, JumpStyle, LineJumpConfig, curve_supports_line_hops, find_edge_intersections,
    process_edges_with_jumps,
};
use crate::render::pipeline::types::LayoutData;
use crate::scene::Path;

/// `jumpRadius: 6` in `adjustLayout.ts`.
pub const JUMP_RADIUS: f64 = 6.0;

fn jump_config(style: &str) -> LineJumpConfig {
    LineJumpConfig {
        enabled: true,
        jump_radius: JUMP_RADIUS,
        jump_style: if style == "gap" {
            JumpStyle::Gap
        } else {
            JumpStyle::Arc
        },
    }
}

/// The rewritten path of every edge that carries a hop, keyed by edge id. Upstream only
/// rewrites the paths that carry a hop, so edges without a crossing are absent from the result
/// and keep the path their polyline already describes.
pub fn line_jump_paths(layout: &LayoutData, style: &str) -> Vec<(String, Path)> {
    let geometries: Vec<EdgeGeom> = layout
        .edges
        .iter()
        .filter(|edge| edge.points.as_ref().is_some_and(|points| points.len() >= 2))
        .filter(|edge| curve_supports_line_hops(edge.curve.as_deref()))
        .map(|edge| EdgeGeom {
            id: edge.id.clone(),
            points: edge.points.clone().unwrap_or_default(),
            curve: edge.curve.clone(),
            corner_radius: edge.corner_radius,
            arrow_type_start: edge.arrow_type_start.clone(),
            arrow_type_end: edge.arrow_type_end.clone(),
        })
        .collect();

    let jumping: std::collections::HashSet<String> = find_edge_intersections(&geometries)
        .into_iter()
        .map(|crossing| crossing.jump_edge_id)
        .collect();

    process_edges_with_jumps(&geometries, jump_config(style))
        .into_iter()
        .filter(|(id, _)| jumping.contains(id))
        .collect()
}

/// `applyLineJumpsToSvg`, for a scene this crate paints itself: returns the rewritten path of
/// every edge that carries a hop, keyed by edge id. The paint step uses a returned path in
/// place of the plain route; an edge without a crossing is absent and keeps its own path,
/// exactly as upstream only rewrites the paths that carry a hop.
pub fn apply_line_jumps_to_svg(
    layout: &LayoutData,
    style: &str,
) -> Result<Vec<(String, Path)>, SwimlaneError> {
    Ok(line_jump_paths(layout, style))
}
