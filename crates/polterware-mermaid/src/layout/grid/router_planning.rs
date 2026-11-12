//! Port of Mermaid 12.3.0's
//! `rendering-util/layout-algorithms/grid/routerPlanning.ts`.
//!
//! Planning is deterministic and side-effect free: it resolves hierarchy chains, bundle lanes,
//! endpoint demand, and route order before the routing session creates any topology.
//!
//! Node references become node indices into [`LayoutData::nodes`] plus a borrow of that slice;
//! upstream's `Map<Node, ...>` keyed by object identity becomes a tuple key of plan and endpoint
//! role, which is unique per edge endpoint.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use crate::layout::grid::geometry::{PIXEL_EPSILON, Rect, clamp, rect_for_node};
use crate::layout::grid::groups::is_ancestor_group;
use crate::layout::grid::router_topology::{
    EDGE_CLEARANCE_PX, ROUTE_CLEARANCE_PX, routing_point_key,
};
use crate::layout::grid::types::{
    Edge, GridAttachmentDemand, GridAttachmentInner, GridContainerId, GridErrorCode,
    GridLayoutResult, GridSide, LayoutData, Node, ROOT_CONTAINER_ID, RouterPoint,
    compare_code_units, grid_error,
};

const MIN_PORT_SEPARATION_PX: f64 = 4.0;
pub const TERMINAL_APPROACH_PX: f64 = 20.0;
pub const LANE_SEPARATION_PX: f64 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EndpointRole {
    Source,
    Target,
}

impl EndpointRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Target => "target",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndpointFinalKind {
    Item,
    Boundary,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EdgeEndpointEntry {
    pub owner_id: String,
    pub side: GridSide,
    pub demand_key: String,
    pub opposite_coord: f64,
    pub preferred_coord: f64,
    pub compact_portal: bool,
    pub ambiguous_sides: Option<Vec<GridSide>>,
    pub inner: Option<GridAttachmentInner>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EdgeEndpointPlan {
    // Entries walk from the endpoint outward toward the least common container. Each adjacent pair
    // represents one hierarchy boundary that the final route must cross through a paired portal.
    pub chain: Vec<EdgeEndpointEntry>,
    pub final_kind: EndpointFinalKind,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EdgeRoutePlan {
    pub edge: Edge,
    pub lca_container_id: GridContainerId,
    pub source: EdgeEndpointPlan,
    pub target: EdgeEndpointPlan,
    pub lane_index: usize,
    pub lane_offset: f64,
    pub bundle_size: usize,
    pub pair_key: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EdgeRouteCandidates {
    pub sources: Vec<EndpointCandidate>,
    pub targets: Vec<EndpointCandidate>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PreparedEdgeRoutes {
    pub plans: Vec<EdgeRoutePlan>,
    pub eligible_plans: Vec<EdgeRoutePlan>,
    pub eligible_ids: HashSet<String>,
    pub ordered_plans: Vec<EdgeRoutePlan>,
    pub demand_coords: HashMap<String, f64>,
    pub endpoint_candidates_by_edge: HashMap<String, EdgeRouteCandidates>,
    pub endpoint_incident_counts: HashMap<String, usize>,
    pub coordinated_endpoint_ids: HashSet<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EndpointCandidate {
    pub owner_id: String,
    pub side: GridSide,
    pub port: RouterPoint,
    pub connect: RouterPoint,
    pub rank: f64,
    // Set only at owners that mix aligned and unaligned edges: whether the candidate uses the side
    // the allocator assigned. The sparse search keeps those owners on their assigned side.
    pub on_assigned_side: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EndpointSlotAssignment {
    pub side: GridSide,
    pub coordinate: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EndpointDemandEntry {
    plan_index: usize,
    role: EndpointRole,
    opposite_index: usize,
}

pub fn owner_group_title(node: &Node) -> bool {
    node.group_title_rect.is_some()
}

/// JSON tuple used as a stable key, matching upstream's `JSON.stringify([ownerId, side])`.
pub fn owner_side_key(owner_id: &str, side: GridSide) -> String {
    serde_json::json!([owner_id, side.as_str()]).to_string()
}

fn demand_key(edge_id: &str, role: EndpointRole, owner_id: &str, side: GridSide) -> String {
    serde_json::json!([edge_id, role.as_str(), owner_id, side.as_str()]).to_string()
}

fn node<'a>(nodes: &'a [Node], result: &GridLayoutResult, id: &str) -> Option<&'a Node> {
    result
        .forest
        .node_by_id
        .get(id)
        .and_then(|index| nodes.get(*index))
}

fn node_index(result: &GridLayoutResult, id: &str) -> Option<usize> {
    result.forest.node_by_id.get(id).copied()
}

fn preferred_side(node: &Node, toward: RouterPoint) -> GridSide {
    let rect = rect_for_node(node);
    let dx = toward.x - rect.cx;
    let dy = toward.y - rect.cy;
    let avoid_top = node.is_group && owner_group_title(node);

    if dx.abs() >= dy.abs() {
        return if dx >= 0.0 {
            GridSide::Right
        } else {
            GridSide::Left
        };
    }
    if dy >= 0.0 {
        return GridSide::Bottom;
    }
    if avoid_top {
        return if dx >= 0.0 {
            GridSide::Right
        } else {
            GridSide::Left
        };
    }
    GridSide::Top
}

fn container_chain(
    node_index: usize,
    nodes: &[Node],
    node_by_id: &HashMap<String, usize>,
) -> Vec<GridContainerId> {
    let mut chain = Vec::new();
    let mut current_parent = nodes[node_index]
        .parent_id
        .clone()
        .unwrap_or_else(|| ROOT_CONTAINER_ID.to_string());
    let mut seen = HashSet::new();
    loop {
        chain.push(current_parent.clone());
        if current_parent == ROOT_CONTAINER_ID {
            break;
        }
        if !seen.insert(current_parent.clone()) {
            break;
        }
        current_parent = node_by_id
            .get(&current_parent)
            .and_then(|index| nodes[*index].parent_id.clone())
            .unwrap_or_else(|| ROOT_CONTAINER_ID.to_string());
    }
    chain
}

fn common_container_id(
    source_index: usize,
    target_index: usize,
    nodes: &[Node],
    node_by_id: &HashMap<String, usize>,
) -> GridContainerId {
    let source = &nodes[source_index];
    let target = &nodes[target_index];
    if source.is_group && is_ancestor_group(&source.id, Some(target_index), nodes, node_by_id) {
        return source
            .parent_id
            .clone()
            .unwrap_or_else(|| ROOT_CONTAINER_ID.to_string());
    }
    if target.is_group && is_ancestor_group(&target.id, Some(source_index), nodes, node_by_id) {
        return target
            .parent_id
            .clone()
            .unwrap_or_else(|| ROOT_CONTAINER_ID.to_string());
    }
    let target_containers: HashSet<GridContainerId> =
        container_chain(target_index, nodes, node_by_id)
            .into_iter()
            .collect();
    for container_id in container_chain(source_index, nodes, node_by_id) {
        if target_containers.contains(&container_id) {
            return container_id;
        }
    }
    ROOT_CONTAINER_ID.to_string()
}

fn opposite_coord_for(rect: Rect, side: GridSide) -> f64 {
    if side == GridSide::Left || side == GridSide::Right {
        rect.cy
    } else {
        rect.cx
    }
}

fn side_midpoint(rect: Rect, side: GridSide) -> RouterPoint {
    if side == GridSide::Left || side == GridSide::Right {
        RouterPoint {
            x: if side == GridSide::Left {
                rect.left
            } else {
                rect.right
            },
            y: rect.cy,
        }
    } else {
        RouterPoint {
            x: rect.cx,
            y: if side == GridSide::Top {
                rect.top
            } else {
                rect.bottom
            },
        }
    }
}

fn tangential_coordinate(point: RouterPoint, side: GridSide) -> f64 {
    if side == GridSide::Left || side == GridSide::Right {
        point.y
    } else {
        point.x
    }
}

fn build_endpoint_plan(
    endpoint_index: usize,
    other_index: usize,
    lca_container_id: &str,
    nodes: &[Node],
    node_by_id: &HashMap<String, usize>,
    edge_id: &str,
    role: EndpointRole,
) -> Result<EdgeEndpointPlan, crate::layout::grid::types::GridError> {
    let mut chain: Vec<EdgeEndpointEntry> = Vec::new();
    let endpoint_rect = rect_for_node(&nodes[endpoint_index]);
    let other_rect = rect_for_node(&nodes[other_index]);
    let other_center = RouterPoint {
        x: nodes[other_index].x.unwrap_or(0.0),
        y: nodes[other_index].y.unwrap_or(0.0),
    };
    let mut current_index = endpoint_index;

    loop {
        let current = &nodes[current_index];
        let parent_id = current
            .parent_id
            .clone()
            .unwrap_or_else(|| ROOT_CONTAINER_ID.to_string());
        let parent = node_by_id.get(&parent_id).map(|index| &nodes[*index]);
        let current_center = RouterPoint {
            x: current.x.unwrap_or(0.0),
            y: current.y.unwrap_or(0.0),
        };
        let dx = other_center.x - current_center.x;
        let dy = other_center.y - current_center.y;
        let current_title = if current.is_group {
            current.group_title_rect
        } else {
            None
        };
        let parent_title = parent.and_then(|parent| parent.group_title_rect);
        let title_avoidance_tie =
            dy < 0.0 && dx.abs() <= PIXEL_EPSILON && current_title.or(parent_title).is_some();
        let mut direct_side_candidates: Vec<GridSide> = Vec::new();
        if (dx.abs() - dy.abs()).abs() <= PIXEL_EPSILON && dx.abs() > PIXEL_EPSILON {
            direct_side_candidates.push(if dx >= 0.0 {
                GridSide::Right
            } else {
                GridSide::Left
            });
            direct_side_candidates.push(if dy >= 0.0 {
                GridSide::Bottom
            } else {
                GridSide::Top
            });
        }
        let top_blocked = current_title.or(parent_title).is_some();
        let ambiguous_candidates = if title_avoidance_tie {
            vec![GridSide::Left, GridSide::Right]
        } else {
            direct_side_candidates
        };
        let ambiguous_sides: Vec<GridSide> = ambiguous_candidates
            .into_iter()
            .filter(|candidate| *candidate != GridSide::Top || !top_blocked)
            .collect();
        let mut side = preferred_side(current, other_center);
        if side == GridSide::Top && parent_title.is_some() {
            side = if dx >= 0.0 {
                GridSide::Right
            } else {
                GridSide::Left
            };
        }
        chain.push(EdgeEndpointEntry {
            owner_id: current.id.clone(),
            side,
            demand_key: demand_key(edge_id, role, &current.id, side),
            opposite_coord: opposite_coord_for(other_rect, side),
            preferred_coord: opposite_coord_for(endpoint_rect, side),
            compact_portal: current.id != nodes[endpoint_index].id,
            ambiguous_sides: (ambiguous_sides.len() > 1).then_some(ambiguous_sides),
            inner: chain.last().map(|previous| GridAttachmentInner {
                demand_key: previous.demand_key.clone(),
                side: previous.side,
            }),
        });

        if current.is_group
            && current.id == lca_container_id
            && is_ancestor_group(&current.id, Some(other_index), nodes, node_by_id)
        {
            return Ok(EdgeEndpointPlan {
                chain,
                final_kind: EndpointFinalKind::Boundary,
            });
        }
        if parent_id == lca_container_id {
            return Ok(EdgeEndpointPlan {
                chain,
                final_kind: EndpointFinalKind::Item,
            });
        }

        let Some(parent) = parent else {
            return Err(grid_error(
                GridErrorCode::InvalidContainment,
                format!("Missing parent group \"{parent_id}\""),
                Some(serde_json::json!({
                    "traversedIds": chain.iter().map(|entry| entry.owner_id.clone()).collect::<Vec<_>>()
                })),
            ));
        };
        if !parent.is_group {
            return Err(grid_error(
                GridErrorCode::InvalidContainment,
                format!("Missing parent group \"{parent_id}\""),
                Some(serde_json::json!({
                    "traversedIds": chain.iter().map(|entry| entry.owner_id.clone()).collect::<Vec<_>>()
                })),
            ));
        }
        current_index = node_by_id[&parent_id];
    }
}

#[derive(Clone, Debug, PartialEq)]
struct PairLane {
    index: usize,
    size: usize,
    offset: f64,
    pair_key: String,
}

fn build_pair_lanes(edges: &[Edge]) -> HashMap<String, PairLane> {
    let mut by_pair: HashMap<String, Vec<&Edge>> = HashMap::new();
    for edge in edges {
        let (Some(start), Some(end)) = (edge.start.as_deref(), edge.end.as_deref()) else {
            continue;
        };
        let pair_key = if start < end {
            format!("{start}|{end}")
        } else {
            format!("{end}|{start}")
        };
        by_pair.entry(pair_key).or_default().push(edge);
    }

    let mut out = HashMap::new();
    for (pair_key, mut entries) in by_pair {
        // Directed endpoint order and edge id make lane assignment stable even if input edges
        // arrive in a different order.
        entries.sort_by(|a, b| {
            compare_code_units(
                &format!(
                    "{}|{}",
                    a.start.as_deref().unwrap_or(""),
                    a.end.as_deref().unwrap_or("")
                ),
                &format!(
                    "{}|{}",
                    b.start.as_deref().unwrap_or(""),
                    b.end.as_deref().unwrap_or("")
                ),
            )
            .then_with(|| compare_code_units(&a.id, &b.id))
        });
        let size = entries.len();
        for (index, edge) in entries.iter().enumerate() {
            out.insert(
                edge.id.clone(),
                PairLane {
                    index,
                    size,
                    offset: (index as f64 - (size as f64 - 1.0) / 2.0) * LANE_SEPARATION_PX,
                    pair_key: pair_key.clone(),
                },
            );
        }
    }
    out
}

fn resolve_endpoint(
    edge_id: &str,
    role: EndpointRole,
    endpoint: &mut EdgeEndpointPlan,
    result: &GridLayoutResult,
    nodes: &[Node],
    reserved: &mut HashMap<String, usize>,
) {
    let ambiguous: Vec<usize> = endpoint
        .chain
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.ambiguous_sides.is_some())
        .map(|(index, _)| index)
        .collect();
    if ambiguous.is_empty() {
        return;
    }
    let mut legal_sides: Vec<GridSide> = [
        GridSide::Right,
        GridSide::Bottom,
        GridSide::Left,
        GridSide::Top,
    ]
    .into_iter()
    .filter(|side| {
        ambiguous.iter().all(|index| {
            endpoint.chain[*index]
                .ambiguous_sides
                .as_ref()
                .is_some_and(|sides| sides.contains(side))
        }) && ambiguous.iter().all(|index| {
            node(nodes, result, &endpoint.chain[*index].owner_id)
                .is_some_and(|owner| side_interval(owner, *side).is_some())
        })
    })
    .collect();
    let original_side = endpoint.chain[ambiguous[0]].side;
    legal_sides.sort_by(|a, b| {
        let count = |side: GridSide| -> usize {
            ambiguous
                .iter()
                .map(|index| {
                    let entry = &endpoint.chain[*index];
                    reserved
                        .get(&owner_side_key(&entry.owner_id, side))
                        .copied()
                        .unwrap_or(0)
                })
                .sum()
        };
        count(*a)
            .cmp(&count(*b))
            .then_with(|| (*a != original_side).cmp(&(*b != original_side)))
    });
    let Some(selected) = legal_sides.first().copied() else {
        return;
    };
    for index in &ambiguous {
        let entry = &mut endpoint.chain[*index];
        entry.side = selected;
        *reserved
            .entry(owner_side_key(&entry.owner_id, selected))
            .or_insert(0) += 1;
        if let Some(owner) = node(nodes, result, &entry.owner_id) {
            entry.preferred_coord = opposite_coord_for(rect_for_node(owner), selected);
        }
    }
    for index in 0..endpoint.chain.len() {
        let owner_id = endpoint.chain[index].owner_id.clone();
        let side = endpoint.chain[index].side;
        endpoint.chain[index].demand_key = demand_key(edge_id, role, &owner_id, side);
        endpoint.chain[index].inner = if index > 0 {
            Some(GridAttachmentInner {
                demand_key: endpoint.chain[index - 1].demand_key.clone(),
                side: endpoint.chain[index - 1].side,
            })
        } else {
            None
        };
    }
}

fn resolve_ambiguous_endpoint_sides(
    plans: &mut [EdgeRoutePlan],
    result: &GridLayoutResult,
    nodes: &[Node],
) {
    // Reserve geometrically preferred sides first. Equal-cost endpoint choices then use the legal
    // side with the fewest strong reservations, preserving the original preference as a tie.
    let mut reserved: HashMap<String, usize> = HashMap::new();
    for plan in plans.iter() {
        for entry in plan.source.chain.iter().chain(plan.target.chain.iter()) {
            if entry.ambiguous_sides.is_none() {
                *reserved
                    .entry(owner_side_key(&entry.owner_id, entry.side))
                    .or_insert(0) += 1;
            }
        }
    }

    let mut endpoints: Vec<(usize, EndpointRole)> = Vec::new();
    for plan_index in 0..plans.len() {
        endpoints.push((plan_index, EndpointRole::Source));
        endpoints.push((plan_index, EndpointRole::Target));
    }
    endpoints.sort_by(|(a_index, a_role), (b_index, b_role)| {
        let (a, b) = (&plans[*a_index].edge, &plans[*b_index].edge);
        compare_code_units(
            &format!(
                "{}|{}|{}|{}",
                a.start.as_deref().unwrap_or(""),
                a.end.as_deref().unwrap_or(""),
                a.id,
                a_role.as_str()
            ),
            &format!(
                "{}|{}|{}|{}",
                b.start.as_deref().unwrap_or(""),
                b.end.as_deref().unwrap_or(""),
                b.id,
                b_role.as_str()
            ),
        )
    });
    for (plan_index, role) in endpoints {
        let plan = &mut plans[plan_index];
        let edge_id = plan.edge.id.clone();
        match role {
            EndpointRole::Source => resolve_endpoint(
                &edge_id,
                role,
                &mut plan.source,
                result,
                nodes,
                &mut reserved,
            ),
            EndpointRole::Target => resolve_endpoint(
                &edge_id,
                role,
                &mut plan.target,
                result,
                nodes,
                &mut reserved,
            ),
        }
    }

    for plan in plans.iter_mut() {
        let source_final = plan.source.chain.last().cloned();
        let target_final = plan.target.chain.last().cloned();
        let (Some(source_final), Some(target_final)) = (source_final, target_final) else {
            continue;
        };
        let (Some(source_owner), Some(target_owner)) = (
            node(nodes, result, &source_final.owner_id),
            node(nodes, result, &target_final.owner_id),
        ) else {
            continue;
        };
        let source_point = side_midpoint(rect_for_node(source_owner), source_final.side);
        let target_point = side_midpoint(rect_for_node(target_owner), target_final.side);
        for entry in plan.source.chain.iter_mut() {
            entry.opposite_coord = tangential_coordinate(target_point, entry.side);
        }
        for entry in plan.target.chain.iter_mut() {
            entry.opposite_coord = tangential_coordinate(source_point, entry.side);
        }
    }
}

fn collect_route_plans(
    layout: &LayoutData,
    result: &GridLayoutResult,
) -> Result<Vec<EdgeRoutePlan>, crate::layout::grid::types::GridError> {
    let nodes = &layout.nodes;
    let node_by_id = &result.forest.node_by_id;
    let pair_lanes = build_pair_lanes(&layout.edges);

    let mut plans = Vec::with_capacity(layout.edges.len());
    for edge in &layout.edges {
        let (Some(start), Some(end)) = (edge.start.as_deref(), edge.end.as_deref()) else {
            return Err(grid_error(
                GridErrorCode::MissingEndpoint,
                format!("Missing endpoint for edge \"{}\"", edge.id),
                Some(serde_json::json!({
                    "edgeId": edge.id,
                    "start": edge.start,
                    "end": edge.end,
                })),
            ));
        };
        let (Some(source_index), Some(target_index)) =
            (node_index(result, start), node_index(result, end))
        else {
            return Err(grid_error(
                GridErrorCode::MissingEndpoint,
                format!("Missing endpoint for edge \"{}\"", edge.id),
                Some(serde_json::json!({
                    "edgeId": edge.id,
                    "start": edge.start,
                    "end": edge.end,
                })),
            ));
        };
        let lca_container_id = common_container_id(source_index, target_index, nodes, node_by_id);
        let pair_lane = pair_lanes
            .get(&edge.id)
            .cloned()
            .unwrap_or_else(|| PairLane {
                index: 0,
                size: 1,
                offset: 0.0,
                pair_key: endpoint_pair_key(edge),
            });
        plans.push(EdgeRoutePlan {
            edge: edge.clone(),
            lca_container_id: lca_container_id.clone(),
            source: build_endpoint_plan(
                source_index,
                target_index,
                &lca_container_id,
                nodes,
                node_by_id,
                &edge.id,
                EndpointRole::Source,
            )?,
            target: build_endpoint_plan(
                target_index,
                source_index,
                &lca_container_id,
                nodes,
                node_by_id,
                &edge.id,
                EndpointRole::Target,
            )?,
            lane_index: pair_lane.index,
            lane_offset: pair_lane.offset,
            bundle_size: pair_lane.size,
            pair_key: pair_lane.pair_key,
        });
    }
    resolve_ambiguous_endpoint_sides(&mut plans, result, nodes);
    Ok(plans)
}

pub fn assign_compact_portal_coordinates(desired: &[f64], low: f64, high: f64) -> Vec<f64> {
    // Forward/backward constraint passes enforce minimum spacing; the final common shift preserves
    // the requested center of mass as far as the legal interval permits.
    let mut coordinates: Vec<f64> = Vec::with_capacity(desired.len());
    for (index, coordinate) in desired.iter().enumerate() {
        coordinates.push(if index == 0 {
            *coordinate
        } else {
            coordinate.max(coordinates[index - 1] + MIN_PORT_SEPARATION_PX)
        });
    }
    for index in (0..coordinates.len()).rev() {
        coordinates[index] = coordinates[index].min(if index == coordinates.len() - 1 {
            high
        } else {
            coordinates[index + 1] - MIN_PORT_SEPARATION_PX
        });
    }
    if desired.is_empty() || coordinates.is_empty() {
        return coordinates;
    }
    let average_desired = desired.iter().sum::<f64>() / desired.len() as f64;
    let average_assigned = coordinates.iter().sum::<f64>() / coordinates.len() as f64;
    let minimum_shift = low - coordinates[0];
    let maximum_shift = high - coordinates[coordinates.len() - 1];
    let shift = clamp(
        average_desired - average_assigned,
        minimum_shift,
        maximum_shift,
    );
    coordinates
        .into_iter()
        .map(|coordinate| coordinate + shift)
        .collect()
}

fn assign_demand_coordinates(
    plans: &[EdgeRoutePlan],
    result: &GridLayoutResult,
    nodes: &[Node],
    endpoint_incident_counts: &HashMap<String, usize>,
) -> HashMap<String, f64> {
    // Allocate all demands for an owner side together. This prevents independently routed edges
    // from selecting the same port and lets bundles retain symmetric lane offsets when capacity
    // allows.
    let node_by_id = &result.forest.node_by_id;
    let plan_by_edge_id: HashMap<&str, usize> = plans
        .iter()
        .enumerate()
        .map(|(index, plan)| (plan.edge.id.as_str(), index))
        .collect();
    let mut demand_by_key: HashMap<String, GridAttachmentDemand> = HashMap::new();
    for plan in plans {
        if plan.edge.start == plan.edge.end {
            continue;
        }
        for entry in plan.source.chain.iter().chain(plan.target.chain.iter()) {
            demand_by_key
                .entry(entry.demand_key.clone())
                .or_insert_with(|| GridAttachmentDemand {
                    owner_id: entry.owner_id.clone(),
                    side: entry.side,
                    edge_id: plan.edge.id.clone(),
                    demand_key: entry.demand_key.clone(),
                    opposite_coord: entry.opposite_coord,
                    preferred_coord: entry.preferred_coord,
                    compact_portal: entry.compact_portal,
                    inner: entry.inner.clone(),
                });
        }
    }

    struct OwnerSideGroup {
        owner_id: String,
        side: GridSide,
        demands: Vec<GridAttachmentDemand>,
    }
    let mut by_owner_side: Vec<OwnerSideGroup> = Vec::new();
    let mut index_by_key: HashMap<String, usize> = HashMap::new();
    for demand in demand_by_key.values() {
        let key = owner_side_key(&demand.owner_id, demand.side);
        let index = *index_by_key.entry(key).or_insert_with(|| {
            by_owner_side.push(OwnerSideGroup {
                owner_id: demand.owner_id.clone(),
                side: demand.side,
                demands: Vec::new(),
            });
            by_owner_side.len() - 1
        });
        by_owner_side[index].demands.push(demand.clone());
    }

    // Ports are placed before the compact portals that line up with them, innermost group first.
    let depth_of = |owner_id: &str| -> usize {
        node_by_id
            .get(owner_id)
            .map(|index| container_chain(*index, nodes, node_by_id).len())
            .unwrap_or(0)
    };
    by_owner_side.sort_by(|a, b| {
        let a_compact = a.demands.iter().all(|demand| demand.compact_portal);
        let b_compact = b.demands.iter().all(|demand| demand.compact_portal);
        (a_compact as u8)
            .cmp(&(b_compact as u8))
            .then_with(|| depth_of(&b.owner_id).cmp(&depth_of(&a.owner_id)))
    });
    let mut assigned: HashMap<String, f64> = HashMap::new();
    for group in &by_owner_side {
        let OwnerSideGroup {
            owner_id,
            side,
            demands,
        } = group;
        let Some(owner_node) = node(nodes, result, owner_id) else {
            continue;
        };
        let rect = rect_for_node(owner_node);
        let mut low = if *side == GridSide::Left || *side == GridSide::Right {
            rect.top + ROUTE_CLEARANCE_PX
        } else {
            rect.left + ROUTE_CLEARANCE_PX
        };
        let high = if *side == GridSide::Left || *side == GridSide::Right {
            rect.bottom - ROUTE_CLEARANCE_PX
        } else {
            rect.right - ROUTE_CLEARANCE_PX
        };
        if let Some(title) = owner_node
            .group_title_rect
            .filter(|_| *side == GridSide::Left || *side == GridSide::Right)
        {
            low = low.max(title.bottom + ROUTE_CLEARANCE_PX);
        }
        let span = (high - low).max(0.0);
        let mut demands = demands.clone();
        demands.sort_by(|a, b| {
            a.opposite_coord
                .total_cmp(&b.opposite_coord)
                .then_with(|| compare_code_units(&a.edge_id, &b.edge_id))
        });
        let demand_plans: Vec<Option<&EdgeRoutePlan>> = demands
            .iter()
            .map(|demand| {
                plan_by_edge_id
                    .get(demand.edge_id.as_str())
                    .map(|index| &plans[*index])
            })
            .collect();
        let pair_keys: HashSet<Option<&str>> = demand_plans
            .iter()
            .map(|plan| plan.map(|plan| plan.pair_key.as_str()))
            .collect();
        let compact_hierarchy_portals =
            owner_node.is_group && demands.iter().all(|demand| demand.compact_portal);
        if !(compact_hierarchy_portals && demands.len() == 1)
            && pair_keys.len() == 1
            && demand_plans.iter().all(|plan| {
                plan.is_some_and(|plan| {
                    plan.bundle_size == demands.len()
                        && plan
                            .edge
                            .start
                            .as_deref()
                            .and_then(|start| endpoint_incident_counts.get(start))
                            .copied()
                            == Some(plan.bundle_size)
                        && plan
                            .edge
                            .end
                            .as_deref()
                            .and_then(|end| endpoint_incident_counts.get(end))
                            .copied()
                            == Some(plan.bundle_size)
                })
            })
        {
            let center = (low + high) / 2.0;
            let coordinates: Vec<f64> = demand_plans
                .iter()
                .map(|plan| center + plan.unwrap().lane_offset)
                .collect();
            if coordinates
                .iter()
                .all(|coordinate| *coordinate >= low && *coordinate <= high)
            {
                for (demand, coordinate) in demands.iter().zip(coordinates) {
                    assigned.insert(demand.demand_key.clone(), coordinate);
                }
                continue;
            }
        }
        if compact_hierarchy_portals {
            let container = result.containers.get(&owner_node.id);
            let corridor_coordinates: Option<&Vec<f64>> =
                if *side == GridSide::Left || *side == GridSide::Right {
                    container.map(|container| &container.horizontal_corridors)
                } else {
                    container.map(|container| &container.vertical_corridors)
                };
            let inner_coordinate = |demand: &GridAttachmentDemand| -> f64 {
                let inner = demand
                    .inner
                    .as_ref()
                    .and_then(|inner| demand_by_key.get(&inner.demand_key));
                let inner_owner = inner.and_then(|inner| node(nodes, result, &inner.owner_id));
                let coordinate = inner.and_then(|inner| assigned.get(&inner.demand_key));
                let (Some(inner), Some(inner_owner), Some(coordinate)) =
                    (inner, inner_owner, coordinate)
                else {
                    return demand.preferred_coord;
                };
                let mut point = side_midpoint(rect_for_node(inner_owner), inner.side);
                if inner.side == GridSide::Left || inner.side == GridSide::Right {
                    point.y = *coordinate;
                } else {
                    point.x = *coordinate;
                }
                tangential_coordinate(point, *side)
            };
            let preferred_coordinate = |demand: &GridAttachmentDemand| -> f64 {
                let inner_coord = demand
                    .inner
                    .as_ref()
                    .and_then(|inner| assigned.get(&inner.demand_key))
                    .copied();
                let aligned_with_inner = match (&demand.inner, inner_coord) {
                    (Some(inner), Some(_)) => {
                        (inner.side == GridSide::Left || inner.side == GridSide::Right)
                            == (*side == GridSide::Left || *side == GridSide::Right)
                    }
                    _ => false,
                };
                let clamped = clamp(
                    if aligned_with_inner {
                        inner_coord.unwrap()
                    } else {
                        demand.preferred_coord
                    },
                    low,
                    high,
                );
                let nearest = corridor_coordinates.and_then(|coordinates| {
                    coordinates.iter().copied().reduce(|best, coordinate| {
                        if (coordinate - clamped).abs() < (best - clamped).abs() {
                            coordinate
                        } else {
                            best
                        }
                    })
                });
                match nearest {
                    Some(nearest) if (nearest - clamped).abs() <= PIXEL_EPSILON => nearest,
                    _ => clamped,
                }
            };
            if demands.len() == 1 {
                assigned.insert(
                    demands[0].demand_key.clone(),
                    preferred_coordinate(&demands[0]),
                );
                continue;
            }
            if span >= MIN_PORT_SEPARATION_PX * (demands.len() as f64 - 1.0) {
                demands.sort_by(|a, b| {
                    inner_coordinate(a)
                        .total_cmp(&inner_coordinate(b))
                        .then_with(|| a.opposite_coord.total_cmp(&b.opposite_coord))
                        .then_with(|| compare_code_units(&a.edge_id, &b.edge_id))
                });
                let desired: Vec<f64> = demands.iter().map(preferred_coordinate).collect();
                let coordinates = assign_compact_portal_coordinates(&desired, low, high);
                for (demand, coordinate) in demands.iter().zip(coordinates) {
                    assigned.insert(demand.demand_key.clone(), coordinate);
                }
                continue;
            }
        }
        if demands.len() == 1 || span <= 0.0 {
            assigned.insert(demands[0].demand_key.clone(), (low + high) / 2.0);
            continue;
        }
        let step = span / (demands.len() as f64 - 1.0);
        for (index, demand) in demands.iter().enumerate() {
            assigned.insert(demand.demand_key.clone(), low + step * index as f64);
        }
    }

    // Keep a nearly aligned hierarchy route on one exact coordinate. Sub-pixel differences between
    // independently allocated endpoint and portal slots are otherwise normalized away as a
    // diagonal, which unnecessarily demotes the route to sparse search.
    for plan in plans {
        if plan.bundle_size != 1 {
            continue;
        }
        let (Some(source_final), Some(target_final)) =
            (plan.source.chain.last(), plan.target.chain.last())
        else {
            continue;
        };
        let vertical_side = |side: GridSide| side == GridSide::Left || side == GridSide::Right;
        if vertical_side(source_final.side) != vertical_side(target_final.side) {
            continue;
        }
        let (Some(source_coord), Some(target_coord)) = (
            assigned.get(&source_final.demand_key).copied(),
            assigned.get(&target_final.demand_key).copied(),
        ) else {
            continue;
        };
        if (source_coord - target_coord).abs() > PIXEL_EPSILON {
            continue;
        }
        let entries: Vec<&EdgeEndpointEntry> = plan
            .source
            .chain
            .iter()
            .chain(plan.target.chain.iter())
            .collect();
        if entries
            .iter()
            .any(|entry| vertical_side(entry.side) != vertical_side(source_final.side))
        {
            continue;
        }
        let aligned_coord = [source_coord, target_coord].into_iter().find(|coordinate| {
            entries.iter().all(|entry| {
                node(nodes, result, &entry.owner_id)
                    .and_then(|owner| side_interval(owner, entry.side))
                    .is_some_and(|interval| {
                        *coordinate >= interval.0 - PIXEL_EPSILON
                            && *coordinate <= interval.1 + PIXEL_EPSILON
                    })
            })
        });
        if let Some(aligned_coord) = aligned_coord {
            for entry in plan.source.chain.iter().chain(plan.target.chain.iter()) {
                assigned.insert(entry.demand_key.clone(), aligned_coord);
            }
        }
    }
    assigned
}

pub fn side_order(side: GridSide) -> u8 {
    // Endpoint preference order is independent of topology's canonical record ordering.
    match side {
        GridSide::Right => 0,
        GridSide::Bottom => 1,
        GridSide::Left => 2,
        GridSide::Top => 3,
    }
}

pub fn side_interval(owner: &Node, side: GridSide) -> Option<(f64, f64)> {
    let rect = rect_for_node(owner);
    if side == GridSide::Top && owner.group_title_rect.is_some() {
        return None;
    }
    let mut low = if side == GridSide::Left || side == GridSide::Right {
        rect.top + ROUTE_CLEARANCE_PX
    } else {
        rect.left + ROUTE_CLEARANCE_PX
    };
    let high = if side == GridSide::Left || side == GridSide::Right {
        rect.bottom - ROUTE_CLEARANCE_PX
    } else {
        rect.right - ROUTE_CLEARANCE_PX
    };
    if let Some(title) = owner
        .group_title_rect
        .filter(|_| side == GridSide::Left || side == GridSide::Right)
    {
        low = low.max(title.bottom + ROUTE_CLEARANCE_PX);
    }
    (low <= high).then_some((low, high))
}

pub fn endpoint_pair_key(edge: &Edge) -> String {
    let start = edge.start.as_deref().unwrap_or("");
    let end = edge.end.as_deref().unwrap_or("");
    if start < end {
        format!("{start}|{end}")
    } else {
        format!("{end}|{start}")
    }
}

fn compare_endpoint_demands(
    a: EndpointDemandEntry,
    b: EndpointDemandEntry,
    side: GridSide,
    plans: &[EdgeRoutePlan],
    nodes: &[Node],
) -> Ordering {
    let a_rect = rect_for_node(&nodes[a.opposite_index]);
    let b_rect = rect_for_node(&nodes[b.opposite_index]);
    let a_coord = if side == GridSide::Left || side == GridSide::Right {
        a_rect.cy
    } else {
        a_rect.cx
    };
    let b_coord = if side == GridSide::Left || side == GridSide::Right {
        b_rect.cy
    } else {
        b_rect.cx
    };
    let a_edge = &plans[a.plan_index].edge;
    let b_edge = &plans[b.plan_index].edge;
    a_coord
        .total_cmp(&b_coord)
        .then_with(|| compare_code_units(&endpoint_pair_key(a_edge), &endpoint_pair_key(b_edge)))
        .then_with(|| {
            compare_code_units(
                &format!(
                    "{}|{}",
                    a_edge.start.as_deref().unwrap_or(""),
                    a_edge.end.as_deref().unwrap_or("")
                ),
                &format!(
                    "{}|{}",
                    b_edge.start.as_deref().unwrap_or(""),
                    b_edge.end.as_deref().unwrap_or("")
                ),
            )
        })
        .then_with(|| compare_code_units(&a_edge.id, &b_edge.id))
        .then_with(|| compare_code_units(a.role.as_str(), b.role.as_str()))
}

/// Sorted demand order is part of the result: callers that place further ports beside these
/// coordinates depend on the same order the upstream `Map` preserves.
fn preferred_endpoint_coordinates(
    owner: &Node,
    side: GridSide,
    demands: &[EndpointDemandEntry],
    plans: &[EdgeRoutePlan],
    nodes: &[Node],
) -> Vec<(EndpointDemandEntry, f64)> {
    let mut out: Vec<(EndpointDemandEntry, f64)> = Vec::new();
    let Some((low, high)) = side_interval(owner, side) else {
        return out;
    };
    if demands.is_empty() {
        return out;
    }
    let mut sorted: Vec<EndpointDemandEntry> = demands.to_vec();
    sorted.sort_by(|a, b| compare_endpoint_demands(*a, *b, side, plans, nodes));
    if sorted.len() == 1 {
        let rect = rect_for_node(owner);
        let center = if side == GridSide::Left || side == GridSide::Right {
            rect.cy
        } else {
            rect.cx
        };
        out.push((sorted[0], clamp(center, low, high)));
        return out;
    }
    if (high - low) / (sorted.len() as f64 - 1.0) < MIN_PORT_SEPARATION_PX {
        for (index, demand) in sorted.iter().enumerate() {
            out.push((
                *demand,
                low + (index as f64 * (high - low)) / (sorted.len() as f64 - 1.0),
            ));
        }
        return out;
    }
    let desired: Vec<f64> = sorted
        .iter()
        .map(|demand| {
            let rect = rect_for_node(&nodes[demand.opposite_index]);
            let coordinate = if side == GridSide::Left || side == GridSide::Right {
                rect.cy
            } else {
                rect.cx
            };
            clamp(coordinate, low, high)
        })
        .collect();
    let coordinates = assign_compact_portal_coordinates(&desired, low, high);
    for (index, demand) in sorted.iter().enumerate() {
        out.push((*demand, coordinates[index]));
    }
    out
}

fn coordinate_for(
    placements: &[(EndpointDemandEntry, f64)],
    demand: EndpointDemandEntry,
) -> Option<f64> {
    placements
        .iter()
        .find(|(candidate, _)| *candidate == demand)
        .map(|(_, coordinate)| *coordinate)
}

fn endpoint_side_capacity(owner: &Node, side: GridSide) -> usize {
    let Some((low, high)) = side_interval(owner, side) else {
        return 0;
    };
    (1.0 + ((high - low) / MIN_PORT_SEPARATION_PX).floor()).max(0.0) as usize
}

fn endpoint_side_preferences(
    owner: &Node,
    demand: EndpointDemandEntry,
    nodes: &[Node],
) -> Vec<GridSide> {
    let opposite = &nodes[demand.opposite_index];
    let opposite_rect = rect_for_node(opposite);
    let preferred = preferred_side(
        owner,
        RouterPoint {
            x: opposite_rect.cx,
            y: opposite_rect.cy,
        },
    );
    let mut sides: Vec<GridSide> = [
        GridSide::Right,
        GridSide::Bottom,
        GridSide::Left,
        GridSide::Top,
    ]
    .into_iter()
    .filter(|side| endpoint_side_capacity(owner, *side) > 0)
    .collect();
    let owner_rect = rect_for_node(owner);
    sides.sort_by(|a, b| {
        if *a == preferred {
            return Ordering::Less;
        }
        if *b == preferred {
            return Ordering::Greater;
        }
        let a_point = side_midpoint(owner_rect, *a);
        let b_point = side_midpoint(owner_rect, *b);
        let a_distance =
            (a_point.x - opposite_rect.cx).abs() + (a_point.y - opposite_rect.cy).abs();
        let b_distance =
            (b_point.x - opposite_rect.cx).abs() + (b_point.y - opposite_rect.cy).abs();
        a_distance
            .total_cmp(&b_distance)
            .then_with(|| side_order(*a).cmp(&side_order(*b)))
    });
    sides
}

fn is_axis_aligned_demand(owner: &Node, demand: EndpointDemandEntry, nodes: &[Node]) -> bool {
    let owner_rect = rect_for_node(owner);
    let opposite_rect = rect_for_node(&nodes[demand.opposite_index]);
    (owner_rect.cx - opposite_rect.cx).abs() <= PIXEL_EPSILON
        || (owner_rect.cy - opposite_rect.cy).abs() <= PIXEL_EPSILON
}

// Places unaligned ports on a side that already carries fixed (aligned) ports. Each port takes the
// legal coordinate nearest its desired one that keeps EDGE_CLEARANCE_PX from every other port, so
// the aligned ports stay on their straight lines. Returns None when the side cannot fit them all.
fn place_ports_beside_fixed(
    owner: &Node,
    side: GridSide,
    fixed: &[f64],
    movable: &[EndpointDemandEntry],
    nodes: &[Node],
) -> Option<Vec<(EndpointDemandEntry, f64)>> {
    let (low, high) = side_interval(owner, side)?;
    let mut placed = fixed.to_vec();
    let mut placements: Vec<(EndpointDemandEntry, f64)> = Vec::new();
    for demand in movable {
        let opposite = rect_for_node(&nodes[demand.opposite_index]);
        let desired = clamp(
            if side == GridSide::Left || side == GridSide::Right {
                opposite.cy
            } else {
                opposite.cx
            },
            low,
            high,
        );
        let mut candidates: Vec<f64> = vec![desired, low, high];
        for coordinate in &placed {
            candidates.push(coordinate - EDGE_CLEARANCE_PX);
            candidates.push(coordinate + EDGE_CLEARANCE_PX);
        }
        candidates.retain(|coordinate| {
            *coordinate >= low - PIXEL_EPSILON
                && *coordinate <= high + PIXEL_EPSILON
                && placed
                    .iter()
                    .all(|other| (*coordinate - *other).abs() >= EDGE_CLEARANCE_PX - PIXEL_EPSILON)
        });
        candidates.sort_by(|a, b| {
            (a - desired)
                .abs()
                .total_cmp(&(b - desired).abs())
                .then_with(|| a.total_cmp(b))
        });
        let coordinate = candidates.first().copied()?;
        placements.push((*demand, coordinate));
        placed.push(coordinate);
    }
    Some(placements)
}

fn allocate_endpoint_slots(
    owner: &Node,
    demands: &[EndpointDemandEntry],
    plans: &[EdgeRoutePlan],
    nodes: &[Node],
) -> HashMap<(usize, EndpointRole), EndpointSlotAssignment> {
    // Aligned edges get first choice of their direct side and keep their straight port. Other edges
    // prefer an unreserved side, or share a reserved side when their port can sit a full clearance
    // away from the aligned ports. This stops a diagonal approach from displacing a straight one.
    let mut assigned_by_side: Vec<(GridSide, Vec<EndpointDemandEntry>)> = Vec::new();
    let capacities: Vec<(GridSide, usize)> = [
        GridSide::Right,
        GridSide::Bottom,
        GridSide::Left,
        GridSide::Top,
    ]
    .into_iter()
    .map(|side| (side, endpoint_side_capacity(owner, side)))
    .collect();

    let mut ordered_demands: Vec<EndpointDemandEntry> = demands.to_vec();
    ordered_demands.sort_by(|a, b| {
        let a_edge = &plans[a.plan_index].edge;
        let b_edge = &plans[b.plan_index].edge;
        compare_code_units(&endpoint_pair_key(a_edge), &endpoint_pair_key(b_edge))
            .then_with(|| {
                compare_code_units(
                    &format!(
                        "{}|{}",
                        a_edge.start.as_deref().unwrap_or(""),
                        a_edge.end.as_deref().unwrap_or("")
                    ),
                    &format!(
                        "{}|{}",
                        b_edge.start.as_deref().unwrap_or(""),
                        b_edge.end.as_deref().unwrap_or("")
                    ),
                )
            })
            .then_with(|| compare_code_units(&a_edge.id, &b_edge.id))
            .then_with(|| compare_code_units(a.role.as_str(), b.role.as_str()))
    });
    let aligned_demands: Vec<EndpointDemandEntry> = ordered_demands
        .iter()
        .copied()
        .filter(|demand| is_axis_aligned_demand(owner, *demand, nodes))
        .collect();
    let unaligned_demands: Vec<EndpointDemandEntry> = ordered_demands
        .iter()
        .copied()
        .filter(|demand| !is_axis_aligned_demand(owner, *demand, nodes))
        .collect();
    let mut reserved_sides: Vec<GridSide> = Vec::new();
    let mut aligned_coordinates: HashMap<GridSide, Vec<f64>> = HashMap::new();
    let mut shared_placements: HashMap<GridSide, Vec<(EndpointDemandEntry, f64)>> = HashMap::new();
    let capacity_of = |side: GridSide| -> usize {
        capacities
            .iter()
            .find(|(candidate, _)| *candidate == side)
            .map(|(_, capacity)| *capacity)
            .unwrap_or(0)
    };
    let has_capacity =
        |side: GridSide, assigned_by_side: &Vec<(GridSide, Vec<EndpointDemandEntry>)>| {
            assigned_by_side
                .iter()
                .find(|(candidate, _)| *candidate == side)
                .map(|(_, entries)| entries.len())
                .unwrap_or(0)
                < capacity_of(side)
        };
    let record =
        |demand: EndpointDemandEntry,
         side: GridSide,
         assigned_by_side: &mut Vec<(GridSide, Vec<EndpointDemandEntry>)>| {
            if let Some(entry) = assigned_by_side
                .iter_mut()
                .find(|(candidate, _)| *candidate == side)
            {
                entry.1.push(demand);
            } else {
                assigned_by_side.push((side, vec![demand]));
            }
        };

    for demand in &aligned_demands {
        if let Some(side) = endpoint_side_preferences(owner, *demand, nodes)
            .into_iter()
            .find(|side| has_capacity(*side, &assigned_by_side))
        {
            record(*demand, side, &mut assigned_by_side);
            if !reserved_sides.contains(&side) {
                reserved_sides.push(side);
            }
        }
    }
    for (side, assigned) in &assigned_by_side {
        let coordinates: Vec<f64> =
            preferred_endpoint_coordinates(owner, *side, assigned, plans, nodes)
                .into_iter()
                .map(|(_, coordinate)| coordinate)
                .collect();
        aligned_coordinates.insert(*side, coordinates);
    }
    let try_share_reserved_side =
        |demand: EndpointDemandEntry,
         side: GridSide,
         assigned_by_side: &mut Vec<(GridSide, Vec<EndpointDemandEntry>)>,
         shared_placements: &mut HashMap<GridSide, Vec<(EndpointDemandEntry, f64)>>| {
            let mut fixed: Vec<f64> = aligned_coordinates.get(&side).cloned().unwrap_or_default();
            if let Some(shared) = shared_placements.get(&side) {
                fixed.extend(shared.iter().map(|(_, coordinate)| *coordinate));
            }
            let Some(placements) = place_ports_beside_fixed(owner, side, &fixed, &[demand], nodes)
            else {
                return false;
            };
            let coordinate = coordinate_for(&placements, demand).unwrap();
            shared_placements
                .entry(side)
                .or_default()
                .push((demand, coordinate));
            record(demand, side, assigned_by_side);
            true
        };
    for demand in &unaligned_demands {
        let preferences: Vec<GridSide> = endpoint_side_preferences(owner, *demand, nodes)
            .into_iter()
            .filter(|side| has_capacity(*side, &assigned_by_side))
            .collect();
        let unreserved = preferences
            .iter()
            .copied()
            .find(|side| !reserved_sides.contains(side));
        let mut sharable = false;
        for candidate in &preferences {
            if let Some(unreserved) = unreserved {
                let candidate_position = preferences.iter().position(|side| side == candidate);
                let unreserved_position = preferences.iter().position(|side| side == &unreserved);
                if !matches!((candidate_position, unreserved_position), (Some(a), Some(b)) if a < b)
                {
                    continue;
                }
            }
            if !reserved_sides.contains(candidate) {
                continue;
            }
            if try_share_reserved_side(
                *demand,
                *candidate,
                &mut assigned_by_side,
                &mut shared_placements,
            ) {
                sharable = true;
                break;
            }
        }
        if sharable {
            continue;
        }
        let side = unreserved.or_else(|| preferences.first().copied());
        if let Some(side) = side {
            record(*demand, side, &mut assigned_by_side);
        }
    }

    let mut assignments: HashMap<(usize, EndpointRole), EndpointSlotAssignment> = HashMap::new();
    for (side, assigned) in &assigned_by_side {
        let Some((low, high)) = side_interval(owner, *side) else {
            continue;
        };
        let shared = shared_placements.get(side);
        let fixed_demands: Vec<EndpointDemandEntry> = match shared {
            Some(shared) => assigned
                .iter()
                .copied()
                .filter(|demand| coordinate_for(shared, *demand).is_none())
                .collect(),
            None => assigned.clone(),
        };
        let mut preferred_coordinates =
            preferred_endpoint_coordinates(owner, *side, &fixed_demands, plans, nodes);
        if let Some(shared) = shared {
            preferred_coordinates.extend(shared.iter().copied());
        }
        let mut sorted = assigned.clone();
        sorted.sort_by(|a, b| compare_endpoint_demands(*a, *b, *side, plans, nodes));
        for (index, demand) in sorted.iter().enumerate() {
            let coordinate = coordinate_for(&preferred_coordinates, *demand).unwrap_or_else(|| {
                if sorted.len() == 1 {
                    (low + high) / 2.0
                } else {
                    low + (index as f64 * (high - low)) / (sorted.len() as f64 - 1.0)
                }
            });
            assignments.insert(
                (demand.plan_index, demand.role),
                EndpointSlotAssignment {
                    side: *side,
                    coordinate,
                },
            );
        }
    }
    assignments
}

#[allow(clippy::too_many_arguments)]
fn endpoint_candidates(
    plan_index: usize,
    plan: &EdgeRoutePlan,
    role: EndpointRole,
    plans: &[EdgeRoutePlan],
    demands_by_owner: &HashMap<String, Vec<EndpointDemandEntry>>,
    assignments_by_owner: &HashMap<String, HashMap<(usize, EndpointRole), EndpointSlotAssignment>>,
    coordinated_owner_ids: &HashSet<String>,
    result: &GridLayoutResult,
    nodes: &[Node],
) -> Result<Vec<EndpointCandidate>, crate::layout::grid::types::GridError> {
    let owner_id = match role {
        EndpointRole::Source => plan.edge.start.as_deref(),
        EndpointRole::Target => plan.edge.end.as_deref(),
    };
    let opposite_id = match role {
        EndpointRole::Source => plan.edge.end.as_deref(),
        EndpointRole::Target => plan.edge.start.as_deref(),
    };
    let (Some(owner_id), Some(opposite_id)) = (owner_id, opposite_id) else {
        return Err(grid_error(
            GridErrorCode::MissingEndpoint,
            format!("Missing endpoint for edge \"{}\"", plan.edge.id),
            None,
        ));
    };
    let (Some(owner), Some(opposite)) = (
        node(nodes, result, owner_id),
        node(nodes, result, opposite_id),
    ) else {
        return Err(grid_error(
            GridErrorCode::MissingEndpoint,
            format!("Missing endpoint for edge \"{}\"", plan.edge.id),
            None,
        ));
    };
    let opposite_rect = rect_for_node(opposite);
    let mut candidates: Vec<EndpointCandidate> = Vec::new();
    let mut candidate_keys: HashSet<String> = HashSet::new();

    let assignment = assignments_by_owner
        .get(owner_id)
        .and_then(|assignments| assignments.get(&(plan_index, role)));
    let track_assigned_side = coordinated_owner_ids.contains(owner_id) && assignment.is_some();
    let mut add_candidate = |side: GridSide, coordinate: f64, rank: f64| {
        let Some((low, high)) = side_interval(owner, side) else {
            return;
        };
        if coordinate < low || coordinate > high {
            return;
        }
        let rect = rect_for_node(owner);
        let port = if side == GridSide::Left || side == GridSide::Right {
            RouterPoint {
                x: if side == GridSide::Left {
                    rect.left
                } else {
                    rect.right
                },
                y: coordinate,
            }
        } else {
            RouterPoint {
                x: coordinate,
                y: if side == GridSide::Top {
                    rect.top
                } else {
                    rect.bottom
                },
            }
        };
        let key = format!("{}:{}", side.as_str(), routing_point_key(port));
        if !candidate_keys.insert(key) {
            return;
        }
        candidates.push(EndpointCandidate {
            owner_id: owner_id.to_string(),
            side,
            port,
            connect: RouterPoint {
                x: port.x
                    + match side {
                        GridSide::Left => -TERMINAL_APPROACH_PX,
                        GridSide::Right => TERMINAL_APPROACH_PX,
                        _ => 0.0,
                    },
                y: port.y
                    + match side {
                        GridSide::Top => -TERMINAL_APPROACH_PX,
                        GridSide::Bottom => TERMINAL_APPROACH_PX,
                        _ => 0.0,
                    },
            },
            rank,
            on_assigned_side: track_assigned_side.then(|| side == assignment.unwrap().side),
        });
    };

    // Negative ranks reserve precedence for the globally allocated slot and the bundle lane. The
    // remaining geometric alternatives are re-ranked after deterministic sorting.
    if let Some(assignment) = assignment.filter(|_| plan.bundle_size == 1) {
        add_candidate(assignment.side, assignment.coordinate, -2.0);
    }

    if plan.bundle_size > 1 {
        let side = preferred_side(
            owner,
            RouterPoint {
                x: opposite_rect.cx,
                y: opposite_rect.cy,
            },
        );
        let owner_rect = rect_for_node(owner);
        let center = if side == GridSide::Left || side == GridSide::Right {
            owner_rect.cy
        } else {
            owner_rect.cx
        };
        add_candidate(side, center + plan.lane_offset, -1.0);
    }

    let owner_demands = demands_by_owner.get(owner_id).cloned().unwrap_or_default();
    for side in [
        GridSide::Right,
        GridSide::Bottom,
        GridSide::Left,
        GridSide::Top,
    ] {
        if side_interval(owner, side).is_none() {
            continue;
        }
        let coordinates = preferred_endpoint_coordinates(owner, side, &owner_demands, plans, nodes);
        let demand = owner_demands
            .iter()
            .find(|entry| entry.plan_index == plan_index && entry.role == role);
        let Some(demand) = demand else {
            continue;
        };
        let Some(coordinate) = coordinate_for(&coordinates, *demand) else {
            continue;
        };
        let rect = rect_for_node(owner);
        let port = if side == GridSide::Left || side == GridSide::Right {
            RouterPoint {
                x: if side == GridSide::Left {
                    rect.left
                } else {
                    rect.right
                },
                y: coordinate,
            }
        } else {
            RouterPoint {
                x: coordinate,
                y: if side == GridSide::Top {
                    rect.top
                } else {
                    rect.bottom
                },
            }
        };
        let distance = (port.x - opposite_rect.cx).abs() + (port.y - opposite_rect.cy).abs();
        add_candidate(side, coordinate, distance);
    }

    candidates.sort_by(|a, b| {
        a.rank
            .total_cmp(&b.rank)
            .then_with(|| side_order(a.side).cmp(&side_order(b.side)))
    });
    for (index, candidate) in candidates.iter_mut().enumerate() {
        candidate.rank = index as f64;
    }
    Ok(candidates)
}

pub fn prepare_edge_routes(
    layout: &LayoutData,
    result: &GridLayoutResult,
) -> Result<PreparedEdgeRoutes, crate::layout::grid::types::GridError> {
    // Route order is a correctness input because committed routes constrain later bundle siblings.
    // Harder hierarchy and loop routes go first, followed by fewer candidate combinations and
    // stable lexical tie-breakers.
    let nodes = &layout.nodes;
    let plans = collect_route_plans(layout, result)?;
    let mut pair_counts: HashMap<String, usize> = HashMap::new();
    let mut endpoint_incident_counts: HashMap<String, usize> = HashMap::new();
    for plan in &plans {
        let key = endpoint_pair_key(&plan.edge);
        *pair_counts.entry(key).or_insert(0) += 1;
        if let Some(start) = plan.edge.start.as_deref() {
            *endpoint_incident_counts
                .entry(start.to_string())
                .or_insert(0) += 1;
        }
        if let Some(end) = plan
            .edge
            .end
            .as_deref()
            .filter(|end| plan.edge.start.as_deref() != Some(*end))
        {
            *endpoint_incident_counts.entry(end.to_string()).or_insert(0) += 1;
        }
    }
    let eligible_indices: Vec<usize> = plans
        .iter()
        .enumerate()
        .filter(|(_, plan)| {
            plan.edge.start != plan.edge.end
                && plan.source.chain.len() == 1
                && plan.target.chain.len() == 1
                && plan.source.final_kind == EndpointFinalKind::Item
                && plan.target.final_kind == EndpointFinalKind::Item
        })
        .map(|(index, _)| index)
        .collect();
    let eligible_plans: Vec<EdgeRoutePlan> = eligible_indices
        .iter()
        .map(|index| plans[*index].clone())
        .collect();
    let mut endpoint_demands_by_owner: HashMap<String, Vec<EndpointDemandEntry>> = HashMap::new();
    for plan_index in &eligible_indices {
        let plan = &plans[*plan_index];
        let (Some(source_id), Some(target_id)) =
            (plan.edge.start.as_deref(), plan.edge.end.as_deref())
        else {
            continue;
        };
        let (Some(source), Some(target)) =
            (node_index(result, source_id), node_index(result, target_id))
        else {
            continue;
        };
        endpoint_demands_by_owner
            .entry(source_id.to_string())
            .or_default()
            .push(EndpointDemandEntry {
                plan_index: *plan_index,
                role: EndpointRole::Source,
                opposite_index: target,
            });
        endpoint_demands_by_owner
            .entry(target_id.to_string())
            .or_default()
            .push(EndpointDemandEntry {
                plan_index: *plan_index,
                role: EndpointRole::Target,
                opposite_index: source,
            });
    }
    let endpoint_assignments_by_owner: HashMap<
        String,
        HashMap<(usize, EndpointRole), EndpointSlotAssignment>,
    > = endpoint_demands_by_owner
        .iter()
        .map(|(owner_id, demands)| {
            let assignments = node(nodes, result, owner_id)
                .map(|owner| allocate_endpoint_slots(owner, demands, &plans, nodes))
                .unwrap_or_default();
            (owner_id.clone(), assignments)
        })
        .collect();
    let mut coordinated_endpoint_ids: HashSet<String> = HashSet::new();
    let mut coordinated_owner_ids: HashSet<String> = HashSet::new();
    for (owner_id, demands) in &endpoint_demands_by_owner {
        let Some(owner) = node(nodes, result, owner_id) else {
            continue;
        };
        let has_aligned = demands
            .iter()
            .any(|demand| is_axis_aligned_demand(owner, *demand, nodes));
        let has_unaligned = demands
            .iter()
            .any(|demand| !is_axis_aligned_demand(owner, *demand, nodes));
        if has_aligned && has_unaligned {
            coordinated_owner_ids.insert(owner_id.clone());
            // The aligned edge already has its straight route, so only the others need the sparse
            // search.
            for demand in demands {
                if !is_axis_aligned_demand(owner, *demand, nodes) {
                    coordinated_endpoint_ids.insert(plans[demand.plan_index].edge.id.clone());
                }
            }
        }
    }
    let mut endpoint_candidates_by_edge: HashMap<String, EdgeRouteCandidates> = HashMap::new();
    for plan_index in &eligible_indices {
        let plan = &plans[*plan_index];
        endpoint_candidates_by_edge.insert(
            plan.edge.id.clone(),
            EdgeRouteCandidates {
                sources: endpoint_candidates(
                    *plan_index,
                    plan,
                    EndpointRole::Source,
                    &plans,
                    &endpoint_demands_by_owner,
                    &endpoint_assignments_by_owner,
                    &coordinated_owner_ids,
                    result,
                    nodes,
                )?,
                targets: endpoint_candidates(
                    *plan_index,
                    plan,
                    EndpointRole::Target,
                    &plans,
                    &endpoint_demands_by_owner,
                    &endpoint_assignments_by_owner,
                    &coordinated_owner_ids,
                    result,
                    nodes,
                )?,
            },
        );
    }
    let endpoint_candidate_products: HashMap<String, usize> = endpoint_candidates_by_edge
        .iter()
        .map(|(edge_id, candidates)| {
            (
                edge_id.clone(),
                candidates.sources.len() * candidates.targets.len(),
            )
        })
        .collect();
    let route_class = |plan: &EdgeRoutePlan| -> usize {
        let source = plan
            .edge
            .start
            .as_deref()
            .and_then(|id| node(nodes, result, id));
        let target = plan
            .edge
            .end
            .as_deref()
            .and_then(|id| node(nodes, result, id));
        if plan.edge.start == plan.edge.end {
            return 0;
        }
        let has_ancestor_endpoint = matches!(
            (source, target),
            (Some(source), Some(target))
                if (source.is_group
                    && is_ancestor_group(
                        &source.id,
                        plan.edge
                            .end
                            .as_deref()
                            .and_then(|id| node_index(result, id)),
                        nodes,
                        &result.forest.node_by_id,
                    ))
                    || (target.is_group
                        && is_ancestor_group(
                            &target.id,
                            plan.edge
                                .start
                                .as_deref()
                                .and_then(|id| node_index(result, id)),
                            nodes,
                            &result.forest.node_by_id,
                        ))
        );
        if has_ancestor_endpoint {
            return 1;
        }
        let source_parent = source
            .and_then(|node| node.parent_id.clone())
            .unwrap_or_else(|| ROOT_CONTAINER_ID.to_string());
        let target_parent = target
            .and_then(|node| node.parent_id.clone())
            .unwrap_or_else(|| ROOT_CONTAINER_ID.to_string());
        if source_parent != target_parent {
            return 2;
        }
        if pair_counts
            .get(&endpoint_pair_key(&plan.edge))
            .copied()
            .unwrap_or(0)
            > 1
        {
            3
        } else {
            4
        }
    };
    let mut ordered_plans = plans.clone();
    ordered_plans.sort_by(|a, b| {
        let a_unordered = endpoint_pair_key(&a.edge);
        let b_unordered = endpoint_pair_key(&b.edge);
        let a_directed = format!(
            "{}|{}",
            a.edge.start.as_deref().unwrap_or(""),
            a.edge.end.as_deref().unwrap_or("")
        );
        let b_directed = format!(
            "{}|{}",
            b.edge.start.as_deref().unwrap_or(""),
            b.edge.end.as_deref().unwrap_or("")
        );
        let a_boundary_count = a.source.chain.len() + a.target.chain.len() - 2;
        let b_boundary_count = b.source.chain.len() + b.target.chain.len() - 2;
        route_class(a)
            .cmp(&route_class(b))
            .then_with(|| b_boundary_count.cmp(&a_boundary_count))
            .then_with(|| {
                endpoint_candidate_products
                    .get(&a.edge.id)
                    .copied()
                    .unwrap_or(0)
                    .cmp(
                        &endpoint_candidate_products
                            .get(&b.edge.id)
                            .copied()
                            .unwrap_or(0),
                    )
            })
            .then_with(|| compare_code_units(&a_unordered, &b_unordered))
            .then_with(|| compare_code_units(&a_directed, &b_directed))
            .then_with(|| compare_code_units(&a.edge.id, &b.edge.id))
    });
    let eligible_ids: HashSet<String> = eligible_plans
        .iter()
        .map(|plan| plan.edge.id.clone())
        .collect();
    let demand_coords =
        assign_demand_coordinates(&ordered_plans, result, nodes, &endpoint_incident_counts);
    // The eligible plans are cloned from `plans`; keep their plan indices available to callers by
    // rebuilding the id order from the original plan list.
    Ok(PreparedEdgeRoutes {
        plans,
        eligible_plans,
        eligible_ids,
        ordered_plans,
        demand_coords,
        endpoint_candidates_by_edge,
        endpoint_incident_counts,
        coordinated_endpoint_ids,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::grid::groups::build_grid_forest;
    use crate::layout::grid::label_spacing::GridLabelRequirements;
    use crate::layout::grid::placement::read_grid_config;
    use crate::layout::grid::router_topology::derive_portal_ranges;

    fn node(id: &str, x: f64, y: f64) -> Node {
        Node {
            id: id.to_string(),
            x: Some(x),
            y: Some(y),
            width: Some(40.0),
            height: Some(40.0),
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

    fn layout_result(nodes: &[Node], data: &LayoutData) -> GridLayoutResult {
        GridLayoutResult {
            forest: build_grid_forest(nodes).unwrap(),
            config: read_grid_config(data),
            containers: HashMap::new(),
            item_meta: HashMap::new(),
            source_order: HashMap::new(),
            label_requirements: GridLabelRequirements::default(),
        }
    }

    #[test]
    fn keeps_compact_portal_coordinates_within_their_available_interval() {
        assert_eq!(
            assign_compact_portal_coordinates(&[0.0, 10.0, 0.0], 0.0, 10.0),
            vec![0.0, 6.0, 10.0]
        );
    }

    #[test]
    fn keeps_demand_identities_distinct_when_edge_and_owner_ids_contain_colons() {
        let nodes = vec![
            node("b", 0.0, 0.0),
            node("target-1", 100.0, 0.0),
            node("source:b", 0.0, 100.0),
            node("target-2", 100.0, 100.0),
        ];
        let edges = vec![
            edge("a:source", "b", "target-1"),
            edge("a", "source:b", "target-2"),
        ];
        let layout = LayoutData {
            nodes: nodes.clone(),
            edges,
            ..LayoutData::default()
        };
        let result = layout_result(&nodes, &layout);

        let prepared = prepare_edge_routes(&layout, &result).unwrap();
        let demand_keys: Vec<String> = prepared
            .plans
            .iter()
            .flat_map(|plan| {
                plan.source
                    .chain
                    .iter()
                    .chain(plan.target.chain.iter())
                    .map(|entry| entry.demand_key.clone())
                    .collect::<Vec<_>>()
            })
            .collect();

        assert_eq!(demand_keys.iter().collect::<HashSet<_>>().len(), 4);
        assert_eq!(prepared.demand_coords.len(), 4);
        assert!(
            demand_keys
                .iter()
                .all(|key| prepared.demand_coords.contains_key(key))
        );
    }

    #[test]
    fn keeps_clustered_endpoint_coordinates_within_the_preferred_side_interval() {
        let nodes = vec![
            node("owner", 0.0, 0.0),
            node("upper", 100.0, -100.0),
            node("lower-1", 100.0, 100.0),
            node("lower-2", 100.0, 100.0),
        ];
        let edges = vec![
            edge("upper-edge", "owner", "upper"),
            edge("lower-edge-1", "owner", "lower-1"),
            edge("lower-edge-2", "owner", "lower-2"),
        ];
        let layout = LayoutData {
            nodes: nodes.clone(),
            edges: edges.clone(),
            ..LayoutData::default()
        };
        let result = layout_result(&nodes, &layout);

        let prepared = prepare_edge_routes(&layout, &result).unwrap();
        let source_candidates: Vec<&EndpointCandidate> = edges
            .iter()
            .map(|edge| &prepared.endpoint_candidates_by_edge[&edge.id].sources[0])
            .collect();

        assert_eq!(
            source_candidates
                .iter()
                .map(|candidate| candidate.side)
                .collect::<Vec<_>>(),
            vec![GridSide::Right, GridSide::Right, GridSide::Right]
        );
        let mut ys: Vec<f64> = source_candidates
            .iter()
            .map(|candidate| candidate.port.y)
            .collect();
        ys.sort_by(f64::total_cmp);
        assert_eq!(ys, vec![-14.0, 10.0, 14.0]);
    }

    #[test]
    fn keeps_clustered_hierarchy_portals_separated_inside_the_topology_range() {
        let owner = Node {
            id: "owner".to_string(),
            is_group: true,
            label: Some("Owner".to_string()),
            x: Some(0.0),
            y: Some(0.0),
            width: Some(40.0),
            height: Some(40.0),
            ..Node::default()
        };
        let nodes = vec![
            owner,
            Node {
                parent_id: Some("owner".to_string()),
                ..node("source-1", 0.0, -15.0)
            },
            Node {
                parent_id: Some("owner".to_string()),
                ..node("source-2", 0.0, -15.0)
            },
            node("target-1", 100.0, -15.0),
            node("target-2", 100.0, -15.0),
        ];
        let edges = vec![
            edge("edge-1", "source-1", "target-1"),
            edge("edge-2", "source-2", "target-2"),
        ];
        let layout = LayoutData {
            nodes: nodes.clone(),
            edges,
            ..LayoutData::default()
        };
        let result = layout_result(&nodes, &layout);

        let prepared = prepare_edge_routes(&layout, &result).unwrap();
        let mut coordinates: Vec<f64> = prepared
            .plans
            .iter()
            .filter_map(|plan| {
                plan.source
                    .chain
                    .iter()
                    .find(|entry| entry.owner_id == "owner")
                    .map(|entry| prepared.demand_coords[&entry.demand_key])
            })
            .collect();
        coordinates.sort_by(f64::total_cmp);
        let range = derive_portal_ranges(
            "owner",
            crate::layout::grid::types::RouterRect {
                left: -20.0,
                right: 20.0,
                top: -20.0,
                bottom: 20.0,
            },
            None,
        )
        .unwrap()
        .into_iter()
        .find(|range| range.side == GridSide::Right)
        .unwrap();

        assert_eq!(coordinates, vec![-14.0, -10.0]);
        assert!(
            coordinates
                .iter()
                .all(|coordinate| *coordinate >= range.low && *coordinate <= range.high)
        );
        assert!(coordinates[1] - coordinates[0] >= 4.0);
    }
}
