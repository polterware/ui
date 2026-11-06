//! `NumberStepper` (`number-stepper.tsx` on Ark's NumberInput, `form-primitives.css`): a label
//! (12px `--color-text-secondary`) 8px over the control: a texture "−" button, a small field
//! 72px wide with its value centered, and a texture "+" button, 4px apart, their marks 12px.
//!
//! Typing a number reports it as it is typed (`onValueChange`); the buttons step it by one,
//! never under the minimum. Text that is not a number reports nothing. Setting the value from
//! outside leaves the field alone while it is focused, so a typed value is never overwritten
//! under the caret.

use gpui::{
    App, AppContext as _, Context, Entity, EventEmitter, Focusable, IntoElement, Render,
    SharedString, Window, div, prelude::*, px,
};

use crate::button::{Button, ButtonKind, ButtonSize};
use crate::icons::Icon;
use crate::text_input::{FieldSize, TextInput};
use crate::theme::Theme;

/// A value the person typed or stepped to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepperChanged(pub f64);

pub struct NumberStepper {
    id: SharedString,
    label: Option<SharedString>,
    input: Entity<TextInput>,
    value: f64,
    min: Option<f64>,
    /// The text last written by the stepper, which the observer does not report back.
    written: String,
}

impl EventEmitter<StepperChanged> for NumberStepper {}

fn format_value(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

impl NumberStepper {
    pub fn new(id: impl Into<SharedString>, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| TextInput::new(cx).size(FieldSize::Sm).centered());
        cx.observe(&input, |this, input, cx| {
            let text = input.read(cx).text().to_string();
            if text == this.written {
                return;
            }
            this.written = text.clone();
            if let Ok(value) = text.trim().parse::<f64>()
                && value.is_finite()
            {
                this.value = value;
                cx.emit(StepperChanged(value));
            }
        })
        .detach();
        Self {
            id: id.into(),
            label: None,
            input,
            value: 0.0,
            min: None,
            written: String::new(),
        }
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn min(mut self, min: f64) -> Self {
        self.min = Some(min);
        self
    }

    pub fn value(&self) -> f64 {
        self.value
    }

    /// Shows `value`, unless the field is being typed in.
    pub fn set_value(&mut self, value: f64, window: &Window, cx: &mut Context<Self>) {
        self.value = value;
        if self.input.read(cx).focus_handle(cx).is_focused(window) {
            return;
        }
        let text = format_value(value);
        if self.input.read(cx).text() != text {
            self.written = text.clone();
            self.input.update(cx, |input, cx| input.set_text(text, cx));
        }
    }

    fn step(&mut self, by: f64, cx: &mut Context<Self>) {
        let mut value = self.value + by;
        if let Some(min) = self.min {
            value = value.max(min);
        }
        self.value = value;
        let text = format_value(value);
        self.written = text.clone();
        self.input.update(cx, |input, cx| input.set_text(text, cx));
        cx.emit(StepperChanged(value));
    }
}

impl Render for NumberStepper {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::global(cx);
        let this = cx.entity();
        let step = |by: f64| {
            let this = this.clone();
            move |_: &mut Window, cx: &mut App| this.update(cx, |stepper, cx| stepper.step(by, cx))
        };
        let at_min = self.min.is_some_and(|min| self.value <= min);
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .text_size(px(12.0))
            .text_color(theme.color_text_secondary)
            .children(self.label.clone())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .child(
                        Button::new(
                            SharedString::from(format!("{}-decrease", self.id)),
                            ButtonKind::Secondary,
                        )
                        .size(ButtonSize::Icon)
                        .icon(Icon::MinusSignIcon)
                        .icon_size(12.0)
                        .disabled(at_min)
                        .on_click(step(-1.0)),
                    )
                    .child(div().w(px(72.0)).child(self.input.clone()))
                    .child(
                        Button::new(
                            SharedString::from(format!("{}-increase", self.id)),
                            ButtonKind::Secondary,
                        )
                        .size(ButtonSize::Icon)
                        .icon(Icon::Plus)
                        .icon_size(12.0)
                        .on_click(step(1.0)),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_values_read_without_a_fraction() {
        assert_eq!(format_value(240.0), "240");
        assert_eq!(format_value(-12.5), "-12.5");
    }
}
