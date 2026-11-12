//! Port of Mermaid 12.3.0's
//! `rendering-util/layout-algorithms/grid/routerTopology.ts`.
//!
//! Builds immutable orthogonal visibility graphs per container. Endpoint-specific vertices are
//! added through bounded overlays so the expensive obstacle sweep is paid once per container.
//! Upstream's proxy-based overlay views are materialized here; the geometry and traversal order
//! are unchanged.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::layout::grid::router_errors::{GridRoutingError, GridRoutingResourceLimitError};
use crate::layout::grid::router_instrumentation::{
    GridRoutingFallbackReason, GridRoutingInstrumentation,
};
use crate::layout::grid::types::{
    ContainerRoutingTopology, Coordinate, GridOrientation, GridSide, OrthogonalIntervalIndex,
    PairedPortal, PortalRange, PortalTransition, RouterArc, RouterArcKind, RouterObstacle,
    RouterPoint, RouterRect, RouterSearchArc, RouterVertex, RouterVertexKind, compare_code_units,
    js_number,
};

// Parallel runs of different edges closer than this are hard to tell apart, so candidates avoid
// them. Perpendicular crossings are not affected.
pub const ROUTE_CLEARANCE_PX: f64 = 6.0;
pub const EDGE_CLEARANCE_PX: f64 = 24.0;

// Caps bound both adversarial geometry and ordinary large-diagram memory growth. Exceeding one is a
// recoverable routing condition handled by the validated compatibility path.
const DEFAULT_MAX_VERTICES: usize = 50_000;
const MAX_VISIBILITY_ADJACENCY_PER_VERTEX: usize = 4;
pub const DEFAULT_MAX_ROUTING_ESTIMATED_BYTES: f64 = 64.0 * 1024.0 * 1024.0;

#[derive(Clone, Debug, PartialEq)]
pub struct RouterObstacleInput {
    pub id: String,
    pub bounds: RouterRect,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ContainerTopologyInput {
    pub container_id: String,
    pub bounds: RouterRect,
    pub obstacles: Vec<RouterObstacleInput>,
    pub title_exclusions: Vec<RouterObstacleInput>,
    pub portal_ranges: Vec<PortalRange>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TopologyResourceCaps {
    pub max_vertices: Option<usize>,
    pub max_adjacency_entries: Option<usize>,
    pub max_estimated_bytes: Option<f64>,
}

#[derive(Default)]
pub struct BuildTopologyOptions<'a> {
    pub metrics: Option<&'a mut GridRoutingInstrumentation>,
    pub caps: TopologyResourceCaps,
}

#[derive(Clone, Debug)]
struct VertexRecord {
    point: RouterPoint,
    kind: RouterVertexKind,
    owner_id: Option<String>,
    side: Option<GridSide>,
}

#[derive(Clone, Debug)]
struct IndexedBand {
    low: f64,
    high: f64,
    intervals: Arc<Vec<(f64, f64)>>,
}

#[derive(Debug)]
struct CompressedIntervalIndex {
    // Obstacle occupancy is constant between sorted boundaries, so bands share merged interval
    // lists. Exact boundaries use strict intervals to allow visibility paths to run along obstacle
    // edges.
    coordinate_count: usize,
    interval_count: usize,
    bands: Vec<IndexedBand>,
    by_low: HashMap<Coordinate, usize>,
    by_high: HashMap<Coordinate, usize>,
    strict_boundary_intervals: HashMap<Coordinate, Arc<Vec<(f64, f64)>>>,
}

impl CompressedIntervalIndex {
    fn containing_band(&self, coordinate: f64) -> Option<&IndexedBand> {
        let coordinate = Coordinate::new(coordinate);
        let mut low = 0usize;
        let mut high = self.bands.len() as isize - 1;
        while low as isize <= high {
            let middle = (low as isize + high) / 2;
            let band = &self.bands[middle as usize];
            if coordinate.value() <= band.low {
                high = middle - 1;
            } else if coordinate.value() >= band.high {
                low = middle as usize + 1;
            } else {
                return Some(band);
            }
        }
        None
    }

    fn overlaps(intervals: &[(f64, f64)], low: f64, high: f64) -> bool {
        let mut left = 0usize;
        let mut right = intervals.len();
        while left < right {
            let middle = (left + right) / 2;
            if intervals[middle].1 <= low {
                left = middle + 1;
            } else {
                right = middle;
            }
        }
        left < intervals.len() && intervals[left].0 < high
    }

    fn intervals_for(&self, coordinate: f64) -> Option<&Vec<(f64, f64)>> {
        let key = Coordinate::new(coordinate);
        match self.strict_boundary_intervals.get(&key) {
            Some(intervals) => Some(intervals),
            None => self
                .containing_band(coordinate)
                .map(|band| &*band.intervals),
        }
    }
}

impl OrthogonalIntervalIndex for CompressedIntervalIndex {
    fn coordinate_count(&self) -> usize {
        self.coordinate_count
    }

    fn interval_count(&self) -> usize {
        self.interval_count
    }

    fn intersects(&self, coordinate: f64, interval_start: f64, interval_end: f64) -> bool {
        let low = interval_start.min(interval_end);
        let high = interval_start.max(interval_end);
        let key = Coordinate::new(coordinate);
        let before = self.by_high.get(&key).map(|&index| &self.bands[index]);
        let after = self.by_low.get(&key).map(|&index| &self.bands[index]);
        if before.is_none()
            && after.is_none()
            && let Some(containing) = self.containing_band(coordinate)
        {
            return Self::overlaps(&containing.intervals, low, high);
        }
        let (Some(before), Some(after)) = (before, after) else {
            return false;
        };
        let mut before_index = 0usize;
        let mut after_index = 0usize;
        while before_index < before.intervals.len() && after_index < after.intervals.len() {
            let overlap_low = low
                .max(before.intervals[before_index].0)
                .max(after.intervals[after_index].0);
            let overlap_high = high
                .min(before.intervals[before_index].1)
                .min(after.intervals[after_index].1);
            if overlap_low < overlap_high {
                return true;
            }
            if before.intervals[before_index].1 < after.intervals[after_index].1 {
                before_index += 1;
            } else {
                after_index += 1;
            }
        }
        false
    }

    fn contains(&self, coordinate: f64, varying: f64) -> bool {
        let Some(intervals) = self.intervals_for(coordinate) else {
            return false;
        };
        let mut low = 0usize;
        let mut high = intervals.len();
        while low < high {
            let middle = (low + high) / 2;
            if intervals[middle].0 < varying {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        low > 0 && varying < intervals[low - 1].1
    }

    fn nearest_boundary(&self, coordinate: f64, origin: f64, direction: i8) -> Option<f64> {
        let intervals = self.intervals_for(coordinate)?;
        if direction < 0 {
            let mut low = 0usize;
            let mut high = intervals.len();
            while low < high {
                let middle = (low + high) / 2;
                if intervals[middle].1 <= origin {
                    low = middle + 1;
                } else {
                    high = middle;
                }
            }
            return if low > 0 {
                Some(intervals[low - 1].1)
            } else {
                None
            };
        }
        let mut low = 0usize;
        let mut high = intervals.len();
        while low < high {
            let middle = (low + high) / 2;
            if intervals[middle].0 < origin {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        if low < intervals.len() {
            Some(intervals[low].0)
        } else {
            None
        }
    }
}

/// One scratch object belongs to one base topology. The undo log clears only overridden base
/// vertices, keeping reset cost proportional to the previous overlay rather than the whole graph.
#[derive(Clone, Debug, Default)]
pub struct EndpointOverlayScratch {
    pub added_vertices: Vec<RouterVertex>,
    pub adjacency_overrides: HashMap<usize, Vec<RouterArc>>,
    pub search_adjacency_overrides: HashMap<usize, Vec<RouterSearchArc>>,
    pub point_overrides: HashMap<String, usize>,
    pub undo_log: Vec<usize>,
    pub reset_count: usize,
    base_container_id: Option<String>,
}

impl EndpointOverlayScratch {
    pub fn new(base: &ContainerRoutingTopology) -> Self {
        Self {
            base_container_id: Some(base.container_id.clone()),
            ..Self::default()
        }
    }

    pub fn reset(&mut self, base: &ContainerRoutingTopology) -> Result<(), GridRoutingError> {
        // Upstream compares object identity; Rust callers construct one scratch per topology, so
        // the container id is the closest stable identity available.
        if self.base_container_id.as_deref() != Some(base.container_id.as_str()) {
            return Err(GridRoutingError::geometry(
                "Endpoint overlay scratch cannot be shared across base topologies",
            ));
        }
        for id in self.undo_log.drain(..) {
            self.adjacency_overrides.remove(&id);
            self.search_adjacency_overrides.remove(&id);
        }
        self.undo_log.clear();
        self.added_vertices.clear();
        self.point_overrides.clear();
        self.reset_count += 1;
        Ok(())
    }

    pub fn set_arcs(&mut self, id: usize, arcs: Vec<RouterArc>) {
        if !self.adjacency_overrides.contains_key(&id) {
            self.undo_log.push(id);
        }
        self.search_adjacency_overrides
            .insert(id, search_arcs(&arcs));
        self.adjacency_overrides.insert(id, arcs);
    }
}

fn normalize_coordinate(value: f64) -> f64 {
    if value == 0.0 { 0.0 } else { value }
}

fn normalize_rect(rect: RouterRect) -> Result<RouterRect, GridRoutingError> {
    let values = [rect.left, rect.right, rect.top, rect.bottom];
    if !values.iter().all(|value| value.is_finite()) {
        return Err(GridRoutingError::geometry(
            "Grid routing geometry must contain only finite coordinates",
        ));
    }
    let left = normalize_coordinate(rect.left.min(rect.right));
    let right = normalize_coordinate(rect.left.max(rect.right));
    let top = normalize_coordinate(rect.top.min(rect.bottom));
    let bottom = normalize_coordinate(rect.top.max(rect.bottom));
    if left == right || top == bottom {
        // The production layout path should reject zero-area nodes before routing; this guards
        // direct topology callers and invalid derived geometry.
        return Err(GridRoutingError::geometry(
            "Grid routing rectangles must have positive area",
        ));
    }
    Ok(RouterRect {
        left,
        right,
        top,
        bottom,
    })
}

fn inflate_and_clip(
    input: &RouterObstacleInput,
    domain: RouterRect,
) -> Result<Option<RouterObstacle>, GridRoutingError> {
    let bounds = normalize_rect(input.bounds)?;
    let obstacle = RouterObstacle {
        id: input.id.clone(),
        left: domain.left.max(bounds.left - ROUTE_CLEARANCE_PX),
        right: domain.right.min(bounds.right + ROUTE_CLEARANCE_PX),
        top: domain.top.max(bounds.top - ROUTE_CLEARANCE_PX),
        bottom: domain.bottom.min(bounds.bottom + ROUTE_CLEARANCE_PX),
    };
    Ok((obstacle.left < obstacle.right && obstacle.top < obstacle.bottom).then_some(obstacle))
}

#[derive(Clone, Debug)]
struct MergedInterval {
    low: f64,
    high: f64,
    id: String,
}

fn merge_obstacle_intervals(intervals: Vec<MergedInterval>) -> Vec<MergedInterval> {
    let mut sorted = intervals;
    sorted.sort_by(|a, b| {
        a.low
            .total_cmp(&b.low)
            .then_with(|| a.high.total_cmp(&b.high))
            .then_with(|| compare_code_units(&a.id, &b.id))
    });
    let mut merged: Vec<MergedInterval> = Vec::new();
    for interval in sorted {
        match merged.last_mut() {
            None => merged.push(interval),
            Some(previous) => {
                if interval.low > previous.high {
                    merged.push(interval);
                } else {
                    previous.high = previous.high.max(interval.high);
                    if compare_code_units(&interval.id, &previous.id).is_lt() {
                        previous.id = interval.id;
                    }
                }
            }
        }
    }
    merged
}

#[derive(Default)]
struct SweepEvent {
    starts: Vec<usize>,
    ends: Vec<usize>,
}

fn union_obstacles(obstacles: &[RouterObstacle]) -> Vec<RouterObstacle> {
    // Sweep x-slabs and merge active y-intervals into a non-overlapping obstacle union. This
    // prevents intersecting inflated nodes and title exclusions from multiplying visibility
    // vertices.
    let mut events: HashMap<Coordinate, SweepEvent> = HashMap::new();
    for (index, obstacle) in obstacles.iter().enumerate() {
        events
            .entry(Coordinate::new(obstacle.left))
            .or_default()
            .starts
            .push(index);
        events
            .entry(Coordinate::new(obstacle.right))
            .or_default()
            .ends
            .push(index);
    }
    let mut x_coordinates: Vec<f64> = events.keys().map(|coordinate| coordinate.value()).collect();
    x_coordinates.sort_by(f64::total_cmp);
    let mut active: HashSet<usize> = HashSet::new();
    let mut slabs: Vec<RouterObstacle> = Vec::new();
    let mut previous_by_interval: HashMap<(Coordinate, Coordinate), usize> = HashMap::new();
    for index in 0..x_coordinates.len().saturating_sub(1) {
        let left = x_coordinates[index];
        let right = x_coordinates[index + 1];
        if let Some(event) = events.get(&Coordinate::new(left)) {
            for end in &event.ends {
                active.remove(end);
            }
            for start in &event.starts {
                active.insert(*start);
            }
        }
        let mut intervals: Vec<MergedInterval> = active
            .iter()
            .map(|&active_index| MergedInterval {
                low: obstacles[active_index].top,
                high: obstacles[active_index].bottom,
                id: obstacles[active_index].id.clone(),
            })
            .collect();
        intervals = merge_obstacle_intervals(intervals);
        let mut next_by_interval: HashMap<(Coordinate, Coordinate), usize> = HashMap::new();
        for interval in intervals {
            let key = (
                Coordinate::new(interval.low),
                Coordinate::new(interval.high),
            );
            if let Some(&previous_index) = previous_by_interval.get(&key) {
                let previous = &mut slabs[previous_index];
                previous.right = right;
                if compare_code_units(&interval.id, &previous.id).is_lt() {
                    previous.id = interval.id;
                }
                next_by_interval.insert(key, previous_index);
            } else {
                slabs.push(RouterObstacle {
                    id: interval.id,
                    left,
                    right,
                    top: interval.low,
                    bottom: interval.high,
                });
                next_by_interval.insert(key, slabs.len() - 1);
            }
        }
        previous_by_interval = next_by_interval;
    }
    slabs.sort_by(|a, b| {
        a.top
            .total_cmp(&b.top)
            .then_with(|| a.left.total_cmp(&b.left))
            .then_with(|| a.bottom.total_cmp(&b.bottom))
            .then_with(|| a.right.total_cmp(&b.right))
            .then_with(|| compare_code_units(&a.id, &b.id))
    });
    slabs
}

pub fn routing_point_key(point: RouterPoint) -> String {
    format!(
        "{}:{}",
        js_number(normalize_coordinate(point.x)),
        js_number(normalize_coordinate(point.y))
    )
}

fn topology_side_ordinal(side: Option<GridSide>) -> u8 {
    match side {
        Some(GridSide::Left) => 0,
        Some(GridSide::Right) => 1,
        Some(GridSide::Top) => 2,
        Some(GridSide::Bottom) => 3,
        None => 4,
    }
}

fn vertex_record_order(a: &VertexRecord, b: &VertexRecord) -> std::cmp::Ordering {
    compare_code_units(a.kind.as_str(), b.kind.as_str())
        .then_with(|| topology_side_ordinal(a.side).cmp(&topology_side_ordinal(b.side)))
        .then_with(|| a.point.x.total_cmp(&b.point.x))
        .then_with(|| a.point.y.total_cmp(&b.point.y))
        .then_with(|| {
            compare_code_units(
                a.owner_id.as_deref().unwrap_or(""),
                b.owner_id.as_deref().unwrap_or(""),
            )
        })
}

fn canonical_records(records: &[VertexRecord]) -> Vec<VertexRecord> {
    // Sorting before duplicate removal makes vertex identity and numeric ids independent of
    // discovery order.
    let mut sorted = records.to_vec();
    sorted.sort_by(vertex_record_order);
    let mut by_point: HashMap<String, VertexRecord> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for record in sorted {
        let normalized = VertexRecord {
            point: RouterPoint {
                x: normalize_coordinate(record.point.x),
                y: normalize_coordinate(record.point.y),
            },
            ..record
        };
        let key = routing_point_key(normalized.point);
        if let std::collections::hash_map::Entry::Vacant(entry) = by_point.entry(key.clone()) {
            entry.insert(normalized);
            order.push(key);
        }
    }
    order
        .into_iter()
        .filter_map(|key| by_point.remove(&key))
        .collect()
}

fn corner_records(bounds: RouterRect, owner_id: &str, kind: RouterVertexKind) -> Vec<VertexRecord> {
    vec![
        VertexRecord {
            point: RouterPoint::new(bounds.left, bounds.top),
            kind,
            owner_id: Some(owner_id.to_string()),
            side: None,
        },
        VertexRecord {
            point: RouterPoint::new(bounds.right, bounds.top),
            kind,
            owner_id: Some(owner_id.to_string()),
            side: None,
        },
        VertexRecord {
            point: RouterPoint::new(bounds.left, bounds.bottom),
            kind,
            owner_id: Some(owner_id.to_string()),
            side: None,
        },
        VertexRecord {
            point: RouterPoint::new(bounds.right, bounds.bottom),
            kind,
            owner_id: Some(owner_id.to_string()),
            side: None,
        },
    ]
}

fn portal_records(range: &PortalRange, bounds: RouterRect) -> Vec<VertexRecord> {
    let coordinate = range.coordinate.unwrap_or(match range.side {
        GridSide::Left => bounds.left,
        GridSide::Right => bounds.right,
        GridSide::Top => bounds.top,
        GridSide::Bottom => bounds.bottom,
    });
    let points = if matches!(range.side, GridSide::Left | GridSide::Right) {
        [
            RouterPoint::new(coordinate, range.low),
            RouterPoint::new(coordinate, range.high),
        ]
    } else {
        [
            RouterPoint::new(range.low, coordinate),
            RouterPoint::new(range.high, coordinate),
        ]
    };
    points
        .into_iter()
        .map(|point| VertexRecord {
            point,
            kind: RouterVertexKind::Portal,
            owner_id: Some(range.owner_id.clone()),
            side: Some(range.side),
        })
        .collect()
}

fn ray_hit(
    point: RouterPoint,
    side: GridSide,
    bounds: RouterRect,
    horizontal_intervals: &dyn OrthogonalIntervalIndex,
    vertical_intervals: &dyn OrthogonalIntervalIndex,
) -> RouterPoint {
    if matches!(side, GridSide::Left | GridSide::Right) {
        let hit = horizontal_intervals
            .nearest_boundary(
                point.y,
                point.x,
                if side == GridSide::Left { -1 } else { 1 },
            )
            .unwrap_or(if side == GridSide::Left {
                bounds.left
            } else {
                bounds.right
            });
        return RouterPoint::new(hit, point.y);
    }

    let hit = vertical_intervals
        .nearest_boundary(point.x, point.y, if side == GridSide::Top { -1 } else { 1 })
        .unwrap_or(if side == GridSide::Top {
            bounds.top
        } else {
            bounds.bottom
        });
    RouterPoint::new(point.x, hit)
}

struct IntervalBudget {
    interval_count: usize,
    max_estimated_bytes: f64,
}

fn estimated_topology_bytes(
    vertices: usize,
    adjacency_entries: usize,
    obstacles: usize,
    intervals: usize,
) -> f64 {
    vertices as f64 * 64.0
        + adjacency_entries as f64 * 56.0
        + obstacles as f64 * 48.0
        + intervals as f64 * 24.0
}

fn enforce_cap(
    actual: f64,
    maximum: f64,
    reason: GridRoutingFallbackReason,
) -> Result<(), GridRoutingError> {
    if actual > maximum {
        return Err(GridRoutingError::ResourceLimit(
            GridRoutingResourceLimitError::new(
                reason,
                format!(
                    "Grid routing {} exceeded: {} > {}",
                    reason.as_str(),
                    js_number(actual),
                    js_number(maximum)
                ),
                None,
            ),
        ));
    }
    Ok(())
}

fn active_intervals(
    obstacles: &[RouterObstacle],
    orientation: GridOrientation,
    active: &HashSet<usize>,
) -> Vec<MergedInterval> {
    merge_obstacle_intervals(
        active
            .iter()
            .map(|&index| {
                let obstacle = &obstacles[index];
                if orientation == GridOrientation::H {
                    MergedInterval {
                        low: obstacle.left,
                        high: obstacle.right,
                        id: obstacle.id.clone(),
                    }
                } else {
                    MergedInterval {
                        low: obstacle.top,
                        high: obstacle.bottom,
                        id: obstacle.id.clone(),
                    }
                }
            })
            .collect(),
    )
}

fn freeze_intervals(
    intervals: Vec<MergedInterval>,
    obstacle_count: usize,
    budget: &mut IntervalBudget,
) -> Result<Arc<Vec<(f64, f64)>>, GridRoutingError> {
    let interval_count = budget.interval_count + intervals.len();
    enforce_cap(
        estimated_topology_bytes(0, 0, obstacle_count, interval_count),
        budget.max_estimated_bytes,
        GridRoutingFallbackReason::EstimatedMemoryCap,
    )?;
    budget.interval_count = interval_count;
    Ok(Arc::new(
        intervals
            .into_iter()
            .map(|interval| (interval.low, interval.high))
            .collect(),
    ))
}

fn create_interval_index(
    obstacles: &[RouterObstacle],
    orientation: GridOrientation,
    budget: &mut IntervalBudget,
) -> Result<CompressedIntervalIndex, GridRoutingError> {
    let mut coordinates: Vec<f64> = obstacles
        .iter()
        .flat_map(|obstacle| {
            if orientation == GridOrientation::H {
                [obstacle.top, obstacle.bottom]
            } else {
                [obstacle.left, obstacle.right]
            }
        })
        .map(normalize_coordinate)
        .collect();
    coordinates.sort_by(f64::total_cmp);
    coordinates.dedup();
    let mut bands: Vec<IndexedBand> = Vec::new();
    let mut strict_boundary_intervals: HashMap<Coordinate, Arc<Vec<(f64, f64)>>> = HashMap::new();
    let mut starts: HashMap<Coordinate, Vec<usize>> = HashMap::new();
    let mut ends: HashMap<Coordinate, Vec<usize>> = HashMap::new();
    for (index, obstacle) in obstacles.iter().enumerate() {
        let (start, end) = if orientation == GridOrientation::H {
            (obstacle.top, obstacle.bottom)
        } else {
            (obstacle.left, obstacle.right)
        };
        starts
            .entry(Coordinate::new(start))
            .or_default()
            .push(index);
        ends.entry(Coordinate::new(end)).or_default().push(index);
    }
    let mut active: HashSet<usize> = HashSet::new();
    for index in 0..coordinates.len() {
        let coordinate = coordinates[index];
        if let Some(ending) = ends.get(&Coordinate::new(coordinate)) {
            for end in ending {
                active.remove(end);
            }
        }
        strict_boundary_intervals.insert(
            Coordinate::new(coordinate),
            freeze_intervals(
                active_intervals(obstacles, orientation, &active),
                obstacles.len(),
                budget,
            )?,
        );
        if let Some(starting) = starts.get(&Coordinate::new(coordinate)) {
            for start in starting {
                active.insert(*start);
            }
        }
        let Some(&high) = coordinates.get(index + 1) else {
            continue;
        };
        let intervals = active_intervals(obstacles, orientation, &active);
        if !intervals.is_empty() {
            bands.push(IndexedBand {
                low: coordinate,
                high,
                intervals: freeze_intervals(intervals, obstacles.len(), budget)?,
            });
        }
    }
    let mut coordinate_keys: HashSet<Coordinate> = HashSet::new();
    let mut interval_count = 0usize;
    let mut by_low: HashMap<Coordinate, usize> = HashMap::new();
    let mut by_high: HashMap<Coordinate, usize> = HashMap::new();
    for (index, band) in bands.iter().enumerate() {
        coordinate_keys.insert(Coordinate::new(band.low));
        coordinate_keys.insert(Coordinate::new(band.high));
        interval_count += band.intervals.len();
        by_low.insert(Coordinate::new(band.low), index);
        by_high.insert(Coordinate::new(band.high), index);
    }
    for intervals in strict_boundary_intervals.values() {
        interval_count += intervals.len();
    }
    Ok(CompressedIntervalIndex {
        coordinate_count: coordinate_keys.len(),
        interval_count,
        bands,
        by_low,
        by_high,
        strict_boundary_intervals,
    })
}

fn index_vertex_lines(
    vertices: &[RouterVertex],
    orientation: GridOrientation,
) -> HashMap<Coordinate, Vec<usize>> {
    let mut lines: HashMap<Coordinate, Vec<usize>> = HashMap::new();
    for vertex in vertices {
        let fixed = if orientation == GridOrientation::H {
            vertex.point.y
        } else {
            vertex.point.x
        };
        lines
            .entry(Coordinate::new(fixed))
            .or_default()
            .push(vertex.id);
    }
    for line in lines.values_mut() {
        line.sort_by(|&a, &b| {
            if orientation == GridOrientation::H {
                vertices[a]
                    .point
                    .x
                    .total_cmp(&vertices[b].point.x)
                    .then_with(|| a.cmp(&b))
            } else {
                vertices[a]
                    .point
                    .y
                    .total_cmp(&vertices[b].point.y)
                    .then_with(|| a.cmp(&b))
            }
        });
    }
    lines
}

fn search_arcs(arcs: &[RouterArc]) -> Vec<RouterSearchArc> {
    // Occupancy and crossing tie-breakers are left for future route-quality work; the compact
    // representation carries zero for both for now, matching upstream.
    arcs.iter()
        .map(|arc| RouterSearchArc {
            to: arc.to,
            orientation_ordinal: if arc.orientation == GridOrientation::H {
                1
            } else {
                2
            },
            length: arc.length,
            boundary_transitions: if arc.kind == RouterArcKind::Portal {
                1
            } else {
                0
            },
        })
        .collect()
}

fn add_arc_pair(
    a: &RouterVertex,
    b: &RouterVertex,
    orientation: GridOrientation,
    adjacency: &mut [Vec<RouterArc>],
) {
    let length = (a.point.x - b.point.x).abs() + (a.point.y - b.point.y).abs();
    if length == 0.0 {
        return;
    }
    let interval_start = if orientation == GridOrientation::H {
        a.point.x.min(b.point.x)
    } else {
        a.point.y.min(b.point.y)
    };
    let interval_end = interval_start + length;
    for (from, to) in [(a, b), (b, a)] {
        adjacency[from.id].push(RouterArc {
            from: from.id,
            to: to.id,
            orientation,
            length,
            kind: RouterArcKind::Visibility,
            interval_start,
            interval_end,
        });
    }
}

struct VisibilityGraph {
    vertices: Vec<RouterVertex>,
    adjacency: HashMap<usize, Vec<RouterArc>>,
    adjacency_entries: usize,
}

fn build_visibility_graph(
    records: &[VertexRecord],
    horizontal_intervals: &dyn OrthogonalIntervalIndex,
    vertical_intervals: &dyn OrthogonalIntervalIndex,
    caps: &TopologyResourceCaps,
) -> Result<VisibilityGraph, GridRoutingError> {
    // Vertices connect only to their immediate visible neighbor on each shared x/y line; longer
    // paths are represented by graph traversal instead of quadratic all-pairs arcs.
    let mut canonical = canonical_records(records);
    canonical.sort_by(vertex_record_order);
    let vertex_cap = caps.max_vertices.unwrap_or(DEFAULT_MAX_VERTICES);
    enforce_cap(
        canonical.len() as f64,
        vertex_cap as f64,
        GridRoutingFallbackReason::VertexCap,
    )?;
    let vertices: Vec<RouterVertex> = canonical
        .into_iter()
        .enumerate()
        .map(|(id, record)| RouterVertex {
            id,
            point: record.point,
            kind: record.kind,
            owner_id: record.owner_id,
            side: record.side,
        })
        .collect();
    let mut adjacency: Vec<Vec<RouterArc>> = vec![Vec::new(); vertices.len()];

    for orientation in [GridOrientation::H, GridOrientation::V] {
        let mut lines: HashMap<Coordinate, Vec<usize>> = HashMap::new();
        for vertex in &vertices {
            let fixed = if orientation == GridOrientation::H {
                vertex.point.y
            } else {
                vertex.point.x
            };
            lines
                .entry(Coordinate::new(fixed))
                .or_default()
                .push(vertex.id);
        }
        let mut ordered_lines: Vec<(f64, Vec<usize>)> = lines
            .into_iter()
            .map(|(coordinate, mut line)| {
                line.sort_by(|&a, &b| {
                    if orientation == GridOrientation::H {
                        vertices[a]
                            .point
                            .x
                            .total_cmp(&vertices[b].point.x)
                            .then_with(|| a.cmp(&b))
                    } else {
                        vertices[a]
                            .point
                            .y
                            .total_cmp(&vertices[b].point.y)
                            .then_with(|| a.cmp(&b))
                    }
                });
                (coordinate.value(), line)
            })
            .collect();
        ordered_lines.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (fixed, line) in ordered_lines {
            for index in 1..line.len() {
                let a = &vertices[line[index - 1]];
                let b = &vertices[line[index]];
                let (start, end) = if orientation == GridOrientation::H {
                    (a.point.x, b.point.x)
                } else {
                    (a.point.y, b.point.y)
                };
                let interval_index = if orientation == GridOrientation::H {
                    horizontal_intervals
                } else {
                    vertical_intervals
                };
                if !interval_index.intersects(fixed, start, end) {
                    add_arc_pair(a, b, orientation, &mut adjacency);
                }
            }
        }
    }

    let mut adjacency_entries = 0usize;
    let mut immutable_adjacency: HashMap<usize, Vec<RouterArc>> = HashMap::new();
    for vertex in &vertices {
        let mut arcs = std::mem::take(&mut adjacency[vertex.id]);
        arcs.sort_by(|a, b| {
            a.kind
                .ordinal()
                .cmp(&b.kind.ordinal())
                .then_with(|| compare_code_units(a.orientation.as_str(), b.orientation.as_str()))
                .then_with(|| vertices[a.to].point.x.total_cmp(&vertices[b.to].point.x))
                .then_with(|| vertices[a.to].point.y.total_cmp(&vertices[b.to].point.y))
                .then_with(|| a.to.cmp(&b.to))
        });
        arcs.dedup_by(|arc, previous| {
            arc.to == previous.to
                && arc.orientation == previous.orientation
                && arc.kind == previous.kind
        });
        adjacency_entries += arcs.len();
        immutable_adjacency.insert(vertex.id, arcs);
    }
    enforce_cap(
        adjacency_entries as f64,
        caps.max_adjacency_entries
            .unwrap_or(vertex_cap * MAX_VISIBILITY_ADJACENCY_PER_VERTEX) as f64,
        GridRoutingFallbackReason::AdjacencyCap,
    )?;
    Ok(VisibilityGraph {
        vertices,
        adjacency: immutable_adjacency,
        adjacency_entries,
    })
}

pub fn derive_portal_ranges(
    owner_id: &str,
    raw_bounds: RouterRect,
    raw_title: Option<RouterRect>,
) -> Result<Vec<PortalRange>, GridRoutingError> {
    let bounds = normalize_rect(raw_bounds)?;
    let title = raw_title.map(normalize_rect).transpose()?;
    let vertical_low = (bounds.top + ROUTE_CLEARANCE_PX)
        .max(title.map_or(f64::NEG_INFINITY, |title| title.bottom) + ROUTE_CLEARANCE_PX);
    let vertical_high = bounds.bottom - ROUTE_CLEARANCE_PX;
    let horizontal_low = bounds.left + ROUTE_CLEARANCE_PX;
    let horizontal_high = bounds.right - ROUTE_CLEARANCE_PX;
    let mut ranges: Vec<PortalRange> = Vec::new();
    if vertical_low <= vertical_high {
        ranges.push(PortalRange {
            owner_id: owner_id.to_string(),
            side: GridSide::Left,
            low: vertical_low,
            high: vertical_high,
            coordinate: None,
        });
        ranges.push(PortalRange {
            owner_id: owner_id.to_string(),
            side: GridSide::Right,
            low: vertical_low,
            high: vertical_high,
            coordinate: None,
        });
    }
    if horizontal_low <= horizontal_high {
        ranges.push(PortalRange {
            owner_id: owner_id.to_string(),
            side: GridSide::Bottom,
            low: horizontal_low,
            high: horizontal_high,
            coordinate: None,
        });
        let title_allows_top =
            title.is_none_or(|title| title.bottom + ROUTE_CLEARANCE_PX <= bounds.top);
        if title_allows_top {
            ranges.push(PortalRange {
                owner_id: owner_id.to_string(),
                side: GridSide::Top,
                low: horizontal_low,
                high: horizontal_high,
                coordinate: None,
            });
        }
    }
    Ok(ranges)
}

pub fn build_paired_portal(
    owner_id: &str,
    bounds: RouterRect,
    title: Option<RouterRect>,
    side: GridSide,
    tangential_coordinate: f64,
) -> Result<PairedPortal, GridRoutingError> {
    // Interior and exterior points sit one clearance unit from the group boundary. The explicit
    // transition is the only hierarchy-crossing arc and contributes one boundary transition to
    // cost.
    let range = derive_portal_ranges(owner_id, bounds, title)?
        .into_iter()
        .find(|entry| entry.side == side);
    let Some(range) = range else {
        return Err(GridRoutingError::geometry(format!(
            "Illegal {} portal for \"{owner_id}\" at {}",
            side.as_str(),
            js_number(tangential_coordinate)
        )));
    };
    if tangential_coordinate < range.low || tangential_coordinate > range.high {
        return Err(GridRoutingError::geometry(format!(
            "Illegal {} portal for \"{owner_id}\" at {}",
            side.as_str(),
            js_number(tangential_coordinate)
        )));
    }
    let boundary = match side {
        GridSide::Left => bounds.left,
        GridSide::Right => bounds.right,
        GridSide::Top => bounds.top,
        GridSide::Bottom => bounds.bottom,
    };
    let inward = if matches!(side, GridSide::Left | GridSide::Top) {
        boundary + ROUTE_CLEARANCE_PX
    } else {
        boundary - ROUTE_CLEARANCE_PX
    };
    let outward = if matches!(side, GridSide::Left | GridSide::Top) {
        boundary - ROUTE_CLEARANCE_PX
    } else {
        boundary + ROUTE_CLEARANCE_PX
    };
    let interior = if matches!(side, GridSide::Left | GridSide::Right) {
        RouterPoint::new(inward, tangential_coordinate)
    } else {
        RouterPoint::new(tangential_coordinate, inward)
    };
    let exterior = if matches!(side, GridSide::Left | GridSide::Right) {
        RouterPoint::new(outward, tangential_coordinate)
    } else {
        RouterPoint::new(tangential_coordinate, outward)
    };
    Ok(PairedPortal {
        owner_id: owner_id.to_string(),
        side,
        tangential_coordinate,
        interior,
        exterior,
        transition: PortalTransition {
            from: interior,
            to: exterior,
            orientation: if matches!(side, GridSide::Left | GridSide::Right) {
                GridOrientation::H
            } else {
                GridOrientation::V
            },
            length: ROUTE_CLEARANCE_PX * 2.0,
            kind: RouterArcKind::Portal,
        },
    })
}

pub fn build_container_routing_topology(
    input: &ContainerTopologyInput,
    options: BuildTopologyOptions<'_>,
) -> Result<ContainerRoutingTopology, GridRoutingError> {
    // Build phases are normalize/inflate, union obstacles, seed corners and portal endpoints,
    // project rays to blocking boundaries, then connect immediate line-of-sight neighbors.
    let bounds = normalize_rect(input.bounds)?;
    let inflated: Vec<RouterObstacle> = input
        .obstacles
        .iter()
        .chain(input.title_exclusions.iter())
        .map(|obstacle| inflate_and_clip(obstacle, bounds))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect();
    let obstacles = union_obstacles(&inflated);
    let mut portal_ranges = input.portal_ranges.clone();
    portal_ranges.sort_by(|a, b| {
        topology_side_ordinal(Some(a.side))
            .cmp(&topology_side_ordinal(Some(b.side)))
            .then_with(|| a.low.total_cmp(&b.low))
            .then_with(|| a.high.total_cmp(&b.high))
            .then_with(|| compare_code_units(&a.owner_id, &b.owner_id))
    });

    let mut seed_records: Vec<VertexRecord> =
        corner_records(bounds, &input.container_id, RouterVertexKind::Corner);
    for obstacle in &obstacles {
        seed_records.extend(corner_records(
            obstacle.bounds(),
            &obstacle.id,
            RouterVertexKind::Corner,
        ));
    }
    for range in &portal_ranges {
        seed_records.extend(portal_records(range, bounds));
    }
    let seeds = canonical_records(&seed_records);
    let max_estimated_bytes = options
        .caps
        .max_estimated_bytes
        .unwrap_or(DEFAULT_MAX_ROUTING_ESTIMATED_BYTES);
    let mut interval_budget = IntervalBudget {
        interval_count: 0,
        max_estimated_bytes,
    };
    enforce_cap(
        estimated_topology_bytes(0, 0, obstacles.len(), 0),
        interval_budget.max_estimated_bytes,
        GridRoutingFallbackReason::EstimatedMemoryCap,
    )?;
    let horizontal_intervals =
        create_interval_index(&obstacles, GridOrientation::H, &mut interval_budget)?;
    let vertical_intervals =
        create_interval_index(&obstacles, GridOrientation::V, &mut interval_budget)?;
    let mut projections: Vec<VertexRecord> = Vec::new();
    for seed in &seeds {
        for side in [
            GridSide::Left,
            GridSide::Right,
            GridSide::Top,
            GridSide::Bottom,
        ] {
            let hit = ray_hit(
                seed.point,
                side,
                bounds,
                &horizontal_intervals,
                &vertical_intervals,
            );
            if hit.x != seed.point.x || hit.y != seed.point.y {
                projections.push(VertexRecord {
                    point: hit,
                    kind: RouterVertexKind::Projection,
                    owner_id: None,
                    side: Some(side),
                });
            }
        }
    }

    let records: Vec<VertexRecord> = seeds.iter().chain(projections.iter()).cloned().collect();
    let VisibilityGraph {
        vertices,
        adjacency,
        adjacency_entries,
    } = build_visibility_graph(
        &records,
        &horizontal_intervals,
        &vertical_intervals,
        &options.caps,
    )?;

    let estimated_bytes = estimated_topology_bytes(
        vertices.len(),
        adjacency_entries,
        obstacles.len(),
        horizontal_intervals.interval_count + vertical_intervals.interval_count,
    );
    enforce_cap(
        estimated_bytes,
        max_estimated_bytes,
        GridRoutingFallbackReason::EstimatedMemoryCap,
    )?;

    if let Some(metrics) = options.metrics {
        metrics.containers_built += 1;
        metrics.base_topology_builds += 1;
        metrics.base_vertices += vertices.len();
        metrics.base_adjacency_entries += adjacency_entries;
        metrics.build_sweep_events += seeds.len() * 4 + obstacles.len() * 4;
        metrics.estimated_bytes += estimated_bytes;
    }

    let adjacency_by_vertex: Vec<Vec<RouterArc>> = vertices
        .iter()
        .map(|vertex| adjacency.get(&vertex.id).cloned().unwrap_or_default())
        .collect();
    let search_adjacency_by_vertex: Vec<Vec<RouterSearchArc>> = adjacency_by_vertex
        .iter()
        .map(|arcs| search_arcs(arcs))
        .collect();
    let point_vertex_ids: HashMap<String, usize> = vertices
        .iter()
        .map(|vertex| (routing_point_key(vertex.point), vertex.id))
        .collect();
    let horizontal_vertex_lines = index_vertex_lines(&vertices, GridOrientation::H);
    let vertical_vertex_lines = index_vertex_lines(&vertices, GridOrientation::V);

    Ok(ContainerRoutingTopology {
        container_id: input.container_id.clone(),
        bounds,
        obstacles,
        vertices: vertices.clone(),
        point_vertex_ids,
        horizontal_vertex_lines,
        vertical_vertex_lines,
        adjacency,
        adjacency_by_vertex,
        search_adjacency_by_vertex,
        adjacency_entries,
        horizontal_intervals: Arc::new(horizontal_intervals),
        vertical_intervals: Arc::new(vertical_intervals),
        portal_ranges,
        seed_count: seeds.len(),
        estimated_bytes,
        vertex_count: vertices.len(),
    })
}

fn point_inside_obstacle(
    point: RouterPoint,
    horizontal_intervals: &dyn OrthogonalIntervalIndex,
) -> bool {
    horizontal_intervals.contains(point.y, point.x)
}

pub fn build_endpoint_routing_overlay(
    base: &ContainerRoutingTopology,
    source: RouterPoint,
    target: RouterPoint,
    metrics: Option<&mut GridRoutingInstrumentation>,
    reusable_scratch: Option<&mut EndpointOverlayScratch>,
    lane_points: &[RouterPoint],
) -> Result<ContainerRoutingTopology, GridRoutingError> {
    // Overlays are intentionally tiny: two endpoints, optional bundle lane points, orthogonal
    // projections, and four obstacle/bounds ray hits per endpoint. Local caps enforce that
    // invariant.
    let mut owned_scratch;
    let scratch: &mut EndpointOverlayScratch = match reusable_scratch {
        Some(scratch) => scratch,
        None => {
            owned_scratch = EndpointOverlayScratch::new(base);
            &mut owned_scratch
        }
    };
    scratch.reset(base)?;
    let mut endpoint_records: Vec<VertexRecord> = vec![
        VertexRecord {
            point: source,
            kind: RouterVertexKind::Endpoint,
            owner_id: Some("source".to_string()),
            side: None,
        },
        VertexRecord {
            point: target,
            kind: RouterVertexKind::Endpoint,
            owner_id: Some("target".to_string()),
            side: None,
        },
    ];
    for point in lane_points {
        if !point_inside_obstacle(*point, &*base.horizontal_intervals) {
            endpoint_records.push(VertexRecord {
                point: *point,
                kind: RouterVertexKind::Lane,
                owner_id: None,
                side: None,
            });
        }
    }
    for point in [
        RouterPoint::new(source.x, target.y),
        RouterPoint::new(target.x, source.y),
    ] {
        if !point_inside_obstacle(point, &*base.horizontal_intervals) {
            endpoint_records.push(VertexRecord {
                point,
                kind: RouterVertexKind::Projection,
                owner_id: None,
                side: None,
            });
        }
    }
    let endpoint_seeds: Vec<VertexRecord> = endpoint_records.iter().take(2).cloned().collect();
    for record in &endpoint_seeds {
        for side in [
            GridSide::Left,
            GridSide::Right,
            GridSide::Top,
            GridSide::Bottom,
        ] {
            let hit = ray_hit(
                record.point,
                side,
                base.bounds,
                &*base.horizontal_intervals,
                &*base.vertical_intervals,
            );
            if hit.x != record.point.x || hit.y != record.point.y {
                endpoint_records.push(VertexRecord {
                    point: hit,
                    kind: RouterVertexKind::Projection,
                    owner_id: None,
                    side: Some(side),
                });
            }
        }
    }
    let mut additions: Vec<VertexRecord> = canonical_records(&endpoint_records)
        .into_iter()
        .filter(|record| {
            !base
                .point_vertex_ids
                .contains_key(&routing_point_key(record.point))
        })
        .collect();
    additions.sort_by(vertex_record_order);
    enforce_cap(
        additions.len() as f64,
        32.0,
        GridRoutingFallbackReason::VertexCap,
    )?;
    let base_vertex_count = base.vertices.len();
    for (index, record) in additions.iter().enumerate() {
        scratch.added_vertices.push(RouterVertex {
            id: base_vertex_count + index,
            point: record.point,
            kind: record.kind,
            owner_id: record.owner_id.clone(),
            side: record.side,
        });
    }
    let mut vertices = base.vertices.clone();
    vertices.extend(scratch.added_vertices.iter().cloned());
    let vertex_at = |id: usize| -> &RouterVertex { &vertices[id] };

    let mut adjacency: HashMap<usize, Vec<RouterArc>> = HashMap::new();
    for vertex in &scratch.added_vertices {
        adjacency.insert(vertex.id, Vec::new());
    }
    let added_ids: HashSet<usize> = scratch.added_vertices.iter().map(|v| v.id).collect();
    let mut added_arc_keys: HashSet<(usize, usize, GridOrientation)> = HashSet::new();
    for orientation in [GridOrientation::H, GridOrientation::V] {
        let base_lines = if orientation == GridOrientation::H {
            &base.horizontal_vertex_lines
        } else {
            &base.vertical_vertex_lines
        };
        let mut affected: HashSet<Coordinate> = HashSet::new();
        for vertex in &scratch.added_vertices {
            affected.insert(Coordinate::new(if orientation == GridOrientation::H {
                vertex.point.y
            } else {
                vertex.point.x
            }));
        }
        let mut affected: Vec<Coordinate> = affected.into_iter().collect();
        affected.sort();
        for fixed in affected {
            let mut line: Vec<usize> = base_lines.get(&fixed).cloned().unwrap_or_default();
            line.extend(
                scratch
                    .added_vertices
                    .iter()
                    .filter(|vertex| {
                        Coordinate::new(if orientation == GridOrientation::H {
                            vertex.point.y
                        } else {
                            vertex.point.x
                        }) == fixed
                    })
                    .map(|vertex| vertex.id),
            );
            line.sort_by(|&a, &b| {
                if orientation == GridOrientation::H {
                    vertex_at(a)
                        .point
                        .x
                        .total_cmp(&vertex_at(b).point.x)
                        .then_with(|| a.cmp(&b))
                } else {
                    vertex_at(a)
                        .point
                        .y
                        .total_cmp(&vertex_at(b).point.y)
                        .then_with(|| a.cmp(&b))
                }
            });
            for index in 1..line.len() {
                let a = line[index - 1];
                let b = line[index];
                if !added_ids.contains(&a) && !added_ids.contains(&b) {
                    continue;
                }
                let (start, end) = if orientation == GridOrientation::H {
                    (vertex_at(a).point.x, vertex_at(b).point.x)
                } else {
                    (vertex_at(a).point.y, vertex_at(b).point.y)
                };
                let interval_index: &dyn OrthogonalIntervalIndex =
                    if orientation == GridOrientation::H {
                        &*base.horizontal_intervals
                    } else {
                        &*base.vertical_intervals
                    };
                if interval_index.intersects(fixed.value(), start, end) {
                    continue;
                }
                let arc_length = (vertex_at(a).point.x - vertex_at(b).point.x).abs()
                    + (vertex_at(a).point.y - vertex_at(b).point.y).abs();
                if arc_length == 0.0 {
                    continue;
                }
                let interval_start = if orientation == GridOrientation::H {
                    vertex_at(a).point.x.min(vertex_at(b).point.x)
                } else {
                    vertex_at(a).point.y.min(vertex_at(b).point.y)
                };
                let interval_end = interval_start + arc_length;
                for (from, to) in [(a, b), (b, a)] {
                    let key = (from, to, orientation);
                    if added_arc_keys.contains(&key)
                        || base.adjacency.get(&from).is_some_and(|arcs| {
                            arcs.iter().any(|arc| {
                                arc.to == to
                                    && arc.orientation == orientation
                                    && arc.kind == RouterArcKind::Visibility
                            })
                        })
                    {
                        continue;
                    }
                    added_arc_keys.insert(key);
                    let arcs = adjacency
                        .entry(from)
                        .or_insert_with(|| base.adjacency.get(&from).cloned().unwrap_or_default());
                    arcs.push(RouterArc {
                        from,
                        to,
                        orientation,
                        length: arc_length,
                        kind: RouterArcKind::Visibility,
                        interval_start,
                        interval_end,
                    });
                }
            }
        }
    }

    for (id, arcs) in adjacency.iter_mut() {
        arcs.sort_by(|a, b| {
            compare_code_units(a.kind.as_str(), b.kind.as_str())
                .then_with(|| compare_code_units(a.orientation.as_str(), b.orientation.as_str()))
                .then_with(|| vertex_at(a.to).point.x.total_cmp(&vertex_at(b.to).point.x))
                .then_with(|| vertex_at(a.to).point.y.total_cmp(&vertex_at(b.to).point.y))
                .then_with(|| a.to.cmp(&b.to))
        });
        let immutable_arcs = arcs.clone();
        let id = *id;
        scratch.set_arcs(id, immutable_arcs);
    }
    let adjacency_entries = base.adjacency_entries + added_arc_keys.len();
    enforce_cap(
        adjacency_entries as f64,
        (base.adjacency_entries + 64) as f64,
        GridRoutingFallbackReason::AdjacencyCap,
    )?;
    let added_vertices = additions.len();
    let estimated_bytes = base.estimated_bytes
        + added_vertices as f64 * 64.0
        + (adjacency_entries - base.adjacency_entries) as f64 * 56.0;
    if let Some(metrics) = metrics {
        metrics.endpoint_overlay_builds += 1;
        metrics.endpoint_overlay_vertices += added_vertices;
        metrics.estimated_bytes = metrics.estimated_bytes.max(estimated_bytes);
    }

    let mut point_vertex_ids = base.point_vertex_ids.clone();
    for vertex in &scratch.added_vertices {
        point_vertex_ids.insert(routing_point_key(vertex.point), vertex.id);
        scratch
            .point_overrides
            .insert(routing_point_key(vertex.point), vertex.id);
    }
    let adjacency_by_vertex: Vec<Vec<RouterArc>> = (0..vertices.len())
        .map(|id| {
            scratch
                .adjacency_overrides
                .get(&id)
                .cloned()
                .unwrap_or_else(|| {
                    base.adjacency_by_vertex
                        .get(id)
                        .cloned()
                        .unwrap_or_default()
                })
        })
        .collect();
    let search_adjacency_by_vertex: Vec<Vec<RouterSearchArc>> = (0..vertices.len())
        .map(|id| {
            scratch
                .search_adjacency_overrides
                .get(&id)
                .cloned()
                .unwrap_or_else(|| {
                    base.search_adjacency_by_vertex
                        .get(id)
                        .cloned()
                        .unwrap_or_default()
                })
        })
        .collect();
    let mut merged_adjacency = base.adjacency.clone();
    for (id, arcs) in &scratch.adjacency_overrides {
        merged_adjacency.insert(*id, arcs.clone());
    }

    Ok(ContainerRoutingTopology {
        container_id: base.container_id.clone(),
        bounds: base.bounds,
        obstacles: base.obstacles.clone(),
        vertices: vertices.clone(),
        point_vertex_ids,
        horizontal_vertex_lines: base.horizontal_vertex_lines.clone(),
        vertical_vertex_lines: base.vertical_vertex_lines.clone(),
        adjacency: merged_adjacency,
        adjacency_by_vertex,
        search_adjacency_by_vertex,
        adjacency_entries,
        horizontal_intervals: base.horizontal_intervals.clone(),
        vertical_intervals: base.vertical_intervals.clone(),
        portal_ranges: base.portal_ranges.clone(),
        seed_count: base.seed_count,
        estimated_bytes,
        vertex_count: vertices.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::grid::router_instrumentation::create_grid_routing_instrumentation;

    fn point(x: f64, y: f64) -> RouterPoint {
        RouterPoint::new(x, y)
    }

    fn obstacle(id: &str, left: f64, right: f64, top: f64, bottom: f64) -> RouterObstacleInput {
        RouterObstacleInput {
            id: id.to_string(),
            bounds: RouterRect {
                left,
                right,
                top,
                bottom,
            },
        }
    }

    fn topology(obstacles: Vec<RouterObstacleInput>) -> ContainerRoutingTopology {
        build_container_routing_topology(
            &ContainerTopologyInput {
                container_id: "__grid_root__".to_string(),
                bounds: RouterRect {
                    left: 0.0,
                    right: 100.0,
                    top: 0.0,
                    bottom: 100.0,
                },
                obstacles,
                ..ContainerTopologyInput::default()
            },
            BuildTopologyOptions::default(),
        )
        .unwrap()
    }

    #[test]
    fn normalizes_negative_zero_in_routing_point_keys() {
        assert_eq!(routing_point_key(point(-0.0, -0.0)), "0:0");
    }

    #[test]
    fn matches_a_brute_force_obstacle_union_and_interval_oracle_on_randomized_cases() {
        struct Lcg(u32);
        impl Lcg {
            fn next(&mut self) -> f64 {
                self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                f64::from(self.0) / 4_294_967_296.0
            }
        }
        let mut random = Lcg(0x71ab_19e3);

        for case_index in 0..500 {
            let count = 1 + (random.next() * 12.0).floor() as usize;
            let mut records: Vec<RouterObstacleInput> = Vec::new();
            for index in 0..count {
                let left = (random.next() * 80.0).floor();
                let top = (random.next() * 80.0).floor();
                let right = left + 1.0 + (random.next() * 20.0).floor();
                let bottom = top + 1.0 + (random.next() * 20.0).floor();
                records.push(obstacle(&format!("o-{index}"), left, right, top, bottom));
            }
            let result = topology(records.clone());
            let inflated: Vec<RouterRect> = records
                .iter()
                .map(|obstacle| RouterRect {
                    left: 0.0f64.max(obstacle.bounds.left - 6.0),
                    right: 100.0f64.min(obstacle.bounds.right + 6.0),
                    top: 0.0f64.max(obstacle.bounds.top - 6.0),
                    bottom: 100.0f64.min(obstacle.bounds.bottom + 6.0),
                })
                .collect();

            for _ in 0..50 {
                let coordinate = (random.next() * 101.0).floor();
                let start = (random.next() * 101.0).floor();
                let end = (random.next() * 101.0).floor();
                let low = start.min(end);
                let high = start.max(end);
                let blocked = |orientation: GridOrientation| -> bool {
                    // A point probe on an outline is legal. A span is blocked only where the
                    // obstacle union has interior on both sides of the sweep line.
                    let is_boundary = inflated.iter().any(|obstacle| {
                        if orientation == GridOrientation::H {
                            obstacle.top == coordinate || obstacle.bottom == coordinate
                        } else {
                            obstacle.left == coordinate || obstacle.right == coordinate
                        }
                    });
                    if is_boundary && low == high {
                        return false;
                    }
                    let before: Vec<&RouterRect> = inflated
                        .iter()
                        .filter(|obstacle| {
                            if orientation == GridOrientation::H {
                                obstacle.top < coordinate && obstacle.bottom >= coordinate
                            } else {
                                obstacle.left < coordinate && obstacle.right >= coordinate
                            }
                        })
                        .collect();
                    let after: Vec<&RouterRect> = inflated
                        .iter()
                        .filter(|obstacle| {
                            if orientation == GridOrientation::H {
                                obstacle.top <= coordinate && obstacle.bottom > coordinate
                            } else {
                                obstacle.left <= coordinate && obstacle.right > coordinate
                            }
                        })
                        .collect();
                    before.iter().any(|first| {
                        after.iter().any(|second| {
                            let (first_low, first_high) = if orientation == GridOrientation::H {
                                (first.left, first.right)
                            } else {
                                (first.top, first.bottom)
                            };
                            let (second_low, second_high) = if orientation == GridOrientation::H {
                                (second.left, second.right)
                            } else {
                                (second.top, second.bottom)
                            };
                            if low == high {
                                low > first_low
                                    && low < first_high
                                    && low > second_low
                                    && low < second_high
                            } else {
                                low.max(first_low).max(second_low)
                                    < high.min(first_high).min(second_high)
                            }
                        })
                    })
                };
                let horizontal = blocked(GridOrientation::H);
                let vertical = blocked(GridOrientation::V);
                assert_eq!(
                    result
                        .horizontal_intervals
                        .intersects(coordinate, start, end),
                    horizontal,
                    "horizontal case={case_index} coordinate={coordinate} range={start}:{end}"
                );
                assert_eq!(
                    result.vertical_intervals.intersects(coordinate, start, end),
                    vertical,
                    "vertical case={case_index} coordinate={coordinate} range={start}:{end}"
                );
            }

            let mut reversed = records;
            reversed.reverse();
            let rebuilt = topology(reversed);
            assert_eq!(rebuilt.obstacles, result.obstacles);
            assert_eq!(rebuilt.vertices, result.vertices);
            assert_eq!(rebuilt.adjacency, result.adjacency);
        }
    }

    #[test]
    fn uses_inflated_measured_geometry_and_title_exclusions_as_obstacles() {
        let result = build_container_routing_topology(
            &ContainerTopologyInput {
                container_id: "group".to_string(),
                bounds: RouterRect {
                    left: 0.0,
                    right: 120.0,
                    top: 0.0,
                    bottom: 100.0,
                },
                obstacles: vec![obstacle("leaf", 20.0, 40.0, 30.0, 50.0)],
                title_exclusions: vec![obstacle("group-title", 50.0, 90.0, 4.0, 14.0)],
                portal_ranges: Vec::new(),
            },
            BuildTopologyOptions::default(),
        )
        .unwrap();

        assert_eq!(
            result.obstacles,
            vec![
                RouterObstacle {
                    id: "group-title".to_string(),
                    left: 44.0,
                    right: 96.0,
                    top: 0.0,
                    bottom: 20.0,
                },
                RouterObstacle {
                    id: "leaf".to_string(),
                    left: 14.0,
                    right: 46.0,
                    top: 24.0,
                    bottom: 56.0,
                },
            ]
        );
        assert!(result.horizontal_intervals.intersects(40.0, 15.0, 45.0));
        assert!(!result.horizontal_intervals.intersects(70.0, 15.0, 45.0));
        assert!(result.vertical_intervals.intersects(30.0, 25.0, 55.0));
    }

    #[test]
    fn normalizes_overlapping_inflated_obstacles_into_disjoint_rectangles() {
        let result = topology(vec![
            obstacle("b", 30.0, 60.0, 30.0, 60.0),
            obstacle("a", 50.0, 75.0, 40.0, 70.0),
        ]);

        for first in 0..result.obstacles.len() {
            for second in first + 1..result.obstacles.len() {
                let a = &result.obstacles[first];
                let b = &result.obstacles[second];
                let overlap_width = a.right.min(b.right) - a.left.max(b.left);
                let overlap_height = a.bottom.min(b.bottom) - a.top.max(b.top);
                assert!(!(overlap_width > 0.0 && overlap_height > 0.0));
            }
        }
    }

    #[test]
    fn marks_a_shared_boundary_inside_the_obstacle_union_as_blocked() {
        let result = topology(vec![
            obstacle("left", 20.0, 44.0, 30.0, 70.0),
            obstacle("right", 56.0, 80.0, 30.0, 70.0),
        ]);

        assert_eq!(
            result.obstacles,
            vec![RouterObstacle {
                id: "left".to_string(),
                left: 14.0,
                right: 86.0,
                top: 24.0,
                bottom: 76.0,
            }]
        );
        assert!(result.vertical_intervals.intersects(50.0, 24.0, 76.0));
    }

    #[test]
    fn adds_nearest_visible_projections_without_cartesian_line_intersections() {
        let result = topology(vec![
            obstacle("center", 40.0, 60.0, 40.0, 60.0),
            obstacle("lower-right", 75.0, 85.0, 70.0, 80.0),
        ]);
        let points: Vec<RouterPoint> = result.vertices.iter().map(|vertex| vertex.point).collect();

        assert!(points.contains(&point(0.0, 34.0)));
        assert!(points.contains(&point(34.0, 0.0)));
        assert!(!points.contains(&point(34.0, 64.0)));
        assert!(result.vertices.len() <= 5 * result.seed_count + 32);
        assert!(result.adjacency_entries <= 4 * result.vertices.len());
    }

    #[test]
    fn assigns_stable_exact_ordinals_and_sorted_immutable_adjacency() {
        let forward = topology(vec![
            obstacle("a", 10.25, 20.5, -0.0, 25.75),
            obstacle("b", 70.125, 80.875, 65.5, 90.25),
        ]);
        let reverse = topology(vec![
            obstacle("b", 70.125, 80.875, 65.5, 90.25),
            obstacle("a", 10.25, 20.5, -0.0, 25.75),
        ]);

        assert_eq!(reverse.vertices, forward.vertices);
        assert_eq!(reverse.adjacency, forward.adjacency);
        for vertex in &forward.vertices {
            assert!(vertex.point.x.to_bits() != (-0.0f64).to_bits());
            assert!(vertex.point.y.to_bits() != (-0.0f64).to_bits());
        }

        for arcs in forward.adjacency.values() {
            let mut sorted = arcs.clone();
            sorted.sort_by(|a, b| {
                compare_code_units(a.kind.as_str(), b.kind.as_str())
                    .then_with(|| {
                        compare_code_units(a.orientation.as_str(), b.orientation.as_str())
                    })
                    .then_with(|| {
                        forward.vertices[a.to]
                            .point
                            .x
                            .total_cmp(&forward.vertices[b.to].point.x)
                    })
                    .then_with(|| {
                        forward.vertices[a.to]
                            .point
                            .y
                            .total_cmp(&forward.vertices[b.to].point.y)
                    })
                    .then_with(|| a.to.cmp(&b.to))
            });
            assert_eq!(arcs, &sorted);
        }
    }

    #[test]
    fn derives_title_safe_portal_ranges_with_exact_corner_clearance() {
        let ranges = derive_portal_ranges(
            "group",
            RouterRect {
                left: 10.0,
                right: 110.0,
                top: 20.0,
                bottom: 100.0,
            },
            Some(RouterRect {
                left: 10.0,
                right: 110.0,
                top: 20.0,
                bottom: 42.0,
            }),
        )
        .unwrap();

        assert_eq!(
            ranges,
            vec![
                PortalRange {
                    owner_id: "group".to_string(),
                    side: GridSide::Left,
                    low: 48.0,
                    high: 94.0,
                    coordinate: None,
                },
                PortalRange {
                    owner_id: "group".to_string(),
                    side: GridSide::Right,
                    low: 48.0,
                    high: 94.0,
                    coordinate: None,
                },
                PortalRange {
                    owner_id: "group".to_string(),
                    side: GridSide::Bottom,
                    low: 16.0,
                    high: 104.0,
                    coordinate: None,
                },
            ]
        );
    }

    #[test]
    fn builds_perpendicular_paired_portals_exactly_six_pixels_inside_and_outside_the_frame() {
        let bounds = RouterRect {
            left: 10.0,
            right: 110.0,
            top: 20.0,
            bottom: 100.0,
        };
        let title = Some(RouterRect {
            left: 10.0,
            right: 110.0,
            top: 20.0,
            bottom: 42.0,
        });

        assert_eq!(
            build_paired_portal("group", bounds, title, GridSide::Right, 48.0).unwrap(),
            PairedPortal {
                owner_id: "group".to_string(),
                side: GridSide::Right,
                tangential_coordinate: 48.0,
                interior: point(104.0, 48.0),
                exterior: point(116.0, 48.0),
                transition: PortalTransition {
                    from: point(104.0, 48.0),
                    to: point(116.0, 48.0),
                    orientation: GridOrientation::H,
                    length: 12.0,
                    kind: RouterArcKind::Portal,
                },
            }
        );
        assert!(
            build_paired_portal("group", bounds, title, GridSide::Right, 47.0)
                .unwrap_err()
                .to_string()
                .contains("Illegal right portal")
        );
        assert!(
            build_paired_portal("group", bounds, title, GridSide::Top, 60.0)
                .unwrap_err()
                .to_string()
                .contains("Illegal top portal")
        );
    }

    #[test]
    fn reports_repeatable_topology_counts_for_identical_geometry() {
        let mut first_metrics = create_grid_routing_instrumentation();
        let mut second_metrics = create_grid_routing_instrumentation();
        let input = ContainerTopologyInput {
            container_id: "__grid_root__".to_string(),
            bounds: RouterRect {
                left: 0.0,
                right: 220.0,
                top: 0.0,
                bottom: 100.0,
            },
            obstacles: vec![
                obstacle("first", 20.0, 80.0, 30.0, 70.0),
                obstacle("second", 140.0, 200.0, 30.0, 70.0),
            ],
            ..ContainerTopologyInput::default()
        };

        let first = build_container_routing_topology(
            &input,
            BuildTopologyOptions {
                metrics: Some(&mut first_metrics),
                ..BuildTopologyOptions::default()
            },
        )
        .unwrap();
        let second = build_container_routing_topology(
            &input,
            BuildTopologyOptions {
                metrics: Some(&mut second_metrics),
                ..BuildTopologyOptions::default()
            },
        )
        .unwrap();

        assert_eq!(
            (
                first.vertices.len(),
                first.adjacency_entries,
                first_metrics.build_sweep_events
            ),
            (
                second.vertices.len(),
                second.adjacency_entries,
                second_metrics.build_sweep_events
            )
        );
        assert_eq!(first_metrics.base_topology_builds, 1);
        assert_eq!(first_metrics.containers_built, 1);
        assert!(first_metrics.estimated_bytes > 0.0);
    }

    #[test]
    fn counts_strict_boundary_intervals_and_enforces_their_memory_cap_during_construction() {
        let input = ContainerTopologyInput {
            container_id: "__grid_root__".to_string(),
            bounds: RouterRect {
                left: 0.0,
                right: 100.0,
                top: 0.0,
                bottom: 100.0,
            },
            obstacles: vec![
                obstacle("first", 10.0, 30.0, 10.0, 40.0),
                obstacle("second", 60.0, 80.0, 30.0, 70.0),
            ],
            ..ContainerTopologyInput::default()
        };

        let topology =
            build_container_routing_topology(&input, BuildTopologyOptions::default()).unwrap();

        assert_eq!(topology.horizontal_intervals.interval_count(), 6);
        assert_eq!(topology.vertical_intervals.interval_count(), 2);
        let error = build_container_routing_topology(
            &input,
            BuildTopologyOptions {
                caps: TopologyResourceCaps {
                    max_estimated_bytes: Some(250.0),
                    ..TopologyResourceCaps::default()
                },
                ..BuildTopologyOptions::default()
            },
        )
        .unwrap_err();
        match error {
            GridRoutingError::ResourceLimit(error) => {
                assert_eq!(error.reason, GridRoutingFallbackReason::EstimatedMemoryCap);
                assert_eq!(
                    error.message,
                    "Grid routing estimated_memory_cap exceeded: 264 > 250"
                );
            }
            GridRoutingError::Geometry(message) => panic!("unexpected geometry error: {message}"),
            GridRoutingError::Grid(error) => panic!("unexpected grid error: {error}"),
        }
    }

    #[test]
    fn reports_the_deterministic_resource_cap() {
        let cases = [
            (
                GridRoutingFallbackReason::VertexCap,
                TopologyResourceCaps {
                    max_vertices: Some(1),
                    ..TopologyResourceCaps::default()
                },
            ),
            (
                GridRoutingFallbackReason::AdjacencyCap,
                TopologyResourceCaps {
                    max_adjacency_entries: Some(1),
                    ..TopologyResourceCaps::default()
                },
            ),
            (
                GridRoutingFallbackReason::EstimatedMemoryCap,
                TopologyResourceCaps {
                    max_estimated_bytes: Some(1.0),
                    ..TopologyResourceCaps::default()
                },
            ),
        ];
        for (expected_reason, caps) in cases {
            let input = ContainerTopologyInput {
                container_id: "__grid_root__".to_string(),
                bounds: RouterRect {
                    left: 0.0,
                    right: 100.0,
                    top: 0.0,
                    bottom: 100.0,
                },
                obstacles: vec![obstacle("center", 40.0, 60.0, 40.0, 60.0)],
                ..ContainerTopologyInput::default()
            };
            let error = build_container_routing_topology(
                &input,
                BuildTopologyOptions {
                    caps,
                    ..BuildTopologyOptions::default()
                },
            )
            .unwrap_err();
            match error {
                GridRoutingError::ResourceLimit(error) => {
                    assert_eq!(error.reason, expected_reason);
                }
                GridRoutingError::Geometry(message) => {
                    panic!("unexpected geometry error: {message}")
                }
                GridRoutingError::Grid(error) => panic!("unexpected grid error: {error}"),
            }
        }
    }
}
