//! Port of Mermaid 12.3.0's
//! `rendering-util/layout-algorithms/grid/routerInstrumentation.ts`, reduced to what part A
//! needs.
//!
//! Part C completes this module: the checkpoint/restore helpers and `recordGridRoute` need the
//! polyline geometry of `layout-utils` and are left for that part. The struct shape is the full
//! upstream shape so later parts can add behavior without changing consumers.

/// Reason a routing resource cap sent a container or search through the fallback path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GridRoutingFallbackReason {
    VertexCap,
    AdjacencyCap,
    EstimatedMemoryCap,
    SearchStateCap,
    LabelIndexWorkCap,
    LabelPlacementExhausted,
}

impl GridRoutingFallbackReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::VertexCap => "vertex_cap",
            Self::AdjacencyCap => "adjacency_cap",
            Self::EstimatedMemoryCap => "estimated_memory_cap",
            Self::SearchStateCap => "search_state_cap",
            Self::LabelIndexWorkCap => "label_index_work_cap",
            Self::LabelPlacementExhausted => "label_placement_exhausted",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GridRouteInstrumentation {
    pub edge_id: String,
    pub route_order: usize,
    pub lane_offset: f64,
    pub route_length: f64,
    pub bend_count: usize,
    pub crossing_count: usize,
    pub shared_length: f64,
    pub boundary_transition_count: usize,
    pub route_signature: String,
}

/// Work counters are cumulative, while committed-output counters roll back with a failed bundle
/// attempt. The disposition of each counter is established by part C.
#[derive(Clone, Debug, PartialEq)]
pub struct GridRoutingInstrumentation {
    pub containers_built: usize,
    pub base_topology_builds: usize,
    pub base_vertices: usize,
    pub base_adjacency_entries: usize,
    pub build_sweep_events: usize,
    pub endpoint_overlay_builds: usize,
    pub endpoint_overlay_vertices: usize,
    pub hierarchy_portal_pairs: usize,
    pub hierarchy_portal_transition_length: f64,
    pub hierarchy_portal_alternative_attempts: usize,
    pub hierarchy_portal_alternative_selections: usize,
    pub hierarchy_boundary_transitions: usize,
    pub label_overlay_builds: usize,
    pub label_overlay_vertices: usize,
    pub label_spacing_edges_examined: usize,
    pub label_spacing_eligible_edges: usize,
    pub label_spacing_boundaries_expanded: usize,
    pub label_spacing_pixels_added: f64,
    pub label_aware_self_loop_candidates: usize,
    pub label_aware_self_loops_committed: usize,
    pub label_aware_self_loop_fallbacks: usize,
    pub searches: usize,
    pub expanded_states: usize,
    pub max_open_set: usize,
    pub routes_found: usize,
    pub routes_impossible: usize,
    pub resource_limit_fallbacks: usize,
    pub fallback_reasons: [usize; 6],
    pub fallback_validation_failures: usize,
    pub compatibility_segments: usize,
    pub compatibility_fast_path_attempts: usize,
    pub compatibility_fast_paths: usize,
    pub compatibility_fast_path_validation_failures: usize,
    pub compatibility_fast_path_non_minimal_routes: usize,
    pub compatibility_recoveries: usize,
    pub compatibility_validation_failures: usize,
    pub bundle_separation_relaxations: usize,
    pub route_length: f64,
    pub bend_count: usize,
    pub crossing_count: usize,
    pub shared_length: f64,
    pub estimated_bytes: f64,
    pub search_workspace_bytes: f64,
    pub route_order: Vec<String>,
    pub routes: Vec<GridRouteInstrumentation>,
}

impl Default for GridRoutingInstrumentation {
    fn default() -> Self {
        Self {
            containers_built: 0,
            base_topology_builds: 0,
            base_vertices: 0,
            base_adjacency_entries: 0,
            build_sweep_events: 0,
            endpoint_overlay_builds: 0,
            endpoint_overlay_vertices: 0,
            hierarchy_portal_pairs: 0,
            hierarchy_portal_transition_length: 0.0,
            hierarchy_portal_alternative_attempts: 0,
            hierarchy_portal_alternative_selections: 0,
            hierarchy_boundary_transitions: 0,
            label_overlay_builds: 0,
            label_overlay_vertices: 0,
            label_spacing_edges_examined: 0,
            label_spacing_eligible_edges: 0,
            label_spacing_boundaries_expanded: 0,
            label_spacing_pixels_added: 0.0,
            label_aware_self_loop_candidates: 0,
            label_aware_self_loops_committed: 0,
            label_aware_self_loop_fallbacks: 0,
            searches: 0,
            expanded_states: 0,
            max_open_set: 0,
            routes_found: 0,
            routes_impossible: 0,
            resource_limit_fallbacks: 0,
            fallback_reasons: [0; 6],
            fallback_validation_failures: 0,
            compatibility_segments: 0,
            compatibility_fast_path_attempts: 0,
            compatibility_fast_paths: 0,
            compatibility_fast_path_validation_failures: 0,
            compatibility_fast_path_non_minimal_routes: 0,
            compatibility_recoveries: 0,
            compatibility_validation_failures: 0,
            bundle_separation_relaxations: 0,
            route_length: 0.0,
            bend_count: 0,
            crossing_count: 0,
            shared_length: 0.0,
            estimated_bytes: 0.0,
            search_workspace_bytes: 0.0,
            route_order: Vec::new(),
            routes: Vec::new(),
        }
    }
}

impl GridRoutingInstrumentation {
    pub fn reason_count(&self, reason: GridRoutingFallbackReason) -> usize {
        self.fallback_reasons[reason as usize]
    }

    pub fn increment_reason(&mut self, reason: GridRoutingFallbackReason) {
        self.fallback_reasons[reason as usize] += 1;
    }
}

pub fn create_grid_routing_instrumentation() -> GridRoutingInstrumentation {
    GridRoutingInstrumentation::default()
}
