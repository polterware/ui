//! The calls part A makes into the direction, routing, and registration parts. Each function
//! names the part that fills it, as the port's split asks. Until then a pass leaves the layout
//! unchanged, and the phases that need geometry report [`super::SwimlaneError`].

use crate::render::pipeline::types::{LayoutData, Node};
use crate::scene::Point;

use super::SwimlaneError;
use super::helpers::Direction;

// ---- Part B: direction and geometry ----

/// `direction/lrTransform.ts`. Part B fills it; until then the flow axis keeps its unit stretch.
pub fn flow_axis_stretch(_nodes: &[Node]) -> f64 {
    1.0
}

/// `direction/geometry.ts`. Part B fills it; until then the polyline is returned unchanged.
pub fn orthogonalize_polyline(points: &[Point]) -> Vec<Point> {
    points.to_vec()
}

/// `direction/geometry.ts`. Part B fills it; until then the polyline is returned unchanged.
pub fn simplify_polyline(points: &[Point]) -> Vec<Point> {
    points.to_vec()
}

/// `direction/lrTransform.ts`. Part B fills it; until then the layout is unchanged.
pub fn apply_lr_direction_transform(_layout: &mut LayoutData, _direction: Direction) -> bool {
    false
}

/// `direction/lrTransform.ts`. Part B fills it; until then the layout is unchanged.
pub fn apply_bt_direction_transform(_layout: &mut LayoutData) -> bool {
    false
}

/// `direction/portSwap.ts`. Part B fills it; until then the edges are unchanged.
pub fn port_swap_to_l_shape(_layout: &mut LayoutData) {}

/// `direction/labelAnchoring.ts`. Part B fills it; until then the labels are unchanged.
pub fn anchor_labels_to_polyline(_layout: &mut LayoutData) {}

/// `direction/endpointClip.ts`. Part B fills it; until then the endpoints are unchanged.
pub fn clip_edge_endpoints_to_node_boundaries(_layout: &mut LayoutData) {}

/// `direction/endpointClip.ts`. Part B fills it; until then the points are unchanged.
pub fn prepare_edge_endpoints_for_renderer(_layout: &mut LayoutData) {}

/// `direction/terminalStub.ts`. Part B fills it; until then the edges are unchanged.
pub fn collapse_short_terminal_stub(_layout: &mut LayoutData) {}

/// `direction/terminalLegAlignment.ts`. Part B fills it; until then no leg moves.
pub fn align_terminal_legs_to_ports(_layout: &mut LayoutData) -> bool {
    false
}

/// `direction/validation.ts`. Part B fills it; until then the layout is accepted.
pub fn validate_swimlanes_layout(_layout: &LayoutData) -> Result<(), SwimlaneError> {
    Ok(())
}

// ---- Part C: materialized geometry and its routing helpers ----

/// `direction/detourSimplification.ts`. Part C fills it; until then the edges are unchanged.
pub fn simplify_detoured_edges(_layout: &mut LayoutData) {}

/// `direction/siblingSharedFaceRouting.ts`. Part C fills it; until then the edges are unchanged.
pub fn straighten_collinear_sibling_detours(_layout: &mut LayoutData) {}

/// `direction/sharedTrackNudging.ts`. Part C fills it; until then the edges are unchanged.
pub fn nudge_shared_interior_subpaths(_layout: &mut LayoutData) {}

/// `direction/materializedGeometry.ts`. Part C fills it; until then the terminals are unchanged.
pub fn separate_shared_rendered_terminal_lanes(_layout: &mut LayoutData) {}

/// `direction/materializedGeometry.ts`. Part C fills it; until then the edges are unchanged.
pub fn collapse_redundant_rectangular_doglegs(_layout: &mut LayoutData) {}

/// `direction/materializedGeometry.ts`. Part C fills it; until then the rails stay put.
pub fn lift_obstacle_hugging_same_side_rails(_layout: &mut LayoutData) {}

/// `direction/materializedGeometry.ts`. Part C fills it; until then the terminals stay put.
pub fn swap_destination_terminal_tails_to_reduce_crossings(_layout: &mut LayoutData) {}

/// `direction/materializedGeometry.ts`. Part C fills it; until then the crossings stay.
pub fn resolve_rendered_orthogonal_crossings(_layout: &mut LayoutData) {}

/// `direction/materializedGeometry.ts`. Part C fills it; until then the rails stay put.
pub fn reassign_crossing_external_rail_channels(_layout: &mut LayoutData) {}

/// `direction/materializedGeometry.ts`. Part C fills it; until then the jogs stay.
pub fn shortcut_redundant_orthogonal_jogs(_layout: &mut LayoutData) {}

/// `direction/materializedGeometry.ts`. Part C fills it; until then the title bands stay put.
pub fn lift_top_lane_title_bands_above_rails(
    _layout: &mut LayoutData,
    _direction: Option<Direction>,
) {
}

/// `direction/materializedGeometry.ts`. Part C fills it; until then the title bands stay put.
pub fn shift_left_lane_title_bands_left_of_rails(
    _layout: &mut LayoutData,
    _direction: Option<Direction>,
) {
}

/// `direction/alignedEdgeStraightening.ts`. Part C fills it; until then no edge is straightened.
pub fn straighten_aligned_cross_lane_edges(_layout: &mut LayoutData) -> bool {
    false
}

// ---- Part D: the orthogonal router and the diagram arm ----

/// `orthogonalRouter/router.ts`. Part D fills it; it is the single source of swimlane edge
/// geometry, so until then the layout cannot finish.
pub fn route_edges_orthogonal(
    _layout: &mut LayoutData,
    _direction: Direction,
) -> Result<(), SwimlaneError> {
    Err(SwimlaneError::unimplemented("D", "routeEdgesOrthogonal"))
}

/// `rendering-elements/lineJump.ts`, run from `adjustLayout.ts`. Part D fills it together with
/// the paint context the line hops draw into.
pub fn apply_line_jumps_to_svg(_layout: &LayoutData, _style: &str) -> Result<(), SwimlaneError> {
    Err(SwimlaneError::unimplemented("D", "applyLineJumpsToSvg"))
}
