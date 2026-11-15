//! `VoiceBlob`: the living mark of a voice conversation. An organic blob, drawn as a path on a
//! canvas, whose size, wobble, and glow follow an audio level read each frame: the person's
//! microphone while they speak, the reply's playback while it is read aloud. Silent, it
//! breathes slowly, so a conversation that is waiting still looks alive.
//!
//! The level comes in by closure (`0..=1`, linear, as an engine's peak or RMS meter reports
//! it). Speech rarely peaks above a third of full scale, so the blob applies a gain and a
//! gentle curve before it reacts, then eases toward the result: a quick attack, a slower
//! release, so it swells with a syllable and settles between words instead of flickering.
//!
//! It asks for a frame each paint while it is drawn, and none once it is gone from the page.
//! `still` draws a plain disc that grows with the level, without wobble (reduced motion).

use std::cell::Cell;
use std::f32::consts::TAU;
use std::rc::Rc;
use std::time::Instant;

use gpui::{
    App, Bounds, ElementId, IntoElement, PathBuilder, Pixels, RenderOnce, Rgba, Window, canvas,
    div, linear_color_stop, linear_gradient, point, prelude::*, px,
};

use crate::theme::{Theme, mix};

/// How many points trace the blob's outline.
const OUTLINE_POINTS: usize = 72;
/// Seconds to rise most of the way to a louder level.
const ATTACK: f32 = 0.05;
/// Seconds to fall most of the way back toward a quieter one.
const RELEASE: f32 = 0.28;

/// The blob's eased energy and the phase of its wobble, kept across frames.
struct Motion {
    energy: Cell<f32>,
    phase: Cell<f32>,
    last: Cell<Option<Instant>>,
}

struct BlobState(Rc<Motion>);

/// A raw level shaped for the eye: gain, then a curve that lifts quiet speech.
pub fn shape_level(raw: f32, gain: f32) -> f32 {
    (raw.max(0.0) * gain).min(1.0).powf(0.65)
}

/// One easing step from `energy` toward `target` over `dt` seconds.
pub fn ease_energy(energy: f32, target: f32, dt: f32) -> f32 {
    let tau = if target > energy { ATTACK } else { RELEASE };
    let k = 1.0 - (-dt.max(0.0) / tau).exp();
    (energy + (target - energy) * k).clamp(0.0, 1.0)
}

/// The outline's radius at angle `theta`, as a share of half the blob's box, for wobble phase
/// `phase` and energy `energy` (0..=1). Always within `0.3..=1.0`.
pub fn blob_radius(theta: f32, phase: f32, energy: f32) -> f32 {
    let e = energy.clamp(0.0, 1.0);
    let breath = 0.03 * (phase * 0.9).sin();
    let base = 0.44 + 0.26 * e + breath;
    let wobble = 0.5 * (3.0 * theta + phase * 1.3).sin()
        + 0.3 * (5.0 * theta - phase * 1.7 + 1.1).sin()
        + 0.2 * (2.0 * theta + phase * 0.7 + 2.3).sin();
    let amplitude = 0.05 + 0.12 * e;
    (base * (1.0 + amplitude * wobble)).clamp(0.3, 1.0)
}

/// The voice blob: see the module's docs.
#[derive(IntoElement)]
pub struct VoiceBlob {
    id: ElementId,
    level: Rc<dyn Fn() -> f32>,
    size: f32,
    color: Option<Rgba>,
    gain: f32,
    still: bool,
}

impl VoiceBlob {
    /// A blob that reads `level` (0..=1) each frame.
    pub fn new(id: impl Into<ElementId>, level: impl Fn() -> f32 + 'static) -> Self {
        Self {
            id: id.into(),
            level: Rc::new(level),
            size: 120.0,
            color: None,
            gain: 2.6,
            still: false,
        }
    }

    /// The blob's box, in pixels (120 by default). The blob fills at most this box.
    pub fn size(mut self, side: f32) -> Self {
        self.size = side;
        self
    }

    /// The blob's ink (the theme's brand color by default).
    pub fn color(mut self, color: Rgba) -> Self {
        self.color = Some(color);
        self
    }

    /// How much the raw level is amplified before it shapes the blob (2.6 by default).
    pub fn gain(mut self, gain: f32) -> Self {
        self.gain = gain;
        self
    }

    /// A plain disc that follows the level without wobble, as with reduced motion.
    pub fn still(mut self, still: bool) -> Self {
        self.still = still;
        self
    }
}

impl RenderOnce for VoiceBlob {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let color = self.color.unwrap_or(theme.color_brand);
        let motion = window
            .use_keyed_state(self.id.clone(), cx, |_, _| {
                BlobState(Rc::new(Motion {
                    energy: Cell::new(0.0),
                    phase: Cell::new(0.0),
                    last: Cell::new(None),
                }))
            })
            .read(cx)
            .0
            .clone();
        let level = self.level;
        let gain = self.gain;
        let still = self.still;
        div().id(self.id).flex_none().size(px(self.size)).child(
            canvas(
                |_, _, _| {},
                move |bounds: Bounds<Pixels>, _, window: &mut Window, _: &mut App| {
                    let now = Instant::now();
                    let dt = motion
                        .last
                        .replace(Some(now))
                        .map_or(0.0, |last| (now - last).as_secs_f32().min(0.1));
                    let target = shape_level(level(), gain);
                    let energy = ease_energy(motion.energy.get(), target, dt);
                    motion.energy.set(energy);
                    let phase = motion.phase.get() + dt * (0.8 + 2.4 * energy);
                    motion.phase.set(phase % (TAU * 100.0));
                    paint(bounds, color, energy, phase, still, window);
                    window.request_animation_frame();
                },
            )
            .size_full(),
        )
    }
}

fn paint(
    bounds: Bounds<Pixels>,
    color: Rgba,
    energy: f32,
    phase: f32,
    still: bool,
    window: &mut Window,
) {
    let half = f32::from(bounds.size.width.min(bounds.size.height)) / 2.0;
    let center = bounds.center();
    let outline = |scale: f32, phase: f32| {
        let mut path = PathBuilder::fill();
        for index in 0..OUTLINE_POINTS {
            let theta = index as f32 / OUTLINE_POINTS as f32 * TAU;
            let radius = if still {
                0.44 + 0.3 * energy
            } else {
                blob_radius(theta, phase, energy)
            };
            let r = half * (radius * scale).min(1.0);
            let at = point(
                center.x + px(r * theta.cos()),
                center.y + px(r * theta.sin()),
            );
            if index == 0 {
                path.move_to(at);
            } else {
                path.line_to(at);
            }
        }
        path.close();
        path.build().ok()
    };
    let with_alpha = |mut ink: Rgba, alpha: f32| {
        ink.a = alpha.clamp(0.0, 1.0);
        ink
    };
    // Two soft halos behind, a step behind the core's wobble, brighter as the voice rises.
    if let Some(halo) = outline(1.0 + 0.32 * energy, phase - 0.6) {
        window.paint_path(halo, with_alpha(color, 0.07 + 0.12 * energy));
    }
    if let Some(halo) = outline(1.0 + 0.16 * energy, phase - 0.3) {
        window.paint_path(halo, with_alpha(color, 0.12 + 0.18 * energy));
    }
    if let Some(core) = outline(1.0, phase) {
        let light = mix(color, gpui::rgba(0xffffffff), 0.35);
        window.paint_path(
            core,
            linear_gradient(
                135.0 + 40.0 * (phase * 0.4).sin(),
                linear_color_stop(with_alpha(light, 0.95), 0.0),
                linear_color_stop(with_alpha(color, 0.95), 1.0),
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_speech_still_moves_the_blob() {
        // A peak of 0.1 (ordinary speech at a laptop mic) lands well above a third.
        assert!(shape_level(0.1, 2.6) > 0.4);
        assert_eq!(shape_level(0.0, 2.6), 0.0);
        assert_eq!(shape_level(1.0, 2.6), 1.0);
    }

    #[test]
    fn energy_rises_fast_and_falls_slow() {
        let up = ease_energy(0.0, 1.0, 1.0 / 60.0);
        let down = 1.0 - ease_energy(1.0, 0.0, 1.0 / 60.0);
        assert!(up > down * 3.0, "attack {up} release {down}");
    }

    #[test]
    fn outline_stays_inside_its_box_and_grows_with_energy() {
        let mean = |energy: f32| {
            (0..360)
                .map(|step| {
                    let r = blob_radius(step as f32 / 360.0 * TAU, 2.0, energy);
                    assert!((0.3..=1.0).contains(&r));
                    r
                })
                .sum::<f32>()
                / 360.0
        };
        assert!(mean(1.0) > mean(0.0) + 0.2);
    }
}
