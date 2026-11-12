//! Port of `layoutCore.ts`: the pure swimlane layout core shared by the renderer and the
//! diagram-diff layout tests.

use crate::render::pipeline::types::LayoutData;

use super::SwimlaneError;
use super::helpers::{Direction, WriteBackOptions, to_graph_view, write_back_to_layout_data};
use super::pipeline::{LayoutOptions, sugiyama_layout};
use super::post_processing::{post_process_swimlane_layout, validate_swimlanes_layout};
use super::stubs;

pub type SwimlaneDirection = Direction;

fn get_swimlane_direction(data: &LayoutData) -> SwimlaneDirection {
    Direction::parse(data.direction.as_deref())
}

/// Pure swimlane layout core shared by browser rendering and DDLT.
///
/// The browser measures DOM nodes before this runs; DDLT injects captured sizes before calling
/// the same function.
pub fn run_swimlane_layout_core(data: &mut LayoutData) -> Result<SwimlaneDirection, SwimlaneError> {
    let direction = get_swimlane_direction(data);
    let node_gap = data.config.flowchart.node_spacing;
    let layer_gap = data.config.flowchart.rank_spacing;
    let ignore_cross_lane_edges = data.config.swimlane.ignore_cross_lane_edges;
    let optimize_ranks_by_crossings = data.config.swimlane.optimize_ranks_by_crossings;
    let automatic_lane_ordering = data.config.swimlane.automatic_lane_ordering;

    let mut g = to_graph_view(data);
    let result = sugiyama_layout(
        &g,
        Some(LayoutOptions {
            node_gap: Some(node_gap),
            layer_gap: Some(layer_gap),
            ignore_cross_lane_edges: Some(ignore_cross_lane_edges),
            optimize_ranks_by_crossings: Some(optimize_ranks_by_crossings),
            automatic_lane_ordering: Some(automatic_lane_ordering),
            direction: Some(direction),
            ..Default::default()
        }),
    );
    write_back_to_layout_data(
        &mut g,
        &result.ordered,
        &result.coordinates,
        Some(WriteBackOptions {
            node_gap: Some(node_gap),
            layer_gap: Some(layer_gap),
        }),
    );
    *data = g.layout;

    // The layout phases position nodes only; they do not emit edge routing. Reset any edge
    // points carried on the input so the router below is the single source of truth.
    for edge in &mut data.edges {
        edge.points = None;
    }
    stubs::route_edges_orthogonal(data, direction)?;

    for edge in &mut data.edges {
        if edge.curve.as_deref().is_none_or(|curve| curve == "basis") {
            edge.curve = Some("rounded".to_string());
        }
    }

    post_process_swimlane_layout(data, Some(direction));

    validate_swimlanes_layout(data)?;

    Ok(direction)
}
