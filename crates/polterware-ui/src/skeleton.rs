//! `Skeleton` and `PageSkeleton` (`@polterware/frontend-design-system`): the loading bones a
//! surface shows in the shape of what it is about to draw, instead of a spinner or a line of
//! "Loading…".
//!
//! A bone (`.pw-skeleton`) is an opaque block at `--radius-control` that pulses between
//! `--color-surface-raised` and `--color-surface-selected` over 1.4s, eased in and out, for as
//! long as it shows (`pw-skeleton-pulse`). Every bone on screen shares one clock, so they
//! breathe together as the CSS animation does, and the pulse draws frames only while a bone
//! is on screen: a loaded app costs nothing.
//!
//! [`page_skeleton`] is `PageSkeleton`, 12px in with its parts 12px apart: a toolbar strip
//! (a title bone, then at the end a 160px control and two square buttons), then, by
//! [`SkeletonLayout`], six table rows (an icon and four cells), page blocks (a full-width
//! card and two shorter ones), one canvas plane, or a detail (a main column of a heading,
//! a line, a card, and two lines beside a 180px rail of a label and three rows).
//! [`list_skeleton`] is a list's rows (a round avatar and two lines, as runs and models are
//! listed), and [`text_skeleton`] a paragraph's lines.

use std::sync::OnceLock;
use std::time::Instant;

use gpui::{App, Div, Rgba, Window, div, prelude::*, px, relative};

use crate::theme::{Theme, mix};

/// `pw-skeleton-pulse`'s period.
const PERIOD: f32 = 1.4;

/// The clock every bone pulses on.
fn epoch() -> Instant {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    *EPOCH.get_or_init(Instant::now)
}

/// This frame's bone color, and the next frame asked for so the pulse goes on. Call once
/// per skeleton and give the color to its [`bone`]s.
pub fn skeleton_tone(window: &mut Window, cx: &App) -> Rgba {
    let theme = Theme::global(cx);
    window.request_animation_frame();
    let phase = (epoch().elapsed().as_secs_f32() % PERIOD) / PERIOD;
    // 0 at the ends and 1 halfway, eased in and out as `ease-in-out` keyframes are.
    let pulse = 0.5 - 0.5 * (phase * std::f32::consts::TAU).cos();
    mix(
        theme.color_surface_raised,
        theme.color_surface_selected,
        pulse,
    )
}

/// One bone in `tone`: size it with `w` and `h`.
pub fn bone(tone: Rgba, cx: &App) -> Div {
    let theme = Theme::global(cx);
    div().flex_none().rounded(px(theme.radius_control)).bg(tone)
}

/// A bone `height` tall and `share` (0 to 1) of its row wide, round-ended as a line of text
/// reads.
pub fn line_bone(tone: Rgba, share: f32, height: f32) -> Div {
    div()
        .flex_none()
        .h(px(height))
        .w(relative(share))
        .rounded_full()
        .bg(tone)
}

/// What a [`page_skeleton`] stands in for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SkeletonLayout {
    /// A page of cards.
    #[default]
    Page,
    /// A table's rows.
    Table,
    /// One plane, such as a chart or a board.
    Canvas,
    /// A record: its main column beside a rail.
    Detail,
}

/// `PageSkeleton` in `layout`, with its toolbar strip.
pub fn page_skeleton(layout: SkeletonLayout) -> PageSkeleton {
    PageSkeleton {
        layout,
        toolbar: true,
        rows: 6,
    }
}

#[derive(IntoElement)]
pub struct PageSkeleton {
    layout: SkeletonLayout,
    toolbar: bool,
    rows: usize,
}

impl PageSkeleton {
    /// Leaves the toolbar strip out, for a page whose real toolbar already shows.
    pub fn without_toolbar(mut self) -> Self {
        self.toolbar = false;
        self
    }

    /// How many rows a table shows. Default: 6.
    pub fn rows(mut self, rows: usize) -> Self {
        self.rows = rows;
        self
    }
}

impl RenderOnce for PageSkeleton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let tone = skeleton_tone(window, cx);
        let toolbar = self.toolbar.then(|| {
            div()
                .h(px(36.0))
                .flex_none()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(bone(tone, cx).h(px(16.0)).w(px(112.0)))
                .child(div().flex_1())
                .child(bone(tone, cx).h(px(28.0)).w(px(160.0)).rounded_full())
                .child(bone(tone, cx).size(px(28.0)).rounded_full())
                .child(bone(tone, cx).size(px(28.0)).rounded_full())
        });
        let body = match self.layout {
            SkeletonLayout::Table => {
                let mut rows = div().flex_1().min_h(px(0.0)).flex().flex_col().gap(px(8.0));
                for _ in 0..self.rows {
                    rows = rows.child(
                        div()
                            .h(px(36.0))
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .child(bone(tone, cx).size(px(28.0)))
                            .child(line_bone(tone, 0.28, 12.0))
                            .child(line_bone(tone, 0.18, 12.0))
                            .child(line_bone(tone, 0.14, 12.0))
                            .child(div().flex_1())
                            .child(line_bone(tone, 0.12, 12.0)),
                    );
                }
                rows
            }
            SkeletonLayout::Page => div()
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .flex_col()
                .gap(px(10.0))
                .child(bone(tone, cx).h(px(96.0)).w_full())
                .child(bone(tone, cx).h(px(64.0)).w(relative(0.85)))
                .child(bone(tone, cx).h(px(64.0)).w(relative(0.70))),
            SkeletonLayout::Canvas => div()
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .flex_col()
                .child(bone(tone, cx).flex_1().w_full()),
            SkeletonLayout::Detail => div()
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .gap(px(16.0))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .gap(px(10.0))
                        .child(bone(tone, cx).h(px(20.0)).w(px(192.0)))
                        .child(line_bone(tone, 0.3, 12.0))
                        .child(bone(tone, cx).mt(px(8.0)).h(px(96.0)).w_full())
                        .child(line_bone(tone, 0.9, 12.0))
                        .child(line_bone(tone, 0.75, 12.0)),
                )
                .child(
                    div()
                        .w(px(180.0))
                        .flex_none()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(bone(tone, cx).h(px(12.0)).w(px(80.0)))
                        .child(bone(tone, cx).h(px(32.0)).w_full())
                        .child(bone(tone, cx).h(px(32.0)).w_full())
                        .child(bone(tone, cx).h(px(32.0)).w_full()),
                ),
        };
        div()
            .size_full()
            .min_h(px(200.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .p(px(12.0))
            .children(toolbar)
            .child(body)
    }
}

/// `rows` list rows in the shape of runs or models: a round `avatar` (px across) and two
/// lines beside it, a title and a shorter detail, each row a little different so the list
/// reads as text rather than as stripes.
pub fn list_skeleton(rows: usize, avatar: f32, window: &mut Window, cx: &App) -> Div {
    const TITLES: [f32; 5] = [0.62, 0.48, 0.7, 0.54, 0.4];
    const DETAILS: [f32; 5] = [0.4, 0.56, 0.34, 0.46, 0.5];
    let tone = skeleton_tone(window, cx);
    let line = (avatar * 0.28).clamp(8.0, 12.0);
    let mut list = div().w_full().flex().flex_col().gap(px(8.0));
    for index in 0..rows {
        list = list.child(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(px(10.0))
                .py(px(4.0))
                .child(bone(tone, cx).size(px(avatar)).rounded_full())
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .child(line_bone(tone, TITLES[index % TITLES.len()], line))
                        .child(line_bone(tone, DETAILS[index % DETAILS.len()], line - 2.0)),
                ),
        );
    }
    list
}

/// `lines` lines of a paragraph `height` tall each, the last one short, as the text it
/// stands in for would wrap.
pub fn text_skeleton(lines: usize, height: f32, window: &mut Window, cx: &App) -> Div {
    const WIDTHS: [f32; 6] = [0.96, 0.88, 0.92, 0.8, 0.94, 0.86];
    let tone = skeleton_tone(window, cx);
    let mut column = div().w_full().flex().flex_col().gap(px(height * 0.75));
    for index in 0..lines {
        let share = if index + 1 == lines {
            0.55
        } else {
            WIDTHS[index % WIDTHS.len()]
        };
        column = column.child(line_bone(tone, share, height));
    }
    column
}
