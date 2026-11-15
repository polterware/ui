//! `EchoRing`, the dot matrix loader the old desktop app's boot splash drew (Dot Matrix's
//! "Echo Ring", `apps/polterware-desktop/src/renderer/boot-splash.css`): a 5×5 grid of 4px
//! square dots 4px apart, each in a diamond ring by its Manhattan distance from the center,
//! and an echo pulse running out from the center every 1.5s.
//!
//! Each dot cycles through the CSS keyframes (rest, primary at 28%, muted at 56%, a mix at
//! 78%, rest again), eased in and out between them, its cycle shifted by its ring (0.14 of the
//! period a ring) and by its ring's parity (0.03 more on odd rings), so the first frame
//! already shows the ripple as the CSS's negative delays do. Every loader on screen shares
//! one clock, and it draws frames only while one shows.

use std::sync::OnceLock;
use std::time::Instant;

use gpui::{App, Div, Rgba, Window, div, prelude::*, px};
use polterware_theme::Ease;

use crate::theme::{Theme, mix};

/// `boot-splash-echo`'s period, in seconds.
const PERIOD: f32 = 1.5;
/// Dots a side, and each dot's side and the gap between them, in pixels.
const SIDE: usize = 5;
const DOT: f32 = 4.0;
const GAP: f32 = 4.0;
/// `ease-in-out`, which CSS applies between each pair of keyframes.
const EASE_IN_OUT: Ease = Ease(0.42, 0.0, 0.58, 1.0);

fn epoch() -> Instant {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    *EPOCH.get_or_init(Instant::now)
}

/// The dot colors at the keyframes, from the theme.
struct Keys {
    rest: Rgba,
    primary: Rgba,
    muted: Rgba,
    echo: Rgba,
}

impl Keys {
    fn new(theme: &Theme) -> Self {
        Self {
            // `color-mix(in srgb, muted 45%, surface-selected)`.
            rest: mix(theme.color_surface_selected, theme.color_text_muted, 0.45),
            primary: theme.color_text_primary,
            muted: theme.color_text_muted,
            // `color-mix(in srgb, primary 60%, muted)`.
            echo: mix(theme.color_text_muted, theme.color_text_primary, 0.6),
        }
    }

    /// The color at `phase` (0 to 1) of the cycle.
    fn at(&self, phase: f32) -> Rgba {
        let stops = [
            (0.0, self.rest),
            (0.28, self.primary),
            (0.56, self.muted),
            (0.78, self.echo),
            (1.0, self.rest),
        ];
        for pair in stops.windows(2) {
            let ((from_at, from), (to_at, to)) = (pair[0], pair[1]);
            if phase <= to_at {
                let t = ((phase - from_at) / (to_at - from_at)).clamp(0.0, 1.0);
                return mix(from, to, EASE_IN_OUT.at(t));
            }
        }
        self.rest
    }
}

/// Where the dot at `(row, column)` is in its cycle at `elapsed` seconds: its ring's offset as
/// the CSS's `animation-delay: (r * 0.14 + p * 0.03 - 1) * 1500ms`.
fn phase(row: usize, column: usize, elapsed: f32) -> f32 {
    let center = (SIDE / 2) as i32;
    let ring = (row as i32 - center).abs() + (column as i32 - center).abs();
    let parity = ring % 2;
    let delay = ring as f32 * 0.14 + parity as f32 * 0.03 - 1.0;
    ((elapsed / PERIOD) - delay).rem_euclid(1.0)
}

/// The Echo Ring loader, 36px square. It asks for the next frame while it draws.
pub fn echo_ring(window: &mut Window, cx: &App) -> Div {
    let theme = Theme::global(cx);
    window.request_animation_frame();
    let keys = Keys::new(theme);
    let elapsed = epoch().elapsed().as_secs_f32();
    let mut grid = div().flex_none().flex().flex_col().gap(px(GAP));
    for row in 0..SIDE {
        let mut line = div().flex().gap(px(GAP));
        for column in 0..SIDE {
            line = line.child(div().size(px(DOT)).bg(keys.at(phase(row, column, elapsed))));
        }
        grid = grid.child(line);
    }
    grid
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_echo_starts_at_the_center_and_runs_out_ring_by_ring() {
        // At the same moment, each ring is 0.14 (plus parity) of a cycle behind the last.
        let center = phase(2, 2, 0.0);
        let ring1 = phase(1, 2, 0.0);
        let ring2 = phase(0, 2, 0.0);
        let behind = |a: f32, b: f32| (a - b).rem_euclid(1.0);
        assert!(
            (behind(center, ring1) - 0.17).abs() < 1e-4,
            "{center} {ring1}"
        );
        assert!(
            (behind(ring1, ring2) - 0.11).abs() < 1e-4,
            "{ring1} {ring2}"
        );
        // The diamond: the same Manhattan distance, the same phase.
        assert_eq!(phase(1, 2, 0.4), phase(2, 1, 0.4));
        assert_eq!(phase(0, 0, 0.4), phase(4, 4, 0.4));
    }

    #[test]
    fn the_keyframes_hold_their_colors() {
        let theme = Theme::dark();
        let keys = Keys::new(&theme);
        assert_eq!(keys.at(0.0), keys.rest);
        assert_eq!(keys.at(0.28), keys.primary);
        assert_eq!(keys.at(0.56), keys.muted);
        assert_eq!(keys.at(1.0), keys.rest);
    }
}
