//! `text_swap`: a line of text that changes with a rolling hand-over, as a route's "Auto"
//! becomes the model it picked.
//!
//! While the text stays the same it is plain text. When it changes, the old letters rise and
//! fade out one after another, and the new ones come up from under the line into place on the
//! same stagger, so the change reads left to right. The new text holds the layout (its width
//! is the line's), and the old one is drawn over it until it is gone. The text takes the
//! style of the element around it.

use gpui::{App, ElementId, IntoElement, SharedString, Window, div, prelude::*, px};

/// How long one letter takes to leave or arrive.
const LETTER_SECONDS: f32 = 0.32;
/// How much later each letter starts than the one before it.
const STAGGER_SECONDS: f32 = 0.022;
/// How far a letter travels, as a share of the font size.
const RISE: f32 = 0.7;

/// `text` that rolls into its next value. `id` keys the last text it showed.
pub fn text_swap(id: impl Into<ElementId>, text: impl Into<SharedString>) -> TextSwap {
    TextSwap {
        id: id.into(),
        text: text.into(),
    }
}

#[derive(IntoElement)]
pub struct TextSwap {
    id: ElementId,
    text: SharedString,
}

/// The text last shown, and the one it is leaving with when the change began.
struct Seen {
    text: SharedString,
    leaving: Option<(SharedString, std::time::Instant)>,
}

/// Where letter `index` of a change is `t` seconds in: 0 before it moves, 1 once it is done.
pub(crate) fn letter_progress(index: usize, t: f32) -> f32 {
    let start = index as f32 * STAGGER_SECONDS;
    let u = ((t - start) / LETTER_SECONDS).clamp(0.0, 1.0);
    // Ease out cubic: quick to leave its place, soft to land.
    1.0 - (1.0 - u).powi(3)
}

/// How long a change from `old` to `new` plays.
pub(crate) fn swap_seconds(old: &str, new: &str) -> f32 {
    let letters = old.chars().count().max(new.chars().count());
    letters.saturating_sub(1) as f32 * STAGGER_SECONDS + LETTER_SECONDS
}

fn letters(
    text: &SharedString,
    t: f32,
    leaving: bool,
    rise: f32,
) -> impl Iterator<Item = gpui::AnyElement> + '_ {
    text.chars().enumerate().map(move |(index, letter)| {
        let k = letter_progress(index, t);
        let (offset, alpha) = if leaving {
            (-rise * k, 1.0 - k)
        } else {
            (rise * (1.0 - k), k)
        };
        let shown = if letter == ' ' { '\u{00a0}' } else { letter };
        div()
            .relative()
            .top(px(offset))
            .opacity(alpha)
            .child(SharedString::from(shown.to_string()))
            .into_any_element()
    })
}

impl RenderOnce for TextSwap {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let seen = window.use_keyed_state(self.id.clone(), cx, |_, _| Seen {
            text: self.text.clone(),
            leaving: None,
        });
        let now = std::time::Instant::now();
        let text = self.text.clone();
        let leaving = seen.update(cx, |seen, _| {
            if seen.text != text {
                let old = std::mem::replace(&mut seen.text, text.clone());
                seen.leaving = Some((old, now));
            }
            if let Some((old, at)) = &seen.leaving
                && at.elapsed().as_secs_f32() >= swap_seconds(old, &text)
            {
                seen.leaving = None;
            }
            seen.leaving.clone()
        });
        let Some((old, at)) = leaving else {
            return div()
                .id(self.id)
                .flex_none()
                .child(self.text)
                .into_any_element();
        };
        window.request_animation_frame();
        let t = at.elapsed().as_secs_f32();
        let rise = f32::from(window.text_style().font_size.to_pixels(window.rem_size())) * RISE;
        div()
            .id(self.id)
            .flex_none()
            .relative()
            .flex()
            .child(div().flex().children(letters(&self.text, t, false, rise)))
            .child(
                div()
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .flex()
                    .children(letters(&old, t, true, rise)),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{LETTER_SECONDS, STAGGER_SECONDS, letter_progress, swap_seconds};

    #[test]
    fn letters_move_one_after_another() {
        assert_eq!(letter_progress(0, 0.0), 0.0);
        assert_eq!(letter_progress(0, LETTER_SECONDS), 1.0);
        // The second letter has not left yet when the first starts.
        assert_eq!(letter_progress(1, STAGGER_SECONDS * 0.5), 0.0);
        assert!(letter_progress(0, 0.1) > letter_progress(3, 0.1));
    }

    #[test]
    fn a_change_lasts_until_its_last_letter_lands() {
        let seconds = swap_seconds("Auto", "gpt-6-luna");
        assert!((seconds - (9.0 * STAGGER_SECONDS + LETTER_SECONDS)).abs() < 1e-6);
        assert_eq!(letter_progress(9, seconds), 1.0);
    }
}
