//! One line of text that ends in `…` when it does not fit, as Tailwind's `truncate`
//! (`overflow: hidden; text-overflow: ellipsis; white-space: nowrap`) draws it, or, from
//! [`fade`], fades out over its last 24px as `.pw-inline-fade` masks it. GPUI cannot mask
//! text, so each glyph in that stretch takes its own alpha.
//!
//! gpui 0.2.2's `truncate()` keeps the size its text element measured first, before the
//! line's width is known, so it clips without the ellipsis. This element sizes to its text,
//! may shrink to nothing in a flex row, and cuts the line in prepaint, once its width is
//! final. Its own width is the text's rounded up to the device pixel grid, so pixel snapping
//! never cuts a line that fits. It takes the inherited text style.

use gpui::{
    App, AvailableSpace, Bounds, ContentMask, Element, ElementId, GlobalElementId, Hsla,
    InspectorElementId, IntoElement, LayoutId, Pixels, ShapedLine, SharedString, Style, TextRun,
    Window, px, size,
};

const ELLIPSIS: &str = "…";
const FADE_WIDTH: f32 = 24.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Overflow {
    Cut,
    Fade,
}

pub struct Ellipsis {
    text: SharedString,
    overflow: Overflow,
}

/// `text` on one line, cut with `…` to the width its parent gives it.
pub fn ellipsis(text: impl Into<SharedString>) -> Ellipsis {
    Ellipsis {
        text: text.into(),
        overflow: Overflow::Cut,
    }
}

/// `text` on one line, fading out over the last 24px of the width its parent gives it.
pub fn fade(text: impl Into<SharedString>) -> Ellipsis {
    Ellipsis {
        text: text.into(),
        overflow: Overflow::Fade,
    }
}

/// The runs of `line` with each glyph past `width - 24` at its share of the color's alpha.
fn fade_runs(text: &str, line: &ShapedLine, base: &TextRun, width: Pixels) -> Vec<TextRun> {
    let width = f32::from(width);
    let start = width - FADE_WIDTH;
    let color = base.color;
    let mut runs: Vec<TextRun> = Vec::new();
    let mut opaque = 0;
    for (index, ch) in text.char_indices() {
        let end = index + ch.len_utf8();
        let center = (f32::from(line.x_for_index(index)) + f32::from(line.x_for_index(end))) / 2.0;
        if runs.is_empty() && center <= start {
            opaque = end;
            continue;
        }
        if runs.is_empty() && opaque > 0 {
            runs.push(TextRun {
                len: opaque,
                ..base.clone()
            });
        }
        let alpha = ((width - center) / FADE_WIDTH).clamp(0.0, 1.0);
        runs.push(TextRun {
            len: end - index,
            color: Hsla {
                a: color.a * alpha,
                ..color
            },
            ..base.clone()
        });
    }
    if runs.is_empty() {
        runs.push(TextRun {
            len: text.len(),
            ..base.clone()
        });
    }
    runs
}

pub struct EllipsisLayout {
    runs: Vec<TextRun>,
    font_size: Pixels,
    line_height: Pixels,
    full: ShapedLine,
}

impl IntoElement for Ellipsis {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Ellipsis {
    type RequestLayoutState = EllipsisLayout;
    type PrepaintState = Option<ShapedLine>;

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
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = style
            .line_height
            .to_pixels(font_size.into(), window.rem_size());
        let runs = vec![style.to_run(self.text.len())];
        let full = window
            .text_system()
            .shape_line(self.text.clone(), font_size, &runs, None);
        // Layout snaps each edge to the device pixel grid, which can take up to a device
        // pixel off a box; sized up to the grid, a line that fits its own width still does.
        let scale = window.scale_factor();
        let width = px((f32::from(full.width) * scale).ceil() / scale);
        let mut layout = Style::default();
        layout.min_size.width = px(0.0).into();
        let id = window.request_measured_layout(layout, move |known, available, _, _| {
            let width = known.width.unwrap_or(match available.width {
                AvailableSpace::Definite(room) => width.min(room),
                _ => width,
            });
            size(width, known.height.unwrap_or(line_height))
        });
        let _ = cx;
        (
            id,
            EllipsisLayout {
                runs,
                font_size,
                line_height,
                full,
            },
        )
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        if self.text.is_empty() {
            return None;
        }
        // The browser cuts or fades a line that overflows by any amount, as a column rounded
        // under its text does; the slack only absorbs layout rounding.
        if layout.full.width <= bounds.size.width + px(0.05) {
            return Some(layout.full.clone());
        }
        if self.overflow == Overflow::Fade {
            let runs = fade_runs(&self.text, &layout.full, &layout.runs[0], bounds.size.width);
            return Some(window.text_system().shape_line(
                self.text.clone(),
                layout.font_size,
                &runs,
                None,
            ));
        }
        let style = window.text_style();
        let mut runs = layout.runs.clone();
        let cut = cx
            .text_system()
            .line_wrapper(style.font(), layout.font_size)
            .truncate_line(self.text.clone(), bounds.size.width, ELLIPSIS, &mut runs);
        Some(
            window
                .text_system()
                .shape_line(cut, layout.font_size, &runs, None),
        )
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        line: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(line) = line {
            // A fading line runs past its box; the box clips it.
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                let _ = line.paint(bounds.origin, layout.line_height, window, cx);
            });
        }
    }
}
