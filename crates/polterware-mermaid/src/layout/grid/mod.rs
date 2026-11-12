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
pub mod geometry;
pub mod groups;
pub mod label_geometry;
pub mod label_spacing;
pub mod layout_core;
pub mod placement;
pub mod router;
pub mod router_compatibility;
pub mod router_constraint;
pub mod router_errors;
pub mod router_instrumentation;
pub mod router_occupancy;
pub mod router_planning;
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
pub use router::{GridRoutingOptions, GridRoutingTestOptions, plan_grid_routes, route_grid_edges};
pub use router_compatibility::{
    CompatibilitySegmentAttachments, PreparedRoutingModes, SELF_LOOP_PORT_GAP,
    SELF_LOOP_PORT_OFFSET_STEP, SegmentAttachment, SegmentAttachmentAlternative, SelfLoopRoute,
    are_exactly_axis_aligned, boundary_attachment, bounded_alternative_portal_coordinates,
    build_routing_context, combine_point_chains, compatibility_hierarchy_attachments,
    compatibility_lca_attachments, group_boundary_endpoint_attachment, item_attachment,
    label_aware_self_loop_candidates, ordered_self_loop_sides, portal_attachment,
    prepare_routing_modes, reverse_points, root_container_meta, route_obstacle_clear_self_loop,
    route_within_container, validated_compatibility_segment,
};
pub use router_constraint::{
    OrthogonalSegment, endpoint_pair_lower_bound, is_pair_separation_error, nonterminal_segments,
    pair_arc_allowed, pair_segments_conflict, route_has_distinct_pair_ports,
    route_satisfies_pair_constraints, router_rect, validate_container_segment,
    validate_same_container_route,
};
pub use router_errors::{GridRoutingError, GridRoutingResourceLimitError, SearchStateScope};
pub use router_instrumentation::{
    GridRouteInstrumentation, GridRoutingFallbackReason, GridRoutingInstrumentation,
    create_grid_routing_instrumentation,
};
pub use router_occupancy::RouteOccupancyIndex;
pub use router_planning::{
    EdgeEndpointEntry, EdgeEndpointPlan, EdgeRouteCandidates, EdgeRoutePlan, EndpointCandidate,
    EndpointFinalKind, EndpointRole, EndpointSlotAssignment, LANE_SEPARATION_PX,
    PreparedEdgeRoutes, TERMINAL_APPROACH_PX, assign_compact_portal_coordinates, endpoint_pair_key,
    owner_group_title, owner_side_key, prepare_edge_routes, side_interval, side_order,
};
pub use router_topology::{
    BuildTopologyOptions, ContainerTopologyInput, DEFAULT_MAX_ROUTING_ESTIMATED_BYTES,
    EDGE_CLEARANCE_PX, EndpointOverlayScratch, ROUTE_CLEARANCE_PX, RouterObstacleInput,
    TopologyResourceCaps, build_container_routing_topology, build_endpoint_routing_overlay,
    build_paired_portal, derive_portal_ranges, routing_point_key,
};
pub use types::*;
