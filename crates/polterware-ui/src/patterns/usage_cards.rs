//! Usage summaries as cards: [`StatTile`], the totals row's figure; [`UsageCard`], one
//! consumer's spend with its parts and an optional limit meter; and [`card_grid`], the
//! responsive grid they sit in.
//!
//! The look is the design system's `Card` (`.card-panel`, [`card`]) with the metric layout of
//! the old Solid stat cards: a 12px muted label over a tabular figure. The grid behaves like
//! CSS `repeat(auto-fill, minmax(<min>, 1fr))`, which taffy's grid cannot express: it reads
//! its width from the frame before and picks as many equal columns as fit.

use gpui::{
    AnyElement, App, Context, ElementId, FontWeight, IntoElement, Rgba, SharedString, Window,
    canvas, div, prelude::*, px,
};

use crate::card::card;
use crate::data_table::{sans, tabular};
use crate::ellipsis::fade;
use crate::progress_bar::{bar_legend, progress_bar_sm, stacked_bar};
use crate::theme::Theme;
use crate::tooltip::tooltip;

/// A summary figure: an optional leading mark (an `IconTile`), then its label in 12px muted
/// ink over the value in 20px/500 tabular figures, on a card, with an optional caption and a
/// tooltip over the value.
#[derive(IntoElement)]
pub struct StatTile {
    id: SharedString,
    leading: Option<AnyElement>,
    label: SharedString,
    value: SharedString,
    caption: Option<SharedString>,
    tooltip: Option<SharedString>,
}

impl StatTile {
    pub fn new(
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        value: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            leading: None,
            label: label.into(),
            value: value.into(),
            caption: None,
            tooltip: None,
        }
    }

    /// A mark before the figure, centred on it: an `IconTile`.
    pub fn leading(mut self, leading: impl IntoElement) -> Self {
        self.leading = Some(leading.into_any_element());
        self
    }

    /// A 12px secondary line under the value.
    pub fn caption(mut self, caption: impl Into<SharedString>) -> Self {
        self.caption = Some(caption.into());
        self
    }

    /// What the value sums, shown on hover.
    pub fn tooltip(mut self, text: impl Into<SharedString>) -> Self {
        self.tooltip = Some(text.into());
        self
    }
}

impl RenderOnce for StatTile {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let mut value = div()
            .id(ElementId::Name(self.id.clone()))
            .min_w(px(0.0))
            .font(tabular(sans(&theme, FontWeight::MEDIUM)))
            .text_size(px(20.0))
            .line_height(px(26.0))
            .text_color(theme.color_text_primary)
            .child(fade(self.value));
        if let Some(text) = self.tooltip {
            value = value.tooltip(tooltip(text));
        }
        let text = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(theme.color_text_muted)
                    .child(fade(self.label)),
            )
            .child(value)
            .when_some(self.caption, |text, caption| {
                text.child(
                    div()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(theme.color_text_secondary)
                        .child(fade(caption)),
                )
            });
        card(cx)
            .min_w(px(0.0))
            .flex()
            .items_center()
            .gap(px(12.0))
            .px(px(14.0))
            .py(px(12.0))
            .children(self.leading)
            .child(text)
    }
}

/// The limit at the foot of a [`UsageCard`]: a label and a value over a small bar, and under
/// it a muted detail (when it resets) with an optional badge (a status chip) at its end.
/// With no fraction the bar is left out: the label says why.
pub struct UsageMeter {
    label: SharedString,
    value: Option<SharedString>,
    detail: Option<SharedString>,
    fraction: Option<f32>,
    color: Option<Rgba>,
    badge: Option<AnyElement>,
    tooltip: Option<SharedString>,
}

impl UsageMeter {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            value: None,
            detail: None,
            fraction: None,
            color: None,
            badge: None,
            tooltip: None,
        }
    }

    pub fn value(mut self, value: impl Into<SharedString>) -> Self {
        self.value = Some(value.into());
        self
    }

    /// A muted line under the bar.
    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        let detail = detail.into();
        self.detail = (!detail.is_empty()).then_some(detail);
        self
    }

    /// How full the bar is, 0 to 1, and its color.
    pub fn fraction(mut self, fraction: Option<f32>, color: Rgba) -> Self {
        self.fraction = fraction;
        self.color = Some(color);
        self
    }

    pub fn badge(mut self, badge: impl IntoElement) -> Self {
        self.badge = Some(badge.into_any_element());
        self
    }

    /// Where the figure comes from, shown on hover over the label.
    pub fn tooltip(mut self, text: impl Into<SharedString>) -> Self {
        self.tooltip = Some(text.into());
        self
    }
}

/// One consumer's usage on a card, in three rows spread from top to bottom (`justify-between`)
/// so the cards of a grid row line up: a leading mark (an avatar) with its name and an aside
/// (its provider); the headline figure over its caption; and at the foot the figure's parts
/// as a [`stacked_bar`] over its [`bar_legend`], then an optional [`UsageMeter`].
#[derive(IntoElement)]
pub struct UsageCard {
    id: SharedString,
    leading: Option<AnyElement>,
    title: SharedString,
    aside: Option<SharedString>,
    figure: SharedString,
    figure_tooltip: Option<SharedString>,
    caption: Option<SharedString>,
    caption_tooltip: Option<SharedString>,
    parts: Vec<(f32, Rgba, SharedString)>,
    meter: Option<UsageMeter>,
}

impl UsageCard {
    pub fn new(
        id: impl Into<SharedString>,
        title: impl Into<SharedString>,
        figure: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            leading: None,
            title: title.into(),
            aside: None,
            figure: figure.into(),
            figure_tooltip: None,
            caption: None,
            caption_tooltip: None,
            parts: Vec::new(),
            meter: None,
        }
    }

    pub fn leading(mut self, leading: impl IntoElement) -> Self {
        self.leading = Some(leading.into_any_element());
        self
    }

    /// Muted text at the header's end.
    pub fn aside(mut self, aside: impl Into<SharedString>) -> Self {
        let aside = aside.into();
        self.aside = (!aside.is_empty()).then_some(aside);
        self
    }

    pub fn figure_tooltip(mut self, text: impl Into<SharedString>) -> Self {
        self.figure_tooltip = Some(text.into());
        self
    }

    pub fn caption(mut self, caption: impl Into<SharedString>) -> Self {
        self.caption = Some(caption.into());
        self
    }

    pub fn caption_tooltip(mut self, text: impl Into<SharedString>) -> Self {
        self.caption_tooltip = Some(text.into());
        self
    }

    /// A part of the figure: its weight, its color, and its legend label.
    pub fn part(mut self, weight: f32, color: Rgba, label: impl Into<SharedString>) -> Self {
        self.parts.push((weight, color, label.into()));
        self
    }

    pub fn meter(mut self, meter: UsageMeter) -> Self {
        self.meter = Some(meter);
        self
    }
}

impl RenderOnce for UsageCard {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let small = |color: Rgba| {
            div()
                .min_w(px(0.0))
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(color)
        };
        let id = |part: &str| ElementId::Name(format!("{}-{part}", self.id).into());

        let header = div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .min_w(px(0.0))
            .children(self.leading)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .text_size(px(13.0))
                    .line_height(px(18.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.color_text_primary)
                    .child(fade(self.title)),
            )
            .when_some(self.aside, |header, aside| {
                header.child(small(theme.color_text_muted).flex_none().child(aside))
            });

        let mut figure = div()
            .id(id("figure"))
            .min_w(px(0.0))
            .font(tabular(sans(&theme, FontWeight::MEDIUM)))
            .text_size(px(24.0))
            .line_height(px(30.0))
            .text_color(theme.color_text_primary)
            .child(fade(self.figure));
        if let Some(text) = self.figure_tooltip {
            figure = figure.tooltip(tooltip(text));
        }

        let caption = self.caption.map(|caption| {
            let line = small(theme.color_text_secondary)
                .id(id("caption"))
                .font(tabular(sans(&theme, FontWeight::NORMAL)))
                .child(fade(caption));
            match self.caption_tooltip {
                Some(text) => line.tooltip(tooltip(text)),
                None => line,
            }
        });

        let parts = (!self.parts.is_empty()).then(|| {
            let weights: Vec<(f32, Rgba)> = self
                .parts
                .iter()
                .map(|(weight, color, _)| (*weight, *color))
                .collect();
            let legend: Vec<(Rgba, SharedString)> = self
                .parts
                .iter()
                .map(|(_, color, label)| (*color, label.clone()))
                .collect();
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(stacked_bar(&weights, cx))
                .child(bar_legend(&legend, cx))
        });

        let meter = self.meter.map(|meter| {
            let mut label = small(theme.color_text_secondary)
                .id(id("meter"))
                .flex_1()
                .child(fade(meter.label));
            if let Some(text) = meter.tooltip {
                label = label.tooltip(tooltip(text));
            }
            let color = meter.color.unwrap_or(theme.color_accent);
            let bar = meter
                .fraction
                .map(|fraction| progress_bar_sm(fraction, color, cx));
            let foot = (meter.detail.is_some() || meter.badge.is_some()).then(|| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .min_h(px(22.0))
                    .child(
                        small(theme.color_text_muted)
                            .flex_1()
                            .children(meter.detail.map(fade)),
                    )
                    .children(meter.badge)
            });
            div()
                .mt(px(12.0))
                .pt(px(10.0))
                .border_t_1()
                .border_color(theme.color_border_subtle)
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(label)
                        .when_some(meter.value, |row, value| {
                            row.child(
                                small(theme.color_text_primary)
                                    .flex_none()
                                    .font(tabular(sans(&theme, FontWeight::MEDIUM)))
                                    .child(value),
                            )
                        }),
                )
                .children(bar)
                .children(foot)
        });

        let middle = div()
            .flex()
            .flex_col()
            .min_w(px(0.0))
            .child(figure)
            .children(caption);
        let foot = (parts.is_some() || meter.is_some()).then(|| {
            div()
                .flex()
                .flex_col()
                .min_w(px(0.0))
                .children(parts)
                .children(meter)
        });
        card(cx)
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .justify_between()
            .gap(px(12.0))
            .px(px(14.0))
            .py(px(12.0))
            .child(header)
            .child(middle)
            .children(foot)
    }
}

struct GridWidth {
    width: Option<f32>,
}

/// The cards in as many equal columns of at least `min_width` as the width allows, `gap`
/// apart both ways; the cards of a row stretch to the tallest. The width comes from the frame
/// before, so a new width draws once more; the first frame guesses `guess` columns.
pub fn card_grid(
    id: impl Into<ElementId>,
    min_width: f32,
    gap: f32,
    guess: u16,
    cards: Vec<AnyElement>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let state = window.use_keyed_state(id, cx, |_, _: &mut Context<GridWidth>| GridWidth {
        width: None,
    });
    let cols = match state.read(cx).width {
        Some(width) => grid_columns(width, min_width, gap),
        None => guess.max(1),
    };
    let probe = canvas(
        move |bounds, window, cx| {
            let width = f32::from(bounds.size.width);
            let changed = state
                .read(cx)
                .width
                .is_none_or(|seen| (seen - width).abs() > 0.5);
            if changed {
                state.update(cx, |state, _| state.width = Some(width));
                window.refresh();
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .w_full()
    .h_0();
    div()
        .relative()
        .w_full()
        .child(probe)
        .child(
            div()
                .w_full()
                .grid()
                .grid_cols(cols)
                .gap(px(gap))
                .children(cards),
        )
        .into_any_element()
}

/// How many columns of at least `min_width`, `gap` apart, fit in `width`: one at least.
pub fn grid_columns(width: f32, min_width: f32, gap: f32) -> u16 {
    if !(width.is_finite() && min_width > 0.0) {
        return 1;
    }
    let fit = ((width + gap) / (min_width + gap)).floor();
    fit.clamp(1.0, f32::from(u16::MAX)) as u16
}

#[cfg(test)]
mod tests {
    use super::grid_columns;

    #[test]
    fn the_grid_fits_as_many_columns_as_the_width_holds() {
        assert_eq!(grid_columns(0.0, 260.0, 12.0), 1);
        assert_eq!(grid_columns(259.0, 260.0, 12.0), 1);
        assert_eq!(grid_columns(532.0, 260.0, 12.0), 2);
        assert_eq!(grid_columns(531.0, 260.0, 12.0), 1);
        assert_eq!(grid_columns(1100.0, 260.0, 12.0), 4);
        assert_eq!(grid_columns(f32::NAN, 260.0, 12.0), 1);
    }
}
