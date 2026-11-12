//! Port of upstream `rendering-util/edgeCornerRadius.ts`.

pub const DEFAULT_EDGE_CORNER_RADIUS: f64 = 5.0;

/// Keep corner geometry consistent between the initial renderer and any later path rewrite.
pub fn resolve_edge_corner_radius(value: Option<f64>) -> f64 {
    match value {
        Some(v) if v.is_finite() && v >= 0.0 => v,
        _ => DEFAULT_EDGE_CORNER_RADIUS,
    }
}
