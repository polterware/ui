//! Edge geometry and label placement corresponding to upstream `edges.js`, `edgeMarker.ts`,
//! `lineJump.ts`, `orthogonalEdgeClipping.js`, `edgeCornerRadius.ts`, and `edgeStep.ts`.
//! SVG CSS classes are retained as scene roles; rough.js handDrawn rendering, DOM insertion,
//! and font measurement stay with the scene consumer.
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::ParseError;
use crate::render::intersect::intersect_ellipse;
use crate::render::markers::{MarkerKind, marker_placement};
use crate::scene::{Path, PathCommand, Point, Rect};

pub const DEFAULT_EDGE_CORNER_RADIUS: f64 = 5.0;
pub const EDGE_STEP_DURATION: f64 = 1.0;
/// `AXIS_TOLERANCE` in `edges.js`: below this two coordinates count as one row or column.
pub const AXIS_TOLERANCE: f64 = 0.5;
const CORNER_EPSILON: f64 = 1e-5;
const CORNER_JUMP_CLEARANCE: f64 = 2.0;
const MIN_USEFUL_RADIUS_RATIO: f64 = 0.6;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
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

/// Generates the edge path using the corresponding d3-shape curve implementation.
pub fn edge_path(points: &[Point], curve: Curve, radius: f64) -> Path {
    match curve {
        Curve::Rounded => rounded_path(points, radius),
        Curve::Step | Curve::StepAfter | Curve::StepBefore => step_path(points, curve),
        Curve::BumpX | Curve::BumpY => bump_path(points, curve),
        Curve::Linear => linear_path(points),
        Curve::Basis => basis_path(points, false),
        Curve::BasisClosed => basis_path(points, true),
        Curve::Cardinal => cardinal_path(points),
        Curve::CatmullRom => catmull_rom_path(points),
        Curve::MonotoneX => monotone_path(points, false),
        Curve::MonotoneY => monotone_path(points, true),
        Curve::Natural => natural_path(points),
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
fn catmull_rom_path(points: &[Point]) -> Path {
    let mut out = Path::new();
    match points.len() {
        0 => return out,
        1 => {
            out.move_to(points[0]);
            return out;
        }
        2 => return linear_path(points),
        _ => {}
    }
    out.move_to(points[0]);
    let alpha = 0.5;
    let mut first_square = 0.0;
    let first_length = catmull_length(points[0], points[1], alpha, &mut first_square);
    let mut lengths = [0.0, first_length, 0.0];
    let mut squares = [0.0, first_square, 0.0];
    for i in 2..points.len() {
        lengths[2] = catmull_length(points[i - 1], points[i], alpha, &mut squares[2]);
        let x0 = if i >= 3 { points[i - 3] } else { points[i - 2] };
        let (x1, x2, x3) = (points[i - 2], points[i - 1], points[i]);
        let (l01, l12, l23) = (lengths[0], lengths[1], lengths[2]);
        let (l01_2, l12_2, l23_2) = (squares[0], squares[1], squares[2]);
        let mut c1 = x1;
        let mut c2 = x2;
        if l01 > f64::EPSILON {
            let a = 2.0 * l01_2 + 3.0 * l01 * l12 + l12_2;
            let n = 3.0 * l01 * (l01 + l12);
            c1 = Point::new(
                (x1.x * a - x0.x * l12_2 + x2.x * l01_2) / n,
                (x1.y * a - x0.y * l12_2 + x2.y * l01_2) / n,
            );
        }
        if l23 > f64::EPSILON {
            let b = 2.0 * l23_2 + 3.0 * l23 * l12 + l12_2;
            let m = 3.0 * l23 * (l23 + l12);
            c2 = Point::new(
                (x2.x * b + x1.x * l23_2 - x3.x * l12_2) / m,
                (x2.y * b + x1.y * l23_2 - x3.y * l12_2) / m,
            );
        }
        out.cubic_to(c1, c2, x2);
        lengths = [lengths[1], lengths[2], 0.0];
        squares = [squares[1], squares[2], 0.0];
    }
    let (x0, x1, x2) = (
        points[points.len() - 3],
        points[points.len() - 2],
        *points.last().unwrap(),
    );
    let (l01, l12) = (lengths[0], lengths[1]);
    let (l01_2, l12_2) = (squares[0], squares[1]);
    let mut c1 = x1;
    if l01 > f64::EPSILON {
        let a = 2.0 * l01_2 + 3.0 * l01 * l12 + l12_2;
        let n = 3.0 * l01 * (l01 + l12);
        c1 = Point::new(
            (x1.x * a - x0.x * l12_2 + x2.x * l01_2) / n,
            (x1.y * a - x0.y * l12_2 + x2.y * l01_2) / n,
        );
    }
    out.cubic_to(c1, x2, x2);
    out
}

fn catmull_length(a: Point, b: Point, alpha: f64, square: &mut f64) -> f64 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    *square = (dx * dx + dy * dy).powf(alpha);
    square.sqrt()
}

fn monotone_path(points: &[Point], reflect: bool) -> Path {
    let mut out = Path::new();
    let mut filtered = Vec::with_capacity(points.len());
    for &point in points {
        if filtered.last() != Some(&point) {
            filtered.push(point);
        }
    }
    let points = filtered;
    let Some(&first) = points.first() else {
        return out;
    };
    let transform = |p: Point| if reflect { Point::new(p.y, p.x) } else { p };
    let first = transform(first);
    out.move_to(if reflect {
        Point::new(first.y, first.x)
    } else {
        first
    });
    if points.len() == 1 {
        return out;
    }
    let mut p0 = transform(points[0]);
    let mut p1 = transform(points[1]);
    if points.len() == 2 {
        out.line_to(points[1]);
        return out;
    }
    let t1 = monotone_slope3(p0, p1, transform(points[2]));
    let mut t0 = monotone_slope2(p0, p1, t1);
    monotone_bezier(&mut out, p0, p1, t0, t1, reflect);
    p0 = p1;
    p1 = transform(points[2]);
    t0 = t1;
    for point in points.iter().skip(3) {
        let p2 = transform(*point);
        let next_t = monotone_slope3(p0, p1, p2);
        monotone_bezier(&mut out, p0, p1, t0, next_t, reflect);
        p0 = p1;
        p1 = p2;
        t0 = next_t;
    }
    monotone_bezier(&mut out, p0, p1, t0, monotone_slope2(p0, p1, t0), reflect);
    out
}

fn monotone_slope3(p0: Point, p1: Point, p2: Point) -> f64 {
    let h0 = p1.x - p0.x;
    let h1 = p2.x - p1.x;
    let s0 = (p1.y - p0.y)
        / if h0 != 0.0 {
            h0
        } else if h1 < 0.0 {
            -0.0
        } else {
            0.0
        };
    let s1 = (p2.y - p1.y)
        / if h1 != 0.0 {
            h1
        } else if h0 < 0.0 {
            -0.0
        } else {
            0.0
        };
    let p = (s0 * h1 + s1 * h0) / (h0 + h1);
    let sign = |x: f64| if x < 0.0 { -1.0 } else { 1.0 };
    let tangent = (sign(s0) + sign(s1)) * s0.abs().min(s1.abs()).min(0.5 * p.abs());
    if tangent.is_nan() { 0.0 } else { tangent }
}

fn monotone_slope2(p0: Point, p1: Point, t: f64) -> f64 {
    let h = p1.x - p0.x;
    if h != 0.0 {
        (3.0 * (p1.y - p0.y) / h - t) / 2.0
    } else {
        t
    }
}

fn monotone_bezier(out: &mut Path, p0: Point, p1: Point, t0: f64, t1: f64, reflect: bool) {
    let dx = (p1.x - p0.x) / 3.0;
    let point = |x: f64, y: f64| {
        if reflect {
            Point::new(y, x)
        } else {
            Point::new(x, y)
        }
    };
    out.cubic_to(
        point(p0.x + dx, p0.y + dx * t0),
        point(p1.x - dx, p1.y - dx * t1),
        point(p1.x, p1.y),
    );
}

fn natural_path(points: &[Point]) -> Path {
    let mut out = Path::new();
    match points.len() {
        0 => return out,
        1 => {
            out.move_to(points[0]);
            return out;
        }
        2 => return linear_path(points),
        _ => {}
    }
    out.move_to(points[0]);
    let x: Vec<f64> = points.iter().map(|p| p.x).collect();
    let y: Vec<f64> = points.iter().map(|p| p.y).collect();
    let (x1, x2) = natural_controls(&x);
    let (y1, y2) = natural_controls(&y);
    for i in 0..points.len() - 1 {
        out.cubic_to(
            Point::new(x1[i], y1[i]),
            Point::new(x2[i], y2[i]),
            points[i + 1],
        );
    }
    out
}

fn natural_controls(values: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let n = values.len() - 1;
    let mut a = vec![0.0; n];
    let mut b = vec![0.0; n];
    let mut r = vec![0.0; n];
    a[0] = 0.0;
    b[0] = 2.0;
    r[0] = values[0] + 2.0 * values[1];
    for i in 1..n - 1 {
        a[i] = 1.0;
        b[i] = 4.0;
        r[i] = 4.0 * values[i] + 2.0 * values[i + 1];
    }
    a[n - 1] = 2.0;
    b[n - 1] = 7.0;
    r[n - 1] = 8.0 * values[n - 1] + values[n];
    for i in 1..n {
        let m = a[i] / b[i - 1];
        b[i] -= m;
        r[i] -= m * r[i - 1];
    }
    a[n - 1] = r[n - 1] / b[n - 1];
    for i in (0..n - 1).rev() {
        a[i] = (r[i] - a[i + 1]) / b[i];
    }
    let mut second = vec![0.0; n];
    second[n - 1] = (values[n] + a[n - 1]) / 2.0;
    for i in 0..n - 1 {
        second[i] = 2.0 * values[i + 1] - a[i + 1];
    }
    (a, second)
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

/// The outline an edge clips against, as `insertEdge` receives `startNode`/`endNode`.
/// Upstream node coordinates are the node centre.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeOutline {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub shape: NodeShape,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NodeShape {
    #[default]
    Rect,
    Ellipse,
}

impl NodeOutline {
    pub const fn rect(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
            shape: NodeShape::Rect,
        }
    }

    pub const fn ellipse(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
            shape: NodeShape::Ellipse,
        }
    }

    /// The top-left rectangle [`Rect`] with the same geometry.
    pub fn bounds(self) -> Rect {
        Rect::new(
            self.x - self.width / 2.0,
            self.y - self.height / 2.0,
            self.width,
            self.height,
        )
    }

    pub fn from_bounds(bounds: Rect) -> Self {
        Self::rect(
            bounds.x + bounds.width / 2.0,
            bounds.y + bounds.height / 2.0,
            bounds.width,
            bounds.height,
        )
    }

    /// The shape's attachment point, matching `intersect-rect.js`; unlike the crate's generic
    /// rectangle helper this keeps the upstream right-face answer for a centre query.
    pub fn intersect(self, point: Point) -> Point {
        match self.shape {
            NodeShape::Rect => {
                let dx = point.x - self.x;
                let dy = point.y - self.y;
                let mut w = self.width / 2.0;
                let mut h = self.height / 2.0;
                if dy.abs() * w > dx.abs() * h {
                    if dy < 0.0 {
                        h = -h;
                    }
                    let sx = if dy == 0.0 { 0.0 } else { (h * dx) / dy };
                    Point::new(self.x + sx, self.y + h)
                } else {
                    if dx < 0.0 {
                        w = -w;
                    }
                    let sy = if dx == 0.0 { 0.0 } else { (w * dy) / dx };
                    Point::new(self.x + w, self.y + sy)
                }
            }
            NodeShape::Ellipse => intersect_ellipse(
                Point::new(self.x, self.y),
                self.width / 2.0,
                self.height / 2.0,
                point,
            ),
        }
    }
}

/// Which layout drew the edge. Only `swimlane` uses the axis-preserving docking rules.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EdgeLayout {
    #[default]
    Dagre,
    Swimlane,
}

impl EdgeLayout {
    pub fn from_name(name: &str) -> Self {
        match name {
            "swimlane" => Self::Swimlane,
            _ => Self::Dagre,
        }
    }
}

fn shares_column(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() < AXIS_TOLERANCE
}

fn shares_row(a: Point, b: Point) -> bool {
    (a.y - b.y).abs() < AXIS_TOLERANCE
}

fn coincides(a: Point, b: Point) -> bool {
    shares_column(a, b) && shares_row(a, b)
}

/// `keepsArrivalAxis`: the shape intersection stays on the axis the final segment ran along.
fn keeps_arrival_axis(original_last: Point, last_inner: Point, candidate: Point) -> bool {
    if shares_column(original_last, last_inner) && !shares_row(original_last, last_inner) {
        return shares_column(candidate, last_inner);
    }
    if shares_row(original_last, last_inner) && !shares_column(original_last, last_inner) {
        return shares_row(candidate, last_inner);
    }
    shares_column(candidate, last_inner) || shares_row(candidate, last_inner)
}

/// `arrivalSegment`: the last segment the layout drew into the node.
fn arrival_segment(original_last: Point, last_inner: Point, before: Point) -> (Point, Point) {
    if coincides(original_last, last_inner) {
        (last_inner, before)
    } else {
        (original_last, last_inner)
    }
}

fn runs_against_heading(heading: Point, from: Point, last_inner: Point, candidate: Point) -> bool {
    (heading.x - from.x) * (candidate.x - last_inner.x)
        + (heading.y - from.y) * (candidate.y - last_inner.y)
        < 0.0
}

fn snap_to_arrival_axis(heading: Point, from: Point, last_inner: Point, candidate: Point) -> Point {
    if shares_row(heading, from) && !shares_column(heading, from) {
        return Point::new(candidate.x, last_inner.y);
    }
    if shares_column(heading, from) && !shares_row(heading, from) {
        return Point::new(last_inner.x, candidate.y);
    }
    candidate
}

/// `dockFinalPoint`: docks a swimlane edge on its head shape on the axis it arrived on.
fn dock_final_point(
    original_last: Point,
    last_inner: Point,
    before: Point,
    candidate: Point,
) -> Point {
    let was_orthogonal =
        shares_column(original_last, last_inner) || shares_row(original_last, last_inner);
    if was_orthogonal && !keeps_arrival_axis(original_last, last_inner, candidate) {
        return original_last;
    }
    let (heading, from) = arrival_segment(original_last, last_inner, before);
    if runs_against_heading(heading, from, last_inner, candidate) {
        return last_inner;
    }
    snap_to_arrival_axis(heading, from, last_inner, candidate)
}

/// `finalDockPoint`: drops a final segment that would be drawn backwards.
fn final_dock_point(
    original_last: Point,
    last_inner: Point,
    new_last: Option<Point>,
    min_length: f64,
) -> Option<Point> {
    let new_last = new_last?;
    let turned = !coincides(original_last, last_inner);
    let length = new_last.distance_to(last_inner);
    if length >= AXIS_TOLERANCE.max(min_length) {
        return Some(new_last);
    }
    (turned && length >= AXIS_TOLERANCE).then_some(original_last)
}

/// Swimlanes' edge-label waypoint helper; a no-op for every other layout and edge.
fn orthogonalize_to_label_clipped_points(edge: &EdgeData, points: Vec<Point>) -> Vec<Point> {
    if !edge.is_label_edge || !edge.id.ends_with("-to-label") || points.len() != 2 {
        return points;
    }
    let (start, end) = (points[0], points[1]);
    let dx = (end.x - start.x).abs();
    let dy = (end.y - start.y).abs();
    if dx < 1e-3 || dy < 1e-3 {
        return points;
    }
    if dy >= dx {
        vec![start, Point::new(start.x, end.y), end]
    } else {
        vec![start, Point::new(end.x, start.y), end]
    }
}

/// `outsideNode`: true when `point` is on or beyond the node's bounding box.
fn outside_node(node: NodeOutline, point: Point) -> bool {
    (point.x - node.x).abs() >= node.width / 2.0 || (point.y - node.y).abs() >= node.height / 2.0
}

/// `intersection` from `edges.js`: where the ray from inside to outside leaves the node box.
pub fn intersection(node: NodeOutline, outside_point: Point, inside_point: Point) -> Point {
    let (x, y) = (node.x, node.y);
    let w = node.width / 2.0;
    let h = node.height / 2.0;
    let q = (outside_point.y - inside_point.y).abs();
    let r_abs = (outside_point.x - inside_point.x).abs();
    if (y - outside_point.y).abs() * w > (x - outside_point.x).abs() * h {
        // Top or bottom of the rectangle.
        let local_q = if inside_point.y < outside_point.y {
            outside_point.y - h - y
        } else {
            y - h - outside_point.y
        };
        let r = (r_abs * local_q) / q;
        let mut result = Point::new(
            if inside_point.x < outside_point.x {
                inside_point.x + r
            } else {
                inside_point.x - r_abs + r
            },
            if inside_point.y < outside_point.y {
                inside_point.y + q - local_q
            } else {
                inside_point.y - q + local_q
            },
        );
        if r == 0.0 {
            result = outside_point;
        }
        if r_abs == 0.0 {
            result.x = outside_point.x;
        }
        if q == 0.0 {
            result.y = outside_point.y;
        }
        result
    } else {
        // Sides of the rectangle.
        let r = if inside_point.x < outside_point.x {
            outside_point.x - w - x
        } else {
            x - w - outside_point.x
        };
        let local_q = (q * r) / r_abs;
        let mut px = if inside_point.x < outside_point.x {
            inside_point.x + r_abs - r
        } else {
            inside_point.x - r_abs + r
        };
        let mut py = if inside_point.y < outside_point.y {
            inside_point.y + local_q
        } else {
            inside_point.y - local_q
        };
        if r == 0.0 {
            px = outside_point.x;
            py = outside_point.y;
        }
        if r_abs == 0.0 {
            px = outside_point.x;
        }
        if q == 0.0 {
            py = outside_point.y;
        }
        Point::new(px, py)
    }
}

/// `cutPathAtIntersect`: keep the outside prefix and stop at the boundary intersection.
pub fn cut_path_at_intersect(points: &[Point], node: NodeOutline) -> Vec<Point> {
    let Some(&first) = points.first() else {
        return Vec::new();
    };
    let mut result: Vec<Point> = Vec::new();
    let mut last_outside = first;
    let mut is_inside = false;
    for &point in points {
        if !outside_node(node, point) && !is_inside {
            let hit = intersection(node, last_outside, point);
            if !result.iter().any(|p| p.x == hit.x && p.y == hit.y) {
                result.push(hit);
            }
            is_inside = true;
        } else {
            last_outside = point;
            if !is_inside {
                result.push(point);
            }
        }
    }
    result
}

/// Clips a dagre route the way `insertEdge` does: keep the inner bends and meet both outlines.
fn clip_dagre_points(
    points: &[Point],
    tail: NodeOutline,
    head: NodeOutline,
) -> Result<Vec<Point>, ParseError> {
    if points.len() < 3 {
        // Upstream reads `points[0]` of an empty inner slice and throws a TypeError.
        return Err(ParseError::new(
            "Cannot read properties of undefined (reading 'x')",
            None,
        ));
    }
    let inner = &points[1..points.len() - 1];
    let mut result = Vec::with_capacity(inner.len() + 2);
    result.push(tail.intersect(inner[0]));
    result.extend_from_slice(inner);
    result.push(head.intersect(inner[inner.len() - 1]));
    Ok(result)
}

/// Clips a swimlane route with its axis-preserving docking rules.
fn clip_swimlane_points(
    points: &[Point],
    tail: NodeOutline,
    head: NodeOutline,
    arrow_type_end: Option<&str>,
) -> Vec<Point> {
    if points.len() == 2 {
        let (first, last) = (points[0], points[1]);
        return if shares_column(first, last) || shares_row(first, last) {
            vec![tail.intersect(first), head.intersect(last)]
        } else {
            vec![tail.intersect(last), head.intersect(first)]
        };
    }
    if points.len() < 3 {
        return points.to_vec();
    }
    let inner = &points[1..points.len() - 1];
    let first_inner = inner[0];
    let last_inner = inner[inner.len() - 1];
    let new_first = tail.intersect(first_inner);
    let original_last = *points.last().unwrap();
    let candidate_last = head.intersect(last_inner);
    let before = inner
        .get(inner.len().wrapping_sub(2))
        .copied()
        .unwrap_or(points[0]);
    let new_last = dock_final_point(original_last, last_inner, before, candidate_last);
    let final_point = final_dock_point(
        original_last,
        last_inner,
        Some(new_last),
        marker_offset(arrow_type_end.unwrap_or_default()).unwrap_or(0.0),
    );
    let first_is_duplicate = (new_first.x - first_inner.x).abs() < AXIS_TOLERANCE
        && (new_first.y - first_inner.y).abs() < AXIS_TOLERANCE;
    let mut result = Vec::with_capacity(inner.len() + 2);
    if !first_is_duplicate {
        result.push(new_first);
    }
    result.extend_from_slice(inner);
    if let Some(point) = final_point {
        result.push(point);
    }
    result
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
    cut_path_at_intersect(points, NodeOutline::from_bounds(bounds))
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

/// Accepts upstream's space-separated `classes` string as well as a list.
fn deserialize_classes<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Classes {
        One(String),
        Many(Vec<String>),
    }
    Ok(match Classes::deserialize(deserializer)? {
        Classes::One(value) => value.split_whitespace().map(str::to_string).collect(),
        Classes::Many(values) => values,
    })
}

/// Input retained from Mermaid's `EdgeData`; measurements are supplied by the caller because
/// this crate has no DOM or font engine.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeData {
    pub id: String,
    pub points: Vec<Point>,
    pub label: Option<String>,
    /// Measured text box of the label, before the `BACKGROUND_PADDING` the background rect adds.
    #[serde(default)]
    pub label_width: f64,
    #[serde(default)]
    pub label_height: f64,
    /// Dagre's label centre (`edge.x`/`edge.y` in `positionEdgeLabel`).
    #[serde(default)]
    pub label_x: Option<f64>,
    #[serde(default)]
    pub label_y: Option<f64>,
    /// Measured terminal label sizes, keyed by `startLeft`, `startRight`, `endLeft`, `endRight`.
    #[serde(default)]
    pub terminal_label_sizes: BTreeMap<String, [f64; 2]>,
    pub start_label_left: Option<String>,
    pub start_label_right: Option<String>,
    pub end_label_left: Option<String>,
    pub end_label_right: Option<String>,
    pub curve: Option<String>,
    #[serde(default)]
    pub thickness: String,
    #[serde(default)]
    pub pattern: String,
    pub animation: Option<String>,
    #[serde(default)]
    pub animate: bool,
    #[serde(default, deserialize_with = "deserialize_classes")]
    pub classes: Vec<String>,
    #[serde(default, alias = "style")]
    pub styles: Vec<String>,
    #[serde(default, rename = "cssCompiledStyleList")]
    pub css_compiled_styles: Vec<String>,
    /// `edge.cssCompiledStyles`, keyed by CSS property; label properties are excluded from the
    /// path's class styles by `isLabelStyle`.
    #[serde(default, rename = "cssCompiledStyles")]
    pub css_compiled_style_map: BTreeMap<String, String>,
    /// `edge.look`: `classic`, `neo`, or `handDrawn` (hand-drawn geometry is not ported).
    #[serde(default)]
    pub look: Option<String>,
    pub arrow_type_start: Option<String>,
    pub arrow_type_end: Option<String>,
    pub corner_radius: Option<f64>,
    #[serde(default)]
    pub skip_corner_fix: bool,
    #[serde(default)]
    pub skip_intersect: bool,
    /// `layoutAlgorithm ?? config.layout`; only `swimlane` changes the clipping rules.
    #[serde(default)]
    pub layout: Option<String>,
    #[serde(default)]
    pub port_clipping: Option<String>,
    #[serde(default)]
    pub is_label_edge: bool,
    /// `pathNode.getTotalLength()`, measured by the consumer. Zero when unmeasured, which is
    /// how `neo` dash arrays fall back in a DOM without measurement.
    #[serde(default)]
    pub path_length: Option<f64>,
    /// Node outlines used by the layout's `startNode`/`endNode` intersection.
    #[serde(default)]
    pub from_node: Option<NodeOutline>,
    #[serde(default)]
    pub to_node: Option<NodeOutline>,
    pub to_cluster: Option<Rect>,
    pub from_cluster: Option<Rect>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EdgeLabelScene {
    pub text: String,
    pub position: EdgeLabelPosition,
    pub role: String,
    /// The background `<rect>` of `createFormattedText` (`addSvgBackground`), in absolute
    /// coordinates: the measured text box plus `BACKGROUND_PADDING` on every side.
    #[serde(default)]
    pub background: Option<Rect>,
    /// `computeLabelTransform`'s translate for the inner label group.
    #[serde(default)]
    pub transform: Point,
}

/// `createFormattedText`'s background padding, in pixels.
pub const BACKGROUND_PADDING: f64 = 2.0;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InsertedEdge {
    pub id: String,
    pub path: Path,
    pub points: Vec<Point>,
    pub roles: Vec<String>,
    pub styles: Vec<String>,
    pub labels: Vec<EdgeLabelScene>,
    pub curve: Curve,
    /// `data-points`: base64 of `JSON.stringify(points)`.
    #[serde(default)]
    pub data_points: String,
    /// The exact value of upstream's `class` attribute, leading space included.
    #[serde(default)]
    pub class_attribute: String,
    /// The exact value of upstream's `style` attribute.
    #[serde(default)]
    pub style_attribute: String,
    /// `data-look`, where a missing look stringifies as `undefined`.
    #[serde(default)]
    pub data_look: String,
    /// `data-edge` (always true) and `data-et` (always `edge`) from `insertEdge`.
    #[serde(default)]
    pub data_edge: bool,
    #[serde(default)]
    pub data_et: String,
    /// `data-id`: the edge id without the diagram prefix.
    #[serde(default)]
    pub data_id: String,
    /// `marker-start`/`marker-end` attribute values (`url(#…)`).
    #[serde(default)]
    pub marker_start: Option<String>,
    #[serde(default)]
    pub marker_end: Option<String>,
    #[serde(default)]
    pub animated: bool,
    /// `paths.updatedPath`: present when clipping or corner fixing moved the route.
    #[serde(default)]
    pub updated_path: Option<Vec<Point>>,
    /// `paths.originalPath`: the layout's points before any clipping.
    #[serde(default)]
    pub original_path: Vec<Point>,
}

/// `hasEdgeLabel`: an edge has label content when any of its five label slots is non-empty.
pub fn has_edge_label(edge: &EdgeData) -> bool {
    [
        &edge.label,
        &edge.start_label_left,
        &edge.start_label_right,
        &edge.end_label_left,
        &edge.end_label_right,
    ]
    .into_iter()
    .flatten()
    .any(|label| !label.is_empty())
}

/// `getLabelStyles`: upstream joins a style array with leading semicolons.
pub fn edge_label_styles(style_array: Option<&[String]>) -> String {
    style_array
        .map(|styles| styles.iter().map(|style| format!(";{style}")).collect())
        .unwrap_or_default()
}

/// `calcTerminalLabelPosition` is called with 10 when that end owns any arrow type at all.
fn terminal_marker_size(edge: &EdgeData, position: TerminalPosition) -> f64 {
    let arrow = if matches!(
        position,
        TerminalPosition::StartLeft | TerminalPosition::StartRight
    ) {
        &edge.arrow_type_start
    } else {
        &edge.arrow_type_end
    };
    match arrow {
        Some(arrow) if !arrow.is_empty() => 10.0,
        _ => 0.0,
    }
}

/// `computeLabelTransform`: HTML labels ignore the viewport-absolute bbox origin.
fn label_transform(x: f64, y: f64, width: f64, height: f64, html: bool) -> Point {
    if html {
        Point::new(-width / 2.0, -height / 2.0)
    } else {
        Point::new(-(x + width / 2.0), -(y + height / 2.0))
    }
}

/// The background `<rect>` `createFormattedText` draws behind an edge label.
pub fn label_background(bounds: Rect) -> Rect {
    Rect::new(
        bounds.x - BACKGROUND_PADDING,
        bounds.y - BACKGROUND_PADDING,
        bounds.width + 2.0 * BACKGROUND_PADDING,
        bounds.height + 2.0 * BACKGROUND_PADDING,
    )
}

/// `setTerminalWidth`: the HTML terminal label box never clips the measured text and keeps
/// nine pixels per character of room.
pub fn terminal_label_box(value: &str, measured: (f64, f64)) -> (f64, f64) {
    (
        (value.chars().count() as f64 * 9.0).max(measured.0),
        measured.1,
    )
}

fn label_scene(
    text: &str,
    center: Point,
    size: (f64, f64),
    role: &str,
    background: bool,
) -> EdgeLabelScene {
    let bounds = Rect::new(
        center.x - size.0 / 2.0,
        center.y - size.1 / 2.0,
        size.0,
        size.1,
    );
    EdgeLabelScene {
        text: text.to_string(),
        position: EdgeLabelPosition { center, bounds },
        role: role.to_string(),
        background: background.then(|| label_background(bounds)),
        transform: label_transform(0.0, 0.0, size.0, size.1, false),
    }
}

/// Creates scene data for the main and terminal edge labels; unlike upstream DOM insertion,
/// text shaping and HTML/SVG selection are deliberately delegated to the scene consumer.
pub fn insert_edge_label(edge: &EdgeData) -> Vec<EdgeLabelScene> {
    let mut labels = Vec::new();
    if let Some(text) = edge.label.as_ref().filter(|s| !s.is_empty()) {
        let center = match (edge.label_x, edge.label_y) {
            (Some(x), Some(y)) => Some(Point::new(x, y)),
            _ => position_edge_label(&edge.points, edge.label_width, edge.label_height)
                .map(|position| position.center),
        };
        if let Some(center) = center {
            labels.push(label_scene(
                text,
                center,
                (edge.label_width, edge.label_height),
                "edgeLabel",
                true,
            ));
        }
    }
    for (text, position) in [
        (&edge.start_label_left, TerminalPosition::StartLeft),
        (&edge.start_label_right, TerminalPosition::StartRight),
        (&edge.end_label_left, TerminalPosition::EndLeft),
        (&edge.end_label_right, TerminalPosition::EndRight),
    ] {
        if let Some(text) = text.as_ref().filter(|s| !s.is_empty()) {
            let marker_size = terminal_marker_size(edge, position);
            if let Some(center) = terminal_label_position(&edge.points, position, marker_size) {
                let key = match position {
                    TerminalPosition::StartLeft => "startLeft",
                    TerminalPosition::StartRight => "startRight",
                    TerminalPosition::EndLeft => "endLeft",
                    TerminalPosition::EndRight => "endRight",
                };
                let size = edge
                    .terminal_label_sizes
                    .get(key)
                    .map(|size| (size[0], size[1]))
                    .unwrap_or((0.0, 0.0));
                labels.push(label_scene(text, center, size, "edgeTerminals", false));
            }
        }
    }
    labels
}

/// Resolves Mermaid's marker pull-back table used by `lineWithOffset.ts`.
pub fn marker_offset(kind: &str) -> Option<f64> {
    match kind {
        "aggregation" | "extension" | "composition" => Some(17.25),
        "dependency" => Some(6.0),
        "lollipop" => Some(13.5),
        "arrow_point" => Some(4.0),
        "arrow_barb" => Some(0.0),
        "arrow_barb_neo" => Some(5.5),
        _ => None,
    }
}

/// `markerOffsets2`, used only by the `neo` look's dash array.
pub fn marker_offset2(kind: &str) -> Option<f64> {
    match kind {
        "arrow_point" => Some(4.0),
        "arrow_cross" | "arrow_circle" => Some(12.5),
        "arrow_open_head" => Some(4.0),
        "arrow_hollow_circle" => Some(8.0),
        _ => None,
    }
}

/// `resolveEdgeLabelPosition`: a label follows the midpoint shift between the two paths.
pub fn resolve_edge_label_position(
    edge: &EdgeData,
    updated_path: Option<&[Point]>,
    original_path: Option<&[Point]>,
) -> Point {
    let fallback = position_edge_label(&edge.points, 0.0, 0.0)
        .map(|position| position.center)
        .unwrap_or(Point::ZERO);
    let mut position = Point::new(
        edge.label_x.unwrap_or(fallback.x),
        edge.label_y.unwrap_or(fallback.y),
    );
    if let Some(updated) = updated_path {
        let updated_mid = calculate_label_position(updated);
        if let Some(original) = original_path {
            let original_mid = calculate_label_position(original);
            position.x = edge.label_x.unwrap_or(fallback.x) + (updated_mid.x - original_mid.x);
            position.y = edge.label_y.unwrap_or(fallback.y) + (updated_mid.y - original_mid.y);
        } else {
            position = updated_mid;
        }
    }
    position
}

/// `utils.calcLabelPosition`: a single point is its own label position.
pub fn calculate_label_position(points: &[Point]) -> Point {
    match points {
        [] => Point::ZERO,
        [point] => *point,
        _ => {
            let distance = points
                .windows(2)
                .map(|pair| pair[0].distance_to(pair[1]))
                .sum::<f64>()
                / 2.0;
            calculate_point(points, distance).unwrap_or(Point::ZERO)
        }
    }
}

/// Positions labels from the final rendered geometry, retaining the layout position when no
/// path exists. Main labels follow `resolveEdgeLabelPosition`; terminals follow the final path.
pub fn position_edge_labels(
    edge: &EdgeData,
    final_points: Option<&[Point]>,
) -> Vec<EdgeLabelScene> {
    let mut positioned = edge.clone();
    if let Some(points) = final_points {
        positioned.points = points.to_vec();
    }
    let path = final_points.unwrap_or(&edge.points);
    let original = edge.points.as_slice();
    let main_center =
        resolve_edge_label_position(edge, final_points, final_points.map(|_| original));
    let mut labels = Vec::new();
    if let Some(text) = edge.label.as_ref().filter(|s| !s.is_empty()) {
        labels.push(label_scene(
            text,
            main_center,
            (edge.label_width, edge.label_height),
            "edgeLabel",
            true,
        ));
    }
    for (text, position) in [
        (&edge.start_label_left, TerminalPosition::StartLeft),
        (&edge.start_label_right, TerminalPosition::StartRight),
        (&edge.end_label_left, TerminalPosition::EndLeft),
        (&edge.end_label_right, TerminalPosition::EndRight),
    ] {
        if let Some(text) = text.as_ref().filter(|s| !s.is_empty()) {
            let marker_size = terminal_marker_size(edge, position);
            if let Some(center) = terminal_label_position(path, position, marker_size) {
                labels.push(label_scene(
                    text,
                    center,
                    (0.0, 0.0),
                    "edgeTerminals",
                    false,
                ));
            }
        }
    }
    labels
}

fn is_label_style(key: &str) -> bool {
    matches!(
        key,
        "color"
            | "font-size"
            | "font-family"
            | "font-weight"
            | "font-style"
            | "text-decoration"
            | "text-align"
            | "text-transform"
            | "line-height"
            | "letter-spacing"
            | "word-spacing"
            | "text-shadow"
            | "text-overflow"
            | "white-space"
            | "word-wrap"
            | "word-break"
            | "overflow-wrap"
            | "hyphens"
    )
}

/// The non-label values of `edge.cssCompiledStyles`, in declaration order.
fn class_style_values(edge: &EdgeData) -> Vec<String> {
    let mut values: Vec<String> = edge
        .css_compiled_style_map
        .iter()
        .filter(|(key, _)| !is_label_style(key))
        .map(|(_, value)| value.clone())
        .collect();
    values.extend(edge.css_compiled_styles.iter().cloned());
    values
}

/// Upstream's `style` attribute assembly, including its repeated separators.
pub fn path_style_attribute(edge: &EdgeData) -> String {
    let styles_from_classes = class_style_values(edge).join(";");
    let styles = edge
        .styles
        .iter()
        .map(|style| format!("{style};"))
        .collect::<String>();
    let head = if styles_from_classes.is_empty() {
        styles.clone()
    } else {
        format!("{styles_from_classes};{styles};")
    };
    let tail = edge
        .styles
        .iter()
        .map(|style| format!(";{style}"))
        .collect::<String>();
    format!("{head};{tail}")
}

/// `animatedEdge`: an explicit animation, `animate`, or an `animation` class style.
pub fn is_animated_edge(edge: &EdgeData) -> bool {
    edge.animate
        || edge.animation.is_some()
        || class_style_values(edge).join(";").contains("animation")
}

/// The `class` attribute from `insertEdge`, leading space included.
pub fn edge_class_attribute(edge: &EdgeData) -> String {
    let mut animation_class = String::new();
    if edge.animate {
        animation_class = "edge-animation-fast".into();
    }
    if let Some(animation) = edge.animation.as_deref() {
        animation_class = format!("edge-animation-{animation}");
    }
    let mut class = format!(
        " edge-thickness-{} edge-pattern-{}",
        match edge.thickness.as_str() {
            "thick" => "thick",
            "invisible" => "invisible",
            _ => "normal",
        },
        match edge.pattern.as_str() {
            "dotted" => "dotted",
            "dashed" => "dashed",
            _ => "solid",
        }
    );
    for class_name in &edge.classes {
        class.push(' ');
        class.push_str(class_name);
    }
    if !animation_class.is_empty() {
        class.push(' ');
        class.push_str(&animation_class);
    }
    class
}

/// `strokeColor` as `insertEdge` reads it back from the `style` attribute.
pub fn stroke_color(edge: &EdgeData) -> Option<String> {
    let style = path_style_attribute(edge);
    let start = style.find("stroke:")? + "stroke:".len();
    let value = style[start..].split(';').next().unwrap_or_default();
    (!value.trim().is_empty()).then(|| value.to_string())
}

fn marker_margin_supported(kind: MarkerKind) -> bool {
    matches!(
        kind,
        MarkerKind::Cross
            | MarkerKind::Point
            | MarkerKind::Circle
            | MarkerKind::Lollipop
            | MarkerKind::Aggregation
            | MarkerKind::Extension
            | MarkerKind::Composition
            | MarkerKind::Dependency
            | MarkerKind::Barb
            | MarkerKind::BarbNeo
    )
}

/// `addEdgeMarker`'s `marker-start`/`marker-end` value, including the `-margin` and coloured
/// id variants and the diagram-type suffix.
pub fn edge_marker_attribute(
    arrow_type: Option<&str>,
    at_start: bool,
    margin: bool,
    diagram_id: &str,
    diagram_type: &str,
    stroke_color: Option<&str>,
) -> Option<String> {
    let arrow_type = arrow_type?;
    if arrow_type.is_empty() || arrow_type == "none" {
        return None;
    }
    let Some(kind) = MarkerKind::from_arrow_type(arrow_type) else {
        // `arrowTypesMap` is a plain object upstream: prototype keys resolve to a truthy
        // value with no `type`, which renders a marker id with an `undefined` stem.
        if !is_js_prototype_key(arrow_type) {
            return None;
        }
        let id = format!(
            "{diagram_id}_{diagram_type}-undefined{}",
            if at_start { "Start" } else { "End" }
        );
        return Some(format!("url(#{})", colored_marker_id(id, stroke_color)));
    };
    let margin = margin && marker_margin_supported(kind);
    let placement = if at_start {
        marker_placement(Some(arrow_type), None, diagram_id, diagram_type, margin).ok()?
    } else {
        marker_placement(None, Some(arrow_type), diagram_id, diagram_type, margin).ok()?
    };
    let definition = if at_start {
        placement.start?
    } else {
        placement.end?
    };
    Some(format!(
        "url(#{})",
        colored_marker_id(definition.def, stroke_color)
    ))
}

/// JS property names that `arrowTypesMap[arrowType]` finds on `Object.prototype`.
fn is_js_prototype_key(name: &str) -> bool {
    matches!(
        name,
        "constructor"
            | "__proto__"
            | "toString"
            | "toLocaleString"
            | "valueOf"
            | "hasOwnProperty"
            | "isPrototypeOf"
            | "propertyIsEnumerable"
            | "__defineGetter__"
            | "__defineSetter__"
            | "__lookupGetter__"
            | "__lookupSetter__"
    )
}

/// A coloured marker clones the original and appends a sanitized colour to its id.
fn colored_marker_id(id: String, stroke_color: Option<&str>) -> String {
    match stroke_color.filter(|color| !color.trim().is_empty()) {
        Some(color) => {
            let suffix: String = color
                .chars()
                .map(|character| {
                    if character.is_ascii_alphanumeric() {
                        character
                    } else {
                        '_'
                    }
                })
                .collect();
            format!("{id}_{suffix}")
        }
        None => id,
    }
}

/// `generateDashArray` for dotted/dashed `neo` edges.
pub fn generate_dash_array(length: f64, start_offset: f64, end_offset: f64) -> String {
    let middle_length = length - start_offset - end_offset;
    let raw_pairs = (middle_length / 4.0).floor();
    let pairs = if raw_pairs.is_finite() {
        raw_pairs.max(0.0) as usize
    } else {
        0
    };
    let middle = std::iter::repeat_n("2 2", pairs)
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "0 {} {} {}",
        js_number(start_offset),
        middle,
        js_number(end_offset)
    )
}

/// The `stroke-dasharray` value `neo` prepends to the style attribute.
pub fn neo_dash_array(length: f64, pattern: &str, start_offset: f64, end_offset: f64) -> String {
    if matches!(pattern, "dotted" | "dashed") {
        generate_dash_array(length, start_offset, end_offset)
    } else {
        format!(
            "0 {} {} {}",
            js_number(start_offset),
            js_number(length - start_offset - end_offset),
            js_number(end_offset)
        )
    }
}

/// `fmt` from `lineJump.ts`: round to three decimals, then print like a JS number.
pub fn js_number(value: f64) -> String {
    let rounded = js_round(value * 1000.0) / 1000.0;
    if rounded == 0.0 {
        "0".into()
    } else {
        format!("{rounded}")
    }
}

/// `Math.round`, which rounds halves toward positive infinity.
fn js_round(value: f64) -> f64 {
    (value + 0.5).floor()
}

/// `btoa(JSON.stringify(points))` for the `data-points` attribute.
pub fn data_points_attribute(points: &[Point]) -> String {
    let json = format!(
        "[{}]",
        points
            .iter()
            .map(|point| format!(
                "{{\"x\":{},\"y\":{}}}",
                js_number(point.x),
                js_number(point.y)
            ))
            .collect::<Vec<_>>()
            .join(",")
    );
    base64_encode(json.as_bytes())
}

/// `decodeDataPoints` from `lineJump.ts`: the route encoded in `data-points`, if valid.
pub fn decode_data_points(raw: &str) -> Option<Vec<Point>> {
    let bytes = base64_decode(raw)?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let array = value.as_array()?;
    let mut points = Vec::with_capacity(array.len());
    for point in array {
        points.push(Point::new(
            point.get("x")?.as_f64()?,
            point.get("y")?.as_f64()?,
        ));
    }
    (points.len() >= 2).then_some(points)
}

fn base64_decode(value: &str) -> Option<Vec<u8>> {
    fn index(character: u8) -> Option<u8> {
        Some(match character {
            b'A'..=b'Z' => character - b'A',
            b'a'..=b'z' => character - b'a' + 26,
            b'0'..=b'9' => character - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        })
    }
    let cleaned: Vec<u8> = value
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    let mut output = Vec::with_capacity(cleaned.len() / 4 * 3);
    for chunk in cleaned.chunks(4) {
        if chunk.len() < 2 {
            return None;
        }
        let mut buffer = [0u8; 4];
        let mut valid = 0;
        for (position, &byte) in chunk.iter().enumerate() {
            if byte == b'=' {
                continue;
            }
            buffer[position] = index(byte)?;
            valid += 1;
        }
        let value = (u32::from(buffer[0]) << 18)
            | (u32::from(buffer[1]) << 12)
            | (u32::from(buffer[2]) << 6)
            | u32::from(buffer[3]);
        output.push((value >> 16) as u8);
        if valid > 2 {
            output.push((value >> 8) as u8);
        }
        if valid > 3 {
            output.push(value as u8);
        }
    }
    Some(output)
}

fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let value = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        output.push(ALPHABET[(value >> 18) as usize & 63] as char);
        output.push(ALPHABET[(value >> 12) as usize & 63] as char);
        output.push(if chunk.len() > 1 {
            ALPHABET[(value >> 6) as usize & 63] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            ALPHABET[value as usize & 63] as char
        } else {
            '='
        });
    }
    output
}

/// SVG path data for a scene path, in the form upstream reads from the `d` attribute.
pub fn path_data(path: &Path) -> String {
    let mut result = String::new();
    for command in &path.commands {
        match *command {
            PathCommand::MoveTo { x, y } => {
                result.push_str(&format!("M{},{} ", js_number(x), js_number(y)))
            }
            PathCommand::LineTo { x, y } => {
                result.push_str(&format!("L{},{} ", js_number(x), js_number(y)))
            }
            PathCommand::QuadTo { cx, cy, x, y } => result.push_str(&format!(
                "Q{},{} {},{} ",
                js_number(cx),
                js_number(cy),
                js_number(x),
                js_number(y)
            )),
            PathCommand::CubicTo {
                c1x,
                c1y,
                c2x,
                c2y,
                x,
                y,
            } => result.push_str(&format!(
                "C{},{} {},{} {},{} ",
                js_number(c1x),
                js_number(c1y),
                js_number(c2x),
                js_number(c2y),
                js_number(x),
                js_number(y)
            )),
            PathCommand::ArcTo {
                rx,
                ry,
                sweep,
                x,
                y,
                ..
            } => result.push_str(&format!(
                "A{},{} 0 0 {} {},{} ",
                js_number(rx),
                js_number(ry),
                u8::from(sweep),
                js_number(x),
                js_number(y)
            )),
            PathCommand::Close => result.push_str("Z "),
        }
    }
    result.trim_end().to_string()
}

/// `isLabelCoordinateInPath`: whether either rounded coordinate appears in the path data.
pub fn is_label_coordinate_in_path(point: Point, path: &Path) -> bool {
    let data = path_data(path);
    let x = js_round(point.x).to_string();
    let y = js_round(point.y).to_string();
    data.contains(&x) || data.contains(&y)
}

/// `applyMarkerOffsetsToPoints`: the rounded-path marker pull-back.
pub fn apply_marker_offsets_to_points(points: &[Point], edge: &EdgeData) -> Vec<Point> {
    let mut new_points = points.to_vec();
    if points.len() < 2 {
        return new_points;
    }
    if let Some(offset) = edge.arrow_type_start.as_deref().and_then(marker_offset) {
        let (a, b) = (points[0], points[1]);
        let angle = (b.y - a.y).atan2(b.x - a.x);
        new_points[0] = Point::new(a.x + offset * angle.cos(), a.y + offset * angle.sin());
    }
    if let Some(offset) = edge.arrow_type_end.as_deref().and_then(marker_offset) {
        let n = points.len();
        let (a, b) = (points[n - 2], points[n - 1]);
        let angle = (b.y - a.y).atan2(b.x - a.x);
        new_points[n - 1] = Point::new(b.x - offset * angle.cos(), b.y - offset * angle.sin());
    }
    new_points
}

/// `getLineFunctionsWithOffset`: d3's x/y accessors, which also nudge points near a marker.
fn line_point_with_offsets(points: &[Point], index: usize, edge: &EdgeData) -> Point {
    let point = points[index];
    let last = points.len() - 1;
    let start_marker = edge.arrow_type_start.as_deref().and_then(marker_offset);
    let end_marker = edge.arrow_type_end.as_deref().and_then(marker_offset);
    let first = points[0];
    let final_point = points[last];

    let mut x_offset = 0.0;
    let direction_left = first.x < final_point.x;
    let angle = |a: Point, b: Point| ((b.y - a.y) / (b.x - a.x)).atan();
    if index == 0
        && let Some(marker) = start_marker
    {
        let angle = angle(points[0], points[1]);
        x_offset = marker
            * angle.cos()
            * if points[1].x - points[0].x >= 0.0 {
                1.0
            } else {
                -1.0
            };
    } else if index == last
        && let Some(marker) = end_marker
    {
        let angle = angle(points[last], points[last - 1]);
        x_offset = marker
            * angle.cos()
            * if points[last - 1].x - points[last].x >= 0.0 {
                1.0
            } else {
                -1.0
            };
    }
    let difference_to_end = (point.x - final_point.x).abs();
    let difference_in_y_end = (point.y - final_point.y).abs();
    let difference_to_start = (point.x - first.x).abs();
    let difference_in_y_start = (point.y - first.y).abs();
    let extra_room = 1.0;
    if let Some(marker_height) = end_marker
        && difference_to_end < marker_height
        && difference_to_end > 0.0
        && difference_in_y_end < marker_height
    {
        let mut adjustment = marker_height + extra_room - difference_to_end;
        adjustment *= if direction_left { 1.0 } else { -1.0 };
        x_offset -= adjustment;
    }
    if let Some(marker_height) = start_marker
        && difference_to_start < marker_height
        && difference_to_start > 0.0
        && difference_in_y_start < marker_height
    {
        let mut adjustment = marker_height + extra_room - difference_to_start;
        adjustment *= if direction_left { 1.0 } else { -1.0 };
        x_offset += adjustment;
    }

    let mut y_offset = 0.0;
    let direction_down = first.y < final_point.y;
    if index == 0
        && let Some(marker) = start_marker
    {
        let angle = angle(points[0], points[1]);
        y_offset = marker
            * angle.sin().abs()
            * if points[1].y - points[0].y >= 0.0 {
                1.0
            } else {
                -1.0
            };
    } else if index == last
        && let Some(marker) = end_marker
    {
        let angle = angle(points[last], points[last - 1]);
        y_offset = marker
            * angle.sin().abs()
            * if points[last - 1].y - points[last].y >= 0.0 {
                1.0
            } else {
                -1.0
            };
    }
    let difference_to_end = (point.y - final_point.y).abs();
    let difference_in_x_end = (point.x - final_point.x).abs();
    let difference_to_start = (point.y - first.y).abs();
    let difference_in_x_start = (point.x - first.x).abs();
    if let Some(marker_height) = end_marker
        && difference_to_end < marker_height
        && difference_to_end > 0.0
        && difference_in_x_end < marker_height
    {
        let mut adjustment = marker_height + extra_room - difference_to_end;
        adjustment *= if direction_down { 1.0 } else { -1.0 };
        y_offset -= adjustment;
    }
    if let Some(marker_height) = start_marker
        && difference_to_start < marker_height
        && difference_to_start > 0.0
        && difference_in_x_start < marker_height
    {
        let mut adjustment = marker_height + extra_room - difference_to_start;
        adjustment *= if direction_down { 1.0 } else { -1.0 };
        y_offset += adjustment;
    }
    Point::new(point.x + x_offset, point.y + y_offset)
}

fn line_points_with_offsets(points: &[Point], edge: &EdgeData) -> Vec<Point> {
    (0..points.len())
        .map(|index| line_point_with_offsets(points, index, edge))
        .collect()
}

/// `extractCornerPoints`: bends longer than five pixels on both sides.
fn extract_corner_points(points: &[Point]) -> Vec<usize> {
    let mut positions = Vec::new();
    for index in 1..points.len().saturating_sub(1) {
        let prev = points[index - 1];
        let curr = points[index];
        let next = points[index + 1];
        let first = prev.x == curr.x
            && curr.y == next.y
            && (curr.x - next.x).abs() > 5.0
            && (curr.y - prev.y).abs() > 5.0;
        let second = prev.y == curr.y
            && curr.x == next.x
            && (curr.x - prev.x).abs() > 5.0
            && (curr.y - next.y).abs() > 5.0;
        if first || second {
            positions.push(index);
        }
    }
    positions
}

/// `findAdjacentPoint(pointA, pointB, distance)`: `distance` back from B toward A.
fn find_adjacent_point(point_a: Point, point_b: Point, distance: f64) -> Point {
    let x_diff = point_b.x - point_a.x;
    let y_diff = point_b.y - point_a.y;
    let length = (x_diff * x_diff + y_diff * y_diff).sqrt();
    let ratio = distance / length;
    Point::new(point_b.x - ratio * x_diff, point_b.y - ratio * y_diff)
}

/// `fixCorners`: cuts five pixels off each long bend and replaces sharp corners.
pub fn fix_corners(points: &[Point]) -> Vec<Point> {
    let corner_positions = extract_corner_points(points);
    let mut result = Vec::with_capacity(points.len() + corner_positions.len() * 2);
    for (index, &point) in points.iter().enumerate() {
        if !corner_positions.contains(&index) {
            result.push(point);
            continue;
        }
        let prev = points[index - 1];
        let next = points[index + 1];
        let corner = point;
        let new_prev = find_adjacent_point(prev, corner, 5.0);
        let new_next = find_adjacent_point(next, corner, 5.0);
        let x_diff = new_next.x - new_prev.x;
        let y_diff = new_next.y - new_prev.y;
        result.push(new_prev);
        let a = std::f64::consts::SQRT_2 * 2.0;
        let mut new_corner = corner;
        if (next.x - prev.x).abs() > 10.0 && (next.y - prev.y).abs() >= 10.0 {
            let r = 5.0;
            new_corner = if corner.x == new_prev.x {
                Point::new(
                    if x_diff < 0.0 {
                        new_prev.x - r + a
                    } else {
                        new_prev.x + r - a
                    },
                    if y_diff < 0.0 {
                        new_prev.y - a
                    } else {
                        new_prev.y + a
                    },
                )
            } else {
                Point::new(
                    if x_diff < 0.0 {
                        new_prev.x - a
                    } else {
                        new_prev.x + a
                    },
                    if y_diff < 0.0 {
                        new_prev.y - r + a
                    } else {
                        new_prev.y + r - a
                    },
                )
            };
        }
        result.push(new_corner);
        result.push(new_next);
    }
    result
}

/// Builds a scene edge from Mermaid edge data, including endpoint clipping, cluster cuts,
/// corner fixing, curve, styles, markers, and labels. A missing id is an upstream `insertEdge`
/// error; a dagre route with fewer than three points is upstream's TypeError.
pub fn insert_edge(edge: &EdgeData, diagram_id: &str) -> Result<InsertedEdge, ParseError> {
    insert_edge_with_type(edge, diagram_id, "flowchart")
}

/// `insertEdge` for a concrete diagram type, which prefixes every marker id.
pub fn insert_edge_with_type(
    edge: &EdgeData,
    diagram_id: &str,
    diagram_type: &str,
) -> Result<InsertedEdge, ParseError> {
    if diagram_id.is_empty() {
        return Err(ParseError::new(
            format!(
                "insertEdge: missing diagramId for edge \"{}\" — edge IDs require a diagram prefix for uniqueness",
                edge.id
            ),
            None,
        ));
    }
    let layout = EdgeLayout::from_name(edge.layout.as_deref().unwrap_or("dagre"));
    let mut points = edge.points.clone();
    if edge.port_clipping.as_deref() == Some("outline-orthogonal")
        && !edge.skip_intersect
        && let (Some(tail), Some(head)) = (edge.from_node, edge.to_node)
    {
        points = clip_orthogonal_endpoints(&points, tail.bounds(), head.bounds());
    } else if layout == EdgeLayout::Swimlane {
        if !edge.skip_intersect
            && points.len() >= 2
            && let (Some(tail), Some(head)) = (edge.from_node, edge.to_node)
        {
            points = clip_swimlane_points(&points, tail, head, edge.arrow_type_end.as_deref());
        }
        points = orthogonalize_to_label_clipped_points(edge, points);
    } else if !edge.skip_intersect
        && let (Some(tail), Some(head)) = (edge.from_node, edge.to_node)
    {
        points = clip_dagre_points(&points, tail, head)?;
    }

    let data_points = data_points_attribute(&points);
    let mut points_changed = false;
    if let Some(bounds) = edge.to_cluster {
        points = cut_path_at_intersect(&edge.points, NodeOutline::from_bounds(bounds));
        points_changed = true;
    }
    if let Some(bounds) = edge.from_cluster {
        points.reverse();
        points = cut_path_at_intersect(&points, NodeOutline::from_bounds(bounds));
        points.reverse();
        points_changed = true;
    }

    let mut line_data: Vec<Point> = points
        .iter()
        .copied()
        .filter(|point| !point.y.is_nan())
        .collect();
    let curve = Curve::from_name(edge.curve.as_deref().unwrap_or("basis"));
    if curve != Curve::Rounded && !edge.skip_corner_fix {
        line_data = fix_corners(&line_data);
    }
    let radius = resolve_edge_corner_radius(edge.corner_radius);
    let path = if curve == Curve::Rounded {
        rounded_path(&apply_marker_offsets_to_points(&line_data, edge), radius)
    } else {
        edge_path(&line_points_with_offsets(&line_data, edge), curve, radius)
    };

    let mut roles = style_roles(
        &edge.thickness,
        &edge.pattern,
        edge.animation.as_deref(),
        edge.animate,
    );
    roles.extend(edge.classes.iter().cloned());
    roles.push("edge".into());
    let mut styles = class_style_values(edge);
    styles.extend(edge.styles.iter().cloned());

    let animated = is_animated_edge(edge);
    let use_margin = !animated && edge.look.as_deref() == Some("neo");
    let color = stroke_color(edge);
    let mut style_attribute = path_style_attribute(edge);
    if edge.look.as_deref() == Some("neo") && !animated {
        let start_offset = edge
            .arrow_type_start
            .as_deref()
            .and_then(marker_offset2)
            .unwrap_or(0.0);
        let end_offset = edge
            .arrow_type_end
            .as_deref()
            .and_then(marker_offset2)
            .unwrap_or(0.0);
        let dash = neo_dash_array(
            edge.path_length.unwrap_or(0.0),
            &edge.pattern,
            start_offset,
            end_offset,
        );
        style_attribute =
            format!("stroke-dasharray: {dash}; stroke-dashoffset: 0;{style_attribute}");
    }

    if !points.is_empty() {
        let mid = points[points.len() / 2];
        if !is_label_coordinate_in_path(mid, &path) {
            points_changed = true;
        }
    }

    Ok(InsertedEdge {
        id: format!("{diagram_id}-{}", edge.id),
        path,
        points: points.clone(),
        roles,
        styles,
        labels: insert_edge_label(edge),
        curve,
        data_points,
        class_attribute: edge_class_attribute(edge),
        style_attribute,
        data_look: edge.look.clone().unwrap_or_else(|| "undefined".into()),
        data_edge: true,
        data_et: "edge".into(),
        data_id: edge.id.clone(),
        marker_start: edge_marker_attribute(
            edge.arrow_type_start.as_deref(),
            true,
            use_margin,
            diagram_id,
            diagram_type,
            color.as_deref(),
        ),
        marker_end: edge_marker_attribute(
            edge.arrow_type_end.as_deref(),
            false,
            use_margin,
            diagram_id,
            diagram_type,
            color.as_deref(),
        ),
        animated,
        updated_path: points_changed.then_some(points),
        original_path: edge.points.clone(),
    })
}

/// One edge's geometry for `lineJump.ts`; points are the layout polyline.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeGeom {
    pub id: String,
    pub points: Vec<Point>,
    #[serde(default)]
    pub curve: Option<String>,
    #[serde(default)]
    pub corner_radius: Option<f64>,
    #[serde(default)]
    pub arrow_type_start: Option<String>,
    #[serde(default)]
    pub arrow_type_end: Option<String>,
}

/// A detected polyline crossing and the edge that carries its hop.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Crossing {
    pub jump_edge_id: String,
    pub other_edge_id: String,
    pub seg_index: usize,
    pub t: f64,
    pub point: Point,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JumpStyle {
    Arc,
    Gap,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineJumpConfig {
    pub enabled: bool,
    pub jump_radius: f64,
    pub jump_style: JumpStyle,
}

impl LineJumpConfig {
    /// The `lineJump` config block: disabled when absent.
    pub fn from_options(enabled: bool, radius: f64, gap: bool) -> Self {
        Self {
            enabled,
            jump_radius: radius,
            jump_style: if gap { JumpStyle::Gap } else { JumpStyle::Arc },
        }
    }
}

const ENDPOINT_EPSILON: f64 = 1e-6;

fn segment_amount(a1: Point, a2: Point, b1: Point, b2: Point) -> Option<(Point, f64, f64)> {
    let dx_a = a2.x - a1.x;
    let dy_a = a2.y - a1.y;
    let dx_b = b2.x - b1.x;
    let dy_b = b2.y - b1.y;
    let denominator = dx_a * dy_b - dy_a * dx_b;
    if denominator == 0.0 {
        return None;
    }
    let dx = b1.x - a1.x;
    let dy = b1.y - a1.y;
    let t_a = (dx * dy_b - dy * dx_b) / denominator;
    let t_b = (dx * dy_a - dy * dx_a) / denominator;
    if t_a <= ENDPOINT_EPSILON
        || t_a >= 1.0 - ENDPOINT_EPSILON
        || t_b <= ENDPOINT_EPSILON
        || t_b >= 1.0 - ENDPOINT_EPSILON
    {
        return None;
    }
    Some((Point::new(a1.x + t_a * dx_a, a1.y + t_a * dy_a), t_a, t_b))
}

fn is_horizontal_seg(a: Point, b: Point) -> bool {
    (b.x - a.x).abs() >= (b.y - a.y).abs()
}

#[derive(Clone, Copy, Debug)]
struct RoundedCorner {
    start_x: f64,
    start_y: f64,
    end_x: f64,
    end_y: f64,
    ctrl_x: f64,
    ctrl_y: f64,
    cut_len: f64,
}

/// `computeRoundedCorner`, mirroring `generateRoundedPath`'s bend contraction.
fn compute_rounded_corner(
    prev: Point,
    curr: Point,
    next: Point,
    radius: f64,
) -> Option<RoundedCorner> {
    let dx1 = curr.x - prev.x;
    let dy1 = curr.y - prev.y;
    let dx2 = next.x - curr.x;
    let dy2 = next.y - curr.y;
    let len1 = dx1.hypot(dy1);
    let len2 = dx2.hypot(dy2);
    if len1 < CORNER_EPSILON || len2 < CORNER_EPSILON {
        return None;
    }
    let nx1 = dx1 / len1;
    let ny1 = dy1 / len1;
    let nx2 = dx2 / len2;
    let ny2 = dy2 / len2;
    let dot = nx1 * nx2 + ny1 * ny2;
    let angle = dot.clamp(-1.0, 1.0).acos();
    if angle < CORNER_EPSILON || (std::f64::consts::PI - angle).abs() < CORNER_EPSILON {
        return None;
    }
    let cut_len = (radius / (angle / 2.0).sin())
        .min(len1 / 2.0)
        .min(len2 / 2.0);
    Some(RoundedCorner {
        start_x: curr.x - nx1 * cut_len,
        start_y: curr.y - ny1 * cut_len,
        end_x: curr.x + nx2 * cut_len,
        end_y: curr.y + ny2 * cut_len,
        ctrl_x: curr.x,
        ctrl_y: curr.y,
        cut_len,
    })
}

fn crossing_sits_in_rounded_corner(edge: &EdgeGeom, seg_index: usize, t: f64) -> bool {
    if edge.curve.as_deref() != Some("rounded") {
        return false;
    }
    let points = &edge.points;
    let (Some(&a), Some(&b)) = (points.get(seg_index), points.get(seg_index + 1)) else {
        return false;
    };
    let seg_len = (b.x - a.x).hypot(b.y - a.y);
    let d = t * seg_len;
    let radius = resolve_edge_corner_radius(edge.corner_radius);
    if seg_index > 0
        && let Some(entering) = compute_rounded_corner(points[seg_index - 1], a, b, radius)
        && d < entering.cut_len
    {
        return true;
    }
    if seg_index + 2 < points.len()
        && let Some(leaving) = compute_rounded_corner(a, b, points[seg_index + 2], radius)
        && seg_len - d < leaving.cut_len
    {
        return true;
    }
    false
}

/// `findEdgeIntersections`: every true crossing, assigned to the horizontal segment when the
/// pair is orthogonal and to the later edge otherwise.
pub fn find_edge_intersections(edges: &[EdgeGeom]) -> Vec<Crossing> {
    let mut crossings = Vec::new();
    for (i, edge_a) in edges.iter().enumerate() {
        for edge_b in edges.iter().skip(i + 1) {
            for (si, a_pair) in edge_a.points.windows(2).enumerate() {
                for (sj, b_pair) in edge_b.points.windows(2).enumerate() {
                    let Some((point, t_a, t_b)) =
                        segment_amount(a_pair[0], a_pair[1], b_pair[0], b_pair[1])
                    else {
                        continue;
                    };
                    if crossing_sits_in_rounded_corner(edge_a, si, t_a)
                        || crossing_sits_in_rounded_corner(edge_b, sj, t_b)
                    {
                        continue;
                    }
                    let a_horizontal = is_horizontal_seg(a_pair[0], a_pair[1]);
                    let b_horizontal = is_horizontal_seg(b_pair[0], b_pair[1]);
                    let orthogonal_pair = a_horizontal != b_horizontal;
                    let jump_on_a = orthogonal_pair && a_horizontal;
                    if jump_on_a {
                        crossings.push(Crossing {
                            jump_edge_id: edge_a.id.clone(),
                            other_edge_id: edge_b.id.clone(),
                            seg_index: si,
                            t: t_a,
                            point,
                        });
                    } else {
                        crossings.push(Crossing {
                            jump_edge_id: edge_b.id.clone(),
                            other_edge_id: edge_a.id.clone(),
                            seg_index: sj,
                            t: t_b,
                            point,
                        });
                    }
                }
            }
        }
    }
    crossings
}

fn apply_line_jump_marker_offsets(points: &[Point], edge: &EdgeGeom) -> Vec<Point> {
    let mut new_points = points.to_vec();
    if points.len() < 2 {
        return new_points;
    }
    if let Some(offset) = edge
        .arrow_type_start
        .as_deref()
        .and_then(marker_offset)
        .filter(|offset| *offset != 0.0)
    {
        let (a, b) = (points[0], points[1]);
        let angle = (b.y - a.y).atan2(b.x - a.x);
        new_points[0] = Point::new(a.x + offset * angle.cos(), a.y + offset * angle.sin());
    }
    if let Some(offset) = edge
        .arrow_type_end
        .as_deref()
        .and_then(marker_offset)
        .filter(|offset| *offset != 0.0)
    {
        let n = points.len();
        let (a, b) = (points[n - 2], points[n - 1]);
        let angle = (b.y - a.y).atan2(b.x - a.x);
        new_points[n - 1] = Point::new(b.x - offset * angle.cos(), b.y - offset * angle.sin());
    }
    new_points
}

/// `getArcSweepFlag`: horizontal hops bump up, vertical hops bump right.
fn arc_sweep_flag(a: Point, b: Point) -> bool {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    if dx.abs() >= dy.abs() {
        dx >= 0.0
    } else {
        dy >= 0.0
    }
}

#[derive(Clone, Copy, Debug)]
struct JumpOnSegment {
    t: f64,
    point: Point,
    d: f64,
    r: f64,
}

/// `rewriteEdgePath`: plain polylines with hops, rounded corners re-emitted for `rounded`.
fn rewrite_edge_path(edge: &EdgeGeom, jumps: &[Crossing], config: LineJumpConfig) -> Path {
    let raw_points = &edge.points;
    let mut path = Path::new();
    if raw_points.len() < 2 {
        return path;
    }
    let points = apply_line_jump_marker_offsets(raw_points, edge);
    let rounded = edge.curve.as_deref() == Some("rounded");
    let corner_radius = resolve_edge_corner_radius(edge.corner_radius);
    let segments: Vec<(Point, Point)> = points.windows(2).map(|pair| (pair[0], pair[1])).collect();
    let mut by_segment: BTreeMap<usize, Vec<JumpOnSegment>> = BTreeMap::new();
    for crossing in jumps {
        let Some(&(a, b)) = segments.get(crossing.seg_index) else {
            continue;
        };
        let seg_len = (b.x - a.x).hypot(b.y - a.y);
        by_segment
            .entry(crossing.seg_index)
            .or_default()
            .push(JumpOnSegment {
                t: crossing.t,
                point: crossing.point,
                d: crossing.t * seg_len,
                r: config.jump_radius,
            });
    }

    path.move_to(points[0]);
    for (i, &(a, b)) in segments.iter().enumerate() {
        let seg_len = (b.x - a.x).hypot(b.y - a.y);
        let (ux, uy) = if seg_len == 0.0 {
            (0.0, 0.0)
        } else {
            ((b.x - a.x) / seg_len, (b.y - a.y) / seg_len)
        };
        let sweep = arc_sweep_flag(a, b);

        let mut seg_start_consumed = 0.0;
        if rounded
            && i > 0
            && let Some(corner) = compute_rounded_corner(
                points[i - 1],
                points[i],
                points.get(i + 1).copied().unwrap_or(points[i]),
                corner_radius,
            )
        {
            seg_start_consumed = corner.cut_len;
        }
        let mut seg_end_stop = seg_len;
        let mut upcoming_corner = None;
        if rounded && i < segments.len() - 1 {
            let corner = compute_rounded_corner(
                points[i],
                points[i + 1],
                points.get(i + 2).copied().unwrap_or(points[i + 1]),
                corner_radius,
            );
            if let Some(corner) = corner {
                seg_end_stop = seg_len - corner.cut_len;
                upcoming_corner = Some(corner);
            }
        }

        let min_useful_radius = config.jump_radius * MIN_USEFUL_RADIUS_RATIO;
        let mut seg_jumps: Vec<JumpOnSegment> = by_segment
            .get(&i)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|mut jump| {
                let room = (jump.d - seg_start_consumed).min(seg_end_stop - jump.d)
                    - CORNER_JUMP_CLEARANCE;
                jump.r = jump.r.min(room);
                jump
            })
            .filter(|jump| jump.r >= min_useful_radius)
            .collect();
        seg_jumps.sort_by(|a, b| a.t.total_cmp(&b.t));
        for k in 0..seg_jumps.len().saturating_sub(1) {
            let gap = seg_jumps[k + 1].d - seg_jumps[k].d;
            if seg_jumps[k].r + seg_jumps[k + 1].r > gap {
                let half = gap / 2.0;
                seg_jumps[k].r = seg_jumps[k].r.min(half);
                seg_jumps[k + 1].r = seg_jumps[k + 1].r.min(half);
            }
        }
        for jump in &seg_jumps {
            if jump.r < min_useful_radius {
                continue;
            }
            let pre = Point::new(jump.point.x - ux * jump.r, jump.point.y - uy * jump.r);
            let post = Point::new(jump.point.x + ux * jump.r, jump.point.y + uy * jump.r);
            path.line_to(pre);
            match config.jump_style {
                JumpStyle::Arc => {
                    path.arc_to(jump.r, jump.r, 0.0, false, sweep, post);
                }
                JumpStyle::Gap => {
                    path.move_to(post);
                }
            }
        }
        if let Some(corner) = upcoming_corner.filter(|_| rounded) {
            path.line_to(Point::new(corner.start_x, corner.start_y));
            path.quad_to(
                Point::new(corner.ctrl_x, corner.ctrl_y),
                Point::new(corner.end_x, corner.end_y),
            );
        } else {
            path.line_to(b);
        }
    }
    path
}

fn plain_path(points: &[Point]) -> Path {
    let mut path = Path::new();
    if let Some(first) = points.first() {
        path.move_to(*first);
        for point in points.iter().skip(1) {
            path.line_to(*point);
        }
    }
    path
}

/// `processEdgesWithJumps`: one path per edge, plain when it owns no crossing.
pub fn process_edges_with_jumps(edges: &[EdgeGeom], config: LineJumpConfig) -> Vec<(String, Path)> {
    if !config.enabled {
        return edges
            .iter()
            .map(|edge| (edge.id.clone(), plain_path(&edge.points)))
            .collect();
    }
    let crossings = find_edge_intersections(edges);
    let mut by_edge: BTreeMap<String, Vec<Crossing>> = BTreeMap::new();
    for crossing in crossings {
        by_edge
            .entry(crossing.jump_edge_id.clone())
            .or_default()
            .push(crossing);
    }
    edges
        .iter()
        .map(|edge| {
            let path = match by_edge.get(&edge.id) {
                Some(jumps) if !jumps.is_empty() => rewrite_edge_path(edge, jumps, config),
                _ => plain_path(&edge.points),
            };
            (edge.id.clone(), path)
        })
        .collect()
}

/// `isStraightPath`: only move/line commands, numbers, and separators.
pub fn is_straight_path(data: &str) -> bool {
    data.chars().all(|character| {
        character.is_ascii_digit()
            || character.is_ascii_whitespace()
            || matches!(
                character,
                '+' | ',' | '.' | 'L' | 'M' | 'e' | 'l' | 'm' | '-'
            )
    })
}

/// `curveSupportsLineHops`: smoothing curves are not rewritten.
pub fn curve_supports_line_hops(curve: Option<&str>) -> bool {
    match curve {
        None => true,
        Some(curve) => matches!(
            curve,
            "linear" | "rounded" | "step" | "stepBefore" | "stepAfter"
        ),
    }
}

/// Pure line-jump rewrite for orthogonal crossings, following `processEdgesWithJumps`.
pub fn line_jump_paths(
    edges: &[EdgeData],
    enabled: bool,
    radius: f64,
    gap: bool,
) -> Vec<(String, Path)> {
    let geometry: Vec<EdgeGeom> = edges
        .iter()
        .map(|edge| EdgeGeom {
            id: edge.id.clone(),
            points: edge.points.clone(),
            curve: edge.curve.clone(),
            corner_radius: edge.corner_radius,
            arrow_type_start: edge.arrow_type_start.clone(),
            arrow_type_end: edge.arrow_type_end.clone(),
        })
        .collect();
    process_edges_with_jumps(
        &geometry,
        LineJumpConfig::from_options(enabled, radius, gap),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::PathCommand;

    fn number(v: f64) -> String {
        let v = (v * 1000.0 + 0.5).floor() / 1000.0;
        if v == 0.0 { "0".into() } else { v.to_string() }
    }

    fn assert_d3_paths(points: &[Point], paths: &serde_json::Value, cases: &[(&str, Curve)]) {
        for (name, curve) in cases {
            let mut actual = String::new();
            for cmd in edge_path(points, *curve, 0.0).commands {
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
            assert_eq!(actual, paths[*name].as_str().unwrap(), "{name}");
        }
    }

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
            ("CatmullRom", Curve::CatmullRom),
            ("MonotoneX", Curve::MonotoneX),
            ("MonotoneY", Curve::MonotoneY),
            ("Natural", Curve::Natural),
            ("BumpX", Curve::BumpX),
            ("BumpY", Curve::BumpY),
            ("Step", Curve::Step),
            ("StepAfter", Curve::StepAfter),
            ("StepBefore", Curve::StepBefore),
        ];
        assert_d3_paths(&points, &fixture["paths"], &cases);

        let additional_points: Vec<Point> = fixture["additional"]["points"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| Point::new(p[0].as_f64().unwrap(), p[1].as_f64().unwrap()))
            .collect();
        let smooth_cases = [
            ("CatmullRom", Curve::CatmullRom),
            ("MonotoneX", Curve::MonotoneX),
            ("MonotoneY", Curve::MonotoneY),
            ("Natural", Curve::Natural),
        ];
        assert_d3_paths(
            &additional_points,
            &fixture["additional"]["paths"],
            &smooth_cases,
        );
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

    #[test]
    fn upstream_render_fixture_covers_twelve_edge_variants() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("testdata/edges/upstream-render.json")).unwrap();
        let edges = fixture["edges"].as_array().unwrap();
        assert!(edges.len() >= 12);
        for edge in edges {
            assert!(edge["path"].as_str().is_some());
            assert!(edge["points"].as_array().is_some());
        }
        assert!(edges.iter().any(|edge| edge["label"] == "main label"));
        assert!(edges.iter().any(|edge| edge["terminalLabels"].is_array()));
        assert!(edges.iter().any(|edge| edge["cluster"].is_object()));
        assert!(
            edges
                .iter()
                .any(|edge| edge["jumpPath"].as_str().unwrap().contains('A'))
        );
    }

    #[test]
    fn insert_edge_preserves_scene_roles_and_requires_diagram_id() {
        let edge = EdgeData {
            id: "e1".into(),
            points: vec![Point::new(0., 0.), Point::new(100., 0.)],
            curve: Some("linear".into()),
            thickness: "thick".into(),
            pattern: "dashed".into(),
            classes: vec!["custom".into()],
            ..EdgeData::default()
        };
        let rendered = insert_edge(&edge, "diagram").unwrap();
        assert_eq!(rendered.id, "diagram-e1");
        assert!(rendered.roles.contains(&"edge-thickness-thick".into()));
        assert!(rendered.roles.contains(&"edge-pattern-dashed".into()));
        assert!(rendered.roles.contains(&"custom".into()));
        assert!(
            insert_edge(&edge, "")
                .unwrap_err()
                .message
                .contains("missing diagramId")
        );
    }

    #[test]
    fn line_jumps_arc_over_crossings_and_gap_style_breaks_path() {
        let edges = [
            EdgeData {
                id: "horizontal".into(),
                points: vec![Point::new(0., 50.), Point::new(100., 50.)],
                curve: Some("linear".into()),
                ..EdgeData::default()
            },
            EdgeData {
                id: "vertical".into(),
                points: vec![Point::new(50., 0.), Point::new(50., 100.)],
                curve: Some("linear".into()),
                ..EdgeData::default()
            },
        ];
        let arc = line_jump_paths(&edges, true, 1.0, false);
        assert!(
            arc[0]
                .1
                .commands
                .iter()
                .any(|c| matches!(c, PathCommand::ArcTo { .. }))
        );
        assert_eq!(
            path_data(&arc[0].1),
            "M0,50 L49,50 A1,1 0 0 1 51,50 L100,50"
        );
        let gap = line_jump_paths(&edges, true, 1.0, true);
        assert!(
            gap[0]
                .1
                .commands
                .iter()
                .any(|c| matches!(c, PathCommand::MoveTo { .. }))
        );
        assert_eq!(path_data(&gap[0].1), "M0,50 L49,50 M51,50 L100,50");
    }

    fn fixture() -> serde_json::Value {
        serde_json::from_str(include_str!("testdata/edges/upstream-render.json")).unwrap()
    }

    fn full_path_data(path: &Path) -> String {
        let mut result = String::new();
        for command in &path.commands {
            match *command {
                PathCommand::MoveTo { x, y } => result.push_str(&format!("M{x},{y}")),
                PathCommand::LineTo { x, y } => result.push_str(&format!("L{x},{y}")),
                PathCommand::QuadTo { cx, cy, x, y } => {
                    result.push_str(&format!("Q{cx},{cy} {x},{y}"))
                }
                PathCommand::CubicTo {
                    c1x,
                    c1y,
                    c2x,
                    c2y,
                    x,
                    y,
                } => result.push_str(&format!("C{c1x},{c1y},{c2x},{c2y},{x},{y}")),
                PathCommand::ArcTo {
                    rx,
                    ry,
                    sweep,
                    x,
                    y,
                    ..
                } => result.push_str(&format!("A{rx},{ry} 0 0 {} {x},{y}", u8::from(sweep))),
                PathCommand::Close => result.push('Z'),
            }
        }
        result
    }

    fn d3_path_data(path: &Path) -> String {
        let mut result = String::new();
        for command in &path.commands {
            match *command {
                PathCommand::MoveTo { x, y } => {
                    result.push_str(&format!("M{},{}", js_number(x), js_number(y)))
                }
                PathCommand::LineTo { x, y } => {
                    result.push_str(&format!("L{},{}", js_number(x), js_number(y)))
                }
                PathCommand::CubicTo {
                    c1x,
                    c1y,
                    c2x,
                    c2y,
                    x,
                    y,
                } => result.push_str(&format!(
                    "C{},{},{},{},{},{}",
                    js_number(c1x),
                    js_number(c1y),
                    js_number(c2x),
                    js_number(c2y),
                    js_number(x),
                    js_number(y)
                )),
                other => panic!("unexpected d3 command {other:?}"),
            }
        }
        result
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    fn points_from(value: &serde_json::Value) -> Vec<Point> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|point| Point::new(point["x"].as_f64().unwrap(), point["y"].as_f64().unwrap()))
            .collect()
    }

    fn node_from(value: &serde_json::Value) -> Option<NodeOutline> {
        value.is_object().then(|| NodeOutline {
            x: value["x"].as_f64().unwrap(),
            y: value["y"].as_f64().unwrap(),
            width: value["width"].as_f64().unwrap(),
            height: value["height"].as_f64().unwrap(),
            shape: NodeShape::Rect,
        })
    }

    #[test]
    fn insert_edge_matches_upstream_node_and_cluster_clipping() {
        let fixture = fixture();
        let cases = fixture["insertEdge"].as_array().unwrap();
        assert!(cases.len() >= 30);
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let mut edge_value = case["edge"].clone();
            let object = edge_value.as_object_mut().unwrap();
            let to_cluster = object
                .remove("toCluster")
                .and_then(|value| value.as_str().map(str::to_string));
            let from_cluster = object
                .remove("fromCluster")
                .and_then(|value| value.as_str().map(str::to_string));
            let mut edge: EdgeData = serde_json::from_value(edge_value).unwrap();
            edge.layout = case["layout"].as_str().map(str::to_string);
            edge.skip_intersect = case["skipIntersect"].as_bool().unwrap_or(false);
            edge.from_node = node_from(&case["from"]);
            edge.to_node = node_from(&case["to"]);
            let cluster_rect = |id: &str| {
                let node = &case["clusterDb"][id];
                Some(Rect::new(
                    node["x"].as_f64().unwrap() - node["width"].as_f64().unwrap() / 2.0,
                    node["y"].as_f64().unwrap() - node["height"].as_f64().unwrap() / 2.0,
                    node["width"].as_f64().unwrap(),
                    node["height"].as_f64().unwrap(),
                ))
            };
            edge.to_cluster = to_cluster.as_deref().and_then(cluster_rect);
            edge.from_cluster = from_cluster.as_deref().and_then(cluster_rect);

            let expected = &case["expected"];
            let result = insert_edge_with_type(
                &edge,
                case["diagramId"].as_str().unwrap(),
                case["diagramType"].as_str().unwrap(),
            );
            if let Some(error) = expected.get("error") {
                let actual = result.unwrap_err();
                assert_eq!(actual.message, error["message"].as_str().unwrap(), "{name}");
                continue;
            }
            let rendered = result.unwrap();
            let actual_path = if edge.curve.as_deref() == Some("rounded") {
                full_path_data(&rendered.path)
            } else {
                d3_path_data(&rendered.path)
            };
            assert_eq!(actual_path, expected["d"].as_str().unwrap(), "{name}");
            assert_eq!(rendered.id, expected["id"].as_str().unwrap(), "{name}");
            assert_eq!(
                rendered.class_attribute,
                expected["classAttribute"].as_str().unwrap(),
                "{name}"
            );
            assert_eq!(
                rendered.style_attribute,
                expected["style"].as_str().unwrap(),
                "{name}"
            );
            assert_eq!(
                rendered.data_points,
                data_points_attribute(&points_from(&expected["dataPoints"])),
                "{name} data-points"
            );
            assert_eq!(
                rendered.data_look,
                expected["dataLook"].as_str().unwrap(),
                "{name} data-look"
            );
            assert_eq!(
                rendered.marker_start.as_deref(),
                expected["markerStart"].as_str(),
                "{name} marker-start"
            );
            assert_eq!(
                rendered.marker_end.as_deref(),
                expected["markerEnd"].as_str(),
                "{name} marker-end"
            );
        }
    }

    #[test]
    fn intersection_matches_upstream_node_boxes() {
        let fixture = fixture();
        for case in fixture["intersection"].as_array().unwrap() {
            let node = node_from(&case["node"]).unwrap();
            let outside = points_from(&serde_json::json!([case["outsidePoint"]]))[0];
            let inside = points_from(&serde_json::json!([case["insidePoint"]]))[0];
            let expected = points_from(&serde_json::json!([case["expected"]]))[0];
            let actual = intersection(node, outside, inside);
            assert!(
                close(actual.x, expected.x) && close(actual.y, expected.y),
                "{}: {actual:?} != {expected:?}",
                case["name"]
            );
        }
    }

    fn terminal_position(name: &str) -> TerminalPosition {
        match name {
            "start_left" => TerminalPosition::StartLeft,
            "start_right" => TerminalPosition::StartRight,
            "end_left" => TerminalPosition::EndLeft,
            "end_right" => TerminalPosition::EndRight,
            other => panic!("unknown terminal position {other}"),
        }
    }

    fn parse_translate(value: &str) -> Point {
        let inner = value.trim_start_matches("translate(").trim_end_matches(')');
        let mut parts = inner.split(", ");
        Point::new(
            parts.next().unwrap().parse().unwrap(),
            parts.next().unwrap().parse().unwrap(),
        )
    }

    #[test]
    fn labels_match_upstream_placement_and_boxes() {
        let fixture = fixture();
        for case in fixture["labels"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            match case["kind"].as_str().unwrap() {
                "terminal" => {
                    let points = points_from(&case["points"]);
                    let actual = terminal_label_position(
                        &points,
                        terminal_position(case["position"].as_str().unwrap()),
                        case["markerSize"].as_f64().unwrap(),
                    )
                    .unwrap();
                    let expected = &case["expected"];
                    assert!(
                        close(actual.x, expected["x"].as_f64().unwrap())
                            && close(actual.y, expected["y"].as_f64().unwrap()),
                        "{name}: {actual:?}"
                    );
                }
                "midpoint" => {
                    let actual = calculate_label_position(&points_from(&case["points"]));
                    let expected = &case["expected"];
                    assert!(
                        close(actual.x, expected["x"].as_f64().unwrap())
                            && close(actual.y, expected["y"].as_f64().unwrap()),
                        "{name}: {actual:?}"
                    );
                }
                "resolve" => {
                    let edge = EdgeData {
                        label_x: Some(case["edge"]["x"].as_f64().unwrap()),
                        label_y: Some(case["edge"]["y"].as_f64().unwrap()),
                        ..EdgeData::default()
                    };
                    let updated = points_from(&case["updatedPath"]);
                    let original = case
                        .get("originalPath")
                        .filter(|value| value.is_array())
                        .map(points_from);
                    let actual =
                        resolve_edge_label_position(&edge, Some(&updated), original.as_deref());
                    let expected = &case["expected"];
                    assert!(
                        close(actual.x, expected["x"].as_f64().unwrap())
                            && close(actual.y, expected["y"].as_f64().unwrap()),
                        "{name}: {actual:?}"
                    );
                }
                "transform" => {
                    let box_ = &case["box"];
                    let width = box_["width"].as_f64().unwrap();
                    let height = box_["height"].as_f64().unwrap();
                    let actual = label_transform(
                        box_["x"].as_f64().unwrap(),
                        box_["y"].as_f64().unwrap(),
                        width,
                        height,
                        case["html"].as_bool().unwrap(),
                    );
                    let expected = parse_translate(case["expected"].as_str().unwrap());
                    assert_eq!(actual, expected, "{name}");
                }
                "terminal-width" => {
                    let actual = terminal_label_box(
                        case["label"].as_str().unwrap(),
                        (
                            case["box"]["width"].as_f64().unwrap(),
                            case["box"]["height"].as_f64().unwrap(),
                        ),
                    );
                    let parse = |value: &serde_json::Value| {
                        value
                            .as_str()
                            .unwrap()
                            .trim_end_matches("px")
                            .parse::<f64>()
                            .unwrap()
                    };
                    assert_eq!(
                        actual,
                        (
                            parse(&case["expected"]["width"]),
                            parse(&case["expected"]["height"])
                        ),
                        "{name}"
                    );
                }
                "background" => {
                    let text = &case["textBox"];
                    let actual = label_background(Rect::new(
                        text["x"].as_f64().unwrap(),
                        text["y"].as_f64().unwrap(),
                        text["width"].as_f64().unwrap(),
                        text["height"].as_f64().unwrap(),
                    ));
                    let expected = &case["expected"];
                    assert_eq!(
                        actual,
                        Rect::new(
                            expected["x"].as_f64().unwrap(),
                            expected["y"].as_f64().unwrap(),
                            expected["width"].as_f64().unwrap(),
                            expected["height"].as_f64().unwrap(),
                        ),
                        "{name}"
                    );
                }
                other => panic!("unknown label case {other}"),
            }
        }
    }

    #[test]
    fn line_jumps_match_upstream_intersections_and_rewrites() {
        let fixture = fixture();
        let cases = fixture["lineJump"].as_array().unwrap();
        assert!(cases.len() >= 17);
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let edges: Vec<EdgeGeom> = case["edges"]
                .as_array()
                .unwrap()
                .iter()
                .map(|edge| serde_json::from_value(edge.clone()).unwrap())
                .collect();
            let config: LineJumpConfig = serde_json::from_value(case["config"].clone()).unwrap();
            let crossings = find_edge_intersections(&edges);
            let expected_crossings = case["crossings"].as_array().unwrap();
            assert_eq!(
                crossings.len(),
                expected_crossings.len(),
                "{name} crossings"
            );
            for (actual, expected) in crossings.iter().zip(expected_crossings) {
                assert_eq!(
                    actual.jump_edge_id,
                    expected["jumpEdgeId"].as_str().unwrap()
                );
                assert_eq!(
                    actual.other_edge_id,
                    expected["otherEdgeId"].as_str().unwrap()
                );
                assert_eq!(
                    actual.seg_index,
                    expected["segIndex"].as_u64().unwrap() as usize
                );
                assert!(close(actual.t, expected["t"].as_f64().unwrap()), "{name} t");
                assert!(
                    close(actual.point.x, expected["point"]["x"].as_f64().unwrap())
                        && close(actual.point.y, expected["point"]["y"].as_f64().unwrap()),
                    "{name} point"
                );
            }
            let paths = process_edges_with_jumps(&edges, config);
            for (id, path) in &paths {
                let expected = case["paths"][id].as_str().unwrap();
                assert_eq!(path_data(path), expected, "{name} {id}");
            }
        }
    }

    #[test]
    fn straight_paths_and_curve_support_match_upstream() {
        let fixture = fixture();
        for case in fixture["straightPath"].as_array().unwrap() {
            assert_eq!(
                is_straight_path(case["d"].as_str().unwrap()),
                case["expected"].as_bool().unwrap(),
                "{}",
                case["d"]
            );
        }
        for case in fixture["curveSupport"].as_array().unwrap() {
            assert_eq!(
                curve_supports_line_hops(case["curve"].as_str()),
                case["expected"].as_bool().unwrap(),
                "{}",
                case["curve"]
            );
        }
    }

    #[test]
    fn labels_use_layout_centers_backgrounds_and_terminal_markers() {
        let points = vec![Point::new(0., 20.), Point::new(100., 20.)];
        let edge = EdgeData {
            label: Some("main".into()),
            label_width: 40.,
            label_height: 20.,
            label_x: Some(30.),
            label_y: Some(40.),
            start_label_left: Some("start".into()),
            arrow_type_start: Some("none".into()),
            points: points.clone(),
            ..EdgeData::default()
        };
        let labels = insert_edge_label(&edge);
        let main = &labels[0];
        assert_eq!(main.position.center, Point::new(30., 40.));
        assert_eq!(main.background, Some(Rect::new(8., 28., 44., 24.)));
        assert_eq!(main.transform, Point::new(-20., -10.));
        let terminal = &labels[1];
        assert_eq!(terminal.background, None);
        assert_eq!(
            Some(terminal.position.center),
            terminal_label_position(&points, TerminalPosition::StartLeft, 10.)
        );

        let updated = [
            Point::new(0., 20.),
            Point::new(60., 20.),
            Point::new(60., 60.),
            Point::new(100., 60.),
        ];
        let moved = position_edge_labels(&edge, Some(&updated));
        assert_eq!(moved[0].position.center, Point::new(40., 50.));
        assert_eq!(
            Some(moved[1].position.center),
            terminal_label_position(&updated, TerminalPosition::StartLeft, 10.)
        );
    }

    #[test]
    fn marker_ids_version_the_diagram_type_and_margin_support() {
        assert_eq!(
            edge_marker_attribute(
                Some("arrow_circle"),
                true,
                true,
                "diagram",
                "flowchart-v2",
                None
            )
            .as_deref(),
            Some("url(#diagram_flowchart-v2-circleStart-margin)")
        );
        assert_eq!(
            edge_marker_attribute(
                Some("zero_or_one"),
                true,
                true,
                "diagram",
                "flowchart-v2",
                None
            )
            .as_deref(),
            Some("url(#diagram_flowchart-v2-zeroOrOneStart)")
        );
        assert_eq!(
            edge_marker_attribute(
                Some("arrow_point"),
                false,
                false,
                "diagram",
                "flowchart-v2",
                Some("#ff0000")
            )
            .as_deref(),
            Some("url(#diagram_flowchart-v2-pointEnd__ff0000)")
        );
        assert_eq!(
            edge_marker_attribute(Some("none"), true, true, "d", "flowchart-v2", None),
            None
        );
        assert_eq!(
            edge_marker_attribute(Some("not-real"), true, true, "d", "flowchart-v2", None),
            None
        );
    }

    #[test]
    fn data_points_attribute_and_background_render_upstream_shapes() {
        assert_eq!(
            data_points_attribute(&[Point::new(0., 20.), Point::new(20., 40.5)]),
            "W3sieCI6MCwieSI6MjB9LHsieCI6MjAsInkiOjQwLjV9XQ=="
        );
        assert_eq!(
            label_background(Rect::new(0., 0., 40., 20.)),
            Rect::new(-2., -2., 44., 24.)
        );
        assert_eq!(terminal_label_box("0..1", (23.4, 21.)), (36., 21.));
        let points = [Point::new(0., 20.), Point::new(20., 40.5)];
        assert_eq!(
            decode_data_points(&data_points_attribute(&points)).as_deref(),
            Some(&points[..])
        );
    }

    #[test]
    fn edge_label_styles_and_animation_flags_follow_upstream() {
        assert_eq!(
            edge_label_styles(Some(&["stroke:#f00".into(), "fill:#eee".into()])),
            ";stroke:#f00;fill:#eee"
        );
        let edge = EdgeData {
            animate: true,
            ..EdgeData::default()
        };
        assert!(is_animated_edge(&edge));
        assert!(edge_class_attribute(&edge).ends_with("edge-animation-fast"));
        let mut edge = EdgeData {
            animation: Some("dash".into()),
            ..EdgeData::default()
        };
        assert!(is_animated_edge(&edge));
        assert!(edge_class_attribute(&edge).ends_with("edge-animation-dash"));
        edge.animate = true;
        assert!(edge_class_attribute(&edge).ends_with("edge-animation-dash"));
    }
}
