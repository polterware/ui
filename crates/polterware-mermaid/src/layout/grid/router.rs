//! Port of Mermaid 12.3.0's `rendering-util/layout-algorithms/grid/router.ts`.
//!
//! Part A wires the routing entry point so the layout core compiles and commits rendering
//! capabilities. Parts B and C port the routing phases behind it, and part D registers the
//! pipeline; until then [`route_grid_edges`] commits no routes and reports success. This stub is
//! deliberately outside public behavior: a part-A layout is a measured grid with edges left
//! unrouted.

use crate::layout::grid::router_errors::GridRoutingError;
use crate::layout::grid::router_instrumentation::GridRoutingInstrumentation;
use crate::layout::grid::router_topology::TopologyResourceCaps;
use crate::layout::grid::types::{GridLayoutResult, LayoutData};

#[derive(Clone, Debug, Default)]
pub struct GridRoutingOptions {
    // Caps are also test seams, so enabling them deliberately bypasses compatibility fast paths.
    pub topology_caps: Option<TopologyResourceCaps>,
}

/// Test options also carry an eventual dual-route comparison callback (part B). Part A needs only
/// the stable options shape.
#[derive(Clone, Debug, Default)]
pub struct GridRoutingTestOptions {
    pub options: GridRoutingOptions,
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
    for edge in &mut layout.edges {
        edge.curve = Some(result.config.curve);
        edge.corner_radius = Some(result.config.edge_corner_radius);
    }
    Ok(())
}
