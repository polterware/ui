//! Port of `layout-base/src/util/Point.js`, the integer point of Java's `Point` class.
//!
//! The coordinates are `f64` because upstream stores JavaScript numbers; `setLocation`
//! rounds the pair when either coordinate has a fraction. Upstream's `move` is
//! [`Point::move_to`] here (`move` is a Rust keyword), and its overloads that take
//! another `Point` are separate methods.

/// A point, upstream's `Point`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    /// X coordinate.
    pub x: f64,
    /// Y coordinate.
    pub y: f64,
}

impl Point {
    /// A point at `(x, y)`.
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Upstream `getX`.
    pub fn get_x(&self) -> f64 {
        self.x
    }

    /// Upstream `getY`.
    pub fn get_y(&self) -> f64 {
        self.y
    }

    /// Upstream `getLocation`.
    pub fn get_location(&self) -> Self {
        Self::new(self.x, self.y)
    }

    /// Upstream `setLocation(x, y)`: integer coordinates move exactly, fractional ones
    /// round with `floor(value + 0.5)`.
    pub fn set_location(&mut self, x: f64, y: f64) {
        if x == x.trunc() && y == y.trunc() {
            self.move_to(x, y);
        } else {
            self.x = (x + 0.5).floor();
            self.y = (y + 0.5).floor();
        }
    }

    /// Upstream `setLocation(point)`.
    pub fn set_location_from(&mut self, point: &Point) {
        self.set_location(point.x, point.y);
    }

    /// Upstream `move`.
    pub fn move_to(&mut self, x: f64, y: f64) {
        self.x = x;
        self.y = y;
    }

    /// Upstream `translate`.
    pub fn translate(&mut self, dx: f64, dy: f64) {
        self.x += dx;
        self.y += dy;
    }

    /// Upstream `equals`.
    pub fn equals(&self, other: &Point) -> bool {
        self.x == other.x && self.y == other.y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn location_rounds_only_fractions() {
        let mut point = Point::new(1.25, -2.75);
        point.set_location(3.0, 4.0);
        assert_eq!(point, Point::new(3.0, 4.0));

        point.set_location(3.4, 4.6);
        assert_eq!(point, Point::new(3.0, 5.0));

        point.set_location(-3.4, -4.6);
        assert_eq!(point, Point::new(-3.0, -5.0));
    }

    #[test]
    fn copies_and_translates() {
        let mut point = Point::new(1.0, 2.0);
        let copy = point.get_location();
        point.translate(2.0, -1.0);
        assert_eq!(point, Point::new(3.0, 1.0));
        assert!(copy.equals(&Point::new(1.0, 2.0)));
        assert!(!copy.equals(&point));
    }
}
