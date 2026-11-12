//! Port of `layout-base/src/util/RandomSeed.js`.
//!
//! Upstream keeps one process-wide `seed` (starting at `1`) and one `x`, and
//! `nextDouble` returns the fraction of `sin(seed++) * 10000`. This port keeps the same
//! sequence per thread, so layouts running on different threads stay deterministic, and
//! adds [`set_seed`] so a caller can repeat a layout (upstream's seed is only ever `1`).
//!
//! `Math.sin` and [`f64::sin`] may differ in the last bit; JavaScript only promises an
//! implementation-approximated sine, so the sequence can differ by that much.

use std::cell::Cell;

thread_local! {
    /// Upstream's `RandomSeed.seed`, plus the `x` it last computed.
    static STATE: Cell<(i64, f64)> = const { Cell::new((1, 0.0)) };
}

/// Sets the next seed, upstream's `RandomSeed.seed`. Not upstream; it makes a layout
/// repeatable in tests.
pub fn set_seed(seed: i64) {
    STATE.with(|state| state.set((seed, 0.0)));
}

/// The last value `sin(seed) * 10000` produced, upstream's `RandomSeed.x`.
pub fn last_x() -> f64 {
    STATE.with(|state| state.get().1)
}

/// Upstream `RandomSeed.nextDouble`: the fraction of `sin(seed++) * 10000`.
pub fn next_double() -> f64 {
    let seed = STATE.with(|state| {
        let (seed, _) = state.get();
        state.set((seed + 1, 0.0));
        seed
    });
    let x = (seed as f64).sin() * 10000.0;
    STATE.with(|state| {
        let (seed, _) = state.get();
        state.set((seed, x));
    });
    x - x.floor()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_upstream_sequence() {
        // node -e 'const R = require(".../util/RandomSeed.js");
        //          console.log([R.nextDouble(), R.nextDouble(), R.nextDouble()])'
        set_seed(1);
        let values: Vec<f64> = (0..3).map(|_| next_double()).collect();
        let expected = [0.7098480789645691, 0.9742682568175951, 0.20008059867222983];
        for (value, expected) in values.iter().zip(expected) {
            assert!((value - expected).abs() < 1e-12, "{value} != {expected}");
        }
        assert_eq!(last_x(), (3.0f64).sin() * 10000.0);
    }
}
