//! Electron's `toast.tsx`: a reversible change with a six-second Undo action.

use std::{collections::HashMap, rc::Rc};

use gpui::{AnyElement, App, IntoElement, Window, div, prelude::*, px};
use polterware_ui::toast::{Toast, ToastAction, ToastTone, toast_stack_with_actions};

use crate::Example;

fn undo(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let toasts = [Toast {
        id: 1,
        message: "Archived 2 cards".into(),
        tone: ToastTone::Info,
    }];
    let actions = HashMap::from([(
        1,
        ToastAction {
            label: "Undo".into(),
            on_click: Rc::new(|_, _| {}),
        },
    )]);
    vec![
        div()
            .relative()
            .w(px(360.0))
            .h(px(100.0))
            .child(toast_stack_with_actions(
                &toasts,
                &actions,
                Rc::new(|_, _, _| {}),
                cx,
            ))
            .into_any_element(),
    ]
}

pub fn examples() -> Vec<Example> {
    vec![Example {
        slug: "toast-undo",
        title: "A reversible change",
        height: 140.0,
        render: undo,
    }]
}
