//! `StatusChip` (`.pw-status-chip`): a tone's texture face as a badge. A 1px frame, the
//! tone's top-to-bottom gradient with a 1px inset rim, 3px 9px padding, and 11px/500 ink at
//! line-height 1.2. It is round-ended, as the buttons and fields are, where the web's is
//! radius 6.

use gpui::{App, SharedString, div, prelude::*, px};

use super::material::face;
use crate::theme::{StatusTone, Theme, ToneSet};

pub fn chip_tone(theme: &Theme, tone: StatusTone) -> ToneSet {
    match tone {
        StatusTone::Ok | StatusTone::Healthy => theme.tone_success,
        StatusTone::Warn | StatusTone::Degraded => theme.tone_warn,
        StatusTone::Danger | StatusTone::Down => theme.tone_danger,
        StatusTone::Syncing => theme.tone_info,
        StatusTone::Muted | StatusTone::Unknown => theme.tone_neutral,
    }
}

/// The chip's face and ink, for callers that draw a chip-colored mark of their own.
pub fn chip_colors(theme: &Theme, tone: StatusTone) -> (gpui::Rgba, gpui::Rgba) {
    let set = chip_tone(theme, tone);
    (set.top, set.ink)
}

pub fn chip(label: impl Into<SharedString>, tone: StatusTone, cx: &App) -> gpui::Div {
    let set = chip_tone(Theme::global(cx), tone);
    div()
        .relative()
        .flex_none()
        .flex()
        .items_center()
        .border_1()
        .border_color(set.frame)
        .rounded_full()
        .bg(face(set.top, set.bottom))
        .px(px(9.0))
        .py(px(3.0))
        .text_size(px(11.0))
        .line_height(px(13.2))
        .font_weight(gpui::FontWeight::MEDIUM)
        .whitespace_nowrap()
        .text_color(set.ink)
        .child(
            div()
                .absolute()
                .inset_0()
                .rounded_full()
                .border_1()
                .border_color(set.rim),
        )
        .child(label.into())
}
