//! Port of `layout-base/src/util/DimensionD.js`.
//!
//! A width and a height. Upstream's constructor leaves both at zero when either
//! argument is `null`; [`DimensionD::default`] is that empty pair.

/// A width and a height, upstream's `DimensionD`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DimensionD {
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
}

impl DimensionD {
    /// A width and a height.
    pub fn new(width: f64, height: f64) -> Self {
        Self { width, height }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_by_default() {
        let dimension = DimensionD::default();
        assert_eq!((dimension.width, dimension.height), (0.0, 0.0));
    }

    #[test]
    fn setters_keep_the_pair() {
        let mut dimension = DimensionD::new(3.0, 4.0);
        assert_eq!(dimension.get_width(), 3.0);
        assert_eq!(dimension.get_height(), 4.0);
        dimension.set_width(5.0);
        dimension.set_height(6.0);
        assert_eq!(dimension, DimensionD::new(5.0, 6.0));
    }
}
