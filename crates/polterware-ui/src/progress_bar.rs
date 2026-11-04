//! `ProgressBar` (`progress-bar.tsx`): an 8px recessed track, the field face, holding a pill
//! whose face is the bar's color mixed 65% into the material face (in Oklab) with the well's
//! inset ring. A value of 0 draws no range.

use gpui::{App, Div, Rgba, SharedString, div, prelude::*, px, relative};

use super::material::{self, Edge, face};
use crate::theme::{Theme, mix_oklab};

const HEIGHT: f32 = 8.0;
/// `size="sm"`.
const HEIGHT_SM: f32 = 6.0;

/// The bar at `fraction` (0 to 1) of its width in `color`.
pub fn progress_bar(fraction: f32, color: Rgba, cx: &App) -> Div {
    bar(fraction, color, HEIGHT, cx)
}

/// [`progress_bar`] at `size="sm"`, 6px tall.
pub fn progress_bar_sm(fraction: f32, color: Rgba, cx: &App) -> Div {
    bar(fraction, color, HEIGHT_SM, cx)
}

/// A 6px track split into one segment per `(weight, color)` part, each as wide as its share
/// of the weights (never under 3px). The segments meet flush, square where they touch; only
/// the bar's two ends are round. Parts that weigh nothing draw nothing; with no weight the
/// empty track shows. The segments wear the bar's mixed face under one inset ring.
pub fn stacked_bar(parts: &[(f32, Rgba)], cx: &App) -> Div {
    let theme = Theme::global(cx);
    let parts: Vec<(f32, Rgba)> = parts
        .iter()
        .copied()
        .filter(|(weight, _)| weight.is_finite() && *weight > 0.0)
        .collect();
    let total: f32 = parts.iter().map(|(weight, _)| weight).sum();
    let end = HEIGHT_SM / 2.0;
    let mut track = div()
        .relative()
        .w_full()
        .min_w(px(0.0))
        .h(px(HEIGHT_SM))
        .flex()
        .rounded_full()
        .bg(face(theme.field_fill_top, theme.field_fill_bottom));
    if total <= 0.0 {
        return track;
    }
    let last = parts.len() - 1;
    for (index, (weight, color)) in parts.iter().enumerate() {
        let mut segment = div()
            .h_full()
            .flex_basis(relative(weight / total))
            .flex_shrink()
            .min_w(px(3.0))
            .bg(face(
                mix_oklab(*color, theme.material_face_top, 0.65),
                mix_oklab(*color, theme.material_face_bottom, 0.65),
            ));
        if index == 0 {
            segment = segment.rounded_l(px(end));
        }
        if index == last {
            segment = segment.rounded_r(px(end));
        }
        track = track.child(segment);
    }
    track.child(
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(material::edge(Edge::Well, end, theme)),
    )
}

/// The key under a [`stacked_bar`]: per part a 6px dot in its color, its label in 12px
/// secondary ink, wrapping onto more lines when the row runs out.
pub fn bar_legend(items: &[(Rgba, SharedString)], cx: &App) -> Div {
    let theme = Theme::global(cx);
    div()
        .flex()
        .flex_wrap()
        .gap_x(px(10.0))
        .gap_y(px(2.0))
        .text_size(px(12.0))
        .line_height(px(16.0))
        .text_color(theme.color_text_secondary)
        .children(items.iter().map(|(color, label)| {
            div()
                .flex()
                .items_center()
                .gap(px(5.0))
                .child(
                    div()
                        .flex_none()
                        .size(px(HEIGHT_SM))
                        .rounded_full()
                        .bg(mix_oklab(*color, theme.material_face_top, 0.65)),
                )
                .child(label.clone())
        }))
}

fn bar(fraction: f32, color: Rgba, height: f32, cx: &App) -> Div {
    let theme = Theme::global(cx);
    let fraction = fraction.clamp(0.0, 1.0);
    div()
        .relative()
        .w_full()
        .min_w(px(0.0))
        .h(px(height))
        .rounded_full()
        .overflow_hidden()
        .bg(face(theme.field_fill_top, theme.field_fill_bottom))
        .when(fraction > 0.0, |track| {
            track.child(
                div()
                    .relative()
                    .h_full()
                    .w(relative(fraction))
                    .rounded_full()
                    .bg(face(
                        mix_oklab(color, theme.material_face_top, 0.65),
                        mix_oklab(color, theme.material_face_bottom, 0.65),
                    ))
                    .child(material::edge(Edge::Well, height / 2.0, theme)),
            )
        })
}
