//! Port of `rendering-elements/intersect/`: attachment points for node shapes.

use crate::scene::{Point, Rect};

/// Geometry needed by upstream's `intersect-node.js` callable node.
pub trait Intersectable {
    fn bounds(&self) -> Rect;
    fn intersect(&self, point: Point) -> Point;
}

/// `intersect-node.js` dispatches to the shape's intersection function.
pub fn intersect_node(node: &impl Intersectable, point: Point) -> Point {
    node.intersect(point)
}

/// `intersect-rect.js`; node coordinates represent the center.
pub fn intersect_rect(rect: Rect, point: Point) -> Point {
    let center = rect.center();
    let dx = point.x - center.x;
    let dy = point.y - center.y;
    if dx == 0.0 && dy == 0.0 {
        return center;
    }
    let (mut half_w, mut half_h) = (rect.width / 2.0, rect.height / 2.0);
    if dy.abs() * half_w > dx.abs() * half_h {
        if dy < 0.0 {
            half_h = -half_h;
        }
        let sx = if dy == 0.0 { 0.0 } else { half_h * dx / dy };
        Point::new(center.x + sx, center.y + half_h)
    } else {
        if dx < 0.0 {
            half_w = -half_w;
        }
        let sy = if dx == 0.0 { 0.0 } else { half_w * dy / dx };
        Point::new(center.x + half_w, center.y + sy)
    }
}

/// `intersect-circle.js` delegates to the ellipse equation with equal radii.
pub fn intersect_circle(center: Point, radius: f64, point: Point) -> Point {
    intersect_ellipse(center, radius, radius, point)
}

/// `intersect-ellipse.js`, including its ray-origin fallback for an interior origin.
pub fn intersect_ellipse(center: Point, rx: f64, ry: f64, point: Point) -> Point {
    let px = center.x - point.x;
    let py = center.y - point.y;
    let det = (rx * rx * py * py + ry * ry * px * px).sqrt();
    if det == 0.0 {
        return center;
    }
    let mut dx = (rx * ry * px / det).abs();
    let mut dy = (rx * ry * py / det).abs();
    if point.x < center.x {
        dx = -dx;
    }
    if point.y < center.y {
        dy = -dy;
    }
    Point::new(center.x + dx, center.y + dy)
}

/// `intersect-polygon.js`, selecting the nearest segment hit to the query point.
pub fn intersect_polygon(rect: Rect, polygon: &[Point], point: Point) -> Point {
    if polygon.is_empty() {
        return rect.center();
    }
    let min_x = polygon.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let min_y = polygon.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    let left = rect.center().x - rect.width / 2.0 - min_x;
    let top = rect.center().y - rect.height / 2.0 - min_y;
    let origin = rect.center();
    let mut hits = Vec::new();
    for i in 0..polygon.len() {
        let a = polygon[i].translated(left, top);
        let b = polygon[(i + 1) % polygon.len()].translated(left, top);
        if let Some(hit) = intersect_line(origin, point, a, b) {
            hits.push(hit);
        }
    }
    hits.into_iter()
        .min_by(|a, b| a.distance_to(point).total_cmp(&b.distance_to(point)))
        .unwrap_or(origin)
}

/// `intersect-line.js`, with unrounded Graphics Gems coordinates.
pub fn intersect_line(p1: Point, p2: Point, q1: Point, q2: Point) -> Option<Point> {
    let a1 = p2.y - p1.y;
    let b1 = p1.x - p2.x;
    let c1 = p2.x * p1.y - p1.x * p2.y;
    let r3 = a1 * q1.x + b1 * q1.y + c1;
    let r4 = a1 * q2.x + b1 * q2.y + c1;
    if r3 != 0.0 && r4 != 0.0 && r3 * r4 > 0.0 {
        return None;
    }
    let a2 = q2.y - q1.y;
    let b2 = q1.x - q2.x;
    let c2 = q2.x * q1.y - q1.x * q2.y;
    let r1 = a2 * p1.x + b2 * p1.y + c2;
    let r2 = a2 * p2.x + b2 * p2.y + c2;
    if r1.abs() < 1e-6 && r2.abs() < 1e-6 && r1 * r2 > 0.0 {
        return None;
    }
    let denom = a1 * b2 - a2 * b1;
    if denom == 0.0 {
        return None;
    }
    Some(Point::new(
        (b1 * c2 - b2 * c1) / denom,
        (a2 * c1 - a1 * c2) / denom,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangle_and_ellipse_attachment_points() {
        assert_eq!(
            intersect_rect(Rect::new(-5.0, -3.0, 10.0, 6.0), Point::new(10.0, 0.0)),
            Point::new(5.0, 0.0)
        );
        assert_eq!(
            intersect_circle(Point::ZERO, 5.0, Point::new(10.0, 0.0)),
            Point::new(5.0, 0.0)
        );
        assert_eq!(
            intersect_ellipse(Point::ZERO, 10.0, 5.0, Point::new(20.0, 0.0)),
            Point::new(10.0, 0.0)
        );
    }

    #[test]
    fn polygon_and_line_intersection() {
        let rect = Rect::new(-5.0, -5.0, 10.0, 10.0);
        let points = [
            Point::new(5.0, 0.0),
            Point::new(10.0, 5.0),
            Point::new(5.0, 10.0),
            Point::new(0.0, 5.0),
        ];
        assert_eq!(
            intersect_polygon(rect, &points, Point::new(20.0, 5.0)),
            Point::new(4.0, 1.0)
        );
        assert_eq!(
            intersect_line(
                Point::new(0.0, 0.0),
                Point::new(10.0, 10.0),
                Point::new(0.0, 10.0),
                Point::new(10.0, 0.0)
            ),
            Some(Point::new(5.0, 5.0))
        );
    }
}
