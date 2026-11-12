//! The arithmetic under the painter and the hit test: composing and inverting the scene's SVG
//! transforms, and turning a path's commands into polylines that can be measured and picked.
//!
//! The scene keeps curves as SVG commands (`C`, `Q`, `A`), so a hit test cannot ask GPUI's
//! tessellator where a path went. Everything that needs points samples the commands here:
//! Bézier curves at fixed steps and elliptical arcs through SVG's endpoint-to-center
//! conversion. The samples are also what places a marker's arrowhead along the path's tangent.

use std::f64::consts::PI;

use polterware_mermaid::scene::{FillRule, Path, PathCommand, Point, Rect, Transform};

/// The inverse of an SVG transform, or `None` when it collapses a plane (a zero scale).
pub(crate) fn inverse(transform: Transform) -> Option<Transform> {
    let determinant = transform.a * transform.d - transform.b * transform.c;
    if determinant.abs() < 1e-12 {
        return None;
    }
    let a = transform.d / determinant;
    let b = -transform.b / determinant;
    let c = -transform.c / determinant;
    let d = transform.a / determinant;
    Some(Transform {
        a,
        b,
        c,
        d,
        e: -(a * transform.e + c * transform.f),
        f: -(b * transform.e + d * transform.f),
    })
}

/// How much the transform scales one scene unit, for stroke widths and text sizes.
pub(crate) fn scale_of(transform: Transform) -> f64 {
    (transform.a * transform.d - transform.b * transform.c)
        .abs()
        .sqrt()
}

/// The axis-aligned bounds of a rectangle's four transformed corners.
pub(crate) fn transformed_bounds(rect: Rect, transform: Transform) -> Rect {
    let corners = [
        transform.apply(Point::new(rect.x, rect.y)),
        transform.apply(Point::new(rect.right(), rect.y)),
        transform.apply(Point::new(rect.right(), rect.bottom())),
        transform.apply(Point::new(rect.x, rect.bottom())),
    ];
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for point in corners {
        min_x = min_x.min(point.x);
        min_y = min_y.min(point.y);
        max_x = max_x.max(point.x);
        max_y = max_y.max(point.y);
    }
    Rect::new(min_x, min_y, max_x - min_x, max_y - min_y)
}

/// One subpath as a polyline. A closed subpath's last point is not repeated.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Subpath {
    pub points: Vec<Point>,
    pub closed: bool,
}

impl Subpath {
    fn push(&mut self, point: Point) {
        if self.points.last().is_none_or(|last| *last != point) {
            self.points.push(point);
        }
    }

    /// The shortest distance from `point` to this polyline, closure included.
    fn distance(&self, point: Point) -> f64 {
        match self.points.as_slice() {
            [] => f64::INFINITY,
            [only] => only.distance_to(point),
            points => {
                let mut best = f64::INFINITY;
                for pair in points.windows(2) {
                    best = best.min(distance_to_segment(point, pair[0], pair[1]));
                }
                if self.closed && points.len() > 2 {
                    best = best.min(distance_to_segment(
                        point,
                        points[points.len() - 1],
                        points[0],
                    ));
                }
                best
            }
        }
    }

    /// A fill treats every subpath as closed, as SVG does; only the stroke cares whether `Z`
    /// was written.
    fn fill_edges(&self) -> impl Iterator<Item = (Point, Point)> + '_ {
        let close = self.points.len() > 1;
        self.points
            .windows(2)
            .map(|pair| (pair[0], pair[1]))
            .chain(close.then(|| (self.points[self.points.len() - 1], self.points[0])))
    }
}

fn distance_to_segment(point: Point, from: Point, to: Point) -> f64 {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    let length_squared = dx * dx + dy * dy;
    if length_squared <= 0.0 {
        return point.distance_to(from);
    }
    let t = (((point.x - from.x) * dx + (point.y - from.y) * dy) / length_squared).clamp(0.0, 1.0);
    point.distance_to(Point::new(from.x + dx * t, from.y + dy * t))
}

/// A path's geometry as polylines, the shape the hit test and the markers use.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PathGeometry {
    pub subpaths: Vec<Subpath>,
}

impl PathGeometry {
    pub fn from_path(path: &Path) -> Self {
        let mut geometry = PathGeometry::default();
        let mut current = Point::ZERO;
        let mut start = Point::ZERO;
        let mut subpath: Option<Subpath> = None;

        for command in &path.commands {
            match *command {
                PathCommand::MoveTo { x, y } => {
                    geometry.flush(&mut subpath);
                    current = Point::new(x, y);
                    start = current;
                    subpath = Some(Subpath {
                        points: vec![current],
                        closed: false,
                    });
                }
                PathCommand::LineTo { x, y } => {
                    let to = Point::new(x, y);
                    geometry.ensure(&mut subpath, current);
                    if let Some(subpath) = subpath.as_mut() {
                        subpath.push(to);
                    }
                    current = to;
                }
                PathCommand::CubicTo {
                    c1x,
                    c1y,
                    c2x,
                    c2y,
                    x,
                    y,
                } => {
                    let control1 = Point::new(c1x, c1y);
                    let control2 = Point::new(c2x, c2y);
                    let to = Point::new(x, y);
                    geometry.ensure(&mut subpath, current);
                    let from = current;
                    for step in 1..=CURVE_STEPS {
                        let t = step as f64 / CURVE_STEPS as f64;
                        if let Some(subpath) = subpath.as_mut() {
                            subpath.push(cubic_at(from, control1, control2, to, t));
                        }
                    }
                    current = to;
                }
                PathCommand::QuadTo { cx, cy, x, y } => {
                    let control = Point::new(cx, cy);
                    let to = Point::new(x, y);
                    geometry.ensure(&mut subpath, current);
                    let from = current;
                    for step in 1..=CURVE_STEPS {
                        let t = step as f64 / CURVE_STEPS as f64;
                        if let Some(subpath) = subpath.as_mut() {
                            subpath.push(quadratic_at(from, control, to, t));
                        }
                    }
                    current = to;
                }
                PathCommand::ArcTo {
                    rx,
                    ry,
                    rotation,
                    large_arc,
                    sweep,
                    x,
                    y,
                } => {
                    let to = Point::new(x, y);
                    geometry.ensure(&mut subpath, current);
                    for point in arc_points(current, rx, ry, rotation, large_arc, sweep, to) {
                        if let Some(subpath) = subpath.as_mut() {
                            subpath.push(point);
                        }
                    }
                    current = to;
                }
                PathCommand::Close => {
                    if let Some(subpath) = subpath.as_mut() {
                        subpath.closed = true;
                    }
                    geometry.flush(&mut subpath);
                    current = start;
                }
            }
        }
        geometry.flush(&mut subpath);
        geometry
    }

    fn ensure(&self, subpath: &mut Option<Subpath>, at: Point) {
        if subpath.is_none() {
            *subpath = Some(Subpath {
                points: vec![at],
                closed: false,
            });
        }
    }

    fn flush(&mut self, subpath: &mut Option<Subpath>) {
        if let Some(subpath) = subpath.take()
            && !subpath.points.is_empty()
        {
            self.subpaths.push(subpath);
        }
    }

    pub fn bounds(&self) -> Option<Rect> {
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        let mut any = false;
        for point in self.subpaths.iter().flat_map(|subpath| &subpath.points) {
            any = true;
            min_x = min_x.min(point.x);
            min_y = min_y.min(point.y);
            max_x = max_x.max(point.x);
            max_y = max_y.max(point.y);
        }
        any.then(|| Rect::new(min_x, min_y, max_x - min_x, max_y - min_y))
    }

    /// A unit tangent at the path's first vertex, pointing into the path.
    pub fn start_tangent(&self) -> Option<Point> {
        self.subpaths
            .iter()
            .find_map(|subpath| tangent(&subpath.points, false))
    }

    /// A unit tangent at the path's last vertex, pointing out of the path.
    pub fn end_tangent(&self) -> Option<Point> {
        self.subpaths
            .iter()
            .rev()
            .find_map(|subpath| tangent(&subpath.points, true))
    }

    pub fn first_point(&self) -> Option<Point> {
        self.subpaths
            .iter()
            .find_map(|subpath| subpath.points.first().copied())
    }

    pub fn last_point(&self) -> Option<Point> {
        self.subpaths
            .iter()
            .rev()
            .find_map(|subpath| subpath.points.last().copied())
    }

    /// Whether a point picks this path: inside when the fill is painted, or within `width / 2`
    /// plus `tolerance` of the stroke.
    pub fn hit(
        &self,
        point: Point,
        fill_rule: FillRule,
        filled: bool,
        stroke_width: f64,
        tolerance: f64,
    ) -> bool {
        if filled && self.contains(point, fill_rule) {
            return true;
        }
        if stroke_width > 0.0 {
            let limit = stroke_width / 2.0 + tolerance;
            return self
                .subpaths
                .iter()
                .any(|subpath| subpath.distance(point) <= limit);
        }
        false
    }

    fn contains(&self, point: Point, rule: FillRule) -> bool {
        match rule {
            FillRule::EvenOdd => {
                let crossings: usize = self
                    .subpaths
                    .iter()
                    .flat_map(Subpath::fill_edges)
                    .filter(|(from, to)| crosses_ray(point, *from, *to))
                    .count();
                crossings % 2 == 1
            }
            FillRule::NonZero => {
                let winding: i32 = self
                    .subpaths
                    .iter()
                    .flat_map(Subpath::fill_edges)
                    .map(|(from, to)| winding_of(point, from, to))
                    .sum();
                winding != 0
            }
        }
    }
}

fn tangent(points: &[Point], at_end: bool) -> Option<Point> {
    if points.len() < 2 {
        return None;
    }
    let (from, to) = if at_end {
        (points[points.len() - 2], points[points.len() - 1])
    } else {
        (points[0], points[1])
    };
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    let length = dx.hypot(dy);
    (length > 1e-9).then(|| Point::new(dx / length, dy / length))
}

fn crosses_ray(point: Point, from: Point, to: Point) -> bool {
    (from.y > point.y) != (to.y > point.y)
        && point.x < (to.x - from.x) * (point.y - from.y) / (to.y - from.y) + from.x
}

fn winding_of(point: Point, from: Point, to: Point) -> i32 {
    let side = (to.x - from.x) * (point.y - from.y) - (point.x - from.x) * (to.y - from.y);
    if from.y <= point.y {
        if to.y > point.y && side > 0.0 {
            return 1;
        }
    } else if to.y <= point.y && side < 0.0 {
        return -1;
    }
    0
}

/// Samples per Bézier curve. Labels and edges are small; sixteen segments keep a 100-unit
/// curve within a third of a unit of its true outline.
const CURVE_STEPS: usize = 16;

fn cubic_at(from: Point, control1: Point, control2: Point, to: Point, t: f64) -> Point {
    let u = 1.0 - t;
    let a = u * u * u;
    let b = 3.0 * u * u * t;
    let c = 3.0 * u * t * t;
    let d = t * t * t;
    Point::new(
        a * from.x + b * control1.x + c * control2.x + d * to.x,
        a * from.y + b * control1.y + c * control2.y + d * to.y,
    )
}

fn quadratic_at(from: Point, control: Point, to: Point, t: f64) -> Point {
    let u = 1.0 - t;
    let a = u * u;
    let b = 2.0 * u * t;
    let c = t * t;
    Point::new(
        a * from.x + b * control.x + c * to.x,
        a * from.y + b * control.y + c * to.y,
    )
}

/// SVG's endpoint-to-center arc conversion, then an even sweep sample. Degenerate radii fall
/// back to a straight line and an arc that cannot reach its endpoint gets scaled up, as SVG's
/// `a` command does.
fn arc_points(
    from: Point,
    rx: f64,
    ry: f64,
    rotation: f64,
    large_arc: bool,
    sweep: bool,
    to: Point,
) -> Vec<Point> {
    let rx = rx.abs();
    let ry = ry.abs();
    if rx < 1e-9 || ry < 1e-9 || from == to {
        return vec![to];
    }
    let phi = rotation.to_radians();
    let (sin_phi, cos_phi) = phi.sin_cos();
    let dx = (from.x - to.x) / 2.0;
    let dy = (from.y - to.y) / 2.0;
    let x1 = cos_phi * dx + sin_phi * dy;
    let y1 = -sin_phi * dx + cos_phi * dy;

    let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    let (rx, ry) = if lambda > 1.0 {
        let factor = lambda.sqrt();
        (rx * factor, ry * factor)
    } else {
        (rx, ry)
    };

    let numerator = (rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1).max(0.0);
    let denominator = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let factor = if denominator <= 0.0 {
        0.0
    } else {
        (numerator / denominator).sqrt()
    } * if large_arc == sweep { -1.0 } else { 1.0 };
    let cx1 = factor * rx * y1 / ry;
    let cy1 = -factor * ry * x1 / rx;
    let cx = cos_phi * cx1 - sin_phi * cy1 + (from.x + to.x) / 2.0;
    let cy = sin_phi * cx1 + cos_phi * cy1 + (from.y + to.y) / 2.0;

    let ux = (x1 - cx1) / rx;
    let uy = (y1 - cy1) / ry;
    let vx = (-x1 - cx1) / rx;
    let vy = (-y1 - cy1) / ry;
    let theta1 = uy.atan2(ux);
    let mut delta = (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
    if !sweep && delta > 0.0 {
        delta -= 2.0 * PI;
    } else if sweep && delta < 0.0 {
        delta += 2.0 * PI;
    }

    let steps = ((delta.abs() / (PI / 8.0)).ceil() as usize).clamp(2, 64);
    let mut points = Vec::with_capacity(steps);
    for step in 1..=steps {
        let angle = theta1 + delta * step as f64 / steps as f64;
        let (sin, cos) = angle.sin_cos();
        points.push(Point::new(
            cx + cos_phi * rx * cos - sin_phi * ry * sin,
            cy + sin_phi * rx * cos + cos_phi * ry * sin,
        ));
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inverse_round_trips_a_point() {
        let transform = Transform::translate(10.0, -4.0)
            .then(Transform::scale(2.0, 3.0))
            .then(Transform::rotate(30.0));
        let point = Point::new(3.0, 7.0);
        let back = inverse(transform)
            .expect("invertible")
            .apply(transform.apply(point));
        assert!((back.x - point.x).abs() < 1e-9);
        assert!((back.y - point.y).abs() < 1e-9);
    }

    #[test]
    fn a_square_path_contains_its_center_and_not_a_point_outside() {
        let mut path = Path::new();
        path.move_to(Point::new(0.0, 0.0))
            .line_to(Point::new(10.0, 0.0))
            .line_to(Point::new(10.0, 10.0))
            .line_to(Point::new(0.0, 10.0))
            .close();
        let geometry = PathGeometry::from_path(&path);
        assert!(geometry.hit(Point::new(5.0, 5.0), FillRule::NonZero, true, 0.0, 0.0));
        assert!(!geometry.hit(Point::new(15.0, 5.0), FillRule::NonZero, true, 0.0, 0.0));
        assert!(geometry.hit(Point::new(11.0, 5.0), FillRule::NonZero, false, 2.0, 0.1));
    }

    #[test]
    fn a_rounded_rect_samples_its_corner_arcs() {
        let geometry =
            PathGeometry::from_path(&Path::rect(Rect::new(0.0, 0.0, 20.0, 10.0), 4.0, 4.0));
        let bounds = geometry.bounds().expect("bounds");
        assert!((bounds.x - 0.0).abs() < 1e-6);
        assert!((bounds.right() - 20.0).abs() < 1e-6);
        assert!((bounds.height - 10.0).abs() < 1e-6);
        let corner = Point::new(0.5, 0.5);
        assert!(!geometry.contains(corner, FillRule::NonZero));
        assert!(geometry.contains(Point::new(10.0, 5.0), FillRule::NonZero));
    }

    #[test]
    fn tangents_point_along_the_first_and_last_segments() {
        let mut path = Path::new();
        path.move_to(Point::new(0.0, 0.0))
            .line_to(Point::new(10.0, 0.0))
            .line_to(Point::new(10.0, 10.0));
        let geometry = PathGeometry::from_path(&path);
        assert_eq!(geometry.start_tangent(), Some(Point::new(1.0, 0.0)));
        assert_eq!(geometry.end_tangent(), Some(Point::new(0.0, 1.0)));
    }

    #[test]
    fn transformed_bounds_wrap_a_rotated_rectangle() {
        let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
        let bounds = transformed_bounds(rect, Transform::rotate(45.0));
        assert!((bounds.width - 14.142_135_6).abs() < 1e-5);
        assert!((bounds.height - 14.142_135_6).abs() < 1e-5);
    }
}
