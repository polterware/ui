//! `VoiceBlob`: a voice conversation's mark, fed here by a simulated level (syllables over a
//! slow phrase), in the brand ink for a reply, the accent for the microphone, silent, and
//! still (reduced motion).

use std::time::Instant;

use gpui::{AnyElement, App, IntoElement, Window, div, prelude::*, px};
use polterware_ui::VoiceBlob;
use polterware_ui::theme::Theme;

use super::super::Example;

/// A speech-like level: syllables about 5 a second, gathered into phrases with pauses.
fn speech(start: Instant) -> f32 {
    let t = start.elapsed().as_secs_f32();
    let phrase = ((t * 0.7).sin() * 0.5 + 0.5).powf(2.0);
    let syllable = ((t * 31.0).sin() * 0.5 + 0.5) * ((t * 13.0).sin() * 0.3 + 0.7);
    phrase * syllable * 0.35
}

fn labeled(blob: VoiceBlob, label: &'static str, cx: &App) -> AnyElement {
    let theme = Theme::global(cx);
    div()
        .w(px(170.0))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(8.0))
        .child(blob)
        .child(
            div()
                .text_size(px(12.0))
                .text_color(theme.color_text_muted)
                .child(label),
        )
        .into_any_element()
}

fn blobs(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let theme = *Theme::global(cx);
    let start = Instant::now();
    vec![
        labeled(
            VoiceBlob::new("gallery-voice-reply", move || speech(start)),
            "Reply speaking",
            cx,
        ),
        labeled(
            VoiceBlob::new("gallery-voice-mic", move || speech(start) * 0.7)
                .color(theme.color_accent),
            "Listening",
            cx,
        ),
        labeled(
            VoiceBlob::new("gallery-voice-silent", || 0.0).color(theme.color_text_muted),
            "Waiting",
            cx,
        ),
        labeled(
            VoiceBlob::new("gallery-voice-still", move || speech(start)).still(true),
            "Reduced motion",
            cx,
        ),
    ]
}

pub fn examples() -> Vec<Example> {
    vec![Example {
        slug: "voice-blob",
        title: "Voice blob",
        height: 190.0,
        render: blobs,
    }]
}
