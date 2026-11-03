//! Shared spring motion.
//!
//! [`Spring`] is the frame integrator (moved from `spring.rs`, tests included).
//! [`SpringTier`] is the Polterware set from `packages/frontend/design-system/src/springs.ts`:
//! fast 0.08s bounce 0 (exit 0.06), moderate 0.16s bounce 0 (exit 0.12),
//! slow 0.24s bounce 0.12 (exit 0.16). Duration and bounce convert to stiffness
//! and damping the way SwiftUI / Motion visual-duration springs do:
//! `stiffness = 4π² / duration²`, `dampingRatio = 1 - bounce` when bounce > 0,
//! else `1 / (bounce + 1)` (bounce 0 is critically damped).

use std::f32::consts::PI;
use std::time::Instant;

use gpui::Window;

use polterware_theme::Ease;

const MAX_FRAME_DT: f32 = 0.064;
const MAX_SUBSTEP_DT: f32 = 0.002;
const REST_DISPLACEMENT: f32 = 0.01;
const REST_VELOCITY: f32 = 0.05;

#[derive(Clone, Copy, Debug)]
pub struct Spring {
    pub stiffness: f32,
    pub damping: f32,
    pub mass: f32,
    value: f32,
    velocity: f32,
    target: f32,
}

impl Spring {
    pub fn new(initial: f32, stiffness: f32, damping: f32, mass: f32) -> Self {
        let clean_initial = if initial.is_finite() { initial } else { 0.0 };
        let clean_stiffness = if stiffness.is_finite() && stiffness > 0.001 {
            stiffness
        } else {
            1.0
        };
        let clean_damping = if damping.is_finite() && damping >= 0.0 {
            damping
        } else {
            1.0
        };
        let clean_mass = if mass.is_finite() && mass > 0.001 {
            mass
        } else {
            1.0
        };

        Self {
            stiffness: clean_stiffness,
            damping: clean_damping,
            mass: clean_mass,
            value: clean_initial,
            velocity: 0.0,
            target: clean_initial,
        }
    }

    /// Snappy near-critically damped preset for sliding the sidebar row highlight.
    pub fn selection(initial: f32) -> Self {
        Self::new(initial, 420.0, 38.0, 1.0)
    }

    /// Critically damped preset for workspace section expand/collapse height.
    pub fn collapse(initial: f32) -> Self {
        Self::new(initial, 380.0, 39.5, 1.0)
    }

    pub fn value(&self) -> f32 {
        self.value
    }

    #[allow(dead_code)]
    pub fn target(&self) -> f32 {
        self.target
    }

    #[cfg(test)]
    pub fn velocity(&self) -> f32 {
        self.velocity
    }

    pub(crate) fn velocity_for_callers(&self) -> f32 {
        self.velocity
    }

    pub fn set_target(&mut self, target: f32) {
        if target.is_finite() {
            self.target = target;
            if self.is_settled() {
                self.value = self.target;
                self.velocity = 0.0;
            }
        }
    }

    pub fn snap(&mut self, value: f32) {
        if value.is_finite() {
            self.value = value;
            self.target = value;
            self.velocity = 0.0;
        }
    }

    pub fn is_settled(&self) -> bool {
        (self.value - self.target).abs() <= REST_DISPLACEMENT
            && self.velocity.abs() <= REST_VELOCITY
    }

    /// Integrates the spring over `dt_secs` using fixed-cap symplectic substeps.
    pub fn step(&mut self, dt_secs: f32) -> f32 {
        if !dt_secs.is_finite() || dt_secs <= 0.0 {
            return self.value;
        }
        if self.is_settled() {
            self.value = self.target;
            self.velocity = 0.0;
            return self.value;
        }

        let k = if self.stiffness.is_finite() && self.stiffness > 0.001 {
            self.stiffness
        } else {
            1.0
        };
        let c = if self.damping.is_finite() && self.damping >= 0.0 {
            self.damping
        } else {
            1.0
        };
        let inv_m = if self.mass.is_finite() && self.mass > 0.001 {
            1.0 / self.mass
        } else {
            1.0
        };

        let clamped_dt = dt_secs.min(MAX_FRAME_DT);
        let steps = ((clamped_dt / MAX_SUBSTEP_DT).ceil() as usize).max(1);
        let h = clamped_dt / (steps as f32);

        let mut x = self.value;
        let mut v = self.velocity;

        for _ in 0..steps {
            let force = -k * (x - self.target) - c * v;
            let accel = force * inv_m;
            v += accel * h;
            x += v * h;
            if !x.is_finite() || !v.is_finite() {
                self.value = self.target;
                self.velocity = 0.0;
                return self.value;
            }
        }

        self.value = x;
        self.velocity = v;

        if self.is_settled() {
            self.value = self.target;
            self.velocity = 0.0;
        }

        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::Spring;

    #[test]
    fn settles_to_target() {
        let mut spring = Spring::selection(0.0);
        spring.set_target(156.0);
        assert!(!spring.is_settled());

        for _ in 0..120 {
            spring.step(1.0 / 60.0);
        }
        assert!(spring.is_settled());
        assert_eq!(spring.value(), 156.0);
        assert_eq!(spring.velocity(), 0.0);

        spring.set_target(-48.0);
        assert!(!spring.is_settled());
        for _ in 0..120 {
            spring.step(1.0 / 60.0);
        }
        assert!(spring.is_settled());
        assert_eq!(spring.value(), -48.0);
    }

    #[test]
    fn never_produces_nan() {
        let mut spring = Spring::new(f32::NAN, -10.0, f32::NAN, 0.0);
        assert!(spring.value().is_finite());

        let weird_dts = [
            0.0,
            -0.5,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            10.0,
            0.016,
        ];
        let targets = [100.0, f32::NAN, -250.0, f32::INFINITY, 42.0];

        for &target in &targets {
            spring.set_target(target);
            for &dt in &weird_dts {
                let val = spring.step(dt);
                assert!(val.is_finite(), "value became non-finite for dt={dt}");
                assert!(
                    spring.velocity().is_finite(),
                    "velocity became non-finite for dt={dt}"
                );
            }
        }
    }

    #[test]
    fn bounded_overshoot() {
        // Underdamped spring moving 0 -> 100 must stay within a strict overshoot bound.
        let mut underdamped = Spring::new(0.0, 320.0, 22.0, 1.0);
        underdamped.set_target(100.0);

        let mut peak = 0.0_f32;
        let mut trough = 0.0_f32;
        for _ in 0..180 {
            let v = underdamped.step(1.0 / 120.0);
            peak = peak.max(v);
            trough = trough.min(v);
        }

        assert!(underdamped.is_settled());
        assert!(
            peak >= 100.0,
            "expected spring to reach target, peak={peak}"
        );
        assert!(
            peak - 100.0 <= 18.0,
            "overshoot exceeded 18% bound: peak={peak}"
        );
        assert!(
            trough >= -0.01,
            "unexpected backward travel: trough={trough}"
        );

        // Collapse preset moving 200 -> 0 should not bounce below zero beyond rest tolerance.
        let mut collapse = Spring::collapse(200.0);
        collapse.set_target(0.0);
        let mut min_height = 200.0_f32;
        for _ in 0..180 {
            min_height = min_height.min(collapse.step(1.0 / 120.0));
        }
        assert!(collapse.is_settled());
        assert!(
            min_height >= -0.05,
            "collapse spring overshot below zero: min={min_height}"
        );
    }
}

/// One of the three shared enter springs, plus its quicker exit tween.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpringTier {
    Fast,
    Moderate,
    Slow,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpringParams {
    pub duration: f32,
    pub bounce: f32,
    pub exit_duration: f32,
    pub stiffness: f32,
    pub damping: f32,
    pub mass: f32,
}

impl SpringTier {
    pub fn params(self) -> SpringParams {
        let (duration, bounce, exit_duration) = match self {
            Self::Fast => (0.08, 0.0, 0.06),
            Self::Moderate => (0.16, 0.0, 0.12),
            Self::Slow => (0.24, 0.12, 0.16),
        };
        params_from_visual(duration, bounce, exit_duration)
    }
}

fn params_from_visual(duration: f32, bounce: f32, exit_duration: f32) -> SpringParams {
    let duration = if duration.is_finite() && duration > 0.001 {
        duration
    } else {
        0.16
    };
    let mass = 1.0;
    let stiffness = (4.0 * PI * PI * mass) / (duration * duration);
    let damping_ratio = if bounce > 0.0 {
        (1.0 - bounce).max(0.0)
    } else {
        1.0 / (bounce + 1.0)
    };
    let damping = damping_ratio * 2.0 * (mass * stiffness).sqrt();
    SpringParams {
        duration,
        bounce,
        exit_duration,
        stiffness,
        damping,
        mass,
    }
}

impl SpringTier {
    /// The enter curve, as Motion's `spring({ duration, bounce })` draws it and the CSS twins
    /// (`spring-css.ts`) sample it: it lands on the target exactly at `duration`.
    pub fn curve(self) -> Curve {
        let params = self.params();
        Curve::spring(params.duration, params.bounce)
    }

    pub fn duration(self) -> f32 {
        self.params().duration
    }

    pub fn exit_duration(self) -> f32 {
        self.params().exit_duration
    }

    /// The exit curve, `animate(from, to, { type: "tween", ...spring.<tier>.exit })`: Motion's
    /// default ease for a tween of one value, `easeOut`, as the Electron app draws it.
    pub fn exit_curve(self) -> Curve {
        Curve::Ease(Ease(0.0, 0.0, 0.58, 1.0))
    }
}

/// The shape of a [`Tween`]: a cubic-bezier (CSS `transition-timing-function`) or a Motion
/// spring with no initial velocity.
#[derive(Clone, Copy, Debug)]
pub enum Curve {
    Ease(Ease),
    Spring {
        omega: f32,
        zeta: f32,
        duration: f32,
    },
}

impl Curve {
    /// Motion's `findSpring`: the undamped frequency whose envelope falls to 0.001 at
    /// `duration`, for `dampingRatio = 1 - bounce` (Newton's method, 12 steps).
    pub fn spring(duration: f32, bounce: f32) -> Self {
        const SAFE_MIN: f64 = 0.001;
        let zeta = (1.0 - bounce as f64).clamp(0.05, 1.0);
        let d = (duration as f64).clamp(0.01, 10.0);
        let envelope = |w: f64| -> f64 {
            if zeta < 1.0 {
                let decay = w * zeta;
                SAFE_MIN - decay / (w * (1.0 - zeta * zeta).sqrt()) * (-decay * d).exp()
            } else {
                -SAFE_MIN + (-w * d).exp() * (w * d + 1.0)
            }
        };
        let derivative = |w: f64| -> f64 {
            if zeta < 1.0 {
                let decay = w * zeta;
                let e = zeta * zeta * w * w * d;
                let f = (-decay * d).exp();
                let g = w * w * (1.0 - zeta * zeta).sqrt();
                let factor = if -envelope(w) + SAFE_MIN > 0.0 {
                    -1.0
                } else {
                    1.0
                };
                factor * (-e * f) / g
            } else {
                (-w * d).exp() * (-w * d * d)
            }
        };
        let mut omega = 5.0 / d;
        for _ in 1..12 {
            omega -= envelope(omega) / derivative(omega);
        }
        if !omega.is_finite() || omega <= 0.0 {
            omega = 10.0;
        }
        Curve::Spring {
            omega: omega as f32,
            zeta: zeta as f32,
            duration,
        }
    }

    /// Progress (0 at the start, 1 at the target) at `t` of the way through the duration.
    pub fn at(self, t: f32) -> f32 {
        if t >= 1.0 {
            return 1.0;
        }
        match self {
            Curve::Ease(ease) => ease.at(t),
            Curve::Spring {
                omega,
                zeta,
                duration,
            } => {
                let time = t.max(0.0) * duration;
                if zeta < 1.0 {
                    let damped = omega * (1.0 - zeta * zeta).sqrt();
                    let envelope = (-zeta * omega * time).exp();
                    1.0 - envelope
                        * ((zeta * omega / damped) * (damped * time).sin() + (damped * time).cos())
                } else {
                    1.0 - (1.0 + omega * time) * (-omega * time).exp()
                }
            }
        }
    }
}

/// A value that moves to its target over a fixed duration along a [`Curve`], on the wall
/// clock, the way a CSS transition does. Retargeting starts from wherever it is.
#[derive(Clone, Copy, Debug)]
pub struct Tween {
    from: f32,
    to: f32,
    start: Option<Instant>,
    duration: f32,
    curve: Curve,
}

impl Tween {
    pub fn new(value: f32) -> Self {
        Self {
            from: value,
            to: value,
            start: None,
            duration: 0.0,
            curve: Curve::Ease(Ease(0.0, 0.0, 1.0, 1.0)),
        }
    }

    pub fn value(&self) -> f32 {
        self.value_at(Instant::now())
    }

    pub fn value_at(&self, now: Instant) -> f32 {
        let Some(start) = self.start else {
            return self.to;
        };
        if self.duration <= 0.0 {
            return self.to;
        }
        let t = now.saturating_duration_since(start).as_secs_f32() / self.duration;
        self.from + (self.to - self.from) * self.curve.at(t)
    }

    pub fn target(&self) -> f32 {
        self.to
    }

    pub fn is_animating(&self) -> bool {
        self.start
            .is_some_and(|start| start.elapsed().as_secs_f32() < self.duration)
    }

    /// Moves to `target` over `duration` seconds; a no-op when already heading there.
    pub fn animate_to(&mut self, target: f32, duration: f32, curve: Curve) {
        if (self.to - target).abs() < f32::EPSILON {
            return;
        }
        self.from = self.value();
        self.to = target;
        self.start = Some(Instant::now());
        self.duration = duration;
        self.curve = curve;
    }

    pub fn snap(&mut self, value: f32) {
        self.from = value;
        self.to = value;
        self.start = None;
    }
}

/// A scalar spring bound to a shared tier.
#[derive(Clone, Copy, Debug)]
pub struct SpringValue {
    inner: Spring,
    tier: SpringTier,
}

impl SpringValue {
    pub fn new(initial: f32, tier: SpringTier) -> Self {
        let params = tier.params();
        Self {
            inner: Spring::new(initial, params.stiffness, params.damping, params.mass),
            tier,
        }
    }

    pub fn tier(&self) -> SpringTier {
        self.tier
    }

    pub fn value(&self) -> f32 {
        self.inner.value()
    }

    pub fn target(&self) -> f32 {
        self.inner.target()
    }

    pub fn velocity(&self) -> f32 {
        self.inner.velocity_for_callers()
    }

    pub fn set_tier(&mut self, tier: SpringTier) {
        let params = tier.params();
        self.tier = tier;
        self.inner.stiffness = params.stiffness;
        self.inner.damping = params.damping;
        self.inner.mass = params.mass;
    }

    pub fn set_target(&mut self, target: f32) {
        self.inner.set_target(target);
    }

    pub fn snap(&mut self, value: f32) {
        self.inner.snap(value);
    }

    pub fn settled(&self) -> bool {
        self.inner.is_settled()
    }

    /// Integrates `dt` seconds. Returns whether the spring is still moving.
    pub fn step(&mut self, dt: f32) -> bool {
        self.inner.step(dt);
        !self.inner.is_settled()
    }
}

/// Keeps a view on the animation clock while any spring is moving.
#[derive(Clone, Debug)]
pub struct Animator {
    last_frame: Option<Instant>,
}

impl Default for Animator {
    fn default() -> Self {
        Self::new()
    }
}

impl Animator {
    pub fn new() -> Self {
        Self { last_frame: None }
    }

    /// Seconds since the previous active frame, or 0 on the first.
    /// Requests another frame while `active` is true and clears the clock otherwise.
    pub fn dt(&mut self, window: &mut Window, active: bool) -> f32 {
        if !active {
            self.last_frame = None;
            return 0.0;
        }
        let now = Instant::now();
        let dt = self
            .last_frame
            .map(|prev| (now - prev).as_secs_f32())
            .unwrap_or(0.0);
        self.last_frame = Some(now);
        window.request_animation_frame();
        dt
    }
}

#[cfg(test)]
mod tier_tests {
    use super::{SpringTier, SpringValue};

    #[test]
    fn tiers_match_shared_duration_and_bounce() {
        let fast = SpringTier::Fast.params();
        assert!((fast.duration - 0.08).abs() < 1e-6);
        assert_eq!(fast.bounce, 0.0);
        assert!((fast.exit_duration - 0.06).abs() < 1e-6);
        let fast_ratio = fast.damping / (2.0 * (fast.mass * fast.stiffness).sqrt());
        assert!((fast_ratio - 1.0).abs() < 1e-3);

        let moderate = SpringTier::Moderate.params();
        assert!((moderate.duration - 0.16).abs() < 1e-6);
        assert_eq!(moderate.bounce, 0.0);
        assert!((moderate.exit_duration - 0.12).abs() < 1e-6);

        let slow = SpringTier::Slow.params();
        assert!((slow.duration - 0.24).abs() < 1e-6);
        assert!((slow.bounce - 0.12).abs() < 1e-6);
        assert!((slow.exit_duration - 0.16).abs() < 1e-6);
        let slow_ratio = slow.damping / (2.0 * (slow.mass * slow.stiffness).sqrt());
        assert!((slow_ratio - 0.88).abs() < 1e-3);
        assert!(slow.stiffness < moderate.stiffness);
        assert!(moderate.stiffness < fast.stiffness);
    }

    #[test]
    fn spring_value_reaches_target_and_reports_motion() {
        let mut value = SpringValue::new(0.0, SpringTier::Moderate);
        value.set_target(1.0);
        assert!(value.step(1.0 / 60.0) || !value.settled());
        let mut moved = false;
        for _ in 0..180 {
            if value.step(1.0 / 60.0) {
                moved = true;
            }
        }
        assert!(moved);
        assert!(!value.step(1.0 / 60.0));
        assert!(value.settled());
        assert!((value.value() - 1.0).abs() < 1e-4);

        value.snap(4.0);
        assert_eq!(value.value(), 4.0);
        assert!(value.settled());
    }

    #[test]
    fn slow_tier_overshoots_more_than_moderate() {
        let mut moderate = SpringValue::new(0.0, SpringTier::Moderate);
        let mut slow = SpringValue::new(0.0, SpringTier::Slow);
        moderate.set_target(100.0);
        slow.set_target(100.0);
        let mut moderate_peak = 0.0_f32;
        let mut slow_peak = 0.0_f32;
        for _ in 0..240 {
            moderate.step(1.0 / 120.0);
            slow.step(1.0 / 120.0);
            moderate_peak = moderate_peak.max(moderate.value());
            slow_peak = slow_peak.max(slow.value());
        }
        assert!(moderate.settled() && slow.settled());
        assert!(slow_peak + 0.05 >= moderate_peak);
        assert!(
            slow_peak > 100.0,
            "slow bounce 0.12 should overshoot, peak={slow_peak}"
        );
    }

    #[test]
    fn spring_tiers_follow_the_motion_curves() {
        // `spring({ keyframes: [0, 1], duration, bounce }).next(p * duration)` from `motion`.
        let cases = [
            (
                super::SpringTier::Fast,
                [0.23607, 0.67107, 0.94448, 0.99221],
            ),
            (
                super::SpringTier::Moderate,
                [0.23607, 0.67107, 0.94448, 0.99221],
            ),
            (
                super::SpringTier::Slow,
                [0.22222, 0.67968, 0.97174, 1.00290],
            ),
        ];
        for (tier, expected) in cases {
            let curve = tier.curve();
            for (p, want) in [0.1, 0.25, 0.5, 0.75].into_iter().zip(expected) {
                let got = curve.at(p);
                assert!(
                    (got - want).abs() < 2e-3,
                    "{tier:?} at {p}: {got} != {want}"
                );
            }
            assert_eq!(curve.at(1.0), 1.0);
        }
    }

    #[test]
    fn tweens_retarget_from_where_they_are() {
        let mut tween = super::Tween::new(0.0);
        assert!(!tween.is_animating());
        tween.animate_to(1.0, 10.0, super::SpringTier::Fast.curve());
        assert!(tween.is_animating());
        assert!(tween.value() < 0.5);
        tween.snap(1.0);
        assert_eq!(tween.value(), 1.0);
    }
}
