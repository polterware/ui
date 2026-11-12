//! Port of `layout-base/src/util/IMath.js`.
//!
//! Upstream's `floor` and `ceil` round away from zero, not toward negative infinity or
//! positive infinity: `floor(-2.3)` is `-2` and `ceil(2.3)` is `3`.

/// Upstream `IMath.sign`: `1.0`, `-1.0`, or `0.0`.
pub fn sign(value: f64) -> f64 {
    if value > 0.0 {
        1.0
    } else if value < 0.0 {
        -1.0
    } else {
        0.0
    }
}

/// Upstream `IMath.floor`: rounds toward zero for negative values.
pub fn floor(value: f64) -> f64 {
    if value < 0.0 {
        value.ceil()
    } else {
        value.floor()
    }
}

/// Upstream `IMath.ceil`: rounds away from zero for negative values.
pub fn ceil(value: f64) -> f64 {
    if value < 0.0 {
        value.floor()
    } else {
        value.ceil()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_upstream_rounding() {
        assert_eq!(sign(3.0), 1.0);
        assert_eq!(sign(-3.0), -1.0);
        assert_eq!(sign(0.0), 0.0);
        assert_eq!(floor(-2.3), -2.0);
        assert_eq!(floor(2.7), 2.0);
        assert_eq!(ceil(-2.3), -3.0);
        assert_eq!(ceil(2.3), 3.0);
    }
}
