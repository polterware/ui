//! Port of `layout-base/src/util/RandomSeed.js`, plus the seeded `Math.random` Mermaid
//! installs for cytoscape-fcose.
//!
//! Upstream keeps one process-wide `seed` (starting at `1`) and one `x`, and
//! `nextDouble` returns the fraction of `sin(seed++) * 10000`. This port keeps the same
//! sequence per thread, so layouts running on different threads stay deterministic, and
//! adds [`set_seed`] so a caller can repeat a layout (upstream's seed is only ever `1`).
//!
//! cose-bilkent draws from `RandomSeed`; cytoscape-fcose draws from `Math.random()`, and
//! Mermaid replaces `Math.random` with the mulberry32 generator of
//! `architectureSeed.ts` around each `layout.run()`. [`set_math_random_seed`] selects the
//! same generator here, so the fcose port draws the same numbers in the same order.
//!
//! `Math.sin` and [`f64::sin`] may differ in the last bit; JavaScript only promises an
//! implementation-approximated sine, so the sequence can differ by that much.

use std::cell::Cell;

thread_local! {
    /// Upstream's `RandomSeed.seed`, plus the `x` it last computed, and the mode.
    static STATE: Cell<State> = const { Cell::new(State::Seed { seed: 1, x: 0.0 }) };
}

/// Which generator a thread's [`next_double`] draws from.
#[derive(Clone, Copy, Debug, PartialEq)]
enum State {
    /// layout-base's `RandomSeed.nextDouble`.
    Seed { seed: i64, x: f64 },
    /// The mulberry32 generator of Mermaid's `withSeededRandom`, upstream's temporarily
    /// replaced `Math.random`.
    MathRandom { state: u32 },
}

/// Sets the next seed, upstream's `RandomSeed.seed`. Not upstream; it makes a layout
/// repeatable in tests.
pub fn set_seed(seed: i64) {
    STATE.with(|state| state.set(State::Seed { seed, x: 0.0 }));
}

/// Selects Mermaid's seeded `Math.random`: mulberry32 with `seed` (a JavaScript `>>> 0`
/// seed). A seed of `0` in Mermaid leaves the real `Math.random` alone; this port always
/// seeds, because a layout must repeat, so seed `0` is the mulberry32 seed zero.
pub fn set_math_random_seed(seed: u32) {
    STATE.with(|state| state.set(State::MathRandom { state: seed }));
}

/// The last value `sin(seed) * 10000` produced, upstream's `RandomSeed.x`. Zero while
/// the seeded `Math.random` mode is selected.
pub fn last_x() -> f64 {
    STATE.with(|state| match state.get() {
        State::Seed { x, .. } => x,
        State::MathRandom { .. } => 0.0,
    })
}

/// Upstream `RandomSeed.nextDouble`, or the seeded `Math.random` when
/// [`set_math_random_seed`] selected it.
pub fn next_double() -> f64 {
    STATE.with(|state| match state.get() {
        State::Seed { seed, .. } => {
            let x = (seed as f64).sin() * 10000.0;
            state.set(State::Seed { seed: seed + 1, x });
            x - x.floor()
        }
        State::MathRandom {
            state: random_state,
        } => {
            let random_state = random_state.wrapping_add(0x6d2b79f5);
            let mut t = random_state;
            t = (t ^ (t >> 15)).wrapping_mul(t | 1);
            t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
            STATE.with(|state| {
                state.set(State::MathRandom {
                    state: random_state,
                })
            });
            ((t ^ (t >> 14)) as f64) / 4294967296.0
        }
    })
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

    #[test]
    fn matches_mermaids_seeded_math_random() {
        // node -e '
        //   let state = 1 >>> 0; const values = [];
        //   for (let i = 0; i < 4; i++) {
        //     state = (state + 0x6d2b79f5) >>> 0;
        //     let t = state;
        //     t = Math.imul(t ^ (t >>> 15), t | 1);
        //     t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
        //     values.push(((t ^ (t >>> 14)) >>> 0) / 4294967296);
        //   }
        //   console.log(values);'
        set_math_random_seed(1);
        let values: Vec<f64> = (0..4).map(|_| next_double()).collect();
        let expected = [
            0.6270739405881613,
            0.002735721180215478,
            0.5274470399599522,
            0.9810509674716741,
        ];
        for (value, expected) in values.iter().zip(expected) {
            assert_eq!(*value, expected);
        }
    }
}
