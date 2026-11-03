//! Icon-only button: the design system's `Button variant="ghost" size="icon"`, a flat 28px
//! circle with a 14px icon. The tooltip text is the accessible label.

use gpui::{App, ElementId, SharedString};

use super::button::{Button, ButtonKind, ButtonSize};
use super::icons::Icon;
use crate::Handler;

pub fn icon_button(
    id: impl Into<ElementId>,
    name: Icon,
    label: impl Into<SharedString>,
    on_click: Handler,
    _cx: &App,
) -> Button {
    Button::new(id, ButtonKind::Ghost)
        .size(ButtonSize::Icon)
        .icon(name)
        .tooltip(label)
        .on_click_rc(on_click)
}
