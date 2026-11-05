//! `CheckboxMark` (`.pw-checkbox__control`): the square control of a choice among many, as
//! [`radio_mark`](crate::radio_mark) is the round one of a single choice. Unchecked, it is
//! the field fill in the strong edge; checked, it takes the control material (the face and
//! the control drop) and a check in the primary text. At 14px its radius is 4px and its check
//! 12px; both scale with `size`.

use gpui::{div, prelude::*, px};

use crate::icons::{Icon, icon_colored};
use crate::material::{Edge, control_drop, edge, face};
use crate::theme::Theme;

/// The checkbox control alone, `size` across (14 in a menu or a picker's rows).
pub fn checkbox_mark(checked: bool, size: f32, theme: &Theme) -> gpui::Div {
    let radius = 4.0 * size / 14.0;
    let fill = if checked {
        face(theme.material_face_top, theme.material_face_bottom)
    } else {
        face(theme.field_fill_top, theme.field_fill_bottom)
    };
    div()
        .relative()
        .size(px(size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius))
        .bg(fill)
        .shadow(control_drop(theme))
        .when(checked, |mark| {
            mark.child(icon_colored(
                Icon::Check,
                size - 2.0,
                theme.color_text_primary,
            ))
        })
        .child(edge(Edge::Strong, radius, theme))
}

/// The control in its mixed state (`indeterminate`): the checked material with a minus, as a
/// header's box shows while only some of its rows are picked.
pub fn checkbox_mark_mixed(size: f32, theme: &Theme) -> gpui::Div {
    let radius = 4.0 * size / 14.0;
    div()
        .relative()
        .size(px(size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius))
        .bg(face(theme.material_face_top, theme.material_face_bottom))
        .shadow(control_drop(theme))
        .child(icon_colored(
            Icon::MinusSignIcon,
            size - 2.0,
            theme.color_text_primary,
        ))
        .child(edge(Edge::Strong, radius, theme))
}
