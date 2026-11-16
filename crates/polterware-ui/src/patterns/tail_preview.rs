//! `TailPreview`: the end of a growing text (an agent's transcript as it streams) in a frame
//! of a fixed height, the newest line at the bottom.
//!
//! It has no counterpart in the web design system; the look is `.pw-scroll-fade`'s: the edges
//! that have more beyond them fade into the surface the preview sits on (the same gradient
//! [`end_fade`](super::scroll_fade::end_fade) lays over a scroll's ends, GPUI cannot mask).
//! A preview that is still live fades at both ends, since text goes on arriving under the bottom
//! one; a finished preview fades only at its top, where earlier lines were cut. gpui-kit has no
//! tail-preview control; its behavior follows a terminal's tail (`tail -f`): only the last
//! lines that fit show, each on one line cut with `…`, and a preview with no lines shows its
//! muted placeholder instead.
//!
//! The component draws what it is given: the caller keeps the lines (a few more than fit is
//! fine, the older ones are dropped here) and says whether the text is still live.

use gpui::{
    AnyElement, App, IntoElement, ParentElement, RenderOnce, Rgba, SharedString, Styled, Window,
    div, prelude::*, px,
};

use crate::data_table::mono;
use crate::ellipsis::ellipsis;
use crate::patterns::scroll_fade::end_fade;
use crate::theme::Theme;

/// How tall a fade at the preview's ends grows, in px.
pub const TAIL_FADE: f32 = 20.0;
/// One line's height, in px.
const LINE_HEIGHT: f32 = 16.0;
/// Room under the newest line of a live preview, so the bottom fade does not cover it.
const LIVE_REST: f32 = 6.0;

/// How a line reads against the others.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TailTone {
    /// The text itself (secondary text).
    #[default]
    Normal,
    /// Something around the text, such as a step or a note (muted text).
    Muted,
}

/// One line of the preview.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TailLine {
    text: SharedString,
    tone: TailTone,
}

impl TailLine {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            tone: TailTone::Normal,
        }
    }

    pub fn tone(mut self, tone: TailTone) -> Self {
        self.tone = tone;
        self
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn line_tone(&self) -> TailTone {
        self.tone
    }
}

/// How many lines of `line_height` fit in `height`, counting the `rest` kept clear under the
/// newest one.
pub fn lines_that_fit(height: f32, line_height: f32, rest: f32) -> usize {
    if line_height <= 0.0 {
        return 0;
    }
    ((height - rest).max(0.0) / line_height).floor() as usize
}

/// The fades at the top and the bottom, in px: a live preview fades at both ends, a finished
/// one only at its top, and a preview with nothing to show at neither.
pub fn fade_heights(live: bool, has_lines: bool) -> (f32, f32) {
    if !has_lines {
        return (0.0, 0.0);
    }
    (TAIL_FADE, if live { TAIL_FADE } else { 0.0 })
}

/// The last `capacity` of `lines`.
fn last_lines(lines: &[TailLine], capacity: usize) -> &[TailLine] {
    &lines[lines.len().saturating_sub(capacity)..]
}

/// The tail of a text: `lines` (oldest first) in a frame `height` tall, fading into `surface`.
pub fn tail_preview(lines: Vec<TailLine>, height: f32, surface: Rgba) -> TailPreview {
    TailPreview {
        lines,
        height,
        surface,
        live: false,
        placeholder: SharedString::default(),
    }
}

#[derive(IntoElement)]
pub struct TailPreview {
    lines: Vec<TailLine>,
    height: f32,
    surface: Rgba,
    live: bool,
    placeholder: SharedString,
}

impl TailPreview {
    /// The text is still arriving: fade at the bottom too, and keep room under the newest line.
    pub fn live(mut self, live: bool) -> Self {
        self.live = live;
        self
    }

    /// What a preview with no lines says, muted at its bottom.
    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }
}

impl RenderOnce for TailPreview {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let line_rest = if self.live { LIVE_REST } else { 0.0 };
        let capacity = lines_that_fit(self.height, LINE_HEIGHT, line_rest);
        let shown = last_lines(&self.lines, capacity);
        let rest = if shown.is_empty() { 0.0 } else { line_rest };
        let (top, bottom) = fade_heights(self.live, !shown.is_empty());
        let row = |text: SharedString, color: Rgba| -> AnyElement {
            div()
                .w_full()
                .h(px(LINE_HEIGHT))
                .flex_none()
                .min_w(px(0.0))
                .text_color(color)
                .child(ellipsis(text))
                .into_any_element()
        };
        let body = if shown.is_empty() {
            vec![row(self.placeholder.clone(), theme.color_text_muted)]
        } else {
            shown
                .iter()
                .map(|line| {
                    row(
                        line.text.clone(),
                        match line.tone {
                            TailTone::Normal => theme.color_text_secondary,
                            TailTone::Muted => theme.color_text_muted,
                        },
                    )
                })
                .collect()
        };
        let edge = |height: f32, at_top: bool| {
            let area = div().absolute().left_0().right_0().h(px(height));
            let area = if at_top {
                area.top_0()
            } else {
                area.bottom_0()
            };
            area.child(end_fade(self.surface, height, at_top))
        };
        div()
            .relative()
            .w_full()
            .h(px(self.height))
            .overflow_hidden()
            .font(mono(&theme, gpui::FontWeight::NORMAL))
            .text_size(px(theme.font_size_xs))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom(px(rest))
                    .flex()
                    .flex_col()
                    .justify_end()
                    .children(body),
            )
            .when(top > 0.5, |area| area.child(edge(top, true)))
            .when(bottom > 0.5, |area| area.child(edge(bottom, false)))
    }
}

#[cfg(test)]
mod tests {
    use gpui::{TestAppContext, prelude::*, size};

    use super::{
        LINE_HEIGHT, LIVE_REST, TAIL_FADE, TailLine, TailTone, fade_heights, last_lines,
        lines_that_fit, tail_preview,
    };
    use crate::test_support::open;
    use crate::theme::Theme;

    #[test]
    fn as_many_lines_fit_as_the_height_holds() {
        assert_eq!(lines_that_fit(160.0, 16.0, 0.0), 10);
        assert_eq!(lines_that_fit(165.0, 16.0, 0.0), 10);
        assert_eq!(lines_that_fit(160.0, 16.0, LIVE_REST), 9);
        assert_eq!(lines_that_fit(10.0, LINE_HEIGHT, 0.0), 0);
        assert_eq!(lines_that_fit(4.0, LINE_HEIGHT, LIVE_REST), 0);
        assert_eq!(lines_that_fit(100.0, 0.0, 0.0), 0);
    }

    #[test]
    fn a_live_preview_fades_at_both_ends_and_a_finished_one_at_its_top() {
        assert_eq!(fade_heights(true, true), (TAIL_FADE, TAIL_FADE));
        assert_eq!(fade_heights(false, true), (TAIL_FADE, 0.0));
        assert_eq!(fade_heights(true, false), (0.0, 0.0));
        assert_eq!(fade_heights(false, false), (0.0, 0.0));
    }

    #[test]
    fn only_the_newest_lines_show() {
        let lines: Vec<TailLine> = (0..5).map(|n| TailLine::new(format!("line {n}"))).collect();
        let shown = last_lines(&lines, 2);
        assert_eq!(shown.len(), 2);
        assert_eq!(shown[0].text(), "line 3");
        assert_eq!(shown[1].text(), "line 4");
        assert_eq!(last_lines(&lines, 9).len(), 5);
        assert!(last_lines(&lines, 0).is_empty());
    }

    #[test]
    fn a_line_takes_a_tone() {
        let line = TailLine::new("step").tone(TailTone::Muted);
        assert_eq!(line.line_tone(), TailTone::Muted);
        assert_eq!(TailLine::new("x").line_tone(), TailTone::Normal);
    }

    #[gpui::test]
    fn renders_live_finished_and_empty_previews(cx: &mut TestAppContext) {
        let visual = open(cx, size(420.0, 600.0), |_, cx| {
            let surface = Theme::global(cx).color_surface_subtle;
            let lines = vec![
                TailLine::new("› Running cargo test").tone(TailTone::Muted),
                TailLine::new("test result: ok"),
            ];
            gpui::div()
                .flex()
                .flex_col()
                .gap_4()
                .child(
                    tail_preview(lines.clone(), 164.0, surface)
                        .live(true)
                        .placeholder("Waiting for output…"),
                )
                .child(tail_preview(lines, 164.0, surface))
                .child(
                    tail_preview(Vec::new(), 164.0, surface)
                        .live(true)
                        .placeholder("Waiting for output…"),
                )
                .into_any_element()
        });
        visual.run_until_parked();
    }
}
