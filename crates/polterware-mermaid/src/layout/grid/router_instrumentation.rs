//! Port of Mermaid 12.3.0's
//! `rendering-util/layout-algorithms/grid/routerInstrumentation.ts`.
//!
//! Instrumentation doubles as observable behavior for retries: work counters are cumulative, while
//! committed-output counters roll back with a failed bundle attempt.
//! [`create_grid_routing_instrumentation_checkpoint`] and
//! [`restore_grid_routing_instrumentation_checkpoint`] implement that transaction boundary, and
//! [`record_grid_route`] appends one committed route's quality measurements.

use crate::layout::grid::geometry::{SegmentOrientation, normalize_polyline, segments_cross};
use crate::layout::grid::types::{RouterPoint, js_number};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridRoutingMetricDisposition {
    Cumulative,
    Transactional,
}

/// The disposition of every metric: cumulative work counters survive a failed bundle attempt, and
/// transactional committed-output counters roll back with it.
pub const GRID_ROUTING_METRIC_DISPOSITION: &[(&str, GridRoutingMetricDisposition)] = &[
    ("containersBuilt", GridRoutingMetricDisposition::Cumulative),
    (
        "baseTopologyBuilds",
        GridRoutingMetricDisposition::Cumulative,
    ),
    ("baseVertices", GridRoutingMetricDisposition::Cumulative),
    (
        "baseAdjacencyEntries",
        GridRoutingMetricDisposition::Cumulative,
    ),
    ("buildSweepEvents", GridRoutingMetricDisposition::Cumulative),
    (
        "endpointOverlayBuilds",
        GridRoutingMetricDisposition::Cumulative,
    ),
    (
        "endpointOverlayVertices",
        GridRoutingMetricDisposition::Cumulative,
    ),
    (
        "hierarchyPortalPairs",
        GridRoutingMetricDisposition::Transactional,
    ),
    (
        "hierarchyPortalTransitionLength",
        GridRoutingMetricDisposition::Transactional,
    ),
    (
        "hierarchyPortalAlternativeAttempts",
        GridRoutingMetricDisposition::Cumulative,
    ),
    (
        "hierarchyPortalAlternativeSelections",
        GridRoutingMetricDisposition::Transactional,
    ),
    (
        "hierarchyBoundaryTransitions",
        GridRoutingMetricDisposition::Transactional,
    ),
    (
        "labelOverlayBuilds",
        GridRoutingMetricDisposition::Cumulative,
    ),
    (
        "labelOverlayVertices",
        GridRoutingMetricDisposition::Cumulative,
    ),
    (
        "labelSpacingEdgesExamined",
        GridRoutingMetricDisposition::Cumulative,
    ),
    (
        "labelSpacingEligibleEdges",
        GridRoutingMetricDisposition::Cumulative,
    ),
    (
        "labelSpacingBoundariesExpanded",
        GridRoutingMetricDisposition::Cumulative,
    ),
    (
        "labelSpacingPixelsAdded",
        GridRoutingMetricDisposition::Cumulative,
    ),
    (
        "labelAwareSelfLoopCandidates",
        GridRoutingMetricDisposition::Cumulative,
    ),
    (
        "labelAwareSelfLoopsCommitted",
        GridRoutingMetricDisposition::Transactional,
    ),
    (
        "labelAwareSelfLoopFallbacks",
        GridRoutingMetricDisposition::Cumulative,
    ),
    ("searches", GridRoutingMetricDisposition::Cumulative),
    ("expandedStates", GridRoutingMetricDisposition::Cumulative),
    ("maxOpenSet", GridRoutingMetricDisposition::Cumulative),
    ("routesFound", GridRoutingMetricDisposition::Transactional),
    (
        "routesImpossible",
        GridRoutingMetricDisposition::Transactional,
    ),
    (
        "resourceLimitFallbacks",
        GridRoutingMetricDisposition::Cumulative,
    ),
    ("fallbackReasons", GridRoutingMetricDisposition::Cumulative),
    (
        "fallbackValidationFailures",
        GridRoutingMetricDisposition::Cumulative,
    ),
    (
        "compatibilitySegments",
        GridRoutingMetricDisposition::Transactional,
    ),
    (
        "compatibilityFastPathAttempts",
        GridRoutingMetricDisposition::Cumulative,
    ),
    (
        "compatibilityFastPaths",
        GridRoutingMetricDisposition::Cumulative,
    ),
    (
        "compatibilityFastPathValidationFailures",
        GridRoutingMetricDisposition::Cumulative,
    ),
    (
        "compatibilityFastPathNonMinimalRoutes",
        GridRoutingMetricDisposition::Cumulative,
    ),
    (
        "compatibilityRecoveries",
        GridRoutingMetricDisposition::Transactional,
    ),
    (
        "compatibilityValidationFailures",
        GridRoutingMetricDisposition::Cumulative,
    ),
    (
        "bundleSeparationRelaxations",
        GridRoutingMetricDisposition::Transactional,
    ),
    ("routeLength", GridRoutingMetricDisposition::Transactional),
    ("bendCount", GridRoutingMetricDisposition::Transactional),
    ("crossingCount", GridRoutingMetricDisposition::Transactional),
    ("sharedLength", GridRoutingMetricDisposition::Transactional),
    ("estimatedBytes", GridRoutingMetricDisposition::Cumulative),
    (
        "searchWorkspaceBytes",
        GridRoutingMetricDisposition::Cumulative,
    ),
    ("routeOrder", GridRoutingMetricDisposition::Transactional),
    ("routes", GridRoutingMetricDisposition::Transactional),
];

/// Transactional scalar counters captured before a bundle attempt. The route arrays are
/// append-only during an attempt, so their lengths are enough and avoid cloning potentially large
/// per-edge records on every retry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridRoutingInstrumentationCheckpointValues {
    pub hierarchy_portal_pairs: usize,
    pub hierarchy_portal_transition_length: f64,
    pub hierarchy_portal_alternative_selections: usize,
    pub hierarchy_boundary_transitions: usize,
    pub label_aware_self_loops_committed: usize,
    pub routes_found: usize,
    pub routes_impossible: usize,
    pub compatibility_segments: usize,
    pub compatibility_recoveries: usize,
    pub bundle_separation_relaxations: usize,
    pub route_length: f64,
    pub bend_count: usize,
    pub crossing_count: usize,
    pub shared_length: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridRoutingInstrumentationCheckpoint {
    pub route_order_length: usize,
    pub routes_length: usize,
    pub values: GridRoutingInstrumentationCheckpointValues,
}

pub fn create_grid_routing_instrumentation_checkpoint(
    metrics: &GridRoutingInstrumentation,
) -> GridRoutingInstrumentationCheckpoint {
    GridRoutingInstrumentationCheckpoint {
        route_order_length: metrics.route_order.len(),
        routes_length: metrics.routes.len(),
        values: GridRoutingInstrumentationCheckpointValues {
            hierarchy_portal_pairs: metrics.hierarchy_portal_pairs,
            hierarchy_portal_transition_length: metrics.hierarchy_portal_transition_length,
            hierarchy_portal_alternative_selections: metrics
                .hierarchy_portal_alternative_selections,
            hierarchy_boundary_transitions: metrics.hierarchy_boundary_transitions,
            label_aware_self_loops_committed: metrics.label_aware_self_loops_committed,
            routes_found: metrics.routes_found,
            routes_impossible: metrics.routes_impossible,
            compatibility_segments: metrics.compatibility_segments,
            compatibility_recoveries: metrics.compatibility_recoveries,
            bundle_separation_relaxations: metrics.bundle_separation_relaxations,
            route_length: metrics.route_length,
            bend_count: metrics.bend_count,
            crossing_count: metrics.crossing_count,
            shared_length: metrics.shared_length,
        },
    }
}

pub fn restore_grid_routing_instrumentation_checkpoint(
    metrics: &mut GridRoutingInstrumentation,
    checkpoint: GridRoutingInstrumentationCheckpoint,
) {
    metrics.route_order.truncate(checkpoint.route_order_length);
    metrics.routes.truncate(checkpoint.routes_length);
    metrics.hierarchy_portal_pairs = checkpoint.values.hierarchy_portal_pairs;
    metrics.hierarchy_portal_transition_length =
        checkpoint.values.hierarchy_portal_transition_length;
    metrics.hierarchy_portal_alternative_selections =
        checkpoint.values.hierarchy_portal_alternative_selections;
    metrics.hierarchy_boundary_transitions = checkpoint.values.hierarchy_boundary_transitions;
    metrics.label_aware_self_loops_committed = checkpoint.values.label_aware_self_loops_committed;
    metrics.routes_found = checkpoint.values.routes_found;
    metrics.routes_impossible = checkpoint.values.routes_impossible;
    metrics.compatibility_segments = checkpoint.values.compatibility_segments;
    metrics.compatibility_recoveries = checkpoint.values.compatibility_recoveries;
    metrics.bundle_separation_relaxations = checkpoint.values.bundle_separation_relaxations;
    metrics.route_length = checkpoint.values.route_length;
    metrics.bend_count = checkpoint.values.bend_count;
    metrics.crossing_count = checkpoint.values.crossing_count;
    metrics.shared_length = checkpoint.values.shared_length;
}

fn interval_overlap(first_start: f64, first_end: f64, second_start: f64, second_end: f64) -> f64 {
    (first_start.max(first_end).min(second_start.max(second_end))
        - first_start.min(first_end).max(second_start.min(second_end)))
    .max(0.0)
}

fn shared_segment_length(first: &[RouterPoint], second: &[RouterPoint]) -> f64 {
    let first_segments = normalize_polyline(first).segments;
    let second_segments = normalize_polyline(second).segments;
    let mut total = 0.0;
    for a in &first_segments {
        for b in &second_segments {
            if a.orientation != b.orientation {
                continue;
            }
            if a.orientation == SegmentOrientation::H && a.a.y == b.a.y {
                total += interval_overlap(a.a.x, a.b.x, b.a.x, b.b.x);
            } else if a.orientation == SegmentOrientation::V && a.a.x == b.a.x {
                total += interval_overlap(a.a.y, a.b.y, b.a.y, b.b.y);
            }
        }
    }
    total
}

fn points_signature(points: &[RouterPoint]) -> String {
    let entries: Vec<String> = points
        .iter()
        .map(|point| {
            format!(
                "{{\"x\":{},\"y\":{}}}",
                js_number(point.x),
                js_number(point.y)
            )
        })
        .collect();
    format!("[{}]", entries.join(","))
}

/// Records one committed route. Quality metrics compare only against previously committed routes,
/// matching visible route order and avoiding double-counting pairs.
pub fn record_grid_route(
    metrics: &mut GridRoutingInstrumentation,
    edge_id: &str,
    points: &[RouterPoint],
    previous_routes: &[Vec<RouterPoint>],
    boundary_transition_count: usize,
    lane_offset: f64,
) {
    let normalized = normalize_polyline(points);
    let mut crossing_count = 0usize;
    let mut shared_length = 0.0;
    for previous in previous_routes {
        let previous_segments = normalize_polyline(previous).segments;
        for segment in &normalized.segments {
            for previous_segment in &previous_segments {
                if segments_cross(*segment, *previous_segment) {
                    crossing_count += 1;
                }
            }
        }
        shared_length += shared_segment_length(&normalized.points, previous);
    }
    let route = GridRouteInstrumentation {
        edge_id: edge_id.to_string(),
        route_order: metrics.routes.len(),
        lane_offset,
        route_length: crate::layout::grid::geometry::manhattan_length(&normalized.points),
        bend_count: normalized.bends,
        crossing_count,
        shared_length,
        boundary_transition_count,
        route_signature: points_signature(&normalized.points),
    };
    metrics.route_order.push(edge_id.to_string());
    metrics.routes.push(route.clone());
    metrics.routes_found += 1;
    metrics.route_length += route.route_length;
    metrics.bend_count += route.bend_count;
    metrics.crossing_count += route.crossing_count;
    metrics.shared_length += route.shared_length;
    metrics.hierarchy_boundary_transitions += boundary_transition_count;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(edge_id: &str) -> GridRouteInstrumentation {
        GridRouteInstrumentation {
            edge_id: edge_id.to_string(),
            route_order: 0,
            lane_offset: 0.0,
            route_length: 10.0,
            bend_count: 1,
            crossing_count: 0,
            shared_length: 0.0,
            boundary_transition_count: 0,
            route_signature: edge_id.to_string(),
        }
    }

    #[test]
    fn classifies_every_metric_as_transactional_or_cumulative() {
        assert_eq!(GRID_ROUTING_METRIC_DISPOSITION.len(), 45);
        let disposition = |name: &str| {
            GRID_ROUTING_METRIC_DISPOSITION
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, disposition)| *disposition)
        };
        assert_eq!(
            disposition("compatibilityFastPaths"),
            Some(GridRoutingMetricDisposition::Cumulative)
        );
        assert_eq!(
            disposition("labelOverlayBuilds"),
            Some(GridRoutingMetricDisposition::Cumulative)
        );
        assert_eq!(
            disposition("labelOverlayVertices"),
            Some(GridRoutingMetricDisposition::Cumulative)
        );

        let mut transactional: Vec<&str> = GRID_ROUTING_METRIC_DISPOSITION
            .iter()
            .filter(|(key, disposition)| {
                *disposition == GridRoutingMetricDisposition::Transactional
                    && *key != "routeOrder"
                    && *key != "routes"
            })
            .map(|(key, _)| *key)
            .collect();
        transactional.sort_unstable();
        let checkpoint =
            create_grid_routing_instrumentation_checkpoint(&create_grid_routing_instrumentation());
        assert!(!transactional.is_empty());
        assert_eq!(checkpoint.values.routes_found, 0);
        assert_eq!(checkpoint.values.route_length, 0.0);
    }

    #[test]
    fn restores_committed_output_while_preserving_work_performed() {
        let mut metrics = create_grid_routing_instrumentation();
        metrics.route_order.push("committed".to_string());
        metrics.routes.push(route("committed"));
        let checkpoint = create_grid_routing_instrumentation_checkpoint(&metrics);
        let before_attempt = metrics.clone();

        metrics.containers_built += 7;
        metrics.base_topology_builds += 7;
        metrics.base_vertices += 7;
        metrics.base_adjacency_entries += 7;
        metrics.build_sweep_events += 7;
        metrics.endpoint_overlay_builds += 7;
        metrics.endpoint_overlay_vertices += 7;
        metrics.hierarchy_portal_pairs += 7;
        metrics.hierarchy_portal_transition_length += 7.0;
        metrics.hierarchy_portal_alternative_attempts += 7;
        metrics.hierarchy_portal_alternative_selections += 7;
        metrics.hierarchy_boundary_transitions += 7;
        metrics.label_overlay_builds += 7;
        metrics.label_overlay_vertices += 7;
        metrics.label_spacing_edges_examined += 7;
        metrics.label_spacing_eligible_edges += 7;
        metrics.label_spacing_boundaries_expanded += 7;
        metrics.label_spacing_pixels_added += 7.0;
        metrics.label_aware_self_loop_candidates += 7;
        metrics.label_aware_self_loops_committed += 7;
        metrics.label_aware_self_loop_fallbacks += 7;
        metrics.searches += 7;
        metrics.expanded_states += 7;
        metrics.max_open_set += 7;
        metrics.routes_found += 7;
        metrics.routes_impossible += 7;
        metrics.resource_limit_fallbacks += 7;
        for reason in &mut metrics.fallback_reasons {
            *reason += 7;
        }
        metrics.fallback_validation_failures += 7;
        metrics.compatibility_segments += 7;
        metrics.compatibility_fast_path_attempts += 7;
        metrics.compatibility_fast_paths += 7;
        metrics.compatibility_fast_path_validation_failures += 7;
        metrics.compatibility_fast_path_non_minimal_routes += 7;
        metrics.compatibility_recoveries += 7;
        metrics.compatibility_validation_failures += 7;
        metrics.bundle_separation_relaxations += 7;
        metrics.route_length += 7.0;
        metrics.bend_count += 7;
        metrics.crossing_count += 7;
        metrics.shared_length += 7.0;
        metrics.estimated_bytes += 7.0;
        metrics.search_workspace_bytes += 7.0;
        metrics.route_order.push("failed-attempt".to_string());
        metrics.routes.push(route("failed-attempt"));
        let after_attempt = metrics.clone();

        restore_grid_routing_instrumentation_checkpoint(&mut metrics, checkpoint);

        assert_eq!(
            metrics.hierarchy_portal_pairs,
            before_attempt.hierarchy_portal_pairs
        );
        assert_eq!(metrics.routes_found, before_attempt.routes_found);
        assert_eq!(metrics.routes_impossible, before_attempt.routes_impossible);
        assert_eq!(metrics.route_length, before_attempt.route_length);
        assert_eq!(metrics.bend_count, before_attempt.bend_count);
        assert_eq!(metrics.crossing_count, before_attempt.crossing_count);
        assert_eq!(metrics.shared_length, before_attempt.shared_length);
        assert_eq!(metrics.route_order, before_attempt.route_order);
        assert_eq!(metrics.routes, before_attempt.routes);
        assert_eq!(metrics.searches, after_attempt.searches);
        assert_eq!(metrics.expanded_states, after_attempt.expanded_states);
        assert_eq!(
            metrics.compatibility_fast_paths,
            after_attempt.compatibility_fast_paths
        );
        assert_eq!(
            metrics.resource_limit_fallbacks,
            after_attempt.resource_limit_fallbacks
        );
        assert_eq!(metrics.fallback_reasons, after_attempt.fallback_reasons);
        assert_eq!(metrics.estimated_bytes, after_attempt.estimated_bytes);
    }
}
