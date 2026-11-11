//! Edge geometry and label placement corresponding to upstream `edges.js`, `lineJump.ts`,
//! `orthogonalEdgeClipping.js`, `edgeCornerRadius.ts`, and `edgeStep.ts`.
//! SVG CSS classes are retained as scene roles; rough.js handDrawn rendering remains a TODO.
use crate::scene::{Path, Point, Rect};

pub const DEFAULT_EDGE_CORNER_RADIUS: f64 = 5.0;
pub const EDGE_STEP_DURATION: f64 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Curve {
    Linear,
    Basis,
    Cardinal,
    BumpX,
    BumpY,
    CatmullRom,
    MonotoneX,
    MonotoneY,
    Natural,
    Step,
    StepAfter,
    StepBefore,
    Rounded,
}

impl Curve {
    /// Unknown curve names follow d3's upstream fallback to `curveBasis`.
    pub fn from_name(name: &str) -> Self {
        match name {
            "linear" => Self::Linear,
            "cardinal" => Self::Cardinal,
            "bumpX" => Self::BumpX,
            "bumpY" => Self::BumpY,
            "catmullRom" => Self::CatmullRom,
            "monotoneX" => Self::MonotoneX,
            "monotoneY" => Self::MonotoneY,
            "natural" => Self::Natural,
            "step" => Self::Step,
            "stepAfter" => Self::StepAfter,
            "stepBefore" => Self::StepBefore,
            "rounded" => Self::Rounded,
            "basis" => Self::Basis,
            _ => Self::Basis,
        }
    }
}

/// Upstream `resolveEdgeCornerRadius`: invalid, negative and non-finite values use 5.
pub fn resolve_edge_corner_radius(value: Option<f64>) -> f64 {
    value
        .filter(|v| v.is_finite() && *v >= 0.0)
        .unwrap_or(DEFAULT_EDGE_CORNER_RADIUS)
}

pub fn get_step_duration(value: Option<f64>) -> f64 {
    value
        .filter(|v| v.is_finite() && *v > 0.0)
        .unwrap_or(EDGE_STEP_DURATION)
}

pub fn is_step_edge(curve: Curve, explicit: Option<f64>) -> bool {
    explicit.is_some() || matches!(curve, Curve::Step | Curve::StepAfter | Curve::StepBefore)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeLabelPosition {
    pub center: Point,
    pub bounds: Rect,
}

/// Places an edge label at the middle point, matching the d3 edge label anchor.
pub fn position_edge_label(points: &[Point], width: f64, height: f64) -> Option<EdgeLabelPosition> {
    let center = points.get(points.len() / 2).copied()?;
    Some(EdgeLabelPosition {
        center,
        bounds: Rect::new(
            center.x - width / 2.0,
            center.y - height / 2.0,
            width,
            height,
        ),
    })
}

/// d3 line generators share the source points; this scene representation preserves every point
/// for linear/rounded edges and provides deterministic cubic approximations for the common curves.
pub fn edge_path(points: &[Point], curve: Curve, radius: f64) -> Path {
    match curve {
        Curve::Rounded => rounded_path(points, radius),
        Curve::Step | Curve::StepAfter | Curve::StepBefore => step_path(points, curve),
        Curve::BumpX | Curve::BumpY => bump_path(points, curve),
        Curve::Linear => Path::polyline(points.iter().copied()),
        Curve::Basis
        | Curve::Cardinal
        | Curve::CatmullRom
        | Curve::MonotoneX
        | Curve::MonotoneY
        | Curve::Natural => smooth_path(points),
    }
}

fn step_path(points: &[Point], curve: Curve) -> Path {
    let mut out = Path::new();
    if let Some(first) = points.first() {
        out.move_to(*first);
    }
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let mid = (a.x + b.x) / 2.0;
        match curve {
            Curve::StepAfter => {
                out.line_to(Point::new(b.x, a.y));
                out.line_to(b);
            }
            Curve::StepBefore => {
                out.line_to(Point::new(a.x, b.y));
                out.line_to(b);
            }
            _ => {
                out.line_to(Point::new(mid, a.y));
                out.line_to(Point::new(mid, b.y));
                out.line_to(b);
            }
        }
    }
    out
}
fn bump_path(points: &[Point], curve: Curve) -> Path {
    let mut out = Path::new();
    if let Some(first) = points.first() {
        out.move_to(*first);
    }
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if curve == Curve::BumpX {
            let mid = (a.x + b.x) / 2.0;
            out.cubic_to(Point::new(mid, a.y), Point::new(mid, b.y), b);
        } else {
            let mid = (a.y + b.y) / 2.0;
            out.cubic_to(Point::new(a.x, mid), Point::new(b.x, mid), b);
        }
    }
    out
}
fn smooth_path(points: &[Point]) -> Path {
    let mut out = Path::new();
    if points.is_empty() {
        return out;
    }
    out.move_to(points[0]);
    for i in 0..points.len().saturating_sub(1) {
        let a = points[i];
        let b = points[i + 1];
        let prev = if i == 0 { a } else { points[i - 1] };
        let next = points.get(i + 2).copied().unwrap_or(b);
        out.cubic_to(
            Point::new(a.x + (b.x - prev.x) / 6.0, a.y + (b.y - prev.y) / 6.0),
            Point::new(b.x - (next.x - a.x) / 6.0, b.y - (next.y - a.y) / 6.0),
            b,
        );
    }
    out
}

pub fn rounded_path(points: &[Point], radius: f64) -> Path {
    let mut path = Path::new();
    if points.len() < 2 {
        return path;
    }
    path.move_to(points[0]);
    for i in 1..points.len().saturating_sub(1) {
        let prev = points[i - 1];
        let corner = points[i];
        let next = points[i + 1];
        let len1 = prev.distance_to(corner);
        let len2 = corner.distance_to(next);
        if len1 < 1e-5 || len2 < 1e-5 {
            path.line_to(corner);
            continue;
        }
        let dot = ((corner.x - prev.x) * (next.x - corner.x)
            + (corner.y - prev.y) * (next.y - corner.y))
            / (len1 * len2);
        let angle = dot.clamp(-1.0, 1.0).acos();
        if angle < 1e-5 || (std::f64::consts::PI - angle).abs() < 1e-5 {
            path.line_to(corner);
            continue;
        }
        let r = (radius / (angle / 2.0).sin())
            .min(len1 / 2.0)
            .min(len2 / 2.0);
        let before = Point::new(
            corner.x + (prev.x - corner.x) * r / prev.distance_to(corner),
            corner.y + (prev.y - corner.y) * r / prev.distance_to(corner),
        );
        let after = Point::new(
            corner.x + (next.x - corner.x) * r / corner.distance_to(next),
            corner.y + (next.y - corner.y) * r / corner.distance_to(next),
        );
        path.line_to(before).quad_to(corner, after);
    }
    if let Some(last) = points.last() {
        path.line_to(*last);
    }
    path
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SegmentCrossing {
    pub point: Point,
    pub first_segment: usize,
    pub second_segment: usize,
}
/// Finds strict interior intersections of axis-aligned segments, as lineJump does.
pub fn segment_intersection(a: Point, b: Point, c: Point, d: Point) -> Option<Point> {
    let denom = (b.x - a.x) * (d.y - c.y) - (b.y - a.y) * (d.x - c.x);
    if denom == 0.0 {
        return None;
    }
    let t = ((c.x - a.x) * (d.y - c.y) - (c.y - a.y) * (d.x - c.x)) / denom;
    let u = ((c.x - a.x) * (b.y - a.y) - (c.y - a.y) * (b.x - a.x)) / denom;
    (t > 1e-6 && t < 1.0 - 1e-6 && u > 1e-6 && u < 1.0 - 1e-6)
        .then(|| Point::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y)))
}

/// Clips first and last points of an orthogonal polyline to node outline rectangles.
pub fn clip_orthogonal_endpoints(points: &[Point], tail: Rect, head: Rect) -> Vec<Point> {
    let mut out = points.to_vec();
    if out.len() < 2 {
        return out;
    }
    out[0] = rect_intersection(tail, out[0], out[1]);
    let n = out.len();
    out[n - 1] = rect_intersection(head, out[n - 1], out[n - 2]);
    out
}
fn rect_intersection(rect: Rect, inside: Point, outside: Point) -> Point {
    let dx = outside.x - inside.x;
    let dy = outside.y - inside.y;
    let mut ts = Vec::new();
    if dx != 0.0 {
        ts.push((rect.x - inside.x) / dx);
        ts.push((rect.right() - inside.x) / dx);
    }
    if dy != 0.0 {
        ts.push((rect.y - inside.y) / dy);
        ts.push((rect.bottom() - inside.y) / dy);
    }
    ts.into_iter()
        .filter(|t| *t >= 0.0 && *t <= 1.0)
        .map(|t| Point::new(inside.x + t * dx, inside.y + t * dy))
        .find(|p| {
            p.x >= rect.x - 1e-8
                && p.x <= rect.right() + 1e-8
                && p.y >= rect.y - 1e-8
                && p.y <= rect.bottom() + 1e-8
        })
        .unwrap_or(inside)
}

/// Cut the polyline at the first intersection with a cluster rectangle, retaining the outside
/// portion. This is used for edges entering/exiting clusters before curve generation.
pub fn cut_path_at_intersection(points: &[Point], bounds: Rect) -> Vec<Point> {
    if points.len() < 2 {
        return points.to_vec();
    }
    for i in 0..points.len() - 1 {
        let a = points[i];
        let b = points[i + 1];
        let hit = rect_intersection(bounds, a, b);
        if (hit.x - a.x).hypot(hit.y - a.y) > 1e-8 && (hit.x - b.x).hypot(hit.y - b.y) > 1e-8 {
            let mut out = vec![hit];
            out.extend_from_slice(&points[i + 1..]);
            return out;
        }
    }
    points.to_vec()
}

pub fn style_roles(
    thickness: &str,
    pattern: &str,
    animation: Option<&str>,
    animate: bool,
) -> Vec<String> {
    let t = match thickness {
        "thick" => "thick",
        "invisible" => "invisible",
        _ => "normal",
    };
    let p = match pattern {
        "dotted" => "dotted",
        "dashed" => "dashed",
        _ => "solid",
    };
    let mut v = vec![format!("edge-thickness-{t}"), format!("edge-pattern-{p}")];
    if let Some(a) = animation {
        v.push(format!("edge-animation-{a}"));
    } else if animate {
        v.push("edge-animation-fast".into());
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::PathCommand;
    #[test]
    fn curve_names_keep_upstream_fallback_and_types() {
        assert_eq!(Curve::from_name("linear"), Curve::Linear);
        assert_eq!(Curve::from_name("unknown"), Curve::Basis);
    }
    #[test]
    fn rounded_edges_cut_corners() {
        let p = rounded_path(
            &[
                Point::new(0., 0.),
                Point::new(10., 0.),
                Point::new(10., 10.),
            ],
            2.,
        );
        assert_eq!(p.commands.len(), 4);
        assert!(matches!(p.commands[2], PathCommand::QuadTo { .. }));
    }
    #[test]
    fn d3_step_variants_emit_expected_vertices() {
        assert_eq!(
            edge_path(&[Point::new(0., 0.), Point::new(10., 10.)], Curve::Step, 0.)
                .commands
                .len(),
            4
        );
    }
    #[test]
    fn label_uses_midpoint_and_measured_size() {
        let p = position_edge_label(
            &[Point::new(0., 0.), Point::new(5., 6.), Point::new(8., 9.)],
            4.,
            2.,
        )
        .unwrap();
        assert_eq!(p.center, Point::new(5., 6.));
        assert_eq!(p.bounds, Rect::new(3., 5., 4., 2.));
    }
    #[test]
    fn markerless_crossing_and_axis_crossing_are_detected() {
        assert_eq!(
            segment_intersection(
                Point::new(0., 0.),
                Point::new(10., 0.),
                Point::new(5., -2.),
                Point::new(5., 2.)
            ),
            Some(Point::new(5., 0.))
        );
    }
    #[test]
    fn edge_options_resolve_defaults() {
        assert_eq!(resolve_edge_corner_radius(Some(-1.)), 5.);
        assert_eq!(get_step_duration(None), 1.);
        assert!(is_step_edge(Curve::Linear, Some(0.5)));
    }
}
