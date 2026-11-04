//! Dark palette, materials, radii, spacing, type metrics, and motion timing.
//!
//! Every value mirrors a CSS variable of `packages/configuration/tailwind/base.css`
//! (`--color-*`, `--material-*`, `--tone-*`, `--field-*`, `--radius-*`, `--duration-*`,
//! `--ease-*`) or of `materials.css` / `button.css` (`--tx-*`, the texture faces), so a
//! view never writes a raw color, radius, or duration.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};

use gpui::{App, Global, Rgba, rgb, rgba};

/// A texture tone: the frame, the lit rim, the gradient face, and the ink
/// (`--tone-<name>-frame|rim|top|bottom|ink`).
#[derive(Clone, Copy, Debug)]
pub struct ToneSet {
    pub frame: Rgba,
    pub rim: Rgba,
    pub top: Rgba,
    pub bottom: Rgba,
    pub ink: Rgba,
}

/// A compact status badge pair (`--color-status-<name>-background|foreground`).
#[derive(Clone, Copy, Debug)]
pub struct StatusPair {
    pub background: Rgba,
    pub foreground: Rgba,
}

/// A texture button face per state (`--tx-face-*` and `--tx-ink`).
#[derive(Clone, Copy, Debug)]
pub struct FaceSet {
    pub top: Rgba,
    pub bottom: Rgba,
    pub hover_top: Rgba,
    pub hover_bottom: Rgba,
    pub active_top: Rgba,
    pub active_bottom: Rgba,
    pub ink: Rgba,
}

/// A cubic-bezier easing, as `cubic-bezier(x1, y1, x2, y2)`.
#[derive(Clone, Copy, Debug)]
pub struct Ease(pub f32, pub f32, pub f32, pub f32);

impl Ease {
    /// The eased progress at `t` (0..=1), solved for x by Newton steps with a bisection fallback.
    pub fn at(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        let Ease(x1, y1, x2, y2) = self;
        let bez = |a: f32, b: f32, s: f32| {
            let u = 1.0 - s;
            3.0 * u * u * s * a + 3.0 * u * s * s * b + s * s * s
        };
        let mut s = t;
        for _ in 0..8 {
            let x = bez(x1, x2, s) - t;
            let u = 1.0 - s;
            let dx = 3.0 * u * u * x1 + 6.0 * u * s * (x2 - x1) + 3.0 * s * s * (1.0 - x2);
            if dx.abs() < 1e-5 {
                break;
            }
            s = (s - x / dx).clamp(0.0, 1.0);
        }
        if (bez(x1, x2, s) - t).abs() > 1e-3 {
            let (mut lo, mut hi) = (0.0f32, 1.0f32);
            for _ in 0..24 {
                s = (lo + hi) / 2.0;
                if bez(x1, x2, s) < t {
                    lo = s;
                } else {
                    hi = s;
                }
            }
        }
        bez(y1, y2, s)
    }
}

/// Installed once at startup. Views read it with [`Theme::global`].
#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub color_app_background: Rgba,
    pub color_shell_background: Rgba,
    pub color_sidebar_background: Rgba,
    /// `--color-shell-glass`: the wash over the vibrancy sidebar.
    pub color_shell_glass: Rgba,
    pub color_surface: Rgba,
    pub color_surface_subtle: Rgba,
    pub color_surface_raised: Rgba,
    pub color_surface_hover: Rgba,
    pub color_surface_active: Rgba,
    pub color_surface_selected: Rgba,
    pub color_border: Rgba,
    pub color_border_subtle: Rgba,
    pub color_border_default: Rgba,
    pub color_border_strong: Rgba,
    pub color_text_primary: Rgba,
    pub color_text_secondary: Rgba,
    pub color_text_muted: Rgba,
    pub color_text_disabled: Rgba,
    pub color_primary: Rgba,
    pub color_primary_foreground: Rgba,
    pub color_brand: Rgba,
    pub color_accent: Rgba,
    pub color_danger: Rgba,
    pub color_warning: Rgba,
    pub color_info: Rgba,
    pub color_success: Rgba,
    pub color_overlay: Rgba,
    pub color_shadow_subtle: Rgba,
    pub color_shadow: Rgba,
    pub color_shadow_float: Rgba,
    pub color_shadow_outline: Rgba,
    pub chart_cyan: Rgba,
    pub chart_blue: Rgba,
    pub chart_green: Rgba,
    pub chart_amber: Rgba,
    pub chart_red: Rgba,
    pub chart_violet: Rgba,
    pub chart_gray: Rgba,
    pub chart_grid: Rgba,
    pub heatmap: [Rgba; 5],
    pub icon_tile_accent_neutral: Rgba,
    pub icon_tile_base: Rgba,
    pub icon_tile_edge: Rgba,
    pub icon_tile_mark: Rgba,
    pub icon_tile_mark_dimmed: Rgba,
    pub status_success: StatusPair,
    pub status_warning: StatusPair,
    pub status_danger: StatusPair,
    pub status_info: StatusPair,
    pub status_muted: StatusPair,
    /// `--material-rim` (white .16): the quiet top edge.
    pub material_rim: Rgba,
    /// `--material-rim-strong` (white .2): the control top edge.
    pub material_rim_strong: Rgba,
    /// `--material-highlight` (white .06): the quiet ring.
    pub material_highlight: Rgba,
    /// `--material-highlight-strong` (white .1): the control ring.
    pub material_highlight_strong: Rgba,
    pub material_drop: Rgba,
    pub material_drop_strong: Rgba,
    /// Chromium's classic dark scrollbar: the thumb at rest and under the pointer.
    pub scrollbar_thumb: Rgba,
    pub scrollbar_thumb_hover: Rgba,
    pub material_face_top: Rgba,
    pub material_face_bottom: Rgba,
    pub field_background: Rgba,
    pub field_background_hover: Rgba,
    pub field_fill_top: Rgba,
    pub field_fill_bottom: Rgba,
    pub field_fill_hover_top: Rgba,
    pub field_fill_hover_bottom: Rgba,
    pub tone_neutral: ToneSet,
    pub tone_brand: ToneSet,
    pub tone_success: ToneSet,
    pub tone_warn: ToneSet,
    pub tone_danger: ToneSet,
    pub tone_info: ToneSet,
    pub tone_neutral_bg: Rgba,
    pub tone_neutral_ink: Rgba,
    pub tone_success_bg: Rgba,
    pub tone_success_ink: Rgba,
    pub tone_warn_bg: Rgba,
    pub tone_warn_ink: Rgba,
    pub tone_danger_bg: Rgba,
    pub tone_danger_ink: Rgba,
    pub tone_info_bg: Rgba,
    pub tone_info_ink: Rgba,
    /// Texture button faces (`materials.css` neutral, `button.css` tones).
    pub face_neutral: FaceSet,
    pub face_primary: FaceSet,
    pub face_danger: FaceSet,
    pub face_disabled: FaceSet,
    /// `--radius-control` (8).
    pub radius_sm: f32,
    /// `--radius-item` (10).
    pub radius_md: f32,
    /// `--radius-menu` (16).
    pub radius_lg: f32,
    pub radius_control: f32,
    pub radius_item: f32,
    pub radius_card: f32,
    pub radius_menu: f32,
    pub radius_surface: f32,
    pub radius_window: f32,
    pub radius_dialog: f32,
    /// `.pw-field-control` and the md texture button (12).
    pub radius_field: f32,
    pub space_1: f32,
    pub space_2: f32,
    pub space_3: f32,
    pub space_4: f32,
    pub space_5: f32,
    pub space_6: f32,
    pub space_7: f32,
    pub space_8: f32,
    pub font_size_2xs: f32,
    pub font_size_xs: f32,
    pub font_size_sm: f32,
    pub font_size_md: f32,
    pub font_size_lg: f32,
    pub font_size_xl: f32,
    /// Tailwind preflight line-height (1.5 of the font size).
    pub line_height: f32,
    pub font_sans: &'static str,
    pub font_mono: &'static str,
    pub font_terminal: &'static str,
    /// Table rows and the page toolbar (`--page-header-height`).
    pub row_height: f32,
    /// `.pw-sidebar-item` min height (28).
    pub sidebar_row_height: f32,
    /// `.pw-sidebar-run` min height (52).
    pub sidebar_run_height: f32,
    pub page_header_height: f32,
    pub titlebar_height: f32,
    pub control_height_sm: f32,
    pub control_height_md: f32,
    pub field_height_sm: f32,
    pub field_height_md: f32,
    /// `--sidebar-width` (232).
    pub sidebar_width: f32,
    pub section_header_height: f32,
    pub section_gap: f32,
    pub sidebar_row_gap: f32,
    /// `.pw-main-frame` padding (8): the inset around the main surface.
    pub main_inset: f32,
    /// `--duration-fast` (120ms).
    pub duration_fast: f32,
    /// `--duration-control` (160ms).
    pub duration_control: f32,
    /// `--duration-enter` (200ms).
    pub duration_enter: f32,
    pub ease_standard: Ease,
    pub ease_enter: Ease,
}

impl Global for Theme {}

const fn tone(frame: u32, rim: u32, top: u32, bottom: u32, ink: u32) -> ToneSet {
    ToneSet {
        frame: hex(frame),
        rim: hex(rim),
        top: hex(top),
        bottom: hex(bottom),
        ink: hex(ink),
    }
}

const fn face(t: u32, b: u32, ht: u32, hb: u32, at: u32, ab: u32, ink: u32) -> FaceSet {
    FaceSet {
        top: hex(t),
        bottom: hex(b),
        hover_top: hex(ht),
        hover_bottom: hex(hb),
        active_top: hex(at),
        active_bottom: hex(ab),
        ink: hex(ink),
    }
}

/// An opaque `#rrggbb` color, usable in `const` items.
pub const fn hex(value: u32) -> Rgba {
    Rgba {
        r: ((value >> 16) & 0xff) as f32 / 255.0,
        g: ((value >> 8) & 0xff) as f32 / 255.0,
        b: (value & 0xff) as f32 / 255.0,
        a: 1.0,
    }
}

/// White or black at `alpha`, as the CSS `rgb(255 255 255 / a)` tokens write it.
pub const fn white(alpha: f32) -> Rgba {
    Rgba {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: alpha,
    }
}

pub const fn black(alpha: f32) -> Rgba {
    Rgba {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: alpha,
    }
}

/// The root font size before an app sets its own: the browser default.
pub const DEFAULT_ROOT_REM: f32 = 16.0;

static ROOT_REM_BITS: AtomicU32 = AtomicU32::new(DEFAULT_ROOT_REM.to_bits());

/// Sets the app's root font size, the CSS `html { font-size }` its Tailwind spacing
/// (`p-4`, `gap-1.5`) is measured in. Dispatch's is 13px. Call it once at startup, before
/// the first frame.
pub fn set_root_rem(px: f32) {
    if px.is_finite() && px > 0.0 {
        ROOT_REM_BITS.store(px.to_bits(), Ordering::Relaxed);
    }
}

/// The app's root font size, in pixels.
pub fn rem() -> f32 {
    f32::from_bits(ROOT_REM_BITS.load(Ordering::Relaxed))
}

/// A Tailwind spacing step (`0.25rem`) at the app's root size: at a 13px root,
/// `tw(4.0)` is `p-4`, 13px.
pub fn tw(steps: f32) -> f32 {
    steps * rem() / 4.0
}

/// An opaque color from 0–1 channels.
pub fn rgb_f32(r: f32, g: f32, b: f32) -> Rgba {
    Rgba { r, g, b, a: 1.0 }
}

/// `color-mix(in srgb, a (1-t), b t)`.
pub fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    Rgba {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a + (b.a - a.a) * t,
    }
}

/// `color-mix(in oklab, a share, b)`: both colors in Oklab, mixed, and back to sRGB.
pub fn mix_oklab(a: Rgba, b: Rgba, share: f32) -> Rgba {
    let (al, aa, ab) = to_oklab(a);
    let (bl, ba, bb) = to_oklab(b);
    let t = 1.0 - share;
    let (r, g, blue) = from_oklab(al + (bl - al) * t, aa + (ba - aa) * t, ab + (bb - ab) * t);
    Rgba {
        r,
        g,
        b: blue,
        a: a.a + (b.a - a.a) * t,
    }
}

fn to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

fn to_oklab(color: Rgba) -> (f32, f32, f32) {
    let (r, g, b) = (to_linear(color.r), to_linear(color.g), to_linear(color.b));
    let l = (0.412_221_47 * r + 0.536_332_55 * g + 0.051_445_99 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    (
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    )
}

fn from_oklab(lightness: f32, a: f32, b: f32) -> (f32, f32, f32) {
    let l = (lightness + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
    let m = (lightness - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
    let s = (lightness - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
    (
        to_srgb(4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s),
        to_srgb(-1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s),
        to_srgb(-0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s),
    )
}

/// `color` with its alpha multiplied by `alpha`.
pub fn fade(color: Rgba, alpha: f32) -> Rgba {
    Rgba {
        a: color.a * alpha,
        ..color
    }
}

impl Theme {
    pub fn global(cx: &App) -> &Self {
        cx.global::<Self>()
    }

    pub fn dark() -> Self {
        let tone_neutral = tone(0x141414, 0x4c4c4c, 0x262626, 0x3b3b3b, 0xa4a4a4);
        let tone_success = tone(0x14301d, 0x6ee7a0, 0x2f7a4a, 0x3a9a5c, 0xeffff4);
        let tone_warn = tone(0x5e3f0e, 0xfcd34d, 0xa8741a, 0xc28a22, 0xfff8e6);
        let tone_danger = tone(0x5e2323, 0xf87171, 0xa64545, 0xb83c3c, 0xfff1f1);
        let tone_info = tone(0x1c3560, 0x93c5fd, 0x2f5fa8, 0x3b74c4, 0xeef5ff);
        Self {
            color_app_background: rgb(0x161616),
            color_shell_background: rgb(0x161616),
            color_sidebar_background: rgb(0x161616),
            color_shell_glass: rgba(0x131313b3),
            color_surface: rgb(0x1b1b1b),
            color_surface_subtle: rgb(0x242424),
            color_surface_raised: rgb(0x292929),
            color_surface_hover: rgb(0x2c2c2c),
            color_surface_active: rgb(0x303030),
            color_surface_selected: rgb(0x343434),
            color_border: white(0.07),
            color_border_subtle: white(0.045),
            color_border_default: white(0.07),
            color_border_strong: white(0.12),
            color_text_primary: rgb(0xececec),
            color_text_secondary: rgb(0xa4a4a4),
            color_text_muted: rgb(0x8f8f8f),
            color_text_disabled: rgb(0x4b4b4b),
            color_primary: rgb(0xeef1f3),
            color_primary_foreground: rgb(0x343434),
            color_brand: rgb(0xeef1f3),
            color_accent: rgb(0x7e9fd8),
            color_danger: rgb(0xe46666),
            color_warning: rgb(0xd8a85d),
            color_info: rgb(0x7e9fd8),
            color_success: rgb(0x01dd96),
            color_overlay: black(0.6),
            color_shadow_subtle: black(0.22),
            color_shadow: black(0.32),
            color_shadow_float: black(0.42),
            color_shadow_outline: black(0.18),
            chart_cyan: rgb(0x8eccc9),
            chart_blue: rgb(0x7e9fd8),
            chart_green: rgb(0x5bbf93),
            chart_amber: rgb(0xd8a85d),
            chart_red: rgb(0xe46666),
            chart_violet: rgb(0x9c8fce),
            chart_gray: rgb(0x6f6f6f),
            chart_grid: white(0.07),
            heatmap: [
                rgb(0x1e1e1e),
                rgb(0x164b3a),
                rgb(0x167451),
                rgb(0x159f6d),
                rgb(0x01dd96),
            ],
            icon_tile_accent_neutral: rgb(0xa8a8a8),
            icon_tile_base: rgb(0xf4f4f4),
            icon_tile_edge: rgb(0xd0d0d0),
            icon_tile_mark: rgb(0x121212),
            icon_tile_mark_dimmed: rgb(0x3a3a3a),
            status_success: StatusPair {
                background: rgb(0x3d8f74),
                foreground: rgb(0x071c16),
            },
            status_warning: StatusPair {
                background: rgb(0xd8a85d),
                foreground: rgb(0x4a310f),
            },
            status_danger: StatusPair {
                background: rgb(0xe46666),
                foreground: rgb(0x47171a),
            },
            status_info: StatusPair {
                background: rgb(0x7e9fd8),
                foreground: rgb(0x26364f),
            },
            status_muted: StatusPair {
                background: rgb(0x343434),
                foreground: rgb(0xa4a4a4),
            },
            material_rim: white(0.16),
            material_rim_strong: white(0.2),
            material_highlight: white(0.06),
            material_highlight_strong: white(0.1),
            material_drop: black(0.3),
            material_drop_strong: black(0.4),
            scrollbar_thumb: rgb(0x373737),
            scrollbar_thumb_hover: rgb(0x8f8f8f),
            material_face_top: rgb(0x303030),
            material_face_bottom: rgb(0x2b2b2b),
            field_background: rgb(0x171717),
            field_background_hover: rgb(0x1b1b1b),
            field_fill_top: rgb(0x1a1a1a),
            field_fill_bottom: rgb(0x171717),
            field_fill_hover_top: rgb(0x1e1e1e),
            field_fill_hover_bottom: rgb(0x1b1b1b),
            tone_neutral,
            tone_brand: tone(0x1a1a1a, 0xffffff, 0xe5e5e5, 0xfafafa, 0x333333),
            tone_success,
            tone_warn,
            tone_danger,
            tone_info,
            tone_neutral_bg: tone_neutral.top,
            tone_neutral_ink: tone_neutral.ink,
            tone_success_bg: tone_success.top,
            tone_success_ink: tone_success.ink,
            tone_warn_bg: tone_warn.top,
            tone_warn_ink: tone_warn.ink,
            tone_danger_bg: tone_danger.top,
            tone_danger_ink: tone_danger.ink,
            tone_info_bg: tone_info.top,
            tone_info_ink: tone_info.ink,
            face_neutral: face(
                0x303030, 0x2b2b2b, 0x383838, 0x333333, 0x2b2b2b, 0x2b2b2b, 0xececec,
            ),
            // `color-mix(in oklab, …, white 10%|14%)` resolved in sRGB.
            face_primary: face(
                0x2f5fa8, 0x3b74c4, 0x436fb1, 0x4f82cb, 0x2f5fa8, 0x5788cd, 0xeef5ff,
            ),
            face_danger: face(
                0xa64545, 0xb83c3c, 0xb34e4e, 0xc64545, 0xc04848, 0xd04848, 0xfff1f1,
            ),
            face_disabled: face(
                0x262626, 0x262626, 0x262626, 0x262626, 0x262626, 0x262626, 0x4b4b4b,
            ),
            radius_sm: 8.0,
            radius_md: 10.0,
            radius_lg: 16.0,
            radius_control: 8.0,
            radius_item: 10.0,
            radius_card: 14.0,
            radius_menu: 16.0,
            // As round as the macOS window around it, about 16pt.
            radius_surface: 16.0,
            radius_window: 24.0,
            radius_dialog: 30.0,
            radius_field: 12.0,
            space_1: 4.0,
            space_2: 8.0,
            space_3: 12.0,
            space_4: 16.0,
            space_5: 20.0,
            space_6: 24.0,
            space_7: 28.0,
            space_8: 32.0,
            font_size_2xs: 10.0,
            font_size_xs: 11.0,
            font_size_sm: 12.0,
            font_size_md: 13.0,
            font_size_lg: 15.0,
            font_size_xl: 18.0,
            line_height: 1.5,
            font_sans: "Inter",
            font_mono: "Menlo",
            font_terminal: "JetBrains Mono",
            row_height: 44.0,
            sidebar_row_height: 28.0,
            sidebar_run_height: 52.0,
            page_header_height: 44.0,
            titlebar_height: 44.0,
            control_height_sm: 28.0,
            control_height_md: 32.0,
            field_height_sm: 32.0,
            field_height_md: 36.0,
            sidebar_width: 232.0,
            section_header_height: 28.0,
            section_gap: 6.0,
            sidebar_row_gap: 2.0,
            main_inset: 8.0,
            duration_fast: 0.12,
            duration_control: 0.16,
            duration_enter: 0.2,
            ease_standard: Ease(0.2, 0.0, 0.0, 1.0),
            ease_enter: Ease(0.16, 1.0, 0.3, 1.0),
        }
    }
}

/// The design system's status tones.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusTone {
    Healthy,
    Degraded,
    Down,
    Unknown,
    Syncing,
    Ok,
    Warn,
    Danger,
    Muted,
}

/// How an app maps its own status strings onto [`StatusTone`]. The app sets it once at startup
/// with [`set_status_tones`]; until then [`default_status_tone`] answers.
pub type StatusTones = fn(&str) -> StatusTone;

static STATUS_TONES: OnceLock<StatusTones> = OnceLock::new();

/// Sets the app's status mapping. Only the first call takes effect.
pub fn set_status_tones(tones: StatusTones) {
    let _ = STATUS_TONES.set(tones);
}

/// The tone of a status string, through the app's mapping.
pub fn status_tone(status: &str) -> StatusTone {
    STATUS_TONES.get().copied().unwrap_or(default_status_tone)(status)
}

/// The mapping an app gets before it sets its own: the plain words every app shares.
pub fn default_status_tone(status: &str) -> StatusTone {
    match status.trim().to_ascii_lowercase().as_str() {
        "ok" | "passed" | "completed" | "ready" | "healthy" | "live" => StatusTone::Ok,
        "running" | "started" | "starting" | "queued" | "pending" | "active" | "syncing"
        | "building" => StatusTone::Syncing,
        "failed" | "error" | "blocked" | "down" => StatusTone::Danger,
        "cancelled" | "canceled" | "warning" | "degraded" | "stopped" => StatusTone::Warn,
        _ => StatusTone::Muted,
    }
}

pub fn tone_color(theme: &Theme, tone: StatusTone) -> Rgba {
    match tone {
        StatusTone::Healthy | StatusTone::Ok => theme.color_brand,
        StatusTone::Degraded | StatusTone::Warn => theme.color_warning,
        StatusTone::Down | StatusTone::Danger => theme.color_danger,
        StatusTone::Syncing => theme.color_info,
        StatusTone::Unknown | StatusTone::Muted => theme.color_text_disabled,
    }
}

pub fn status_color(status: &str) -> Rgba {
    tone_color(&Theme::dark(), status_tone(status))
}

#[cfg(test)]
mod tests {
    use super::{
        Ease, StatusTone, Theme, default_status_tone, hex, mix_oklab, rem, set_root_rem, tw,
    };

    #[test]
    fn oklab_mix_matches_the_browser() {
        // `color-mix(in oklab, #8eccc9 65%, #303030)`, which Chromium paints rgb(108, 145, 143).
        let mixed = mix_oklab(hex(0x8eccc9), hex(0x303030), 0.65);
        let channel = |c: f32| (c * 255.0).round() as i32;
        assert!((channel(mixed.r) - 108).abs() <= 1, "{mixed:?}");
        assert!((channel(mixed.g) - 145).abs() <= 1, "{mixed:?}");
        assert!((channel(mixed.b) - 143).abs() <= 1, "{mixed:?}");
    }

    #[test]
    fn the_default_status_tones_cover_the_shared_words() {
        assert_eq!(default_status_tone("running"), StatusTone::Syncing);
        assert_eq!(default_status_tone("Completed"), StatusTone::Ok);
        assert_eq!(default_status_tone("failed"), StatusTone::Danger);
        assert_eq!(default_status_tone("cancelled"), StatusTone::Warn);
        assert_eq!(default_status_tone("mystery"), StatusTone::Muted);
    }

    #[test]
    fn the_root_rem_scales_tailwind_steps() {
        set_root_rem(13.0);
        assert_eq!(rem(), 13.0);
        assert_eq!(tw(4.0), 13.0);
        set_root_rem(-1.0);
        assert_eq!(rem(), 13.0);
    }

    #[test]
    fn eases_start_and_end_on_their_targets() {
        let theme = Theme::dark();
        for ease in [
            theme.ease_standard,
            theme.ease_enter,
            Ease(0.25, 0.1, 0.25, 1.0),
        ] {
            assert!(ease.at(0.0).abs() < 1e-3);
            assert!((ease.at(1.0) - 1.0).abs() < 1e-3);
        }
        // ease-standard is front-loaded: past 80% by the half.
        assert!(theme.ease_standard.at(0.5) > 0.8);
    }
}
