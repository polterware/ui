//! Text that wraps over lines at the width its parent gives it, as `white-space: normal`
//! does on the web. It takes the inherited text style.
//!
//! gpui 0.2.2's text element paints the lines of the last time taffy measured it. Taffy
//! probes a text at other widths while it sizes a flex row (a zero-basis column is measured
//! at 0px) and serves the final size from its cache without measuring again, so a value that
//! fits its column can paint one letter a line. This element's measure only reports a size;
//! it shapes the lines it paints in prepaint, at its final width.
//!
//! [`WrapText::with_styles`] sets ranges of it apart, as `StyledText::with_highlights` does,
//! plus a font of their own (inline code in the mono face), for Markdown's inline marks.

use std::ops::Range;

use gpui::{
    App, AvailableSpace, Bounds, Element, ElementId, GlobalElementId, HighlightStyle,
    InspectorElementId, IntoElement, LayoutId, Pixels, SharedString, Size, Style, TextRun,
    TextStyle, Window, WrappedLine, px,
};

pub struct WrapText {
    text: SharedString,
    styles: Vec<(Range<usize>, SpanStyle)>,
}

/// How a range of a [`WrapText`] differs from the inherited style.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpanStyle {
    pub highlight: HighlightStyle,
    /// Another font family, such as the mono face for inline code.
    pub font_family: Option<SharedString>,
}

/// `text`, wrapping at the width it is given.
pub fn wrap_text(text: impl Into<SharedString>) -> WrapText {
    WrapText {
        text: text.into(),
        styles: Vec::new(),
    }
}

impl WrapText {
    /// Styles ranges of the text, in order and apart (a range that overlaps the one before,
    /// or does not fall on character boundaries, is left plain).
    pub fn with_styles(
        mut self,
        styles: impl IntoIterator<Item = (Range<usize>, SpanStyle)>,
    ) -> Self {
        self.styles = styles.into_iter().collect();
        self
    }
}

/// The runs of `len` bytes of text in `base`, with `styles` over their ranges.
fn styled_runs(text: &str, base: &TextStyle, styles: &[(Range<usize>, SpanStyle)]) -> Vec<TextRun> {
    let mut runs = Vec::new();
    let mut at = 0;
    for (range, style) in styles {
        if range.start < at
            || range.start >= range.end
            || range.end > text.len()
            || !text.is_char_boundary(range.start)
            || !text.is_char_boundary(range.end)
        {
            continue;
        }
        if at < range.start {
            runs.push(base.to_run(range.start - at));
        }
        let mut styled = base.clone().highlight(style.highlight);
        if let Some(family) = &style.font_family {
            styled.font_family = family.clone();
        }
        runs.push(styled.to_run(range.len()));
        at = range.end;
    }
    if at < text.len() || runs.is_empty() {
        runs.push(base.to_run(text.len() - at));
    }
    runs
}

pub struct WrapLayout {
    runs: Vec<TextRun>,
    font_size: Pixels,
    line_height: Pixels,
}

/// `text` shaped at `width` (none: one line a paragraph) and the size of its lines, rounded
/// up to the device pixel grid so that snapping never wraps a line that fits.
fn shape(
    text: &SharedString,
    layout: &WrapLayout,
    width: Option<Pixels>,
    window: &Window,
) -> (Vec<WrappedLine>, Size<Pixels>) {
    let lines = window
        .text_system()
        .shape_text(text.clone(), layout.font_size, &layout.runs, width, None)
        .map(|lines| lines.into_iter().collect::<Vec<_>>())
        .unwrap_or_default();
    let mut size: Size<Pixels> = Size::default();
    for line in &lines {
        let line_size = line.size(layout.line_height);
        size.height += line_size.height;
        size.width = size.width.max(line_size.width);
    }
    let scale = window.scale_factor();
    size.width = px((f32::from(size.width) * scale).ceil() / scale);
    if lines.is_empty() {
        size.height = layout.line_height;
    }
    (lines, size)
}

impl IntoElement for WrapText {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for WrapText {
    type RequestLayoutState = WrapLayout;
    type PrepaintState = Vec<WrappedLine>;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        _cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = style
            .line_height
            .to_pixels(font_size.into(), window.rem_size());
        let layout = WrapLayout {
            runs: styled_runs(&self.text, &style, &self.styles),
            font_size,
            line_height,
        };
        let measured = WrapLayout {
            runs: layout.runs.clone(),
            font_size,
            line_height,
        };
        let text = self.text.clone();
        // A flex row may shrink it to any width: it wraps to fit rather than holding the row
        // open at its longest word.
        let mut box_style = Style::default();
        box_style.min_size.width = px(0.0).into();
        let id = window.request_measured_layout(box_style, move |known, available, window, _| {
            let width = known.width.or(match available.width {
                AvailableSpace::Definite(room) => Some(room),
                _ => None,
            });
            let (_, size) = shape(&text, &measured, width, window);
            Size {
                width: known.width.unwrap_or(size.width),
                height: known.height.unwrap_or(size.height),
            }
        });
        (id, layout)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
        // The slack absorbs layout rounding: a line as wide as its box stays one line.
        let (lines, _) = shape(
            &self.text,
            layout,
            Some(bounds.size.width + px(0.5)),
            window,
        );
        lines
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        lines: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let align = window.text_style().text_align;
        let mut origin = bounds.origin;
        for line in lines.iter() {
            let _ = line.paint(origin, layout.line_height, align, Some(bounds), window, cx);
            origin.y += line.size(layout.line_height).height;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::FontWeight;

    #[test]
    fn styled_ranges_split_the_runs_and_a_bad_range_stays_plain() {
        let base = TextStyle::default();
        let bold = SpanStyle {
            highlight: HighlightStyle {
                font_weight: Some(FontWeight::BOLD),
                ..HighlightStyle::default()
            },
            font_family: None,
        };
        let mono = SpanStyle {
            font_family: Some("Mono".into()),
            ..SpanStyle::default()
        };
        let text = "a bold and code";
        let runs = styled_runs(text, &base, &[(2..6, bold.clone()), (11..15, mono)]);
        let lens: Vec<usize> = runs.iter().map(|run| run.len).collect();
        assert_eq!(lens, [2, 4, 5, 4]);
        assert_eq!(runs[1].font.weight, FontWeight::BOLD);
        assert_eq!(runs[3].font.family.as_ref(), "Mono");
        // Overlapping, empty, and out-of-range ranges are dropped; the runs still cover it all.
        let runs = styled_runs(
            text,
            &base,
            &[
                (2..6, bold.clone()),
                (4..8, bold),
                (20..30, SpanStyle::default()),
            ],
        );
        assert_eq!(runs.iter().map(|run| run.len).sum::<usize>(), text.len());
        assert_eq!(styled_runs("", &base, &[]).len(), 1);
    }
}
