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
    BasisClosed,
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
            "basisClosed" => Self::BasisClosed,
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
    if let [center] = points {
        return Some(EdgeLabelPosition {
            center: *center,
            bounds: Rect::new(
                center.x - width / 2.0,
                center.y - height / 2.0,
                width,
                height,
            ),
        });
    }
    let distance = points
        .windows(2)
        .map(|p| p[0].distance_to(p[1]))
        .sum::<f64>()
        / 2.0;
    let center = calculate_point(points, distance)?;
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

/// Returns the point at a distance along the polyline, following `utils.calculatePoint`.
pub fn calculate_point(points: &[Point], mut distance: f64) -> Option<Point> {
    for pair in points.windows(2) {
        let length = pair[0].distance_to(pair[1]);
        if length == 0.0 {
            return Some(pair[0]);
        }
        if length < distance {
            distance -= length;
            continue;
        }
        let ratio = distance / length;
        if ratio <= 0.0 {
            return Some(pair[0]);
        }
        if ratio >= 1.0 {
            return Some(pair[1]);
        }
        let round = |v: f64| (v * 100_000.0 + 0.5).floor() / 100_000.0;
        return Some(Point::new(
            round((1.0 - ratio) * pair[0].x + ratio * pair[1].x),
            round((1.0 - ratio) * pair[0].y + ratio * pair[1].y),
        ));
    }
    None
}

/// Terminal position names accepted by `calcTerminalLabelPosition`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalPosition {
    StartLeft,
    StartRight,
    EndLeft,
    EndRight,
}

/// Finds the center of a terminal label, or returns `None` if the path is too short.
pub fn terminal_label_position(
    points: &[Point],
    position: TerminalPosition,
    marker_size: f64,
) -> Option<Point> {
    let mut path = points.to_vec();
    if matches!(
        position,
        TerminalPosition::EndLeft | TerminalPosition::EndRight
    ) {
        path.reverse();
    }
    let start = *path.first()?;
    let center = calculate_point(&path, 25.0 + marker_size)?;
    let mut angle = (start.y - center.y).atan2(start.x - center.x);
    match position {
        TerminalPosition::StartLeft => angle += std::f64::consts::PI,
        TerminalPosition::EndRight => angle -= std::f64::consts::PI,
        _ => {}
    }
    let offset = 10.0 + marker_size * 0.5;
    Some(Point::new(
        angle.sin() * offset + (start.x + center.x) / 2.0,
        -angle.cos() * offset + (start.y + center.y) / 2.0,
    ))
}

/// Generates the edge path. Catmull-Rom, monotone and natural curves are still approximations.
pub fn edge_path(points: &[Point], curve: Curve, radius: f64) -> Path {
    match curve {
        Curve::Rounded => rounded_path(points, radius),
        Curve::Step | Curve::StepAfter | Curve::StepBefore => step_path(points, curve),
        Curve::BumpX | Curve::BumpY => bump_path(points, curve),
        Curve::Linear => linear_path(points),
        Curve::Basis => basis_path(points, false),
        Curve::BasisClosed => basis_path(points, true),
        Curve::Cardinal => cardinal_path(points),
        Curve::CatmullRom | Curve::MonotoneX | Curve::MonotoneY | Curve::Natural => {
            smooth_path(points)
        }
    }
}

fn linear_path(points: &[Point]) -> Path {
    let mut path = Path::polyline(points.iter().copied());
    if points.len() == 1 {
        path.close();
    }
    path
}

fn step_path(points: &[Point], curve: Curve) -> Path {
    let mut out = Path::new();
    if let Some(first) = points.first() {
        out.move_to(*first);
    }
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        match curve {
            Curve::StepBefore => {
                out.line_to(Point::new(a.x, b.y)).line_to(b);
            }
            _ => {
                let x = if curve == Curve::StepAfter {
                    b.x
                } else {
                    (a.x + b.x) / 2.0
                };
                out.line_to(Point::new(x, a.y)).line_to(Point::new(x, b.y));
            }
        }
    }
    if points.len() == 1 {
        out.close();
    } else if curve == Curve::Step && points.len() > 1 {
        out.line_to(*points.last().unwrap());
    }
    out
}

fn weighted(a: Point, wa: f64, b: Point, wb: f64, c: Point, wc: f64, d: f64) -> Point {
    Point::new(
        (wa * a.x + wb * b.x + wc * c.x) / d,
        (wa * a.y + wb * b.y + wc * c.y) / d,
    )
}
fn basis_cubic(out: &mut Path, a: Point, b: Point, c: Point) {
    out.cubic_to(
        weighted(a, 2., b, 1., c, 0., 3.),
        weighted(a, 1., b, 2., c, 0., 3.),
        weighted(a, 1., b, 4., c, 1., 6.),
    );
}
fn basis_path(points: &[Point], closed: bool) -> Path {
    let mut out = Path::new();
    match (closed, points.len()) {
        (_, 0) => {}
        (_, 1) => {
            out.move_to(points[0]).close();
        }
        (true, 2) => {
            out.move_to(weighted(points[0], 1., points[1], 2., points[0], 0., 3.));
            out.line_to(weighted(points[1], 1., points[0], 2., points[0], 0., 3.))
                .close();
        }
        (false, 2) => {
            out.move_to(points[0]).line_to(points[1]);
        }
        (false, _) => {
            out.move_to(points[0])
                .line_to(weighted(points[0], 5., points[1], 1., points[0], 0., 6.));
            for i in 2..points.len() {
                basis_cubic(&mut out, points[i - 2], points[i - 1], points[i]);
            }
            let n = points.len();
            basis_cubic(&mut out, points[n - 2], points[n - 1], points[n - 1]);
            out.line_to(points[n - 1]);
        }
        (true, n) => {
            out.move_to(weighted(points[0], 1., points[1], 4., points[2], 1., 6.));
            for i in 3..n + 3 {
                basis_cubic(
                    &mut out,
                    points[(i - 2) % n],
                    points[(i - 1) % n],
                    points[i % n],
                );
            }
        }
    }
    out
}
fn cardinal_path(points: &[Point]) -> Path {
    if points.len() < 3 {
        return linear_path(points);
    }
    let mut out = Path::new();
    out.move_to(points[0]);
    for i in 0..points.len() - 1 {
        let a = points[i];
        let b = points[i + 1];
        let prev = if i == 0 { b } else { points[i - 1] };
        let next = points.get(i + 2).copied().unwrap_or(a);
        out.cubic_to(
            Point::new(a.x + (b.x - prev.x) / 6., a.y + (b.y - prev.y) / 6.),
            Point::new(b.x + (a.x - next.x) / 6., b.y + (a.y - next.y) / 6.),
            b,
        );
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
    if points.len() == 1 {
        out.close();
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

/// Cut the polyline at the first entry into a cluster rectangle, retaining the outside prefix.
pub fn cut_path_at_intersection(points: &[Point], bounds: Rect) -> Vec<Point> {
    let Some(&first) = points.first() else {
        return Vec::new();
    };
    let outside = |p: Point| {
        p.x <= bounds.x || p.x >= bounds.right() || p.y <= bounds.y || p.y >= bounds.bottom()
    };
    let mut result = Vec::new();
    let mut last_outside = first;
    let mut inside = false;
    for &point in points {
        if !outside(point) && !inside {
            let hit = rect_intersection(bounds, last_outside, point);
            if !result.contains(&hit) {
                result.push(hit);
            }
            inside = true;
        } else {
            last_outside = point;
            if !inside {
                result.push(point);
            }
        }
    }
    result
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
    fn matches_d3_curve_fixture() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("testdata/edges/d3-curves.json")).unwrap();
        let points: Vec<Point> = fixture["points"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| Point::new(p[0].as_f64().unwrap(), p[1].as_f64().unwrap()))
            .collect();
        let cases = [
            ("Linear", Curve::Linear),
            ("Basis", Curve::Basis),
            ("BasisClosed", Curve::BasisClosed),
            ("Cardinal", Curve::Cardinal),
            ("BumpX", Curve::BumpX),
            ("BumpY", Curve::BumpY),
            ("Step", Curve::Step),
            ("StepAfter", Curve::StepAfter),
            ("StepBefore", Curve::StepBefore),
        ];
        fn number(v: f64) -> String {
            let v = (v * 1000.0 + 0.5).floor() / 1000.0;
            if v == 0.0 { "0".into() } else { v.to_string() }
        }
        for (name, curve) in cases {
            let mut actual = String::new();
            for cmd in edge_path(&points, curve, 0.).commands {
                match cmd {
                    PathCommand::MoveTo { x, y } => {
                        actual.push_str(&format!("M{},{}", number(x), number(y)))
                    }
                    PathCommand::LineTo { x, y } => {
                        actual.push_str(&format!("L{},{}", number(x), number(y)))
                    }
                    PathCommand::CubicTo {
                        c1x,
                        c1y,
                        c2x,
                        c2y,
                        x,
                        y,
                    } => actual.push_str(&format!(
                        "C{},{},{},{},{},{}",
                        number(c1x),
                        number(c1y),
                        number(c2x),
                        number(c2y),
                        number(x),
                        number(y)
                    )),
                    PathCommand::Close => actual.push('Z'),
                    _ => panic!("unexpected command"),
                }
            }
            assert_eq!(actual, fixture["paths"][name].as_str().unwrap(), "{name}");
        }
    }
    #[test]
    fn cut_keeps_outside_prefix_and_boundary() {
        let bounds = Rect::new(10., 0., 10., 10.);
        let points = [
            Point::new(0., 5.),
            Point::new(5., 5.),
            Point::new(15., 5.),
            Point::new(25., 5.),
        ];
        assert_eq!(
            cut_path_at_intersection(&points, bounds),
            vec![points[0], points[1], Point::new(10., 5.)]
        );
    }
    #[test]
    fn single_point_label_is_positioned() {
        assert_eq!(
            position_edge_label(&[Point::new(3., 4.)], 2., 2.)
                .unwrap()
                .center,
            Point::new(3., 4.)
        );
    }
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
            &[Point::new(0., 0.), Point::new(5., 0.), Point::new(10., 0.)],
            4.,
            2.,
        )
        .unwrap();
        assert_eq!(p.center, Point::new(5., 0.));
        assert_eq!(p.bounds, Rect::new(3., -1., 4., 2.));
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
