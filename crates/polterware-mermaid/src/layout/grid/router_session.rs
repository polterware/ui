//! Port of Mermaid 12.3.0's
//! `rendering-util/layout-algorithms/grid/routerSession.ts`.
//!
//! The session is the transaction boundary for one routing invocation. It owns mutable
//! reservations, reusable search memory, and retry state while the base layout geometry remains
//! read-only. Commits are only published to the caller's layout after every bundle succeeds.

use std::collections::{HashMap, HashSet};

use crate::layout::grid::geometry::{clamp, rect_for_node};
use crate::layout::grid::router::GridRoutingTestOptions;
use crate::layout::grid::router_compatibility::{
    PreparedRoutingModes, SegmentAttachment, SegmentAttachmentAlternative,
    bounded_alternative_portal_coordinates, build_routing_context, combine_point_chains,
    compatibility_hierarchy_attachments, compatibility_lca_attachments,
    group_boundary_endpoint_attachment, item_attachment, portal_attachment, prepare_routing_modes,
    reverse_points, root_container_meta, route_within_container, validated_compatibility_segment,
};
use crate::layout::grid::router_constraint::{
    is_pair_separation_error, route_has_distinct_pair_ports, route_satisfies_pair_constraints,
    router_rect,
};
use crate::layout::grid::router_errors::GridRoutingError;
use crate::layout::grid::router_instrumentation::{
    GridRoutingInstrumentation, GridRoutingInstrumentationCheckpoint,
    create_grid_routing_instrumentation_checkpoint, record_grid_route,
    restore_grid_routing_instrumentation_checkpoint,
};
use crate::layout::grid::router_occupancy::RouteOccupancyIndex;
use crate::layout::grid::router_planning::{
    EdgeEndpointEntry, EdgeEndpointPlan, EdgeRoutePlan, PreparedEdgeRoutes, owner_group_title,
    owner_side_key, prepare_edge_routes, side_interval,
};
use crate::layout::grid::router_search::RouterSearchWorkspace;
use crate::layout::grid::router_sparse::{
    PortalSelection, SparseOccupied, sparse_container_segment, sparse_same_container_route,
    sparse_self_loop_route,
};
use crate::layout::grid::router_topology::{
    EndpointOverlayScratch, build_paired_portal, derive_portal_ranges,
};
use crate::layout::grid::types::{
    Edge, GridContainerId, GridCurve, GridErrorCode, GridLayoutResult, GridRoutingContext,
    GridSide, LayoutData, Node, PairedPortal, ROOT_CONTAINER_ID, RouterPoint, RouterRect,
    grid_error,
};

fn route_not_found(message: impl Into<String>) -> GridRoutingError {
    GridRoutingError::Grid(grid_error(GridErrorCode::RouteNotFound, message, None))
}

fn group_title_rect(node: &Node) -> Option<RouterRect> {
    node.group_title_rect.map(|title| RouterRect {
        left: title.left,
        right: title.right,
        top: title.top,
        bottom: title.bottom,
    })
}

fn alternative_item_attachments(
    result: &GridLayoutResult,
    nodes: &[Node],
    prepared: &PreparedEdgeRoutes,
    plan: &EdgeRoutePlan,
    entry: &EdgeEndpointEntry,
    container_id: &GridContainerId,
) -> Result<Vec<SegmentAttachmentAlternative>, GridRoutingError> {
    let current = item_attachment(
        &entry.owner_id,
        entry.side,
        &entry.demand_key,
        container_id,
        result,
        nodes,
        &prepared.demand_coords,
        None,
    )?;
    let Some(owner_index) = result.forest.node_by_id.get(&entry.owner_id).copied() else {
        return Ok(Vec::new());
    };
    let owner = &nodes[owner_index];
    let mut attachments = Vec::new();
    for side in [
        GridSide::Right,
        GridSide::Bottom,
        GridSide::Left,
        GridSide::Top,
    ] {
        if side == current.side || (side == GridSide::Top && owner_group_title(owner)) {
            continue;
        }
        let Some((low, high)) = side_interval(owner, side) else {
            continue;
        };
        let rect = rect_for_node(owner);
        let center = if side == GridSide::Left || side == GridSide::Right {
            rect.cy
        } else {
            rect.cx
        };
        let coordinate = clamp(center + plan.lane_offset, low, high);
        let attachment = item_attachment(
            &entry.owner_id,
            side,
            &entry.demand_key,
            container_id,
            result,
            nodes,
            &prepared.demand_coords,
            Some(coordinate),
        )?;
        attachments.push(SegmentAttachmentAlternative {
            attachment: SegmentAttachment {
                owner_id: entry.owner_id.clone(),
                port: attachment.port,
                connect: attachment.connect,
                orientation: attachment.orientation,
                side: attachment.side,
            },
            selection: Vec::new(),
        });
        if attachments.len() == 3 {
            break;
        }
    }
    Ok(attachments)
}

fn alternative_paired_portal_attachments(
    result: &GridLayoutResult,
    nodes: &[Node],
    prepared: &PreparedEdgeRoutes,
    paired_portals: &HashMap<String, PairedPortal>,
    entry: &EdgeEndpointEntry,
    interior: bool,
) -> Vec<SegmentAttachmentAlternative> {
    let Some(owner_index) = result.forest.node_by_id.get(&entry.owner_id).copied() else {
        return Vec::new();
    };
    let owner = &nodes[owner_index];
    if !owner.is_group {
        return Vec::new();
    }
    let rect = router_rect(owner);
    let title = group_title_rect(owner);
    let Ok(ranges) = derive_portal_ranges(&entry.owner_id, rect, title) else {
        return Vec::new();
    };
    let Some(range) = ranges.iter().find(|range| range.side == entry.side) else {
        return Vec::new();
    };
    let selected = match paired_portals.get(&entry.demand_key) {
        Some(portal) => portal.tangential_coordinate,
        None => {
            let coordinate = prepared
                .demand_coords
                .get(&entry.demand_key)
                .copied()
                .unwrap_or(
                    if entry.side == GridSide::Left || entry.side == GridSide::Right {
                        (rect.top + rect.bottom) / 2.0
                    } else {
                        (rect.left + rect.right) / 2.0
                    },
                );
            clamp(coordinate, range.low, range.high)
        }
    };
    let corridors: &[f64] = match result.containers.get(&owner.id) {
        Some(container) => {
            if entry.side == GridSide::Left || entry.side == GridSide::Right {
                &container.horizontal_corridors
            } else {
                &container.vertical_corridors
            }
        }
        None => &[],
    };
    bounded_alternative_portal_coordinates(selected, range.low, range.high, corridors, 3)
        .into_iter()
        .filter_map(|coordinate| {
            let portal =
                build_paired_portal(&entry.owner_id, rect, title, entry.side, coordinate).ok()?;
            Some(SegmentAttachmentAlternative {
                attachment: portal_attachment(&portal, interior),
                selection: vec![PortalSelection {
                    demand_key: entry.demand_key.clone(),
                    portal,
                    coordinate,
                }],
            })
        })
        .collect()
}

struct BundleEdgeCheckpoint {
    edge_id: String,
    points: Vec<RouterPoint>,
    curve: Option<GridCurve>,
    corner_radius: Option<f64>,
}

struct BundleCheckpoint {
    pair_key: String,
    occupancy: Option<(GridContainerId, usize)>,
    pair_routes: Option<Vec<Vec<RouterPoint>>>,
    demand_keys: Vec<String>,
    paired_portals: HashMap<String, PairedPortal>,
    demand_coords: HashMap<String, f64>,
    edges: Vec<BundleEdgeCheckpoint>,
    instrumented_routes_length: Option<usize>,
    metrics: Option<GridRoutingInstrumentationCheckpoint>,
}

/// Owns every mutable routing structure for one invocation and commits routes in deterministic
/// order.
pub struct GridEdgeRoutingSession<'a> {
    layout: LayoutData,
    result: GridLayoutResult,
    metrics: Option<GridRoutingInstrumentation>,
    options: &'a GridRoutingTestOptions,
    prepared: PreparedEdgeRoutes,
    modes: PreparedRoutingModes,
    routing_context: GridRoutingContext,
    search_workspace: RouterSearchWorkspace,
    endpoint_overlay_scratch: HashMap<GridContainerId, EndpointOverlayScratch>,
    paired_portals: HashMap<String, PairedPortal>,
    owner_side_counts: HashMap<String, usize>,
    instrumented_routes: Option<Vec<Vec<RouterPoint>>>,
    self_loop_counts: HashMap<String, usize>,
    pair_routes: HashMap<String, Vec<Vec<RouterPoint>>>,
    edge_index_by_id: HashMap<String, usize>,
    routed: bool,
}

impl<'a> GridEdgeRoutingSession<'a> {
    pub fn new(
        layout: LayoutData,
        result: &GridLayoutResult,
        metrics: Option<GridRoutingInstrumentation>,
        options: &'a GridRoutingTestOptions,
    ) -> Self {
        let edge_index_by_id = layout
            .edges
            .iter()
            .enumerate()
            .map(|(index, edge)| (edge.id.clone(), index))
            .collect();
        Self {
            layout,
            result: result.clone(),
            metrics,
            options,
            prepared: PreparedEdgeRoutes::default(),
            modes: PreparedRoutingModes::default(),
            routing_context: GridRoutingContext::default(),
            search_workspace: RouterSearchWorkspace::new(),
            endpoint_overlay_scratch: HashMap::new(),
            paired_portals: HashMap::new(),
            owner_side_counts: HashMap::new(),
            instrumented_routes: None,
            self_loop_counts: HashMap::new(),
            pair_routes: HashMap::new(),
            edge_index_by_id,
            routed: false,
        }
    }

    /// Returns the routed working edges and the instrumentation that observed the run.
    pub fn into_parts(self) -> (Vec<Edge>, Option<GridRoutingInstrumentation>) {
        (self.layout.edges, self.metrics)
    }

    pub fn route(&mut self) -> Result<(), GridRoutingError> {
        if self.routed {
            return Err(GridRoutingError::geometry(
                "Grid edge routing session can only be run once",
            ));
        }
        self.routed = true;
        self.prepare()?;

        // Pair insertion order follows the already canonical plan order; retaining that order when
        // bundles are routed as indivisible retry units is what makes output deterministic.
        let mut pair_order: Vec<String> = Vec::new();
        let mut plans_by_pair: HashMap<String, Vec<EdgeRoutePlan>> = HashMap::new();
        for plan in &self.prepared.ordered_plans {
            if !plans_by_pair.contains_key(&plan.pair_key) {
                pair_order.push(plan.pair_key.clone());
            }
            plans_by_pair
                .entry(plan.pair_key.clone())
                .or_default()
                .push(plan.clone());
        }
        for pair_key in pair_order {
            let pair_plans = plans_by_pair
                .remove(&pair_key)
                .expect("pair order came from the map");
            self.route_bundle(&pair_plans)?;
        }
        Ok(())
    }

    fn prepare(&mut self) -> Result<(), GridRoutingError> {
        // Preparation is front-loaded so routing attempts never observe partially built planning or
        // topology state.
        root_container_meta(&mut self.result);
        let prepared =
            prepare_edge_routes(&self.layout, &self.result).map_err(GridRoutingError::Grid)?;
        let modes = prepare_routing_modes(
            &prepared,
            &self.result,
            &self.layout.nodes,
            self.metrics.as_mut(),
            self.options,
        )?;
        let routing_context = build_routing_context(
            &modes.routed_container_ids,
            &self.result,
            &self.layout.nodes,
            self.metrics.as_mut(),
            self.options,
        )?;
        let endpoint_overlay_scratch = routing_context
            .topologies
            .iter()
            .map(|(container_id, topology)| {
                (container_id.clone(), EndpointOverlayScratch::new(topology))
            })
            .collect();
        self.instrumented_routes = if self.metrics.is_some() {
            Some(Vec::new())
        } else {
            None
        };
        self.prepared = prepared;
        self.modes = modes;
        self.routing_context = routing_context;
        self.endpoint_overlay_scratch = endpoint_overlay_scratch;
        let mut counted_demand_keys: HashSet<String> = HashSet::new();
        for plan in &self.prepared.ordered_plans {
            if plan.edge.start == plan.edge.end {
                continue;
            }
            for entry in plan.source.chain.iter().chain(plan.target.chain.iter()) {
                if !counted_demand_keys.insert(entry.demand_key.clone()) {
                    continue;
                }
                let key = owner_side_key(&entry.owner_id, entry.side);
                *self.owner_side_counts.entry(key).or_insert(0) += 1;
            }
        }
        Ok(())
    }

    fn paired_portal(
        &mut self,
        entry: &EdgeEndpointEntry,
    ) -> Result<PairedPortal, GridRoutingError> {
        if let Some(existing) = self.paired_portals.get(&entry.demand_key) {
            return Ok(existing.clone());
        }
        let Some(owner_index) = self.result.forest.node_by_id.get(&entry.owner_id).copied() else {
            return Err(route_not_found(format!(
                "Missing portal owner \"{}\"",
                entry.owner_id
            )));
        };
        let owner = self.layout.nodes[owner_index].clone();
        if !owner.is_group {
            return Err(route_not_found(format!(
                "Missing portal owner \"{}\"",
                entry.owner_id
            )));
        }
        let rect = router_rect(&owner);
        let coordinate = self
            .prepared
            .demand_coords
            .get(&entry.demand_key)
            .copied()
            .unwrap_or(
                if entry.side == GridSide::Left || entry.side == GridSide::Right {
                    (rect.top + rect.bottom) / 2.0
                } else {
                    (rect.left + rect.right) / 2.0
                },
            );
        let title = group_title_rect(&owner);
        let range = derive_portal_ranges(&entry.owner_id, rect, title)?
            .into_iter()
            .find(|range| range.side == entry.side);
        let Some(range) = range else {
            return Err(route_not_found(format!(
                "No legal {} portal for \"{}\"",
                entry.side.as_str(),
                entry.owner_id
            )));
        };
        let portal = build_paired_portal(
            &entry.owner_id,
            rect,
            title,
            entry.side,
            clamp(coordinate, range.low, range.high),
        )?;
        self.paired_portals
            .insert(entry.demand_key.clone(), portal.clone());
        if let Some(metrics) = self.metrics.as_mut() {
            metrics.hierarchy_portal_pairs += 1;
            metrics.hierarchy_portal_transition_length += portal.transition.length;
        }
        Ok(portal)
    }

    fn item_segment_attachment(
        &self,
        entry: &EdgeEndpointEntry,
        container_id: &GridContainerId,
    ) -> Result<SegmentAttachment, GridRoutingError> {
        let attachment = item_attachment(
            &entry.owner_id,
            entry.side,
            &entry.demand_key,
            container_id,
            &self.result,
            &self.layout.nodes,
            &self.prepared.demand_coords,
            None,
        )?;
        Ok(SegmentAttachment {
            owner_id: entry.owner_id.clone(),
            port: attachment.port,
            connect: attachment.connect,
            orientation: attachment.orientation,
            side: attachment.side,
        })
    }

    fn apply_selections(&mut self, selections: Vec<PortalSelection>) {
        for selection in selections {
            self.paired_portals
                .insert(selection.demand_key.clone(), selection.portal);
            self.prepared
                .demand_coords
                .insert(selection.demand_key, selection.coordinate);
        }
    }

    fn commit_edge(
        &mut self,
        edge_id: &str,
        points: &[RouterPoint],
    ) -> Result<(), GridRoutingError> {
        let Some(index) = self.edge_index_by_id.get(edge_id).copied() else {
            return Err(route_not_found(format!(
                "Missing endpoint for edge \"{edge_id}\""
            )));
        };
        let curve = self.result.config.curve;
        let corner_radius = self.result.config.edge_corner_radius;
        let edge = &mut self.layout.edges[index];
        edge.points = points.to_vec();
        edge.curve = Some(curve);
        edge.corner_radius = Some(corner_radius);
        Ok(())
    }

    fn occupancy_for(&mut self, container_id: &GridContainerId) -> &mut RouteOccupancyIndex {
        self.modes
            .route_occupancy
            .entry(container_id.clone())
            .or_default()
    }

    fn route_plan(
        &mut self,
        plan: &EdgeRoutePlan,
        allow_hierarchy_relaxation: bool,
    ) -> Result<(), GridRoutingError> {
        // A plan is routed in three phases: source ascent, one LCA-container segment, and reversed
        // target descent. The chains are committed only after the combined route satisfies pair
        // rules.
        let edge_id = plan.edge.id.clone();
        let source_index = plan
            .edge
            .start
            .as_deref()
            .and_then(|id| self.result.forest.node_by_id.get(id).copied());
        let target_index = plan
            .edge
            .end
            .as_deref()
            .and_then(|id| self.result.forest.node_by_id.get(id).copied());
        let (Some(source_index), Some(target_index)) = (source_index, target_index) else {
            return Err(GridRoutingError::Grid(grid_error(
                GridErrorCode::MissingEndpoint,
                format!("Missing endpoint for edge \"{edge_id}\""),
                Some(serde_json::json!({ "edgeId": edge_id })),
            )));
        };
        let source_node = self.layout.nodes[source_index].clone();
        let target_node = self.layout.nodes[target_index].clone();

        if source_node.id == target_node.id {
            let container_id = source_node
                .parent_id
                .clone()
                .unwrap_or_else(|| ROOT_CONTAINER_ID.to_string());
            let pair_routes_snapshot = self
                .pair_routes
                .get(&plan.pair_key)
                .cloned()
                .unwrap_or_default();
            let overlay_scratch = self.endpoint_overlay_scratch.get_mut(&container_id);
            let route = sparse_self_loop_route(
                plan,
                &source_node,
                &self.result,
                &self.layout.nodes,
                &mut self.routing_context,
                &mut self.metrics,
                &mut self.search_workspace,
                overlay_scratch,
                &self.owner_side_counts,
                &self.self_loop_counts,
                &pair_routes_snapshot,
                self.options,
            )?;
            let count_key = format!("{}:{}", source_node.id, route.side.as_str());
            self.self_loop_counts.insert(count_key, route.index + 1);
            self.commit_edge(&edge_id, &route.points)?;
            self.pair_routes
                .entry(plan.pair_key.clone())
                .or_default()
                .push(route.points.clone());
            if let (Some(metrics), Some(routes)) =
                (self.metrics.as_mut(), self.instrumented_routes.as_mut())
            {
                record_grid_route(
                    metrics,
                    &edge_id,
                    &route.points,
                    routes,
                    0,
                    plan.lane_offset,
                );
                routes.push(route.points);
            }
            return Ok(());
        }

        let Some(source_final) = plan.source.chain.last() else {
            return Err(route_not_found(format!(
                "Missing endpoint for edge \"{edge_id}\""
            )));
        };
        let Some(target_final) = plan.target.chain.last() else {
            return Err(route_not_found(format!(
                "Missing endpoint for edge \"{edge_id}\""
            )));
        };
        let source_final = source_final.clone();
        let target_final = target_final.clone();
        let use_sparse_lca = self.modes.sparse_lca_ids.contains(&edge_id);
        let compatibility_lca = if use_sparse_lca {
            None
        } else {
            Some(compatibility_lca_attachments(
                plan,
                &self.result,
                &self.layout.nodes,
                &self.prepared.demand_coords,
            )?)
        };
        let lca_start = match &compatibility_lca {
            Some(compatibility) => compatibility.0.clone(),
            None => self.lca_attachment(&plan.source, &source_final, &plan.lca_container_id)?,
        };
        let lca_end = match &compatibility_lca {
            Some(compatibility) => compatibility.1.clone(),
            None => self.lca_attachment(&plan.target, &target_final, &plan.lca_container_id)?,
        };
        let compatibility_fast_route = self.modes.compatibility_fast_routes.get(&edge_id).cloned();
        let lca_points = if let Some(fast_route) = &compatibility_fast_route {
            if let Some(metrics) = self.metrics.as_mut() {
                metrics.compatibility_fast_paths += 1;
            }
            fast_route.clone()
        } else if self.prepared.eligible_ids.contains(&edge_id) {
            let candidates = self
                .prepared
                .endpoint_candidates_by_edge
                .get(&edge_id)
                .cloned()
                .ok_or_else(|| {
                    route_not_found(format!("Missing endpoint for edge \"{edge_id}\""))
                })?;
            let pair_routes_snapshot = self
                .pair_routes
                .get(&plan.pair_key)
                .cloned()
                .unwrap_or_default();
            self.modes
                .route_occupancy
                .entry(plan.lca_container_id.clone())
                .or_default();
            let occupied = self
                .modes
                .route_occupancy
                .get(&plan.lca_container_id)
                .map(|index| SparseOccupied {
                    index,
                    pair_key: &plan.pair_key,
                    prioritize_endpoint_candidates: self
                        .prepared
                        .coordinated_endpoint_ids
                        .contains(&edge_id),
                });
            let overlay_scratch = self
                .endpoint_overlay_scratch
                .get_mut(&plan.lca_container_id)
                .ok_or_else(|| {
                    route_not_found(format!(
                        "Missing routing topology \"{}\"",
                        plan.lca_container_id
                    ))
                })?;
            let mut legacy_lca_points: Option<Vec<RouterPoint>> = None;
            let mut legacy_route = || -> Result<Vec<RouterPoint>, GridRoutingError> {
                if let Some(points) = &legacy_lca_points {
                    return Ok(points.clone());
                }
                let points = route_within_container(
                    &plan.lca_container_id,
                    &self.result,
                    &lca_start.as_attachment(),
                    &lca_end.as_attachment(),
                    plan.lane_index,
                )?;
                legacy_lca_points = Some(points.clone());
                Ok(points)
            };
            sparse_same_container_route(
                plan,
                &candidates.sources,
                &candidates.targets,
                &source_node,
                &target_node,
                &mut legacy_route,
                &self.result,
                &self.layout.nodes,
                &mut self.routing_context,
                &mut self.metrics,
                &mut self.search_workspace,
                overlay_scratch,
                self.options,
                &pair_routes_snapshot,
                occupied.as_ref(),
            )?
        } else if use_sparse_lca {
            let pair_routes_snapshot = self
                .pair_routes
                .get(&plan.pair_key)
                .cloned()
                .unwrap_or_default();
            self.occupancy_for(&plan.lca_container_id);
            let occupied = if self.modes.overlap_avoidance_enabled {
                self.modes
                    .route_occupancy
                    .get(&plan.lca_container_id)
                    .map(|index| SparseOccupied {
                        index,
                        pair_key: &plan.pair_key,
                        prioritize_endpoint_candidates: false,
                    })
            } else {
                None
            };
            let (points, selections) = {
                let mut end_alternatives = || {
                    if plan.target.final_kind
                        == crate::layout::grid::router_planning::EndpointFinalKind::Item
                        && plan.target.chain.len() > 1
                    {
                        alternative_paired_portal_attachments(
                            &self.result,
                            &self.layout.nodes,
                            &self.prepared,
                            &self.paired_portals,
                            &target_final,
                            false,
                        )
                    } else {
                        Vec::new()
                    }
                };
                let mut start_alternatives = || {
                    if plan.source.final_kind
                        == crate::layout::grid::router_planning::EndpointFinalKind::Item
                        && plan.source.chain.len() > 1
                    {
                        alternative_paired_portal_attachments(
                            &self.result,
                            &self.layout.nodes,
                            &self.prepared,
                            &self.paired_portals,
                            &source_final,
                            false,
                        )
                    } else {
                        Vec::new()
                    }
                };
                let mut lca_legacy = || {
                    route_within_container(
                        &plan.lca_container_id,
                        &self.result,
                        &lca_start.as_attachment(),
                        &lca_end.as_attachment(),
                        plan.lane_index,
                    )
                };
                let overlay_scratch = self
                    .endpoint_overlay_scratch
                    .get_mut(&plan.lca_container_id);
                let segment = sparse_container_segment(
                    &edge_id,
                    &plan.lca_container_id,
                    &lca_start,
                    &lca_end,
                    &mut lca_legacy,
                    &self.result,
                    &self.layout.nodes,
                    &mut self.routing_context,
                    &mut self.metrics,
                    &mut self.search_workspace,
                    overlay_scratch,
                    self.options,
                    &pair_routes_snapshot,
                    &mut end_alternatives,
                    &mut start_alternatives,
                    plan.bundle_size > 1,
                    occupied.as_ref(),
                )?;
                (segment.points, segment.selections)
            };
            self.apply_selections(selections);
            points
        } else {
            let points = route_within_container(
                &plan.lca_container_id,
                &self.result,
                &lca_start.as_attachment(),
                &lca_end.as_attachment(),
                plan.lane_index,
            )?;
            validated_compatibility_segment(
                &edge_id,
                &plan.lca_container_id,
                &lca_start,
                &lca_end,
                || points,
                &self.result,
                &self.layout.nodes,
                self.metrics.as_mut(),
            )?
        };

        let mut source_chains: Vec<Vec<RouterPoint>> = Vec::new();
        let mut target_chains: Vec<Vec<RouterPoint>> = Vec::new();
        self.route_hierarchy_chain(plan, &plan.source, &mut source_chains)?;
        self.route_hierarchy_chain(plan, &plan.target, &mut target_chains)?;

        let mut chains: Vec<Vec<RouterPoint>> = source_chains;
        chains.push(lca_points.clone());
        for chain in target_chains.iter().rev() {
            chains.push(reverse_points(chain));
        }
        let points = combine_point_chains(&chains);
        let committed_pair_routes = self
            .pair_routes
            .get(&plan.pair_key)
            .cloned()
            .unwrap_or_default();
        let satisfies_pair_constraints =
            route_satisfies_pair_constraints(&points, &committed_pair_routes);
        let relax_hierarchy_separation = !satisfies_pair_constraints
            && plan.bundle_size > 1
            && plan.source.chain.len() + plan.target.chain.len() > 2
            && route_has_distinct_pair_ports(&points, &committed_pair_routes);
        if plan.bundle_size > 1
            && !satisfies_pair_constraints
            && (!relax_hierarchy_separation || !allow_hierarchy_relaxation)
        {
            if let Some(metrics) = self.metrics.as_mut() {
                metrics.routes_impossible += 1;
            }
            let reason = if relax_hierarchy_separation {
                "pair-separation"
            } else {
                "no-distinct-route"
            };
            return Err(GridRoutingError::Grid(grid_error(
                GridErrorCode::RouteNotFound,
                format!("No distinct lane route for \"{edge_id}\""),
                Some(serde_json::json!({ "edgeId": edge_id, "reason": reason })),
            )));
        }
        if relax_hierarchy_separation && let Some(metrics) = self.metrics.as_mut() {
            metrics.bundle_separation_relaxations += 1;
        }
        self.commit_edge(&edge_id, &points)?;
        self.pair_routes
            .entry(plan.pair_key.clone())
            .or_default()
            .push(points.clone());
        if compatibility_fast_route.is_none() {
            self.occupancy_for(&plan.lca_container_id)
                .add(&lca_points, &plan.pair_key);
        }
        if let (Some(metrics), Some(routes)) =
            (self.metrics.as_mut(), self.instrumented_routes.as_mut())
        {
            let boundary_transition_count = plan.source.chain.len() + plan.target.chain.len() - 2;
            record_grid_route(
                metrics,
                &edge_id,
                &points,
                routes,
                boundary_transition_count,
                plan.lane_offset,
            );
            routes.push(points);
        }
        Ok(())
    }

    fn lca_attachment(
        &mut self,
        endpoint: &EdgeEndpointPlan,
        final_entry: &EdgeEndpointEntry,
        lca_container_id: &GridContainerId,
    ) -> Result<SegmentAttachment, GridRoutingError> {
        if endpoint.final_kind == crate::layout::grid::router_planning::EndpointFinalKind::Boundary
        {
            Ok(group_boundary_endpoint_attachment(
                &final_entry.owner_id,
                final_entry.side,
                &final_entry.demand_key,
                &self.result,
                &self.layout.nodes,
                &self.prepared.demand_coords,
            )?)
        } else if endpoint.chain.len() > 1 {
            Ok(portal_attachment(&self.paired_portal(final_entry)?, false))
        } else {
            self.item_segment_attachment(final_entry, lca_container_id)
        }
    }

    fn route_hierarchy_chain(
        &mut self,
        plan: &EdgeRoutePlan,
        endpoint: &EdgeEndpointPlan,
        chains: &mut Vec<Vec<RouterPoint>>,
    ) -> Result<(), GridRoutingError> {
        if !self.modes.sparse_hierarchy_ids.contains(&plan.edge.id) {
            let attachments = compatibility_hierarchy_attachments(
                endpoint,
                &self.result,
                &self.layout.nodes,
                &self.prepared.demand_coords,
            )?;
            for attachment in attachments {
                let points = route_within_container(
                    &attachment.container_id,
                    &self.result,
                    &attachment.start.as_attachment(),
                    &attachment.end.as_attachment(),
                    plan.lane_index,
                )?;
                chains.push(validated_compatibility_segment(
                    &plan.edge.id,
                    &attachment.container_id,
                    &attachment.start,
                    &attachment.end,
                    || points,
                    &self.result,
                    &self.layout.nodes,
                    self.metrics.as_mut(),
                )?);
            }
            return Ok(());
        }
        for index in 0..endpoint.chain.len().saturating_sub(1) {
            let from = endpoint.chain[index].clone();
            let to = endpoint.chain[index + 1].clone();
            let start = if index == 0 {
                self.item_segment_attachment(&from, &to.owner_id)?
            } else {
                portal_attachment(&self.paired_portal(&from)?, false)
            };
            let end = portal_attachment(&self.paired_portal(&to)?, true);
            let pair_routes_snapshot = self
                .pair_routes
                .get(&plan.pair_key)
                .cloned()
                .unwrap_or_default();
            self.occupancy_for(&to.owner_id);
            let occupied = if self.modes.overlap_avoidance_enabled {
                self.modes
                    .route_occupancy
                    .get(&to.owner_id)
                    .map(|index| SparseOccupied {
                        index,
                        pair_key: &plan.pair_key,
                        prioritize_endpoint_candidates: false,
                    })
            } else {
                None
            };
            let (points, selections) = {
                let mut end_alternatives = || {
                    alternative_paired_portal_attachments(
                        &self.result,
                        &self.layout.nodes,
                        &self.prepared,
                        &self.paired_portals,
                        &to,
                        true,
                    )
                };
                let mut start_alternatives = || {
                    if index == 0 {
                        alternative_item_attachments(
                            &self.result,
                            &self.layout.nodes,
                            &self.prepared,
                            plan,
                            &from,
                            &to.owner_id,
                        )
                        .unwrap_or_default()
                    } else {
                        Vec::new()
                    }
                };
                let mut legacy_route = || {
                    route_within_container(
                        &to.owner_id,
                        &self.result,
                        &start.as_attachment(),
                        &end.as_attachment(),
                        plan.lane_index,
                    )
                };
                let overlay_scratch = self.endpoint_overlay_scratch.get_mut(&to.owner_id);
                let segment = sparse_container_segment(
                    &plan.edge.id,
                    &to.owner_id,
                    &start,
                    &end,
                    &mut legacy_route,
                    &self.result,
                    &self.layout.nodes,
                    &mut self.routing_context,
                    &mut self.metrics,
                    &mut self.search_workspace,
                    overlay_scratch,
                    self.options,
                    &pair_routes_snapshot,
                    &mut end_alternatives,
                    &mut start_alternatives,
                    plan.bundle_size > 1,
                    occupied.as_ref(),
                )?;
                (segment.points, segment.selections)
            };
            self.apply_selections(selections);
            chains.push(points);
        }
        Ok(())
    }

    fn create_bundle_checkpoint(&mut self, pair_plans: &[EdgeRoutePlan]) -> BundleCheckpoint {
        // Snapshot only state a failed bundle can mutate. Topologies and prepared plans are
        // immutable and intentionally shared across attempts.
        let pair_key = pair_plans[0].pair_key.clone();
        let lca_container_id = pair_plans[0].lca_container_id.clone();
        let occupancy_size = self.occupancy_for(&lca_container_id).size();
        let committed_pair_routes = self.pair_routes.get(&pair_key).cloned();
        let mut demand_keys: Vec<String> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for plan in pair_plans {
            for entry in plan.source.chain.iter().chain(plan.target.chain.iter()) {
                if seen.insert(entry.demand_key.clone()) {
                    demand_keys.push(entry.demand_key.clone());
                }
            }
        }
        let mut paired_portals = HashMap::new();
        let mut demand_coords = HashMap::new();
        for demand_key in &demand_keys {
            if let Some(portal) = self.paired_portals.get(demand_key) {
                paired_portals.insert(demand_key.clone(), portal.clone());
            }
            if let Some(coordinate) = self.prepared.demand_coords.get(demand_key) {
                demand_coords.insert(demand_key.clone(), *coordinate);
            }
        }
        let mut edges = Vec::new();
        for plan in pair_plans {
            if let Some(index) = self.edge_index_by_id.get(&plan.edge.id).copied() {
                let edge = &self.layout.edges[index];
                edges.push(BundleEdgeCheckpoint {
                    edge_id: edge.id.clone(),
                    points: edge.points.clone(),
                    curve: edge.curve,
                    corner_radius: edge.corner_radius,
                });
            }
        }
        BundleCheckpoint {
            pair_key,
            occupancy: Some((lca_container_id, occupancy_size)),
            pair_routes: committed_pair_routes,
            demand_keys,
            paired_portals,
            demand_coords,
            edges,
            instrumented_routes_length: self.instrumented_routes.as_ref().map(Vec::len),
            metrics: self
                .metrics
                .as_ref()
                .map(create_grid_routing_instrumentation_checkpoint),
        }
    }

    fn restore_bundle_checkpoint(&mut self, checkpoint: BundleCheckpoint) {
        if let Some((container_id, size)) = checkpoint.occupancy {
            self.occupancy_for(&container_id).truncate(size);
        }
        match checkpoint.pair_routes {
            Some(pair_routes) => {
                self.pair_routes.insert(checkpoint.pair_key, pair_routes);
            }
            None => {
                self.pair_routes.remove(&checkpoint.pair_key);
            }
        }
        for demand_key in checkpoint.demand_keys {
            match checkpoint.paired_portals.get(&demand_key) {
                Some(portal) => {
                    self.paired_portals
                        .insert(demand_key.clone(), portal.clone());
                }
                None => {
                    self.paired_portals.remove(&demand_key);
                }
            }
            match checkpoint.demand_coords.get(&demand_key) {
                Some(coordinate) => {
                    self.prepared
                        .demand_coords
                        .insert(demand_key.clone(), *coordinate);
                }
                None => {
                    self.prepared.demand_coords.remove(&demand_key);
                }
            }
        }
        for edge_checkpoint in checkpoint.edges {
            if let Some(index) = self.edge_index_by_id.get(&edge_checkpoint.edge_id).copied() {
                let edge = &mut self.layout.edges[index];
                edge.points = edge_checkpoint.points;
                edge.curve = edge_checkpoint.curve;
                edge.corner_radius = edge_checkpoint.corner_radius;
            }
        }
        if let (Some(routes), Some(length)) = (
            self.instrumented_routes.as_mut(),
            checkpoint.instrumented_routes_length,
        ) {
            routes.truncate(length);
        }
        if let (Some(metrics), Some(metrics_checkpoint)) =
            (self.metrics.as_mut(), checkpoint.metrics)
        {
            restore_grid_routing_instrumentation_checkpoint(metrics, metrics_checkpoint);
        }
    }

    fn route_bundle(&mut self, pair_plans: &[EdgeRoutePlan]) -> Result<(), GridRoutingError> {
        // Small bundles first require strict pair separation. If that fails only because hierarchy
        // interiors overlap, restore the whole bundle and retry with that separation relaxed while
        // preserving distinct endpoint ports.
        let can_attempt_strict_separation = pair_plans.len() > 1
            && pair_plans.len() <= 8
            && pair_plans
                .iter()
                .all(|plan| plan.edge.start != plan.edge.end);
        if !can_attempt_strict_separation {
            for plan in pair_plans {
                self.route_plan(plan, true)?;
            }
            return Ok(());
        }
        // A bundle is the retry unit because earlier siblings reserve pair corridors and portals for
        // later ones. Restore every transactional structure before relaxing separation. The
        // cumulative search budget intentionally retains work performed by the discarded attempt.
        let checkpoint = self.create_bundle_checkpoint(pair_plans);
        let mut initial_error: Option<GridRoutingError> = None;
        for plan in pair_plans {
            if let Err(error) = self.route_plan(plan, false) {
                initial_error = Some(error);
                break;
            }
        }
        let Some(initial_error) = initial_error else {
            return Ok(());
        };
        self.restore_bundle_checkpoint(checkpoint);

        if !is_pair_separation_error(&initial_error) {
            return Err(initial_error);
        }
        for plan in pair_plans {
            self.route_plan(plan, true)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::grid::geometry::{normalize_polyline, rect_for_node};
    use crate::layout::grid::layout_core::{GridLayoutCoreInternalOptions, run_grid_layout_core};
    use crate::layout::grid::types::{Edge, GridConfig, LabelSize};
    use serde_json::json;

    fn leaf(
        id: &str,
        width: f64,
        height: f64,
        metadata: serde_json::Value,
        parent: Option<&str>,
    ) -> Node {
        Node {
            id: id.to_string(),
            parent_id: parent.map(str::to_string),
            is_group: false,
            width: Some(width),
            height: Some(height),
            metadata: Some(metadata),
            ..Node::default()
        }
    }

    fn group(id: &str, label: &str, metadata: serde_json::Value) -> Node {
        Node {
            id: id.to_string(),
            label: Some(label.to_string()),
            label_bbox: Some(LabelSize {
                width: 70.0,
                height: 20.0,
            }),
            is_group: true,
            metadata: Some(metadata),
            ..Node::default()
        }
    }

    fn edge(id: &str, start: &str, end: &str) -> Edge {
        Edge {
            id: id.to_string(),
            start: Some(start.to_string()),
            end: Some(end.to_string()),
            ..Edge::default()
        }
    }

    fn base_layout(nodes: Vec<Node>, edges: Vec<Edge>, grid: serde_json::Value) -> LayoutData {
        LayoutData {
            nodes,
            edges,
            config: GridConfig { grid: Some(grid) },
        }
    }

    fn points_of(data: &LayoutData, id: &str) -> Vec<RouterPoint> {
        normalize_polyline(
            &data
                .edges
                .iter()
                .find(|edge| edge.id == id)
                .expect("edge exists")
                .points,
        )
        .points
    }

    #[test]
    fn keeps_an_aligned_edge_straight_and_routes_a_diagonal_edge_into_a_separate_endpoint_side() {
        let create_layout = |edges: Vec<Edge>| {
            base_layout(
                vec![
                    leaf("A", 80.0, 40.0, json!({ "row": 1, "column": 1 }), None),
                    leaf("C", 80.0, 40.0, json!({ "row": 2, "column": 1 }), None),
                    leaf("D", 80.0, 40.0, json!({ "row": 1, "column": 3 }), None),
                ],
                edges,
                json!({ "columns": 3, "rowGap": 45, "columnGap": 55 }),
            )
        };
        let mut data = create_layout(vec![edge("A-D", "A", "D"), edge("C-D", "C", "D")]);
        run_grid_layout_core(&mut data, GridLayoutCoreInternalOptions::default())
            .expect("layout runs");

        let aligned_points = points_of(&data, "A-D");
        let diagonal_points = points_of(&data, "C-D");
        let destination = data
            .nodes
            .iter()
            .find(|node| node.id == "D")
            .expect("destination");
        let destination_bounds = rect_for_node(destination);

        assert!(
            aligned_points
                .iter()
                .all(|point| point.y == aligned_points[0].y)
        );
        assert_eq!(
            diagonal_points.last().expect("diagonal endpoint").y,
            destination_bounds.bottom
        );
        assert!(diagonal_points[diagonal_points.len() - 2].y > destination_bounds.bottom);

        let mut reversed = create_layout(vec![edge("C-D", "C", "D"), edge("A-D", "A", "D")]);
        run_grid_layout_core(&mut reversed, GridLayoutCoreInternalOptions::default())
            .expect("layout runs");
        let points_by_edge = |data: &LayoutData| {
            let mut entries: Vec<(String, Vec<RouterPoint>)> = data
                .edges
                .iter()
                .map(|edge| (edge.id.clone(), edge.points.clone()))
                .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            entries
        };
        assert_eq!(points_by_edge(&reversed), points_by_edge(&data));
    }

    #[test]
    fn routes_ordinary_edges_into_valid_orthogonal_polylines() {
        let mut data = base_layout(
            vec![
                group("g", "Group", json!({ "row": 1, "column": 1 })),
                leaf("a", 80.0, 40.0, json!({ "row": 1, "column": 1 }), Some("g")),
                leaf("b", 90.0, 40.0, json!({ "row": 1, "column": 2 }), None),
                leaf("c", 70.0, 40.0, json!({ "row": 2, "column": 2 }), None),
            ],
            vec![edge("e1", "a", "b"), edge("e3", "a", "c")],
            json!({ "rowGap": 40, "columnGap": 40 }),
        );

        run_grid_layout_core(&mut data, GridLayoutCoreInternalOptions::default())
            .expect("layout runs");

        for edge in &data.edges {
            assert!(edge.points.len() >= 2, "edge {} has route points", edge.id);
            assert!(
                edge.points
                    .iter()
                    .all(|point| point.x.is_finite() && point.y.is_finite())
            );
            for index in 1..edge.points.len() {
                assert!(
                    edge.points[index - 1].x == edge.points[index].x
                        || edge.points[index - 1].y == edge.points[index].y,
                    "edge {} is orthogonal",
                    edge.id
                );
            }
            assert!(edge.curve.is_some());
            assert!(edge.corner_radius.is_some());
        }
    }

    #[test]
    fn routes_a_leaf_to_group_edge_with_finite_orthogonal_points() {
        let mut data = base_layout(
            vec![
                group("g", "Group", json!({ "row": 1, "column": 1 })),
                leaf("a", 80.0, 40.0, json!({ "row": 1, "column": 1 }), Some("g")),
                leaf("b", 90.0, 40.0, json!({ "row": 1, "column": 2 }), None),
            ],
            vec![edge("e2", "b", "g")],
            json!({ "rowGap": 40, "columnGap": 40 }),
        );

        run_grid_layout_core(&mut data, GridLayoutCoreInternalOptions::default())
            .expect("layout runs");

        for edge in &data.edges {
            assert!(edge.points.len() >= 2);
            assert!(
                edge.points
                    .iter()
                    .all(|point| point.x.is_finite() && point.y.is_finite())
            );
            for index in 1..edge.points.len() {
                assert!(
                    edge.points[index - 1].x == edge.points[index].x
                        || edge.points[index - 1].y == edge.points[index].y
                );
            }
        }
    }

    #[test]
    fn allocates_distinct_ports_for_repeated_self_loops_on_the_same_node() {
        let mut data = base_layout(
            vec![leaf(
                "a",
                800.0,
                400.0,
                json!({ "row": 1, "column": 1 }),
                None,
            )],
            (0..5)
                .map(|index| edge(&format!("loop-{index}"), "a", "a"))
                .collect(),
            json!({}),
        );

        run_grid_layout_core(&mut data, GridLayoutCoreInternalOptions::default())
            .expect("layout runs");

        let routes: HashSet<String> = data
            .edges
            .iter()
            .map(|edge| {
                let points: Vec<(u64, u64)> = edge
                    .points
                    .iter()
                    .map(|point| (point.x.to_bits(), point.y.to_bits()))
                    .collect();
                serde_json::to_string(&points).expect("points serialize")
            })
            .collect();
        assert_eq!(routes.len(), 5);
    }

    fn longest_shared_nonterminal_subpath(first: &Edge, second: &Edge) -> f64 {
        let first_segments = normalize_polyline(&first.points).segments;
        let second_segments = normalize_polyline(&second.points).segments;
        let first_segments = &first_segments[1..first_segments.len().saturating_sub(1)];
        let second_segments = &second_segments[1..second_segments.len().saturating_sub(1)];
        let mut longest = 0.0_f64;
        for a in first_segments {
            for b in second_segments {
                if a.orientation != b.orientation {
                    continue;
                }
                if a.orientation == crate::layout::grid::geometry::SegmentOrientation::H
                    && a.a.y == b.a.y
                {
                    longest = longest.max(0.0_f64.max(
                        a.a.x.max(a.b.x).min(b.a.x.max(b.b.x))
                            - a.a.x.min(a.b.x).max(b.a.x.min(b.b.x)),
                    ));
                } else if a.orientation == crate::layout::grid::geometry::SegmentOrientation::V
                    && a.a.x == b.a.x
                {
                    longest = longest.max(0.0_f64.max(
                        a.a.y.max(a.b.y).min(b.a.y.max(b.b.y))
                            - a.a.y.min(a.b.y).max(b.a.y.min(b.b.y)),
                    ));
                }
            }
        }
        longest
    }

    #[test]
    fn keeps_reverse_pair_lanes_distinct_across_hierarchy_portals() {
        let mut data = base_layout(
            vec![
                group("left-group", "Left", json!({ "row": 1, "column": 1 })),
                leaf(
                    "a",
                    80.0,
                    40.0,
                    json!({ "row": 1, "column": 1 }),
                    Some("left-group"),
                ),
                group("right-group", "Right", json!({ "row": 1, "column": 2 })),
                leaf(
                    "b",
                    80.0,
                    40.0,
                    json!({ "row": 1, "column": 1 }),
                    Some("right-group"),
                ),
            ],
            vec![
                edge("e1", "a", "b"),
                edge("e2", "a", "b"),
                edge("e3", "b", "a"),
            ],
            json!({ "rowGap": 50, "columnGap": 90 }),
        );

        run_grid_layout_core(&mut data, GridLayoutCoreInternalOptions::default())
            .expect("layout runs");

        let routes: HashSet<String> = data
            .edges
            .iter()
            .map(|edge| {
                let points: Vec<(u64, u64)> = edge
                    .points
                    .iter()
                    .map(|point| (point.x.to_bits(), point.y.to_bits()))
                    .collect();
                serde_json::to_string(&points).expect("points serialize")
            })
            .collect();
        assert_eq!(routes.len(), 3);

        let a = data
            .nodes
            .iter()
            .find(|node| node.id == "a")
            .expect("node a");
        let mut a_ports: Vec<f64> = data
            .edges
            .iter()
            .map(|edge| {
                let point = if edge.start.as_deref() == Some("a") {
                    edge.points.first().copied()
                } else {
                    edge.points.last().copied()
                };
                point.map_or(0.0, |point| point.y - a.y.unwrap_or(0.0))
            })
            .collect();
        a_ports.sort_by(f64::total_cmp);
        assert_eq!(a_ports, vec![-8.0, 0.0, 8.0]);

        for first in 0..data.edges.len() {
            for second in (first + 1)..data.edges.len() {
                assert!(
                    longest_shared_nonterminal_subpath(&data.edges[first], &data.edges[second])
                        < 8.0
                );
            }
        }
    }

    #[test]
    fn routes_a_same_container_edge_through_the_sparse_search() {
        let mut data = base_layout(
            vec![
                leaf("A", 80.0, 40.0, json!({ "row": 1, "column": 1 }), None),
                leaf("B", 80.0, 40.0, json!({ "row": 2, "column": 2 }), None),
            ],
            vec![edge("A-B", "A", "B")],
            json!({ "rowGap": 45, "columnGap": 55 }),
        );
        let mut metrics =
            crate::layout::grid::router_instrumentation::create_grid_routing_instrumentation();
        run_grid_layout_core(
            &mut data,
            GridLayoutCoreInternalOptions {
                metrics: Some(&mut metrics),
                routing: GridRoutingTestOptions {
                    options: crate::layout::grid::router::GridRoutingOptions {
                        search_caps: Some(
                            crate::layout::grid::router_search::RouterSearchCaps::default(),
                        ),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            },
        )
        .expect("layout runs");

        let points = points_of(&data, "A-B");
        assert!(points.len() >= 2);
        assert!(
            points
                .iter()
                .all(|point| point.x.is_finite() && point.y.is_finite())
        );
        assert!(metrics.searches > 0);
        assert_eq!(metrics.routes_found, 1);
        assert_eq!(metrics.resource_limit_fallbacks, 0);
    }

    #[test]
    fn falls_back_to_the_validated_corridor_route_under_a_search_state_cap() {
        let mut data = base_layout(
            vec![
                leaf("A", 80.0, 40.0, json!({ "row": 1, "column": 1 }), None),
                leaf("B", 80.0, 40.0, json!({ "row": 1, "column": 2 }), None),
            ],
            vec![edge("A-B", "A", "B")],
            json!({ "rowGap": 45, "columnGap": 55 }),
        );
        let mut metrics =
            crate::layout::grid::router_instrumentation::create_grid_routing_instrumentation();
        run_grid_layout_core(
            &mut data,
            GridLayoutCoreInternalOptions {
                metrics: Some(&mut metrics),
                routing: GridRoutingTestOptions {
                    options: crate::layout::grid::router::GridRoutingOptions {
                        search_caps: Some(crate::layout::grid::router_search::RouterSearchCaps {
                            max_expanded_states: Some(0),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            },
        )
        .expect("layout runs");

        let points = points_of(&data, "A-B");
        assert!(points.len() >= 2);
        assert!(points.iter().all(|point| point.y == points[0].y));
        assert_eq!(
            metrics.reason_count(
                crate::layout::grid::router_instrumentation::GridRoutingFallbackReason::SearchStateCap
            ),
            1
        );
        assert_eq!(metrics.routes_found, 1);
    }

    #[test]
    fn reports_pair_separation_detail_through_the_error() {
        let error = GridRoutingError::Grid(grid_error(
            GridErrorCode::RouteNotFound,
            "No distinct lane route for \"a-b\"",
            Some(serde_json::json!({ "edgeId": "a-b", "reason": "pair-separation" })),
        ));
        assert!(is_pair_separation_error(&error));
    }
}
