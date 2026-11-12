//! The calls part A makes into the direction, routing, and registration parts. Part B's
//! direction and geometry passes are ported in [`super::direction`] and reached from here;
//! parts C and D are still stubs, each naming the pass that fills it. Until then a pass leaves
//! the layout unchanged, and the phases that need geometry report [`super::SwimlaneError`].

use crate::render::pipeline::types::{LayoutData, Node};
use crate::scene::Point;

use super::SwimlaneError;
use super::direction;
use super::helpers::Direction;

// ---- Part B: direction and geometry ----

/// `direction/lrTransform.ts`.
pub fn flow_axis_stretch(nodes: &[Node]) -> f64 {
    direction::lr_transform::flow_axis_stretch(nodes)
}

/// `direction/geometry.ts`.
pub fn orthogonalize_polyline(points: &[Point]) -> Vec<Point> {
    direction::geometry::orthogonalize_polyline(points)
}

/// `direction/geometry.ts`.
pub fn simplify_polyline(points: &[Point]) -> Vec<Point> {
    direction::geometry::simplify_polyline(points)
}

/// `direction/lrTransform.ts`.
pub fn apply_lr_direction_transform(layout: &mut LayoutData, direction: Direction) -> bool {
    direction::lr_transform::apply_lr_direction_transform(layout, direction)
}

/// `direction/lrTransform.ts`.
pub fn apply_bt_direction_transform(layout: &mut LayoutData) -> bool {
    direction::lr_transform::apply_bt_direction_transform(layout)
}

/// `direction/portSwap.ts`.
pub fn port_swap_to_l_shape(layout: &mut LayoutData) {
    direction::port_swap::port_swap_to_l_shape(layout)
}

/// `direction/labelAnchoring.ts`.
pub fn anchor_labels_to_polyline(layout: &mut LayoutData) {
    direction::label_anchoring::anchor_labels_to_polyline(layout)
}

/// `direction/endpointClip.ts`.
pub fn clip_edge_endpoints_to_node_boundaries(layout: &mut LayoutData) {
    direction::endpoint_clip::clip_edge_endpoints_to_node_boundaries(layout)
}

/// `direction/endpointClip.ts`.
pub fn prepare_edge_endpoints_for_renderer(layout: &mut LayoutData) {
    direction::endpoint_clip::prepare_edge_endpoints_for_renderer(layout)
}

/// `direction/terminalStub.ts`.
pub fn collapse_short_terminal_stub(layout: &mut LayoutData) {
    direction::terminal_stub::collapse_short_terminal_stub(layout)
}

/// `direction/terminalLegAlignment.ts`.
pub fn align_terminal_legs_to_ports(layout: &mut LayoutData) -> bool {
    direction::terminal_leg_alignment::align_terminal_legs_to_ports(layout)
}

/// `direction/validation.ts`.
pub fn validate_swimlanes_layout(layout: &LayoutData) -> Result<(), SwimlaneError> {
    direction::validation::validate_swimlanes_layout(layout)
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
