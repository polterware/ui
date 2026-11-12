//! Port of Mermaid 12.3.0's `rendering-util/layout-algorithms/grid/router.ts`.
//!
//! Part A wires the routing entry point so the layout core compiles and commits rendering
//! capabilities. Part B ports the planning, constraint, and compatibility phases and exposes them
//! for the routing session; part C ports `routerSparse`, `routerSearch`, and `routerSession`, which
//! owns the commit loop behind [`route_grid_edges`]. Until then [`route_grid_edges`] commits no
//! routes and reports success. This stub is deliberately outside public behavior: a part-A/B
//! layout is a measured grid with edges left unrouted.

use crate::layout::grid::router_errors::GridRoutingError;
use crate::layout::grid::router_instrumentation::GridRoutingInstrumentation;
use crate::layout::grid::router_planning::{PreparedEdgeRoutes, prepare_edge_routes};
use crate::layout::grid::router_topology::TopologyResourceCaps;
use crate::layout::grid::types::{GridLayoutResult, LayoutData};

#[derive(Clone, Debug, Default)]
pub struct GridRoutingOptions {
    // Caps are also test seams, so enabling them deliberately bypasses compatibility fast paths.
    pub topology_caps: Option<TopologyResourceCaps>,
    /// Part C adds `RouterSearchCaps`; a configured search cap also disables the compatibility fast
    /// path so tests exercise sparse search. The flag stands in until the cap type exists.
    pub search_caps_configured: bool,
}

/// Test options also carry an eventual dual-route comparison callback (part C). Part B only needs
/// the flag that disables the fast path while that comparison runs.
#[derive(Clone, Debug, Default)]
pub struct GridRoutingTestOptions {
    pub options: GridRoutingOptions,
    pub dual_route_comparison: bool,
}

impl GridRoutingTestOptions {
    /// The production corridor fast path only runs when no test seam is configured.
    pub fn compatibility_fast_path_enabled(&self) -> bool {
        self.options.topology_caps.is_none()
            && !self.options.search_caps_configured
            && !self.dual_route_comparison
    }
}

/// Routing facade. Parts B and C port the routing phases; part D registers the pipeline. Until
/// then this publishes the configured edge style (the part of routing the layout core commits)
/// and computes no route points.
pub fn route_grid_edges(
    layout: &mut LayoutData,
    result: &GridLayoutResult,
    _metrics: Option<&mut GridRoutingInstrumentation>,
    _options: &GridRoutingTestOptions,
) -> Result<(), GridRoutingError> {
    // Part C's `GridEdgeRoutingSession` calls `root_container_meta`, `prepare_edge_routes`,
    // `prepare_routing_modes`, and `build_routing_context`, then routes bundles in plan order. This
    // stub keeps the style commit so a part-A/B layout still renders measured edges.
    for edge in &mut layout.edges {
        edge.curve = Some(result.config.curve);
        edge.corner_radius = Some(result.config.edge_corner_radius);
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
