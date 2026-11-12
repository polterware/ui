//! Port of Mermaid 12.3.0's grid layout engine,
//! `rendering-util/layout-algorithms/grid`.
//!
//! Part A ports the geometry core (`types`, `groups`, `placement`, `labelGeometry`,
//! `labelSpacing`, `layoutCore`, `routerTopology`) and wires the routing and label entry points.
//! Parts B through D fill the routing phases (`routerCompatibility`, `routerPlanning`,
//! `routerConstraint`, `routerSparse`, `routerSearch`, `routerSession`, `routerInstrumentation`)
//! and the edge-label transaction (`edgeLabels`).
//!
//! The sources are Mermaid 12.3.0 (MIT, Copyright (c) 2014 - 2022 Knut Sveidqvist), checked out at
//! `research/mermaid-port/upstream/mermaid`.

pub mod edge_labels;
pub mod groups;
pub mod label_geometry;
pub mod label_spacing;
pub mod layout_core;
pub mod placement;
pub mod router;
pub mod router_errors;
pub mod router_instrumentation;
pub mod router_topology;
pub mod types;

pub use edge_labels::{
    GridEdgeLabelInstrumentation, GridEdgeLabelOptions, position_grid_edge_labels,
};
pub use groups::{build_grid_forest, is_ancestor_group};
pub use label_geometry::{
    GRID_EDGE_END_MARKER_CLEARANCE, GRID_LABEL_CLEARANCE, GRID_LABEL_SPAN_MARGIN,
    GridLabelSpanRequirement, grid_label_required_segment_length, grid_label_span_requirement,
};
pub use label_spacing::{
    GridContainerLabelGaps, GridLabelRequirements, GridLabelledEdge, build_grid_label_requirements,
    derive_grid_container_label_gaps,
};
pub use layout_core::{GridLayoutCoreInternalOptions, run_grid_layout_core};
pub use placement::{
    GridPlacementResolution, build_grid_source_order, build_grid_source_order_indices,
    read_grid_config, resolve_grid_placements, validate_grid_placement_map,
};
pub use router::{GridRoutingOptions, GridRoutingTestOptions, route_grid_edges};
pub use router_errors::{GridRoutingError, GridRoutingResourceLimitError, SearchStateScope};
pub use router_instrumentation::{
    GridRouteInstrumentation, GridRoutingFallbackReason, GridRoutingInstrumentation,
    create_grid_routing_instrumentation,
};
pub use router_topology::{
    BuildTopologyOptions, ContainerTopologyInput, DEFAULT_MAX_ROUTING_ESTIMATED_BYTES,
    EDGE_CLEARANCE_PX, EndpointOverlayScratch, ROUTE_CLEARANCE_PX, RouterObstacleInput,
    TopologyResourceCaps, build_container_routing_topology, build_endpoint_routing_overlay,
    build_paired_portal, derive_portal_ranges, routing_point_key,
};
pub use types::*;
