//! Toast models and the stack drawn over the window. A toast goes on its own once its
//! [`toast_lifetime`] has passed (the app holding the toasts dismisses it then), or at once
//! from its Close.

use std::time::Duration;

use gpui::{App, IntoElement, SharedString, div, prelude::*, px};

use crate::theme::Theme;
use crate::{Handler, ValueHandler};

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

/// A follow-up in `toast.tsx`, with gpui-kit's Notification action callback behavior.
#[derive(Clone)]
pub struct ToastAction {
    pub label: SharedString,
    pub on_click: Handler,
}

/// `TOAST_ACTION_MS` in Electron's `toast.tsx`.
pub const TOAST_ACTION_LIFETIME: Duration = Duration::from_millis(6000);

pub fn toast_stack(
    toasts: &[Toast],
    on_dismiss: ValueHandler<u64>,
    cx: &App,
) -> impl IntoElement + use<> {
    toast_stack_with_actions(toasts, &Default::default(), on_dismiss, cx)
}

/// Electron's `ToastRegion` action slot; callers retain the toast models and remove an
/// action before invoking it, as gpui-kit's Notification does when dismissed.
pub fn toast_stack_with_actions(
    toasts: &[Toast],
    actions: &std::collections::HashMap<u64, ToastAction>,
    on_dismiss: ValueHandler<u64>,
    cx: &App,
) -> impl IntoElement + use<> {
    let theme = *Theme::global(cx);
    let mut stack = div()
        .absolute()
        .bottom(px(theme.space_4))
        .right(px(theme.space_4))
        .w(px(320.0))
        .flex()
        .flex_col()
        .gap(px(theme.space_2))
        .when(!actions.is_empty(), |stack| {
            let rem = crate::theme::rem();
            stack
                .w(px(20.0 * rem))
                .bottom(px(rem))
                .right(px(rem))
                .gap(px(0.5 * rem))
        });
    for toast in toasts {
        let id = toast.id;
        let dismiss = on_dismiss.clone();
        let color = tone_color(&theme, toast.tone);
        let action = actions.get(&id).cloned();
        let actionable = action.is_some();
        stack =
            stack.child(
                div()
                    .id(("toast", id))
                    .w_full()
                    .p(px(theme.space_3))
                    .when(actionable, |toast| {
                        // `ToastRegion`: px-3 py-2, items-start, text-[13px], radius-item.
                        let rem = crate::theme::rem();
                        toast
                            .px(px(0.75 * rem))
                            .py(px(0.5 * rem))
                            .rounded(px(theme.radius_item))
                            .shadow(crate::material::float_drop(&theme))
                    })
                    .rounded(px(if actionable {
                        theme.radius_item
                    } else {
                        theme.radius_md
                    }))
                    .bg(theme.color_surface_raised)
                    .border_1()
                    .border_color(theme.color_border)
                    .flex()
                    .items_center()
                    .when(actionable, |toast| toast.items_start())
                    .gap(px(theme.space_2))
                    .child(if actionable {
                        div().mt(px(0.125 * crate::theme::rem())).flex_none().child(
                            crate::icons::icon(crate::icons::Icon::InformationCircle, 14.0, cx),
                        )
                    } else {
                        div().size(px(8.0)).flex_none().rounded_full().bg(color)
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .text_size(px(if actionable { 13.0 } else { theme.font_size_sm }))
                            .text_color(theme.color_text_primary)
                            .child(toast.message.clone()),
                    )
                    .children(action.map(|action| {
                        let dismiss = on_dismiss.clone();
                        let used = std::cell::Cell::new(false);
                        div()
                            .id(("toast-action", id))
                            .debug_selector(|| "toast-action".into())
                            .cursor_pointer()
                            .rounded(px(theme.radius_item))
                            .px(px(6.0))
                            .text_size(px(12.5))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(theme.color_accent)
                            .hover(|style| style.bg(theme.color_surface_hover))
                            .child(action.label)
                            .on_click(move |_, window, cx| {
                                if used.replace(true) {
                                    return;
                                }
                                dismiss(id, window, cx);
                                (action.on_click)(window, cx);
                            })
                    }))
                    .children((!actionable).then(|| {
                        div()
                            .id(("toast-dismiss", id))
                            .cursor_pointer()
                            .text_size(px(theme.font_size_sm))
                            .text_color(theme.color_text_muted)
                            .child("Close")
                            .on_click(move |_, window, cx| dismiss(id, window, cx))
                    })),
            );
    }
    stack
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, TestAppContext, size};
    use std::{cell::RefCell, collections::HashMap, rc::Rc};

    #[gpui::test]
    fn undo_dismisses_before_calling_the_action_once(cx: &mut TestAppContext) {
        let events = Rc::new(RefCell::new(Vec::new()));
        let log = events.clone();
        let visible = Rc::new(std::cell::Cell::new(true));
        let mut cx = crate::test_support::open(cx, size(600.0, 400.0), move |_, cx| {
            if !visible.get() {
                return div().into_any_element();
            }
            let action_log = log.clone();
            let dismiss_log = log.clone();
            let visible = visible.clone();
            let toast = Toast {
                id: 1,
                message: "Archived 2 cards".into(),
                tone: ToastTone::Info,
            };
            let actions = HashMap::from([(
                1,
                ToastAction {
                    label: "Undo".into(),
                    on_click: Rc::new(move |_, _| action_log.borrow_mut().push("undo")),
                },
            )]);
            toast_stack_with_actions(
                &[toast],
                &actions,
                Rc::new(move |_, window, _| {
                    dismiss_log.borrow_mut().push("dismiss");
                    visible.set(false);
                    window.refresh();
                }),
                cx,
            )
            .into_any_element()
        });
        // The action is before Close in the 320px stack at the bottom right.
        let action = cx
            .debug_bounds("toast-action")
            .expect("the action is drawn");
        cx.simulate_click(action.center(), Modifiers::none());
        cx.simulate_click(action.center(), Modifiers::none());
        assert_eq!(*events.borrow(), vec!["dismiss", "undo"]);
        assert_eq!(TOAST_ACTION_LIFETIME, Duration::from_millis(6000));
    }
}
