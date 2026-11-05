//! `ChatBubble`: the user's own message in a conversation, as a chat app draws a sent one.
//! It sits at the end of its row, at most 78% of the row wide, on the raised surface with no
//! border, corners at `radius_lg` except the bottom-right (the tail side) at `radius_sm`.
//! The text is `text-sm` at prose leading; under the bubble a `text-xs` muted meta line holds
//! an optional label, an optional time, and an optional copy button. A long message collapses
//! to its first lines with a "Show all" link, and "Show less" folds it back.
//!
//! gpui-kit has no chat bubble to port; the expanded state is controlled, as in its other
//! disclosure controls: the caller keeps it, passes it in, and flips it from
//! [`ChatBubble::on_toggle`].

use std::rc::Rc;

use gpui::{
    App, ElementId, IntoElement, RenderOnce, SharedString, Window, div, prelude::*, px, relative,
};

use crate::Handler;
use crate::icon_button::icon_button;
use crate::icons::Icon;
use crate::theme::Theme;
use crate::wrap::wrap_text;

/// The widest a bubble grows, as a share of its row.
const MAX_SHARE: f32 = 0.78;
/// The trailing mark of text cut short.
const ELLIPSIS: &str = "…";

/// `text` cut to its first `max_lines` lines and a trailing ellipsis, or `None` when it has no
/// more lines than that (nothing to cut). Lines are the text's own (`\n`), not wrapped ones.
pub fn collapsed_text(text: &str, max_lines: usize) -> Option<String> {
    let max_lines = max_lines.max(1);
    let mut lines = text.lines();
    let kept: Vec<&str> = lines.by_ref().take(max_lines).collect();
    lines.next()?;
    let mut cut = kept.join("\n");
    cut.truncate(cut.trim_end().len());
    cut.push_str(ELLIPSIS);
    Some(cut)
}

/// Asks for the bubble open (`true`) or folded; the caller keeps the state.
type ToggleHandler = Rc<dyn Fn(bool, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct ChatBubble {
    id: ElementId,
    text: SharedString,
    meta_label: Option<SharedString>,
    time: Option<SharedString>,
    on_copy: Option<Handler>,
    collapse_after: Option<usize>,
    expanded: bool,
    on_toggle: Option<ToggleHandler>,
}

impl ChatBubble {
    pub fn new(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
            meta_label: None,
            time: None,
            on_copy: None,
            collapse_after: None,
            expanded: false,
            on_toggle: None,
        }
    }

    /// The meta line's label, such as "You · via Claude Code".
    pub fn meta_label(mut self, label: impl Into<SharedString>) -> Self {
        self.meta_label = Some(label.into());
        self
    }

    /// The meta line's time, already formatted.
    pub fn time(mut self, time: impl Into<SharedString>) -> Self {
        self.time = Some(time.into());
        self
    }

    /// Shows a copy button on the meta line.
    pub fn on_copy(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_copy = Some(Rc::new(handler));
        self
    }

    /// Collapses a text of more than `lines` lines to its first `lines` while it is not
    /// [`expanded`](Self::expanded).
    pub fn collapse_after(mut self, lines: usize) -> Self {
        self.collapse_after = Some(lines);
        self
    }

    /// Whether the whole text shows. The caller keeps it.
    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }

    /// Called with the expanded state the link asks for.
    pub fn on_toggle(mut self, handler: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_toggle = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for ChatBubble {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let cut = self
            .collapse_after
            .and_then(|lines| collapsed_text(&self.text, lines));
        let collapsible = cut.is_some();
        let shown = match cut {
            Some(cut) if !self.expanded => SharedString::from(cut),
            _ => self.text.clone(),
        };

        let toggle = collapsible.then(|| {
            let expanded = self.expanded;
            let on_toggle = self.on_toggle.clone();
            div()
                .id((self.id.clone(), "toggle"))
                .mt(px(theme.space_2))
                .cursor_pointer()
                .text_color(theme.color_accent)
                .hover(|style| style.text_color(theme.color_text_primary))
                .child(if expanded { "Show less" } else { "Show all" })
                .on_click(move |_, window, cx| {
                    if let Some(on_toggle) = &on_toggle {
                        on_toggle(!expanded, window, cx);
                    }
                })
        });

        let bubble = div()
            .max_w(relative(MAX_SHARE))
            .min_w(px(0.0))
            .px(px(theme.space_4))
            .py(px(theme.space_3))
            .bg(theme.color_surface_raised)
            .rounded(px(theme.radius_lg))
            .rounded_br(px(theme.radius_sm))
            .text_size(px(theme.font_size_sm))
            .line_height(px(theme.font_size_sm * theme.line_height))
            .text_color(theme.color_text_primary)
            .child(wrap_text(shown))
            .children(toggle);

        let copy = self.on_copy.map(|on_copy| {
            icon_button(
                (self.id.clone(), "copy"),
                Icon::Copy,
                "Copy message",
                on_copy,
                cx,
            )
        });
        let has_meta = self.meta_label.is_some() || self.time.is_some() || copy.is_some();
        let meta = has_meta.then(|| {
            div()
                .flex()
                .items_center()
                .justify_end()
                .gap(px(theme.space_2))
                .text_size(px(theme.font_size_xs))
                .line_height(px(theme.font_size_xs * theme.line_height))
                .text_color(theme.color_text_muted)
                .children(self.meta_label)
                .children(self.time)
                .children(copy)
        });

        div()
            .id(self.id)
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .items_end()
            .gap(px(theme.space_1))
            .child(bubble)
            .children(meta)
    }
}

#[cfg(test)]
mod tests {
    use super::collapsed_text;

    #[test]
    fn text_within_the_limit_is_not_cut() {
        assert_eq!(collapsed_text("one\ntwo", 2), None);
        assert_eq!(collapsed_text("one\ntwo", 5), None);
        assert_eq!(collapsed_text("", 1), None);
        assert_eq!(collapsed_text("one\n", 1), None);
    }

    #[test]
    fn longer_text_keeps_its_first_lines_and_an_ellipsis() {
        assert_eq!(
            collapsed_text("one\ntwo\nthree\nfour", 2).as_deref(),
            Some("one\ntwo…")
        );
        assert_eq!(collapsed_text("one\ntwo", 1).as_deref(), Some("one…"));
    }

    #[test]
    fn the_ellipsis_follows_the_last_word_not_trailing_space() {
        assert_eq!(
            collapsed_text("one  \n\n\nfour", 2).as_deref(),
            Some("one…")
        );
    }

    #[test]
    fn a_limit_of_zero_keeps_one_line() {
        assert_eq!(collapsed_text("one\ntwo", 0).as_deref(), Some("one…"));
    }
}
