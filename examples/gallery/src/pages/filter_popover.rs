//! `ActiveFilterBar` and the filter button (`FilterPopover`): a bar of clauses with its `+` and
//! Clear, the pills alone, select-like, for fields that always hold one value, and a menu whose
//! fields come in sections (a scope's Domain and Component, then the page's Status), a hairline
//! between them. The menus are not open in any example.

use gpui::{AnyElement, App, Context, Entity, IntoElement, Window, div, prelude::*, px};
use polterware_ui::filter::{FilterOption, FilterSpec};
use polterware_ui::{FilterField, FilterPopover, Icon};

use super::super::Example;

fn fields() -> Vec<FilterField> {
    let options = |values: &[(&str, &str)]| {
        values
            .iter()
            .map(|(id, label)| FilterOption::new(*id, *label))
            .collect()
    };
    vec![
        FilterField::new(
            FilterSpec::multi(
                "by",
                "Group by",
                options(&[("day", "Day"), ("week", "Week"), ("model", "Model")]),
            )
            .single()
            .plural("groupings"),
            Some(Icon::Layers),
        ),
        FilterField::new(
            FilterSpec::multi(
                "since",
                "Since",
                options(&[("7d", "Last 7 days"), ("30d", "Last 30 days")]),
            )
            .single()
            .plural("windows"),
            Some(Icon::CalendarClock),
        ),
    ]
}

fn popover(
    id: &'static str,
    trigger: bool,
    window: &mut Window,
    cx: &mut App,
) -> Entity<FilterPopover> {
    window.use_keyed_state(id, cx, move |_, cx: &mut Context<FilterPopover>| {
        let mut popover = FilterPopover::new(id, "Filters", false, cx);
        if !trigger {
            popover = popover.without_trigger();
        }
        popover.set_fields(fields());
        popover.add_filter("by", &["day".to_string()], cx);
        popover.add_filter("since", &["7d".to_string()], cx);
        popover
    })
}

/// A scope's levels first, as `scopeFilterFields` gives them, then the page's own field.
fn sectioned_fields() -> Vec<FilterField> {
    let level = |id: &str, label: &str, icon: Icon, values: &[(&str, &str)]| {
        let mut spec = FilterSpec::multi(
            id,
            label,
            values
                .iter()
                .map(|(id, label)| FilterOption::new(*id, *label))
                .collect(),
        );
        spec.negatable = false;
        spec.repeatable = false;
        FilterField::new(spec, Some(icon)).section("scope")
    };
    vec![
        level(
            "scope-domain",
            "Domain",
            Icon::Boxes,
            &[("billing", "Billing"), ("identity", "Identity")],
        ),
        level(
            "scope-component",
            "Component",
            Icon::Blockchain01Icon,
            &[("invoices", "Invoices"), ("sessions", "Sessions")],
        ),
        FilterField::new(
            FilterSpec::multi(
                "status",
                "Status",
                vec![
                    FilterOption::new("open", "Open"),
                    FilterOption::new("later", "Later"),
                ],
            ),
            Some(Icon::ListChecks),
        ),
    ]
}

fn sections(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let filters = window.use_keyed_state(
        "filter-sections",
        cx,
        |_, cx: &mut Context<FilterPopover>| {
            let mut popover = FilterPopover::new("filter-sections", "Filters", false, cx);
            popover.set_fields(sectioned_fields());
            popover.add_filter("scope-domain", &["billing".to_string()], cx);
            popover
        },
    );
    vec![
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(div().flex().justify_end().child(filters.clone()))
            .children(FilterPopover::bar(&filters, window, cx))
            .into_any_element(),
    ]
}

fn bar(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let filters = popover("filter-bar", true, window, cx);
    vec![
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(div().flex().justify_end().child(filters.clone()))
            .children(FilterPopover::bar(&filters, window, cx))
            .into_any_element(),
    ]
}

fn pills(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let filters = popover("filter-pills", false, window, cx);
    vec![
        div()
            .flex()
            .items_center()
            .children(FilterPopover::pills(&filters, window, cx))
            .child(filters)
            .into_any_element(),
    ]
}

pub fn examples() -> Vec<Example> {
    let example = |title, render| Example {
        slug: "filter-popover",
        title,
        height: 120.0,
        render,
    };
    vec![
        example("Bar", bar),
        example("Pills", pills),
        example("Fields in sections", sections),
    ]
}
