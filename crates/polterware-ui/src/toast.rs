//! Toast models and the stack drawn over the window. A toast goes on its own once its
//! [`toast_lifetime`] has passed (the app holding the toasts dismisses it then), or at once
//! from its Close.

use std::time::Duration;

use gpui::{App, IntoElement, SharedString, div, prelude::*, px};

use crate::ValueHandler;
use crate::theme::Theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastTone {
    Info,
    Success,
    Warning,
    Danger,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Toast {
    pub id: u64,
    pub message: SharedString,
    pub tone: ToastTone,
}

/// How long a toast stays: a note or a success 4s, a warning 6s, an error 8s, so the longer
/// it may need reading, the longer it stays.
pub fn toast_lifetime(tone: ToastTone) -> Duration {
    Duration::from_secs(match tone {
        ToastTone::Info | ToastTone::Success => 4,
        ToastTone::Warning => 6,
        ToastTone::Danger => 8,
    })
}

fn tone_color(theme: &Theme, tone: ToastTone) -> gpui::Rgba {
    match tone {
        ToastTone::Info => theme.color_info,
        ToastTone::Success => theme.color_success,
        ToastTone::Warning => theme.color_warning,
        ToastTone::Danger => theme.color_danger,
    }
}

pub fn toast_stack(toasts: &[Toast], on_dismiss: ValueHandler<u64>, cx: &App) -> impl IntoElement {
    let theme = *Theme::global(cx);
    let mut stack = div()
        .absolute()
        .bottom(px(theme.space_4))
        .right(px(theme.space_4))
        .w(px(320.0))
        .flex()
        .flex_col()
        .gap(px(theme.space_2));
    for toast in toasts {
        let id = toast.id;
        let dismiss = on_dismiss.clone();
        let color = tone_color(&theme, toast.tone);
        stack = stack.child(
            div()
                .id(("toast", id))
                .w_full()
                .p(px(theme.space_3))
                .rounded(px(theme.radius_md))
                .bg(theme.color_surface_raised)
                .border_1()
                .border_color(theme.color_border)
                .flex()
                .items_center()
                .gap(px(theme.space_2))
                .child(div().size(px(8.0)).flex_none().rounded_full().bg(color))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .text_size(px(theme.font_size_sm))
                        .text_color(theme.color_text_primary)
                        .child(toast.message.clone()),
                )
                .child(
                    div()
                        .id(("toast-dismiss", id))
                        .cursor_pointer()
                        .text_size(px(theme.font_size_sm))
                        .text_color(theme.color_text_muted)
                        .child("Close")
                        .on_click(move |_, window, cx| dismiss(id, window, cx)),
                ),
        );
    }
    stack
}
