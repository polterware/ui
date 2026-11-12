//! The calls part A makes into the direction, routing, and registration parts. Parts A through
//! C are ported: the direction and geometry passes live in [`super::direction`], and the routing
//! cleanup passes of part C live in [`super::direction::materialized_geometry`] and its
//! sibling modules. Part D is still a stub, naming the pass that fills it.

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

/// `direction/detourSimplification.ts`.
pub fn simplify_detoured_edges(layout: &mut LayoutData) {
    direction::detour_simplification::simplify_detoured_edges(&mut layout.edges, &layout.nodes);
}

/// `direction/siblingSharedFaceRouting.ts`.
pub fn straighten_collinear_sibling_detours(layout: &mut LayoutData) {
    direction::sibling_shared_face_routing::straighten_collinear_sibling_detours(
        &mut layout.edges,
        &layout.nodes,
    );
}

/// `direction/sharedTrackNudging.ts`.
pub fn nudge_shared_interior_subpaths(layout: &mut LayoutData) {
    direction::shared_track_nudging::nudge_shared_interior_subpaths(
        &mut layout.edges,
        &layout.nodes,
    );
}

/// `direction/materializedGeometry.ts`.
pub fn separate_shared_rendered_terminal_lanes(layout: &mut LayoutData) {
    direction::materialized_geometry::separate_shared_rendered_terminal_lanes(
        &mut layout.edges,
        &layout.nodes,
    );
}

/// `direction/materializedGeometry.ts`.
pub fn collapse_redundant_rectangular_doglegs(layout: &mut LayoutData) {
    direction::materialized_geometry::collapse_redundant_rectangular_doglegs(
        &mut layout.edges,
        &layout.nodes,
    );
}

/// `direction/materializedGeometry.ts`.
pub fn lift_obstacle_hugging_same_side_rails(layout: &mut LayoutData) {
    direction::materialized_geometry::lift_obstacle_hugging_same_side_rails(
        &mut layout.edges,
        &layout.nodes,
    );
}

/// `direction/materializedGeometry.ts`.
pub fn swap_destination_terminal_tails_to_reduce_crossings(layout: &mut LayoutData) {
    direction::materialized_geometry::swap_destination_terminal_tails_to_reduce_crossings(
        &mut layout.edges,
        &layout.nodes,
    );
}

/// `direction/materializedGeometry.ts`.
pub fn resolve_rendered_orthogonal_crossings(layout: &mut LayoutData) {
    direction::materialized_geometry::resolve_rendered_orthogonal_crossings(
        &mut layout.edges,
        &layout.nodes,
    );
}

/// `direction/materializedGeometry.ts`.
pub fn reassign_crossing_external_rail_channels(layout: &mut LayoutData) {
    direction::materialized_geometry::reassign_crossing_external_rail_channels(
        &mut layout.edges,
        &layout.nodes,
    );
}

/// `direction/materializedGeometry.ts`.
pub fn shortcut_redundant_orthogonal_jogs(layout: &mut LayoutData) {
    direction::materialized_geometry::shortcut_redundant_orthogonal_jogs(
        &mut layout.edges,
        &layout.nodes,
    );
}

/// `direction/materializedGeometry.ts`.
pub fn lift_top_lane_title_bands_above_rails(
    layout: &mut LayoutData,
    direction: Option<Direction>,
) {
    direction::materialized_geometry::lift_top_lane_title_bands_above_rails(
        &layout.edges,
        &mut layout.nodes,
        direction.map(Direction::as_str),
    );
}

/// `direction/materializedGeometry.ts`.
pub fn shift_left_lane_title_bands_left_of_rails(
    layout: &mut LayoutData,
    direction: Option<Direction>,
) {
    direction::materialized_geometry::shift_left_lane_title_bands_left_of_rails(
        &layout.edges,
        &mut layout.nodes,
        direction.map(Direction::as_str),
    );
}

/// `direction/alignedEdgeStraightening.ts`.
pub fn straighten_aligned_cross_lane_edges(layout: &mut LayoutData) -> bool {
    direction::aligned_edge_straightening::straighten_aligned_cross_lane_edges(
        &mut layout.edges,
        &layout.nodes,
    )
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
