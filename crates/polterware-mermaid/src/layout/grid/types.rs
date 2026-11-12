//! Port of Mermaid 12.3.0's
//! `rendering-util/layout-algorithms/grid/types.ts`.
//!
//! The TypeScript source describes a `Node`/`Edge` graph with object references. This port owns
//! the graph in [`LayoutData`] and refers to nodes by index so that the geometry passes can
//! borrow the node list mutably while the forest and placement results stay readable. The
//! `*_index` and `item` fields are those indices.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::layout::grid::label_spacing::GridLabelRequirements;
use crate::layout::grid::router_instrumentation::GridRoutingInstrumentation;

// The root participates in the same container algorithms as groups but has no backing Node.
pub const ROOT_CONTAINER_ID: &str = "__grid_root__";
// Label helpers belong to edges and must not consume cells in the node-placement forest.
pub const GRID_LABEL_PREFIX: &str = "edge-label-";

pub type GridContainerId = String;
pub type RouterVertexId = usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GridHorizontalAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GridVerticalAlign {
    Top,
    Center,
    Bottom,
}

impl GridHorizontalAlign {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
        }
    }
}

impl GridVerticalAlign {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Center => "center",
            Self::Bottom => "bottom",
        }
    }
}

// Only occupied rows and columns create tracks; empty indices contribute no size or additional
// gap.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GridPlacement {
    pub row: Option<f64>,
    pub column: Option<f64>,
    /// Whether the source record owns a `row` key. An owned key with no number is an authored
    /// `null` (or another invalid value) and is rejected instead of treated as omitted.
    pub has_row: bool,
    pub has_column: bool,
    pub horizontal_align: Option<GridHorizontalAlign>,
    pub vertical_align: Option<GridVerticalAlign>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GridSide {
    Left,
    Right,
    Top,
    Bottom,
}

impl GridSide {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GridOrientation {
    H,
    V,
}

impl GridOrientation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::H => "H",
            Self::V => "V",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GridCurve {
    Basis,
    BumpX,
    BumpY,
    Cardinal,
    CatmullRom,
    Linear,
    MonotoneX,
    MonotoneY,
    Natural,
    Step,
    StepAfter,
    StepBefore,
    Rounded,
}

impl GridCurve {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Basis => "basis",
            Self::BumpX => "bumpX",
            Self::BumpY => "bumpY",
            Self::Cardinal => "cardinal",
            Self::CatmullRom => "catmullRom",
            Self::Linear => "linear",
            Self::MonotoneX => "monotoneX",
            Self::MonotoneY => "monotoneY",
            Self::Natural => "natural",
            Self::Step => "step",
            Self::StepAfter => "stepAfter",
            Self::StepBefore => "stepBefore",
            Self::Rounded => "rounded",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "basis" => Self::Basis,
            "bumpX" => Self::BumpX,
            "bumpY" => Self::BumpY,
            "cardinal" => Self::Cardinal,
            "catmullRom" => Self::CatmullRom,
            "linear" => Self::Linear,
            "monotoneX" => Self::MonotoneX,
            "monotoneY" => Self::MonotoneY,
            "natural" => Self::Natural,
            "step" => Self::Step,
            "stepAfter" => Self::StepAfter,
            "stepBefore" => Self::StepBefore,
            "rounded" => Self::Rounded,
            _ => return None,
        })
    }
}

/// Shared edge corner radius, from `rendering-util/edgeCornerRadius.ts`.
pub const DEFAULT_EDGE_CORNER_RADIUS: f64 = 5.0;

pub fn resolve_edge_corner_radius(value: Option<f64>) -> f64 {
    match value {
        Some(value) if value.is_finite() && value >= 0.0 => value,
        _ => DEFAULT_EDGE_CORNER_RADIUS,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LabelSize {
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GroupTitleRect {
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
}

/// The subset of Mermaid's `Node` the grid layout reads and writes. Optional dimensions mirror
/// JavaScript's `undefined`; `is_group` mirrors `isGroup`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Node {
    pub id: String,
    pub label: Option<String>,
    pub parent_id: Option<String>,
    pub is_group: bool,
    pub is_edge_label: bool,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub label_bbox: Option<LabelSize>,
    pub group_title_rect: Option<GroupTitleRect>,
    pub metadata: Option<Value>,
    pub placement_id: Option<String>,
    pub padding: Option<f64>,
}

impl Node {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ..Self::default()
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortClipping {
    OutlineOrthogonal,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Edge {
    pub id: String,
    pub label: Option<String>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub label_node_id: Option<String>,
    pub points: Vec<RouterPoint>,
    pub curve: Option<GridCurve>,
    pub corner_radius: Option<f64>,
    pub port_clipping: Option<PortClipping>,
    pub skip_corner_fix: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GridConfig {
    pub grid: Option<Value>,
}

/// Mermaid's `LayoutData` limited to what the grid algorithm touches.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayoutData {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub config: GridConfig,
}

/// Upstream's `LayoutData & { config: { grid?: ... } }`.
pub type GridLayoutData = LayoutData;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GridErrorCode {
    InvalidCoordinate,
    CellAlignmentConflict,
    InvalidContainment,
    MissingMeasurement,
    MissingEndpoint,
    RouteNotFound,
}

impl GridErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidCoordinate => "GRID_INVALID_COORDINATE",
            Self::CellAlignmentConflict => "GRID_CELL_ALIGNMENT_CONFLICT",
            Self::InvalidContainment => "GRID_INVALID_CONTAINMENT",
            Self::MissingMeasurement => "GRID_MISSING_MEASUREMENT",
            Self::MissingEndpoint => "GRID_MISSING_ENDPOINT",
            Self::RouteNotFound => "GRID_ROUTE_NOT_FOUND",
        }
    }
}

/// Upstream's `GridError extends Error`: the display prefix is the code, matching the message the
/// TypeScript constructor produces.
#[derive(Clone, Debug, PartialEq)]
pub struct GridError {
    pub code: GridErrorCode,
    pub message: String,
    pub details: Option<Value>,
}

impl GridError {
    pub fn new(code: GridErrorCode, message: impl Into<String>, details: Option<Value>) -> Self {
        Self {
            code,
            message: message.into(),
            details,
        }
    }
}

impl std::fmt::Display for GridError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for GridError {}

pub fn grid_error(
    code: GridErrorCode,
    message: impl Into<String>,
    details: Option<Value>,
) -> GridError {
    GridError::new(code, message, details)
}

/// Formats a number the way `String(value)` does for the values the layout produces in messages.
pub fn js_number(value: f64) -> String {
    if value == 0.0 {
        return "0".to_string();
    }
    if value.is_finite() && value.fract() == 0.0 && value.abs() < 1e21 {
        return format!("{}", value as i128);
    }
    format!("{value}")
}

pub fn js_optional_number(value: Option<f64>) -> String {
    match value {
        Some(value) => js_number(value),
        None => "undefined".to_string(),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GridDefaults {
    pub columns: usize,
    pub row_gap: f64,
    pub column_gap: f64,
    pub cell_gap: f64,
    pub container_padding: f64,
    pub title_gap: f64,
    pub horizontal_align: GridHorizontalAlign,
    pub vertical_align: GridVerticalAlign,
    pub curve: GridCurve,
    pub edge_corner_radius: f64,
}

pub const GRID_DEFAULTS: GridDefaults = GridDefaults {
    columns: 0,
    row_gap: 50.0,
    column_gap: 50.0,
    cell_gap: 20.0,
    container_padding: 20.0,
    title_gap: 8.0,
    horizontal_align: GridHorizontalAlign::Center,
    vertical_align: GridVerticalAlign::Center,
    curve: GridCurve::Rounded,
    edge_corner_radius: DEFAULT_EDGE_CORNER_RADIUS,
};

#[derive(Clone, Debug, PartialEq)]
pub struct GridLayoutConfigNormalized {
    pub placements: HashMap<String, GridPlacement>,
    pub columns: usize,
    pub row_gap: f64,
    pub column_gap: f64,
    pub cell_gap: f64,
    pub container_padding: f64,
    pub title_gap: f64,
    pub horizontal_align: GridHorizontalAlign,
    pub vertical_align: GridVerticalAlign,
    pub curve: GridCurve,
    pub edge_corner_radius: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GridResolvedPlacement {
    /// Index into [`LayoutData::nodes`], standing in for upstream's `item: Node` reference.
    pub item: usize,
    pub item_id: String,
    pub row: i64,
    pub column: i64,
    pub horizontal_align: GridHorizontalAlign,
    pub vertical_align: GridVerticalAlign,
    pub source_order: i64,
    pub explicit_row: bool,
    pub explicit_column: bool,
    pub explicit_cell: bool,
}

// Multiple nodes may intentionally share a cell; they are laid out as one vertical stack.
#[derive(Clone, Debug, PartialEq)]
pub struct GridCellStack {
    pub row: i64,
    pub column: i64,
    pub items: Vec<GridResolvedPlacement>,
    pub width: f64,
    pub height: f64,
    pub vertical_align: GridVerticalAlign,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GridItemLayoutMeta {
    pub container_id: GridContainerId,
    pub row: i64,
    pub column: i64,
    pub cell_left: f64,
    pub cell_top: f64,
    pub cell_width: f64,
    pub cell_height: f64,
    pub left_corridor_x: f64,
    pub right_corridor_x: f64,
    pub top_corridor_y: f64,
    pub bottom_corridor_y: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GridContainerLayoutMeta {
    pub id: GridContainerId,
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
    pub content_left: f64,
    pub content_top: f64,
    pub content_right: f64,
    pub content_bottom: f64,
    // Corridors describe empty track boundaries for the routing layer added by the next stack PR.
    pub vertical_corridors: Vec<f64>,
    pub horizontal_corridors: Vec<f64>,
}

/// The containment model. Nodes are indices into the laid-out [`LayoutData::nodes`].
#[derive(Clone, Debug, PartialEq)]
pub struct GridForest {
    pub node_by_id: HashMap<String, usize>,
    pub group_by_id: HashMap<String, usize>,
    pub children_by_parent: HashMap<GridContainerId, Vec<usize>>,
    pub root_children: Vec<usize>,
    // Children-first ordering lets parent groups measure already-sized nested groups.
    pub post_order_groups: Vec<usize>,
    pub helper_node_ids: HashSet<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GridAttachmentDemand {
    // demandKey identifies one edge endpoint at one hierarchy boundary and remains stable across
    // retries so portal coordinates can be captured and restored transactionally.
    pub owner_id: String,
    pub side: GridSide,
    pub edge_id: String,
    pub demand_key: String,
    pub opposite_coord: f64,
    pub preferred_coord: f64,
    pub compact_portal: bool,
    // The next entry inward along the endpoint chain; a compact portal lines up with its port.
    pub inner: Option<GridAttachmentInner>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GridAttachmentInner {
    pub demand_key: String,
    pub side: GridSide,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GridAttachment {
    pub port: RouterPoint,
    pub connect: RouterPoint,
    pub orientation: GridOrientation,
    pub side: GridSide,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GridLayoutResult {
    pub forest: GridForest,
    pub config: GridLayoutConfigNormalized,
    pub containers: HashMap<GridContainerId, GridContainerLayoutMeta>,
    pub item_meta: HashMap<String, GridItemLayoutMeta>,
    pub source_order: HashMap<String, i64>,
    pub label_requirements: GridLabelRequirements,
}

// ---------------------------------------------------------------------------
// Routing visibility graph types
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RouterPoint {
    pub x: f64,
    pub y: f64,
}

impl RouterPoint {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RouterRect {
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RouterObstacle {
    pub id: String,
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
}

impl RouterObstacle {
    pub fn bounds(&self) -> RouterRect {
        RouterRect {
            left: self.left,
            right: self.right,
            top: self.top,
            bottom: self.bottom,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PortalRange {
    pub owner_id: String,
    pub side: GridSide,
    pub low: f64,
    pub high: f64,
    pub coordinate: Option<f64>,
}

/// Crossing a group boundary is represented as two vertices plus one explicit transition.
/// Keeping both sides of the boundary prevents a route from accidentally treating hierarchy as
/// open space.
#[derive(Clone, Debug, PartialEq)]
pub struct PairedPortal {
    pub owner_id: String,
    pub side: GridSide,
    pub tangential_coordinate: f64,
    pub interior: RouterPoint,
    pub exterior: RouterPoint,
    pub transition: PortalTransition,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PortalTransition {
    pub from: RouterPoint,
    pub to: RouterPoint,
    pub orientation: GridOrientation,
    pub length: f64,
    pub kind: RouterArcKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RouterVertexKind {
    Corner,
    Projection,
    Portal,
    Endpoint,
    Lane,
}

impl RouterVertexKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Corner => "corner",
            Self::Projection => "projection",
            Self::Portal => "portal",
            Self::Endpoint => "endpoint",
            Self::Lane => "lane",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RouterVertex {
    pub id: RouterVertexId,
    pub point: RouterPoint,
    pub kind: RouterVertexKind,
    pub owner_id: Option<String>,
    pub side: Option<GridSide>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RouterArcKind {
    Visibility,
    Terminal,
    Portal,
    Lane,
}

impl RouterArcKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Visibility => "visibility",
            Self::Terminal => "terminal",
            Self::Portal => "portal",
            Self::Lane => "lane",
        }
    }

    /// Upstream's `arcKindOrder` for adjacency sorting.
    pub fn ordinal(self) -> u8 {
        match self {
            Self::Visibility => 0,
            Self::Terminal => 1,
            Self::Portal => 2,
            Self::Lane => 3,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RouterArc {
    pub from: RouterVertexId,
    pub to: RouterVertexId,
    pub orientation: GridOrientation,
    pub length: f64,
    pub kind: RouterArcKind,
    pub interval_start: f64,
    pub interval_end: f64,
}

/// The allocation-conscious representation consumed by hot searches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RouterSearchArc {
    pub to: RouterVertexId,
    pub orientation_ordinal: u8,
    pub length: f64,
    pub boundary_transitions: u8,
}

pub trait OrthogonalIntervalIndex: Send + Sync + std::fmt::Debug {
    fn coordinate_count(&self) -> usize;
    fn interval_count(&self) -> usize;
    fn intersects(&self, coordinate: f64, interval_start: f64, interval_end: f64) -> bool;
    fn contains(&self, coordinate: f64, varying: f64) -> bool;
    fn nearest_boundary(&self, coordinate: f64, origin: f64, direction: i8) -> Option<f64>;
}

/// A hashable/ordered coordinate key. JavaScript `Map`s key numbers by identity; this wrapper
/// normalizes `-0` to `0` and uses total ordering so it matches those lookups deterministically.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Coordinate(pub f64);

impl Coordinate {
    pub fn new(value: f64) -> Self {
        Self(if value == 0.0 { 0.0 } else { value })
    }

    pub fn value(self) -> f64 {
        self.0
    }
}

impl Eq for Coordinate {}

impl std::hash::Hash for Coordinate {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.to_bits().hash(state);
    }
}

impl Ord for Coordinate {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

impl PartialOrd for Coordinate {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

pub struct ContainerRoutingTopology {
    // Base topologies are immutable and shared by all edges in a container. Endpoint overlays own
    // their additions and copied maps, which replaces upstream's lightweight overlay views.
    pub container_id: GridContainerId,
    pub bounds: RouterRect,
    pub obstacles: Vec<RouterObstacle>,
    pub vertices: Vec<RouterVertex>,
    pub point_vertex_ids: HashMap<String, RouterVertexId>,
    pub horizontal_vertex_lines: HashMap<Coordinate, Vec<RouterVertexId>>,
    pub vertical_vertex_lines: HashMap<Coordinate, Vec<RouterVertexId>>,
    pub adjacency: HashMap<RouterVertexId, Vec<RouterArc>>,
    pub adjacency_by_vertex: Vec<Vec<RouterArc>>,
    pub search_adjacency_by_vertex: Vec<Vec<RouterSearchArc>>,
    pub adjacency_entries: usize,
    pub horizontal_intervals: std::sync::Arc<dyn OrthogonalIntervalIndex>,
    pub vertical_intervals: std::sync::Arc<dyn OrthogonalIntervalIndex>,
    pub portal_ranges: Vec<PortalRange>,
    pub seed_count: usize,
    pub estimated_bytes: f64,
    pub vertex_count: usize,
}

impl Clone for ContainerRoutingTopology {
    fn clone(&self) -> Self {
        Self {
            container_id: self.container_id.clone(),
            bounds: self.bounds,
            obstacles: self.obstacles.clone(),
            vertices: self.vertices.clone(),
            point_vertex_ids: self.point_vertex_ids.clone(),
            horizontal_vertex_lines: self.horizontal_vertex_lines.clone(),
            vertical_vertex_lines: self.vertical_vertex_lines.clone(),
            adjacency: self.adjacency.clone(),
            adjacency_by_vertex: self.adjacency_by_vertex.clone(),
            search_adjacency_by_vertex: self.search_adjacency_by_vertex.clone(),
            adjacency_entries: self.adjacency_entries,
            horizontal_intervals: self.horizontal_intervals.clone(),
            vertical_intervals: self.vertical_intervals.clone(),
            portal_ranges: self.portal_ranges.clone(),
            seed_count: self.seed_count,
            estimated_bytes: self.estimated_bytes,
            vertex_count: self.vertex_count,
        }
    }
}

impl std::fmt::Debug for ContainerRoutingTopology {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContainerRoutingTopology")
            .field("container_id", &self.container_id)
            .field("bounds", &self.bounds)
            .field("obstacles", &self.obstacles)
            .field("vertices", &self.vertices)
            .field("adjacency_entries", &self.adjacency_entries)
            .field("seed_count", &self.seed_count)
            .field("estimated_bytes", &self.estimated_bytes)
            .finish_non_exhaustive()
    }
}

impl ContainerRoutingTopology {
    pub fn get_vertex(&self, id: RouterVertexId) -> Option<&RouterVertex> {
        self.vertices.get(id)
    }

    pub fn get_search_arcs(&self, id: RouterVertexId) -> &[RouterSearchArc] {
        self.search_adjacency_by_vertex
            .get(id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}

pub struct GridRoutingContext {
    // Containers that exceed a topology cap are recorded here and routed through the validated
    // compatibility path; resource exhaustion is degradation, not permission to emit invalid paths.
    pub topologies: HashMap<GridContainerId, ContainerRoutingTopology>,
    pub fallback_containers: HashMap<
        GridContainerId,
        crate::layout::grid::router_instrumentation::GridRoutingFallbackReason,
    >,
    pub search_budget: GridSearchBudget,
    pub search_budget_warning_emitted: bool,
    pub base_estimated_bytes: f64,
    pub metrics: Option<GridRoutingInstrumentation>,
}

impl Default for GridRoutingContext {
    fn default() -> Self {
        Self {
            topologies: HashMap::new(),
            fallback_containers: HashMap::new(),
            search_budget: GridSearchBudget { expanded_states: 0 },
            search_budget_warning_emitted: false,
            base_estimated_bytes: 0.0,
            metrics: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GridSearchBudget {
    pub expanded_states: usize,
}

pub fn is_edge_label_node(node: &Node) -> bool {
    node.is_edge_label
}

pub fn is_finite_positive_number(value: Option<f64>) -> bool {
    matches!(value, Some(value) if value.is_finite() && value > 0.0)
}

/// Compares strings by UTF-16 code units so algorithmic ordering is independent of locale and
/// ICU, matching `layout-utils/helpers.ts`'s `compareCodeUnits`.
pub fn compare_code_units(a: &str, b: &str) -> std::cmp::Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}
