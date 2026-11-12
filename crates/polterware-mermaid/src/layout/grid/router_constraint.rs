//! Port of Mermaid 12.3.0's
//! `rendering-util/layout-algorithms/grid/routerConstraint.ts`.
//!
//! Centralizes post-route validity checks so sparse routes, compatibility fallbacks, and bundle
//! retries all enforce the same obstacle-clearance and lane-separation contract. Node children are
//! indices into the layout node list, resolved through the routing result's forest.

use crate::layout::grid::geometry::{
    PIXEL_EPSILON, SegmentOrientation, normalize_polyline, rect_for_node,
};
use crate::layout::grid::router_errors::GridRoutingError;
use crate::layout::grid::router_planning::{
    EndpointCandidate, LANE_SEPARATION_PX, TERMINAL_APPROACH_PX,
};
use crate::layout::grid::router_topology::ROUTE_CLEARANCE_PX;
use crate::layout::grid::types::{
    GridLayoutResult, GridOrientation, GridSide, Node, RouterPoint, RouterRect, is_edge_label_node,
};

/// Upstream inspects `error.details.reason === 'pair-separation'`. The routing session attaches
/// that detail when a bundle fails strict separation but keeps distinct endpoint ports.
pub fn is_pair_separation_error(error: &GridRoutingError) -> bool {
    match error {
        GridRoutingError::Grid(error) => {
            error
                .details
                .as_ref()
                .and_then(|details| details.get("reason"))
                .and_then(|reason| reason.as_str())
                == Some("pair-separation")
        }
        _ => false,
    }
}

pub fn router_rect(node: &Node) -> RouterRect {
    let rect = rect_for_node(node);
    RouterRect {
        left: rect.left,
        right: rect.right,
        top: rect.top,
        bottom: rect.bottom,
    }
}

fn canonicalize_orthogonal_polyline(points: &[RouterPoint]) -> Option<Vec<RouterPoint>> {
    if points.len() < 2
        || points
            .iter()
            .any(|point| !point.x.is_finite() || !point.y.is_finite())
    {
        return None;
    }

    let mut collapsed: Vec<RouterPoint> = vec![points[0]];
    for index in 1..points.len() {
        let point = points[index];
        let previous = *collapsed.last().expect("collapsed is not empty");
        let is_near_duplicate = (point.x - previous.x).abs() <= PIXEL_EPSILON
            && (point.y - previous.y).abs() <= PIXEL_EPSILON;
        if !is_near_duplicate {
            collapsed.push(point);
            continue;
        }
        if index != points.len() - 1 {
            continue;
        }
        while collapsed.len() > 1 {
            let before_previous = collapsed[collapsed.len() - 2];
            if (point.x - before_previous.x).abs() > PIXEL_EPSILON
                || (point.y - before_previous.y).abs() > PIXEL_EPSILON
            {
                break;
            }
            collapsed.pop();
        }
        if collapsed.len() == 1 {
            return None;
        }
        *collapsed.last_mut().expect("collapsed is not empty") = point;
    }
    if collapsed.len() < 2 {
        return None;
    }

    let mut orientations: Vec<GridOrientation> = Vec::new();
    for index in 0..collapsed.len() - 1 {
        let a = collapsed[index];
        let b = collapsed[index + 1];
        let dx = (b.x - a.x).abs();
        let dy = (b.y - a.y).abs();
        if dy <= PIXEL_EPSILON {
            orientations.push(GridOrientation::H);
        } else if dx <= PIXEL_EPSILON {
            orientations.push(GridOrientation::V);
        } else {
            return None;
        }
    }

    let mut canonical = collapsed.clone();
    let mut start = 0usize;
    while start < orientations.len() {
        let orientation = orientations[start];
        let mut end = start + 1;
        while end < orientations.len() && orientations[end] == orientation {
            end += 1;
        }
        let axis_y = orientation == GridOrientation::H;
        let axis = |point: &RouterPoint| if axis_y { point.y } else { point.x };
        if start == 0
            && end == collapsed.len() - 1
            && axis(&collapsed[start]) != axis(&collapsed[end])
        {
            return None;
        }
        let coordinate = if end == collapsed.len() - 1 {
            axis(&collapsed[end])
        } else {
            axis(&collapsed[start])
        };
        for point_index in start..=end {
            if (axis(&collapsed[point_index]) - coordinate).abs() > PIXEL_EPSILON {
                return None;
            }
            if axis_y {
                canonical[point_index].y = coordinate;
            } else {
                canonical[point_index].x = coordinate;
            }
        }
        start = end;
    }
    Some(canonical)
}

fn segment_enters_rect(a: RouterPoint, b: RouterPoint, rect: RouterRect) -> bool {
    if a.y == b.y {
        return a.y > rect.top
            && a.y < rect.bottom
            && a.x.max(b.x) > rect.left
            && a.x.min(b.x) < rect.right;
    }
    if a.x == b.x {
        return a.x > rect.left
            && a.x < rect.right
            && a.y.max(b.y) > rect.top
            && a.y.min(b.y) < rect.bottom;
    }
    true
}

fn inflated_rect(node: &Node) -> RouterRect {
    let rect = router_rect(node);
    RouterRect {
        left: rect.left - ROUTE_CLEARANCE_PX,
        right: rect.right + ROUTE_CLEARANCE_PX,
        top: rect.top - ROUTE_CLEARANCE_PX,
        bottom: rect.bottom + ROUTE_CLEARANCE_PX,
    }
}

fn route_length(a: RouterPoint, b: RouterPoint) -> f64 {
    (a.x - b.x).abs() + (a.y - b.y).abs()
}

pub fn validate_same_container_route(
    points: &[RouterPoint],
    source: &Node,
    target: &Node,
    container_id: &str,
    result: &GridLayoutResult,
    nodes: &[Node],
) -> bool {
    // Terminal segments may touch their own endpoint rectangles, but must leave them directly and
    // every other segment must remain outside clearance-inflated child geometry.
    let Some(canonical) = canonicalize_orthogonal_polyline(points) else {
        return false;
    };
    let normalized = normalize_polyline(&canonical);
    if normalized.points.len() < 2 {
        return false;
    }
    if normalized.segments.len() > 1
        && (route_length(normalized.segments[0].a, normalized.segments[0].b) < TERMINAL_APPROACH_PX
            || route_length(
                normalized
                    .segments
                    .last()
                    .expect("segments are not empty")
                    .a,
                normalized
                    .segments
                    .last()
                    .expect("segments are not empty")
                    .b,
            ) < TERMINAL_APPROACH_PX)
    {
        return false;
    }
    let children: Vec<usize> = result
        .forest
        .children_by_parent
        .get(container_id)
        .map(|children| {
            children
                .iter()
                .copied()
                .filter(|index| !is_edge_label_node(&nodes[*index]))
                .collect()
        })
        .unwrap_or_default();
    for obstacle_index in children {
        let obstacle = &nodes[obstacle_index];
        let rect = inflated_rect(obstacle);
        for index in 0..normalized.points.len() - 1 {
            let terminal_obstacle = (obstacle.id == source.id && index == 0)
                || (obstacle.id == target.id && index == normalized.points.len() - 2);
            if terminal_obstacle {
                if segment_enters_rect(
                    normalized.points[index],
                    normalized.points[index + 1],
                    router_rect(obstacle),
                ) {
                    return false;
                }
                continue;
            }
            if segment_enters_rect(normalized.points[index], normalized.points[index + 1], rect) {
                return false;
            }
        }
    }
    true
}

pub fn validate_container_segment(
    points: &[RouterPoint],
    start_owner_id: &str,
    end_owner_id: &str,
    container_id: &str,
    result: &GridLayoutResult,
    nodes: &[Node],
) -> bool {
    let Some(canonical) = canonicalize_orthogonal_polyline(points) else {
        return false;
    };
    let normalized = normalize_polyline(&canonical);
    if normalized.points.len() < 2 {
        return false;
    }
    let children: Vec<usize> = result
        .forest
        .children_by_parent
        .get(container_id)
        .map(|children| {
            children
                .iter()
                .copied()
                .filter(|index| !is_edge_label_node(&nodes[*index]))
                .collect()
        })
        .unwrap_or_default();
    for obstacle_index in children {
        let obstacle = &nodes[obstacle_index];
        let rect = inflated_rect(obstacle);
        for index in 0..normalized.points.len() - 1 {
            let terminal_obstacle = (obstacle.id == start_owner_id && index == 0)
                || (obstacle.id == end_owner_id && index == normalized.points.len() - 2);
            if terminal_obstacle {
                if segment_enters_rect(
                    normalized.points[index],
                    normalized.points[index + 1],
                    router_rect(obstacle),
                ) {
                    return false;
                }
                continue;
            }
            if segment_enters_rect(normalized.points[index], normalized.points[index + 1], rect) {
                return false;
            }
        }
    }
    true
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrthogonalSegment {
    pub a: RouterPoint,
    pub b: RouterPoint,
    pub orientation: GridOrientation,
}

fn segment_span(segment: &OrthogonalSegment) -> (f64, f64) {
    if segment.orientation == GridOrientation::H {
        (segment.a.x.min(segment.b.x), segment.a.x.max(segment.b.x))
    } else {
        (segment.a.y.min(segment.b.y), segment.a.y.max(segment.b.y))
    }
}

pub fn nonterminal_segments(points: &[RouterPoint]) -> Vec<OrthogonalSegment> {
    let normalized = normalize_polyline(points);
    let mut segments: Vec<OrthogonalSegment> = normalized
        .segments
        .iter()
        .filter(|segment| segment.orientation != SegmentOrientation::Z)
        .map(|segment| OrthogonalSegment {
            a: segment.a,
            b: segment.b,
            orientation: if segment.orientation == SegmentOrientation::H {
                GridOrientation::H
            } else {
                GridOrientation::V
            },
        })
        .collect();
    if segments.is_empty() {
        return Vec::new();
    }
    if segments.len() == 1
        && route_length(segments[0].a, segments[0].b) <= TERMINAL_APPROACH_PX * 2.0
    {
        return Vec::new();
    }
    // Pair lanes may share the short attachment stubs at nodes. Trim those stubs before measuring
    // corridor overlap so only the independently routable interior is constrained.
    fn trim(segment: &mut OrthogonalSegment, at_start: bool) {
        if segment.orientation == GridOrientation::H {
            let direction = (segment.b.x - segment.a.x).signum();
            if at_start {
                segment.a.x += direction * TERMINAL_APPROACH_PX;
            } else {
                segment.b.x -= direction * TERMINAL_APPROACH_PX;
            }
        } else {
            let direction = (segment.b.y - segment.a.y).signum();
            if at_start {
                segment.a.y += direction * TERMINAL_APPROACH_PX;
            } else {
                segment.b.y -= direction * TERMINAL_APPROACH_PX;
            }
        }
    }
    let last = segments.len() - 1;
    trim(&mut segments[0], true);
    trim(&mut segments[last], false);
    segments
        .into_iter()
        .filter(|segment| segment.a.x != segment.b.x || segment.a.y != segment.b.y)
        .collect()
}

pub fn pair_segments_conflict(
    candidate: &OrthogonalSegment,
    committed: &OrthogonalSegment,
) -> bool {
    if candidate.orientation != committed.orientation {
        return false;
    }
    let (first_low, first_high) = segment_span(candidate);
    let (second_low, second_high) = segment_span(committed);
    let overlap = 0.0_f64.max(first_high.min(second_high) - first_low.max(second_low));
    if overlap <= 0.0 {
        return false;
    }
    let separation = if candidate.orientation == GridOrientation::H {
        (candidate.a.y - committed.a.y).abs()
    } else {
        (candidate.a.x - committed.a.x).abs()
    };
    // Collinear overlap is illegal when lanes coincide for a meaningful distance or are closer
    // than the configured lane spacing. Endpoint-only contact has zero overlap and stays legal.
    (separation == 0.0 && overlap >= LANE_SEPARATION_PX)
        || (separation > 0.0 && separation < LANE_SEPARATION_PX)
}

fn same_point(a: RouterPoint, b: RouterPoint) -> bool {
    a.x == b.x && a.y == b.y
}

pub fn route_satisfies_pair_constraints(
    points: &[RouterPoint],
    pair_routes: &[Vec<RouterPoint>],
) -> bool {
    let normalized = normalize_polyline(points).points;
    let candidate_ports = [
        normalized[0],
        *normalized.last().expect("points are not empty"),
    ];
    let candidate_segments = nonterminal_segments(&normalized);
    pair_routes.iter().all(|route| {
        let committed = normalize_polyline(route).points;
        let committed_ports = [
            committed[0],
            *committed.last().expect("points are not empty"),
        ];
        if candidate_ports.iter().any(|port| {
            committed_ports
                .iter()
                .any(|other| same_point(*port, *other))
        }) {
            return false;
        }
        let committed_segments = nonterminal_segments(&committed);
        candidate_segments.iter().all(|candidate| {
            committed_segments
                .iter()
                .all(|committed_segment| !pair_segments_conflict(candidate, committed_segment))
        })
    })
}

pub fn route_has_distinct_pair_ports(
    points: &[RouterPoint],
    pair_routes: &[Vec<RouterPoint>],
) -> bool {
    let normalized = normalize_polyline(points).points;
    let candidate_ports = [
        normalized[0],
        *normalized.last().expect("points are not empty"),
    ];
    pair_routes.iter().all(|route| {
        let committed = normalize_polyline(route).points;
        let committed_ports = [
            committed[0],
            *committed.last().expect("points are not empty"),
        ];
        !candidate_ports.iter().any(|port| {
            committed_ports
                .iter()
                .any(|other| same_point(*port, *other))
        })
    })
}

pub fn pair_arc_allowed(
    from: RouterPoint,
    to: RouterPoint,
    pair_routes: &[Vec<RouterPoint>],
) -> bool {
    let orientation = if from.y == to.y {
        GridOrientation::H
    } else {
        GridOrientation::V
    };
    let candidate = OrthogonalSegment {
        a: from,
        b: to,
        orientation,
    };
    pair_routes.iter().all(|route| {
        nonterminal_segments(route)
            .iter()
            .all(|committed_segment| !pair_segments_conflict(&candidate, committed_segment))
    })
}

pub fn endpoint_pair_lower_bound(
    source: &EndpointCandidate,
    target: &EndpointCandidate,
) -> (f64, usize) {
    // This admissible lower bound includes both terminal stubs and the minimum bend count implied
    // by endpoint orientations; candidate-pair search can safely prune against it.
    let source_orientation = if source.side == GridSide::Left || source.side == GridSide::Right {
        GridOrientation::H
    } else {
        GridOrientation::V
    };
    let target_orientation = if target.side == GridSide::Left || target.side == GridSide::Right {
        GridOrientation::H
    } else {
        GridOrientation::V
    };
    let dx = (source.connect.x - target.connect.x).abs();
    let dy = (source.connect.y - target.connect.y).abs();
    let bends = if dx == 0.0 && dy == 0.0 {
        usize::from(source_orientation != target_orientation)
    } else if dx == 0.0 || dy == 0.0 {
        let path_orientation = if dx == 0.0 {
            GridOrientation::V
        } else {
            GridOrientation::H
        };
        usize::from(source_orientation != path_orientation)
            + usize::from(target_orientation != path_orientation)
    } else {
        usize::min(
            usize::from(source_orientation != GridOrientation::H)
                + 1
                + usize::from(target_orientation != GridOrientation::V),
            usize::from(source_orientation != GridOrientation::V)
                + 1
                + usize::from(target_orientation != GridOrientation::H),
        )
    };
    (TERMINAL_APPROACH_PX * 2.0 + dx + dy, bends)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::grid::groups::build_grid_forest;
    use crate::layout::grid::label_spacing::GridLabelRequirements;
    use crate::layout::grid::placement::read_grid_config;
    use crate::layout::grid::router_planning::EndpointCandidate;
    use crate::layout::grid::types::{
        GridLayoutResult, GridSide, LayoutData, Node, ROOT_CONTAINER_ID,
    };
    use std::collections::HashMap;

    fn endpoint(side: GridSide, x: f64, y: f64) -> EndpointCandidate {
        EndpointCandidate {
            owner_id: side.as_str().to_string(),
            side,
            port: RouterPoint { x, y },
            connect: RouterPoint { x, y },
            rank: 0.0,
            on_assigned_side: None,
        }
    }

    fn layout_result(nodes: &[Node]) -> GridLayoutResult {
        let data = LayoutData {
            nodes: nodes.to_vec(),
            ..LayoutData::default()
        };
        GridLayoutResult {
            forest: build_grid_forest(nodes).unwrap(),
            config: read_grid_config(&data),
            containers: HashMap::new(),
            item_meta: HashMap::new(),
            source_order: HashMap::new(),
            label_requirements: GridLabelRequirements::default(),
        }
    }

    fn routing_fixture() -> (Vec<Node>, GridLayoutResult) {
        let source = Node {
            id: "source".to_string(),
            x: Some(0.0),
            y: Some(0.0),
            width: Some(40.0),
            height: Some(40.0),
            ..Node::default()
        };
        let target = Node {
            id: "target".to_string(),
            x: Some(100.0),
            y: Some(0.0),
            width: Some(40.0),
            height: Some(40.0),
            ..Node::default()
        };
        let nodes = vec![source, target];
        let result = layout_result(&nodes);
        (nodes, result)
    }

    fn points(values: &[(f64, f64)]) -> Vec<RouterPoint> {
        values
            .iter()
            .map(|(x, y)| RouterPoint { x: *x, y: *y })
            .collect()
    }

    #[test]
    fn computes_admissible_lower_bounds_for_endpoint_pairs() {
        let cases: [(EndpointCandidate, EndpointCandidate, (f64, usize)); 6] = [
            (
                endpoint(GridSide::Right, 0.0, 0.0),
                endpoint(GridSide::Left, 0.0, 0.0),
                (40.0, 0),
            ),
            (
                endpoint(GridSide::Bottom, 0.0, 0.0),
                endpoint(GridSide::Top, 0.0, 0.0),
                (40.0, 0),
            ),
            (
                endpoint(GridSide::Right, 0.0, 0.0),
                endpoint(GridSide::Top, 0.0, 0.0),
                (40.0, 1),
            ),
            (
                endpoint(GridSide::Right, 0.0, 0.0),
                endpoint(GridSide::Left, 10.0, 0.0),
                (50.0, 0),
            ),
            (
                endpoint(GridSide::Right, 0.0, 0.0),
                endpoint(GridSide::Left, 0.0, 10.0),
                (50.0, 2),
            ),
            (
                endpoint(GridSide::Right, 0.0, 0.0),
                endpoint(GridSide::Top, 10.0, 10.0),
                (60.0, 1),
            ),
        ];
        for (source, target, expected) in cases {
            assert_eq!(endpoint_pair_lower_bound(&source, &target), expected);
        }
    }

    #[test]
    fn rejects_a_terminal_segment_that_passes_through_its_destination_node() {
        let (nodes, result) = routing_fixture();
        let route = points(&[
            (20.0, 0.0),
            (20.0, -40.0),
            (120.0, -40.0),
            (120.0, 0.0),
            (80.0, 0.0),
        ]);
        assert!(!validate_same_container_route(
            &route,
            &nodes[0],
            &nodes[1],
            ROOT_CONTAINER_ID,
            &result,
            &nodes
        ));
    }

    #[test]
    fn snaps_near_axis_interior_segments_before_validating_obstacles() {
        let (nodes, result) = routing_fixture();
        let route = points(&[
            (20.0, 0.0),
            (40.0, 0.0),
            (40.5, -40.0),
            (80.0, -39.5),
            (80.0, 0.0),
        ]);
        assert!(validate_same_container_route(
            &route,
            &nodes[0],
            &nodes[1],
            ROOT_CONTAINER_ID,
            &result,
            &nodes
        ));
    }

    #[test]
    fn accepts_exact_orthogonal_segments_with_fractional_coordinates() {
        let (nodes, result) = routing_fixture();
        let route = points(&[
            (20.0, 0.0),
            (40.25, 0.0),
            (40.25, -40.5),
            (80.0, -40.5),
            (80.0, 0.0),
        ]);
        assert!(validate_same_container_route(
            &route,
            &nodes[0],
            &nodes[1],
            ROOT_CONTAINER_ID,
            &result,
            &nodes
        ));
    }

    #[test]
    fn rejects_segments_that_remain_diagonal_beyond_the_pixel_tolerance() {
        let (nodes, result) = routing_fixture();
        let route = points(&[(20.0, 0.0), (40.0, -20.0), (80.0, -20.0), (80.0, 0.0)]);
        assert!(!validate_same_container_route(
            &route,
            &nodes[0],
            &nodes[1],
            ROOT_CONTAINER_ID,
            &result,
            &nodes
        ));
    }

    #[test]
    fn does_not_snap_a_route_endpoint_to_make_a_segment_orthogonal() {
        let (nodes, result) = routing_fixture();
        let route = points(&[(20.0, 0.0), (80.0, 0.5)]);
        assert!(!validate_same_container_route(
            &route,
            &nodes[0],
            &nodes[1],
            ROOT_CONTAINER_ID,
            &result,
            &nodes
        ));
    }

    #[test]
    fn preserves_an_endpoint_when_collapsing_a_trailing_near_duplicate() {
        let (nodes, result) = routing_fixture();
        let route = points(&[
            (20.0, 0.0),
            (40.0, 0.0),
            (40.0, -40.0),
            (80.0, -40.0),
            (80.0, -0.5),
            (80.0, 0.0),
        ]);
        assert!(validate_same_container_route(
            &route,
            &nodes[0],
            &nodes[1],
            ROOT_CONTAINER_ID,
            &result,
            &nodes
        ));
    }

    #[test]
    fn rejects_accumulated_drift_beyond_the_pixel_tolerance() {
        let (nodes, result) = routing_fixture();
        let route = points(&[
            (20.0, 0.0),
            (40.0, 0.8),
            (60.0, 1.6),
            (80.0, 1.6),
            (80.0, 0.0),
        ]);
        assert!(!validate_same_container_route(
            &route,
            &nodes[0],
            &nodes[1],
            ROOT_CONTAINER_ID,
            &result,
            &nodes
        ));
    }
}
