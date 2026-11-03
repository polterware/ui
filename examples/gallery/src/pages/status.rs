//! `status.docs.tsx`: StatusChip in each tone and with a custom label. `StatusDot` with a
//! label has no GPUI port (Dispatch's `status_dot` is the run's bare dot), so "Compact
//! dots" is left out.

use gpui::{AnyElement, App, IntoElement, Window};
use polterware_ui::chip;
use polterware_ui::theme::StatusTone;

use super::super::Example;

fn chips(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    [
        ("Healthy", StatusTone::Healthy),
        ("Degraded", StatusTone::Degraded),
        ("Down", StatusTone::Down),
        ("Syncing", StatusTone::Syncing),
    ]
    .into_iter()
    .map(|(label, tone)| chip(label, tone, cx).into_any_element())
    .collect()
}

fn custom_label(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    vec![chip("Needs review", StatusTone::Warn, cx).into_any_element()]
}

pub fn examples() -> Vec<Example> {
    let example = |title, render| Example {
        slug: "status",
        title,
        height: 120.0,
        render,
    };
    vec![
        example("Status chips", chips),
        example("Custom label", custom_label),
    ]
}
