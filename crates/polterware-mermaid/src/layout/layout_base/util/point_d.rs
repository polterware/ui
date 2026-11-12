//! Port of `layout-base/src/util/PointD.js`.
//!
//! A real-valued point, the one layout-base moves nodes with.

use super::dimension_d::DimensionD;

/// A point with fractional coordinates, upstream's `PointD`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PointD {
    /// X coordinate.
    pub x: f64,
    /// Y coordinate.
    pub y: f64,
}

impl PointD {
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

    /// Upstream `setX`.
    pub fn set_x(&mut self, x: f64) {
        self.x = x;
    }

    /// Upstream `setY`.
    pub fn set_y(&mut self, y: f64) {
        self.y = y;
    }

    /// Upstream `getDifference`.
    pub fn get_difference(&self, point: &PointD) -> DimensionD {
        DimensionD::new(self.x - point.x, self.y - point.y)
    }

    /// Upstream `getCopy`.
    pub fn get_copy(&self) -> Self {
        Self::new(self.x, self.y)
    }

    /// Upstream `translate`.
    pub fn translate(&mut self, dimension: &DimensionD) {
        self.x += dimension.width;
        self.y += dimension.height;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn difference_and_translation() {
        let point = PointD::new(5.0, 7.0);
        let difference = point.get_difference(&PointD::new(2.0, 3.0));
        assert_eq!(difference, DimensionD::new(3.0, 4.0));

        let mut moved = point;
        moved.translate(&difference);
        assert_eq!(moved, PointD::new(8.0, 11.0));
        assert_eq!(point.get_copy(), PointD::new(5.0, 7.0));
    }
}
