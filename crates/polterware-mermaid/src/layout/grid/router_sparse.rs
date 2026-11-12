//! Port of Mermaid 12.3.0's
//! `rendering-util/layout-algorithms/grid/routerSparse.ts`.
//!
//! Sparse routing overlays edge-specific endpoints onto immutable container visibility graphs,
//! then validates every result before it can replace the deterministic corridor route.

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::layout::grid::geometry::{manhattan_length, normalize_polyline, rect_for_node};
use crate::layout::grid::router::{GridRoutingDualRouteComparison, GridRoutingRouteQuality};
use crate::layout::grid::router_compatibility::{
    SELF_LOOP_PORT_GAP, SELF_LOOP_PORT_OFFSET_STEP, SegmentAttachment,
    SegmentAttachmentAlternative, are_exactly_axis_aligned, label_aware_self_loop_candidates,
    ordered_self_loop_sides, route_obstacle_clear_self_loop,
};
use crate::layout::grid::router_constraint::{
    endpoint_pair_lower_bound, pair_arc_allowed, route_has_distinct_pair_ports,
    route_satisfies_pair_constraints, validate_container_segment, validate_same_container_route,
};
use crate::layout::grid::router_errors::{
    GridRoutingError, GridRoutingResourceLimitError, SearchStateScope,
};
use crate::layout::grid::router_instrumentation::{
    GridRoutingFallbackReason, GridRoutingInstrumentation,
};
use crate::layout::grid::router_occupancy::RouteOccupancyIndex;
use crate::layout::grid::router_planning::{
    EdgeRoutePlan, EndpointCandidate, TERMINAL_APPROACH_PX, side_interval, side_order,
};
use crate::layout::grid::router_search::{
    RouterSearchOptions, RouterSearchResult, RouterSearchWorkspace, RouterTupleCost,
    assert_search_budget_available, compare_tuple_cost, find_shortest_route,
};
use crate::layout::grid::router_topology::{
    DEFAULT_MAX_ROUTING_ESTIMATED_BYTES, EndpointOverlayScratch, build_endpoint_routing_overlay,
    routing_point_key,
};
use crate::layout::grid::types::{
    GridErrorCode, GridLayoutResult, GridOrientation, GridRoutingContext, GridSide, Node,
    ROOT_CONTAINER_ID, RouterPoint, grid_error,
};

// Sparse routing validates every result before it can replace the deterministic corridor route.

fn route_not_found(
    message: impl Into<String>,
    details: Option<serde_json::Value>,
) -> GridRoutingError {
    GridRoutingError::Grid(grid_error(GridErrorCode::RouteNotFound, message, details))
}

/// Mutable occupancy view used while a bundle keeps its lanes apart.
pub struct SparseOccupied<'a> {
    pub index: &'a RouteOccupancyIndex,
    pub pair_key: &'a str,
    pub prioritize_endpoint_candidates: bool,
}

/// The sparse hierarchy segment plus the portal selections a successful attempt committed.
#[derive(Clone, Debug, PartialEq)]
pub struct SparseSegmentRoute {
    pub points: Vec<RouterPoint>,
    pub selections: Vec<PortalSelection>,
}

/// A successful portal attachment that must replace the demand coordinate and paired portal after
/// the route that selected it validates.
#[derive(Clone, Debug, PartialEq)]
pub struct PortalSelection {
    pub demand_key: String,
    pub portal: crate::layout::grid::types::PairedPortal,
    pub coordinate: f64,
}

fn record_fallback(
    context: &mut GridRoutingContext,
    metrics: &mut Option<GridRoutingInstrumentation>,
    reason: GridRoutingFallbackReason,
    _edge_id: &str,
    _container_id: &str,
    search_state_scope: Option<SearchStateScope>,
) {
    if let Some(metrics) = metrics.as_mut() {
        metrics.resource_limit_fallbacks += 1;
        metrics.increment_reason(reason);
    }
    if reason == GridRoutingFallbackReason::SearchStateCap
        && search_state_scope == Some(SearchStateScope::Invocation)
        && !context.search_budget_warning_emitted
    {
        context.search_budget_warning_emitted = true;
    }
}

#[allow(clippy::too_many_arguments)]
fn run_validated_fallback<T>(
    context: &mut GridRoutingContext,
    metrics: &mut Option<GridRoutingInstrumentation>,
    reason: GridRoutingFallbackReason,
    edge_id: &str,
    container_id: &str,
    search_state_scope: Option<SearchStateScope>,
    fallback: T,
    is_valid: impl Fn(&T) -> bool,
    include_container_id_in_error: bool,
) -> Result<T, GridRoutingError> {
    record_fallback(
        context,
        metrics,
        reason,
        edge_id,
        container_id,
        search_state_scope,
    );
    if is_valid(&fallback) {
        return Ok(fallback);
    }
    if let Some(metrics) = metrics.as_mut() {
        metrics.fallback_validation_failures += 1;
    }
    let mut details = serde_json::Map::new();
    details.insert(
        "edgeId".to_string(),
        serde_json::Value::String(edge_id.to_string()),
    );
    if include_container_id_in_error {
        details.insert(
            "containerId".to_string(),
            serde_json::Value::String(container_id.to_string()),
        );
    }
    details.insert(
        "reason".to_string(),
        serde_json::Value::String(reason.as_str().to_string()),
    );
    Err(route_not_found(
        format!("Invalid legacy fallback for \"{edge_id}\""),
        Some(serde_json::Value::Object(details)),
    ))
}

struct PairRecord {
    source_index: usize,
    target_index: usize,
    pair_rank: f64,
    lower_cost: RouterTupleCost,
}

struct PairOutcome {
    result: RouterSearchResult,
    points: Vec<RouterPoint>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PairSearchMode {
    All,
    Preferred,
    AssignedSide,
}

#[allow(clippy::too_many_arguments)]
fn search_pairs(
    mode: PairSearchMode,
    pairs: &[PairRecord],
    sources: &[EndpointCandidate],
    targets: &[EndpointCandidate],
    plan: &EdgeRoutePlan,
    source: &Node,
    target: &Node,
    _legacy_route: &mut dyn FnMut() -> Result<Vec<RouterPoint>, GridRoutingError>,
    result: &GridLayoutResult,
    nodes: &[Node],
    context: &mut GridRoutingContext,
    metrics: &mut Option<GridRoutingInstrumentation>,
    search_workspace: &mut RouterSearchWorkspace,
    overlay_scratch: &mut EndpointOverlayScratch,
    options: &crate::layout::grid::router::GridRoutingTestOptions,
    pair_routes: &[Vec<RouterPoint>],
    occupied: Option<&SparseOccupied<'_>>,
    avoid_occupied: bool,
    best: &mut Option<PairOutcome>,
    search_cap: &mut Option<GridRoutingResourceLimitError>,
) -> Result<(), GridRoutingError> {
    let bundle_requires_arcs = plan.bundle_size > 1;
    for pair in pairs {
        if mode == PairSearchMode::Preferred && pair.pair_rank != 0.0 {
            continue;
        }
        let source_candidate = &sources[pair.source_index];
        let target_candidate = &targets[pair.target_index];
        if mode == PairSearchMode::AssignedSide
            && (source_candidate.on_assigned_side == Some(false)
                || target_candidate.on_assigned_side == Some(false))
        {
            continue;
        }
        if let Some(best) = best
            && compare_tuple_cost(&best.result.cost, &pair.lower_cost) != Ordering::Greater
        {
            break;
        }
        let outcome = (|| -> Result<Option<PairOutcome>, GridRoutingError> {
            assert_search_budget_available(
                metrics.as_ref(),
                options.options.search_caps.as_ref(),
                Some(&context.search_budget),
            )?;
            let lane_points = if plan.bundle_size > 1
                && (source_candidate.connect.x == target_candidate.connect.x
                    || source_candidate.connect.y == target_candidate.connect.y)
            {
                vec![RouterPoint::new(
                    (source_candidate.connect.x + target_candidate.connect.x) / 2.0,
                    (source_candidate.connect.y + target_candidate.connect.y) / 2.0,
                )]
            } else {
                Vec::new()
            };
            let topology = context
                .topologies
                .get(&plan.lca_container_id)
                .ok_or_else(|| {
                    route_not_found(
                        format!("Missing routing topology \"{}\"", plan.lca_container_id),
                        None,
                    )
                })?;
            let overlay = build_endpoint_routing_overlay(
                topology,
                source_candidate.connect,
                target_candidate.connect,
                metrics.as_mut(),
                Some(overlay_scratch),
                &lane_points,
            )?;
            let live_estimated_bytes =
                context.base_estimated_bytes + overlay.estimated_bytes - topology.estimated_bytes;
            let memory_cap = options
                .options
                .topology_caps
                .and_then(|caps| caps.max_estimated_bytes)
                .unwrap_or(DEFAULT_MAX_ROUTING_ESTIMATED_BYTES);
            if live_estimated_bytes > memory_cap {
                return Err(GridRoutingError::ResourceLimit(
                    GridRoutingResourceLimitError::new(
                        GridRoutingFallbackReason::EstimatedMemoryCap,
                        "Grid routing estimated_memory_cap exceeded",
                        None,
                    ),
                ));
            }
            if let Some(metrics) = metrics.as_mut() {
                metrics.estimated_bytes = metrics.estimated_bytes.max(live_estimated_bytes);
            }
            let source_id = overlay
                .point_vertex_ids
                .get(&routing_point_key(source_candidate.connect));
            let target_id = overlay
                .point_vertex_ids
                .get(&routing_point_key(target_candidate.connect));
            let (Some(source_id), Some(target_id)) = (source_id.copied(), target_id.copied())
            else {
                return Err(route_not_found(
                    format!(
                        "Endpoint projection is missing from the routing overlay for \"{}\"",
                        plan.edge.id
                    ),
                    Some(serde_json::json!({
                        "edgeId": plan.edge.id,
                        "containerId": plan.lca_container_id,
                        "source": { "x": source_candidate.connect.x, "y": source_candidate.connect.y },
                        "target": { "x": target_candidate.connect.x, "y": target_candidate.connect.y },
                    })),
                ));
            };
            let source_orientation = if source_candidate.side == GridSide::Left
                || source_candidate.side == GridSide::Right
            {
                GridOrientation::H
            } else {
                GridOrientation::V
            };
            let target_orientation = if target_candidate.side == GridSide::Left
                || target_candidate.side == GridSide::Right
            {
                GridOrientation::H
            } else {
                GridOrientation::V
            };
            let mut search_caps = options.options.search_caps.unwrap_or_default();
            search_caps.max_estimated_bytes = options
                .options
                .search_caps
                .and_then(|caps| caps.max_estimated_bytes)
                .or_else(|| {
                    options
                        .options
                        .topology_caps
                        .and_then(|caps| caps.max_estimated_bytes)
                })
                .or(Some(DEFAULT_MAX_ROUTING_ESTIMATED_BYTES));
            let arc_allowed = |from: RouterPoint, to: RouterPoint| {
                (plan.bundle_size <= 1 || pair_arc_allowed(from, to, pair_routes))
                    && !(avoid_occupied
                        && occupied.is_some_and(|occupied| {
                            occupied
                                .index
                                .conflicts_with_arc(from, to, occupied.pair_key)
                        }))
            };
            let arc_allowed_ref: Option<&dyn Fn(RouterPoint, RouterPoint) -> bool> =
                if bundle_requires_arcs || avoid_occupied {
                    Some(&arc_allowed)
                } else {
                    None
                };
            let result_for_pair = find_shortest_route(
                &overlay,
                source_id,
                target_id,
                RouterSearchOptions {
                    metrics: metrics.as_mut(),
                    caps: Some(search_caps),
                    endpoint_candidate_rank: pair.pair_rank,
                    record_outcome: Some(false),
                    budget: Some(&mut context.search_budget),
                    initial_orientation: Some(source_orientation),
                    initial_length: Some(TERMINAL_APPROACH_PX),
                    target_orientation: Some(target_orientation),
                    target_length: Some(TERMINAL_APPROACH_PX),
                    topology_validated: true,
                    workspace: Some(search_workspace),
                    estimated_bytes_base: Some(live_estimated_bytes),
                    arc_allowed: arc_allowed_ref,
                    ..Default::default()
                },
            )?;
            let Some(result_for_pair) = result_for_pair else {
                return Ok(None);
            };
            let mut route_points = vec![source_candidate.port];
            route_points.extend(result_for_pair.points.iter().copied());
            route_points.push(target_candidate.port);
            let points = normalize_polyline(&route_points).points;
            if !validate_same_container_route(
                &points,
                source,
                target,
                &plan.lca_container_id,
                result,
                nodes,
            ) {
                return Ok(None);
            }
            if plan.bundle_size > 1 && !route_satisfies_pair_constraints(&points, pair_routes) {
                return Ok(None);
            }
            if avoid_occupied
                && occupied.is_some_and(|occupied| {
                    occupied
                        .index
                        .conflicts_with_route(&points, occupied.pair_key)
                })
            {
                return Ok(None);
            }
            Ok(Some(PairOutcome {
                result: result_for_pair,
                points,
            }))
        })();
        match outcome {
            Ok(Some(candidate)) => {
                let better = best.as_ref().is_none_or(|best| {
                    compare_tuple_cost(&candidate.result.cost, &best.result.cost) == Ordering::Less
                });
                if better {
                    *best = Some(candidate);
                    *search_cap = None;
                }
            }
            Ok(None) => {}
            Err(GridRoutingError::ResourceLimit(error)) => {
                if best.is_some() {
                    break;
                }
                *search_cap = Some(error);
                break;
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn sparse_same_container_route(
    plan: &EdgeRoutePlan,
    sources: &[EndpointCandidate],
    targets: &[EndpointCandidate],
    source: &Node,
    target: &Node,
    legacy_route: &mut dyn FnMut() -> Result<Vec<RouterPoint>, GridRoutingError>,
    result: &GridLayoutResult,
    nodes: &[Node],
    context: &mut GridRoutingContext,
    metrics: &mut Option<GridRoutingInstrumentation>,
    search_workspace: &mut RouterSearchWorkspace,
    overlay_scratch: &mut EndpointOverlayScratch,
    options: &crate::layout::grid::router::GridRoutingTestOptions,
    pair_routes: &[Vec<RouterPoint>],
    occupied: Option<&SparseOccupied<'_>>,
) -> Result<Vec<RouterPoint>, GridRoutingError> {
    let fallback_reason = context
        .fallback_containers
        .get(&plan.lca_container_id)
        .copied();
    if let Some(fallback_reason) = fallback_reason {
        let fallback = legacy_route()?;
        return run_validated_fallback(
            context,
            metrics,
            fallback_reason,
            &plan.edge.id,
            &plan.lca_container_id,
            None,
            fallback,
            |legacy| {
                validate_same_container_route(
                    legacy,
                    source,
                    target,
                    &plan.lca_container_id,
                    result,
                    nodes,
                ) && (plan.bundle_size <= 1
                    || route_satisfies_pair_constraints(legacy, pair_routes))
            },
            false,
        );
    }
    if !context.topologies.contains_key(&plan.lca_container_id) {
        return Err(route_not_found(
            format!("Missing routing topology \"{}\"", plan.lca_container_id),
            None,
        ));
    }
    if sources.is_empty() || targets.is_empty() {
        if let Some(metrics) = metrics.as_mut() {
            metrics.routes_impossible += 1;
        }
        return Err(route_not_found(
            if plan.bundle_size > 1 {
                format!("No distinct lane route for \"{}\"", plan.edge.id)
            } else {
                format!("No legal endpoint candidates for \"{}\"", plan.edge.id)
            },
            Some(serde_json::json!({ "edgeId": plan.edge.id })),
        ));
    }

    let mut pairs: Vec<PairRecord> = Vec::new();
    for (source_index, source_candidate) in sources.iter().enumerate() {
        for (target_index, target_candidate) in targets.iter().enumerate() {
            let pair_rank = source_candidate.rank * targets.len() as f64 + target_candidate.rank;
            let (length, bends) = endpoint_pair_lower_bound(source_candidate, target_candidate);
            pairs.push(PairRecord {
                source_index,
                target_index,
                pair_rank,
                lower_cost: [length, bends as f64, 0.0, pair_rank],
            });
        }
    }
    pairs.sort_by(|a, b| {
        compare_tuple_cost(&a.lower_cost, &b.lower_cost)
            .then_with(|| {
                side_order(sources[a.source_index].side)
                    .cmp(&side_order(sources[b.source_index].side))
            })
            .then_with(|| {
                side_order(targets[a.target_index].side)
                    .cmp(&side_order(targets[b.target_index].side))
            })
    });

    // Candidate pairs are ordered by an admissible lower bound. Once a completed route is no worse
    // than that bound, neither this pair nor any remaining pair can improve the current best.
    let mut best: Option<PairOutcome> = None;
    let mut search_cap: Option<GridRoutingResourceLimitError> = None;
    // Prefer a route that does not share a corridor with another edge. Sharing is the last resort,
    // so a failed constrained search retries without it instead of reporting no route.
    let avoid_occupied = occupied.is_some_and(|occupied| occupied.index.size() > 0);
    if occupied.is_some_and(|occupied| occupied.prioritize_endpoint_candidates) {
        // The assigned ports win unless another pair on the same sides at coordinated owners is
        // cheaper, which avoids accepting a legal but contorted route for the assigned ports.
        search_pairs(
            PairSearchMode::Preferred,
            &pairs,
            sources,
            targets,
            plan,
            source,
            target,
            legacy_route,
            result,
            nodes,
            context,
            metrics,
            search_workspace,
            overlay_scratch,
            options,
            pair_routes,
            occupied,
            avoid_occupied,
            &mut best,
            &mut search_cap,
        )?;
        search_pairs(
            PairSearchMode::AssignedSide,
            &pairs,
            sources,
            targets,
            plan,
            source,
            target,
            legacy_route,
            result,
            nodes,
            context,
            metrics,
            search_workspace,
            overlay_scratch,
            options,
            pair_routes,
            occupied,
            avoid_occupied,
            &mut best,
            &mut search_cap,
        )?;
    }
    if best.is_none() {
        search_pairs(
            PairSearchMode::All,
            &pairs,
            sources,
            targets,
            plan,
            source,
            target,
            legacy_route,
            result,
            nodes,
            context,
            metrics,
            search_workspace,
            overlay_scratch,
            options,
            pair_routes,
            occupied,
            avoid_occupied,
            &mut best,
            &mut search_cap,
        )?;
    }
    if best.is_none()
        && search_cap.is_none()
        && occupied.is_some_and(|occupied| occupied.index.size() > 0)
    {
        search_pairs(
            PairSearchMode::All,
            &pairs,
            sources,
            targets,
            plan,
            source,
            target,
            legacy_route,
            result,
            nodes,
            context,
            metrics,
            search_workspace,
            overlay_scratch,
            options,
            pair_routes,
            occupied,
            false,
            &mut best,
            &mut search_cap,
        )?;
    }
    if let Some(search_cap) = search_cap {
        // Resource caps select the compatibility route only after it passes the same geometry and
        // bundle constraints; caps never weaken correctness.
        let fallback = legacy_route()?;
        return run_validated_fallback(
            context,
            metrics,
            search_cap.reason,
            &plan.edge.id,
            &plan.lca_container_id,
            search_cap.search_state_scope,
            fallback,
            |legacy| {
                validate_same_container_route(
                    legacy,
                    source,
                    target,
                    &plan.lca_container_id,
                    result,
                    nodes,
                ) && (plan.bundle_size <= 1
                    || route_satisfies_pair_constraints(legacy, pair_routes))
            },
            false,
        );
    }
    let Some(best) = best else {
        if let Some(metrics) = metrics.as_mut() {
            metrics.routes_impossible += 1;
        }
        return Err(route_not_found(
            if plan.bundle_size > 1 {
                format!("No distinct lane route for \"{}\"", plan.edge.id)
            } else {
                format!("No cell-aware route for \"{}\"", plan.edge.id)
            },
            Some(serde_json::json!({ "edgeId": plan.edge.id })),
        ));
    };
    if let Some(sink) = &options.on_dual_route_comparison {
        let legacy = legacy_route()?;
        let sparse_normalized = normalize_polyline(&best.points);
        let legacy_normalized = normalize_polyline(&legacy);
        sink.borrow_mut().push(GridRoutingDualRouteComparison {
            edge_id: plan.edge.id.clone(),
            sparse: GridRoutingRouteQuality {
                valid: true,
                length: manhattan_length(&sparse_normalized.points),
                bends: sparse_normalized.bends,
            },
            legacy: GridRoutingRouteQuality {
                valid: validate_same_container_route(
                    &legacy,
                    source,
                    target,
                    &plan.lca_container_id,
                    result,
                    nodes,
                ),
                length: manhattan_length(&legacy_normalized.points),
                bends: legacy_normalized.bends,
            },
        });
    }
    Ok(best.points)
}

fn self_loop_attachments(
    owner: &Node,
    side: GridSide,
    index: usize,
) -> Option<(EndpointCandidate, EndpointCandidate)> {
    let (low, high) = side_interval(owner, side)?;
    if high - low < SELF_LOOP_PORT_GAP {
        return None;
    }
    let rect = rect_for_node(owner);
    let center = (low + high) / 2.0;
    let port_offset = index as f64 * SELF_LOOP_PORT_OFFSET_STEP;
    let coordinates = [
        center - SELF_LOOP_PORT_GAP / 2.0 - port_offset,
        center + SELF_LOOP_PORT_GAP / 2.0 + port_offset,
    ];
    if coordinates[0] < low || coordinates[1] > high {
        return None;
    }
    let candidate = |coordinate: f64| -> EndpointCandidate {
        let port = if side == GridSide::Left || side == GridSide::Right {
            RouterPoint::new(
                if side == GridSide::Left {
                    rect.left
                } else {
                    rect.right
                },
                coordinate,
            )
        } else {
            RouterPoint::new(
                coordinate,
                if side == GridSide::Top {
                    rect.top
                } else {
                    rect.bottom
                },
            )
        };
        EndpointCandidate {
            owner_id: owner.id.clone(),
            side,
            port,
            connect: RouterPoint::new(
                port.x
                    + match side {
                        GridSide::Left => -TERMINAL_APPROACH_PX,
                        GridSide::Right => TERMINAL_APPROACH_PX,
                        _ => 0.0,
                    },
                port.y
                    + match side {
                        GridSide::Top => -TERMINAL_APPROACH_PX,
                        GridSide::Bottom => TERMINAL_APPROACH_PX,
                        _ => 0.0,
                    },
            ),
            rank: 0.0,
            on_assigned_side: None,
        }
    };
    Some((candidate(coordinates[0]), candidate(coordinates[1])))
}

#[allow(clippy::too_many_arguments)]
pub fn sparse_self_loop_route(
    plan: &EdgeRoutePlan,
    owner: &Node,
    result: &GridLayoutResult,
    nodes: &[Node],
    context: &mut GridRoutingContext,
    metrics: &mut Option<GridRoutingInstrumentation>,
    search_workspace: &mut RouterSearchWorkspace,
    overlay_scratch: Option<&mut EndpointOverlayScratch>,
    owner_side_counts: &HashMap<String, usize>,
    self_loop_counts: &HashMap<String, usize>,
    pair_routes: &[Vec<RouterPoint>],
    options: &crate::layout::grid::router::GridRoutingTestOptions,
) -> Result<crate::layout::grid::router_compatibility::SelfLoopRoute, GridRoutingError> {
    // Try sides in load order and reserve progressively wider port pairs for repeated loops on the
    // same side.
    let container_id = owner
        .parent_id
        .clone()
        .unwrap_or_else(|| ROOT_CONTAINER_ID.to_string());
    let label_requirement = result.label_requirements.self_loops.get(&plan.edge.id);
    let fallback_reason = context.fallback_containers.get(&container_id).copied();
    if let Some(fallback_reason) = fallback_reason {
        let fallback = route_obstacle_clear_self_loop(
            owner,
            owner_side_counts,
            self_loop_counts,
            result,
            nodes,
            label_requirement,
            metrics.as_mut(),
        )?;
        let fallback = run_validated_fallback(
            context,
            metrics,
            fallback_reason,
            &plan.edge.id,
            &container_id,
            None,
            fallback,
            |legacy| {
                validate_same_container_route(
                    &legacy.points,
                    owner,
                    owner,
                    &container_id,
                    result,
                    nodes,
                ) && route_satisfies_pair_constraints(&legacy.points, pair_routes)
            },
            false,
        )?;
        if label_requirement.is_some()
            && let Some(metrics) = metrics.as_mut()
        {
            metrics.label_aware_self_loops_committed += 1;
        }
        return Ok(fallback);
    }
    let overlay_scratch = match overlay_scratch {
        Some(scratch) if context.topologies.contains_key(&container_id) => scratch,
        _ => {
            return Err(route_not_found(
                format!("Missing routing topology \"{container_id}\""),
                None,
            ));
        }
    };
    let ordered_sides = ordered_self_loop_sides(owner, owner_side_counts, self_loop_counts);
    if let Some(requirement) = label_requirement {
        for side in &ordered_sides {
            let index = self_loop_counts
                .get(&format!("{}:{}", owner.id, side.as_str()))
                .copied()
                .unwrap_or(0);
            let rect = rect_for_node(owner);
            let demand_coord = if *side == GridSide::Left || *side == GridSide::Right {
                rect.cy
            } else {
                rect.cx
            };
            let candidates =
                label_aware_self_loop_candidates(owner, *side, index, demand_coord, requirement);
            if let Some(metrics) = metrics.as_mut() {
                metrics.label_aware_self_loop_candidates += candidates.len();
            }
            for points in candidates {
                if validate_same_container_route(
                    &points,
                    owner,
                    owner,
                    &container_id,
                    result,
                    nodes,
                ) && route_satisfies_pair_constraints(&points, pair_routes)
                {
                    if let Some(metrics) = metrics.as_mut() {
                        metrics.label_aware_self_loops_committed += 1;
                    }
                    return Ok(crate::layout::grid::router_compatibility::SelfLoopRoute {
                        points,
                        side: *side,
                        index,
                    });
                }
            }
        }
        if let Some(metrics) = metrics.as_mut() {
            metrics.label_aware_self_loop_fallbacks += 1;
        }
    }
    for side in ordered_sides {
        let index = self_loop_counts
            .get(&format!("{}:{}", owner.id, side.as_str()))
            .copied()
            .unwrap_or(0);
        let Some((start, target)) = self_loop_attachments(owner, side, index) else {
            continue;
        };
        let outcome = (|| -> Result<Option<Vec<RouterPoint>>, GridRoutingError> {
            assert_search_budget_available(
                metrics.as_ref(),
                options.options.search_caps.as_ref(),
                Some(&context.search_budget),
            )?;
            let topology = context.topologies.get(&container_id).ok_or_else(|| {
                route_not_found(format!("Missing routing topology \"{container_id}\""), None)
            })?;
            let lane_points = vec![RouterPoint::new(
                (start.connect.x + target.connect.x) / 2.0,
                (start.connect.y + target.connect.y) / 2.0,
            )];
            let overlay = build_endpoint_routing_overlay(
                topology,
                start.connect,
                target.connect,
                metrics.as_mut(),
                Some(overlay_scratch),
                &lane_points,
            )?;
            let source_id = overlay
                .point_vertex_ids
                .get(&routing_point_key(start.connect))
                .copied();
            let target_id = overlay
                .point_vertex_ids
                .get(&routing_point_key(target.connect))
                .copied();
            let (Some(source_id), Some(target_id)) = (source_id, target_id) else {
                return Err(route_not_found(
                    format!(
                        "Self-loop endpoint projection is missing from the routing overlay for \"{}\"",
                        plan.edge.id
                    ),
                    Some(serde_json::json!({
                        "edgeId": plan.edge.id,
                        "containerId": container_id,
                        "source": { "x": start.connect.x, "y": start.connect.y },
                        "target": { "x": target.connect.x, "y": target.connect.y },
                        "side": side.as_str(),
                    })),
                ));
            };
            let orientation = if side == GridSide::Left || side == GridSide::Right {
                GridOrientation::H
            } else {
                GridOrientation::V
            };
            let mut search_caps = options.options.search_caps.unwrap_or_default();
            search_caps.max_estimated_bytes = options
                .options
                .search_caps
                .and_then(|caps| caps.max_estimated_bytes)
                .or_else(|| {
                    options
                        .options
                        .topology_caps
                        .and_then(|caps| caps.max_estimated_bytes)
                })
                .or(Some(DEFAULT_MAX_ROUTING_ESTIMATED_BYTES));
            let arc_allowed =
                |from: RouterPoint, to: RouterPoint| pair_arc_allowed(from, to, pair_routes);
            let route = find_shortest_route(
                &overlay,
                source_id,
                target_id,
                RouterSearchOptions {
                    metrics: metrics.as_mut(),
                    caps: Some(search_caps),
                    record_outcome: Some(false),
                    budget: Some(&mut context.search_budget),
                    initial_orientation: Some(orientation),
                    initial_side: Some(side),
                    initial_length: Some(TERMINAL_APPROACH_PX),
                    target_orientation: Some(orientation),
                    target_side: Some(side),
                    target_length: Some(TERMINAL_APPROACH_PX),
                    topology_validated: true,
                    workspace: Some(search_workspace),
                    estimated_bytes_base: Some(
                        context.base_estimated_bytes + overlay.estimated_bytes
                            - topology.estimated_bytes,
                    ),
                    arc_allowed: Some(&arc_allowed),
                    ..Default::default()
                },
            )?;
            let Some(route) = route else {
                return Ok(None);
            };
            let mut route_points = vec![start.port];
            route_points.extend(route.points.iter().copied());
            route_points.push(target.port);
            let points = normalize_polyline(&route_points).points;
            if validate_same_container_route(&points, owner, owner, &container_id, result, nodes)
                && route_satisfies_pair_constraints(&points, pair_routes)
            {
                return Ok(Some(points));
            }
            Ok(None)
        })();
        match outcome {
            Ok(Some(points)) => {
                return Ok(crate::layout::grid::router_compatibility::SelfLoopRoute {
                    points,
                    side,
                    index,
                });
            }
            Ok(None) => {}
            Err(GridRoutingError::ResourceLimit(error)) => {
                let fallback = route_obstacle_clear_self_loop(
                    owner,
                    owner_side_counts,
                    self_loop_counts,
                    result,
                    nodes,
                    label_requirement,
                    metrics.as_mut(),
                )?;
                let fallback = run_validated_fallback(
                    context,
                    metrics,
                    error.reason,
                    &plan.edge.id,
                    &container_id,
                    error.search_state_scope,
                    fallback,
                    |legacy| {
                        validate_same_container_route(
                            &legacy.points,
                            owner,
                            owner,
                            &container_id,
                            result,
                            nodes,
                        ) && route_satisfies_pair_constraints(&legacy.points, pair_routes)
                    },
                    false,
                )?;
                if label_requirement.is_some()
                    && let Some(metrics) = metrics.as_mut()
                {
                    metrics.label_aware_self_loops_committed += 1;
                }
                return Ok(fallback);
            }
            Err(error) => return Err(error),
        }
    }
    if let Some(metrics) = metrics.as_mut() {
        metrics.routes_impossible += 1;
    }
    Err(route_not_found(
        format!("No distinct self-loop route for \"{}\"", plan.edge.id),
        Some(serde_json::json!({ "edgeId": plan.edge.id, "nodeId": owner.id })),
    ))
}

fn append_alternative_attempts(
    attempts: &mut Vec<(SegmentAttachment, SegmentAttachment, Vec<PortalSelection>)>,
    alternatives_added: &mut bool,
    start: &SegmentAttachment,
    end: &SegmentAttachment,
    start_alternatives: &mut dyn FnMut() -> Vec<SegmentAttachmentAlternative>,
    end_alternatives: &mut dyn FnMut() -> Vec<SegmentAttachmentAlternative>,
) {
    if *alternatives_added {
        return;
    }
    *alternatives_added = true;
    let starts = start_alternatives();
    let ends = end_alternatives();
    for alternative in &ends {
        attempts.push((
            start.clone(),
            alternative.attachment.clone(),
            alternative.selection.clone(),
        ));
    }
    for alternative in &starts {
        attempts.push((
            alternative.attachment.clone(),
            end.clone(),
            alternative.selection.clone(),
        ));
    }
    for start_alternative in &starts {
        for end_alternative in &ends {
            let mut selections = start_alternative.selection.clone();
            selections.extend(end_alternative.selection.iter().cloned());
            attempts.push((
                start_alternative.attachment.clone(),
                end_alternative.attachment.clone(),
                selections,
            ));
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn sparse_container_segment(
    edge_id: &str,
    container_id: &str,
    start: &SegmentAttachment,
    end: &SegmentAttachment,
    legacy_route: &mut dyn FnMut() -> Result<Vec<RouterPoint>, GridRoutingError>,
    result: &GridLayoutResult,
    nodes: &[Node],
    context: &mut GridRoutingContext,
    metrics: &mut Option<GridRoutingInstrumentation>,
    search_workspace: &mut RouterSearchWorkspace,
    overlay_scratch: Option<&mut EndpointOverlayScratch>,
    options: &crate::layout::grid::router::GridRoutingTestOptions,
    pair_routes: &[Vec<RouterPoint>],
    end_alternatives: &mut dyn FnMut() -> Vec<SegmentAttachmentAlternative>,
    start_alternatives: &mut dyn FnMut() -> Vec<SegmentAttachmentAlternative>,
    allow_shared_pair_corridors: bool,
    occupied: Option<&SparseOccupied<'_>>,
) -> Result<SparseSegmentRoute, GridRoutingError> {
    let fallback_reason = context.fallback_containers.get(container_id).copied();
    if let Some(fallback_reason) = fallback_reason {
        let fallback = legacy_route()?;
        return Ok(SparseSegmentRoute {
            points: run_validated_fallback(
                context,
                metrics,
                fallback_reason,
                edge_id,
                container_id,
                None,
                fallback,
                |legacy| {
                    validate_container_segment(
                        legacy,
                        &start.owner_id,
                        &end.owner_id,
                        container_id,
                        result,
                        nodes,
                    ) && route_satisfies_pair_constraints(legacy, pair_routes)
                },
                true,
            )?,
            selections: Vec::new(),
        });
    }
    let overlay_scratch = match overlay_scratch {
        Some(scratch) if context.topologies.contains_key(container_id) => scratch,
        _ => {
            return Err(route_not_found(
                format!("Missing routing topology \"{container_id}\""),
                None,
            ));
        }
    };
    let mut attempts: Vec<(SegmentAttachment, SegmentAttachment, Vec<PortalSelection>)> =
        vec![(start.clone(), end.clone(), Vec::new())];
    // Alternatives are generated lazily only after the selected attachments fail. Their selections
    // are applied after a complete route validates.
    let mut alternatives_added = false;
    // Prefer attachments and corridors that do not run along another edge. Sharing is the last
    // resort, so a failed constrained pass retries without it.
    let mut pass_iterations: Vec<bool> =
        if occupied.is_some_and(|occupied| occupied.index.size() > 0) {
            vec![true, false]
        } else {
            vec![false]
        };
    for avoid_occupied in pass_iterations.drain(..) {
        let mut index = 0usize;
        while index < attempts.len() {
            let (attempted_start, attempted_end, selections) = attempts[index].clone();
            if index > 0
                && let Some(metrics) = metrics.as_mut()
            {
                metrics.hierarchy_portal_alternative_attempts += 1;
            }
            let aligned = are_exactly_axis_aligned(attempted_start.connect, attempted_end.connect);
            if aligned {
                let direct = normalize_polyline(&[
                    attempted_start.port,
                    attempted_start.connect,
                    attempted_end.connect,
                    attempted_end.port,
                ])
                .points;
                if validate_container_segment(
                    &direct,
                    &attempted_start.owner_id,
                    &attempted_end.owner_id,
                    container_id,
                    result,
                    nodes,
                ) && route_satisfies_pair_constraints(&direct, pair_routes)
                    && !(avoid_occupied
                        && occupied.is_some_and(|occupied| {
                            occupied
                                .index
                                .conflicts_with_route(&direct, occupied.pair_key)
                        }))
                {
                    if index > 0
                        && let Some(metrics) = metrics.as_mut()
                    {
                        metrics.hierarchy_portal_alternative_selections += 1;
                    }
                    return Ok(SparseSegmentRoute {
                        points: direct,
                        selections,
                    });
                }
            }
            let outcome = (|| -> Result<Option<Vec<RouterPoint>>, GridRoutingError> {
                assert_search_budget_available(
                    metrics.as_ref(),
                    options.options.search_caps.as_ref(),
                    Some(&context.search_budget),
                )?;
                let topology = context.topologies.get(container_id).ok_or_else(|| {
                    route_not_found(format!("Missing routing topology \"{container_id}\""), None)
                })?;
                let overlay = build_endpoint_routing_overlay(
                    topology,
                    attempted_start.connect,
                    attempted_end.connect,
                    metrics.as_mut(),
                    Some(overlay_scratch),
                    &[],
                )?;
                let live_estimated_bytes = context.base_estimated_bytes + overlay.estimated_bytes
                    - topology.estimated_bytes;
                let memory_cap = options
                    .options
                    .topology_caps
                    .and_then(|caps| caps.max_estimated_bytes)
                    .unwrap_or(DEFAULT_MAX_ROUTING_ESTIMATED_BYTES);
                if live_estimated_bytes > memory_cap {
                    return Err(GridRoutingError::ResourceLimit(
                        GridRoutingResourceLimitError::new(
                            GridRoutingFallbackReason::EstimatedMemoryCap,
                            "Grid routing estimated_memory_cap exceeded",
                            None,
                        ),
                    ));
                }
                let source_id = overlay
                    .point_vertex_ids
                    .get(&routing_point_key(attempted_start.connect))
                    .copied();
                let target_id = overlay
                    .point_vertex_ids
                    .get(&routing_point_key(attempted_end.connect))
                    .copied();
                let (Some(source_id), Some(target_id)) = (source_id, target_id) else {
                    return Ok(None);
                };
                let mut search_caps = options.options.search_caps.unwrap_or_default();
                search_caps.max_estimated_bytes = options
                    .options
                    .search_caps
                    .and_then(|caps| caps.max_estimated_bytes)
                    .or_else(|| {
                        options
                            .options
                            .topology_caps
                            .and_then(|caps| caps.max_estimated_bytes)
                    })
                    .or(Some(DEFAULT_MAX_ROUTING_ESTIMATED_BYTES));
                let arc_allowed = |from: RouterPoint, to: RouterPoint| {
                    (pair_routes.is_empty() || pair_arc_allowed(from, to, pair_routes))
                        && !(avoid_occupied
                            && occupied.is_some_and(|occupied| {
                                occupied
                                    .index
                                    .conflicts_with_arc(from, to, occupied.pair_key)
                            }))
                };
                let arc_allowed_ref: Option<&dyn Fn(RouterPoint, RouterPoint) -> bool> =
                    if !pair_routes.is_empty() || avoid_occupied {
                        Some(&arc_allowed)
                    } else {
                        None
                    };
                let route = find_shortest_route(
                    &overlay,
                    source_id,
                    target_id,
                    RouterSearchOptions {
                        metrics: metrics.as_mut(),
                        caps: Some(search_caps),
                        record_outcome: Some(false),
                        budget: Some(&mut context.search_budget),
                        initial_orientation: Some(
                            if attempted_start.side == GridSide::Left
                                || attempted_start.side == GridSide::Right
                            {
                                GridOrientation::H
                            } else {
                                GridOrientation::V
                            },
                        ),
                        initial_side: Some(attempted_start.side),
                        initial_length: Some(
                            (attempted_start.port.x - attempted_start.connect.x).abs()
                                + (attempted_start.port.y - attempted_start.connect.y).abs(),
                        ),
                        target_orientation: Some(
                            if attempted_end.side == GridSide::Left
                                || attempted_end.side == GridSide::Right
                            {
                                GridOrientation::H
                            } else {
                                GridOrientation::V
                            },
                        ),
                        target_side: Some(attempted_end.side),
                        target_length: Some(
                            (attempted_end.port.x - attempted_end.connect.x).abs()
                                + (attempted_end.port.y - attempted_end.connect.y).abs(),
                        ),
                        topology_validated: true,
                        workspace: Some(search_workspace),
                        estimated_bytes_base: Some(live_estimated_bytes),
                        arc_allowed: arc_allowed_ref,
                        ..Default::default()
                    },
                )?;
                let Some(route) = route else {
                    return Ok(None);
                };
                let mut route_points = vec![attempted_start.port];
                route_points.extend(route.points.iter().copied());
                route_points.push(attempted_end.port);
                let points = normalize_polyline(&route_points).points;
                if !validate_container_segment(
                    &points,
                    &attempted_start.owner_id,
                    &attempted_end.owner_id,
                    container_id,
                    result,
                    nodes,
                ) || !route_satisfies_pair_constraints(&points, pair_routes)
                    || (avoid_occupied
                        && occupied.is_some_and(|occupied| {
                            occupied
                                .index
                                .conflicts_with_route(&points, occupied.pair_key)
                        }))
                {
                    return Ok(None);
                }
                Ok(Some(points))
            })();
            match outcome {
                Ok(Some(points)) => {
                    if index > 0
                        && let Some(metrics) = metrics.as_mut()
                    {
                        metrics.hierarchy_portal_alternative_selections += 1;
                    }
                    return Ok(SparseSegmentRoute { points, selections });
                }
                Ok(None) => {
                    if index == 0 {
                        append_alternative_attempts(
                            &mut attempts,
                            &mut alternatives_added,
                            start,
                            end,
                            start_alternatives,
                            end_alternatives,
                        );
                    }
                }
                Err(GridRoutingError::ResourceLimit(error)) => {
                    let fallback = legacy_route()?;
                    return Ok(SparseSegmentRoute {
                        points: run_validated_fallback(
                            context,
                            metrics,
                            error.reason,
                            edge_id,
                            container_id,
                            error.search_state_scope,
                            fallback,
                            |legacy| {
                                validate_container_segment(
                                    legacy,
                                    &start.owner_id,
                                    &end.owner_id,
                                    container_id,
                                    result,
                                    nodes,
                                ) && route_satisfies_pair_constraints(legacy, pair_routes)
                            },
                            true,
                        )?,
                        selections: Vec::new(),
                    });
                }
                Err(error) => return Err(error),
            }
            index += 1;
        }
    }
    let compatibility = legacy_route()?;
    // Exhausted sparse geometry may recover through the corridor router. Bundled hierarchy segments
    // can share interior corridors only when their endpoint ports remain distinct.
    if validate_container_segment(
        &compatibility,
        &start.owner_id,
        &end.owner_id,
        container_id,
        result,
        nodes,
    ) && (route_satisfies_pair_constraints(&compatibility, pair_routes)
        || (allow_shared_pair_corridors
            && route_has_distinct_pair_ports(&compatibility, pair_routes)))
    {
        if let Some(metrics) = metrics.as_mut() {
            metrics.compatibility_segments += 1;
            metrics.compatibility_recoveries += 1;
        }
        return Ok(SparseSegmentRoute {
            points: compatibility,
            selections: Vec::new(),
        });
    }
    Err(route_not_found(
        format!(
            "No hierarchy route for \"{edge_id}\" in \"{container_id}\" from {} to {}",
            routing_point_key(start.connect),
            routing_point_key(end.connect)
        ),
        Some(serde_json::json!({ "edgeId": edge_id, "containerId": container_id })),
    ))
}
