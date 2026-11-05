//! `EchoRing` (Vitals' `EchoRing.tsx` over `boot-splash.css`'s `.echo-ring`): the boot
//! splash's echo pulse as a progress mark. A 5 × 5 grid of square dots `--echo-dot` wide
//! (4px) and as far apart; each dot's Manhattan distance from the center (`--r`) and checker
//! parity (`--p`) delay its pulse by `(r × 0.14 + p × 0.03 − 1) × 1500ms`, so diamond rings
//! ripple out from the middle, and the negative delays start the ripple mid-cycle, so the
//! first frame already shows it. Every 1.5s a dot goes from its rest color
//! (`--echo-dot-rest`, `color-mix(in srgb, --color-text-muted 45%, --color-surface-selected)`)
//! to the peak (`--echo-peak`, `--color-text-primary`) at 28%, to `--color-text-muted` at 56%,
//! to a 60% mix of the peak and muted ink at 78%, and back, eased in and out between the steps
//! (`ease-in-out`). Still (`prefers-reduced-motion`), each ring is a step dimmer than the one
//! inside it: muted ink at `100% − r × 22%` over `--color-surface-selected`.
//!
//! It behaves like gpui-kit's `Spinner`: an element with nothing to configure but its size and
//! colors, which runs one repeating animation (`with_animation`) and asks for frames only
//! while it is drawn.

use std::time::Duration;

use gpui::{
    Animation, AnimationExt, App, ElementId, IntoElement, RenderOnce, Rgba, Window, div,
    prelude::*, px,
};

use crate::theme::{Ease, Theme, mix};

/// One pulse.
pub const ECHO_PERIOD: Duration = Duration::from_millis(1500);
/// `ease-in-out`, between each pair of keyframes.
const EASE_IN_OUT: Ease = Ease(0.42, 0.0, 0.58, 1.0);

/// The three inks a dot pulses through.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EchoColors {
    pub rest: Rgba,
    pub peak: Rgba,
    pub muted: Rgba,
}

impl EchoColors {
    /// The boot splash's: `.echo-ring`'s defaults.
    pub fn splash(theme: &Theme) -> Self {
        Self {
            rest: mix(theme.color_surface_selected, theme.color_text_muted, 0.45),
            peak: theme.color_text_primary,
            muted: theme.color_text_muted,
        }
    }
}

/// A dot's distance from the center (`--r`) and its checker parity (`--p`), row by row.
fn dot_rings() -> impl Iterator<Item = (u8, u8)> {
    (0..25u8).map(|index| {
        let (x, y) = (index % 5, index / 5);
        (x.abs_diff(2) + y.abs_diff(2), (x + y) % 2)
    })
}

/// The color of a dot `r` rings out and of parity `p`, `t` (0 to 1) of the way through the
/// ring's cycle.
pub fn echo_color(colors: &EchoColors, r: u8, p: u8, t: f32) -> Rgba {
    let delay = f32::from(r) * 0.14 + f32::from(p) * 0.03;
    let phase = (t - delay).rem_euclid(1.0);
    let echo = mix(colors.muted, colors.peak, 0.6);
    let keys = [
        (0.0, colors.rest),
        (0.28, colors.peak),
        (0.56, colors.muted),
        (0.78, echo),
        (1.0, colors.rest),
    ];
    for pair in keys.windows(2) {
        let ((from_at, from), (to_at, to)) = (pair[0], pair[1]);
        if phase <= to_at {
            let local = ((phase - from_at) / (to_at - from_at)).clamp(0.0, 1.0);
            return mix(from, to, EASE_IN_OUT.at(local));
        }
    }
    colors.rest
}

/// A still dot `r` rings out: see the module's docs.
pub fn echo_still_color(theme: &Theme, r: u8) -> Rgba {
    let share = (1.0 - f32::from(r) * 0.22).max(0.0);
    mix(theme.color_surface_selected, theme.color_text_muted, share)
}

/// The echo ring: see the module's docs.
#[derive(IntoElement)]
pub struct EchoRing {
    id: ElementId,
    dot: f32,
    rest: Option<Rgba>,
    peak: Option<Rgba>,
    still: bool,
}

impl EchoRing {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            dot: 4.0,
            rest: None,
            peak: None,
            still: false,
        }
    }

    /// `--echo-dot`: each dot's side, and the gap between them.
    pub fn dot(mut self, side: f32) -> Self {
        self.dot = side;
        self
    }

    /// `--echo-dot-rest`.
    pub fn rest(mut self, color: Rgba) -> Self {
        self.rest = Some(color);
        self
    }

    /// `--echo-peak`.
    pub fn peak(mut self, color: Rgba) -> Self {
        self.peak = Some(color);
        self
    }

    /// Drawn still, as with reduced motion: it asks for no frames.
    pub fn still(mut self, still: bool) -> Self {
        self.still = still;
        self
    }
}

impl RenderOnce for EchoRing {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let splash = EchoColors::splash(&theme);
        let colors = EchoColors {
            rest: self.rest.unwrap_or(splash.rest),
            peak: self.peak.unwrap_or(splash.peak),
            muted: splash.muted,
        };
        let side = self.dot;
        let ring = move |paint: &dyn Fn(u8, u8) -> Rgba| {
            div()
                .flex_none()
                .size(px(side * 9.0))
                .grid()
                .grid_cols(5)
                .grid_rows(5)
                .gap(px(side))
                .children(
                    dot_rings().map(|(r, p)| div().size(px(side)).flex_none().bg(paint(r, p))),
                )
        };
        if self.still {
            return ring(&|r, _| echo_still_color(&theme, r)).into_any_element();
        }
        div()
            .flex_none()
            .with_animation(
                self.id,
                Animation::new(ECHO_PERIOD).repeat(),
                move |frame, t| frame.child(ring(&|r, p| echo_color(&colors, r, p, t))),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use gpui::{TestAppContext, div, prelude::*, px, size};

    use super::*;
    use crate::test_support::{draw, open};

    fn colors() -> EchoColors {
        EchoColors::splash(&Theme::dark())
    }

    #[test]
    fn the_rings_are_diamonds_around_the_center() {
        let rings: Vec<u8> = dot_rings().map(|(r, _)| r).collect();
        assert_eq!(rings[12], 0, "the center");
        assert_eq!(&rings[0..5], &[4, 3, 2, 3, 4]);
        assert_eq!(rings.iter().filter(|r| **r == 1).count(), 4);
        let parity: Vec<u8> = dot_rings().map(|(_, p)| p).collect();
        assert_eq!(&parity[0..3], &[0, 1, 0]);
    }

    fn same(a: Rgba, b: Rgba) -> bool {
        [a.r - b.r, a.g - b.g, a.b - b.b, a.a - b.a]
            .iter()
            .all(|delta| delta.abs() < 1e-3)
    }

    #[test]
    fn a_dot_pulses_through_its_keyframes() {
        let colors = colors();
        // The center starts its cycle at 0: rest, peak at 28%, muted at 56%, back to rest.
        assert!(same(echo_color(&colors, 0, 0, 0.0), colors.rest));
        assert!(same(echo_color(&colors, 0, 0, 0.28), colors.peak));
        assert!(same(echo_color(&colors, 0, 0, 0.56), colors.muted));
        let echo = mix(colors.muted, colors.peak, 0.6);
        assert!(same(echo_color(&colors, 0, 0, 0.78), echo));
        assert!(same(echo_color(&colors, 0, 0, 1.0), colors.rest));
        // Each ring peaks 0.14 of a cycle after the one inside it; odd dots 0.03 later.
        assert!(same(echo_color(&colors, 1, 0, 0.42), colors.peak));
        assert!(same(echo_color(&colors, 2, 1, 0.28 + 0.31), colors.peak));
        // Between two keys the ease starts and ends slow.
        let early = echo_color(&colors, 0, 0, 0.028);
        let quarter = (early.r - colors.rest.r) / (colors.peak.r - colors.rest.r);
        assert!(quarter > 0.0 && quarter < 0.1, "{quarter}");
    }

    #[test]
    fn still_rings_dim_outward() {
        let theme = Theme::dark();
        assert!(same(echo_still_color(&theme, 0), theme.color_text_muted));
        let one = echo_still_color(&theme, 1);
        let four = echo_still_color(&theme, 4);
        assert!(one.r > four.r);
        assert!(four.r > theme.color_surface_selected.r);
    }

    #[gpui::test]
    fn the_ring_is_nine_dots_square(cx: &mut TestAppContext) {
        let mut cx = open(cx, size(200.0, 200.0), |_, _| {
            div()
                .flex()
                .items_start()
                .child(
                    div()
                        .debug_selector(|| "ring".into())
                        .child(EchoRing::new("ring").dot(3.0)),
                )
                .child(
                    div()
                        .debug_selector(|| "still".into())
                        .child(EchoRing::new("still").still(true)),
                )
                .into_any_element()
        });
        draw(&mut cx);
        let ring = cx.debug_bounds("ring").expect("the ring draws");
        assert_eq!(ring.size.width, px(27.0));
        assert_eq!(ring.size.height, px(27.0));
        let still = cx.debug_bounds("still").expect("the still ring draws");
        assert_eq!(still.size.width, px(36.0));
    }
}
