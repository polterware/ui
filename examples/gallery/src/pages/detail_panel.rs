//! `detail-panel.docs.tsx`: the floating panel a list opens its active record in, here as
//! Polterware's Inbox draws a conflict: its chips after the title, sections of properties
//! and a numbered list, and the actions in the footer. The resize and expand controls have no
//! GPUI port, so the panel keeps its 400px.

use gpui::{AnyElement, App, IntoElement, Window, div, prelude::*, px};
use polterware_ui::patterns::{
    DetailPanel, detail_numbered_list, detail_properties, detail_property, detail_section,
};
use polterware_ui::theme::{StatusTone, Theme};
use polterware_ui::{Button, ButtonKind, ButtonSize, Icon, chip_rounded};

use super::super::Example;

fn record(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let theme = *Theme::global(cx);
    let value = |text: &'static str| div().child(text).into_any_element();
    let panel = DetailPanel::new("detail-panel-record", "Rename export snapshot")
        .aside(chip_rounded("Conflict", StatusTone::Muted, 6.0, cx))
        .aside(chip_rounded("Blocking", StatusTone::Danger, 6.0, cx))
        .section(detail_section(
            "Goes against",
            Icon::AlertDiamond,
            detail_properties([detail_property(
                "Rule",
                value("Wire every board before the canvas."),
                cx,
            )]),
            cx,
        ))
        .section(detail_section(
            "Options",
            Icon::ListFilter,
            detail_numbered_list(["Keep the rule", "Allow it for this card"], cx),
            cx,
        ))
        .section(detail_section(
            "Context",
            Icon::Network,
            detail_properties([detail_property("Where", value("card-0006"), cx)]),
            cx,
        ))
        .footer(
            div()
                .w_full()
                .flex()
                .gap(px(6.0))
                .child(
                    div().flex_1().child(
                        Button::new("detail-panel-decide", ButtonKind::Primary)
                            .size(ButtonSize::Sm)
                            .icon(Icon::Check)
                            .label("Decide")
                            .radius(8.0)
                            .full_width(),
                    ),
                )
                .child(
                    div().flex_1().child(
                        Button::new("detail-panel-later", ButtonKind::Secondary)
                            .size(ButtonSize::Sm)
                            .icon(Icon::Clock)
                            .label("Later")
                            .radius(8.0)
                            .full_width(),
                    ),
                ),
        );
    vec![
        div()
            .h(px(440.0))
            .w_full()
            .relative()
            .rounded(px(theme.radius_surface))
            .bg(theme.color_surface)
            .overflow_hidden()
            .child(panel)
            .into_any_element(),
    ]
}

pub fn examples() -> Vec<Example> {
    vec![Example {
        slug: "detail-panel",
        title: "Record beside a list",
        height: 488.0,
        render: record,
    }]
}
