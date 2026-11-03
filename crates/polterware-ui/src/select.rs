//! `Select`'s trigger (`select.tsx`): a `control-field` that shows the picked option's label
//! and a 14px muted chevron, 0.5rem apart. The field is the recessed field face (lighter under
//! the pointer) with the control edge and drop; `sm` is 32px with 12px text on a 15px line.
//! Toolbars round it into a pill padded 1rem (`!rounded-full !px-4`).

use gpui::{App, ElementId, SharedString, Stateful, div, prelude::*, px};

use super::ellipsis::ellipsis;
use super::icons::{Icon, icon_colored};
use super::material::{self, Edge, face};
use super::tooltip::tooltip;
use crate::Handler;
use crate::theme::{Theme, tw};

pub fn select_trigger(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    title: Option<&'static str>,
    on_click: Handler,
    cx: &App,
) -> Stateful<gpui::Div> {
    let theme = *Theme::global(cx);
    let height = theme.field_height_sm;
    let radius = height / 2.0;
    let hover = face(theme.field_fill_hover_top, theme.field_fill_hover_bottom);
    div()
        .id(id.into())
        .relative()
        .w_full()
        .min_w(px(0.0))
        .h(px(height))
        .px(px(tw(4.0)))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(tw(2.0)))
        .rounded(px(radius))
        .bg(face(theme.field_fill_top, theme.field_fill_bottom))
        .hover(move |style| style.bg(hover))
        .shadow(material::control_drop(&theme))
        .text_size(px(12.0))
        .line_height(px(15.0))
        .text_color(theme.color_text_primary)
        .cursor_pointer()
        .when_some(title, |trigger, title| trigger.tooltip(tooltip(title)))
        .on_click(move |_, window, cx| on_click(window, cx))
        .child(div().flex_1().min_w(px(0.0)).child(ellipsis(label.into())))
        .child(icon_colored(
            Icon::ChevronDown,
            14.0,
            theme.color_text_muted,
        ))
        .child(material::edge(Edge::Strong, radius, &theme))
}
