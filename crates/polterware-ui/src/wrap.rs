//! Text that wraps over lines at the width its parent gives it, as `white-space: normal`
//! does on the web. It takes the inherited text style.
//!
//! gpui 0.2.2's text element paints the lines of the last time taffy measured it. Taffy
//! probes a text at other widths while it sizes a flex row (a zero-basis column is measured
//! at 0px) and serves the final size from its cache without measuring again, so a value that
//! fits its column can paint one letter a line. This element's measure only reports a size;
//! it shapes the lines it paints in prepaint, at its final width.

use gpui::{
    App, AvailableSpace, Bounds, Element, ElementId, GlobalElementId, InspectorElementId,
    IntoElement, LayoutId, Pixels, SharedString, Size, Style, TextRun, Window, WrappedLine, px,
};

pub struct WrapText {
    text: SharedString,
}

/// `text`, wrapping at the width it is given.
pub fn wrap_text(text: impl Into<SharedString>) -> WrapText {
    WrapText { text: text.into() }
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
            runs: vec![style.to_run(self.text.len())],
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
