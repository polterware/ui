//! Port of `layout-base/src/util/RectangleD.js`.
//!
//! An axis-aligned rectangle with real coordinates. Upstream's constructor leaves every
//! field at zero when one of the four arguments is `null`; [`RectangleD::default`] is
//! that empty rectangle.

/// A rectangle, upstream's `RectangleD`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RectangleD {
    /// Left edge.
    pub x: f64,
    /// Top edge.
    pub y: f64,
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
}

impl RectangleD {
    /// A rectangle at `(x, y)` with the given size.
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Upstream `getX`.
    pub fn get_x(&self) -> f64 {
        self.x
    }

    /// Upstream `setX`.
    pub fn set_x(&mut self, x: f64) {
        self.x = x;
    }

    /// Upstream `getY`.
    pub fn get_y(&self) -> f64 {
        self.y
    }

    /// Upstream `setY`.
    pub fn set_y(&mut self, y: f64) {
        self.y = y;
    }

    /// Upstream `getWidth`.
    pub fn get_width(&self) -> f64 {
        self.width
    }

    /// Upstream `setWidth`.
    pub fn set_width(&mut self, width: f64) {
        self.width = width;
    }

    /// Upstream `getHeight`.
    pub fn get_height(&self) -> f64 {
        self.height
    }

    /// Upstream `setHeight`.
    pub fn set_height(&mut self, height: f64) {
        self.height = height;
    }

    /// Upstream `getRight`.
    pub fn get_right(&self) -> f64 {
        self.x + self.width
    }

    /// Upstream `getBottom`.
    pub fn get_bottom(&self) -> f64 {
        self.y + self.height
    }

    /// Upstream `intersects`: touching edges intersect.
    pub fn intersects(&self, other: &RectangleD) -> bool {
        if self.get_right() < other.x {
            return false;
        }

        if self.get_bottom() < other.y {
            return false;
        }

        if other.get_right() < self.x {
            return false;
        }

        if other.get_bottom() < self.y {
            return false;
        }

        true
    }

    /// Upstream `getCenterX`.
    pub fn get_center_x(&self) -> f64 {
        self.x + self.width / 2.0
    }

    /// Upstream `getMinX`.
    pub fn get_min_x(&self) -> f64 {
        self.get_x()
    }

    /// Upstream `getMaxX`.
    pub fn get_max_x(&self) -> f64 {
        self.get_x() + self.width
    }

    /// Upstream `getCenterY`.
    pub fn get_center_y(&self) -> f64 {
        self.y + self.height / 2.0
    }

    /// Upstream `getMinY`.
    pub fn get_min_y(&self) -> f64 {
        self.get_y()
    }

    /// Upstream `getMaxY`.
    pub fn get_max_y(&self) -> f64 {
        self.get_y() + self.height
    }

    /// Upstream `getWidthHalf`.
    pub fn get_width_half(&self) -> f64 {
        self.width / 2.0
    }

    /// Upstream `getHeightHalf`.
    pub fn get_height_half(&self) -> f64 {
        self.height / 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_and_center() {
        let rect = RectangleD::new(0.0, 0.0, 100.0, 50.0);
        assert_eq!(rect.get_right(), 100.0);
        assert_eq!(rect.get_bottom(), 50.0);
        assert_eq!(rect.get_center_x(), 50.0);
        assert_eq!(rect.get_center_y(), 25.0);
        assert_eq!(rect.get_min_x(), 0.0);
        assert_eq!(rect.get_max_x(), 100.0);
        assert_eq!(rect.get_min_y(), 0.0);
        assert_eq!(rect.get_max_y(), 50.0);
        assert_eq!(rect.get_width_half(), 50.0);
        assert_eq!(rect.get_height_half(), 25.0);
    }

    #[test]
    fn touching_edges_intersect() {
        let rect = RectangleD::new(0.0, 0.0, 100.0, 50.0);
        assert!(rect.intersects(&RectangleD::new(100.0, 50.0, 10.0, 10.0)));
        assert!(rect.intersects(&RectangleD::new(-10.0, -10.0, 10.0, 10.0)));
        assert!(!rect.intersects(&RectangleD::new(101.0, 50.0, 10.0, 10.0)));
        assert!(!rect.intersects(&RectangleD::new(0.0, 51.0, 100.0, 10.0)));
        assert!(!rect.intersects(&RectangleD::new(-20.0, 0.0, 10.0, 10.0)));
        assert!(!rect.intersects(&RectangleD::new(0.0, -20.0, 100.0, 10.0)));
    }

    #[test]
    fn empty_rectangle_is_zero() {
        assert_eq!(RectangleD::default(), RectangleD::new(0.0, 0.0, 0.0, 0.0));
    }
}
