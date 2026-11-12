//! Port of `d3-sankey` 0.12.3 `src/constant.js`.
//!
//! Upstream's `constant(x)` is how `nodeId` and `nodeAlign` turn a non-function
//! argument into an accessor. This port's nodes carry their id and its alignment is
//! [`super::SankeyAlignment`], so nothing in the pipeline needs it; it stays here
//! because it is part of the module being ported, and its test keeps the shape.

/// `constant(x)`: a function that always returns `x`.
pub fn constant<T: Clone>(value: T) -> impl Fn() -> T {
    move || value.clone()
}

#[cfg(test)]
mod tests {
    use super::constant;

    #[test]
    fn returns_its_value() {
        let eight = constant(8.0);
        assert_eq!(eight(), 8.0);
        assert_eq!(eight(), 8.0);
    }
}
