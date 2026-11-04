//! `RadioGroup` (`.pw-radio-group`): single-choice rows 2px apart. A row (`.pw-radio-row`) is
//! padded `7px 8px` with a 10px gap at `--radius-item`, `--color-surface-hover` under the
//! pointer, its 12.5px label secondary and primary once picked. Its 16px round control
//! (`.pw-radio__control`) is the field fill (`--field-fill-top` to `--field-fill-bottom`) in
//! the well's ring; picked, it takes the control material (`--material-face-top` to
//! `--material-face-bottom`, the control drop) and a 6px dot in `--color-text-primary`.

use gpui::{App, IntoElement, SharedString, div, prelude::*, px};

use crate::ValueHandler;
use crate::material::{Edge, control_drop, edge, face};
use crate::theme::Theme;

/// `.pw-radio__control`'s size.
const CONTROL: f32 = 16.0;
/// The picked control's dot, at the 16px size.
const DOT: f32 = 6.0;
const LABEL: f32 = 12.5;

/// `RadioMark`: the round control alone, `size` across (16 in a group, 14 in a picker's
/// rows), its dot scaled with it.
pub fn radio_mark(on: bool, size: f32, theme: &Theme) -> gpui::Div {
    let dot = DOT * size / CONTROL;
    div()
        .relative()
        .size(px(size))
        .flex_none()
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .map(|control| {
            if on {
                control
                    .bg(face(theme.material_face_top, theme.material_face_bottom))
                    .shadow(control_drop(theme))
                    .child(
                        div()
                            .size(px(dot))
                            .rounded_full()
                            .bg(theme.color_text_primary),
                    )
            } else {
                control.bg(face(theme.field_fill_top, theme.field_fill_bottom))
            }
        })
        .child(edge(
            if on { Edge::Strong } else { Edge::Well },
            size / 2.0,
            theme,
        ))
}

#[derive(Clone, Debug)]
pub struct RadioOption {
    pub label: SharedString,
}

pub fn radio_group(
    group_id: &'static str,
    options: &[RadioOption],
    selected: usize,
    on_select: ValueHandler<usize>,
    cx: &App,
) -> impl IntoElement {
    let theme = *Theme::global(cx);
    let mut column = div().w_full().flex().flex_col().gap(px(2.0));
    for (index, option) in options.iter().enumerate() {
        let handler = on_select.clone();
        let on = index == selected;
        let control = radio_mark(on, CONTROL, &theme);
        column = column.child(
            div()
                .id((group_id, index))
                .w_full()
                .py(px(7.0))
                .px(px(theme.space_2))
                .flex()
                .items_center()
                .gap(px(10.0))
                .rounded(px(theme.radius_item))
                .cursor_pointer()
                .hover(move |style| style.bg(theme.color_surface_hover))
                .on_click(move |_, window, cx| handler(index, window, cx))
                .child(control)
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .text_size(px(LABEL))
                        .line_height(px(LABEL * 1.5))
                        .text_color(if on {
                            theme.color_text_primary
                        } else {
                            theme.color_text_secondary
                        })
                        .child(option.label.clone()),
                ),
        );
    }
    column
}
