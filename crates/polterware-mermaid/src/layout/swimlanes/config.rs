//! Port of `config.ts`: the swimlanes layout's constants.

/// Clear space kept between two arrowheads that land side by side on one face (px).
const ARROWHEAD_GAP: f64 = 4.0;

/// Stroke width of an edge line (px); the flowchart `.flowchart-link` style default.
const EDGE_STROKE_WIDTH: f64 = 2.0;

/// Edge routing and spacing constants.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeRouting {
    /// Spacing between parallel edges in the same corridor (px).
    pub edge_gap: f64,
    /// Horizontal offset from lane boundary to corridor center (px).
    pub lane_margin: f64,
    /// Edge of the square the point marker draws an arrowhead in (px); see `markers.js`.
    pub arrowhead_size: f64,
    pub arrowhead_gap: f64,
    /// Closest two edges may end on one face of a node: an arrowhead and the gap beside it.
    pub min_arrowhead_spacing: f64,
    pub edge_stroke_width: f64,
    /// Distance under which a bend of one route and another route read as one stroke (px).
    pub route_touch_tolerance: f64,
}

pub const EDGE_ROUTING: EdgeRouting = EdgeRouting {
    edge_gap: 12.0,
    lane_margin: 20.0,
    arrowhead_size: 8.0,
    arrowhead_gap: ARROWHEAD_GAP,
    min_arrowhead_spacing: 12.0,
    edge_stroke_width: EDGE_STROKE_WIDTH,
    route_touch_tolerance: ARROWHEAD_GAP + EDGE_STROKE_WIDTH,
};

/// Numerical precision constants.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Precision {
    /// Epsilon for floating-point comparisons.
    pub epsilon: f64,
}

pub const PRECISION: Precision = Precision { epsilon: 1e-6 };

/// Layer assignment constants.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayeringConstants {
    /// Default number of iterations for gravity-based layering.
    pub gravity_iterations: usize,
    /// Maximum number of passes for crossing-based rank optimization.
    pub max_crossing_optimization_passes: usize,
    /// Whether to compact single-input nodes by default.
    pub default_compact_single_input: bool,
}

pub const LAYERING: LayeringConstants = LayeringConstants {
    gravity_iterations: 8,
    max_crossing_optimization_passes: 4,
    default_compact_single_input: true,
};

/// Coordinate assignment constants.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CoordinateConstants {
    /// Default vertical gap between layers (px).
    pub default_layer_gap: f64,
    /// Default horizontal gap between nodes (px).
    pub default_node_gap: f64,
    /// Room kept between an edge label and the nodes the edge joins (px).
    pub edge_label_clearance: f64,
}

pub const COORDINATES: CoordinateConstants = CoordinateConstants {
    default_layer_gap: 100.0,
    default_node_gap: 40.0,
    edge_label_clearance: 16.0,
};
