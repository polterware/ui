//! Port of Mermaid 12.3.0's
//! `rendering-util/layout-algorithms/grid/edgeLabels.ts`.
//!
//! `prepareGridLayout` turns edge text into measured helper nodes before sizing; after routing,
//! [`position_grid_edge_labels`] places each helper on its owning edge, rerouting edges
//! transactionally around labels, and stores the final helper positions. The two-pass placement
//! search, the shared index-work budget, the frozen reservation set, and the rollback contract are
//! the upstream ones. The f64 arithmetic keeps upstream's order so fixtures match exactly.
//!
//! Upstream's `log.warn` on placement exhaustion has no counterpart here: the port records
//! `placementSearchFallbacks` and the routing fallback reason, which is what callers observe.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use serde_json::json;

use crate::layout::grid::geometry::{
    Rect, Segment, SegmentOrientation, clamp, normalize_polyline, polyline_intersects_rect,
    rect_for_node, segment_intersects_rect_interior,
};
use crate::layout::grid::label_geometry::{
    GRID_EDGE_END_MARKER_CLEARANCE, GRID_LABEL_CLEARANCE, grid_label_required_segment_length,
};
use crate::layout::grid::router_instrumentation::{
    GridRoutingFallbackReason, GridRoutingInstrumentation,
};
use crate::layout::grid::types::{
    Edge, GRID_LABEL_PREFIX, GridError, GridErrorCode, LayoutData, Node, RouterPoint,
    compare_code_units, grid_error, is_edge_label_node, is_finite_positive_number,
};

const LABEL_CLEARANCE: f64 = GRID_LABEL_CLEARANCE;
const LABEL_GUTTER: f64 = 14.0;
const LABEL_GUTTER_STEP: f64 = 18.0;
const MAX_GUTTER_STEPS: usize = 16;
const MAX_DETOUR_LINE_CANDIDATES: usize = 128;
const MAX_DETOUR_CENTER_CANDIDATES: usize = 64;
// Match the router's invocation-wide search budget: retries share one deterministic work ceiling.
const DEFAULT_MAX_INDEX_WORK: f64 = 2_000_000.0;
const EDGE_END_MARKER_CLEARANCE: f64 = GRID_EDGE_END_MARKER_CLEARANCE;
const LABEL_EPSILON: f64 = 1e-6;
const MIN_SELF_PARALLEL_GAP: f64 = 7.0;
const MIN_SELF_PARALLEL_OVERLAP: f64 = 8.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Interval {
    start: f64,
    end: f64,
}

#[derive(Clone, Debug, PartialEq)]
struct PlacedLabel {
    edge_id: String,
    node_id: String,
    center: RouterPoint,
    rect: Rect,
}

#[derive(Clone, Debug, PartialEq)]
struct SegmentCandidate {
    segment: Segment,
    segment_index: usize,
    order: usize,
}

#[derive(Clone, Debug)]
struct LabelWorkItem {
    edge_index: usize,
    label_node_index: usize,
    source_index: usize,
}

#[derive(Clone, Debug, Default)]
struct EdgePointOverrides {
    entries: Vec<(String, Vec<RouterPoint>)>,
}

impl EdgePointOverrides {
    fn get(&self, edge_id: &str) -> Option<&Vec<RouterPoint>> {
        self.entries
            .iter()
            .find(|(id, _)| id == edge_id)
            .map(|(_, points)| points)
    }

    fn set(&mut self, edge_id: &str, points: Vec<RouterPoint>) {
        if let Some((_, existing)) = self.entries.iter_mut().find(|(id, _)| id == edge_id) {
            *existing = points;
        } else {
            self.entries.push((edge_id.to_string(), points));
        }
    }

    fn has(&self, edge_id: &str) -> bool {
        self.entries.iter().any(|(id, _)| id == edge_id)
    }

    fn keys(&self) -> Vec<String> {
        self.entries.iter().map(|(id, _)| id.clone()).collect()
    }
}

#[derive(Clone, Debug, Default)]
struct PlacedLabels {
    entries: Vec<PlacedLabel>,
}

impl PlacedLabels {
    fn get(&self, edge_id: &str) -> Option<&PlacedLabel> {
        self.entries.iter().find(|label| label.edge_id == edge_id)
    }

    fn set(&mut self, label: PlacedLabel) {
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|existing| existing.edge_id == label.edge_id)
        {
            *existing = label;
        } else {
            self.entries.push(label);
        }
    }

    fn has(&self, edge_id: &str) -> bool {
        self.get(edge_id).is_some()
    }

    fn values(&self) -> Vec<PlacedLabel> {
        self.entries.clone()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Bounds {
    left: f64,
    right: f64,
    top: f64,
    bottom: f64,
}

#[derive(Clone, Debug, PartialEq)]
struct ObstacleEntry {
    id: String,
    bounds: Bounds,
    rect: Rect,
    node_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
struct BorderEntry {
    id: String,
    bounds: Bounds,
    rect: Rect,
}

#[derive(Clone, Debug, PartialEq)]
struct EdgeSegmentEntry {
    id: String,
    bounds: Bounds,
    edge_id: String,
    segment: Segment,
}

#[derive(Clone, Debug, PartialEq)]
struct EdgeGeometryCache {
    points: Vec<RouterPoint>,
    segments: Vec<Segment>,
    candidate_segments: Vec<SegmentCandidate>,
    marker_rects: Vec<Rect>,
}

/// Edge-label work counters, upstream's `GridEdgeLabelInstrumentation`.
#[derive(Clone, Debug, Default)]
pub struct GridEdgeLabelInstrumentation {
    pub obstacle_rect_queries: usize,
    pub obstacle_band_queries: usize,
    pub obstacle_polyline_queries: usize,
    pub obstacle_candidates_visited: usize,
    pub segment_rect_queries: usize,
    pub segment_band_queries: usize,
    pub segment_candidates_visited: usize,
    pub foreign_edge_lookups: usize,
    pub label_passes: usize,
    pub frozen_reservations: usize,
    pub impacted_edge_reroutes: usize,
    pub preserved_anchors: usize,
    pub reroute_candidates_evaluated: usize,
    pub reroute_rejected_blocked_rect: usize,
    pub reroute_rejected_protected_obstacle: usize,
    pub reroute_rejected_label_anchor: usize,
    pub reroute_rejected_degenerate_span: usize,
    pub reroute_partial_progress_candidates: usize,
    pub individual_label_reroute_attempts: usize,
    pub individual_label_reroute_successes: usize,
    pub label_overlap_fallbacks: usize,
    pub label_overlap_reservations: usize,
    pub degenerate_label_routes_skipped: usize,
    pub unmeasured_labels_skipped: usize,
    pub index_work_units: usize,
    pub index_work_limit_fallbacks: usize,
    pub placement_search_fallbacks: usize,
    pub rollbacks: usize,
    pub index_coordinate_count: usize,
}

/// Upstream's `GridEdgeLabelOptions`.
#[derive(Clone, Debug, Default)]
pub struct GridEdgeLabelOptions {
    pub max_index_work: Option<f64>,
}

/// Upstream's `createGridEdgeLabelInstrumentation`.
pub fn create_grid_edge_label_instrumentation() -> GridEdgeLabelInstrumentation {
    GridEdgeLabelInstrumentation::default()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LabelMetric {
    ObstacleRectQueries,
    ObstacleBandQueries,
    ObstaclePolylineQueries,
    ObstacleCandidatesVisited,
    SegmentRectQueries,
    SegmentBandQueries,
    SegmentCandidatesVisited,
    ForeignEdgeLookups,
    LabelPasses,
    FrozenReservations,
    ImpactedEdgeReroutes,
    PreservedAnchors,
    RerouteCandidatesEvaluated,
    RerouteRejectedBlockedRect,
    RerouteRejectedProtectedObstacle,
    RerouteRejectedLabelAnchor,
    RerouteRejectedDegenerateSpan,
    ReroutePartialProgressCandidates,
    IndividualLabelRerouteAttempts,
    IndividualLabelRerouteSuccesses,
    LabelOverlapFallbacks,
    LabelOverlapReservations,
    DegenerateLabelRoutesSkipped,
    UnmeasuredLabelsSkipped,
    IndexWorkUnits,
    IndexWorkLimitFallbacks,
    PlacementSearchFallbacks,
    Rollbacks,
    IndexCoordinateCount,
}

fn increment_metric(
    metrics: &Rc<RefCell<GridEdgeLabelInstrumentation>>,
    key: LabelMetric,
    delta: usize,
) {
    let mut metrics = metrics.borrow_mut();
    let field = match key {
        LabelMetric::ObstacleRectQueries => &mut metrics.obstacle_rect_queries,
        LabelMetric::ObstacleBandQueries => &mut metrics.obstacle_band_queries,
        LabelMetric::ObstaclePolylineQueries => &mut metrics.obstacle_polyline_queries,
        LabelMetric::ObstacleCandidatesVisited => &mut metrics.obstacle_candidates_visited,
        LabelMetric::SegmentRectQueries => &mut metrics.segment_rect_queries,
        LabelMetric::SegmentBandQueries => &mut metrics.segment_band_queries,
        LabelMetric::SegmentCandidatesVisited => &mut metrics.segment_candidates_visited,
        LabelMetric::ForeignEdgeLookups => &mut metrics.foreign_edge_lookups,
        LabelMetric::LabelPasses => &mut metrics.label_passes,
        LabelMetric::FrozenReservations => &mut metrics.frozen_reservations,
        LabelMetric::ImpactedEdgeReroutes => &mut metrics.impacted_edge_reroutes,
        LabelMetric::PreservedAnchors => &mut metrics.preserved_anchors,
        LabelMetric::RerouteCandidatesEvaluated => &mut metrics.reroute_candidates_evaluated,
        LabelMetric::RerouteRejectedBlockedRect => &mut metrics.reroute_rejected_blocked_rect,
        LabelMetric::RerouteRejectedProtectedObstacle => {
            &mut metrics.reroute_rejected_protected_obstacle
        }
        LabelMetric::RerouteRejectedLabelAnchor => &mut metrics.reroute_rejected_label_anchor,
        LabelMetric::RerouteRejectedDegenerateSpan => &mut metrics.reroute_rejected_degenerate_span,
        LabelMetric::ReroutePartialProgressCandidates => {
            &mut metrics.reroute_partial_progress_candidates
        }
        LabelMetric::IndividualLabelRerouteAttempts => {
            &mut metrics.individual_label_reroute_attempts
        }
        LabelMetric::IndividualLabelRerouteSuccesses => {
            &mut metrics.individual_label_reroute_successes
        }
        LabelMetric::LabelOverlapFallbacks => &mut metrics.label_overlap_fallbacks,
        LabelMetric::LabelOverlapReservations => &mut metrics.label_overlap_reservations,
        LabelMetric::DegenerateLabelRoutesSkipped => &mut metrics.degenerate_label_routes_skipped,
        LabelMetric::UnmeasuredLabelsSkipped => &mut metrics.unmeasured_labels_skipped,
        LabelMetric::IndexWorkUnits => &mut metrics.index_work_units,
        LabelMetric::IndexWorkLimitFallbacks => &mut metrics.index_work_limit_fallbacks,
        LabelMetric::PlacementSearchFallbacks => &mut metrics.placement_search_fallbacks,
        LabelMetric::Rollbacks => &mut metrics.rollbacks,
        LabelMetric::IndexCoordinateCount => &mut metrics.index_coordinate_count,
    };
    *field += delta;
}

struct LabelIndexWorkBudget {
    used: f64,
    limit: f64,
    metrics: Rc<RefCell<GridEdgeLabelInstrumentation>>,
}

impl LabelIndexWorkBudget {
    fn charge(&mut self, units: f64) -> Result<(), LabelError> {
        if units <= 0.0 {
            return Ok(());
        }
        if self.used + units > self.limit {
            return Err(LabelError::WorkLimit);
        }
        self.used += units;
        increment_metric(&self.metrics, LabelMetric::IndexWorkUnits, units as usize);
        Ok(())
    }
}

trait BoundsEntry: Clone {
    fn id(&self) -> &str;
    fn bounds(&self) -> Bounds;
}

impl BoundsEntry for ObstacleEntry {
    fn id(&self) -> &str {
        &self.id
    }
    fn bounds(&self) -> Bounds {
        self.bounds
    }
}

impl BoundsEntry for BorderEntry {
    fn id(&self) -> &str {
        &self.id
    }
    fn bounds(&self) -> Bounds {
        self.bounds
    }
}

impl BoundsEntry for EdgeSegmentEntry {
    fn id(&self) -> &str {
        &self.id
    }
    fn bounds(&self) -> Bounds {
        self.bounds
    }
}

/// A dynamic coordinate-compressed bounds index. Entries are keyed by their actual boundaries; no
/// bucket is allocated for any coordinate between them. Queries charge the shared work budget
/// before sorting so the cap is independent of the engine's comparison order.
struct CompressedBoundsIndex<T: BoundsEntry> {
    entries_by_id: HashMap<String, T>,
    sorted_by_left: Vec<T>,
    dirty: bool,
    work_budget: Rc<RefCell<LabelIndexWorkBudget>>,
    metrics: Rc<RefCell<GridEdgeLabelInstrumentation>>,
}

impl<T: BoundsEntry> CompressedBoundsIndex<T> {
    fn new(
        work_budget: Rc<RefCell<LabelIndexWorkBudget>>,
        metrics: Rc<RefCell<GridEdgeLabelInstrumentation>>,
    ) -> Self {
        Self {
            entries_by_id: HashMap::new(),
            sorted_by_left: Vec::new(),
            dirty: false,
            work_budget,
            metrics,
        }
    }

    fn upsert(&mut self, entry: T) {
        let is_new = !self.entries_by_id.contains_key(entry.id());
        self.entries_by_id.insert(entry.id().to_string(), entry);
        self.dirty = true;
        if is_new {
            // Four stored boundary coordinates per entry, independent of their span.
            increment_metric(&self.metrics, LabelMetric::IndexCoordinateCount, 4);
        }
    }

    fn remove(&mut self, id: &str) {
        if self.entries_by_id.remove(id).is_some() {
            self.dirty = true;
        }
    }

    fn query(&mut self, bounds: Bounds) -> Result<Vec<T>, LabelError> {
        if self.dirty {
            let entry_count = self.entries_by_id.len();
            // A deterministic n log n estimate before sorting.
            self.work_budget.borrow_mut().charge(
                entry_count as f64 * (1.0_f64).max((entry_count as f64 + 1.0).log2().ceil()),
            )?;
            let mut entries: Vec<T> = self.entries_by_id.values().cloned().collect();
            entries.sort_by(|a, b| {
                a.bounds()
                    .left
                    .partial_cmp(&b.bounds().left)
                    .unwrap_or(Ordering::Equal)
                    .then_with(|| compare_code_units(a.id(), b.id()))
            });
            self.sorted_by_left = entries;
            self.dirty = false;
        }

        let mut result = Vec::new();
        for entry in &self.sorted_by_left {
            self.work_budget.borrow_mut().charge(1.0)?;
            if entry.bounds().left > bounds.right {
                break;
            }
            if bounds_overlap(entry.bounds(), bounds) {
                result.push(entry.clone());
            }
        }
        Ok(result)
    }
}

#[derive(Debug)]
enum LabelError {
    WorkLimit,
    PlacementExhausted(Box<PlacementFailure>),
    Grid(GridError),
}

impl From<GridError> for LabelError {
    fn from(error: GridError) -> Self {
        Self::Grid(error)
    }
}

#[derive(Debug)]
#[allow(dead_code)] // Upstream's warning details; callers observe the metrics and fallback reason.
struct PlacementFailure {
    edge_id: String,
    label_node_id: String,
}

#[derive(Clone, Debug)]
struct BlockedIntervals {
    low: f64,
    high: f64,
    blocked: Vec<Interval>,
}

struct EdgeLabelContext {
    edge_by_id: HashMap<String, Edge>,
    edge_geometry_by_id: HashMap<String, EdgeGeometryCache>,
    obstacle_index: CompressedBoundsIndex<ObstacleEntry>,
    placed_label_index: CompressedBoundsIndex<ObstacleEntry>,
    group_border_index: CompressedBoundsIndex<BorderEntry>,
    segment_index: CompressedBoundsIndex<EdgeSegmentEntry>,
    search_bounds: Bounds,
    metrics: Rc<RefCell<GridEdgeLabelInstrumentation>>,
}

fn range_overlap_length(
    first_start: f64,
    first_end: f64,
    second_start: f64,
    second_end: f64,
) -> f64 {
    let start = first_start.min(first_end).max(second_start.min(second_end));
    let end = first_start.max(first_end).min(second_start.max(second_end));
    0.0_f64.max(end - start)
}

fn polyline_has_self_conflict(points: &[RouterPoint]) -> bool {
    let segments = normalize_polyline(points).segments;
    for (first_index, first) in segments.iter().enumerate() {
        for second in segments.iter().skip(first_index + 2) {
            if crate::layout::grid::geometry::segments_cross(*first, *second) {
                return true;
            }
            if first.orientation != second.orientation || first.orientation == SegmentOrientation::Z
            {
                continue;
            }

            let same_axis = if first.orientation == SegmentOrientation::H {
                (first.a.y - second.a.y).abs() <= LABEL_EPSILON
            } else {
                (first.a.x - second.a.x).abs() <= LABEL_EPSILON
            };
            let overlap = if first.orientation == SegmentOrientation::H {
                range_overlap_length(first.a.x, first.b.x, second.a.x, second.b.x)
            } else {
                range_overlap_length(first.a.y, first.b.y, second.a.y, second.b.y)
            };
            if same_axis && overlap > LABEL_EPSILON {
                return true;
            }
            let gap = if first.orientation == SegmentOrientation::H {
                (first.a.y - second.a.y).abs()
            } else {
                (first.a.x - second.a.x).abs()
            };
            if !same_axis && gap < MIN_SELF_PARALLEL_GAP && overlap >= MIN_SELF_PARALLEL_OVERLAP {
                return true;
            }
        }
    }
    false
}

fn rect_overlap(a: Rect, b: Rect) -> bool {
    a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top
}

fn label_rect_at(node: &Node, center: RouterPoint) -> Rect {
    let width = node.width.unwrap_or(0.0);
    let height = node.height.unwrap_or(0.0);
    Rect {
        cx: center.x,
        cy: center.y,
        left: center.x - width / 2.0,
        right: center.x + width / 2.0,
        top: center.y - height / 2.0,
        bottom: center.y + height / 2.0,
    }
}

fn group_title_rect(node: &Node) -> Option<Rect> {
    node.group_title_rect.map(|rect| Rect {
        cx: (rect.left + rect.right) / 2.0,
        cy: (rect.top + rect.bottom) / 2.0,
        left: rect.left,
        right: rect.right,
        top: rect.top,
        bottom: rect.bottom,
    })
}

fn clone_point(point: RouterPoint) -> RouterPoint {
    RouterPoint {
        x: point.x,
        y: point.y,
    }
}

fn bounds_for_rect(rect: Rect) -> Bounds {
    Bounds {
        left: rect.left,
        right: rect.right,
        top: rect.top,
        bottom: rect.bottom,
    }
}

fn bounds_for_segment(segment: Segment) -> Bounds {
    Bounds {
        left: segment.a.x.min(segment.b.x),
        right: segment.a.x.max(segment.b.x),
        top: segment.a.y.min(segment.b.y),
        bottom: segment.a.y.max(segment.b.y),
    }
}

fn bounds_for_points(points: &[RouterPoint]) -> Option<Bounds> {
    let first = points.first()?;
    let mut bounds = Bounds {
        left: first.x,
        right: first.x,
        top: first.y,
        bottom: first.y,
    };
    for point in &points[1..] {
        bounds.left = bounds.left.min(point.x);
        bounds.right = bounds.right.max(point.x);
        bounds.top = bounds.top.min(point.y);
        bounds.bottom = bounds.bottom.max(point.y);
    }
    Some(bounds)
}

fn merge_bounds(a: Bounds, b: Bounds) -> Bounds {
    Bounds {
        left: a.left.min(b.left),
        right: a.right.max(b.right),
        top: a.top.min(b.top),
        bottom: a.bottom.max(b.bottom),
    }
}

fn expand_bounds(bounds: Bounds, padding: f64) -> Bounds {
    Bounds {
        left: bounds.left - padding,
        right: bounds.right + padding,
        top: bounds.top - padding,
        bottom: bounds.bottom + padding,
    }
}

fn bounds_overlap(a: Bounds, b: Bounds) -> bool {
    a.left <= b.right && a.right >= b.left && a.top <= b.bottom && a.bottom >= b.top
}

fn candidate_segments_from_segments(segments: &[Segment]) -> Vec<SegmentCandidate> {
    let indexes: Vec<usize> = if segments.len() <= 2 {
        (0..segments.len()).collect()
    } else {
        let mut indexes: Vec<usize> = (1..segments.len() - 1).collect();
        indexes.push(0);
        indexes.push(segments.len() - 1);
        indexes
    };
    indexes
        .into_iter()
        .enumerate()
        .map(|(order, segment_index)| SegmentCandidate {
            segment: segments[segment_index],
            segment_index,
            order,
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Terminal {
    Start,
    End,
}

fn has_terminal_marker(edge: &Edge, terminal: Terminal) -> bool {
    let marker_type = match terminal {
        Terminal::Start => edge.arrow_type_start.as_deref(),
        Terminal::End => edge.arrow_type_end.as_deref(),
    };
    // Explicit terminal settings override edge.type, including values that disable markers.
    if let Some(marker_type) = marker_type {
        let trimmed = marker_type.trim();
        if !trimmed.is_empty() {
            return trimmed != "none" && trimmed != "arrow_open";
        }
    }

    let Some(edge_type) = edge.edge_type.as_deref() else {
        return false;
    };
    // Flowchart/swimlane edges often carry marker semantics in `type`.
    if terminal == Terminal::Start && edge_type.starts_with("double_") {
        return true;
    }
    terminal == Terminal::End
        && (edge_type.contains("arrow_point")
            || edge_type.contains("arrow_cross")
            || edge_type.contains("arrow_circle")
            || edge_type.contains("arrow_barb")
            || edge_type.contains("double_arrow"))
}

fn terminal_marker_clearance_rect(
    points: &[RouterPoint],
    terminal: Terminal,
    length: f64,
    half_width: f64,
    tolerance: f64,
) -> Option<Rect> {
    if points.len() < 2 {
        return None;
    }

    let tip = match terminal {
        Terminal::End => points[points.len() - 1],
        Terminal::Start => points[0],
    };
    let inner = match terminal {
        Terminal::End => points[points.len() - 2],
        Terminal::Start => points[1],
    };
    let dx = inner.x - tip.x;
    let dy = inner.y - tip.y;

    if dx.abs() <= tolerance && dy.abs() <= tolerance {
        return None;
    }
    if dy.abs() <= tolerance {
        let inner_x = tip.x + js_sign(dx) * length;
        let left = tip.x.min(inner_x);
        let right = tip.x.max(inner_x);
        return Some(Rect {
            cx: (left + right) / 2.0,
            cy: tip.y,
            left,
            right,
            top: tip.y - half_width,
            bottom: tip.y + half_width,
        });
    }
    if dx.abs() <= tolerance {
        let inner_y = tip.y + js_sign(dy) * length;
        let top = tip.y.min(inner_y);
        let bottom = tip.y.max(inner_y);
        return Some(Rect {
            cx: tip.x,
            cy: (top + bottom) / 2.0,
            left: tip.x - half_width,
            right: tip.x + half_width,
            top,
            bottom,
        });
    }
    None
}

fn js_sign(value: f64) -> f64 {
    if value > 0.0 {
        1.0
    } else if value < 0.0 {
        -1.0
    } else {
        value
    }
}

fn marker_clearance_rects_from_points(edge: &Edge, points: &[RouterPoint]) -> Vec<Rect> {
    [Terminal::Start, Terminal::End]
        .into_iter()
        .filter(|terminal| has_terminal_marker(edge, *terminal))
        .filter_map(|terminal| {
            terminal_marker_clearance_rect(
                points,
                terminal,
                EDGE_END_MARKER_CLEARANCE,
                7.0,
                LABEL_EPSILON,
            )
        })
        .collect()
}

fn build_edge_geometry(edge: &Edge, points: &[RouterPoint]) -> EdgeGeometryCache {
    let normalized = normalize_polyline(points);
    let marker_rects = marker_clearance_rects_from_points(edge, &normalized.points);
    EdgeGeometryCache {
        points: normalized.points,
        candidate_segments: candidate_segments_from_segments(&normalized.segments),
        marker_rects,
        segments: normalized.segments,
    }
}

fn segment_length(segment: Segment) -> f64 {
    if segment.orientation == SegmentOrientation::H {
        (segment.b.x - segment.a.x).abs()
    } else {
        (segment.b.y - segment.a.y).abs()
    }
}

fn border_cuts_rect(rect: Rect, border: Rect) -> bool {
    let corners = [
        RouterPoint::new(border.left, border.top),
        RouterPoint::new(border.right, border.top),
        RouterPoint::new(border.right, border.bottom),
        RouterPoint::new(border.left, border.bottom),
    ];
    for index in 0..corners.len() {
        if segment_intersects_rect_interior(
            corners[index],
            corners[(index + 1) % corners.len()],
            rect,
        ) {
            return true;
        }
    }
    false
}

fn has_finite_position(node: &Node) -> bool {
    matches!(node.x, Some(x) if x.is_finite()) && matches!(node.y, Some(y) if y.is_finite())
}

fn points_for_edge(
    data: &LayoutData,
    edge_index: usize,
    overrides: Option<&EdgePointOverrides>,
    context: &EdgeLabelContext,
) -> Vec<RouterPoint> {
    let edge = &data.edges[edge_index];
    if let Some(override_points) = overrides.and_then(|overrides| overrides.get(&edge.id)) {
        return normalize_polyline(override_points).points;
    }
    if let Some(cached) = context.edge_geometry_by_id.get(&edge.id) {
        return cached.points.clone();
    }
    normalize_polyline(&edge.points).points
}

fn marker_clearance_rects_for_edge(
    data: &LayoutData,
    edge_index: usize,
    overrides: Option<&EdgePointOverrides>,
    context: &EdgeLabelContext,
) -> Vec<Rect> {
    let edge = &data.edges[edge_index];
    if !overrides.is_some_and(|overrides| overrides.has(&edge.id))
        && let Some(cached) = context.edge_geometry_by_id.get(&edge.id)
    {
        return cached.marker_rects.clone();
    }
    marker_clearance_rects_from_points(edge, &points_for_edge(data, edge_index, overrides, context))
}

fn range_overlaps(a_start: f64, a_end: f64, b_start: f64, b_end: f64) -> bool {
    a_start.min(a_end).max(b_start.min(b_end))
        < a_start.max(a_end).min(b_start.max(b_end)) - LABEL_EPSILON
}

fn clamp_interval(interval: Interval, low: f64, high: f64) -> Option<Interval> {
    let start = clamp(interval.start.min(interval.end), low, high);
    let end = clamp(interval.start.max(interval.end), low, high);
    (end - start > LABEL_EPSILON).then_some(Interval { start, end })
}

fn merge_blocked_label_intervals(intervals: &[Interval], low: f64, high: f64) -> Vec<Interval> {
    let mut clamped: Vec<Interval> = intervals
        .iter()
        .filter_map(|interval| clamp_interval(*interval, low, high))
        .collect();
    clamped.sort_by(|a, b| {
        a.start
            .partial_cmp(&b.start)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.end.partial_cmp(&b.end).unwrap_or(Ordering::Equal))
    });

    if clamped.is_empty() {
        return Vec::new();
    }

    let mut merged = vec![clamped[0]];
    for current in &clamped[1..] {
        let previous = merged.last_mut().expect("merged has one entry");
        if current.start <= previous.end + LABEL_EPSILON {
            previous.end = previous.end.max(current.end);
        } else {
            merged.push(*current);
        }
    }
    merged
}

fn js_round(value: f64) -> f64 {
    (value + 0.5).floor()
}

fn center_candidates_from_intervals(
    low: f64,
    high: f64,
    blocked: &[Interval],
    preferred: f64,
) -> Vec<f64> {
    if high - low <= LABEL_EPSILON {
        return Vec::new();
    }

    let mut allowed: Vec<Interval> = Vec::new();
    let mut cursor = low;
    for interval in merge_blocked_label_intervals(blocked, low, high) {
        if interval.start - cursor > LABEL_EPSILON {
            allowed.push(Interval {
                start: cursor,
                end: interval.start,
            });
        }
        cursor = cursor.max(interval.end);
    }
    if high - cursor > LABEL_EPSILON {
        allowed.push(Interval {
            start: cursor,
            end: high,
        });
    }

    let mut candidates: Vec<(f64, f64, f64)> = allowed
        .iter()
        .map(|interval| {
            (
                clamp(preferred, interval.start, interval.end),
                interval.start,
                interval.end,
            )
        })
        .collect();
    candidates.sort_by(|a, b| {
        let preferred_delta = (a.0 - preferred).abs() - (b.0 - preferred).abs();
        if preferred_delta.abs() > LABEL_EPSILON {
            return preferred_delta.partial_cmp(&0.0).unwrap_or(Ordering::Equal);
        }
        let width_delta = (b.2 - b.1) - (a.2 - a.1);
        if width_delta.abs() > LABEL_EPSILON {
            return width_delta.partial_cmp(&0.0).unwrap_or(Ordering::Equal);
        }
        a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal)
    });

    let mut seen: HashSet<i64> = HashSet::new();
    let mut values = Vec::new();
    for candidate in candidates {
        let rounded = js_round(candidate.0 * 1e6) / 1e6;
        let key = (if rounded == 0.0 { 0.0 } else { rounded }).to_bits() as i64;
        if seen.contains(&key) {
            continue;
        }
        seen.insert(key);
        values.push(candidate.0);
    }
    values
}

fn obstacle_rect_for_node(node: &Node) -> Option<Rect> {
    if node.is_group {
        return None;
    }
    if is_edge_label_node(node) && !has_finite_position(node) {
        return None;
    }
    Some(rect_for_node(node))
}

fn candidate_segments(
    data: &LayoutData,
    edge_index: usize,
    context: &EdgeLabelContext,
) -> Vec<SegmentCandidate> {
    let edge = &data.edges[edge_index];
    if let Some(cached) = context.edge_geometry_by_id.get(&edge.id) {
        return cached.candidate_segments.clone();
    }
    candidate_segments_from_segments(&normalize_polyline(&edge.points).segments)
}

fn store_edge_geometry(context: &mut EdgeLabelContext, edge_id: &str, geometry: EdgeGeometryCache) {
    if let Some(previous) = context.edge_geometry_by_id.get(edge_id) {
        for (index, _) in previous.segments.iter().enumerate() {
            context.segment_index.remove(&format!("{edge_id}:{index}"));
        }
    }

    for (index, segment) in geometry.segments.iter().enumerate() {
        context.segment_index.upsert(EdgeSegmentEntry {
            id: format!("{edge_id}:{index}"),
            edge_id: edge_id.to_string(),
            segment: *segment,
            bounds: bounds_for_segment(*segment),
        });
    }
    context
        .edge_geometry_by_id
        .insert(edge_id.to_string(), geometry);
}

fn set_edge_points(
    data: &mut LayoutData,
    context: &mut EdgeLabelContext,
    edge_index: usize,
    points: &[RouterPoint],
) {
    let edge = &data.edges[edge_index];
    let geometry = build_edge_geometry(edge, points);
    data.edges[edge_index].points = geometry.points.iter().copied().map(clone_point).collect();
    let edge_id = data.edges[edge_index].id.clone();
    store_edge_geometry(context, &edge_id, geometry);
}

fn upsert_placed_label_obstacle(context: &mut EdgeLabelContext, placed_label: &PlacedLabel) {
    context.placed_label_index.upsert(ObstacleEntry {
        id: format!("label:{}", placed_label.node_id),
        node_id: Some(placed_label.node_id.clone()),
        rect: placed_label.rect,
        bounds: bounds_for_rect(placed_label.rect),
    });
}

fn create_edge_label_context(
    data: &LayoutData,
    work_budget: Rc<RefCell<LabelIndexWorkBudget>>,
    metrics: Rc<RefCell<GridEdgeLabelInstrumentation>>,
) -> Result<EdgeLabelContext, LabelError> {
    // A fresh context is the isolation boundary for one placement pass.
    let mut context = EdgeLabelContext {
        edge_by_id: HashMap::new(),
        edge_geometry_by_id: HashMap::new(),
        obstacle_index: CompressedBoundsIndex::new(work_budget.clone(), metrics.clone()),
        placed_label_index: CompressedBoundsIndex::new(work_budget.clone(), metrics.clone()),
        group_border_index: CompressedBoundsIndex::new(work_budget.clone(), metrics.clone()),
        segment_index: CompressedBoundsIndex::new(work_budget.clone(), metrics.clone()),
        search_bounds: Bounds::default(),
        metrics: metrics.clone(),
    };

    let mut aggregate_bounds: Option<Bounds> = None;
    let mut remember_bounds = |bounds: Bounds| {
        aggregate_bounds = Some(match aggregate_bounds {
            Some(existing) => merge_bounds(existing, bounds),
            None => bounds,
        });
    };

    for node in &data.nodes {
        if node.is_group {
            let border_rect = rect_for_node(node);
            let border_bounds = bounds_for_rect(border_rect);
            context.group_border_index.upsert(BorderEntry {
                id: format!("border:{}", node.id),
                rect: border_rect,
                bounds: border_bounds,
            });
            remember_bounds(border_bounds);

            if let Some(title_rect) = group_title_rect(node) {
                let title_bounds = bounds_for_rect(title_rect);
                context.obstacle_index.upsert(ObstacleEntry {
                    id: format!("title:{}", node.id),
                    rect: title_rect,
                    bounds: title_bounds,
                    node_id: None,
                });
                remember_bounds(title_bounds);
            }
            continue;
        }

        let Some(obstacle_rect) = obstacle_rect_for_node(node) else {
            continue;
        };

        let entry = ObstacleEntry {
            id: format!(
                "{}:{}",
                if is_edge_label_node(node) {
                    "label"
                } else {
                    "node"
                },
                node.id
            ),
            rect: obstacle_rect,
            bounds: bounds_for_rect(obstacle_rect),
            node_id: Some(node.id.clone()),
        };
        if is_edge_label_node(node) {
            context.placed_label_index.upsert(entry.clone());
        } else {
            context.obstacle_index.upsert(entry.clone());
        }
        remember_bounds(entry.bounds);
    }

    for edge in &data.edges {
        context.edge_by_id.insert(edge.id.clone(), edge.clone());
        let geometry = build_edge_geometry(edge, &edge.points);
        store_edge_geometry(&mut context, &edge.id, geometry);
        let geometry = context
            .edge_geometry_by_id
            .get(&edge.id)
            .expect("geometry was stored");
        if let Some(point_bounds) = bounds_for_points(&geometry.points) {
            remember_bounds(point_bounds);
        }
    }

    context.search_bounds = expand_bounds(aggregate_bounds.unwrap_or_default(), 128.0);
    Ok(context)
}

fn query_obstacle_entries(
    context: &mut EdgeLabelContext,
    bounds: Bounds,
    metric_key: LabelMetric,
    exclude_node_id: Option<&str>,
    skip_node_ids: Option<&HashSet<String>>,
) -> Result<Vec<ObstacleEntry>, LabelError> {
    increment_metric(&context.metrics, metric_key, 1);

    let mut collected = Vec::new();
    for entry in context.obstacle_index.query(bounds)? {
        if let Some(node_id) = entry.node_id.as_deref()
            && (Some(node_id) == exclude_node_id
                || skip_node_ids.is_some_and(|skip| skip.contains(node_id)))
        {
            continue;
        }
        collected.push(entry);
    }
    for entry in context.placed_label_index.query(bounds)? {
        if let Some(node_id) = entry.node_id.as_deref()
            && (Some(node_id) == exclude_node_id
                || skip_node_ids.is_some_and(|skip| skip.contains(node_id)))
        {
            continue;
        }
        collected.push(entry);
    }
    increment_metric(
        &context.metrics,
        LabelMetric::ObstacleCandidatesVisited,
        collected.len(),
    );
    Ok(collected)
}

fn query_foreign_segments(
    context: &mut EdgeLabelContext,
    bounds: Bounds,
    self_edge_id: &str,
    overrides: Option<&EdgePointOverrides>,
    metric_key: LabelMetric,
) -> Result<Vec<EdgeSegmentEntry>, LabelError> {
    increment_metric(&context.metrics, metric_key, 1);

    let overridden_edge_ids: Option<HashSet<String>> =
        overrides.map(|overrides| overrides.keys().into_iter().collect());
    let mut collected = Vec::new();
    let indexed_entries = context.segment_index.query(bounds)?;
    for entry in &indexed_entries {
        if entry.edge_id == self_edge_id
            || overridden_edge_ids
                .as_ref()
                .is_some_and(|overridden| overridden.contains(&entry.edge_id))
        {
            continue;
        }
        collected.push(entry.clone());
    }

    let mut visited = indexed_entries.len();
    if let Some(overrides) = overrides {
        for (edge_id, points) in &overrides.entries {
            if edge_id == self_edge_id {
                continue;
            }
            let Some(edge) = context.edge_by_id.get(edge_id) else {
                continue;
            };
            let geometry = build_edge_geometry(edge, points);
            for (index, segment) in geometry.segments.iter().enumerate() {
                let segment_bounds = bounds_for_segment(*segment);
                if !bounds_overlap(segment_bounds, bounds) {
                    continue;
                }
                visited += 1;
                collected.push(EdgeSegmentEntry {
                    id: format!("override:{edge_id}:{index}"),
                    edge_id: edge_id.clone(),
                    segment: *segment,
                    bounds: segment_bounds,
                });
            }
        }
    }

    increment_metric(
        &context.metrics,
        LabelMetric::SegmentCandidatesVisited,
        visited,
    );
    Ok(collected)
}

fn foreign_edge_ids_intersecting_rect(
    context: &mut EdgeLabelContext,
    rect: Rect,
    self_edge_id: &str,
    overrides: Option<&EdgePointOverrides>,
) -> Result<Vec<String>, LabelError> {
    increment_metric(&context.metrics, LabelMetric::ForeignEdgeLookups, 1);
    let mut ids: HashSet<String> = HashSet::new();
    for entry in query_foreign_segments(
        context,
        bounds_for_rect(rect),
        self_edge_id,
        overrides,
        LabelMetric::SegmentRectQueries,
    )? {
        if segment_intersects_rect_interior(entry.segment.a, entry.segment.b, rect) {
            ids.insert(entry.edge_id);
        }
    }
    let mut ids: Vec<String> = ids.into_iter().collect();
    ids.sort_by(|a, b| compare_code_units(a, b));
    Ok(ids)
}

fn obstacle_band_query_bounds(segment: Segment, label_node: &Node) -> Option<Bounds> {
    if !is_finite_positive_number(label_node.width) || !is_finite_positive_number(label_node.height)
    {
        return None;
    }
    let width = label_node.width.expect("measured width");
    let height = label_node.height.expect("measured height");

    if segment.orientation == SegmentOrientation::H {
        return Some(Bounds {
            left: segment.a.x.min(segment.b.x) - width / 2.0,
            right: segment.a.x.max(segment.b.x) + width / 2.0,
            top: segment.a.y - height / 2.0,
            bottom: segment.a.y + height / 2.0,
        });
    }

    Some(Bounds {
        left: segment.a.x - width / 2.0,
        right: segment.a.x + width / 2.0,
        top: segment.a.y.min(segment.b.y) - height / 2.0,
        bottom: segment.a.y.max(segment.b.y) + height / 2.0,
    })
}

fn detour_line_query_bounds(segment: Segment, label_node: &Node, search_bounds: Bounds) -> Bounds {
    if segment.orientation == SegmentOrientation::H {
        let along_padding = label_node.width.unwrap_or(0.0) + LABEL_CLEARANCE * 2.0 + LABEL_GUTTER;
        return Bounds {
            left: segment.a.x.min(segment.b.x) - along_padding,
            right: segment.a.x.max(segment.b.x) + along_padding,
            top: search_bounds.top,
            bottom: search_bounds.bottom,
        };
    }

    let along_padding = label_node.height.unwrap_or(0.0) + LABEL_CLEARANCE * 2.0 + LABEL_GUTTER;
    Bounds {
        left: search_bounds.left,
        right: search_bounds.right,
        top: segment.a.y.min(segment.b.y) - along_padding,
        bottom: segment.a.y.max(segment.b.y) + along_padding,
    }
}

fn reroute_line_query_bounds(segment: Segment, search_bounds: Bounds) -> Bounds {
    if segment.orientation == SegmentOrientation::H {
        return Bounds {
            left: segment.a.x.min(segment.b.x) - LABEL_CLEARANCE,
            right: segment.a.x.max(segment.b.x) + LABEL_CLEARANCE,
            top: search_bounds.top,
            bottom: search_bounds.bottom,
        };
    }

    Bounds {
        left: search_bounds.left,
        right: search_bounds.right,
        top: segment.a.y.min(segment.b.y) - LABEL_CLEARANCE,
        bottom: segment.a.y.max(segment.b.y) + LABEL_CLEARANCE,
    }
}

fn rect_safe_for_label(
    data: &LayoutData,
    rect: Rect,
    edge_index: usize,
    label_node_id: &str,
    context: &mut EdgeLabelContext,
    overrides: Option<&EdgePointOverrides>,
    allowed_foreign_edge_ids: Option<&HashSet<String>>,
) -> Result<bool, LabelError> {
    for obstacle in query_obstacle_entries(
        context,
        bounds_for_rect(rect),
        LabelMetric::ObstacleRectQueries,
        Some(label_node_id),
        None,
    )? {
        if rect_overlap(rect, obstacle.rect) {
            return Ok(false);
        }
    }

    for border in context.group_border_index.query(bounds_for_rect(rect))? {
        if border_cuts_rect(rect, border.rect) {
            return Ok(false);
        }
    }

    for clearance_rect in marker_clearance_rects_for_edge(data, edge_index, overrides, context) {
        if rect_overlap(rect, clearance_rect) {
            return Ok(false);
        }
    }

    let edge_id = data.edges[edge_index].id.clone();
    for foreign_segment in query_foreign_segments(
        context,
        bounds_for_rect(rect),
        &edge_id,
        overrides,
        LabelMetric::SegmentRectQueries,
    )? {
        if allowed_foreign_edge_ids
            .is_some_and(|allowed| allowed.contains(&foreign_segment.edge_id))
        {
            continue;
        }
        if segment_intersects_rect_interior(
            foreign_segment.segment.a,
            foreign_segment.segment.b,
            rect,
        ) {
            return Ok(false);
        }
    }

    Ok(true)
}

fn required_segment_length(node: &Node, segment: Segment) -> f64 {
    if segment.orientation == SegmentOrientation::Z {
        return 0.0;
    }
    grid_label_required_segment_length(
        node,
        if segment.orientation == SegmentOrientation::H {
            crate::layout::grid::types::GridOrientation::H
        } else {
            crate::layout::grid::types::GridOrientation::V
        },
    )
}

fn blocked_center_intervals(
    data: &LayoutData,
    edge_index: usize,
    label_node: &Node,
    segment: Segment,
    context: &mut EdgeLabelContext,
    overrides: Option<&EdgePointOverrides>,
    include_foreign_edges: bool,
) -> Result<Option<BlockedIntervals>, LabelError> {
    if !is_finite_positive_number(label_node.width) || !is_finite_positive_number(label_node.height)
    {
        return Ok(None);
    }
    let label_width = label_node.width.expect("measured width");
    let label_height = label_node.height.expect("measured height");

    let along_half = if segment.orientation == SegmentOrientation::H {
        label_width / 2.0 + LABEL_CLEARANCE
    } else {
        label_height / 2.0 + LABEL_CLEARANCE
    };
    let low = if segment.orientation == SegmentOrientation::H {
        segment.a.x.min(segment.b.x) + along_half
    } else {
        segment.a.y.min(segment.b.y) + along_half
    };
    let high = if segment.orientation == SegmentOrientation::H {
        segment.a.x.max(segment.b.x) - along_half
    } else {
        segment.a.y.max(segment.b.y) - along_half
    };
    if high - low <= LABEL_EPSILON {
        return Ok(None);
    }

    // Project every obstacle onto the candidate segment; the gaps are the only centers worth
    // testing.
    let mut blocked: Vec<Interval> = Vec::new();
    let Some(band_bounds) = obstacle_band_query_bounds(segment, label_node) else {
        return Ok(None);
    };
    let mut add_rect = |rect: Rect| {
        if segment.orientation == SegmentOrientation::H {
            let band_top = segment.a.y - label_height / 2.0;
            let band_bottom = segment.a.y + label_height / 2.0;
            if range_overlaps(band_top, band_bottom, rect.top, rect.bottom) {
                blocked.push(Interval {
                    start: rect.left - label_width / 2.0,
                    end: rect.right + label_width / 2.0,
                });
            }
            return;
        }

        let band_left = segment.a.x - label_width / 2.0;
        let band_right = segment.a.x + label_width / 2.0;
        if range_overlaps(band_left, band_right, rect.left, rect.right) {
            blocked.push(Interval {
                start: rect.top - label_height / 2.0,
                end: rect.bottom + label_height / 2.0,
            });
        }
    };

    for obstacle in query_obstacle_entries(
        context,
        band_bounds,
        LabelMetric::ObstacleBandQueries,
        Some(&label_node.id),
        None,
    )? {
        add_rect(obstacle.rect);
    }
    for clearance_rect in marker_clearance_rects_for_edge(data, edge_index, overrides, context) {
        add_rect(clearance_rect);
    }

    if include_foreign_edges {
        let edge_id = data.edges[edge_index].id.clone();
        for foreign_segment in query_foreign_segments(
            context,
            band_bounds,
            &edge_id,
            overrides,
            LabelMetric::SegmentBandQueries,
        )? {
            let segment_entry = foreign_segment.segment;
            if segment.orientation == SegmentOrientation::H {
                let band_top = segment.a.y - label_node.height.unwrap_or(0.0) / 2.0;
                let band_bottom = segment.a.y + label_node.height.unwrap_or(0.0) / 2.0;
                if segment_entry.orientation == SegmentOrientation::H {
                    if band_top < segment_entry.a.y && band_bottom > segment_entry.a.y {
                        blocked.push(Interval {
                            start: segment_entry.a.x.min(segment_entry.b.x)
                                - label_node.width.unwrap_or(0.0) / 2.0,
                            end: segment_entry.a.x.max(segment_entry.b.x)
                                + label_node.width.unwrap_or(0.0) / 2.0,
                        });
                    }
                } else if range_overlaps(
                    band_top,
                    band_bottom,
                    segment_entry.a.y.min(segment_entry.b.y),
                    segment_entry.a.y.max(segment_entry.b.y),
                ) {
                    blocked.push(Interval {
                        start: segment_entry.a.x - label_node.width.unwrap_or(0.0) / 2.0,
                        end: segment_entry.a.x + label_node.width.unwrap_or(0.0) / 2.0,
                    });
                }
                continue;
            }

            let band_left = segment.a.x - label_node.width.unwrap_or(0.0) / 2.0;
            let band_right = segment.a.x + label_node.width.unwrap_or(0.0) / 2.0;
            if segment_entry.orientation == SegmentOrientation::V {
                if band_left < segment_entry.a.x && band_right > segment_entry.a.x {
                    blocked.push(Interval {
                        start: segment_entry.a.y.min(segment_entry.b.y)
                            - label_node.height.unwrap_or(0.0) / 2.0,
                        end: segment_entry.a.y.max(segment_entry.b.y)
                            + label_node.height.unwrap_or(0.0) / 2.0,
                    });
                }
            } else if range_overlaps(
                band_left,
                band_right,
                segment_entry.a.x.min(segment_entry.b.x),
                segment_entry.a.x.max(segment_entry.b.x),
            ) {
                blocked.push(Interval {
                    start: segment_entry.a.y - label_node.height.unwrap_or(0.0) / 2.0,
                    end: segment_entry.a.y + label_node.height.unwrap_or(0.0) / 2.0,
                });
            }
        }
    }

    Ok(Some(BlockedIntervals { low, high, blocked }))
}

fn placement_on_existing_segment(
    data: &LayoutData,
    edge_index: usize,
    label_node: &Node,
    segment: Segment,
    context: &mut EdgeLabelContext,
) -> Result<Option<RouterPoint>, LabelError> {
    if segment_length(segment) < required_segment_length(label_node, segment) {
        return Ok(None);
    }

    let Some(interval_data) =
        blocked_center_intervals(data, edge_index, label_node, segment, context, None, true)?
    else {
        return Ok(None);
    };

    let preferred = if segment.orientation == SegmentOrientation::H {
        (segment.a.x.min(segment.b.x) + segment.a.x.max(segment.b.x)) / 2.0
    } else {
        (segment.a.y.min(segment.b.y) + segment.a.y.max(segment.b.y)) / 2.0
    };

    let blocked = merge_blocked_label_intervals(
        &interval_data.blocked,
        interval_data.low,
        interval_data.high,
    );
    let candidates = center_candidates_from_intervals(
        interval_data.low,
        interval_data.high,
        &blocked,
        preferred,
    );
    for candidate in candidates {
        let center = if segment.orientation == SegmentOrientation::H {
            RouterPoint::new(candidate, segment.a.y)
        } else {
            RouterPoint::new(segment.a.x, candidate)
        };
        let rect = label_rect_at(label_node, center);
        if rect_safe_for_label(data, rect, edge_index, &label_node.id, context, None, None)? {
            return Ok(Some(center));
        }
    }

    Ok(None)
}

fn placement_on_existing_segment_with_reroutes(
    data: &mut LayoutData,
    edge_index: usize,
    label_node: &Node,
    segment: Segment,
    context: &mut EdgeLabelContext,
    placed_labels_by_edge_id: &PlacedLabels,
) -> Result<Option<RouterPoint>, LabelError> {
    let Some(interval_data) =
        blocked_center_intervals(data, edge_index, label_node, segment, context, None, false)?
    else {
        return Ok(None);
    };

    let preferred = if segment.orientation == SegmentOrientation::H {
        (segment.a.x.min(segment.b.x) + segment.a.x.max(segment.b.x)) / 2.0
    } else {
        (segment.a.y.min(segment.b.y) + segment.a.y.max(segment.b.y)) / 2.0
    };
    let blocked = merge_blocked_label_intervals(
        &interval_data.blocked,
        interval_data.low,
        interval_data.high,
    );
    let candidates = center_candidates_from_intervals(
        interval_data.low,
        interval_data.high,
        &blocked,
        preferred,
    );

    let own_edge_id = data.edges[edge_index].id.clone();
    for candidate in candidates {
        let center = if segment.orientation == SegmentOrientation::H {
            RouterPoint::new(candidate, segment.a.y)
        } else {
            RouterPoint::new(segment.a.x, candidate)
        };
        let rect = label_rect_at(label_node, center);
        let mut overrides = EdgePointOverrides::default();
        let foreign_edge_ids =
            foreign_edge_ids_intersecting_rect(context, rect, &own_edge_id, None)?;
        let foreign_edge_indexes: Vec<usize> = foreign_edge_ids
            .iter()
            .filter_map(|id| data.edges.iter().position(|edge| &edge.id == id))
            .collect();

        let mut ok = true;
        for foreign_edge_index in foreign_edge_indexes {
            let rerouted = reroute_foreign_edge_around_rect(
                data,
                foreign_edge_index,
                rect,
                context,
                placed_labels_by_edge_id,
                &mut overrides,
                None,
            )?;
            let Some(rerouted) = rerouted else {
                ok = false;
                break;
            };
            let foreign_id = data.edges[foreign_edge_index].id.clone();
            overrides.set(&foreign_id, rerouted);
        }

        if !ok {
            continue;
        }
        if !rect_safe_for_label(
            data,
            rect,
            edge_index,
            &label_node.id,
            context,
            Some(&overrides),
            None,
        )? {
            continue;
        }

        for (edge_id, points) in overrides.entries.clone() {
            if let Some(index) = data.edges.iter().position(|edge| edge.id == edge_id) {
                set_edge_points(data, context, index, &points);
            }
        }
        return Ok(Some(center));
    }

    Ok(None)
}

fn polyline_hits_protected_obstacles(
    data: &LayoutData,
    points: &[RouterPoint],
    context: &mut EdgeLabelContext,
    skip_node_ids: &HashSet<String>,
) -> Result<bool, LabelError> {
    for segment in normalize_polyline(points).segments {
        for obstacle in query_obstacle_entries(
            context,
            bounds_for_segment(segment),
            LabelMetric::ObstaclePolylineQueries,
            None,
            Some(skip_node_ids),
        )? {
            if segment_intersects_rect_interior(segment.a, segment.b, obstacle.rect) {
                return Ok(true);
            }
        }
    }
    let _ = data;
    Ok(false)
}

fn label_still_anchored(
    edge_id: &str,
    points: &[RouterPoint],
    placed_labels_by_edge_id: &PlacedLabels,
) -> bool {
    let Some(placed) = placed_labels_by_edge_id.get(edge_id) else {
        return true;
    };
    orthogonal_polyline_contains_point(points, placed.center, LABEL_EPSILON)
}

fn orthogonal_polyline_contains_point(
    points: &[RouterPoint],
    point: RouterPoint,
    tolerance: f64,
) -> bool {
    for index in 1..points.len() {
        let a = points[index - 1];
        let b = points[index];
        if approx_equal(a.y, b.y, tolerance) && !approx_equal(a.x, b.x, tolerance) {
            if approx_equal(point.y, a.y, tolerance)
                && point.x >= a.x.min(b.x) - tolerance
                && point.x <= a.x.max(b.x) + tolerance
            {
                return true;
            }
        } else if approx_equal(a.x, b.x, tolerance)
            && !approx_equal(a.y, b.y, tolerance)
            && approx_equal(point.x, a.x, tolerance)
            && point.y >= a.y.min(b.y) - tolerance
            && point.y <= a.y.max(b.y) + tolerance
        {
            return true;
        }
    }
    false
}

fn approx_equal(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance
}

fn detour_offsets(cross_half_size: f64) -> Vec<f64> {
    let mut out = Vec::new();
    for step in 0..MAX_GUTTER_STEPS {
        let offset = cross_half_size + LABEL_GUTTER + step as f64 * LABEL_GUTTER_STEP;
        out.push(-offset);
        out.push(offset);
    }
    out
}

fn unique_sorted_candidates(values: &[f64], anchor: f64) -> Vec<f64> {
    let mut rounded: Vec<(i64, f64)> = Vec::new();
    for value in values {
        let rounded_value = js_round(value * 1e6) / 1e6;
        let key = (if rounded_value == 0.0 {
            0.0
        } else {
            rounded_value
        })
        .to_bits() as i64;
        if !rounded.iter().any(|(existing, _)| *existing == key) {
            rounded.push((key, *value));
        }
    }
    let mut values: Vec<f64> = rounded.into_iter().map(|(_, value)| value).collect();
    values.sort_by(|a, b| {
        let distance_delta = (a - anchor).abs() - (b - anchor).abs();
        if distance_delta.abs() > LABEL_EPSILON {
            return distance_delta.partial_cmp(&0.0).unwrap_or(Ordering::Equal);
        }
        a.partial_cmp(b).unwrap_or(Ordering::Equal)
    });
    values
}

fn detour_line_candidates(
    data: &LayoutData,
    edge_index: usize,
    label_node: &Node,
    segment: Segment,
    context: &mut EdgeLabelContext,
) -> Result<Vec<f64>, LabelError> {
    if !is_finite_positive_number(label_node.width) || !is_finite_positive_number(label_node.height)
    {
        return Ok(Vec::new());
    }

    let label_height = label_node.height.expect("measured height");
    let label_width = label_node.width.expect("measured width");
    let anchor = if segment.orientation == SegmentOrientation::H {
        segment.a.y
    } else {
        segment.a.x
    };
    let cross_half_size = if segment.orientation == SegmentOrientation::H {
        label_height / 2.0
    } else {
        label_width / 2.0
    };
    let mut candidates: Vec<f64> = detour_offsets(cross_half_size)
        .into_iter()
        .map(|offset| anchor + offset)
        .collect();
    let mut add_rect = |rect: Rect| {
        if segment.orientation == SegmentOrientation::H {
            candidates.push(rect.top - cross_half_size - LABEL_GUTTER);
            candidates.push(rect.bottom + cross_half_size + LABEL_GUTTER);
            return;
        }
        candidates.push(rect.left - cross_half_size - LABEL_GUTTER);
        candidates.push(rect.right + cross_half_size + LABEL_GUTTER);
    };

    let query_bounds = detour_line_query_bounds(segment, label_node, context.search_bounds);
    for obstacle in query_obstacle_entries(
        context,
        query_bounds,
        LabelMetric::ObstacleBandQueries,
        Some(&label_node.id),
        None,
    )? {
        add_rect(obstacle.rect);
    }
    for clearance_rect in marker_clearance_rects_for_edge(data, edge_index, None, context) {
        add_rect(clearance_rect);
    }

    Ok(unique_sorted_candidates(&candidates, anchor)
        .into_iter()
        .take(MAX_DETOUR_LINE_CANDIDATES)
        .collect())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RerouteLane {
    key: String,
    orientation: SegmentOrientation,
    side: LaneSide,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LaneSide {
    Top,
    Bottom,
    Left,
    Right,
}

fn reroute_line_candidates(
    segment: Segment,
    blocked_rect: Rect,
    context: &mut EdgeLabelContext,
    skip_node_ids: &HashSet<String>,
    lane: Option<&RerouteLane>,
) -> Result<Vec<f64>, LabelError> {
    let anchor = if segment.orientation == SegmentOrientation::H {
        segment.a.y
    } else {
        segment.a.x
    };
    let mut candidates = Vec::new();
    for offset in detour_offsets(0.0) {
        if segment.orientation == SegmentOrientation::H {
            candidates.push(if offset < 0.0 {
                blocked_rect.top + offset
            } else {
                blocked_rect.bottom + offset
            });
        } else {
            candidates.push(if offset < 0.0 {
                blocked_rect.left + offset
            } else {
                blocked_rect.right + offset
            });
        }
    }

    let mut add_rect = |rect: Rect| {
        if segment.orientation == SegmentOrientation::H {
            candidates.push(rect.top - LABEL_GUTTER);
            candidates.push(rect.bottom + LABEL_GUTTER);
            return;
        }
        candidates.push(rect.left - LABEL_GUTTER);
        candidates.push(rect.right + LABEL_GUTTER);
    };

    let query_bounds = reroute_line_query_bounds(segment, context.search_bounds);
    for obstacle in query_obstacle_entries(
        context,
        query_bounds,
        LabelMetric::ObstacleBandQueries,
        None,
        Some(skip_node_ids),
    )? {
        add_rect(obstacle.rect);
    }

    let sorted = unique_sorted_candidates(&candidates, anchor);
    let side_candidates: Vec<f64> = match lane {
        Some(lane) if lane.orientation == segment.orientation => sorted
            .into_iter()
            .filter(|candidate| match lane.side {
                LaneSide::Top => *candidate < blocked_rect.top,
                LaneSide::Bottom => *candidate > blocked_rect.bottom,
                LaneSide::Left => *candidate < blocked_rect.left,
                LaneSide::Right => *candidate > blocked_rect.right,
            })
            .collect(),
        _ => sorted,
    };
    Ok(side_candidates
        .into_iter()
        .take(MAX_DETOUR_LINE_CANDIDATES)
        .collect())
}

fn detour_center_candidates(
    data: &LayoutData,
    edge_index: usize,
    label_node: &Node,
    segment: Segment,
    detour_line_coord: f64,
    context: &mut EdgeLabelContext,
) -> Result<Vec<f64>, LabelError> {
    if !is_finite_positive_number(label_node.width) || !is_finite_positive_number(label_node.height)
    {
        return Ok(Vec::new());
    }

    let label_width = label_node.width.expect("measured width");
    let label_height = label_node.height.expect("measured height");
    let detour_segment = if segment.orientation == SegmentOrientation::H {
        Segment {
            orientation: SegmentOrientation::H,
            a: RouterPoint::new(segment.a.x, detour_line_coord),
            b: RouterPoint::new(segment.b.x, detour_line_coord),
        }
    } else {
        Segment {
            orientation: SegmentOrientation::V,
            a: RouterPoint::new(detour_line_coord, segment.a.y),
            b: RouterPoint::new(detour_line_coord, segment.b.y),
        }
    };

    let preferred = if segment.orientation == SegmentOrientation::H {
        (segment.a.x.min(segment.b.x) + segment.a.x.max(segment.b.x)) / 2.0
    } else {
        (segment.a.y.min(segment.b.y) + segment.a.y.max(segment.b.y)) / 2.0
    };

    let Some(interval_data) = blocked_center_intervals(
        data,
        edge_index,
        label_node,
        detour_segment,
        context,
        None,
        false,
    )?
    else {
        let mut candidates = vec![preferred];
        let query_bounds = obstacle_band_query_bounds(detour_segment, label_node);
        let mut add_rect = |rect: Rect| {
            if segment.orientation == SegmentOrientation::H {
                candidates.push(rect.left - LABEL_GUTTER - label_width / 2.0);
                candidates.push(rect.right + LABEL_GUTTER + label_width / 2.0);
                return;
            }
            candidates.push(rect.top - LABEL_GUTTER - label_height / 2.0);
            candidates.push(rect.bottom + LABEL_GUTTER + label_height / 2.0);
        };
        if let Some(query_bounds) = query_bounds {
            for obstacle in query_obstacle_entries(
                context,
                query_bounds,
                LabelMetric::ObstacleBandQueries,
                Some(&label_node.id),
                None,
            )? {
                add_rect(obstacle.rect);
            }
        }
        return Ok(unique_sorted_candidates(&candidates, preferred)
            .into_iter()
            .take(MAX_DETOUR_CENTER_CANDIDATES)
            .collect());
    };

    let blocked = merge_blocked_label_intervals(
        &interval_data.blocked,
        interval_data.low,
        interval_data.high,
    );
    Ok(center_candidates_from_intervals(
        interval_data.low,
        interval_data.high,
        &blocked,
        preferred,
    ))
}

fn detour_return_coordinate(source_line: f64, detour_line: f64, label_half_size: f64) -> f64 {
    let direction = if detour_line >= source_line {
        1.0
    } else {
        -1.0
    };
    detour_line + direction * (label_half_size + LABEL_CLEARANCE)
}

#[derive(Clone, Copy)]
enum DetourAxis {
    Horizontal,
    Vertical,
}

impl DetourAxis {
    fn along(self, point: RouterPoint) -> f64 {
        match self {
            Self::Horizontal => point.x,
            Self::Vertical => point.y,
        }
    }

    fn cross(self, point: RouterPoint) -> f64 {
        match self {
            Self::Horizontal => point.y,
            Self::Vertical => point.x,
        }
    }

    fn point(self, along: f64, cross: f64) -> RouterPoint {
        match self {
            Self::Horizontal => RouterPoint::new(along, cross),
            Self::Vertical => RouterPoint::new(cross, along),
        }
    }

    fn label_along_size(self, label_node: &Node) -> f64 {
        match self {
            Self::Horizontal => label_node.width.unwrap_or(0.0),
            Self::Vertical => label_node.height.unwrap_or(0.0),
        }
    }

    fn label_cross_size(self, label_node: &Node) -> f64 {
        match self {
            Self::Horizontal => label_node.height.unwrap_or(0.0),
            Self::Vertical => label_node.width.unwrap_or(0.0),
        }
    }
}

fn detour_axis(segment: Segment) -> DetourAxis {
    if segment.orientation == SegmentOrientation::H {
        DetourAxis::Horizontal
    } else {
        DetourAxis::Vertical
    }
}

fn build_label_detour_points(
    points: &[RouterPoint],
    segment_index: usize,
    segment: Segment,
    label_node: &Node,
    center_along: f64,
    detour_line_coord: f64,
) -> Option<Vec<RouterPoint>> {
    if !is_finite_positive_number(label_node.width) || !is_finite_positive_number(label_node.height)
    {
        return None;
    }

    // Keep the original terminal-adjacent anchors intact; label placement owns only the interior
    // detour.
    let a = points[segment_index];
    let b = points[segment_index + 1];
    let axis = detour_axis(segment);
    let source_line = axis.cross(segment.a);
    let a_along = axis.along(a);
    let b_along = axis.along(b);
    let span = axis.label_along_size(label_node) + LABEL_CLEARANCE * 2.0;
    let detour_start = center_along - span / 2.0;
    let detour_end = center_along + span / 2.0;
    let extends_before_start = detour_start < a_along.min(b_along) - LABEL_EPSILON;
    let extends_after_end = detour_end > a_along.max(b_along) + LABEL_EPSILON;
    let along_direction = if b_along >= a_along { 1.0 } else { -1.0 };
    let segment_inset =
        0.0_f64.max(EDGE_END_MARKER_CLEARANCE.min((b_along - a_along).abs() / 2.0 - LABEL_EPSILON));
    let entry_along = a_along + along_direction * segment_inset;
    let exit_along = b_along - along_direction * segment_inset;

    let mut detour_points: Vec<RouterPoint> = points[..segment_index].to_vec();
    if !extends_before_start && !extends_after_end {
        detour_points.push(a);
        detour_points.push(axis.point(detour_start, source_line));
        detour_points.push(axis.point(detour_start, detour_line_coord));
        detour_points.push(axis.point(detour_end, detour_line_coord));
        detour_points.push(axis.point(detour_end, source_line));
        detour_points.push(b);
        detour_points.extend_from_slice(&points[segment_index + 2..]);
        return Some(normalize_polyline(&detour_points).points);
    }

    let return_line = detour_return_coordinate(
        source_line,
        detour_line_coord,
        axis.label_cross_size(label_node) / 2.0,
    );
    detour_points.push(a);
    if extends_before_start {
        detour_points.push(axis.point(entry_along, source_line));
        detour_points.push(axis.point(entry_along, return_line));
        detour_points.push(axis.point(detour_start, return_line));
        detour_points.push(axis.point(detour_start, detour_line_coord));
    } else {
        detour_points.push(axis.point(detour_start, source_line));
        detour_points.push(axis.point(detour_start, detour_line_coord));
    }
    detour_points.push(axis.point(detour_end, detour_line_coord));
    if extends_after_end {
        detour_points.push(axis.point(detour_end, return_line));
        detour_points.push(axis.point(exit_along, return_line));
        detour_points.push(axis.point(exit_along, source_line));
    } else {
        detour_points.push(axis.point(detour_end, source_line));
    }
    detour_points.push(b);
    detour_points.extend_from_slice(&points[segment_index + 2..]);
    Some(normalize_polyline(&detour_points).points)
}

fn build_extended_label_detour_points(
    points: &[RouterPoint],
    segment_index: usize,
    segment: Segment,
    label_node: &Node,
    center_along: f64,
    detour_line_coord: f64,
    separate_entry_and_exit: bool,
) -> Option<Vec<RouterPoint>> {
    if !is_finite_positive_number(label_node.width) || !is_finite_positive_number(label_node.height)
    {
        return None;
    }

    let a = points[segment_index];
    let b = points[segment_index + 1];
    // Self-loop terminal segments are short; quarter points keep the entry and exit lanes distinct.
    let inset_divisor = if separate_entry_and_exit { 4.0 } else { 2.0 };
    let axis = detour_axis(segment);
    let source_line = axis.cross(segment.a);
    let a_along = axis.along(a);
    let b_along = axis.along(b);
    let span = axis.label_along_size(label_node) + LABEL_CLEARANCE * 2.0;
    let detour_start = center_along - span / 2.0;
    let detour_end = center_along + span / 2.0;
    let along_direction = if b_along >= a_along { 1.0 } else { -1.0 };
    let segment_inset = 0.0_f64.max(
        EDGE_END_MARKER_CLEARANCE.min((b_along - a_along).abs() / inset_divisor - LABEL_EPSILON),
    );
    let entry_along = a_along + along_direction * segment_inset;
    let exit_along = b_along - along_direction * segment_inset;
    let return_line = detour_return_coordinate(
        source_line,
        detour_line_coord,
        axis.label_cross_size(label_node) / 2.0,
    );
    let mut detour_points: Vec<RouterPoint> = points[..segment_index].to_vec();
    detour_points.push(a);
    detour_points.push(axis.point(entry_along, source_line));
    detour_points.push(axis.point(entry_along, detour_line_coord));
    detour_points.push(axis.point(detour_start, detour_line_coord));
    detour_points.push(axis.point(detour_end, detour_line_coord));
    detour_points.push(axis.point(detour_end, return_line));
    detour_points.push(axis.point(exit_along, return_line));
    detour_points.push(axis.point(exit_along, source_line));
    detour_points.push(b);
    detour_points.extend_from_slice(&points[segment_index + 2..]);
    Some(normalize_polyline(&detour_points).points)
}

fn external_detour_center_candidates(segment: Segment, label_node: &Node) -> Vec<f64> {
    if !is_finite_positive_number(label_node.width) || !is_finite_positive_number(label_node.height)
    {
        return Vec::new();
    }

    if segment.orientation == SegmentOrientation::H {
        let min_x = segment.a.x.min(segment.b.x);
        let max_x = segment.a.x.max(segment.b.x);
        return vec![
            min_x - label_node.width.unwrap_or(0.0) / 2.0 - LABEL_GUTTER,
            max_x + label_node.width.unwrap_or(0.0) / 2.0 + LABEL_GUTTER,
        ];
    }

    let min_y = segment.a.y.min(segment.b.y);
    let max_y = segment.a.y.max(segment.b.y);
    vec![
        min_y - label_node.height.unwrap_or(0.0) / 2.0 - LABEL_GUTTER,
        max_y + label_node.height.unwrap_or(0.0) / 2.0 + LABEL_GUTTER,
    ]
}

#[derive(Clone, Copy, Debug, Default)]
struct BlockedIntersectionScore {
    count: usize,
    length: f64,
}

fn blocked_intersection_score(points: &[RouterPoint], rect: Rect) -> BlockedIntersectionScore {
    let mut count = 0;
    let mut length = 0.0;
    for segment in normalize_polyline(points).segments {
        if !segment_intersects_rect_interior(segment.a, segment.b, rect) {
            continue;
        }
        count += 1;
        if segment.orientation == SegmentOrientation::H {
            length += 0.0_f64.max(
                segment.a.x.max(segment.b.x).min(rect.right)
                    - segment.a.x.min(segment.b.x).max(rect.left),
            );
        } else if segment.orientation == SegmentOrientation::V {
            length += 0.0_f64.max(
                segment.a.y.max(segment.b.y).min(rect.bottom)
                    - segment.a.y.min(segment.b.y).max(rect.top),
            );
        }
    }
    BlockedIntersectionScore { count, length }
}

fn reduces_blocked_intersections(
    candidate: &[RouterPoint],
    blocked_rect: Rect,
    baseline: BlockedIntersectionScore,
) -> bool {
    let score = blocked_intersection_score(candidate, blocked_rect);
    score.count < baseline.count
        || (score.count == baseline.count && score.length < baseline.length - LABEL_EPSILON)
}

fn reroute_lane(
    points: &[RouterPoint],
    blocked_rect: Rect,
    blocking_reservations: &[PlacedLabel],
) -> Option<RerouteLane> {
    let segment = normalize_polyline(points)
        .segments
        .into_iter()
        .find(|candidate| {
            segment_intersects_rect_interior(candidate.a, candidate.b, blocked_rect)
        })?;
    if segment.orientation == SegmentOrientation::Z {
        return None;
    }

    let side = if segment.orientation == SegmentOrientation::H {
        if (segment.a.y - blocked_rect.top).abs() <= (blocked_rect.bottom - segment.a.y).abs() {
            LaneSide::Top
        } else {
            LaneSide::Bottom
        }
    } else if (segment.a.x - blocked_rect.left).abs() <= (blocked_rect.right - segment.a.x).abs() {
        LaneSide::Left
    } else {
        LaneSide::Right
    };
    let mut reservation_ids: Vec<String> = blocking_reservations
        .iter()
        .map(|reservation| reservation.edge_id.clone())
        .collect();
    reservation_ids.sort_by(|a, b| compare_code_units(a, b));
    Some(RerouteLane {
        key: format!(
            "[{}]|{}|{:?}",
            reservation_ids.join(","),
            if segment.orientation == SegmentOrientation::H {
                "H"
            } else {
                "V"
            },
            side
        ),
        orientation: segment.orientation,
        side,
    })
}

#[allow(clippy::too_many_arguments)]
fn reroute_segment_around_rect(
    data: &LayoutData,
    edge_index: usize,
    points: &[RouterPoint],
    segment_index: usize,
    blocked_rect: Rect,
    context: &mut EdgeLabelContext,
    placed_labels_by_edge_id: &PlacedLabels,
    lane: Option<&RerouteLane>,
) -> Result<Option<Vec<RouterPoint>>, LabelError> {
    let normalized = normalize_polyline(points);
    let baseline_score = blocked_intersection_score(&normalized.points, blocked_rect);
    let Some(segment) = normalized.segments.get(segment_index).copied() else {
        return Ok(None);
    };
    if segment.orientation == SegmentOrientation::Z {
        return Ok(None);
    }

    let a = normalized.points[segment_index];
    let b = normalized.points[segment_index + 1];
    let edge = &data.edges[edge_index];
    let mut skip_node_ids = HashSet::new();
    if let Some(own_label_node_id) = placed_labels_by_edge_id
        .get(&edge.id)
        .map(|placed| placed.node_id.clone())
    {
        skip_node_ids.insert(own_label_node_id);
    }

    // One detour may remove only the first intersection with a wide label. Accept strictly
    // monotonic progress here; the bounded outer loop completes the reroute segment by segment.
    if segment.orientation == SegmentOrientation::H {
        let min_x = a.x.min(b.x);
        let max_x = a.x.max(b.x);
        let entry = min_x.max(blocked_rect.left - LABEL_CLEARANCE);
        let exit = max_x.min(blocked_rect.right + LABEL_CLEARANCE);
        if exit - entry <= LABEL_EPSILON {
            increment_metric(
                &context.metrics,
                LabelMetric::RerouteRejectedDegenerateSpan,
                1,
            );
            return Ok(None);
        }

        for y in reroute_line_candidates(segment, blocked_rect, context, &skip_node_ids, lane)? {
            increment_metric(&context.metrics, LabelMetric::RerouteCandidatesEvaluated, 1);
            let mut candidate_points = normalized.points[..segment_index].to_vec();
            candidate_points.push(a);
            candidate_points.push(RouterPoint::new(entry, segment.a.y));
            candidate_points.push(RouterPoint::new(entry, y));
            candidate_points.push(RouterPoint::new(exit, y));
            candidate_points.push(RouterPoint::new(exit, segment.a.y));
            candidate_points.push(b);
            candidate_points.extend_from_slice(&normalized.points[segment_index + 2..]);
            let candidate_points = normalize_polyline(&candidate_points).points;
            if polyline_intersects_rect(&candidate_points, blocked_rect)
                && !reduces_blocked_intersections(&candidate_points, blocked_rect, baseline_score)
            {
                increment_metric(&context.metrics, LabelMetric::RerouteRejectedBlockedRect, 1);
                continue;
            }
            if polyline_intersects_rect(&candidate_points, blocked_rect) {
                increment_metric(
                    &context.metrics,
                    LabelMetric::ReroutePartialProgressCandidates,
                    1,
                );
            }
            if polyline_hits_protected_obstacles(data, &candidate_points, context, &skip_node_ids)?
            {
                increment_metric(
                    &context.metrics,
                    LabelMetric::RerouteRejectedProtectedObstacle,
                    1,
                );
                continue;
            }
            if !label_still_anchored(&edge.id, &candidate_points, placed_labels_by_edge_id) {
                increment_metric(&context.metrics, LabelMetric::RerouteRejectedLabelAnchor, 1);
                continue;
            }
            return Ok(Some(candidate_points));
        }
        return Ok(None);
    }

    let min_y = a.y.min(b.y);
    let max_y = a.y.max(b.y);
    let entry = min_y.max(blocked_rect.top - LABEL_CLEARANCE);
    let exit = max_y.min(blocked_rect.bottom + LABEL_CLEARANCE);
    if exit - entry <= LABEL_EPSILON {
        increment_metric(
            &context.metrics,
            LabelMetric::RerouteRejectedDegenerateSpan,
            1,
        );
        return Ok(None);
    }

    for x in reroute_line_candidates(segment, blocked_rect, context, &skip_node_ids, lane)? {
        increment_metric(&context.metrics, LabelMetric::RerouteCandidatesEvaluated, 1);
        let mut candidate_points = normalized.points[..segment_index].to_vec();
        candidate_points.push(a);
        candidate_points.push(RouterPoint::new(segment.a.x, entry));
        candidate_points.push(RouterPoint::new(x, entry));
        candidate_points.push(RouterPoint::new(x, exit));
        candidate_points.push(RouterPoint::new(segment.a.x, exit));
        candidate_points.push(b);
        candidate_points.extend_from_slice(&normalized.points[segment_index + 2..]);
        let candidate_points = normalize_polyline(&candidate_points).points;
        if polyline_intersects_rect(&candidate_points, blocked_rect)
            && !reduces_blocked_intersections(&candidate_points, blocked_rect, baseline_score)
        {
            increment_metric(&context.metrics, LabelMetric::RerouteRejectedBlockedRect, 1);
            continue;
        }
        if polyline_intersects_rect(&candidate_points, blocked_rect) {
            increment_metric(
                &context.metrics,
                LabelMetric::ReroutePartialProgressCandidates,
                1,
            );
        }
        if polyline_hits_protected_obstacles(data, &candidate_points, context, &skip_node_ids)? {
            increment_metric(
                &context.metrics,
                LabelMetric::RerouteRejectedProtectedObstacle,
                1,
            );
            continue;
        }
        if !label_still_anchored(&edge.id, &candidate_points, placed_labels_by_edge_id) {
            increment_metric(&context.metrics, LabelMetric::RerouteRejectedLabelAnchor, 1);
            continue;
        }
        return Ok(Some(candidate_points));
    }

    Ok(None)
}

fn reroute_foreign_edge_around_rect(
    data: &mut LayoutData,
    edge_index: usize,
    blocked_rect: Rect,
    context: &mut EdgeLabelContext,
    placed_labels_by_edge_id: &PlacedLabels,
    overrides: &mut EdgePointOverrides,
    lane: Option<&RerouteLane>,
) -> Result<Option<Vec<RouterPoint>>, LabelError> {
    let mut current_points = points_for_edge(data, edge_index, Some(overrides), context);
    for _attempt in 0..8 {
        if !polyline_intersects_rect(&current_points, blocked_rect) {
            return Ok(Some(current_points));
        }

        let normalized = normalize_polyline(&current_points);
        let segment_index = normalized
            .points
            .iter()
            .enumerate()
            .position(|(index, point)| {
                index != normalized.points.len() - 1
                    && segment_intersects_rect_interior(
                        *point,
                        normalized.points[index + 1],
                        blocked_rect,
                    )
            });
        let Some(segment_index) = segment_index else {
            return Ok(Some(current_points));
        };

        let rerouted = reroute_segment_around_rect(
            data,
            edge_index,
            &normalized.points,
            segment_index,
            blocked_rect,
            context,
            placed_labels_by_edge_id,
            lane,
        )?;
        let Some(rerouted) = rerouted else {
            return Ok(None);
        };
        current_points = rerouted;
    }

    Ok((!polyline_intersects_rect(&current_points, blocked_rect)).then_some(current_points))
}

fn reroute_foreign_edge_around_labels(
    data: &mut LayoutData,
    edge_index: usize,
    reservations: &[PlacedLabel],
    context: &mut EdgeLabelContext,
    placed_labels_by_edge_id: &PlacedLabels,
    overrides: &mut EdgePointOverrides,
) -> Result<Option<Vec<RouterPoint>>, LabelError> {
    let edge_id = data.edges[edge_index].id.clone();
    let initial_points = points_for_edge(data, edge_index, Some(overrides), context);
    let mut current_points = initial_points.clone();
    // A detour around a later label can re-enter an earlier reservation, so make one deterministic
    // repair pass and one convergence pass before abandoning the edge-level recovery.
    for _pass in 0..2 {
        let mut changed = false;
        for reservation in reservations {
            if !polyline_intersects_rect(&current_points, reservation.rect) {
                continue;
            }
            increment_metric(
                &context.metrics,
                LabelMetric::IndividualLabelRerouteAttempts,
                1,
            );
            overrides.set(&edge_id, current_points.clone());
            let rerouted = reroute_foreign_edge_around_rect(
                data,
                edge_index,
                reservation.rect,
                context,
                placed_labels_by_edge_id,
                overrides,
                None,
            )?;
            let Some(rerouted) = rerouted else {
                overrides.set(&edge_id, initial_points.clone());
                return Ok(None);
            };
            increment_metric(
                &context.metrics,
                LabelMetric::IndividualLabelRerouteSuccesses,
                1,
            );
            current_points = rerouted;
            changed = true;
        }
        if reservations
            .iter()
            .all(|reservation| !polyline_intersects_rect(&current_points, reservation.rect))
        {
            return Ok(Some(current_points));
        }
        if !changed {
            break;
        }
    }
    overrides.set(&edge_id, initial_points);
    Ok(None)
}

fn label_placement_with_extended_detour(
    data: &mut LayoutData,
    edge_index: usize,
    label_node_index: usize,
    segment_candidate: &SegmentCandidate,
    context: &mut EdgeLabelContext,
    placed_labels_by_edge_id: &PlacedLabels,
) -> Result<Option<RouterPoint>, LabelError> {
    let label_node = data.nodes[label_node_index].clone();
    if !is_finite_positive_number(label_node.width) || !is_finite_positive_number(label_node.height)
    {
        return Ok(None);
    }

    let base_points = points_for_edge(data, edge_index, None, context);
    let segment = segment_candidate.segment;

    for detour_line_coord in
        detour_line_candidates(data, edge_index, &label_node, segment, context)?
    {
        let center_candidates = unique_sorted_candidates(
            &external_detour_center_candidates(segment, &label_node),
            if segment.orientation == SegmentOrientation::H {
                (segment.a.x.min(segment.b.x) + segment.a.x.max(segment.b.x)) / 2.0
            } else {
                (segment.a.y.min(segment.b.y) + segment.a.y.max(segment.b.y)) / 2.0
            },
        );

        for center_along in center_candidates {
            let separate_entry_and_exit = {
                let edge = &data.edges[edge_index];
                edge.start == edge.end
            };
            let Some(candidate_points) = build_extended_label_detour_points(
                &base_points,
                segment_candidate.segment_index,
                segment,
                &label_node,
                center_along,
                detour_line_coord,
                separate_entry_and_exit,
            ) else {
                continue;
            };

            if separate_entry_and_exit && polyline_has_self_conflict(&candidate_points) {
                continue;
            }

            let mut skip = HashSet::new();
            skip.insert(label_node.id.clone());
            if polyline_hits_protected_obstacles(data, &candidate_points, context, &skip)? {
                continue;
            }

            let center = if segment.orientation == SegmentOrientation::H {
                RouterPoint::new(center_along, detour_line_coord)
            } else {
                RouterPoint::new(detour_line_coord, center_along)
            };
            let rect = label_rect_at(&label_node, center);
            let mut overrides = EdgePointOverrides::default();
            let own_edge_id = data.edges[edge_index].id.clone();
            overrides.set(&own_edge_id, candidate_points.clone());

            let foreign_edge_ids =
                foreign_edge_ids_intersecting_rect(context, rect, &own_edge_id, Some(&overrides))?;
            let foreign_edge_indexes: Vec<usize> = foreign_edge_ids
                .iter()
                .filter_map(|id| data.edges.iter().position(|edge| &edge.id == id))
                .collect();

            let mut ok = true;
            for foreign_edge_index in foreign_edge_indexes {
                let rerouted = reroute_foreign_edge_around_rect(
                    data,
                    foreign_edge_index,
                    rect,
                    context,
                    placed_labels_by_edge_id,
                    &mut overrides,
                    None,
                )?;
                let Some(rerouted) = rerouted else {
                    ok = false;
                    break;
                };
                let foreign_id = data.edges[foreign_edge_index].id.clone();
                overrides.set(&foreign_id, rerouted);
            }

            if !ok {
                continue;
            }
            if !rect_safe_for_label(
                data,
                rect,
                edge_index,
                &label_node.id,
                context,
                Some(&overrides),
                None,
            )? {
                continue;
            }

            set_edge_points(data, context, edge_index, &candidate_points);
            for (edge_id, points) in overrides.entries.clone() {
                if edge_id == own_edge_id {
                    continue;
                }
                if let Some(index) = data.edges.iter().position(|edge| edge.id == edge_id) {
                    set_edge_points(data, context, index, &points);
                }
            }
            return Ok(Some(center));
        }
    }

    Ok(None)
}

fn label_placement_with_detour(
    data: &mut LayoutData,
    edge_index: usize,
    label_node_index: usize,
    segment_candidate: &SegmentCandidate,
    context: &mut EdgeLabelContext,
    placed_labels_by_edge_id: &PlacedLabels,
) -> Result<Option<RouterPoint>, LabelError> {
    let label_node = data.nodes[label_node_index].clone();
    if !is_finite_positive_number(label_node.width) || !is_finite_positive_number(label_node.height)
    {
        return Ok(None);
    }

    let base_points = points_for_edge(data, edge_index, None, context);
    let segment = segment_candidate.segment;
    let separate_entry_and_exit = {
        let edge = &data.edges[edge_index];
        edge.start == edge.end
    };

    for detour_line_coord in
        detour_line_candidates(data, edge_index, &label_node, segment, context)?
    {
        let center_candidates = detour_center_candidates(
            data,
            edge_index,
            &label_node,
            segment,
            detour_line_coord,
            context,
        )?;
        for center_along in center_candidates {
            let Some(candidate_points) = build_label_detour_points(
                &base_points,
                segment_candidate.segment_index,
                segment,
                &label_node,
                center_along,
                detour_line_coord,
            ) else {
                continue;
            };

            if separate_entry_and_exit && polyline_has_self_conflict(&candidate_points) {
                continue;
            }

            let mut skip = HashSet::new();
            skip.insert(label_node.id.clone());
            if polyline_hits_protected_obstacles(data, &candidate_points, context, &skip)? {
                continue;
            }

            let center = if segment.orientation == SegmentOrientation::H {
                RouterPoint::new(center_along, detour_line_coord)
            } else {
                RouterPoint::new(detour_line_coord, center_along)
            };
            let rect = label_rect_at(&label_node, center);
            let mut overrides = EdgePointOverrides::default();
            let own_edge_id = data.edges[edge_index].id.clone();
            overrides.set(&own_edge_id, candidate_points.clone());

            let foreign_edge_ids =
                foreign_edge_ids_intersecting_rect(context, rect, &own_edge_id, Some(&overrides))?;
            let foreign_edge_indexes: Vec<usize> = foreign_edge_ids
                .iter()
                .filter_map(|id| data.edges.iter().position(|edge| &edge.id == id))
                .collect();

            let mut ok = true;
            for foreign_edge_index in foreign_edge_indexes {
                let rerouted = reroute_foreign_edge_around_rect(
                    data,
                    foreign_edge_index,
                    rect,
                    context,
                    placed_labels_by_edge_id,
                    &mut overrides,
                    None,
                )?;
                let Some(rerouted) = rerouted else {
                    ok = false;
                    break;
                };
                let foreign_id = data.edges[foreign_edge_index].id.clone();
                overrides.set(&foreign_id, rerouted);
            }

            if !ok {
                continue;
            }
            if !rect_safe_for_label(
                data,
                rect,
                edge_index,
                &label_node.id,
                context,
                Some(&overrides),
                None,
            )? {
                continue;
            }

            set_edge_points(data, context, edge_index, &candidate_points);
            for (edge_id, points) in overrides.entries.clone() {
                if edge_id == own_edge_id {
                    continue;
                }
                if let Some(index) = data.edges.iter().position(|edge| edge.id == edge_id) {
                    set_edge_points(data, context, index, &points);
                }
            }
            return Ok(Some(center));
        }
    }

    Ok(None)
}

fn park_degenerate_label(
    data: &mut LayoutData,
    edge_index: usize,
    label_node_index: usize,
    node_by_id: &HashMap<String, usize>,
) {
    let edge = data.edges[edge_index].clone();
    let route_point = edge
        .points
        .iter()
        .copied()
        .find(|point| point.x.is_finite() && point.y.is_finite());
    let source_point = edge
        .start
        .as_deref()
        .and_then(|id| node_by_id.get(id))
        .map(|index| &data.nodes[*index])
        .filter(|node| has_finite_position(node))
        .and_then(|node| node.x.zip(node.y).map(|(x, y)| RouterPoint::new(x, y)));
    let target_point = edge
        .end
        .as_deref()
        .and_then(|id| node_by_id.get(id))
        .map(|index| &data.nodes[*index])
        .and_then(|node| node.x.zip(node.y).map(|(x, y)| RouterPoint::new(x, y)));
    let fallback = route_point.or(source_point).or(target_point);
    if let Some(point) = fallback.filter(|point| point.x.is_finite() && point.y.is_finite()) {
        data.nodes[label_node_index].x = Some(point.x);
        data.nodes[label_node_index].y = Some(point.y);
        return;
    }
    data.nodes[label_node_index].x = None;
    data.nodes[label_node_index].y = None;
}

fn place_label_at_route_midpoint(
    data: &mut LayoutData,
    edge_index: usize,
    label_node_index: usize,
    node_by_id: &HashMap<String, usize>,
) {
    let mut longest_segment: Option<Segment> = None;
    for segment in normalize_polyline(&data.edges[edge_index].points).segments {
        if segment.orientation == SegmentOrientation::Z {
            continue;
        }
        if longest_segment.is_none_or(|longest| segment_length(segment) > segment_length(longest)) {
            longest_segment = Some(segment);
        }
    }
    let Some(segment) = longest_segment else {
        park_degenerate_label(data, edge_index, label_node_index, node_by_id);
        return;
    };
    data.nodes[label_node_index].x = Some((segment.a.x + segment.b.x) / 2.0);
    data.nodes[label_node_index].y = Some((segment.a.y + segment.b.y) / 2.0);
}

fn preferred_grid_label_node_id(edge: &Edge) -> String {
    format!(
        "{GRID_LABEL_PREFIX}{}-{}-{}",
        edge.start.as_deref().unwrap_or(""),
        edge.end.as_deref().unwrap_or(""),
        edge.id
    )
}

fn ensure_unique_grid_label_node_id(edge: &Edge, node_by_id: &HashMap<String, usize>) -> String {
    let base_id = edge
        .label_node_id
        .clone()
        .unwrap_or_else(|| preferred_grid_label_node_id(edge));
    let mut candidate_id = base_id.clone();
    let mut suffix = 1;
    loop {
        if !node_by_id.contains_key(&candidate_id) {
            return candidate_id;
        }
        candidate_id = format!("{base_id}-{suffix}");
        suffix += 1;
    }
}

fn is_prepared_label_node(node: &Node) -> bool {
    is_edge_label_node(node)
}

/// `prepareGridLayout`: labels become measured helper nodes before grid sizing, but remain outside
/// the placement forest. The original edge text is cleared so the shared renderer draws it once.
pub fn prepare_grid_layout(data: &mut LayoutData) {
    let mut node_by_id: HashMap<String, usize> = data
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id.clone(), index))
        .collect();

    let mut new_nodes: Vec<Node> = Vec::new();
    for edge_index in 0..data.edges.len() {
        let label = data.edges[edge_index].label.clone();
        let Some(label) = label.filter(|label| !label.is_empty()) else {
            continue;
        };

        let source_dir = data.edges[edge_index]
            .start
            .as_deref()
            .and_then(|id| node_by_id.get(id))
            .and_then(|index| data.nodes[*index].dir.clone());
        let existing_helper_index = data.edges[edge_index]
            .label_node_id
            .as_deref()
            .and_then(|id| node_by_id.get(id))
            .copied();
        if let Some(existing_helper_index) = existing_helper_index
            && is_prepared_label_node(&data.nodes[existing_helper_index])
        {
            let edge_label_style = data.edges[edge_index]
                .label_style
                .as_ref()
                .and_then(|styles| styles.first())
                .cloned()
                .unwrap_or_default();
            let helper = &mut data.nodes[existing_helper_index];
            helper.label = Some(label);
            helper.label_style = Some(edge_label_style);
            helper.edge_start = data.edges[edge_index].start.clone().or(Some(String::new()));
            helper.edge_end = data.edges[edge_index].end.clone().or(Some(String::new()));
            if let Some(dir) = source_dir {
                helper.dir = Some(dir);
            }
            data.edges[edge_index].label = None;
            continue;
        }

        let label_node_id = ensure_unique_grid_label_node_id(&data.edges[edge_index], &node_by_id);
        let label_style = data.edges[edge_index]
            .label_style
            .as_ref()
            .and_then(|styles| styles.first())
            .cloned()
            .unwrap_or_default();
        let label_node = Node {
            id: label_node_id.clone(),
            label: Some(label),
            edge_start: data.edges[edge_index].start.clone().or(Some(String::new())),
            edge_end: data.edges[edge_index].end.clone().or(Some(String::new())),
            shape: Some("labelRect".to_string()),
            width: Some(0.0),
            height: Some(0.0),
            is_edge_label: true,
            is_dummy: true,
            is_group: false,
            parent_id: None,
            label_style: Some(label_style),
            dir: source_dir,
            ..Node::default()
        };
        new_nodes.push(label_node);
        let label_node_index = data.nodes.len() + new_nodes.len() - 1;
        node_by_id.insert(label_node_id.clone(), label_node_index);
        data.edges[edge_index].label_node_id = Some(label_node_id);
        data.edges[edge_index].label = None;
    }

    data.nodes.extend(new_nodes);
}

fn restore_base_edge_points(data: &mut LayoutData, base_edge_points: &[Option<Vec<RouterPoint>>]) {
    for (edge_index, edge) in data.edges.iter_mut().enumerate() {
        if let Some(points) = &base_edge_points[edge_index] {
            edge.points = points.iter().copied().map(clone_point).collect();
        } else {
            edge.points.clear();
        }
    }
}

/// The label transaction. Upstream's signature is `positionGridEdgeLabels(data, instrumentation,
/// routingInstrumentation, options)`; the f64 pass order and the two-pass retry match it.
pub fn position_grid_edge_labels(
    data: &mut LayoutData,
    instrumentation: Option<&mut GridEdgeLabelInstrumentation>,
    routing_instrumentation: Option<&mut GridRoutingInstrumentation>,
    options: &GridEdgeLabelOptions,
) -> Result<(), GridError> {
    let metrics = Rc::new(RefCell::new(GridEdgeLabelInstrumentation::default()));
    let result =
        position_grid_edge_labels_inner(data, metrics.clone(), routing_instrumentation, options);
    if let Some(target) = instrumentation {
        *target = metrics.borrow().clone();
    }
    result
}

fn position_grid_edge_labels_inner(
    data: &mut LayoutData,
    metrics: Rc<RefCell<GridEdgeLabelInstrumentation>>,
    mut routing_instrumentation: Option<&mut GridRoutingInstrumentation>,
    options: &GridEdgeLabelOptions,
) -> Result<(), GridError> {
    let node_by_id: HashMap<String, usize> = data
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id.clone(), index))
        .collect();
    // Coordinates belong to the previous invocation. Retain them only for rollback; the current
    // pass must build its obstacle index from labels placed during this invocation.
    let label_node_indexes: Vec<usize> = data
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| is_edge_label_node(node))
        .map(|(index, _)| index)
        .collect();
    let base_label_positions: Vec<(Option<f64>, Option<f64>)> = label_node_indexes
        .iter()
        .map(|index| (data.nodes[*index].x, data.nodes[*index].y))
        .collect();
    for index in &label_node_indexes {
        data.nodes[*index].x = None;
        data.nodes[*index].y = None;
    }

    let mut prepared_labels: Vec<LabelWorkItem> = Vec::new();
    for (source_index, edge) in data.edges.iter().enumerate() {
        let Some(label_node_id) = edge.label_node_id.as_deref() else {
            continue;
        };
        let Some(label_node_index) = node_by_id.get(label_node_id).copied() else {
            continue;
        };
        let label_node = &data.nodes[label_node_index];
        if !is_finite_positive_number(label_node.width)
            || !is_finite_positive_number(label_node.height)
        {
            increment_metric(&metrics, LabelMetric::UnmeasuredLabelsSkipped, 1);
            continue;
        }
        prepared_labels.push(LabelWorkItem {
            edge_index: source_index,
            label_node_index,
            source_index,
        });
    }

    if prepared_labels.is_empty() {
        return Ok(());
    }

    // Keep the routed geometry for midpoint degradation and the full snapshot for error rollback.
    let base_edge_points: Vec<Option<Vec<RouterPoint>>> = data
        .edges
        .iter()
        .map(|edge| Some(edge.points.iter().copied().map(clone_point).collect()))
        .collect();

    let restore_base_state = |data: &mut LayoutData| {
        restore_base_edge_points(data, &base_edge_points);
        for (label_index, index) in label_node_indexes.iter().enumerate() {
            let (x, y) = base_label_positions[label_index];
            if let (Some(x), Some(y)) = (x, y)
                && x.is_finite()
                && y.is_finite()
            {
                data.nodes[*index].x = Some(x);
                data.nodes[*index].y = Some(y);
            } else {
                data.nodes[*index].x = None;
                data.nodes[*index].y = None;
            }
        }
    };

    let mut work_items: Vec<LabelWorkItem> = Vec::new();
    for item in prepared_labels {
        let has_segments = !normalize_polyline(&data.edges[item.edge_index].points)
            .segments
            .is_empty();
        if has_segments {
            work_items.push(item);
        } else {
            park_degenerate_label(data, item.edge_index, item.label_node_index, &node_by_id);
            increment_metric(&metrics, LabelMetric::DegenerateLabelRoutesSkipped, 1);
        }
    }
    work_items.sort_by(|a, b| {
        a.source_index.cmp(&b.source_index).then_with(|| {
            compare_code_units(&data.edges[a.edge_index].id, &data.edges[b.edge_index].id)
        })
    });
    if work_items.is_empty() {
        return Ok(());
    }

    let place_labels_at_route_midpoints = |data: &mut LayoutData| {
        restore_base_edge_points(data, &base_edge_points);
        for item in &work_items {
            place_label_at_route_midpoint(
                data,
                item.edge_index,
                item.label_node_index,
                &node_by_id,
            );
        }
    };

    let configured_index_work = options.max_index_work.unwrap_or(DEFAULT_MAX_INDEX_WORK);
    let index_work_limit = if configured_index_work.is_finite() && configured_index_work >= 0.0 {
        configured_index_work.floor()
    } else {
        DEFAULT_MAX_INDEX_WORK
    };
    let index_work_budget = Rc::new(RefCell::new(LabelIndexWorkBudget {
        used: 0.0,
        limit: index_work_limit,
        metrics: metrics.clone(),
    }));
    let mut placement_failures: Vec<PlacementFailure> = Vec::new();

    for pass in 1..=2 {
        // Pass 1 prefers existing segments and foreign-edge reroutes. Pass 2 additionally permits
        // owner-edge detours, rebuilding every route from the original routing result.
        increment_metric(&metrics, LabelMetric::LabelPasses, 1);
        let mut context =
            match create_edge_label_context(data, index_work_budget.clone(), metrics.clone()) {
                Ok(context) => context,
                Err(LabelError::WorkLimit) => {
                    place_labels_at_route_midpoints(data);
                    increment_metric(&metrics, LabelMetric::IndexWorkLimitFallbacks, 1);
                    if let Some(routing) = routing_instrumentation.as_mut() {
                        routing.resource_limit_fallbacks += 1;
                        routing.increment_reason(GridRoutingFallbackReason::LabelIndexWorkCap);
                    }
                    return Ok(());
                }
                Err(LabelError::Grid(error)) => return Err(error),
                Err(LabelError::PlacementExhausted(failure)) => {
                    placement_failures.push(*failure);
                    continue;
                }
            };
        let mut placed_labels_by_edge_id = PlacedLabels::default();
        let mut allowed_overlaps_by_label_edge_id: HashMap<String, HashSet<String>> =
            HashMap::new();
        let pass_start_points: Vec<Vec<RouterPoint>> = (0..data.edges.len())
            .map(|index| points_for_edge(data, index, None, &context))
            .collect();
        let mut proposed_owner_routes: Vec<(usize, Vec<RouterPoint>)> = Vec::new();

        let outcome: Result<(), LabelError> = (|| {
            for item in &work_items {
                let edge_index = item.edge_index;
                let label_node_index = item.label_node_index;
                let mut segments = candidate_segments(data, edge_index, &context);
                segments.sort_by(|a, b| {
                    let length_delta = segment_length(b.segment) - segment_length(a.segment);
                    if length_delta.abs() > LABEL_EPSILON {
                        return length_delta.partial_cmp(&0.0).unwrap_or(Ordering::Equal);
                    }
                    a.order.cmp(&b.order)
                });

                let label_node = data.nodes[label_node_index].clone();
                let mut label_center: Option<RouterPoint> = None;
                for segment_candidate in &segments {
                    label_center = placement_on_existing_segment(
                        data,
                        edge_index,
                        &label_node,
                        segment_candidate.segment,
                        &mut context,
                    )?;
                    if label_center.is_some() {
                        break;
                    }
                }

                if label_center.is_none() {
                    for segment_candidate in &segments {
                        label_center = placement_on_existing_segment_with_reroutes(
                            data,
                            edge_index,
                            &label_node,
                            segment_candidate.segment,
                            &mut context,
                            &placed_labels_by_edge_id,
                        )?;
                        if label_center.is_some() {
                            break;
                        }
                    }
                }

                if pass == 2 && label_center.is_none() {
                    for segment_candidate in &segments {
                        label_center = label_placement_with_detour(
                            data,
                            edge_index,
                            label_node_index,
                            segment_candidate,
                            &mut context,
                            &placed_labels_by_edge_id,
                        )?;
                        if label_center.is_some() {
                            break;
                        }
                    }
                }

                if pass == 2 && label_center.is_none() {
                    for segment_candidate in &segments {
                        label_center = label_placement_with_extended_detour(
                            data,
                            edge_index,
                            label_node_index,
                            segment_candidate,
                            &mut context,
                            &placed_labels_by_edge_id,
                        )?;
                        if label_center.is_some() {
                            break;
                        }
                    }
                }

                let Some(label_center) = label_center else {
                    return Err(LabelError::PlacementExhausted(Box::new(PlacementFailure {
                        edge_id: data.edges[edge_index].id.clone(),
                        label_node_id: data.nodes[label_node_index].id.clone(),
                    })));
                };

                data.nodes[label_node_index].x = Some(label_center.x);
                data.nodes[label_node_index].y = Some(label_center.y);
                let placed_label = PlacedLabel {
                    edge_id: data.edges[edge_index].id.clone(),
                    node_id: data.nodes[label_node_index].id.clone(),
                    center: label_center,
                    rect: label_rect_at(&data.nodes[label_node_index], label_center),
                };
                placed_labels_by_edge_id.set(placed_label.clone());
                upsert_placed_label_obstacle(&mut context, &placed_label);
            }

            // Once every label has a provisional home, freeze the complete reservation set before
            // rerouting.
            let frozen_labels = placed_labels_by_edge_id.values();
            increment_metric(
                &metrics,
                LabelMetric::FrozenReservations,
                frozen_labels.len(),
            );
            if let Some(routing) = routing_instrumentation.as_mut() {
                routing.label_overlay_builds += 1;
                routing.label_overlay_vertices += frozen_labels.len() * 4;
            }

            // The placement search is provisional: capture its route proposals, roll the routes
            // back to the pass boundary, and only then commit each impacted edge once against the
            // complete frozen reservation set.
            for (edge_index, pass_start) in pass_start_points.iter().enumerate() {
                let pass_start = pass_start.clone();
                let proposed = points_for_edge(data, edge_index, None, &context);
                if proposed != pass_start {
                    proposed_owner_routes.push((
                        edge_index,
                        proposed.iter().copied().map(clone_point).collect(),
                    ));
                }
                set_edge_points(data, &mut context, edge_index, &pass_start);
            }
            for (edge_index, points) in proposed_owner_routes.clone() {
                let edge_id = data.edges[edge_index].id.clone();
                if !placed_labels_by_edge_id.has(&edge_id) {
                    continue;
                }
                set_edge_points(data, &mut context, edge_index, &points);
            }

            let mut impacted: HashSet<String> = proposed_owner_routes
                .iter()
                .map(|(edge_index, _)| data.edges[*edge_index].id.clone())
                .collect();
            for edge_index in 0..data.edges.len() {
                let edge_id = data.edges[edge_index].id.clone();
                let own_label = placed_labels_by_edge_id.has(&edge_id);
                let points = points_for_edge(data, edge_index, None, &context);
                for reservation in &frozen_labels {
                    if reservation.edge_id != edge_id
                        && polyline_intersects_rect(&points, reservation.rect)
                    {
                        impacted.insert(edge_id.clone());
                    }
                }
                if own_label && !label_still_anchored(&edge_id, &points, &placed_labels_by_edge_id)
                {
                    impacted.insert(edge_id.clone());
                }
            }

            let mut reroute_lane_ordinals: HashMap<String, usize> = HashMap::new();
            for edge_index in 0..data.edges.len() {
                let edge_id = data.edges[edge_index].id.clone();
                if !impacted.contains(&edge_id) {
                    continue;
                }
                let mut points = points_for_edge(data, edge_index, None, &context);
                let mut overrides = EdgePointOverrides::default();
                overrides.set(&edge_id, points.clone());
                let blocking_reservations: Vec<PlacedLabel> = frozen_labels
                    .iter()
                    .filter(|reservation| {
                        reservation.edge_id != edge_id
                            && polyline_intersects_rect(&points, reservation.rect)
                    })
                    .cloned()
                    .collect();
                if !blocking_reservations.is_empty() {
                    let first = blocking_reservations[0].rect;
                    let combined_rect =
                        blocking_reservations[1..]
                            .iter()
                            .fold(first, |combined, reservation| Rect {
                                cx: (combined.left.min(reservation.rect.left)
                                    + combined.right.max(reservation.rect.right))
                                    / 2.0,
                                cy: (combined.top.min(reservation.rect.top)
                                    + combined.bottom.max(reservation.rect.bottom))
                                    / 2.0,
                                left: combined.left.min(reservation.rect.left),
                                right: combined.right.max(reservation.rect.right),
                                top: combined.top.min(reservation.rect.top),
                                bottom: combined.bottom.max(reservation.rect.bottom),
                            });
                    let lane = reroute_lane(&points, combined_rect, &blocking_reservations);
                    let reroute_ordinal = lane
                        .as_ref()
                        .and_then(|lane| reroute_lane_ordinals.get(&lane.key).copied())
                        .unwrap_or(0);
                    // Leave one full separation lane beyond the 6 px label clearance; only routes
                    // avoiding the same reservation set on the same side share an ordinal.
                    let lane_padding = (reroute_ordinal + 2) as f64 * 12.0;
                    let blocked_rect = Rect {
                        cx: combined_rect.cx,
                        cy: combined_rect.cy,
                        left: combined_rect.left - lane_padding,
                        right: combined_rect.right + lane_padding,
                        top: combined_rect.top - lane_padding,
                        bottom: combined_rect.bottom + lane_padding,
                    };
                    let mut rerouted = reroute_foreign_edge_around_rect(
                        data,
                        edge_index,
                        blocked_rect,
                        &mut context,
                        &placed_labels_by_edge_id,
                        &mut overrides,
                        lane.as_ref(),
                    )?;
                    let used_scoped_lane = rerouted.is_some();
                    if rerouted.is_none() {
                        rerouted = reroute_foreign_edge_around_labels(
                            data,
                            edge_index,
                            &blocking_reservations,
                            &mut context,
                            &placed_labels_by_edge_id,
                            &mut overrides,
                        )?;
                    }
                    if let Some(rerouted) = rerouted {
                        points = rerouted;
                        overrides.set(&edge_id, points.clone());
                        if let Some(lane) = lane
                            && used_scoped_lane
                        {
                            reroute_lane_ordinals.insert(lane.key, reroute_ordinal + 1);
                        }
                    } else {
                        // Preserving a valid edge and anchored labels is safer than failing the
                        // whole layout. Record the unavoidable overlap so final validation permits
                        // only this edge-label pair.
                        increment_metric(&metrics, LabelMetric::LabelOverlapFallbacks, 1);
                        for reservation in &blocking_reservations {
                            if !polyline_intersects_rect(&points, reservation.rect) {
                                continue;
                            }
                            let allowed_edges = allowed_overlaps_by_label_edge_id
                                .entry(reservation.edge_id.clone())
                                .or_default();
                            if !allowed_edges.contains(&edge_id) {
                                increment_metric(
                                    &metrics,
                                    LabelMetric::LabelOverlapReservations,
                                    1,
                                );
                            }
                            allowed_edges.insert(edge_id.clone());
                        }
                    }
                }
                if !label_still_anchored(&edge_id, &points, &placed_labels_by_edge_id) {
                    return Err(LabelError::Grid(grid_error(
                        GridErrorCode::RouteNotFound,
                        format!("Label anchor was lost for edge \"{edge_id}\""),
                        Some(json!({ "edgeId": edge_id, "pass": pass })),
                    )));
                }
                if placed_labels_by_edge_id.has(&edge_id) {
                    increment_metric(&metrics, LabelMetric::PreservedAnchors, 1);
                }
                set_edge_points(data, &mut context, edge_index, &points);
                increment_metric(&metrics, LabelMetric::ImpactedEdgeReroutes, 1);
            }
            for item in &work_items {
                let edge_id = data.edges[item.edge_index].id.clone();
                let Some(reservation) = placed_labels_by_edge_id.get(&edge_id).cloned() else {
                    return Err(LabelError::Grid(grid_error(
                        GridErrorCode::RouteNotFound,
                        format!("Label pass {pass} did not validate"),
                        Some(json!({ "edgeId": edge_id, "pass": pass })),
                    )));
                };
                let anchored = label_still_anchored(
                    &edge_id,
                    &points_for_edge(data, item.edge_index, None, &context),
                    &placed_labels_by_edge_id,
                );
                let allowed = allowed_overlaps_by_label_edge_id.get(&edge_id);
                let safe = rect_safe_for_label(
                    data,
                    reservation.rect,
                    item.edge_index,
                    &data.nodes[item.label_node_index].id,
                    &mut context,
                    None,
                    allowed,
                )?;
                if !anchored || !safe {
                    return Err(LabelError::Grid(grid_error(
                        GridErrorCode::RouteNotFound,
                        format!("Label pass {pass} did not validate"),
                        Some(json!({ "edgeId": edge_id, "pass": pass })),
                    )));
                }
            }
            Ok(())
        })();

        match outcome {
            Ok(()) => return Ok(()),
            Err(LabelError::WorkLimit) => {
                place_labels_at_route_midpoints(data);
                increment_metric(&metrics, LabelMetric::IndexWorkLimitFallbacks, 1);
                if let Some(routing) = routing_instrumentation.as_mut() {
                    routing.resource_limit_fallbacks += 1;
                    routing.increment_reason(GridRoutingFallbackReason::LabelIndexWorkCap);
                }
                return Ok(());
            }
            Err(LabelError::PlacementExhausted(failure)) => {
                placement_failures.push(*failure);
                // A failed pass may contain detours around provisional label positions. Restore
                // routing before retrying so the next pass depends only on the labels it places.
                restore_base_edge_points(data, &base_edge_points);
                for item in &work_items {
                    data.nodes[item.label_node_index].x = None;
                    data.nodes[item.label_node_index].y = None;
                }
            }
            Err(LabelError::Grid(error)) => {
                restore_base_state(data);
                increment_metric(&metrics, LabelMetric::Rollbacks, 1);
                return Err(error);
            }
        }
    }

    place_labels_at_route_midpoints(data);
    increment_metric(&metrics, LabelMetric::PlacementSearchFallbacks, 1);
    if let Some(routing) = routing_instrumentation.as_mut() {
        routing.increment_reason(GridRoutingFallbackReason::LabelPlacementExhausted);
    }
    let _ = placement_failures;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::grid::types::GridConfig;
    use crate::layout::grid::types::GridCurve;

    fn node(id: &str) -> Node {
        Node {
            id: id.to_string(),
            width: Some(80.0),
            height: Some(40.0),
            shape: Some("rect".to_string()),
            ..Node::default()
        }
    }

    fn manual_node(id: &str, x: f64, y: f64, width: f64, height: f64) -> Node {
        Node {
            id: id.to_string(),
            x: Some(x),
            y: Some(y),
            width: Some(width),
            height: Some(height),
            shape: Some("rect".to_string()),
            ..Node::default()
        }
    }

    fn edge(id: &str, start: &str, end: &str, label: Option<&str>) -> Edge {
        Edge {
            id: id.to_string(),
            start: Some(start.to_string()),
            end: Some(end.to_string()),
            label: label.map(str::to_string),
            label_style: Some(vec!["fill:red".to_string()]),
            ..Edge::default()
        }
    }

    fn manual_edge(
        id: &str,
        start: &str,
        end: &str,
        points: &[(f64, f64)],
        label: Option<&str>,
    ) -> Edge {
        Edge {
            id: id.to_string(),
            start: Some(start.to_string()),
            end: Some(end.to_string()),
            points: points
                .iter()
                .map(|(x, y)| RouterPoint::new(*x, *y))
                .collect(),
            label: label.map(str::to_string),
            arrow_type_start: Some("none".to_string()),
            arrow_type_end: Some("arrow_point".to_string()),
            edge_type: Some("arrow_point".to_string()),
            curve: Some(GridCurve::Linear),
            ..Edge::default()
        }
    }

    fn data(nodes: Vec<Node>, edges: Vec<Edge>) -> LayoutData {
        LayoutData {
            nodes,
            edges,
            config: GridConfig::default(),
        }
    }

    fn label_node(data: &LayoutData, edge_index: usize) -> &Node {
        let id = data.edges[edge_index]
            .label_node_id
            .as_deref()
            .expect("label node id");
        data.nodes
            .iter()
            .find(|node| node.id == id)
            .expect("label node")
    }

    fn set_label_size(data: &mut LayoutData, edge_index: usize, width: f64, height: f64) {
        let id = data.edges[edge_index]
            .label_node_id
            .clone()
            .expect("label node id");
        let node = data
            .nodes
            .iter_mut()
            .find(|node| node.id == id)
            .expect("label node");
        node.width = Some(width);
        node.height = Some(height);
    }

    #[test]
    fn creates_measurable_label_nodes_without_duplicate_edge_labels() {
        let mut data = data(
            vec![node("a"), node("b")],
            vec![edge("e1", "a", "b", Some("hello"))],
        );

        prepare_grid_layout(&mut data);

        assert!(data.edges[0].label.is_none());
        let label_id = data.edges[0].label_node_id.clone().expect("label node id");
        let helper = data
            .nodes
            .iter()
            .find(|node| node.id == label_id)
            .expect("helper");
        assert!(helper.is_edge_label);
        assert_eq!(helper.label.as_deref(), Some("hello"));
        assert_eq!(helper.shape.as_deref(), Some("labelRect"));
    }

    #[test]
    fn reuses_existing_helpers_and_avoids_helper_shaped_user_ids() {
        let existing_helper_id = "edge-label-a-b-prepared";
        let mut data = data(
            vec![
                node("a"),
                node("b"),
                node("edge-label-user-node"),
                Node {
                    id: existing_helper_id.to_string(),
                    label: Some("existing".to_string()),
                    edge_start: Some("a".to_string()),
                    edge_end: Some("b".to_string()),
                    shape: Some("labelRect".to_string()),
                    width: Some(0.0),
                    height: Some(0.0),
                    is_edge_label: true,
                    is_dummy: true,
                    ..Node::default()
                },
            ],
            vec![
                Edge {
                    label_node_id: Some(existing_helper_id.to_string()),
                    ..edge("prepared", "a", "b", Some("updated"))
                },
                edge("fresh", "a", "edge-label-user-node", Some("new label")),
            ],
        );

        prepare_grid_layout(&mut data);

        assert_eq!(
            data.edges[0].label_node_id.as_deref(),
            Some(existing_helper_id)
        );
        assert!(data.edges[0].label.is_none());
        assert_eq!(
            data.nodes
                .iter()
                .find(|node| node.id == existing_helper_id)
                .and_then(|node| node.label.as_deref()),
            Some("updated")
        );
        let fresh_id = data.edges[1].label_node_id.clone().expect("fresh id");
        assert_ne!(fresh_id, "edge-label-user-node");
        assert_eq!(
            data.nodes.iter().filter(|node| node.is_edge_label).count(),
            2
        );
    }

    #[test]
    fn places_a_fitting_label_on_its_existing_segment_without_a_routing_pass() {
        let mut data = data(
            vec![
                manual_node("a", 40.0, 100.0, 20.0, 20.0),
                manual_node("b", 360.0, 100.0, 20.0, 20.0),
            ],
            vec![manual_edge(
                "labelled",
                "a",
                "b",
                &[(50.0, 100.0), (350.0, 100.0)],
                Some("fits"),
            )],
        );
        prepare_grid_layout(&mut data);
        set_label_size(&mut data, 0, 40.0, 20.0);
        let original_points = data.edges[0].points.clone();
        let mut metrics = GridEdgeLabelInstrumentation::default();
        let mut routing = GridRoutingInstrumentation::default();

        position_grid_edge_labels(
            &mut data,
            Some(&mut metrics),
            Some(&mut routing),
            &GridEdgeLabelOptions::default(),
        )
        .expect("labels");

        assert_eq!(data.edges[0].points, original_points);
        assert_eq!(label_node(&data, 0).x, Some(200.0));
        assert_eq!(label_node(&data, 0).y, Some(100.0));
        assert_eq!(metrics.label_passes, 1);
        assert_eq!(metrics.frozen_reservations, 1);
        assert_eq!(metrics.impacted_edge_reroutes, 0);
        assert_eq!(routing.label_overlay_builds, 1);
        assert_eq!(routing.label_overlay_vertices, 4);
    }

    #[test]
    fn ignores_stale_label_positions_from_a_previous_layout() {
        let create_data = || {
            let mut data = data(
                vec![
                    manual_node("top-start", 40.0, 100.0, 20.0, 20.0),
                    manual_node("top-end", 360.0, 100.0, 20.0, 20.0),
                    manual_node("bottom-start", 40.0, 200.0, 20.0, 20.0),
                    manual_node("bottom-end", 360.0, 200.0, 20.0, 20.0),
                ],
                vec![
                    manual_edge(
                        "top",
                        "top-start",
                        "top-end",
                        &[(50.0, 100.0), (350.0, 100.0)],
                        Some("top label"),
                    ),
                    manual_edge(
                        "bottom",
                        "bottom-start",
                        "bottom-end",
                        &[(50.0, 200.0), (350.0, 200.0)],
                        Some("bottom label"),
                    ),
                ],
            );
            prepare_grid_layout(&mut data);
            set_label_size(&mut data, 0, 80.0, 20.0);
            set_label_size(&mut data, 1, 80.0, 20.0);
            data
        };
        let mut fresh = create_data();
        let mut reused = create_data();
        {
            let id = reused.edges[1].label_node_id.clone().expect("id");
            let stale = reused.nodes.iter_mut().find(|node| node.id == id).unwrap();
            stale.x = Some(200.0);
            stale.y = Some(100.0);
        }

        position_grid_edge_labels(&mut fresh, None, None, &GridEdgeLabelOptions::default())
            .expect("fresh");
        position_grid_edge_labels(&mut reused, None, None, &GridEdgeLabelOptions::default())
            .expect("reused");

        assert_eq!(reused.edges[0].points, fresh.edges[0].points);
        assert_eq!(reused.edges[1].points, fresh.edges[1].points);
        for edge_index in 0..2 {
            assert_eq!(
                label_node(&reused, edge_index).x,
                label_node(&fresh, edge_index).x
            );
            assert_eq!(
                label_node(&reused, edge_index).y,
                label_node(&fresh, edge_index).y
            );
        }
    }

    #[test]
    fn parks_degenerate_route_labels_without_aborting_valid_placement() {
        let mut data = data(
            vec![
                manual_node("empty-start", 40.0, 100.0, 80.0, 40.0),
                manual_node("empty-end", 360.0, 100.0, 80.0, 40.0),
                manual_node("point-start", 40.0, 200.0, 80.0, 40.0),
                manual_node("point-end", 360.0, 200.0, 80.0, 40.0),
                manual_node("valid-start", 40.0, 300.0, 80.0, 40.0),
                manual_node("valid-end", 360.0, 300.0, 80.0, 40.0),
            ],
            vec![
                manual_edge(
                    "empty",
                    "empty-start",
                    "empty-end",
                    &[],
                    Some("empty route"),
                ),
                manual_edge(
                    "point",
                    "point-start",
                    "point-end",
                    &[(200.0, 200.0), (200.0, 200.0)],
                    Some("point route"),
                ),
                manual_edge(
                    "valid",
                    "valid-start",
                    "valid-end",
                    &[(80.0, 300.0), (320.0, 300.0)],
                    Some("valid route"),
                ),
            ],
        );
        prepare_grid_layout(&mut data);
        for edge_index in 0..3 {
            set_label_size(&mut data, edge_index, 70.0, 20.0);
        }
        let mut metrics = GridEdgeLabelInstrumentation::default();

        position_grid_edge_labels(
            &mut data,
            Some(&mut metrics),
            None,
            &GridEdgeLabelOptions::default(),
        )
        .expect("labels");

        assert_eq!(
            (label_node(&data, 0).x, label_node(&data, 0).y),
            (Some(40.0), Some(100.0))
        );
        assert_eq!(
            (label_node(&data, 1).x, label_node(&data, 1).y),
            (Some(200.0), Some(200.0))
        );
        assert_eq!(
            (label_node(&data, 2).x, label_node(&data, 2).y),
            (Some(200.0), Some(300.0))
        );
        assert!(data.edges[0].points.is_empty());
        assert_eq!(data.edges[1].points.len(), 2);
        assert_eq!(metrics.degenerate_label_routes_skipped, 2);
        assert_eq!(metrics.label_passes, 1);
    }

    #[test]
    fn counts_labels_skipped_before_measurement() {
        let mut data = data(
            vec![
                manual_node("a", 40.0, 100.0, 80.0, 40.0),
                manual_node("b", 360.0, 100.0, 80.0, 40.0),
            ],
            vec![manual_edge(
                "unmeasured",
                "a",
                "b",
                &[(80.0, 100.0), (320.0, 100.0)],
                Some("waiting for measurement"),
            )],
        );
        prepare_grid_layout(&mut data);
        let mut metrics = GridEdgeLabelInstrumentation::default();

        position_grid_edge_labels(
            &mut data,
            Some(&mut metrics),
            None,
            &GridEdgeLabelOptions::default(),
        )
        .expect("labels");

        let label = label_node(&data, 0);
        assert_eq!((label.width, label.height), (Some(0.0), Some(0.0)));
        assert_eq!((label.x, label.y), (None, None));
        assert_eq!(metrics.unmeasured_labels_skipped, 1);
        assert_eq!(metrics.label_passes, 0);
    }

    #[test]
    fn falls_back_to_anchored_midpoints_when_the_index_work_cap_is_exhausted() {
        let mut data = data(
            vec![
                manual_node("a", 40.0, 100.0, 80.0, 40.0),
                manual_node("b", 360.0, 100.0, 80.0, 40.0),
            ],
            vec![manual_edge(
                "capped",
                "a",
                "b",
                &[(80.0, 100.0), (320.0, 100.0)],
                Some("bounded work"),
            )],
        );
        prepare_grid_layout(&mut data);
        set_label_size(&mut data, 0, 80.0, 20.0);
        let original_points = data.edges[0].points.clone();
        let mut metrics = GridEdgeLabelInstrumentation::default();
        let mut routing = GridRoutingInstrumentation::default();

        position_grid_edge_labels(
            &mut data,
            Some(&mut metrics),
            Some(&mut routing),
            &GridEdgeLabelOptions {
                max_index_work: Some(0.0),
            },
        )
        .expect("labels");

        assert_eq!(data.edges[0].points, original_points);
        assert_eq!(
            (label_node(&data, 0).x, label_node(&data, 0).y),
            (Some(200.0), Some(100.0))
        );
        assert_eq!(metrics.label_passes, 1);
        assert_eq!(metrics.index_work_units, 0);
        assert_eq!(metrics.index_work_limit_fallbacks, 1);
        assert_eq!(routing.resource_limit_fallbacks, 1);
        assert_eq!(
            routing.reason_count(GridRoutingFallbackReason::LabelIndexWorkCap),
            1
        );
    }

    #[test]
    fn reroutes_an_owning_edge_while_preserving_its_frozen_label_anchor() {
        let mut data = data(
            vec![
                manual_node("left", 40.0, 160.0, 20.0, 20.0),
                manual_node("right", 460.0, 160.0, 20.0, 20.0),
                manual_node("top-1", 220.0, 40.0, 20.0, 20.0),
                manual_node("bottom-1", 220.0, 280.0, 20.0, 20.0),
                manual_node("top-2", 280.0, 40.0, 20.0, 20.0),
                manual_node("bottom-2", 280.0, 280.0, 20.0, 20.0),
                manual_node("owner-blocker", 250.0, 160.0, 70.0, 70.0),
            ],
            vec![
                manual_edge(
                    "owner",
                    "left",
                    "right",
                    &[(50.0, 160.0), (450.0, 160.0)],
                    Some("owner label"),
                ),
                manual_edge(
                    "foreign-1",
                    "top-1",
                    "bottom-1",
                    &[(220.0, 50.0), (220.0, 270.0)],
                    None,
                ),
                manual_edge(
                    "foreign-2",
                    "top-2",
                    "bottom-2",
                    &[(280.0, 50.0), (280.0, 270.0)],
                    None,
                ),
            ],
        );
        prepare_grid_layout(&mut data);
        set_label_size(&mut data, 0, 300.0, 28.0);
        let mut metrics = GridEdgeLabelInstrumentation::default();
        let mut routing = GridRoutingInstrumentation::default();

        position_grid_edge_labels(
            &mut data,
            Some(&mut metrics),
            Some(&mut routing),
            &GridEdgeLabelOptions::default(),
        )
        .expect("labels");

        assert_eq!(data.edges[1].points.len(), 2);
        assert_eq!(data.edges[2].points.len(), 2);
        assert!(data.edges[0].points.len() > 2);
        assert_eq!(metrics.label_passes, 2);
        assert!(metrics.index_work_units > 0);
        assert_eq!(metrics.index_work_limit_fallbacks, 0);
        assert_eq!(routing.label_overlay_builds, 1);
        assert_eq!(routing.label_overlay_vertices, 4);
        assert_eq!(metrics.frozen_reservations, 1);
        assert!(metrics.impacted_edge_reroutes >= 1);
        assert!(metrics.preserved_anchors > 0);
        assert!(metrics.reroute_candidates_evaluated > 0);
        assert!(
            metrics.reroute_rejected_blocked_rect
                + metrics.reroute_rejected_protected_obstacle
                + metrics.reroute_rejected_label_anchor
                <= metrics.reroute_candidates_evaluated
        );
        assert!(metrics.reroute_rejected_protected_obstacle > 0);
    }

    #[test]
    fn detours_a_label_around_tall_endpoints_when_the_direct_gap_is_too_short() {
        let mut data = data(
            vec![
                manual_node("source", 260.0, 240.0, 220.0, 260.0),
                manual_node("target", 523.0, 240.0, 180.0, 220.0),
            ],
            vec![manual_edge(
                "narrow-gap",
                "source",
                "target",
                &[(370.0, 240.0), (433.0, 240.0)],
                Some("CWM"),
            )],
        );
        prepare_grid_layout(&mut data);
        set_label_size(&mut data, 0, 66.0, 39.0);

        position_grid_edge_labels(&mut data, None, None, &GridEdgeLabelOptions::default())
            .expect("labels");

        assert!(data.edges[0].points.len() > 2);
        let label = label_node(&data, 0);
        assert!(label.x.is_some());
        assert!(label.y.is_some());
        assert!((label.y.unwrap() - 240.0).abs() > 100.0);
    }

    #[test]
    fn produces_the_same_geometry_when_reused_layout_data_contains_stale_label_positions() {
        let create_data = || {
            let mut data = data(
                vec![
                    Node {
                        metadata: Some(json!({ "row": 1, "column": 1 })),
                        ..node("a")
                    },
                    Node {
                        metadata: Some(json!({ "row": 1, "column": 3 })),
                        ..node("b")
                    },
                    Node {
                        metadata: Some(json!({ "row": 2, "column": 1 })),
                        ..node("c")
                    },
                    Node {
                        metadata: Some(json!({ "row": 2, "column": 3 })),
                        ..node("d")
                    },
                ],
                vec![
                    edge("top", "a", "b", Some("top label")),
                    edge("bottom", "c", "d", Some("bottom label")),
                ],
            );
            prepare_grid_layout(&mut data);
            set_label_size(&mut data, 0, 80.0, 20.0);
            set_label_size(&mut data, 1, 80.0, 20.0);
            data
        };
        let mut fresh = create_data();
        let mut reused = create_data();
        let options = || crate::layout::grid::layout_core::GridLayoutCoreInternalOptions::default();
        crate::layout::grid::layout_core::run_grid_layout_core(&mut fresh, options())
            .expect("fresh");
        crate::layout::grid::layout_core::run_grid_layout_core(&mut reused, options())
            .expect("reused");
        let top = label_node(&reused, 0).clone();
        {
            let id = reused.edges[1].label_node_id.clone().expect("bottom label");
            let bottom = reused.nodes.iter_mut().find(|node| node.id == id).unwrap();
            bottom.x = top.x;
            bottom.y = top.y;
        }
        crate::layout::grid::layout_core::run_grid_layout_core(&mut reused, options())
            .expect("reused again");

        for (fresh_node, reused_node) in fresh.nodes.iter().zip(reused.nodes.iter()) {
            assert_eq!(
                (
                    fresh_node.x,
                    fresh_node.y,
                    fresh_node.width,
                    fresh_node.height
                ),
                (
                    reused_node.x,
                    reused_node.y,
                    reused_node.width,
                    reused_node.height
                )
            );
        }
        for (fresh_edge, reused_edge) in fresh.edges.iter().zip(reused.edges.iter()) {
            assert_eq!(fresh_edge.points, reused_edge.points);
        }
    }

    #[test]
    fn uses_the_same_labelled_self_loop_geometry_during_resource_fallback() {
        let mut data = data(
            vec![node("a")],
            vec![manual_edge("loop", "a", "a", &[], Some("wide loop label"))],
        );
        prepare_grid_layout(&mut data);
        set_label_size(&mut data, 0, 90.0, 20.0);
        let mut metrics = GridRoutingInstrumentation::default();

        crate::layout::grid::layout_core::run_grid_layout_core(
            &mut data,
            crate::layout::grid::layout_core::GridLayoutCoreInternalOptions {
                metrics: Some(&mut metrics),
                routing: crate::layout::grid::router::GridRoutingTestOptions {
                    options: crate::layout::grid::router::GridRoutingOptions {
                        topology_caps: Some(
                            crate::layout::grid::router_topology::TopologyResourceCaps {
                                max_vertices: Some(1),
                                ..Default::default()
                            },
                        ),
                        search_caps: None,
                    },
                    on_dual_route_comparison: None,
                },
            },
        )
        .expect("grid layout");

        assert_eq!(normalize_polyline(&data.edges[0].points).segments.len(), 7);
        assert!(!polyline_has_self_conflict(&data.edges[0].points));
        let label = label_node(&data, 0);
        assert!(label.x.unwrap() + label.width.unwrap() / 2.0 <= data.edges[0].points[1].x - 6.0);
        assert_eq!(metrics.resource_limit_fallbacks, 1);
        assert_eq!(metrics.fallback_validation_failures, 0);
        assert_eq!(metrics.label_aware_self_loop_candidates, 1);
        assert_eq!(metrics.label_aware_self_loops_committed, 1);
        assert_eq!(metrics.label_aware_self_loop_fallbacks, 0);
    }

    #[test]
    fn expands_an_adjacent_boundary_so_a_measured_label_stays_on_the_direct_route() {
        let mut data = data(
            vec![node("a"), node("b")],
            vec![manual_edge("labelled", "a", "b", &[], Some("long label"))],
        );
        prepare_grid_layout(&mut data);
        set_label_size(&mut data, 0, 90.0, 20.0);

        crate::layout::grid::layout_core::run_grid_layout_core(
            &mut data,
            crate::layout::grid::layout_core::GridLayoutCoreInternalOptions::default(),
        )
        .expect("grid layout");

        let (a, b) = (&data.nodes[0], &data.nodes[1]);
        assert_eq!(
            (b.x.unwrap() - b.width.unwrap() / 2.0) - (a.x.unwrap() + a.width.unwrap() / 2.0),
            116.0
        );
        assert_eq!(normalize_polyline(&data.edges[0].points).segments.len(), 1);
        let label = label_node(&data, 0);
        assert_eq!((label.x, label.y), (Some(138.0), Some(20.0)));
    }

    #[test]
    fn keeps_a_labelled_self_loop_from_overlapping_its_own_route() {
        let mut data = data(
            vec![node("a")],
            vec![manual_edge("loop", "a", "a", &[], Some("wide loop label"))],
        );
        prepare_grid_layout(&mut data);
        set_label_size(&mut data, 0, 90.0, 20.0);
        let mut metrics = GridRoutingInstrumentation::default();

        crate::layout::grid::layout_core::run_grid_layout_core(
            &mut data,
            crate::layout::grid::layout_core::GridLayoutCoreInternalOptions {
                metrics: Some(&mut metrics),
                ..Default::default()
            },
        )
        .expect("grid layout");

        let segments = normalize_polyline(&data.edges[0].points).segments;
        assert_eq!(segments.len(), 7);
        assert!(!polyline_has_self_conflict(&data.edges[0].points));
        let label = label_node(&data, 0);
        assert!(label.x.unwrap() < data.nodes[0].x.unwrap() - data.nodes[0].width.unwrap() / 2.0);
        assert_eq!(label.y, Some(20.0));
        assert!(label.x.unwrap() + label.width.unwrap() / 2.0 <= data.edges[0].points[1].x - 6.0);
        assert_eq!(metrics.label_aware_self_loop_candidates, 1);
        assert_eq!(metrics.label_aware_self_loops_committed, 1);
        assert_eq!(metrics.label_aware_self_loop_fallbacks, 0);
    }

    #[test]
    fn reroutes_crossing_vertical_edges_around_a_horizontal_edge_label() {
        let mut data = data(
            vec![
                manual_node("start", 50.0, 150.0, 80.0, 40.0),
                manual_node("target", 450.0, 150.0, 80.0, 40.0),
                manual_node("top-1", 150.0, 50.0, 20.0, 20.0),
                manual_node("bottom-1", 150.0, 250.0, 20.0, 20.0),
                manual_node("top-2", 200.0, 50.0, 20.0, 20.0),
                manual_node("bottom-2", 200.0, 250.0, 20.0, 20.0),
                manual_node("top-3", 250.0, 50.0, 20.0, 20.0),
                manual_node("bottom-3", 250.0, 250.0, 20.0, 20.0),
                manual_node("top-4", 300.0, 50.0, 20.0, 20.0),
                manual_node("bottom-4", 300.0, 250.0, 20.0, 20.0),
                manual_node("top-5", 350.0, 50.0, 20.0, 20.0),
                manual_node("bottom-5", 350.0, 250.0, 20.0, 20.0),
            ],
            vec![
                manual_edge(
                    "labelled-horizontal",
                    "start",
                    "target",
                    &[(90.0, 150.0), (410.0, 150.0)],
                    Some("g1"),
                ),
                manual_edge(
                    "v-1",
                    "top-1",
                    "bottom-1",
                    &[(150.0, 60.0), (150.0, 240.0)],
                    None,
                ),
                manual_edge(
                    "v-2",
                    "top-2",
                    "bottom-2",
                    &[(200.0, 60.0), (200.0, 240.0)],
                    None,
                ),
                manual_edge(
                    "v-3",
                    "top-3",
                    "bottom-3",
                    &[(250.0, 60.0), (250.0, 240.0)],
                    None,
                ),
                manual_edge(
                    "v-4",
                    "top-4",
                    "bottom-4",
                    &[(300.0, 60.0), (300.0, 240.0)],
                    None,
                ),
                manual_edge(
                    "v-5",
                    "top-5",
                    "bottom-5",
                    &[(350.0, 60.0), (350.0, 240.0)],
                    None,
                ),
            ],
        );
        prepare_grid_layout(&mut data);
        set_label_size(&mut data, 0, 180.0, 20.0);
        let mut metrics = GridEdgeLabelInstrumentation::default();

        position_grid_edge_labels(
            &mut data,
            Some(&mut metrics),
            None,
            &GridEdgeLabelOptions::default(),
        )
        .expect("labels");

        assert!(
            data.edges
                .iter()
                .filter(|edge| edge.id.starts_with("v-"))
                .any(|edge| edge.points.len() > 2)
        );
        assert!(metrics.impacted_edge_reroutes > 1);
        assert!(metrics.reroute_candidates_evaluated > 0);
        let label = label_node(&data, 0);
        assert!(label.x.is_some());
        assert!(label.y.is_some());
    }

    #[test]
    fn keeps_a_fourteen_pixel_side_gutter_when_rerouting_around_a_horizontal_label() {
        let mut data = data(
            vec![
                manual_node("owner-start", 100.0, 120.0, 20.0, 20.0),
                manual_node("owner-end", 300.0, 180.0, 20.0, 20.0),
                manual_node("crossing-start", 200.0, 0.0, 20.0, 20.0),
                manual_node("crossing-end", 200.0, 300.0, 20.0, 20.0),
                manual_node("nearby-obstacle", 138.0, 80.0, 8.0, 8.0),
            ],
            vec![
                manual_edge(
                    "owner",
                    "owner-start",
                    "owner-end",
                    &[
                        (100.0, 130.0),
                        (100.0, 150.0),
                        (300.0, 150.0),
                        (300.0, 170.0),
                    ],
                    Some("label"),
                ),
                manual_edge(
                    "crossing",
                    "crossing-start",
                    "crossing-end",
                    &[(200.0, 10.0), (200.0, 290.0)],
                    None,
                ),
            ],
        );
        prepare_grid_layout(&mut data);
        set_label_size(&mut data, 0, 120.0, 20.0);
        let original_crossing = data.edges[1].points.clone();

        position_grid_edge_labels(&mut data, None, None, &GridEdgeLabelOptions::default())
            .expect("labels");

        assert_ne!(data.edges[1].points, original_crossing);
        let label = label_node(&data, 0).clone();
        let rect = rect_for_node(&label);
        let gaps: Vec<f64> = normalize_polyline(&data.edges[1].points)
            .segments
            .into_iter()
            .filter(|segment| {
                segment.orientation == SegmentOrientation::V
                    && segment.a.y.min(segment.b.y) < rect.bottom
                    && segment.a.y.max(segment.b.y) > rect.top
            })
            .map(|segment| {
                (rect.left - segment.a.x)
                    .max(segment.a.x - rect.right)
                    .max(0.0)
            })
            .collect();
        assert!(!gaps.is_empty());
        assert!(gaps.iter().copied().fold(f64::INFINITY, f64::min) >= 14.0);
    }

    #[test]
    fn considers_terminal_dogleg_segments_when_the_interior_segment_is_too_short() {
        let mut data = data(
            vec![
                manual_node("start", 80.0, 120.0, 40.0, 40.0),
                manual_node("end", 520.0, 180.0, 40.0, 40.0),
            ],
            vec![manual_edge(
                "dogleg-label",
                "start",
                "end",
                &[
                    (100.0, 120.0),
                    (320.0, 120.0),
                    (320.0, 180.0),
                    (500.0, 180.0),
                ],
                Some("terminal segment"),
            )],
        );
        prepare_grid_layout(&mut data);
        set_label_size(&mut data, 0, 140.0, 78.0);

        position_grid_edge_labels(&mut data, None, None, &GridEdgeLabelOptions::default())
            .expect("labels");

        let label = label_node(&data, 0);
        assert!(label.x.is_some());
        assert!(label.y.is_some());
        assert!((label.x.unwrap() - 320.0).abs() > 0.5);
    }
}
