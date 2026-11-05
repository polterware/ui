//! A confirmation as the apps ask one (`A11yDialog size="sm"` with a footer, as Vitals asks
//! before a kill): a small [`dialog`] that says what an action touches before it runs. The
//! title and its description (12px muted, under the title), an optional recovery note in
//! the body (12px muted, how to get it back or what it cannot undo), Close, and Cancel beside
//! the confirm button, which is the danger button for removals and stops, over a
//! [`modal_layer`](super::modal_layer).
//!
//! It behaves like the design system's confirm and gpui-kit's `Dialog` with confirm and cancel
//! buttons: Enter confirms and Escape cancels (the app binds them to [`ConfirmAction`] and
//! [`CancelAction`] in the `Confirm` key context it holds focus in), and asking again while one
//! is open replaces it, which the app gets by keeping one pending request.
//!
//! [`ConfirmDialog::reason`] makes it the confirm that also asks why (`requestReason`), for a
//! change of intent such as reopening a decision: a field under the description, and the
//! confirm button, primary, stays disabled (Enter does nothing) until the field holds a
//! reason, which the app reads from the field when it confirms.

use gpui::{
    App, Entity, FocusHandle, IntoElement, KeyBinding, SharedString, Window, actions, div,
    prelude::*, px,
};

use crate::Handler;
use crate::button::{Button, ButtonKind};
use crate::text_input::TextInput;
use crate::theme::Theme;

use super::dialog::{DialogPose, DialogSize, dialog};

actions!(confirm, [ConfirmAction, CancelAction]);

/// Enter and Escape inside a `Confirm` context. [`crate::init`] installs them.
pub fn register_key_bindings(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("enter", ConfirmAction, Some("Confirm")),
        KeyBinding::new("escape", CancelAction, Some("Confirm")),
    ]);
}

#[derive(IntoElement)]
pub struct ConfirmDialog {
    id: SharedString,
    title: SharedString,
    description: SharedString,
    recovery: Option<SharedString>,
    confirm_label: SharedString,
    danger: bool,
    focus: Option<FocusHandle>,
    pose: DialogPose,
    on_cancel: Option<Handler>,
    on_confirm: Option<Handler>,
    reason: Option<Entity<TextInput>>,
}

impl ConfirmDialog {
    pub fn new(
        id: impl Into<SharedString>,
        title: impl Into<SharedString>,
        description: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            description: description.into(),
            recovery: None,
            confirm_label: "Confirm".into(),
            danger: false,
            focus: None,
            pose: DialogPose::SHOWN,
            on_cancel: None,
            on_confirm: None,
            reason: None,
        }
    }

    /// Asks why: `field` goes under the description, and confirming waits for a reason in it.
    pub fn reason(mut self, field: &Entity<TextInput>) -> Self {
        self.reason = Some(field.clone());
        self
    }

    /// How the change can be undone, or that it cannot.
    pub fn recovery(mut self, recovery: impl Into<SharedString>) -> Self {
        self.recovery = Some(recovery.into());
        self
    }

    pub fn confirm_label(mut self, label: impl Into<SharedString>) -> Self {
        self.confirm_label = label.into();
        self
    }

    /// The confirm button reads as danger, for removals and stops.
    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    /// The handle the dialog tracks, so Enter and Escape reach it; the app focuses it when the
    /// dialog opens.
    pub fn focus(mut self, focus: &FocusHandle) -> Self {
        self.focus = Some(focus.clone());
        self
    }

    /// How far in it is drawn, from [`DialogPresence::frame`](super::DialogPresence::frame).
    pub fn pose(mut self, pose: DialogPose) -> Self {
        self.pose = pose;
        self
    }

    pub fn on_cancel(mut self, handler: Handler) -> Self {
        self.on_cancel = Some(handler);
        self
    }

    pub fn on_confirm(mut self, handler: Handler) -> Self {
        self.on_confirm = Some(handler);
        self
    }
}

impl RenderOnce for ConfirmDialog {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let needs_reason = self
            .reason
            .as_ref()
            .is_some_and(|field| field.read(cx).text().trim().is_empty());
        let (cancel, confirm) = (self.on_cancel.clone(), self.on_confirm.clone());
        let button =
            |suffix: &str, kind: ButtonKind, label: SharedString, handler: &Option<Handler>| {
                let disabled = needs_reason && suffix == "confirm";
                let button = Button::new(SharedString::from(format!("{}-{suffix}", self.id)), kind)
                    .label(label)
                    .disabled(disabled);
                let handler = if disabled { &None } else { handler };
                match handler {
                    Some(handler) => button.on_click_rc(handler.clone()),
                    None => button,
                }
                .into_any_element()
            };
        let actions = [
            button(
                "cancel",
                ButtonKind::Secondary,
                "Cancel".into(),
                &self.on_cancel,
            ),
            button(
                "confirm",
                if self.danger {
                    ButtonKind::Danger
                } else {
                    ButtonKind::Primary
                },
                self.confirm_label.clone(),
                &self.on_confirm,
            ),
        ];
        let mut card = dialog(self.title, 1.0)
            .pose(self.pose)
            .size(DialogSize::Sm)
            .description(self.description)
            .footer(actions)
            .children(self.recovery.map(|recovery| {
                div()
                    .text_size(px(12.0))
                    .text_color(theme.color_text_muted)
                    .child(recovery)
            }))
            .children(
                self.reason
                    .clone()
                    .map(|field| div().w_full().mt(px(12.0)).child(field)),
            );
        if let Some(handler) = self.on_cancel.clone() {
            card = card.on_close(handler);
        }
        let mut root = div()
            .key_context("Confirm")
            .on_action(move |_: &CancelAction, window, cx| {
                if let Some(handler) = &cancel {
                    handler(window, cx);
                }
            })
            .on_action(move |_: &ConfirmAction, window, cx| {
                if needs_reason {
                    return;
                }
                if let Some(handler) = &confirm {
                    handler(window, cx);
                }
            })
            .child(card);
        if let Some(focus) = &self.focus {
            root = root.track_focus(focus);
        }
        root
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use gpui::{AppContext as _, TestAppContext, size};

    use super::*;
    use crate::test_support::{draw, open};

    #[gpui::test]
    fn a_confirm_that_asks_why_waits_for_a_reason(cx: &mut TestAppContext) {
        let field = cx.update(|cx| cx.new(TextInput::new));
        let focus = cx.update(|cx| cx.focus_handle());
        let confirmed = Rc::new(Cell::new(0));
        let (dialog_field, dialog_focus, count) = (field.clone(), focus.clone(), confirmed.clone());
        let mut cx = open(cx, size(800.0, 600.0), move |_, _| {
            let count = count.clone();
            ConfirmDialog::new("why", "Open a review?", "Say what is wrong with it.")
                .reason(&dialog_field)
                .focus(&dialog_focus)
                .on_confirm(Rc::new(move |_, _| count.set(count.get() + 1)))
                .into_any_element()
        });
        cx.update(|window, _| focus.focus(window));
        draw(&mut cx);
        cx.dispatch_action(ConfirmAction);
        assert_eq!(confirmed.get(), 0, "a blank reason confirms nothing");
        cx.update(|_, cx| field.update(cx, |field, cx| field.set_text("It is stale", cx)));
        draw(&mut cx);
        cx.update(|window, _| focus.focus(window));
        cx.dispatch_action(ConfirmAction);
        assert_eq!(confirmed.get(), 1);
    }

    #[test]
    fn it_confirms_plainly_unless_told_otherwise() {
        let plain = ConfirmDialog::new("reset", "Reset?", "Cards go back.");
        assert_eq!(plain.confirm_label.as_ref(), "Confirm");
        assert!(!plain.danger && plain.recovery.is_none());
        let kill = ConfirmDialog::new("kill", "Kill?", "Sends TERM.")
            .confirm_label("Kill")
            .recovery("It may not stop.")
            .danger();
        assert_eq!(kill.confirm_label.as_ref(), "Kill");
        assert!(kill.danger && kill.recovery.is_some());
    }
}
