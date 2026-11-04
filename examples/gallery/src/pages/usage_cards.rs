//! The usage cards: the Segmented control in its states, the stacked bar and its legend,
//! a row of stat tiles, and usage cards with a known, an unknown, and a badged limit in the
//! responsive card grid.

use gpui::{AnyElement, App, IntoElement, Window, div, prelude::*, px};
use polterware_ui::theme::{StatusTone, Theme};
use polterware_ui::{
    Icon, IconTile, SegmentedControl, StatTile, TileTone, UsageCard, UsageMeter, bar_legend,
    card_grid, chip, stacked_bar,
};

use super::super::Example;

fn segmented(_: &mut Window, _: &mut App) -> Vec<AnyElement> {
    vec![
        SegmentedControl::new("gallery-window")
            .options(["24h", "7 days", "30 days", "All time"])
            .selected(1)
            .on_select(|_, _, _| {})
            .into_any_element(),
        SegmentedControl::new("gallery-two")
            .options(["Cards", "Table"])
            .into_any_element(),
    ]
}

fn stacked(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let theme = *Theme::global(cx);
    let parts = [
        (4.0, theme.chart_blue),
        (11.1, theme.chart_cyan),
        (3.1, theme.chart_violet),
    ];
    vec![
        div()
            .w(px(240.0))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(stacked_bar(&parts, cx))
            .child(bar_legend(
                &[
                    (theme.chart_blue, "Input 4.0M".into()),
                    (theme.chart_cyan, "Cache 11.1M".into()),
                    (theme.chart_violet, "Output 3.1M".into()),
                ],
                cx,
            ))
            .child(stacked_bar(&[], cx))
            .into_any_element(),
    ]
}

fn tiles(_: &mut Window, _: &mut App) -> Vec<AnyElement> {
    vec![
        div()
            .w(px(560.0))
            .grid()
            .grid_cols(4)
            .gap(px(8.0))
            .child(
                StatTile::new("gallery-cost", "Estimated cost", "$48.20")
                    .leading(
                        IconTile::new(Icon::CreditCard)
                            .size(32.0, 16.0)
                            .tone(TileTone::Infrastructure),
                    )
                    .tooltip("API prices"),
            )
            .child(StatTile::new("gallery-tokens", "Tokens", "31.4M"))
            .child(StatTile::new("gallery-runs", "Runs", "126"))
            .child(StatTile::new("gallery-per-run", "Per run", "$0.38").caption("120 priced"))
            .into_any_element(),
    ]
}

fn cards(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let theme = *Theme::global(cx);
    let card = |id: &str, name: &str, cost: &str, used: Option<f32>| {
        let mut meter = UsageMeter::new(if used.is_some() {
            "Weekly limit"
        } else {
            "Limit not reported"
        })
        .fraction(used, theme.color_success);
        if let Some(used) = used {
            meter = meter.value(format!("{:.0}%", used * 100.0));
        }
        UsageCard::new(id.to_string(), name.to_string(), cost.to_string())
            .aside("Provider")
            .caption("9.7M tokens · 41 runs")
            .part(3.0, theme.chart_blue, "Input")
            .part(4.6, theme.chart_cyan, "Cache")
            .part(2.1, theme.chart_violet, "Output")
            .meter(meter)
    };
    let badged = card("gallery-card-c", "antigravity", "$2.10", Some(0.996)).meter(
        UsageMeter::new("5h limit")
            .value("99.6%")
            .detail("Resets in 3h")
            .fraction(Some(0.996), theme.color_warning)
            .badge(chip("Ahead", StatusTone::Warn, cx)),
    );
    vec![
        div()
            .w(px(720.0))
            .child(card_grid(
                "gallery-cards",
                220.0,
                10.0,
                3,
                vec![
                    card("gallery-card-a", "codex", "$14.80", Some(0.23)).into_any_element(),
                    card("gallery-card-b", "cursor", "$4.30", None).into_any_element(),
                    badged.into_any_element(),
                ],
                window,
                cx,
            ))
            .into_any_element(),
    ]
}

pub fn examples() -> Vec<Example> {
    let example = |title, height, render| Example {
        slug: "usage-cards",
        title,
        height,
        render,
    };
    vec![
        example("Segmented control", 120.0, segmented),
        example("Stacked bar and legend", 140.0, stacked),
        example("Stat tiles", 140.0, tiles),
        example("Usage cards in a grid", 320.0, cards),
    ]
}
