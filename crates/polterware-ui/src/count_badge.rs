//! `CountBadge` (`count-badge.css` over `.pw-badge`): a count on a tone's texture face. The
//! badge's 1px frame, top-to-bottom gradient, and 1px inset rim, at least as wide as it is
//! tall, centered tabular 650 ink at line-height 1. Inline it is 16px tall, padded 0 4px, 9px
//! text on a `--radius-control - 2` radius; beside a sidebar label (`.pw-sidebar-item-count`)
//! 18px, padded 0 6px, 10px text on `--radius-item`.

use gpui::{App, FontWeight, IntoElement, SharedString, Window, div, prelude::*, px};

use crate::chip::chip_tone;
use crate::data_table::{sans, tabular};
use crate::material::face;
use crate::theme::{StatusTone, Theme};

/// `CountBadgeTone`, as the badge's tones map it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CountTone {
    Neutral,
    Info,
    Warning,
    Success,
    Danger,
}

impl CountTone {
    fn status(self) -> StatusTone {
        match self {
            CountTone::Neutral => StatusTone::Muted,
            CountTone::Info => StatusTone::Syncing,
            CountTone::Warning => StatusTone::Warn,
            CountTone::Success => StatusTone::Ok,
            CountTone::Danger => StatusTone::Danger,
        }
    }
}

/// The badge's geometry.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Geometry {
    size: f32,
    pad_x: f32,
    font_size: f32,
    radius: f32,
}

#[derive(IntoElement)]
pub struct CountBadge {
    count: u64,
    tone: CountTone,
    sidebar: bool,
}

impl CountBadge {
    /// The inline badge in the info tone, as `CountBadge` defaults to.
    pub fn new(count: u64) -> Self {
        Self {
            count,
            tone: CountTone::Info,
            sidebar: false,
        }
    }

    pub fn tone(mut self, tone: CountTone) -> Self {
        self.tone = tone;
        self
    }

    /// `.pw-sidebar-item-count`: the larger badge after a sidebar label.
    pub fn sidebar(mut self) -> Self {
        self.sidebar = true;
        self
    }

    fn geometry(&self, theme: &Theme) -> Geometry {
        if self.sidebar {
            Geometry {
                size: 18.0,
                pad_x: 6.0,
                font_size: 10.0,
                radius: theme.radius_item,
            }
        } else {
            Geometry {
                size: 16.0,
                pad_x: 4.0,
                font_size: 9.0,
                radius: theme.radius_control - 2.0,
            }
        }
    }
}

impl RenderOnce for CountBadge {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let set = chip_tone(&theme, self.tone.status());
        let geometry = self.geometry(&theme);
        // The border box holds the frame; the rim sits inside it.
        let inner = (geometry.radius - 1.0).max(0.0);
        div()
            .relative()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .h(px(geometry.size))
            .min_w(px(geometry.size))
            .px(px(geometry.pad_x))
            .border_1()
            .border_color(set.frame)
            .rounded(px(geometry.radius))
            .bg(face(set.top, set.bottom))
            .font(tabular(sans(&theme, FontWeight(650.0))))
            .text_size(px(geometry.font_size))
            .line_height(px(geometry.font_size))
            .text_color(set.ink)
            .whitespace_nowrap()
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded(px(inner))
                    .border_1()
                    .border_color(set.rim),
            )
            .child(SharedString::from(self.count.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::CountBadge;
    use crate::theme::Theme;

    #[test]
    fn the_sidebar_badge_is_the_larger_one() {
        let theme = Theme::dark();
        let inline = CountBadge::new(3).geometry(&theme);
        let sidebar = CountBadge::new(3).sidebar().geometry(&theme);
        assert_eq!((inline.size, inline.font_size), (16.0, 9.0));
        assert_eq!((sidebar.size, sidebar.pad_x), (18.0, 6.0));
        assert_eq!(sidebar.radius, theme.radius_item);
    }
}
