//! Port of `postProcessing.ts`: applies the direction transforms and the routing cleanup passes
//! in order. The passes themselves are the ported modules [`super::direction`] exposes; the
//! call site goes through [`super::stubs`], so this module pins the order they run in.

use crate::render::pipeline::types::LayoutData;

use super::helpers::{Direction, node_is_group};
use super::stubs;
pub use super::stubs::validate_swimlanes_layout;

/// Straightens edges between aligned nodes in different lanes, moves terminal legs onto their
/// ports, then re-anchors labels and endpoints of the routes that moved.
fn align_terminal_legs_and_refresh(layout: &mut LayoutData) {
    let straightened = stubs::straighten_aligned_cross_lane_edges(layout);
    if stubs::align_terminal_legs_to_ports(layout) || straightened {
        stubs::anchor_labels_to_polyline(layout);
        stubs::prepare_edge_endpoints_for_renderer(layout);
    }
}

fn finalize_rendered_edges(layout: &mut LayoutData) {
    stubs::resolve_rendered_orthogonal_crossings(layout);
    // Crossing cleanup can bring a route onto a face another edge already leaves or enters
    // from, so split shared terminal lanes again before endpoints are pinned.
    stubs::separate_shared_rendered_terminal_lanes(layout);
    stubs::reassign_crossing_external_rail_channels(layout);
    stubs::shortcut_redundant_orthogonal_jogs(layout);
    stubs::anchor_labels_to_polyline(layout);
    stubs::prepare_edge_endpoints_for_renderer(layout);
    stubs::lift_obstacle_hugging_same_side_rails(layout);
    stubs::anchor_labels_to_polyline(layout);
    stubs::prepare_edge_endpoints_for_renderer(layout);
}

/// Applies direction transforms and post-routing cleanup to a swimlane layout.
pub fn post_process_swimlane_layout(layout: &mut LayoutData, direction: Option<Direction>) {
    let content_nodes = layout
        .nodes
        .iter()
        .filter(|node| !node_is_group(node))
        .count();

    // TB is the canonical orientation. LR/RL rotate rank progression onto X; BT mirrors the
    // canonical Y progression. Cleanup below operates in whichever system this step leaves.
    if let Some(direction) = direction.filter(|direction| direction.is_horizontal())
        && content_nodes > 0
        && !stubs::apply_lr_direction_transform(layout, direction)
    {
        return;
    }

    if direction == Some(Direction::Bt)
        && content_nodes > 0
        && !stubs::apply_bt_direction_transform(layout)
    {
        return;
    }

    for edge in &mut layout.edges {
        if edge.is_layout_only.unwrap_or(false) {
            continue;
        }
        let Some(points) = edge.points.as_mut() else {
            continue;
        };
        if points.len() < 2 {
            continue;
        }
        let orthogonal = stubs::orthogonalize_polyline(points);
        edge.points = Some(stubs::simplify_polyline(&orthogonal));
    }

    stubs::simplify_detoured_edges(layout);

    // Prefer shorter sibling routes when a local port shift keeps them clear.
    stubs::straighten_collinear_sibling_detours(layout);

    // Swap a source port only when it produces a clear L-shape with fewer bends.
    stubs::port_swap_to_l_shape(layout);

    // Initial label anchoring against the routed polylines. The cleanup passes below can
    // still reshape these polylines, so labels are re-anchored once more at the end.
    stubs::anchor_labels_to_polyline(layout);

    stubs::clip_edge_endpoints_to_node_boundaries(layout);

    // Retarget short terminal stubs that are hidden by endpoint clipping.
    stubs::collapse_short_terminal_stub(layout);

    // Wybrow-style shared-track nudge: coincident middle rails must be separated into nearby
    // parallel tracks, with endpoint stubs pinned.
    stubs::nudge_shared_interior_subpaths(layout);

    // Materialized-render terminal lane split: split coincident first/last stubs on a shared
    // node face before the endpoint-duplication handoff pins them.
    stubs::separate_shared_rendered_terminal_lanes(layout);

    // Once terminal lanes have been separated, some earlier same-track detours become
    // unnecessary; remove only provably redundant rectangular doglegs.
    stubs::collapse_redundant_rectangular_doglegs(layout);

    // Same-side rails can be routed just inside a taller intervening node's border; lift them
    // outside the blocker before crossing cleanup.
    stubs::lift_obstacle_hugging_same_side_rails(layout);

    // Some shared-destination crossings only improve when two terminal ports exchange places
    // together; try that bounded transaction before whole-edge reroutes.
    stubs::swap_destination_terminal_tails_to_reduce_crossings(layout);

    finalize_rendered_edges(layout);

    // Run the shared-track pass once more on the visible shape, then refresh labels and
    // endpoint handoff. `prepare_edge_endpoints_for_renderer` is idempotent.
    stubs::nudge_shared_interior_subpaths(layout);

    // Endpoint preparation can reveal a crossing that was not in the pre-materialized route.
    finalize_rendered_edges(layout);

    // Straighten a route, or move a terminal leg onto its port, where that is clear.
    align_terminal_legs_and_refresh(layout);

    // Swimlane title bands are visual headers, not routing obstacles. Once edge geometry is
    // final, move the aligned title bands out of any clear rail that crosses them.
    stubs::lift_top_lane_title_bands_above_rails(layout, direction);
    stubs::shift_left_lane_title_bands_left_of_rails(layout, direction);
    // Moving one band can expose a second title/rail interaction; the passes are idempotent,
    // so one bounded repeat keeps headers out of late routes.
    stubs::lift_top_lane_title_bands_above_rails(layout, direction);
    stubs::shift_left_lane_title_bands_left_of_rails(layout, direction);
}
