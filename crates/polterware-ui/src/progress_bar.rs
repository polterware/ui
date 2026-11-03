//! `ProgressBar` (`progress-bar.tsx`): an 8px recessed track, the field face, holding a pill
//! whose face is the bar's color mixed 65% into the material face (in Oklab) with the well's
//! inset ring. A value of 0 draws no range.

use gpui::{App, Div, Rgba, div, prelude::*, px, relative};

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
