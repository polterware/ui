//! Port of upstream `rendering-util/edgeStep.ts`.

use super::types::Edge;

/// Seconds the travelling dot spends on one step when `flowchart.stepDuration` is not set.
pub const EDGE_STEP_DURATION: f64 = 1.0;

/// The configured step length in seconds, or the default when it is not a positive number.
pub fn get_step_duration(value: Option<f64>) -> f64 {
    match value {
        Some(v) if v.is_finite() && v > 0.0 => v,
        _ => EDGE_STEP_DURATION,
    }
}

/// True when the edge takes part in the step animation (a positive integer `step`).
pub fn is_step_edge(edge: &Edge) -> bool {
    matches!(edge.step, Some(v) if v.is_finite() && v > 0.0 && v.fract() == 0.0)
}
