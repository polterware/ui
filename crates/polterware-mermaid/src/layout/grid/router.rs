//! Port of Mermaid 12.3.0's `rendering-util/layout-algorithms/grid/router.ts`.
//!
//! The routing phases live in their own modules: planning, constraints, and compatibility (part
//! B), sparse search and the routing session (part C), and edge-label placement (part D). This
//! facade keeps the stable options, exports, and entry point; [`route_grid_edges`] runs one
//! [`crate::layout::grid::router_session::GridEdgeRoutingSession`] and commits only on success.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::layout::grid::router_errors::GridRoutingError;
use crate::layout::grid::router_instrumentation::GridRoutingInstrumentation;
use crate::layout::grid::router_planning::{PreparedEdgeRoutes, prepare_edge_routes};
use crate::layout::grid::router_search::RouterSearchCaps;
use crate::layout::grid::router_session::GridEdgeRoutingSession;
use crate::layout::grid::router_topology::TopologyResourceCaps;
use crate::layout::grid::types::{Edge, GridLayoutResult, LayoutData};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GridRoutingRouteQuality {
    pub valid: bool,
    pub length: f64,
    pub bends: usize,
}

/// The sparse-versus-legacy measurements reported when dual-route comparison is enabled.
#[derive(Clone, Debug, PartialEq)]
pub struct GridRoutingDualRouteComparison {
    pub edge_id: String,
    pub sparse: GridRoutingRouteQuality,
    pub legacy: GridRoutingRouteQuality,
}

#[derive(Clone, Debug, Default)]
pub struct GridRoutingOptions {
    // Caps are also test seams, so enabling them deliberately bypasses compatibility fast paths.
    pub topology_caps: Option<TopologyResourceCaps>,
    pub search_caps: Option<RouterSearchCaps>,
}

/// Test options also carry the dual-route comparison sink that records what sparse search would
/// choose beside the legacy corridor route.
#[derive(Clone, Debug, Default)]
pub struct GridRoutingTestOptions {
    pub options: GridRoutingOptions,
    pub on_dual_route_comparison: Option<Rc<RefCell<Vec<GridRoutingDualRouteComparison>>>>,
}

impl GridRoutingTestOptions {
    /// The production corridor fast path only runs when no test seam is configured.
    pub fn compatibility_fast_path_enabled(&self) -> bool {
        self.options.topology_caps.is_none()
            && self.options.search_caps.is_none()
            && self.on_dual_route_comparison.is_none()
    }
}

/// Routing facade: a fresh session owns every mutable routing structure for one invocation and
/// commits routes in deterministic plan order. A failed invocation leaves the caller's edges
/// untouched; part D registers the pipeline and places labels on the committed routes.
pub fn route_grid_edges(
    layout: &mut LayoutData,
    result: &GridLayoutResult,
    metrics: Option<&mut GridRoutingInstrumentation>,
    options: &GridRoutingTestOptions,
) -> Result<(), GridRoutingError> {
    let metrics_snapshot = metrics.as_ref().map(|metrics| (*metrics).clone());
    let mut session =
        GridEdgeRoutingSession::new(layout.clone(), result, metrics_snapshot, options);
    session.route()?;
    let (edges, session_metrics) = session.into_parts();
    if let (Some(target), Some(local)) = (metrics, session_metrics) {
        *target = local;
    }
    let by_id: HashMap<&str, &Edge> = edges.iter().map(|edge| (edge.id.as_str(), edge)).collect();
    for edge in &mut layout.edges {
        let Some(routed) = by_id.get(edge.id.as_str()) else {
            continue;
        };
        edge.points = routed.points.clone();
        edge.curve = routed.curve;
        edge.corner_radius = routed.corner_radius;
    }
    Ok(())
}

/// Re-exports part B publishes for the routing session and tests.
pub use crate::layout::grid::router_compatibility::{
    are_exactly_axis_aligned, bounded_alternative_portal_coordinates,
};
pub use crate::layout::grid::router_constraint::validate_same_container_route;

/// Planning entry point consumed by part C's routing session.
pub fn plan_grid_routes(
    layout: &LayoutData,
    result: &GridLayoutResult,
) -> Result<PreparedEdgeRoutes, GridRoutingError> {
    prepare_edge_routes(layout, result)
        .map_err(|error| GridRoutingError::geometry(error.to_string()))
}
