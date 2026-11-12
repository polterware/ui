//! The bridge between Mermaid's layout and GPUI's text system.
//!
//! [`TextMeasure`] is the seam `polterware-mermaid` leaves for the painter: its wrapper asks
//! how wide a line and how tall a font are, and the answer decides where labels break and how
//! large their boxes are. If the painter measured with anything but the text system it then
//! draws through, a label could wrap one way and paint another. [`SceneMeasure`] therefore
//! forwards every question to the same [`WindowTextSystem`] and the same fonts the paint pass
//! uses, and the scene's already-resolved lines are shaped with the same runs.
//!
//! Families are the app's, not the diagram's: Mermaid's default is Arial and its code spans
//! ask for `monospace`, which the theme answers with its sans and mono faces. A specific
//! family the theme does not have would fall back unpredictably, so all proportional text is
//! drawn in `theme.font_sans` and all code in `theme.font_mono`.

use std::sync::Arc;

use gpui::{
    Font as GpuiFont, FontStyle, FontWeight, Pixels, ShapedLine, SharedString,
    TextRun as GpuiTextRun, WindowTextSystem, black, px,
};

use polterware_mermaid::measure::{Font, TextMeasure};

/// A [`TextMeasure`] over the window's text system and the theme's fonts.
pub struct SceneMeasure {
    text_system: Arc<WindowTextSystem>,
    sans: SharedString,
    mono: SharedString,
}

impl SceneMeasure {
    pub fn new(
        text_system: Arc<WindowTextSystem>,
        sans: impl Into<SharedString>,
        mono: impl Into<SharedString>,
    ) -> Self {
        Self {
            text_system,
            sans: sans.into(),
            mono: mono.into(),
        }
    }

    /// The GPUI font a Mermaid font request becomes.
    pub fn gpui_font(&self, font: &Font) -> GpuiFont {
        GpuiFont {
            family: self.family(&font.family),
            weight: FontWeight(font.weight as f32),
            style: if font.italic {
                FontStyle::Italic
            } else {
                FontStyle::Normal
            },
            ..gpui::font(self.sans.clone())
        }
    }

    fn family(&self, family: &str) -> SharedString {
        let lower = family.to_ascii_lowercase();
        if lower.contains("mono") || lower.contains("code") || lower.contains("courier") {
            self.mono.clone()
        } else {
            self.sans.clone()
        }
    }

    /// Shapes one line with the same runs the paint pass builds, so its width is what is drawn.
    pub fn shape_line(
        &self,
        text: SharedString,
        font_size: Pixels,
        runs: &[GpuiTextRun],
    ) -> ShapedLine {
        self.text_system.shape_line(text, font_size, runs, None)
    }
}

impl TextMeasure for SceneMeasure {
    fn width(&self, text: &str, font: &Font) -> f64 {
        if text.is_empty() {
            return 0.0;
        }
        let run = GpuiTextRun {
            len: text.len(),
            font: self.gpui_font(font),
            color: black(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let layout = self.text_system.layout_line(
            text,
            px(font.size as f32),
            std::slice::from_ref(&run),
            None,
        );
        f32::from(layout.width) as f64
    }

    fn line_height(&self, font: &Font) -> f64 {
        let font_id = self.text_system.resolve_font(&self.gpui_font(font));
        let size = px(font.size as f32);
        let ascent = f32::from(self.text_system.ascent(font_id, size));
        let descent = f32::from(self.text_system.descent(font_id, size)).abs();
        (ascent + descent) as f64
    }
}
