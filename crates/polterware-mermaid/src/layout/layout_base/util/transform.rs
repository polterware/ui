//! Port of `layout-base/src/util/Transform.js`.
//!
//! Upstream's affine mapping between world coordinates and device coordinates, with the
//! origin and extent of each space. Both spaces are one-to-one until a caller sets a
//! different extent, so the default transform maps a point to itself.

use super::point_d::PointD;

/// The world-to-device mapping, upstream's `Transform`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    /// World origin X, upstream's `lworldOrgX`.
    pub world_org_x: f64,
    /// World origin Y, upstream's `lworldOrgY`.
    pub world_org_y: f64,
    /// Device origin X, upstream's `ldeviceOrgX`.
    pub device_org_x: f64,
    /// Device origin Y, upstream's `ldeviceOrgY`.
    pub device_org_y: f64,
    /// World extent X, upstream's `lworldExtX`.
    pub world_ext_x: f64,
    /// World extent Y, upstream's `lworldExtY`.
    pub world_ext_y: f64,
    /// Device extent X, upstream's `ldeviceExtX`.
    pub device_ext_x: f64,
    /// Device extent Y, upstream's `ldeviceExtY`.
    pub device_ext_y: f64,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            world_org_x: 0.0,
            world_org_y: 0.0,
            device_org_x: 0.0,
            device_org_y: 0.0,
            world_ext_x: 1.0,
            world_ext_y: 1.0,
            device_ext_x: 1.0,
            device_ext_y: 1.0,
        }
    }
}

impl Transform {
    /// The identity transform. Upstream's constructor takes two unused arguments.
    pub fn new() -> Self {
        Self::default()
    }

    /// Upstream `getWorldOrgX`.
    pub fn get_world_org_x(&self) -> f64 {
        self.world_org_x
    }

    /// Upstream `setWorldOrgX`.
    pub fn set_world_org_x(&mut self, world_org_x: f64) {
        self.world_org_x = world_org_x;
    }

    /// Upstream `getWorldOrgY`.
    pub fn get_world_org_y(&self) -> f64 {
        self.world_org_y
    }

    /// Upstream `setWorldOrgY`.
    pub fn set_world_org_y(&mut self, world_org_y: f64) {
        self.world_org_y = world_org_y;
    }

    /// Upstream `getWorldExtX`.
    pub fn get_world_ext_x(&self) -> f64 {
        self.world_ext_x
    }

    /// Upstream `setWorldExtX`.
    pub fn set_world_ext_x(&mut self, world_ext_x: f64) {
        self.world_ext_x = world_ext_x;
    }

    /// Upstream `getWorldExtY`.
    pub fn get_world_ext_y(&self) -> f64 {
        self.world_ext_y
    }

    /// Upstream `setWorldExtY`.
    pub fn set_world_ext_y(&mut self, world_ext_y: f64) {
        self.world_ext_y = world_ext_y;
    }

    /// Upstream `getDeviceOrgX`.
    pub fn get_device_org_x(&self) -> f64 {
        self.device_org_x
    }

    /// Upstream `setDeviceOrgX`.
    pub fn set_device_org_x(&mut self, device_org_x: f64) {
        self.device_org_x = device_org_x;
    }

    /// Upstream `getDeviceOrgY`.
    pub fn get_device_org_y(&self) -> f64 {
        self.device_org_y
    }

    /// Upstream `setDeviceOrgY`.
    pub fn set_device_org_y(&mut self, device_org_y: f64) {
        self.device_org_y = device_org_y;
    }

    /// Upstream `getDeviceExtX`.
    pub fn get_device_ext_x(&self) -> f64 {
        self.device_ext_x
    }

    /// Upstream `setDeviceExtX`.
    pub fn set_device_ext_x(&mut self, device_ext_x: f64) {
        self.device_ext_x = device_ext_x;
    }

    /// Upstream `getDeviceExtY`.
    pub fn get_device_ext_y(&self) -> f64 {
        self.device_ext_y
    }

    /// Upstream `setDeviceExtY`.
    pub fn set_device_ext_y(&mut self, device_ext_y: f64) {
        self.device_ext_y = device_ext_y;
    }

    /// Upstream `transformX`: zero when the world extent is zero.
    pub fn transform_x(&self, x: f64) -> f64 {
        let mut x_device = 0.0;
        let world_ext_x = self.world_ext_x;
        if world_ext_x != 0.0 {
            x_device =
                self.device_org_x + ((x - self.world_org_x) * self.device_ext_x / world_ext_x);
        }

        x_device
    }

    /// Upstream `transformY`: zero when the world extent is zero.
    pub fn transform_y(&self, y: f64) -> f64 {
        let mut y_device = 0.0;
        let world_ext_y = self.world_ext_y;
        if world_ext_y != 0.0 {
            y_device =
                self.device_org_y + ((y - self.world_org_y) * self.device_ext_y / world_ext_y);
        }

        y_device
    }

    /// Upstream `inverseTransformX`: zero when the device extent is zero.
    pub fn inverse_transform_x(&self, x: f64) -> f64 {
        let mut x_world = 0.0;
        let device_ext_x = self.device_ext_x;
        if device_ext_x != 0.0 {
            x_world =
                self.world_org_x + ((x - self.device_org_x) * self.world_ext_x / device_ext_x);
        }

        x_world
    }

    /// Upstream `inverseTransformY`: zero when the device extent is zero.
    pub fn inverse_transform_y(&self, y: f64) -> f64 {
        let mut y_world = 0.0;
        let device_ext_y = self.device_ext_y;
        if device_ext_y != 0.0 {
            y_world =
                self.world_org_y + ((y - self.device_org_y) * self.world_ext_y / device_ext_y);
        }
        y_world
    }

    /// Upstream `inverseTransformPoint`.
    pub fn inverse_transform_point(&self, point: &PointD) -> PointD {
        PointD::new(
            self.inverse_transform_x(point.x),
            self.inverse_transform_y(point.y),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scaled() -> Transform {
        let mut transform = Transform::new();
        transform.set_world_org_x(10.0);
        transform.set_world_org_y(20.0);
        transform.set_device_org_x(100.0);
        transform.set_device_org_y(200.0);
        transform.set_world_ext_x(50.0);
        transform.set_world_ext_y(100.0);
        transform.set_device_ext_x(500.0);
        transform.set_device_ext_y(1000.0);
        transform
    }

    #[test]
    fn identity_maps_a_point_to_itself() {
        let transform = Transform::new();
        assert_eq!(transform.transform_x(5.0), 5.0);
        assert_eq!(transform.transform_y(5.0), 5.0);
        assert_eq!(transform.inverse_transform_x(5.0), 5.0);
        assert_eq!(transform.inverse_transform_y(5.0), 5.0);
    }

    #[test]
    fn scales_around_the_origins() {
        // Expected values from upstream:
        // node -e 'const T = require(".../util/Transform.js"); ...' (see the module doc).
        let transform = scaled();
        assert_eq!(transform.transform_x(60.0), 600.0);
        assert_eq!(transform.transform_y(120.0), 1200.0);
        assert_eq!(transform.inverse_transform_x(600.0), 60.0);
        assert_eq!(transform.inverse_transform_y(1200.0), 120.0);
        assert_eq!(
            transform.inverse_transform_point(&PointD::new(600.0, 1200.0)),
            PointD::new(60.0, 120.0)
        );
    }

    #[test]
    fn zero_extent_maps_to_the_origin() {
        let mut transform = Transform::new();
        transform.set_world_ext_x(0.0);
        transform.set_world_ext_y(0.0);
        transform.set_device_ext_x(0.0);
        transform.set_device_ext_y(0.0);
        assert_eq!(transform.transform_x(5.0), 0.0);
        assert_eq!(transform.transform_y(5.0), 0.0);
        assert_eq!(transform.inverse_transform_x(5.0), 0.0);
        assert_eq!(transform.inverse_transform_y(5.0), 0.0);
    }
}
