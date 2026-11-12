//! Port of Mermaid 12.3.0's
//! `rendering-util/layout-algorithms/grid/routerCompatibility.ts`.
//!
//! Deterministic corridor routing remains the validated fast path and the resource-limit fallback
//! while sparse visibility routing is incrementally adopted for harder routes.
//!
//! Upstream `Node` references become indices resolved through [`GridLayoutResult::forest`] against
//! the layout node slice. Metrics are threaded as `Option<&mut GridRoutingInstrumentation>` because
//! the routing session (part C) owns the instrumentation; the context does not alias it.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::layout::grid::geometry::{
    Rect, manhattan_length, normalize_polyline, polyline_intersects_rect, rect_for_node,
};
use crate::layout::grid::groups::is_ancestor_group;
use crate::layout::grid::label_geometry::{
    GRID_LABEL_CLEARANCE, GRID_LABEL_SPAN_MARGIN, GridLabelSpanRequirement,
};
use crate::layout::grid::router::GridRoutingTestOptions;
use crate::layout::grid::router_constraint::{
    router_rect, validate_container_segment, validate_same_container_route,
};
use crate::layout::grid::router_errors::GridRoutingError;
use crate::layout::grid::router_instrumentation::{
    GridRoutingFallbackReason, GridRoutingInstrumentation,
};
use crate::layout::grid::router_occupancy::RouteOccupancyIndex;
use crate::layout::grid::router_planning::{
    EdgeEndpointPlan, EdgeRoutePlan, EndpointFinalKind, LANE_SEPARATION_PX, PreparedEdgeRoutes,
    TERMINAL_APPROACH_PX, owner_group_title, owner_side_key,
};
use crate::layout::grid::router_topology::{
    BuildTopologyOptions, ContainerTopologyInput, DEFAULT_MAX_ROUTING_ESTIMATED_BYTES,
    ROUTE_CLEARANCE_PX, RouterObstacleInput, build_container_routing_topology,
    derive_portal_ranges,
};
use crate::layout::grid::types::{
    Coordinate, GridAttachment, GridContainerId, GridLayoutResult, GridRoutingContext, GridSide,
    Node, PairedPortal, PortalRange, ROOT_CONTAINER_ID, RouterPoint, RouterRect,
    is_edge_label_node,
};

const ROOT_OUTER_MARGIN: f64 = 24.0;

pub const SELF_LOOP_PORT_GAP: f64 = 18.0;

const SELF_LOOP_APPROACH: f64 = 14.0;

// validateLayout rejects endpoints within 3 px of a node corner.
const SELF_LOOP_CORNER_CLEARANCE: f64 = 4.0;

const SELF_LOOP_TRACK_GAP: f64 = 18.0;

pub const SELF_LOOP_PORT_OFFSET_STEP: f64 = 4.0;

const MAX_PLANS_FOR_OVERLAP_AVOIDANCE: usize = 64;
const MAX_FAST_ROUTE_SEPARATION_STEPS: usize = 4;

#[derive(Clone, Debug, Default)]
pub struct PreparedRoutingModes {
    pub overlap_avoidance_enabled: bool,
    pub compatibility_fast_routes: HashMap<String, Vec<RouterPoint>>,
    pub sparse_hierarchy_ids: HashSet<String>,
    pub sparse_lca_ids: HashSet<String>,
    pub routed_container_ids: Vec<GridContainerId>,
    pub route_occupancy: HashMap<GridContainerId, RouteOccupancyIndex>,
}

/// Moves one interior segment of a minimal corridor route sideways until it no longer shares a
/// corridor with another edge's route. Shifting an interior segment keeps both terminal stubs and
/// the Manhattan length, so the result is accepted only if it still passes the caller's validation.
fn separate_route_from_occupied(
    route: &[RouterPoint],
    occupancy: &RouteOccupancyIndex,
    pair_key: &str,
    is_acceptable: impl Fn(&[RouterPoint]) -> bool,
) -> Option<Vec<RouterPoint>> {
    if !occupancy.conflicts_with_route(route, pair_key) {
        return Some(route.to_vec());
    }
    let points = normalize_polyline(route).points;
    for step in 1..=MAX_FAST_ROUTE_SEPARATION_STEPS {
        for sign in [1.0_f64, -1.0_f64] {
            let delta = sign * step as f64 * LANE_SEPARATION_PX;
            for index in 1..points.len().saturating_sub(2) {
                let vertical = points[index].x == points[index + 1].x;
                let shifted: Vec<RouterPoint> = points
                    .iter()
                    .enumerate()
                    .map(|(point_index, point)| {
                        if point_index == index || point_index == index + 1 {
                            if vertical {
                                RouterPoint {
                                    x: point.x + delta,
                                    y: point.y,
                                }
                            } else {
                                RouterPoint {
                                    x: point.x,
                                    y: point.y + delta,
                                }
                            }
                        } else {
                            *point
                        }
                    })
                    .collect();
                if !occupancy.conflicts_with_route(&shifted, pair_key) && is_acceptable(&shifted) {
                    return Some(shifted);
                }
            }
        }
    }
    None
}

fn terminal_stubs_are_long_enough(points: &[RouterPoint]) -> bool {
    if points.len() < 2 {
        return false;
    }
    let stub = |a: RouterPoint, b: RouterPoint| (a.x - b.x).abs() + (a.y - b.y).abs();
    let last = points.len() - 1;
    stub(points[0], points[1]) >= TERMINAL_APPROACH_PX
        && stub(points[last], points[last - 1]) >= TERMINAL_APPROACH_PX
}

pub fn bounded_alternative_portal_coordinates(
    selected: f64,
    low: f64,
    high: f64,
    corridors: &[f64],
    limit: usize,
) -> Vec<f64> {
    // Retry the nearest established corridors first, then the range's canonical midpoint and ends.
    // The hard limit bounds hierarchy retry fan-out and keeps output independent of map iteration.
    let mut from_corridors: Vec<f64> = corridors
        .iter()
        .copied()
        .filter(|coordinate| *coordinate >= low && *coordinate <= high)
        .collect();
    from_corridors.sort_by(|a, b| {
        (a - selected)
            .abs()
            .total_cmp(&(b - selected).abs())
            .then_with(|| a.total_cmp(b))
    });
    let mut candidates = from_corridors;
    candidates.push((low + high) / 2.0);
    candidates.push(low);
    candidates.push(high);
    let mut seen: HashSet<Coordinate> = HashSet::new();
    seen.insert(Coordinate::new(selected));
    let mut alternatives: Vec<f64> = Vec::new();
    for coordinate in candidates {
        let key = Coordinate::new(coordinate);
        if !seen.insert(key) {
            continue;
        }
        alternatives.push(coordinate);
        if alternatives.len() == limit {
            break;
        }
    }
    alternatives
}

fn equal_point(a: RouterPoint, b: RouterPoint) -> bool {
    (a.x - b.x).abs() <= 1e-6 && (a.y - b.y).abs() <= 1e-6
}

pub fn are_exactly_axis_aligned(a: RouterPoint, b: RouterPoint) -> bool {
    a.x == b.x || a.y == b.y
}

fn choose_intermediate_coordinate(values: &[f64], start: f64, end: f64, variant: usize) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| {
        ((a - start).abs() + (a - end).abs())
            .total_cmp(&((b - start).abs() + (b - end).abs()))
            .then_with(|| a.total_cmp(b))
    });
    if sorted.is_empty() {
        return (start + end) / 2.0;
    }
    sorted[variant.min(sorted.len() - 1)]
}

fn dedupe_consecutive(points: &[RouterPoint]) -> Vec<RouterPoint> {
    let mut out: Vec<RouterPoint> = Vec::new();
    for point in points {
        if out
            .last()
            .is_none_or(|previous| !equal_point(*previous, *point))
        {
            out.push(*point);
        }
    }
    out
}

pub fn route_within_container(
    container_id: &str,
    result: &GridLayoutResult,
    start: &GridAttachment,
    end: &GridAttachment,
    variant: usize,
) -> Result<Vec<RouterPoint>, GridRoutingError> {
    // This compatibility router only chooses among measured grid corridors. `variant` selects the
    // next deterministic dogleg when a bundle retry needs a different lane.
    let Some(container) = result.containers.get(container_id) else {
        return Err(GridRoutingError::geometry(format!(
            "GRID_ROUTE_NOT_FOUND: Missing container layout \"{container_id}\""
        )));
    };

    let mut points: Vec<RouterPoint> = vec![start.port, start.connect];
    if start.orientation == crate::layout::grid::types::GridOrientation::V
        && end.orientation == crate::layout::grid::types::GridOrientation::V
    {
        if !equal_point(start.connect, end.connect)
            && (start.connect.x - end.connect.x).abs() > 1e-6
        {
            let y = choose_intermediate_coordinate(
                &container.horizontal_corridors,
                start.connect.y,
                end.connect.y,
                variant,
            );
            points.push(RouterPoint {
                x: start.connect.x,
                y,
            });
            points.push(RouterPoint {
                x: end.connect.x,
                y,
            });
        } else if !equal_point(start.connect, end.connect)
            && (start.connect.y - end.connect.y).abs() > 1e-6
            && variant > 0
        {
            let alternate_corridors: Vec<f64> = container
                .vertical_corridors
                .iter()
                .copied()
                .filter(|value| (value - start.connect.x).abs() > 1e-6)
                .collect();
            let x = choose_intermediate_coordinate(
                if alternate_corridors.is_empty() {
                    &container.vertical_corridors
                } else {
                    &alternate_corridors
                },
                start.connect.x,
                end.connect.x,
                variant - 1,
            );
            points.push(RouterPoint {
                x,
                y: start.connect.y,
            });
            points.push(RouterPoint {
                x,
                y: end.connect.y,
            });
        }
    } else if start.orientation == crate::layout::grid::types::GridOrientation::H
        && end.orientation == crate::layout::grid::types::GridOrientation::H
    {
        if !equal_point(start.connect, end.connect)
            && (start.connect.y - end.connect.y).abs() > 1e-6
        {
            let x = choose_intermediate_coordinate(
                &container.vertical_corridors,
                start.connect.x,
                end.connect.x,
                variant,
            );
            points.push(RouterPoint {
                x,
                y: start.connect.y,
            });
            points.push(RouterPoint {
                x,
                y: end.connect.y,
            });
        }
    } else if start.orientation == crate::layout::grid::types::GridOrientation::V {
        points.push(RouterPoint {
            x: start.connect.x,
            y: end.connect.y,
        });
    } else {
        points.push(RouterPoint {
            x: end.connect.x,
            y: start.connect.y,
        });
    }
    points.push(end.connect);
    points.push(end.port);
    Ok(normalize_polyline(&dedupe_consecutive(&points)).points)
}

fn node<'a>(nodes: &'a [Node], result: &GridLayoutResult, id: &str) -> Option<&'a Node> {
    result
        .forest
        .node_by_id
        .get(id)
        .and_then(|index| nodes.get(*index))
}

#[allow(clippy::too_many_arguments)]
pub fn item_attachment(
    owner_id: &str,
    side: GridSide,
    demand_key: &str,
    container_id: &str,
    result: &GridLayoutResult,
    nodes: &[Node],
    demand_coords: &HashMap<String, f64>,
    coordinate: Option<f64>,
) -> Result<GridAttachment, GridRoutingError> {
    let (Some(owner), Some(item)) = (
        node(nodes, result, owner_id),
        result.item_meta.get(owner_id),
    ) else {
        return Err(GridRoutingError::geometry(format!(
            "GRID_ROUTE_NOT_FOUND: Missing item metadata for \"{owner_id}\" in \"{container_id}\""
        )));
    };
    if item.container_id != container_id {
        return Err(GridRoutingError::geometry(format!(
            "GRID_ROUTE_NOT_FOUND: Missing item metadata for \"{owner_id}\" in \"{container_id}\""
        )));
    }

    let rect = rect_for_node(owner);
    let coord = coordinate
        .or_else(|| demand_coords.get(demand_key).copied())
        .unwrap_or_else(|| {
            if side == GridSide::Left || side == GridSide::Right {
                rect.cy
            } else {
                rect.cx
            }
        });
    Ok(match side {
        GridSide::Left => GridAttachment {
            port: RouterPoint {
                x: rect.left,
                y: coord,
            },
            connect: RouterPoint {
                x: item.left_corridor_x,
                y: coord,
            },
            orientation: crate::layout::grid::types::GridOrientation::V,
            side,
        },
        GridSide::Right => GridAttachment {
            port: RouterPoint {
                x: rect.right,
                y: coord,
            },
            connect: RouterPoint {
                x: item.right_corridor_x,
                y: coord,
            },
            orientation: crate::layout::grid::types::GridOrientation::V,
            side,
        },
        GridSide::Top => GridAttachment {
            port: RouterPoint {
                x: coord,
                y: rect.top,
            },
            connect: RouterPoint {
                x: coord,
                y: item.top_corridor_y,
            },
            orientation: crate::layout::grid::types::GridOrientation::H,
            side,
        },
        GridSide::Bottom => GridAttachment {
            port: RouterPoint {
                x: coord,
                y: rect.bottom,
            },
            connect: RouterPoint {
                x: coord,
                y: item.bottom_corridor_y,
            },
            orientation: crate::layout::grid::types::GridOrientation::H,
            side,
        },
    })
}

pub fn boundary_attachment(
    container_id: &str,
    side: GridSide,
    demand_key: &str,
    result: &GridLayoutResult,
    nodes: &[Node],
    demand_coords: &HashMap<String, f64>,
) -> Result<GridAttachment, GridRoutingError> {
    let (Some(owner), Some(container)) = (
        node(nodes, result, container_id),
        result.containers.get(container_id),
    ) else {
        return Err(GridRoutingError::geometry(format!(
            "GRID_ROUTE_NOT_FOUND: Missing boundary metadata for \"{container_id}\""
        )));
    };
    let rect = rect_for_node(owner);
    let coord = demand_coords.get(demand_key).copied().unwrap_or_else(|| {
        if side == GridSide::Left || side == GridSide::Right {
            rect.cy
        } else {
            rect.cx
        }
    });
    Ok(match side {
        GridSide::Left => GridAttachment {
            port: RouterPoint {
                x: rect.left,
                y: coord,
            },
            connect: RouterPoint {
                x: container.vertical_corridors[0],
                y: coord,
            },
            orientation: crate::layout::grid::types::GridOrientation::V,
            side,
        },
        GridSide::Right => GridAttachment {
            port: RouterPoint {
                x: rect.right,
                y: coord,
            },
            connect: RouterPoint {
                x: container.vertical_corridors[container.vertical_corridors.len() - 1],
                y: coord,
            },
            orientation: crate::layout::grid::types::GridOrientation::V,
            side,
        },
        GridSide::Top => GridAttachment {
            port: RouterPoint {
                x: coord,
                y: rect.top,
            },
            connect: RouterPoint {
                x: coord,
                y: container.horizontal_corridors[0],
            },
            orientation: crate::layout::grid::types::GridOrientation::H,
            side,
        },
        GridSide::Bottom => GridAttachment {
            port: RouterPoint {
                x: coord,
                y: rect.bottom,
            },
            connect: RouterPoint {
                x: coord,
                y: container.horizontal_corridors[container.horizontal_corridors.len() - 1],
            },
            orientation: crate::layout::grid::types::GridOrientation::H,
            side,
        },
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct SegmentAttachment {
    pub owner_id: String,
    pub port: RouterPoint,
    pub connect: RouterPoint,
    pub orientation: crate::layout::grid::types::GridOrientation,
    pub side: GridSide,
}

pub struct SegmentAttachmentAlternative {
    pub attachment: SegmentAttachment,
    pub select: Box<dyn FnMut()>,
}

impl SegmentAttachment {
    pub fn as_attachment(&self) -> GridAttachment {
        GridAttachment {
            port: self.port,
            connect: self.connect,
            orientation: self.orientation,
            side: self.side,
        }
    }
}

fn portal_boundary_point(portal: &PairedPortal) -> RouterPoint {
    RouterPoint {
        x: (portal.interior.x + portal.exterior.x) / 2.0,
        y: (portal.interior.y + portal.exterior.y) / 2.0,
    }
}

pub fn portal_attachment(portal: &PairedPortal, interior: bool) -> SegmentAttachment {
    // The visible port is the group boundary midpoint; search starts on one side of the clearance
    // transition so each container topology remains independent.
    SegmentAttachment {
        owner_id: portal.owner_id.clone(),
        port: portal_boundary_point(portal),
        connect: if interior {
            portal.interior
        } else {
            portal.exterior
        },
        orientation: if portal.side == GridSide::Left || portal.side == GridSide::Right {
            crate::layout::grid::types::GridOrientation::V
        } else {
            crate::layout::grid::types::GridOrientation::H
        },
        side: portal.side,
    }
}

pub fn group_boundary_endpoint_attachment(
    owner_id: &str,
    side: GridSide,
    demand_key: &str,
    result: &GridLayoutResult,
    nodes: &[Node],
    demand_coords: &HashMap<String, f64>,
) -> Result<SegmentAttachment, GridRoutingError> {
    let Some(owner) = node(nodes, result, owner_id) else {
        return Err(GridRoutingError::geometry(format!(
            "GRID_ROUTE_NOT_FOUND: Missing group endpoint \"{owner_id}\""
        )));
    };
    if !owner.is_group {
        return Err(GridRoutingError::geometry(format!(
            "GRID_ROUTE_NOT_FOUND: Missing group endpoint \"{owner_id}\""
        )));
    }
    let rect = rect_for_node(owner);
    let coordinate = demand_coords.get(demand_key).copied().unwrap_or_else(|| {
        if side == GridSide::Left || side == GridSide::Right {
            rect.cy
        } else {
            rect.cx
        }
    });
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
    Ok(SegmentAttachment {
        owner_id: owner_id.to_string(),
        port,
        connect: RouterPoint {
            x: port.x
                + match side {
                    GridSide::Left => TERMINAL_APPROACH_PX,
                    GridSide::Right => -TERMINAL_APPROACH_PX,
                    _ => 0.0,
                },
            y: port.y
                + match side {
                    GridSide::Top => TERMINAL_APPROACH_PX,
                    GridSide::Bottom => -TERMINAL_APPROACH_PX,
                    _ => 0.0,
                },
        },
        orientation: if side == GridSide::Left || side == GridSide::Right {
            crate::layout::grid::types::GridOrientation::V
        } else {
            crate::layout::grid::types::GridOrientation::H
        },
        side,
    })
}

pub fn reverse_points(points: &[RouterPoint]) -> Vec<RouterPoint> {
    points.iter().rev().copied().collect()
}

pub fn combine_point_chains(chains: &[Vec<RouterPoint>]) -> Vec<RouterPoint> {
    let mut points: Vec<RouterPoint> = Vec::new();
    for chain in chains {
        for point in chain {
            if points
                .last()
                .is_none_or(|previous| !equal_point(*previous, *point))
            {
                points.push(*point);
            }
        }
    }
    normalize_polyline(&points).points
}

fn route_self_loop(
    owner: &Node,
    side: GridSide,
    index: usize,
    demand_coord: f64,
) -> Vec<RouterPoint> {
    let rect = rect_for_node(owner);
    let depth = SELF_LOOP_APPROACH + (index as f64 + 1.0) * SELF_LOOP_TRACK_GAP;
    let port_offset = index as f64 * SELF_LOOP_PORT_OFFSET_STEP;
    match side {
        GridSide::Right => {
            let start_y = demand_coord - SELF_LOOP_PORT_GAP / 2.0 - port_offset;
            let end_y = demand_coord + SELF_LOOP_PORT_GAP / 2.0 + port_offset;
            normalize_polyline(&[
                RouterPoint {
                    x: rect.right,
                    y: start_y,
                },
                RouterPoint {
                    x: rect.right + SELF_LOOP_APPROACH,
                    y: start_y,
                },
                RouterPoint {
                    x: rect.right + depth,
                    y: start_y,
                },
                RouterPoint {
                    x: rect.right + depth,
                    y: end_y,
                },
                RouterPoint {
                    x: rect.right + SELF_LOOP_APPROACH,
                    y: end_y,
                },
                RouterPoint {
                    x: rect.right,
                    y: end_y,
                },
            ])
            .points
        }
        GridSide::Left => {
            let start_y = demand_coord - SELF_LOOP_PORT_GAP / 2.0 - port_offset;
            let end_y = demand_coord + SELF_LOOP_PORT_GAP / 2.0 + port_offset;
            normalize_polyline(&[
                RouterPoint {
                    x: rect.left,
                    y: start_y,
                },
                RouterPoint {
                    x: rect.left - SELF_LOOP_APPROACH,
                    y: start_y,
                },
                RouterPoint {
                    x: rect.left - depth,
                    y: start_y,
                },
                RouterPoint {
                    x: rect.left - depth,
                    y: end_y,
                },
                RouterPoint {
                    x: rect.left - SELF_LOOP_APPROACH,
                    y: end_y,
                },
                RouterPoint {
                    x: rect.left,
                    y: end_y,
                },
            ])
            .points
        }
        GridSide::Top => {
            let start_x = demand_coord - SELF_LOOP_PORT_GAP / 2.0 - port_offset;
            let end_x = demand_coord + SELF_LOOP_PORT_GAP / 2.0 + port_offset;
            normalize_polyline(&[
                RouterPoint {
                    x: start_x,
                    y: rect.top,
                },
                RouterPoint {
                    x: start_x,
                    y: rect.top - SELF_LOOP_APPROACH,
                },
                RouterPoint {
                    x: start_x,
                    y: rect.top - depth,
                },
                RouterPoint {
                    x: end_x,
                    y: rect.top - depth,
                },
                RouterPoint {
                    x: end_x,
                    y: rect.top - SELF_LOOP_APPROACH,
                },
                RouterPoint {
                    x: end_x,
                    y: rect.top,
                },
            ])
            .points
        }
        GridSide::Bottom => {
            let start_x = demand_coord - SELF_LOOP_PORT_GAP / 2.0 - port_offset;
            let end_x = demand_coord + SELF_LOOP_PORT_GAP / 2.0 + port_offset;
            normalize_polyline(&[
                RouterPoint {
                    x: start_x,
                    y: rect.bottom,
                },
                RouterPoint {
                    x: start_x,
                    y: rect.bottom + SELF_LOOP_APPROACH,
                },
                RouterPoint {
                    x: start_x,
                    y: rect.bottom + depth,
                },
                RouterPoint {
                    x: end_x,
                    y: rect.bottom + depth,
                },
                RouterPoint {
                    x: end_x,
                    y: rect.bottom + SELF_LOOP_APPROACH,
                },
                RouterPoint {
                    x: end_x,
                    y: rect.bottom,
                },
            ])
            .points
        }
    }
}

pub fn label_aware_self_loop_candidates(
    owner: &Node,
    side: GridSide,
    index: usize,
    demand_coord: f64,
    requirement: &GridLabelSpanRequirement,
) -> Vec<Vec<RouterPoint>> {
    let rect = rect_for_node(owner);
    let base_depth = SELF_LOOP_APPROACH + (index as f64 + 1.0) * SELF_LOOP_TRACK_GAP;
    let port_offset = index as f64 * SELF_LOOP_PORT_OFFSET_STEP;
    let base_span = SELF_LOOP_PORT_GAP + port_offset * 2.0;
    let vertical_side = side == GridSide::Left || side == GridSide::Right;
    let span = base_span.max(
        (if vertical_side {
            requirement.cross
        } else {
            requirement.along
        }) + GRID_LABEL_CLEARANCE * 2.0
            + GRID_LABEL_SPAN_MARGIN,
    );
    let depth = base_depth.max(
        (if vertical_side {
            requirement.along
        } else {
            requirement.cross
        }) / 2.0
            + GRID_LABEL_CLEARANCE,
    );
    let interval = if vertical_side {
        [rect.top, rect.bottom]
    } else {
        [rect.left, rect.right]
    };
    let direct_start = demand_coord - span / 2.0;
    let direct_end = demand_coord + span / 2.0;

    if direct_start - interval[0] >= SELF_LOOP_CORNER_CLEARANCE
        && interval[1] - direct_end >= SELF_LOOP_CORNER_CLEARANCE
    {
        return match side {
            GridSide::Right => vec![
                normalize_polyline(&[
                    RouterPoint {
                        x: rect.right,
                        y: direct_start,
                    },
                    RouterPoint {
                        x: rect.right + depth,
                        y: direct_start,
                    },
                    RouterPoint {
                        x: rect.right + depth,
                        y: direct_end,
                    },
                    RouterPoint {
                        x: rect.right,
                        y: direct_end,
                    },
                ])
                .points,
            ],
            GridSide::Left => vec![
                normalize_polyline(&[
                    RouterPoint {
                        x: rect.left,
                        y: direct_start,
                    },
                    RouterPoint {
                        x: rect.left - depth,
                        y: direct_start,
                    },
                    RouterPoint {
                        x: rect.left - depth,
                        y: direct_end,
                    },
                    RouterPoint {
                        x: rect.left,
                        y: direct_end,
                    },
                ])
                .points,
            ],
            GridSide::Top => vec![
                normalize_polyline(&[
                    RouterPoint {
                        x: direct_start,
                        y: rect.top,
                    },
                    RouterPoint {
                        x: direct_start,
                        y: rect.top - depth,
                    },
                    RouterPoint {
                        x: direct_end,
                        y: rect.top - depth,
                    },
                    RouterPoint {
                        x: direct_end,
                        y: rect.top,
                    },
                ])
                .points,
            ],
            GridSide::Bottom => vec![
                normalize_polyline(&[
                    RouterPoint {
                        x: direct_start,
                        y: rect.bottom,
                    },
                    RouterPoint {
                        x: direct_start,
                        y: rect.bottom + depth,
                    },
                    RouterPoint {
                        x: direct_end,
                        y: rect.bottom + depth,
                    },
                    RouterPoint {
                        x: direct_end,
                        y: rect.bottom,
                    },
                ])
                .points,
            ],
        };
    }

    let port_start = demand_coord - SELF_LOOP_PORT_GAP / 2.0 - port_offset;
    let port_end = demand_coord + SELF_LOOP_PORT_GAP / 2.0 + port_offset;
    if port_start < interval[0] || port_end > interval[1] {
        return Vec::new();
    }
    let outer_start = demand_coord - span / 2.0;
    let outer_end = demand_coord + span / 2.0;
    let shoulder_approach = SELF_LOOP_APPROACH.max(TERMINAL_APPROACH_PX);
    // Keep the label clear of the return shoulders as well as the owner.
    let shoulder_depth = depth.max(
        shoulder_approach
            + (if vertical_side {
                requirement.along
            } else {
                requirement.cross
            }) / 2.0
            + GRID_LABEL_CLEARANCE,
    );
    match side {
        GridSide::Right => vec![
            normalize_polyline(&[
                RouterPoint {
                    x: rect.right,
                    y: port_start,
                },
                RouterPoint {
                    x: rect.right + shoulder_approach,
                    y: port_start,
                },
                RouterPoint {
                    x: rect.right + shoulder_approach,
                    y: outer_start,
                },
                RouterPoint {
                    x: rect.right + shoulder_depth,
                    y: outer_start,
                },
                RouterPoint {
                    x: rect.right + shoulder_depth,
                    y: outer_end,
                },
                RouterPoint {
                    x: rect.right + shoulder_approach,
                    y: outer_end,
                },
                RouterPoint {
                    x: rect.right + shoulder_approach,
                    y: port_end,
                },
                RouterPoint {
                    x: rect.right,
                    y: port_end,
                },
            ])
            .points,
        ],
        GridSide::Left => vec![
            normalize_polyline(&[
                RouterPoint {
                    x: rect.left,
                    y: port_start,
                },
                RouterPoint {
                    x: rect.left - shoulder_approach,
                    y: port_start,
                },
                RouterPoint {
                    x: rect.left - shoulder_approach,
                    y: outer_start,
                },
                RouterPoint {
                    x: rect.left - shoulder_depth,
                    y: outer_start,
                },
                RouterPoint {
                    x: rect.left - shoulder_depth,
                    y: outer_end,
                },
                RouterPoint {
                    x: rect.left - shoulder_approach,
                    y: outer_end,
                },
                RouterPoint {
                    x: rect.left - shoulder_approach,
                    y: port_end,
                },
                RouterPoint {
                    x: rect.left,
                    y: port_end,
                },
            ])
            .points,
        ],
        GridSide::Top => vec![
            normalize_polyline(&[
                RouterPoint {
                    x: port_start,
                    y: rect.top,
                },
                RouterPoint {
                    x: port_start,
                    y: rect.top - shoulder_approach,
                },
                RouterPoint {
                    x: outer_start,
                    y: rect.top - shoulder_approach,
                },
                RouterPoint {
                    x: outer_start,
                    y: rect.top - shoulder_depth,
                },
                RouterPoint {
                    x: outer_end,
                    y: rect.top - shoulder_depth,
                },
                RouterPoint {
                    x: outer_end,
                    y: rect.top - shoulder_approach,
                },
                RouterPoint {
                    x: port_end,
                    y: rect.top - shoulder_approach,
                },
                RouterPoint {
                    x: port_end,
                    y: rect.top,
                },
            ])
            .points,
        ],
        GridSide::Bottom => vec![
            normalize_polyline(&[
                RouterPoint {
                    x: port_start,
                    y: rect.bottom,
                },
                RouterPoint {
                    x: port_start,
                    y: rect.bottom + shoulder_approach,
                },
                RouterPoint {
                    x: outer_start,
                    y: rect.bottom + shoulder_approach,
                },
                RouterPoint {
                    x: outer_start,
                    y: rect.bottom + shoulder_depth,
                },
                RouterPoint {
                    x: outer_end,
                    y: rect.bottom + shoulder_depth,
                },
                RouterPoint {
                    x: outer_end,
                    y: rect.bottom + shoulder_approach,
                },
                RouterPoint {
                    x: port_end,
                    y: rect.bottom + shoulder_approach,
                },
                RouterPoint {
                    x: port_end,
                    y: rect.bottom,
                },
            ])
            .points,
        ],
    }
}

pub fn ordered_self_loop_sides(
    node: &Node,
    counts: &HashMap<String, usize>,
    self_loop_counts: &HashMap<String, usize>,
) -> Vec<GridSide> {
    let mut sides: Vec<GridSide> = if node.is_group && owner_group_title(node) {
        vec![GridSide::Left, GridSide::Right, GridSide::Bottom]
    } else {
        vec![
            GridSide::Left,
            GridSide::Right,
            GridSide::Top,
            GridSide::Bottom,
        ]
    };
    sides.sort_by(|a, b| {
        let count = |side: GridSide| -> usize {
            counts
                .get(&owner_side_key(&node.id, side))
                .copied()
                .unwrap_or(0)
                + self_loop_counts
                    .get(&format!("{}:{}", node.id, side.as_str()))
                    .copied()
                    .unwrap_or(0)
        };
        count(*a).cmp(&count(*b))
    });
    sides
}

fn self_loop_obstacles(owner: &Node, result: &GridLayoutResult, nodes: &[Node]) -> Vec<Rect> {
    let mut obstacles: Vec<Rect> = Vec::new();
    for index in result.forest.node_by_id.values() {
        let node = &nodes[*index];
        if node.id != owner.id && !node.is_group && node.x.is_some() && node.y.is_some() {
            obstacles.push(rect_for_node(node));
        }
        if let Some(title) = node.group_title_rect.filter(|_| node.id != owner.id) {
            obstacles.push(Rect {
                cx: (title.left + title.right) / 2.0,
                cy: (title.top + title.bottom) / 2.0,
                left: title.left,
                right: title.right,
                top: title.top,
                bottom: title.bottom,
            });
        }
    }
    obstacles
}

#[derive(Clone, Debug, PartialEq)]
pub struct SelfLoopRoute {
    pub points: Vec<RouterPoint>,
    pub side: GridSide,
    pub index: usize,
}

pub fn route_obstacle_clear_self_loop(
    owner: &Node,
    owner_side_counts: &HashMap<String, usize>,
    self_loop_counts: &HashMap<String, usize>,
    result: &GridLayoutResult,
    nodes: &[Node],
    label_requirement: Option<&GridLabelSpanRequirement>,
    mut metrics: Option<&mut GridRoutingInstrumentation>,
) -> Result<SelfLoopRoute, GridRoutingError> {
    // Side load determines preference, but fixed side ordering breaks ties deterministically.
    let rect = rect_for_node(owner);
    let obstacles = self_loop_obstacles(owner, result, nodes);
    for side in ordered_self_loop_sides(owner, owner_side_counts, self_loop_counts) {
        let count_key = format!("{}:{}", owner.id, side.as_str());
        let index = self_loop_counts.get(&count_key).copied().unwrap_or(0);
        let demand_coord = if side == GridSide::Left || side == GridSide::Right {
            rect.cy
        } else {
            rect.cx
        };
        let candidates = match label_requirement {
            Some(requirement) => {
                label_aware_self_loop_candidates(owner, side, index, demand_coord, requirement)
            }
            None => vec![route_self_loop(owner, side, index, demand_coord)],
        };
        if let (true, Some(metrics)) = (label_requirement.is_some(), metrics.as_mut()) {
            metrics.label_aware_self_loop_candidates += candidates.len();
        }
        for points in candidates {
            if !obstacles
                .iter()
                .any(|obstacle| polyline_intersects_rect(&points, *obstacle))
            {
                return Ok(SelfLoopRoute {
                    points,
                    side,
                    index,
                });
            }
        }
    }
    Err(GridRoutingError::geometry(format!(
        "GRID_ROUTE_NOT_FOUND: No obstacle-clear self-loop route for \"{}\"",
        owner.id
    )))
}

pub fn root_container_meta(result: &mut GridLayoutResult) {
    if result.containers.contains_key(ROOT_CONTAINER_ID) {
        return;
    }
    let mut min_left = 0.0_f64;
    let mut max_right = 0.0_f64;
    let mut min_top = 0.0_f64;
    let mut max_bottom = 0.0_f64;
    let mut first = true;
    for item in result.item_meta.values() {
        if item.container_id != ROOT_CONTAINER_ID {
            continue;
        }
        if first {
            min_left = item.cell_left;
            max_right = item.cell_left + item.cell_width;
            min_top = item.cell_top;
            max_bottom = item.cell_top + item.cell_height;
            first = false;
        } else {
            min_left = min_left.min(item.cell_left);
            max_right = max_right.max(item.cell_left + item.cell_width);
            min_top = min_top.min(item.cell_top);
            max_bottom = max_bottom.max(item.cell_top + item.cell_height);
        }
    }

    let mut horizontal_corridors: BTreeSet<Coordinate> = BTreeSet::new();
    horizontal_corridors.insert(Coordinate::new(min_top - ROOT_OUTER_MARGIN));
    horizontal_corridors.insert(Coordinate::new(max_bottom + ROOT_OUTER_MARGIN));
    // The synthetic root has no node bounds, so derive its routable domain from root cells and keep
    // an outer track available for routes that must pass around the whole diagram.
    let mut vertical_corridors: BTreeSet<Coordinate> = BTreeSet::new();
    vertical_corridors.insert(Coordinate::new(min_left - ROOT_OUTER_MARGIN));
    vertical_corridors.insert(Coordinate::new(max_right + ROOT_OUTER_MARGIN));
    for item in result.item_meta.values() {
        if item.container_id != ROOT_CONTAINER_ID {
            continue;
        }
        horizontal_corridors.insert(Coordinate::new(item.top_corridor_y));
        horizontal_corridors.insert(Coordinate::new(item.bottom_corridor_y));
        vertical_corridors.insert(Coordinate::new(item.left_corridor_x));
        vertical_corridors.insert(Coordinate::new(item.right_corridor_x));
    }
    result.containers.insert(
        ROOT_CONTAINER_ID.to_string(),
        crate::layout::grid::types::GridContainerLayoutMeta {
            id: ROOT_CONTAINER_ID.to_string(),
            left: min_left - ROOT_OUTER_MARGIN,
            top: min_top - ROOT_OUTER_MARGIN,
            width: max_right - min_left + ROOT_OUTER_MARGIN * 2.0,
            height: max_bottom - min_top + ROOT_OUTER_MARGIN * 2.0,
            content_left: min_left,
            content_top: min_top,
            content_right: max_right,
            content_bottom: max_bottom,
            vertical_corridors: vertical_corridors
                .into_iter()
                .map(Coordinate::value)
                .collect(),
            horizontal_corridors: horizontal_corridors
                .into_iter()
                .map(Coordinate::value)
                .collect(),
        },
    );
}

fn container_bounds(
    container_id: &str,
    result: &GridLayoutResult,
    nodes: &[Node],
) -> Result<RouterRect, GridRoutingError> {
    let Some(container) = result.containers.get(container_id) else {
        return Err(GridRoutingError::geometry(format!(
            "GRID_ROUTE_NOT_FOUND: Missing container layout \"{container_id}\""
        )));
    };
    if container_id == ROOT_CONTAINER_ID {
        let root_items: Vec<&crate::layout::grid::types::GridItemLayoutMeta> = result
            .item_meta
            .values()
            .filter(|item| item.container_id == ROOT_CONTAINER_ID)
            .collect();
        if !root_items.is_empty() {
            let min_left = root_items
                .iter()
                .map(|item| item.cell_left)
                .fold(f64::INFINITY, f64::min);
            let max_right = root_items
                .iter()
                .map(|item| item.cell_left + item.cell_width)
                .fold(f64::NEG_INFINITY, f64::max);
            let min_top = root_items
                .iter()
                .map(|item| item.cell_top)
                .fold(f64::INFINITY, f64::min);
            let max_bottom = root_items
                .iter()
                .map(|item| item.cell_top + item.cell_height)
                .fold(f64::NEG_INFINITY, f64::max);
            return Ok(RouterRect {
                left: min_left - ROOT_OUTER_MARGIN,
                right: max_right + ROOT_OUTER_MARGIN,
                top: min_top - ROOT_OUTER_MARGIN,
                bottom: max_bottom + ROOT_OUTER_MARGIN,
            });
        }
        return Ok(RouterRect {
            left: container.left,
            right: container.left + container.width,
            top: container.top,
            bottom: container.top + container.height,
        });
    }
    let Some(owner) = node(nodes, result, container_id) else {
        return Err(GridRoutingError::geometry(format!(
            "GRID_ROUTE_NOT_FOUND: Missing container owner \"{container_id}\""
        )));
    };
    let owner_bounds = router_rect(owner);
    Ok(RouterRect {
        left: owner_bounds.left + ROUTE_CLEARANCE_PX,
        right: owner_bounds.right - ROUTE_CLEARANCE_PX,
        top: owner_bounds.top + ROUTE_CLEARANCE_PX,
        bottom: owner_bounds.bottom - ROUTE_CLEARANCE_PX,
    })
}

fn portal_coordinate(bounds: RouterRect, side: GridSide, interior: bool) -> f64 {
    let boundary = match side {
        GridSide::Left => bounds.left,
        GridSide::Right => bounds.right,
        GridSide::Top => bounds.top,
        GridSide::Bottom => bounds.bottom,
    };
    let toward_interior = if side == GridSide::Left || side == GridSide::Top {
        1.0
    } else {
        -1.0
    };
    boundary + toward_interior * ROUTE_CLEARANCE_PX * if interior { 1.0 } else { -1.0 }
}

fn title_bounds(node: &Node) -> Option<RouterRect> {
    node.group_title_rect.map(|title| RouterRect {
        left: title.left,
        right: title.right,
        top: title.top,
        bottom: title.bottom,
    })
}

fn container_portal_ranges(
    container_id: &str,
    result: &GridLayoutResult,
    nodes: &[Node],
) -> Result<Vec<PortalRange>, GridRoutingError> {
    let mut ranges: Vec<PortalRange> = Vec::new();
    let owner = if container_id != ROOT_CONTAINER_ID {
        node(nodes, result, container_id)
    } else {
        None
    };
    if let Some(owner) = owner {
        let bounds = router_rect(owner);
        for mut range in derive_portal_ranges(container_id, bounds, title_bounds(owner))? {
            range.coordinate = Some(portal_coordinate(bounds, range.side, true));
            ranges.push(range);
        }
    }
    for child_index in result
        .forest
        .children_by_parent
        .get(container_id)
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        let child = &nodes[*child_index];
        if !child.is_group {
            continue;
        }
        let bounds = router_rect(child);
        for mut range in derive_portal_ranges(&child.id, bounds, title_bounds(child))? {
            range.coordinate = Some(portal_coordinate(bounds, range.side, false));
            ranges.push(range);
        }
    }
    Ok(ranges)
}

pub fn build_routing_context(
    container_ids: &[GridContainerId],
    result: &GridLayoutResult,
    nodes: &[Node],
    mut metrics: Option<&mut GridRoutingInstrumentation>,
    options: &GridRoutingTestOptions,
) -> Result<GridRoutingContext, GridRoutingError> {
    // Build each immutable container topology once. Per-edge endpoints are added later as cheap
    // overlays, avoiding repeated obstacle sweeps for large diagrams.
    let mut context = GridRoutingContext::default();
    let mut estimated_bytes = 0.0_f64;
    let mut unique_ids: Vec<GridContainerId> = Vec::new();
    for container_id in container_ids {
        if !unique_ids.contains(container_id) {
            unique_ids.push(container_id.clone());
        }
    }
    unique_ids.sort_by(|a, b| crate::layout::grid::types::compare_code_units(a, b));
    for container_id in unique_ids {
        let children: Vec<usize> = result
            .forest
            .children_by_parent
            .get(&container_id)
            .map(|children| {
                children
                    .iter()
                    .copied()
                    .filter(|index| !is_edge_label_node(&nodes[*index]))
                    .collect()
            })
            .unwrap_or_default();
        let obstacles: Vec<RouterObstacleInput> = children
            .iter()
            .map(|index| RouterObstacleInput {
                id: nodes[*index].id.clone(),
                bounds: router_rect(&nodes[*index]),
            })
            .collect();
        let title = result
            .forest
            .node_by_id
            .get(&container_id)
            .and_then(|index| nodes.get(*index))
            .and_then(title_bounds);
        let title_exclusions = title
            .map(|bounds| {
                vec![RouterObstacleInput {
                    id: format!("{container_id}:title"),
                    bounds,
                }]
            })
            .unwrap_or_default();
        let input = ContainerTopologyInput {
            container_id: container_id.clone(),
            bounds: container_bounds(&container_id, result, nodes)?,
            obstacles,
            title_exclusions,
            portal_ranges: container_portal_ranges(&container_id, result, nodes)?,
        };
        let topology = match build_container_routing_topology(
            &input,
            BuildTopologyOptions {
                metrics: None,
                caps: options.options.topology_caps.unwrap_or_default(),
            },
        ) {
            Ok(topology) => topology,
            Err(error) => {
                if !error.is_resource_limit() {
                    return Err(error);
                }
                let GridRoutingError::ResourceLimit(resource_error) = error else {
                    unreachable!("is_resource_limit implies a resource limit error");
                };
                context
                    .fallback_containers
                    .insert(container_id, resource_error.reason);
                continue;
            }
        };
        let invocation_memory_cap = options
            .options
            .topology_caps
            .and_then(|caps| caps.max_estimated_bytes)
            .unwrap_or(DEFAULT_MAX_ROUTING_ESTIMATED_BYTES);
        if estimated_bytes + topology.estimated_bytes > invocation_memory_cap {
            // Keep the invocation-wide cap honest: an individually valid topology may still exceed
            // the aggregate budget when combined with topologies already retained by this session.
            context
                .fallback_containers
                .insert(container_id, GridRoutingFallbackReason::EstimatedMemoryCap);
            continue;
        }
        estimated_bytes += topology.estimated_bytes;
        context.base_estimated_bytes = estimated_bytes;
        if let Some(metrics) = metrics.as_deref_mut() {
            metrics.containers_built += 1;
            metrics.base_topology_builds += 1;
            metrics.base_vertices += topology.vertices.len();
            metrics.base_adjacency_entries += topology.adjacency_entries;
            metrics.build_sweep_events += topology.seed_count * 4 + topology.obstacles.len() * 4;
            metrics.estimated_bytes = estimated_bytes;
        }
        context.topologies.insert(container_id, topology);
    }
    Ok(context)
}

#[allow(clippy::too_many_arguments)]
pub fn validated_compatibility_segment(
    edge_id: &str,
    container_id: &str,
    start: &SegmentAttachment,
    end: &SegmentAttachment,
    route: impl FnOnce() -> Vec<RouterPoint>,
    result: &GridLayoutResult,
    nodes: &[Node],
    mut metrics: Option<&mut GridRoutingInstrumentation>,
) -> Result<Vec<RouterPoint>, GridRoutingError> {
    let points = route();
    if let Some(metrics) = metrics.as_deref_mut() {
        metrics.compatibility_segments += 1;
    }
    if validate_container_segment(
        &points,
        &start.owner_id,
        &end.owner_id,
        container_id,
        result,
        nodes,
    ) {
        return Ok(points);
    }
    if let Some(metrics) = metrics.as_mut() {
        metrics.compatibility_validation_failures += 1;
        metrics.routes_impossible += 1;
    }
    Err(GridRoutingError::geometry(format!(
        "GRID_ROUTE_NOT_FOUND: Invalid compatibility route for \"{edge_id}\" (container: {container_id})"
    )))
}

pub fn compatibility_lca_attachments(
    plan: &EdgeRoutePlan,
    result: &GridLayoutResult,
    nodes: &[Node],
    demand_coords: &HashMap<String, f64>,
) -> Result<(SegmentAttachment, SegmentAttachment), GridRoutingError> {
    let Some(source_final) = plan.source.chain.last() else {
        return Err(GridRoutingError::geometry(format!(
            "GRID_ROUTE_NOT_FOUND: Invalid compatibility route for \"{}\"",
            plan.edge.id
        )));
    };
    let Some(target_final) = plan.target.chain.last() else {
        return Err(GridRoutingError::geometry(format!(
            "GRID_ROUTE_NOT_FOUND: Invalid compatibility route for \"{}\"",
            plan.edge.id
        )));
    };
    let final_attachment =
        |endpoint: &EdgeEndpointPlan,
         final_entry: &crate::layout::grid::router_planning::EdgeEndpointEntry|
         -> Result<SegmentAttachment, GridRoutingError> {
            let attachment = if endpoint.final_kind == EndpointFinalKind::Boundary {
                boundary_attachment(
                    &plan.lca_container_id,
                    final_entry.side,
                    &final_entry.demand_key,
                    result,
                    nodes,
                    demand_coords,
                )?
            } else {
                item_attachment(
                    &final_entry.owner_id,
                    final_entry.side,
                    &final_entry.demand_key,
                    &plan.lca_container_id,
                    result,
                    nodes,
                    demand_coords,
                    None,
                )?
            };
            Ok(SegmentAttachment {
                owner_id: final_entry.owner_id.clone(),
                port: attachment.port,
                connect: attachment.connect,
                orientation: attachment.orientation,
                side: attachment.side,
            })
        };
    Ok((
        final_attachment(&plan.source, source_final)?,
        final_attachment(&plan.target, target_final)?,
    ))
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompatibilitySegmentAttachments {
    pub container_id: GridContainerId,
    pub start: SegmentAttachment,
    pub end: SegmentAttachment,
}

pub fn compatibility_hierarchy_attachments(
    endpoint: &EdgeEndpointPlan,
    result: &GridLayoutResult,
    nodes: &[Node],
    demand_coords: &HashMap<String, f64>,
) -> Result<Vec<CompatibilitySegmentAttachments>, GridRoutingError> {
    let mut attachments = Vec::new();
    for (index, from) in endpoint
        .chain
        .iter()
        .enumerate()
        .take(endpoint.chain.len().saturating_sub(1))
    {
        let to = &endpoint.chain[index + 1];
        let item = item_attachment(
            &from.owner_id,
            from.side,
            &from.demand_key,
            &to.owner_id,
            result,
            nodes,
            demand_coords,
            None,
        )?;
        let boundary = boundary_attachment(
            &to.owner_id,
            to.side,
            &to.demand_key,
            result,
            nodes,
            demand_coords,
        )?;
        attachments.push(CompatibilitySegmentAttachments {
            container_id: to.owner_id.clone(),
            start: SegmentAttachment {
                owner_id: from.owner_id.clone(),
                port: item.port,
                connect: item.connect,
                orientation: item.orientation,
                side: item.side,
            },
            end: SegmentAttachment {
                owner_id: to.owner_id.clone(),
                port: boundary.port,
                connect: boundary.connect,
                orientation: boundary.orientation,
                side: boundary.side,
            },
        });
    }
    Ok(attachments)
}

fn compatibility_plan_is_valid(
    plan: &EdgeRoutePlan,
    result: &GridLayoutResult,
    nodes: &[Node],
    demand_coords: &HashMap<String, f64>,
) -> Result<bool, GridRoutingError> {
    let segment_is_valid = |container_id: &str,
                            start: &SegmentAttachment,
                            end: &SegmentAttachment|
     -> Result<bool, GridRoutingError> {
        let route = route_within_container(
            container_id,
            result,
            &start.as_attachment(),
            &end.as_attachment(),
            plan.lane_index,
        )?;
        Ok(validate_container_segment(
            &route,
            &start.owner_id,
            &end.owner_id,
            container_id,
            result,
            nodes,
        ))
    };
    for endpoint in [&plan.source, &plan.target] {
        for segment in compatibility_hierarchy_attachments(endpoint, result, nodes, demand_coords)?
        {
            if !segment_is_valid(&segment.container_id, &segment.start, &segment.end)? {
                return Ok(false);
            }
        }
    }
    let (start, end) = compatibility_lca_attachments(plan, result, nodes, demand_coords)?;
    segment_is_valid(&plan.lca_container_id, &start, &end)
}

/// Classifies every plan before building topologies: proven minimal corridor routes avoid sparse
/// setup, while only containers needed by sparse LCA, hierarchy, or loop segments are materialized.
///
/// Part C's session calls this after [`crate::layout::grid::router_planning::prepare_edge_routes`].
pub fn prepare_routing_modes(
    prepared: &PreparedEdgeRoutes,
    result: &GridLayoutResult,
    nodes: &[Node],
    mut metrics: Option<&mut GridRoutingInstrumentation>,
    options: &GridRoutingTestOptions,
) -> Result<PreparedRoutingModes, GridRoutingError> {
    let plans = &prepared.plans;
    let eligible_plans = &prepared.eligible_plans;
    let eligible_ids = &prepared.eligible_ids;
    let demand_coords = &prepared.demand_coords;
    let endpoint_candidates_by_edge = &prepared.endpoint_candidates_by_edge;
    let endpoint_incident_counts = &prepared.endpoint_incident_counts;
    let coordinated_endpoint_ids = &prepared.coordinated_endpoint_ids;
    let has_isolated_endpoints = |plan: &EdgeRoutePlan| -> bool {
        plan.edge
            .start
            .as_deref()
            .and_then(|start| endpoint_incident_counts.get(start))
            .copied()
            .unwrap_or(0)
            == plan.bundle_size
            && plan
                .edge
                .end
                .as_deref()
                .and_then(|end| endpoint_incident_counts.get(end))
                .copied()
                .unwrap_or(0)
                == plan.bundle_size
    };
    let has_ancestor_endpoint = |plan: &EdgeRoutePlan| -> bool {
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
        let (Some(source), Some(target)) = (source, target) else {
            return false;
        };
        (source.is_group
            && is_ancestor_group(
                &source.id,
                plan.edge
                    .end
                    .as_deref()
                    .and_then(|id| result.forest.node_by_id.get(id).copied()),
                nodes,
                &result.forest.node_by_id,
            ))
            || (target.is_group
                && is_ancestor_group(
                    &target.id,
                    plan.edge
                        .start
                        .as_deref()
                        .and_then(|id| result.forest.node_by_id.get(id).copied()),
                    nodes,
                    &result.forest.node_by_id,
                ))
    };
    let mut compatibility_fast_routes: HashMap<String, Vec<RouterPoint>> = HashMap::new();
    let mut route_occupancy: HashMap<GridContainerId, RouteOccupancyIndex> = HashMap::new();
    let overlap_avoidance_enabled = plans.len() <= MAX_PLANS_FOR_OVERLAP_AVOIDANCE;
    let compatibility_fast_path_enabled = options.compatibility_fast_path_enabled();
    for plan in if compatibility_fast_path_enabled {
        eligible_plans.as_slice()
    } else {
        &[]
    } {
        if coordinated_endpoint_ids.contains(&plan.edge.id) {
            continue;
        }
        let Some(candidates) = endpoint_candidates_by_edge.get(&plan.edge.id) else {
            continue;
        };
        if plan.bundle_size != 1 || candidates.sources.is_empty() || candidates.targets.is_empty() {
            continue;
        }
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
        let (Some(source), Some(target)) = (source, target) else {
            continue;
        };
        let (start, end) = compatibility_lca_attachments(plan, result, nodes, demand_coords)?;
        let route = route_within_container(
            &plan.lca_container_id,
            result,
            &start.as_attachment(),
            &end.as_attachment(),
            plan.lane_index,
        )?;
        let normalized = normalize_polyline(&route);
        let (Some(first), Some(last)) = (normalized.points.first(), normalized.points.last())
        else {
            continue;
        };
        let source_candidate = candidates
            .sources
            .iter()
            .find(|candidate| candidate.port.x == first.x && candidate.port.y == first.y);
        let target_candidate = candidates
            .targets
            .iter()
            .find(|candidate| candidate.port.x == last.x && candidate.port.y == last.y);
        if source_candidate.is_none() || target_candidate.is_none() {
            continue;
        }
        if let Some(metrics) = metrics.as_deref_mut() {
            metrics.compatibility_fast_path_attempts += 1;
        }
        let lower_bound_length = (first.x - last.x).abs() + (first.y - last.y).abs();
        if !validate_same_container_route(
            &route,
            source,
            target,
            &plan.lca_container_id,
            result,
            nodes,
        ) {
            if let Some(metrics) = metrics.as_deref_mut() {
                metrics.compatibility_fast_path_validation_failures += 1;
            }
            continue;
        }
        if manhattan_length(&normalized.points) != lower_bound_length {
            if let Some(metrics) = metrics.as_deref_mut() {
                metrics.compatibility_fast_path_non_minimal_routes += 1;
            }
            continue;
        }
        // Small layouts demote a route that runs close and parallel to an earlier edge to sparse
        // routing. Large layouts keep the fast path to stay search-free within the performance
        // contract, nudging the route sideways when possible.
        let occupancy = route_occupancy
            .entry(plan.lca_container_id.clone())
            .or_default();
        let conflicting = occupancy.conflicts_with_route(&normalized.points, &plan.pair_key);
        if conflicting && overlap_avoidance_enabled {
            // The sparse router can choose other ports and corridors, which reads better than a
            // nudge.
            continue;
        }
        let separated = separate_route_from_occupied(
            &normalized.points,
            occupancy,
            &plan.pair_key,
            |candidate| {
                terminal_stubs_are_long_enough(candidate)
                    && manhattan_length(candidate) == lower_bound_length
                    && validate_same_container_route(
                        candidate,
                        source,
                        target,
                        &plan.lca_container_id,
                        result,
                        nodes,
                    )
            },
        );
        let final_route = separated.unwrap_or_else(|| route.clone());
        occupancy.add(&final_route, &plan.pair_key);
        compatibility_fast_routes.insert(plan.edge.id.clone(), final_route);
    }
    let mut sparse_hierarchy_ids: HashSet<String> = HashSet::new();
    for plan in plans {
        let sparse_candidate = !eligible_ids.contains(&plan.edge.id)
            && plan.edge.start != plan.edge.end
            && (plan.bundle_size > 1
                || has_isolated_endpoints(plan)
                || !compatibility_plan_is_valid(plan, result, nodes, demand_coords)?);
        if sparse_candidate {
            sparse_hierarchy_ids.insert(plan.edge.id.clone());
        }
    }
    let mut planning_occupancy: HashMap<GridContainerId, RouteOccupancyIndex> = HashMap::new();
    let mut overlapping_hierarchy_ids: HashSet<String> = HashSet::new();
    if overlap_avoidance_enabled {
        for plan in &prepared.ordered_plans {
            if eligible_ids.contains(&plan.edge.id)
                || plan.edge.start == plan.edge.end
                || has_ancestor_endpoint(plan)
                || sparse_hierarchy_ids.contains(&plan.edge.id)
            {
                continue;
            }
            let (start, end) = compatibility_lca_attachments(plan, result, nodes, demand_coords)?;
            let route = route_within_container(
                &plan.lca_container_id,
                result,
                &start.as_attachment(),
                &end.as_attachment(),
                plan.lane_index,
            )?;
            let occupancy = planning_occupancy
                .entry(plan.lca_container_id.clone())
                .or_default();
            if occupancy.conflicts_with_route(&route, &plan.pair_key) {
                overlapping_hierarchy_ids.insert(plan.edge.id.clone());
            } else {
                occupancy.add(&route, &plan.pair_key);
            }
        }
    }
    sparse_hierarchy_ids.extend(overlapping_hierarchy_ids);
    let mut sparse_lca_ids = sparse_hierarchy_ids.clone();
    for plan in plans {
        if has_ancestor_endpoint(plan) {
            sparse_lca_ids.insert(plan.edge.id.clone());
        }
    }
    let routed_container_ids: Vec<GridContainerId> = plans
        .iter()
        .filter(|plan| {
            (eligible_ids.contains(&plan.edge.id)
                && !compatibility_fast_routes.contains_key(&plan.edge.id))
                || sparse_lca_ids.contains(&plan.edge.id)
                || plan.edge.start == plan.edge.end
        })
        .flat_map(|plan| {
            let mut ids = vec![plan.lca_container_id.clone()];
            ids.extend(
                plan.source
                    .chain
                    .iter()
                    .skip(1)
                    .map(|entry| entry.owner_id.clone()),
            );
            ids.extend(
                plan.target
                    .chain
                    .iter()
                    .skip(1)
                    .map(|entry| entry.owner_id.clone()),
            );
            ids
        })
        .collect();
    Ok(PreparedRoutingModes {
        overlap_avoidance_enabled,
        compatibility_fast_routes,
        sparse_hierarchy_ids,
        sparse_lca_ids,
        routed_container_ids,
        route_occupancy,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::grid::router_planning::{TERMINAL_APPROACH_PX, owner_side_key};
    use crate::layout::grid::types::{GridSide, Node, RouterPoint};

    fn point(x: f64, y: f64) -> RouterPoint {
        RouterPoint { x, y }
    }

    fn requirement(along: f64, cross: f64, required_along_span: f64) -> GridLabelSpanRequirement {
        GridLabelSpanRequirement {
            along,
            cross,
            start_inset: 12.0,
            end_inset: 12.0,
            required_along_span,
        }
    }

    #[test]
    fn bounds_alternative_portal_coordinates_and_prefers_nearby_routing_corridors() {
        assert_eq!(
            bounded_alternative_portal_coordinates(
                50.0,
                10.0,
                90.0,
                &[80.0, 20.0, 60.0, 50.0, 100.0],
                3
            ),
            vec![60.0, 20.0, 80.0]
        );
        assert_eq!(
            bounded_alternative_portal_coordinates(50.0, 10.0, 90.0, &[], 4),
            vec![10.0, 90.0]
        );
    }

    #[test]
    fn requires_exact_axis_alignment_for_the_direct_route_shortcut() {
        assert!(!are_exactly_axis_aligned(point(0.0, 0.0), point(0.5, 10.0)));
        assert!(are_exactly_axis_aligned(point(0.0, 0.0), point(0.0, 10.0)));
    }

    #[test]
    fn keeps_a_labelled_side_loop_clear_of_corners_and_return_shoulders() {
        let owner = Node {
            id: "owner".to_string(),
            x: Some(40.0),
            y: Some(20.0),
            width: Some(80.0),
            height: Some(40.0),
            ..Node::default()
        };
        for (side, boundary_x, shoulder_x, outer_x) in [
            (GridSide::Left, 0.0, -20.0, -71.0),
            (GridSide::Right, 80.0, 100.0, 151.0),
        ] {
            assert_eq!(
                label_aware_self_loop_candidates(
                    &owner,
                    side,
                    0,
                    20.0,
                    &requirement(90.0, 20.0, 116.0)
                ),
                vec![vec![
                    point(boundary_x, 11.0),
                    point(shoulder_x, 11.0),
                    point(shoulder_x, 3.0),
                    point(outer_x, 3.0),
                    point(outer_x, 37.0),
                    point(shoulder_x, 37.0),
                    point(shoulder_x, 29.0),
                    point(boundary_x, 29.0),
                ]]
            );
        }
    }

    #[test]
    fn uses_safe_ports_for_labelled_loop_corner_clearance() {
        for side in [
            GridSide::Left,
            GridSide::Right,
            GridSide::Top,
            GridSide::Bottom,
        ] {
            let vertical_side = side == GridSide::Left || side == GridSide::Right;
            let width = if vertical_side { 80.0 } else { 42.0 };
            let height = if vertical_side { 42.0 } else { 80.0 };
            let owner = Node {
                id: "owner".to_string(),
                x: Some(width / 2.0),
                y: Some(height / 2.0),
                width: Some(width),
                height: Some(height),
                ..Node::default()
            };
            let port_point = |coordinate: f64| {
                if vertical_side {
                    point(if side == GridSide::Left { 0.0 } else { width }, coordinate)
                } else {
                    point(coordinate, if side == GridSide::Top { 0.0 } else { height })
                }
            };
            for (demand_coord, port_start, port_end, point_count) in [
                (20.0, 11.0, 29.0, 8usize),
                (22.0, 13.0, 31.0, 8usize),
                (21.0, 4.0, 38.0, 4usize),
            ] {
                let candidates = label_aware_self_loop_candidates(
                    &owner,
                    side,
                    0,
                    demand_coord,
                    &requirement(20.0, 20.0, 46.0),
                );
                let candidate = &candidates[0];

                assert_eq!(candidate.len(), point_count);
                assert_eq!(candidate[0], port_point(port_start));
                assert_eq!(candidate[candidate.len() - 1], port_point(port_end));
                for (start, end) in [
                    (candidate[0], candidate[1]),
                    (
                        candidate[candidate.len() - 2],
                        candidate[candidate.len() - 1],
                    ),
                ] {
                    assert!(
                        (end.x - start.x).abs() + (end.y - start.y).abs() >= TERMINAL_APPROACH_PX
                    );
                }
            }
        }
    }

    #[test]
    fn uses_one_symmetric_shoulder_with_label_placement_slack() {
        let owner = Node {
            id: "owner".to_string(),
            x: Some(40.0),
            y: Some(20.0),
            width: Some(80.0),
            height: Some(40.0),
            ..Node::default()
        };
        for (side, boundary_y, shoulder_y, outer_y) in [
            (GridSide::Top, 0.0, -20.0, -36.0),
            (GridSide::Bottom, 40.0, 60.0, 76.0),
        ] {
            let candidates = label_aware_self_loop_candidates(
                &owner,
                side,
                0,
                40.0,
                &requirement(90.0, 20.0, 116.0),
            );
            assert_eq!(
                candidates[0],
                vec![
                    point(31.0, boundary_y),
                    point(31.0, shoulder_y),
                    point(-12.0, shoulder_y),
                    point(-12.0, outer_y),
                    point(92.0, outer_y),
                    point(92.0, shoulder_y),
                    point(49.0, shoulder_y),
                    point(49.0, boundary_y),
                ]
            );
        }
    }

    #[test]
    fn combines_point_chains() {
        assert_eq!(combine_point_chains(&[]), Vec::<RouterPoint>::new());
        assert_eq!(
            combine_point_chains(&[
                vec![point(0.0, 0.0), point(10.0, 0.0)],
                vec![point(10.0, 0.0), point(10.0, 10.0)],
            ]),
            vec![point(0.0, 0.0), point(10.0, 0.0), point(10.0, 10.0)]
        );
        assert_eq!(
            combine_point_chains(&[
                vec![point(0.0, 0.0), point(10.0, 0.0)],
                vec![point(10.0, 0.0), point(20.0, 0.0)],
            ]),
            vec![point(0.0, 0.0), point(20.0, 0.0)]
        );
    }

    #[test]
    fn reverses_points_without_mutating_the_input() {
        assert_eq!(reverse_points(&[]), Vec::<RouterPoint>::new());
        let points = vec![point(0.0, 0.0), point(10.0, 0.0), point(10.0, 10.0)];
        let before = points.clone();
        assert_eq!(
            reverse_points(&points),
            vec![point(10.0, 10.0), point(10.0, 0.0), point(0.0, 0.0)]
        );
        assert_eq!(points, before);
    }

    #[test]
    fn orders_self_loop_sides_by_combined_demand() {
        let mut counts: HashMap<String, usize> = HashMap::new();
        let mut self_loop_counts: HashMap<String, usize> = HashMap::new();
        for (side, count) in [
            (GridSide::Left, 2usize),
            (GridSide::Top, 4usize),
            (GridSide::Bottom, 1usize),
        ] {
            counts.insert(owner_side_key("owner:1", side), count);
        }
        for (side, count) in [(GridSide::Right, 3usize), (GridSide::Bottom, 1usize)] {
            self_loop_counts.insert(format!("owner:1:{}", side.as_str()), count);
        }
        let node = Node {
            id: "owner:1".to_string(),
            ..Node::default()
        };
        assert_eq!(
            ordered_self_loop_sides(&node, &counts, &self_loop_counts),
            vec![
                GridSide::Left,
                GridSide::Bottom,
                GridSide::Right,
                GridSide::Top
            ]
        );

        let titled = Node {
            id: "owner:1".to_string(),
            is_group: true,
            group_title_rect: Some(crate::layout::grid::types::GroupTitleRect {
                left: -10.0,
                right: 10.0,
                top: -10.0,
                bottom: 0.0,
            }),
            ..Node::default()
        };
        let mut titled_counts: HashMap<String, usize> = HashMap::new();
        for (side, count) in [
            (GridSide::Left, 2usize),
            (GridSide::Right, 1usize),
            (GridSide::Top, 0usize),
            (GridSide::Bottom, 3usize),
        ] {
            titled_counts.insert(owner_side_key("owner:1", side), count);
        }
        assert_eq!(
            ordered_self_loop_sides(&titled, &titled_counts, &HashMap::new()),
            vec![GridSide::Right, GridSide::Left, GridSide::Bottom]
        );

        assert_eq!(
            ordered_self_loop_sides(&node, &HashMap::new(), &HashMap::new()),
            vec![
                GridSide::Left,
                GridSide::Right,
                GridSide::Top,
                GridSide::Bottom
            ]
        );
    }
}
