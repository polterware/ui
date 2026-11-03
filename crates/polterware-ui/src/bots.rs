//! Animated agent bots and the mascot, drawn by `bot-avatar` (feature `bots`).
//!
//! Views call [`agent_avatar`] or [`agent_avatar_keyed`], and [`mascot`] for `EmptyBot`. Each
//! avatar keeps one
//! `bot_avatar::Avatar` in GPUI element state. A shared cache renders each
//! (agent, status, size, scale) once per frame and reuses the [`RenderImage`].
//!
//! The raster seam ([`premultiplied_rgba_to_bgra`], [`render_image_from_rgba_premultiplied`],
//! [`paint_rgba_premultiplied`]) turns a premultiplied RGBA pixmap into GPUI's BGRA image.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

use bot_avatar::types::{BotAvatarState, BotAvatarType};
use bot_avatar::{Avatar, MascotMood, kind_for_agent, state_for_status};
use gpui::prelude::*;
use gpui::{
    App, Bounds, Corners, ElementId, EntityId, GlobalElementId, ImageSource, Img,
    InspectorElementId, LayoutId, Pixels, RenderImage, SharedString, Style, Window, div, img,
    point, px, size,
};
use image::{Frame, RgbaImage};
use smallvec::SmallVec;

/// Pixmap samples per second for one avatar. The sim itself steps at 120Hz.
const FRAME_SECONDS: f64 = 1.0 / 30.0;

/// The agent's bot, as `DispatchAgentAvatar` draws it: the bot alone, with no status mark
/// (the row's status dot says it).
pub fn agent_avatar(agent: &str, status: &str, size_px: f32) -> impl IntoElement {
    let size_px = finite_px(size_px);
    agent_avatar_keyed(derived_key(agent, status, size_px), agent, status, size_px)
}

/// Same avatar as [`agent_avatar`], with a caller key so the `Avatar` survives
/// status changes. The element id is the key plus the agent.
pub fn agent_avatar_keyed(
    key: impl Into<SharedString>,
    agent: &str,
    status: &str,
    size_px: f32,
) -> impl IntoElement {
    let size_px = finite_px(size_px);
    let id = ElementId::from((
        ElementId::from(key.into()),
        SharedString::from(agent.to_string()),
    ));
    div()
        .relative()
        .size(px(size_px))
        .flex_none()
        .child(AgentAvatarElement {
            id,
            agent: agent.to_string(),
            status: status.to_string(),
            mood: None,
            size_px,
        })
}

/// `EmptyBot`: the Polterware mascot (`PolterMascot`, the ghost) in `mood`, as `PageEmpty`
/// leads with it.
pub fn mascot(mood: MascotMood, size_px: f32) -> impl IntoElement {
    let size_px = finite_px(size_px);
    div()
        .relative()
        .size(px(size_px))
        .flex_none()
        .child(AgentAvatarElement {
            id: ElementId::from(SharedString::from("polter-mascot")),
            agent: format!("\u{0}mascot-{mood:?}"),
            status: String::new(),
            mood: Some(mood),
            size_px,
        })
}

/// `EmptyBot` sleeps when asleep and is awake otherwise.
fn mascot_state(mood: MascotMood) -> BotAvatarState {
    match mood {
        MascotMood::Sleeping => BotAvatarState::Sleeping,
        MascotMood::Sad | MascotMood::Lost => BotAvatarState::Default,
    }
}

/// The agent's bot, or the mascot when a mood is set.
fn build_avatar(
    agent: &str,
    mood: Option<MascotMood>,
    state: BotAvatarState,
    size_px: u32,
    scale: f32,
) -> Avatar {
    match mood {
        Some(mood) => Avatar::mascot(mood, size_px, scale),
        None => Avatar::for_agent(agent, state.as_str(), size_px, scale),
    }
}

fn derived_key(agent: &str, status: &str, size_px: f32) -> String {
    format!("{agent}\u{1}{status}\u{1}{size_px}")
}

struct AgentAvatarElement {
    id: ElementId,
    agent: String,
    status: String,
    /// The mascot's mood; `agent` and `status` then only key its frames.
    mood: Option<MascotMood>,
    size_px: f32,
}

impl IntoElement for AgentAvatarElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

/// Working hops, default idles (blink and gaze), sleeping breathes.
fn state_animates(state: BotAvatarState) -> bool {
    match state {
        BotAvatarState::Working | BotAvatarState::Default | BotAvatarState::Sleeping => true,
    }
}

struct HeldAvatar {
    avatar: Avatar,
    mood: Option<MascotMood>,
    kind: BotAvatarType,
    state: BotAvatarState,
    size_px: u32,
    scale_bits: u32,
    /// `animation_now` when this avatar last asked for another frame.
    last_request: Option<f64>,
    /// When a 30fps wake is already scheduled, so paints do not stack timers.
    wake_at: Option<f64>,
}

#[derive(PartialEq, Eq)]
enum AvatarSync {
    Unchanged,
    State,
    Rebuilt,
}

impl HeldAvatar {
    fn new(
        agent: &str,
        mood: Option<MascotMood>,
        kind: BotAvatarType,
        state: BotAvatarState,
        size_px: u32,
        scale: f32,
    ) -> Self {
        Self {
            avatar: build_avatar(agent, mood, state, size_px, scale),
            mood,
            kind,
            state,
            size_px,
            scale_bits: scale.to_bits(),
            last_request: None,
            wake_at: None,
        }
    }
}

/// Status changes call `set_state`. Size, scale, kind, or mood allocate a new `Avatar`.
fn sync_avatar(
    held: &mut HeldAvatar,
    agent: &str,
    mood: Option<MascotMood>,
    kind: BotAvatarType,
    state: BotAvatarState,
    size_px: u32,
    scale: f32,
) -> AvatarSync {
    let scale_bits = scale.to_bits();
    if held.kind != kind
        || held.mood != mood
        || held.size_px != size_px
        || held.scale_bits != scale_bits
    {
        held.avatar = build_avatar(agent, mood, state, size_px, scale);
        held.mood = mood;
        held.kind = kind;
        held.state = state;
        held.size_px = size_px;
        held.scale_bits = scale_bits;
        AvatarSync::Rebuilt
    } else if held.state != state {
        held.avatar.set_state(state);
        held.state = state;
        AvatarSync::State
    } else {
        AvatarSync::Unchanged
    }
}

impl Element for AgentAvatarElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = px(self.size_px).into();
        style.size.height = px(self.size_px).into();
        style.flex_grow = 0.0;
        style.flex_shrink = 0.0;
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        // Parent lists omit off-screen rows, and a clip skips `paint`. Ticking
        // only here stops the avatar when it is not painted.
        let Some(id) = id else {
            return;
        };
        let (kind, state) = match self.mood {
            Some(mood) => (BotAvatarType::Ghost, mascot_state(mood)),
            None => (kind_for_agent(&self.agent), state_for_status(&self.status)),
        };
        let mood = self.mood;
        let raster = raster_px(self.size_px);
        let scale = finite_scale(window.scale_factor());
        let picture = canvas_bounds(bounds, self.size_px);
        let on_screen = bounds_are_visible(picture, window.content_mask().bounds);
        let active = window.is_window_active();
        let animates = state_animates(state);
        let now = animation_now();
        let bucket = sample_bucket(now);
        let sample = sample_seconds(bucket);
        let key = frame_key(&self.agent, state, raster, scale);
        let mut arm: Option<(EntityId, Duration)> = None;

        window.with_element_state(id, |slot, window| {
            let mut held = slot
                .unwrap_or_else(|| HeldAvatar::new(&self.agent, mood, kind, state, raster, scale));
            let sync = sync_avatar(&mut held, &self.agent, mood, kind, state, raster, scale);
            if on_screen
                && let Some(frame) = load_frame(
                    &mut held.avatar,
                    &key,
                    bucket,
                    sample,
                    sync != AvatarSync::Unchanged
                        || (active
                            && animates
                            && seconds_until_slot(held.last_request, now) == 0.0),
                )
            {
                if let Some(retired) = frame.retired {
                    let _ = window.drop_image(retired);
                }
                let _ = window.paint_image(picture, Corners::all(px(0.0)), frame.image, 0, false);
            }

            if on_screen && active && animates {
                let remain = seconds_until_slot(held.last_request, now);
                if remain == 0.0 {
                    held.last_request = Some(now);
                    held.wake_at = None;
                    window.request_animation_frame();
                } else if held.wake_at.is_none() {
                    held.wake_at = Some(now + remain);
                    arm = Some((window.current_view(), Duration::from_secs_f64(remain)));
                }
            } else {
                held.wake_at = None;
            }
            ((), held)
        });

        if let Some((view, wait)) = arm {
            window
                .spawn(cx, async move |cx| {
                    cx.background_executor().timer(wait).await;
                    let _ = cx.update(|_, app| app.notify(view));
                })
                .detach();
        }
    }
}

/// Layout box stays `size_px`. The pixmap is `OVERSCAN` larger and the body
/// sits `RISE` below the canvas centre, matching `BotAvatar`'s negative margins.
fn canvas_bounds(slot: Bounds<Pixels>, size_px: f32) -> Bounds<Pixels> {
    let overscan = bot_avatar::draw::OVERSCAN as f32;
    let rise = bot_avatar::draw::RISE as f32;
    let side = (overscan - 1.0) * 0.5;
    Bounds {
        origin: point(
            slot.origin.x - px(size_px * side),
            slot.origin.y - px(size_px * (side + rise)),
        ),
        size: size(px(size_px * overscan), px(size_px * overscan)),
    }
}

fn bounds_are_visible(bounds: Bounds<Pixels>, mask: Bounds<Pixels>) -> bool {
    bounds.size.width > px(0.0) && bounds.size.height > px(0.0) && bounds.intersects(&mask)
}

fn animation_now() -> f64 {
    static ORIGIN: OnceLock<Instant> = OnceLock::new();
    ORIGIN.get_or_init(Instant::now).elapsed().as_secs_f64()
}

fn sample_bucket(now_seconds: f64) -> u64 {
    if !now_seconds.is_finite() || now_seconds <= 0.0 {
        0
    } else {
        (now_seconds / FRAME_SECONDS).floor() as u64
    }
}

fn sample_seconds(bucket: u64) -> f64 {
    bucket as f64 * FRAME_SECONDS
}

/// `0` when a new sample is due. Otherwise the wait until the next 30fps slot.
fn seconds_until_slot(last_request: Option<f64>, now: f64) -> f64 {
    let Some(last) = last_request else {
        return 0.0;
    };
    let elapsed = now - last;
    if !elapsed.is_finite() || elapsed >= FRAME_SECONDS {
        0.0
    } else {
        FRAME_SECONDS - elapsed
    }
}

fn finite_px(size_px: f32) -> f32 {
    if size_px.is_finite() && size_px > 0.0 {
        size_px
    } else {
        1.0
    }
}

fn raster_px(size_px: f32) -> u32 {
    let n = finite_px(size_px).round();
    if n <= u32::MAX as f32 {
        n as u32
    } else {
        u32::MAX
    }
}

fn finite_scale(scale: f32) -> f32 {
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

fn cache_agent(agent: &str) -> String {
    let trimmed = agent.trim();
    if trimmed.is_empty() {
        "agent".to_string()
    } else {
        trimmed.to_string()
    }
}

#[derive(Clone, Eq, PartialEq, Hash)]
struct FrameKey {
    agent: String,
    state: BotAvatarState,
    size_px: u32,
    scale_bits: u32,
}

fn frame_key(agent: &str, state: BotAvatarState, size_px: u32, scale: f32) -> FrameKey {
    FrameKey {
        agent: cache_agent(agent),
        state,
        size_px,
        scale_bits: scale.to_bits(),
    }
}

struct CacheEntry {
    bucket: u64,
    image: Arc<RenderImage>,
    /// Previous frame's image. Dropped one replacement later so the frame
    /// that just painted it is already presented.
    previous: Option<Arc<RenderImage>>,
}

struct SharedFrame {
    image: Arc<RenderImage>,
    retired: Option<Arc<RenderImage>>,
}

fn frame_cache() -> &'static Mutex<HashMap<FrameKey, CacheEntry>> {
    static CACHE: OnceLock<Mutex<HashMap<FrameKey, CacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// One render per key per bucket. `refresh` false reuses the last image.
fn load_frame(
    avatar: &mut Avatar,
    key: &FrameKey,
    bucket: u64,
    sample_seconds: f64,
    refresh: bool,
) -> Option<SharedFrame> {
    let mut guard = frame_cache().lock().unwrap_or_else(|err| err.into_inner());
    if let Some(entry) = guard.get(key)
        && (entry.bucket == bucket || !refresh)
    {
        return Some(SharedFrame {
            image: entry.image.clone(),
            retired: None,
        });
    }

    let pixmap = avatar.render(sample_seconds);
    let image = render_image_from_rgba_premultiplied(
        pixmap.width(),
        pixmap.height(),
        pixmap.data().to_vec(),
    )?;
    let old = guard.remove(key);
    let retired = old.as_ref().and_then(|entry| entry.previous.clone());
    let previous = old.map(|entry| entry.image);
    guard.insert(
        key.clone(),
        CacheEntry {
            bucket,
            image: image.clone(),
            previous,
        },
    );
    Some(SharedFrame { image, retired })
}

/// Converts a premultiplied RGBA8 buffer in place to straight BGRA8, matching `gpui::RenderImage`.
pub fn premultiplied_rgba_to_bgra(buf: &mut [u8]) {
    for px in buf.chunks_exact_mut(4) {
        let r_p = px[0];
        let g_p = px[1];
        let b_p = px[2];
        let a = px[3];

        if a == 0 {
            px[0] = 0;
            px[1] = 0;
            px[2] = 0;
        } else if a == 255 {
            px[0] = b_p;
            px[1] = g_p;
            px[2] = r_p;
        } else {
            let inv_a = 255.0 / (a as f32);
            px[0] = ((b_p as f32) * inv_a).round().min(255.0) as u8;
            px[1] = ((g_p as f32) * inv_a).round().min(255.0) as u8;
            px[2] = ((r_p as f32) * inv_a).round().min(255.0) as u8;
        }
    }
}

/// Builds a GPUI [`RenderImage`] from a premultiplied RGBA8 frame buffer `(width, height, Vec<u8>)`.
pub fn render_image_from_rgba_premultiplied(
    width: u32,
    height: u32,
    mut rgba_premul: Vec<u8>,
) -> Option<Arc<RenderImage>> {
    if width == 0 || height == 0 {
        return None;
    }
    let expected = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)?;
    if rgba_premul.len() != expected {
        return None;
    }

    premultiplied_rgba_to_bgra(&mut rgba_premul);
    let image_buffer = RgbaImage::from_raw(width, height, rgba_premul)?;
    let frames = SmallVec::from_buf([Frame::new(image_buffer)]);
    Some(Arc::new(RenderImage::new(frames)))
}

/// Seam function: accepts a premultiplied RGBA8 frame buffer `(width, height, Vec<u8>)`
/// (such as output from a `tiny-skia` pixmap) and returns a GPUI [`Img`] element.
#[allow(dead_code)]
pub fn paint_rgba_premultiplied(width: u32, height: u32, rgba_premul: Vec<u8>) -> Img {
    let render_image = render_image_from_rgba_premultiplied(width, height, rgba_premul)
        .unwrap_or_else(empty_fallback_image);
    img(ImageSource::Render(render_image))
}

fn empty_fallback_image() -> Arc<RenderImage> {
    static EMPTY: OnceLock<Arc<RenderImage>> = OnceLock::new();
    EMPTY
        .get_or_init(|| {
            let buffer = RgbaImage::from_raw(1, 1, vec![0, 0, 0, 0]).expect("1x1 buffer");
            Arc::new(RenderImage::new(SmallVec::from_buf([Frame::new(buffer)])))
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::{
        AvatarSync, HeldAvatar, bounds_are_visible, load_frame, premultiplied_rgba_to_bgra,
        raster_px, sample_bucket, sample_seconds, seconds_until_slot, state_animates, sync_avatar,
    };
    use crate::bots::{FRAME_SECONDS, frame_key};
    use bot_avatar::{kind_for_agent, state_for_status};
    use gpui::{Bounds, point, px, size};
    use std::sync::Arc;

    #[test]
    fn premultiplied_rgba_unpremultiplies_to_straight_bgra() {
        // 50% alpha pixel of RGB (200, 100, 50) -> premultiplied (100, 50, 25, 128).
        let mut buf = vec![100, 50, 25, 128, 0, 0, 0, 0];
        premultiplied_rgba_to_bgra(&mut buf);

        assert!((buf[0] as i32 - 50).abs() <= 1); // B
        assert!((buf[1] as i32 - 100).abs() <= 1); // G
        assert!((buf[2] as i32 - 199).abs() <= 2); // R
        assert_eq!(buf[3], 128);
        assert_eq!(&buf[4..8], &[0, 0, 0, 0]);
    }

    #[test]
    fn every_dispatch_state_animates() {
        assert!(state_animates(state_for_status("running")));
        assert!(state_animates(state_for_status("idle")));
        assert!(state_animates(state_for_status("failed")));
    }

    #[test]
    fn frame_slot_is_thirty_hertz() {
        assert_eq!(seconds_until_slot(None, 1.0), 0.0);
        assert!((seconds_until_slot(Some(0.0), 0.0) - FRAME_SECONDS).abs() < 1e-9);
        assert_eq!(seconds_until_slot(Some(0.0), FRAME_SECONDS), 0.0);
        assert!(seconds_until_slot(Some(0.0), FRAME_SECONDS * 0.5) > 0.0);
        assert_eq!(sample_bucket(0.0), 0);
        assert_eq!(sample_bucket(FRAME_SECONDS - 0.000_1), 0);
        assert_eq!(sample_bucket(FRAME_SECONDS), 1);
        assert!((sample_seconds(2) - 2.0 * FRAME_SECONDS).abs() < 1e-9);
    }

    #[test]
    fn off_screen_bounds_are_not_visible() {
        let mask = Bounds::new(point(px(0.0), px(0.0)), size(px(100.0), px(100.0)));
        let on = Bounds::new(point(px(10.0), px(10.0)), size(px(18.0), px(18.0)));
        let off = Bounds::new(point(px(200.0), px(0.0)), size(px(18.0), px(18.0)));
        let empty = Bounds::new(point(px(10.0), px(10.0)), size(px(0.0), px(18.0)));
        assert!(bounds_are_visible(on, mask));
        assert!(!bounds_are_visible(off, mask));
        assert!(!bounds_are_visible(empty, mask));
    }

    #[test]
    fn status_change_keeps_the_avatar_and_size_change_rebuilds() {
        let kind = kind_for_agent("claude");
        let running = state_for_status("running");
        let failed = state_for_status("failed");
        let mut held = HeldAvatar::new("claude", None, kind, running, 32, 2.0);
        assert!(matches!(
            sync_avatar(&mut held, "claude", None, kind, failed, 32, 2.0),
            AvatarSync::State
        ));
        assert_eq!(held.state, failed);
        assert!(matches!(
            sync_avatar(&mut held, "claude", None, kind, failed, 32, 2.0),
            AvatarSync::Unchanged
        ));
        assert!(matches!(
            sync_avatar(&mut held, "claude", None, kind, failed, 48, 2.0),
            AvatarSync::Rebuilt
        ));
        assert_eq!(held.size_px, 48);
        assert_eq!(raster_px(18.0), 18);
    }

    #[test]
    fn one_bucket_renders_once_and_reuses_the_image() {
        let kind = kind_for_agent("claude");
        let state = state_for_status("running");
        let mut first = bot_avatar::Avatar::new(kind, state, 32, 1.0);
        let mut second = bot_avatar::Avatar::new(kind, state, 32, 1.0);
        let key = frame_key("claude", state, 32, 1.0);
        let bucket = 4;
        let sample = sample_seconds(bucket);
        let a = load_frame(&mut first, &key, bucket, sample, true).expect("first frame");
        let b = load_frame(&mut second, &key, bucket, sample, true).expect("shared frame");
        assert!(Arc::ptr_eq(&a.image, &b.image));
        assert_eq!(a.image.size(0).width.0, 48);
        assert_eq!(a.image.frame_count(), 1);
        let stale = load_frame(
            &mut second,
            &key,
            bucket + 1,
            sample_seconds(bucket + 1),
            false,
        )
        .expect("frozen frame");
        assert!(Arc::ptr_eq(&a.image, &stale.image));
    }
}
