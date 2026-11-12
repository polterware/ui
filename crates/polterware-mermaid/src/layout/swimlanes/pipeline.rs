//! Port of `pipeline.ts`: the Sugiyama phases in order.

use super::config::LAYERING;
use super::helpers::{Coordinates, Direction, Edge, Graph, Layering, OrderedLayers};
use super::lane_ordering::{AUTOMATIC_LANE_ORDERING_RESTARTS, optimize_top_lane_order};
use super::phase0_helpers::normalize_graph;
use super::phase1_cycles::remove_cycles_dfs;
use super::phase2_dummies::make_proper_layering;
use super::phase2_gravity::assign_layers_gravity;
use super::phase2_lane_aware_compact::assign_layers_lane_aware_compact;
use super::phase2_options::LayeringOptions;
use super::phase3_ordering::{OrderOptions, order_layers};
use super::phase4_coordinates::{CoordOptions, assign_coordinates};

#[derive(Clone, Debug, Default)]
pub struct LayoutOptions {
    /// Defaults to true for compact swimlanes.
    pub compact_single_input: Option<bool>,
    pub ignore_cross_lane_edges: Option<bool>,
    pub optimize_ranks_by_crossings: Option<bool>,
    pub automatic_lane_ordering: Option<bool>,
    pub layer_gap: Option<f64>,
    pub node_gap: Option<f64>,
    pub direction: Option<Direction>,
}

#[derive(Clone, Debug, Default)]
pub struct LayoutResult {
    pub acyclic: Graph,
    pub reversed: Vec<Edge>,
    pub layering: Layering,
    pub ordered: OrderedLayers,
    pub coordinates: Coordinates,
}

pub fn sugiyama_layout(g: &Graph, opts: Option<LayoutOptions>) -> LayoutResult {
    let opts = opts.unwrap_or_default();
    let ignore_cross_lane_edges = opts.ignore_cross_lane_edges.unwrap_or(true);
    let optimize_ranks_by_crossings = opts.optimize_ranks_by_crossings.unwrap_or(true);
    let g0 = normalize_graph(g);
    let lane_order = if opts.automatic_lane_ordering.unwrap_or(false) {
        Some(optimize_top_lane_order(
            &g0,
            Some(AUTOMATIC_LANE_ORDERING_RESTARTS),
        ))
    } else {
        None
    };

    // Phase 1: cycle removal.
    let cycle_result = remove_cycles_dfs(&g0);
    let g_acyclic = cycle_result.acyclic;

    // Phase 2: layering.
    let compact_single_input = opts
        .compact_single_input
        .unwrap_or(LAYERING.default_compact_single_input);
    let layering = if ignore_cross_lane_edges {
        assign_layers_lane_aware_compact(
            &g_acyclic,
            LayeringOptions {
                compact_single_input: Some(compact_single_input),
                ignore_cross_lane_edges: Some(true),
                direction: opts.direction,
                ..Default::default()
            },
        )
    } else {
        assign_layers_gravity(
            &g_acyclic,
            LayeringOptions {
                compact_single_input: Some(compact_single_input),
                ignore_cross_lane_edges: Some(false),
                optimize_ranks_by_crossings: Some(optimize_ranks_by_crossings),
                ..Default::default()
            },
        )
    };
    let (proper_layering, graph_with_dummies) = make_proper_layering(&layering, &g_acyclic);
    // Phase 3: ordering.
    let ordered = order_layers(
        &proper_layering,
        &graph_with_dummies,
        Some(OrderOptions {
            lane_order: lane_order.clone(),
        }),
    );

    // Phase 4: coordinates.
    let coordinates = assign_coordinates(
        &ordered,
        &graph_with_dummies,
        Some(CoordOptions {
            layer_gap: opts.layer_gap,
            node_gap: opts.node_gap,
            lane_gap: None,
            direction: opts.direction,
            lane_order,
        }),
    );

    LayoutResult {
        acyclic: g_acyclic,
        reversed: cycle_result.reversed,
        layering: proper_layering,
        ordered,
        coordinates,
    }
}
