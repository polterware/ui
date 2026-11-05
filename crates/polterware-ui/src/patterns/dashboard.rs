//! A dashboard of live readings, from the old Solid metrics page (Vitals' `MetricsDashboard`,
//! `MetricCard`, and `card-layout.ts`):
//!
//! - [`DashboardCard`] (`.vitals-section.card-panel`): a [`card`] padded `0 16px 16px` whose
//!   44px header holds a 14px icon and the title in 12px/600 uppercase muted ink, with an
//!   optional trailing slot; the body under it spreads its blocks over the card's height
//!   (`justify-content: space-between`, 8px apart) and scrolls when they run taller.
//! - [`headline`] (`.vitals-stat-block`): a 28px/600 tabular figure over its 12px muted caption.
//! - [`Reading`] (`.vitals-sensor`): one sensor value in 18px/600 tabular figures with its unit
//!   in 11px muted text after it, a quiet 11px label under it, and an optional small bar.
//!   [`readings`] lays them out as `.vitals-sensors`, an even grid of at least 72px columns.
//! - [`Meter`] (`.vitals-core`): a compact reading, its 11px mono label and value on one line
//!   over a small bar; [`meters`] lays them out as `.vitals-cores`, at least 96px columns 8px
//!   apart.
//! - [`PickRow`] (`.vitals-iface`): a row of 12px tabular cells (the first takes the room and
//!   fades, the rest keep their width) that picks what the charts under it draw, padded
//!   `4px 8px` at `--radius-control`: `--color-surface-hover` under the pointer,
//!   `--color-surface-selected` in primary ink when it is the picked one.
//! - [`Level`]: the bar's tone by threshold, `--color-chart-cyan`, `-amber`, or `-red`.
//!
//! The grid the cards sit on, and how they are dragged by their header and [`CardGrip`], is
//! [`dashboard_grid`](super::dashboard_grid).

use gpui::{
    AnyElement, App, CursorStyle, ElementId, FontWeight, IntoElement, MouseButton, Rgba,
    SharedString, Window, div, prelude::*, px,
};

use crate::Handler;
use crate::card::card;
use crate::data_table::{sans, tabular};
use crate::ellipsis::fade;
use crate::icons::{Icon, icon_colored};
use crate::progress_bar::progress_bar_sm;
use crate::theme::Theme;

use super::dashboard_grid::CardGrip;

const HEADER_HEIGHT: f32 = 44.0;
const CARD_PAD: f32 = 16.0;

/// How hot a reading runs, which picks its bar's color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Ok,
    Warn,
    Hot,
}

impl Level {
    /// `Hot` from `hot` up, `Warn` from `warn` up, else `Ok`.
    pub fn of(value: f64, warn: f64, hot: f64) -> Self {
        if value >= hot {
            Level::Hot
        } else if value >= warn {
            Level::Warn
        } else {
            Level::Ok
        }
    }

    pub fn color(self, theme: &Theme) -> Rgba {
        match self {
            Level::Ok => theme.chart_cyan,
            Level::Warn => theme.chart_amber,
            Level::Hot => theme.chart_red,
        }
    }
}

/// One dashboard card: its header and the body's blocks.
#[derive(IntoElement)]
pub struct DashboardCard {
    id: ElementId,
    title: SharedString,
    icon: Option<Icon>,
    trailing: Option<AnyElement>,
    grip: Option<CardGrip>,
    body: Vec<AnyElement>,
    spread: bool,
}

impl DashboardCard {
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            icon: None,
            trailing: None,
            grip: None,
            body: Vec::new(),
            spread: true,
        }
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Controls at the header's end.
    pub fn trailing(mut self, trailing: impl IntoElement) -> Self {
        self.trailing = Some(trailing.into_any_element());
        self
    }

    /// Dragged by its header and its grip (at the header's end, after the trailing
    /// controls), on the grid the grip comes from.
    pub fn grip(mut self, grip: CardGrip) -> Self {
        self.grip = Some(grip);
        self
    }

    /// The blocks start at the top instead of spreading over the card's height, for a body
    /// that fills it (a table, a chart that grows).
    pub fn packed(mut self) -> Self {
        self.spread = false;
        self
    }
}

impl ParentElement for DashboardCard {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.body.extend(elements);
    }
}

impl RenderOnce for DashboardCard {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let title = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .min_w(px(0.0))
            .text_size(px(12.0))
            .line_height(px(16.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme.color_text_muted)
            .children(
                self.icon
                    .map(|icon| icon_colored(icon, 14.0, theme.color_text_muted)),
            )
            .child(fade(SharedString::from(self.title.to_uppercase())));
        // `.vitals-card-actions`: the trailing controls, which a press on does not drag, then
        // the grip.
        let grip = self.grip.as_ref().map(|grip| grip.element(window, cx));
        let actions = (self.trailing.is_some() || grip.is_some()).then(|| {
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .children(self.trailing.map(|trailing| {
                    div()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(trailing)
                }))
                .children(grip)
        });
        let header = div()
            .flex_none()
            .h(px(HEADER_HEIGHT))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .when_some(self.grip, |header, grip| {
                header
                    .cursor(CursorStyle::OpenHand)
                    .on_mouse_down(MouseButton::Left, grip.on_press())
                    .on_mouse_up(MouseButton::Left, grip.on_release())
            })
            .child(title)
            .children(actions);
        // `.vitals-card-body`'s `overflow: auto`.
        let mut body = div()
            .id("body")
            .flex_1()
            .min_h(px(0.0))
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .overflow_y_scroll()
            .children(self.body);
        if self.spread {
            body = body.justify_between();
        }
        card(cx)
            .id(self.id)
            .size_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .px(px(CARD_PAD))
            .pb(px(CARD_PAD))
            .child(header)
            .child(body)
    }
}

/// A headline figure over its caption, and under them an optional bar.
pub fn headline(
    value: impl Into<SharedString>,
    caption: impl Into<SharedString>,
    bar: Option<(f32, Rgba)>,
    cx: &App,
) -> AnyElement {
    let theme = Theme::global(cx);
    div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .min_w(px(0.0))
        .child(
            div()
                .font(tabular(sans(theme, FontWeight::SEMIBOLD)))
                .text_size(px(28.0))
                .line_height(px(31.0))
                .text_color(theme.color_text_primary)
                .child(fade(value.into())),
        )
        .child(
            div()
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(theme.color_text_muted)
                .child(fade(caption.into())),
        )
        .children(bar.map(|(fraction, color)| {
            div()
                .mt(px(4.0))
                .child(progress_bar_sm(fraction, color, cx))
        }))
        .into_any_element()
}

/// One sensor reading.
#[derive(Clone)]
pub struct Reading {
    pub value: SharedString,
    pub unit: Option<SharedString>,
    pub label: SharedString,
    /// The bar, 0 to 1, and its color.
    pub bar: Option<(f32, Rgba)>,
}

impl Reading {
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            value: value.into(),
            unit: None,
            label: label.into(),
            bar: None,
        }
    }

    /// `"154.4 MB"` as the value `154.4` with the unit `MB`.
    pub fn split(text: &str, label: impl Into<SharedString>) -> Self {
        match text.rsplit_once(' ') {
            Some((value, unit)) => Self::new(value.to_string(), label).unit(unit.to_string()),
            None => Self::new(text.to_string(), label),
        }
    }

    pub fn unit(mut self, unit: impl Into<SharedString>) -> Self {
        let unit = unit.into();
        self.unit = (!unit.is_empty()).then_some(unit);
        self
    }

    pub fn bar(mut self, fraction: f32, color: Rgba) -> Self {
        self.bar = Some((fraction, color));
        self
    }

    fn render(self, cx: &App) -> AnyElement {
        let theme = Theme::global(cx);
        let value = div()
            .flex()
            // GPUI has no baseline alignment: the unit's line sits on the value's bottom,
            // lifted to its baseline.
            .items_end()
            .min_w(px(0.0))
            .overflow_hidden()
            .whitespace_nowrap()
            .child(
                div()
                    .font(tabular(sans(theme, FontWeight::SEMIBOLD)))
                    .text_size(px(18.0))
                    .line_height(px(20.0))
                    .text_color(theme.color_text_primary)
                    .child(self.value),
            )
            .children(self.unit.map(|unit| {
                div()
                    .ml(px(2.0))
                    .pb(px(2.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(px(11.0))
                    .line_height(px(14.0))
                    .text_color(theme.color_text_muted)
                    .child(unit)
            }));
        div()
            .flex()
            .flex_col()
            .gap(px(3.0))
            .min_w(px(0.0))
            .child(value)
            .child(
                div()
                    .text_size(px(11.0))
                    .line_height(px(14.0))
                    .text_color(theme.color_text_muted)
                    .child(fade(self.label)),
            )
            .children(self.bar.map(|(fraction, color)| {
                div()
                    .mt(px(2.0))
                    .child(progress_bar_sm(fraction, color, cx))
            }))
            .into_any_element()
    }
}

/// Readings in an even grid: `columns` of them a row (`.vitals-sensors-pair` is 2), or as
/// many columns of at least 72px as `width` holds, 10px apart, rows 12px apart.
pub fn readings(items: Vec<Reading>, columns: Option<u16>, width: f32, cx: &App) -> AnyElement {
    let columns = columns
        .unwrap_or_else(|| grid_fit(width, 72.0, 10.0))
        .max(1);
    div()
        .w_full()
        .grid()
        .grid_cols(columns)
        .gap_x(px(10.0))
        .gap_y(px(12.0))
        .children(items.into_iter().map(|item| item.render(cx)))
        .into_any_element()
}

fn grid_fit(width: f32, min: f32, gap: f32) -> u16 {
    if !(width.is_finite() && width > 0.0) {
        return 1;
    }
    ((width + gap) / (min + gap)).floor().clamp(1.0, 64.0) as u16
}

/// One compact reading: see the module's docs.
pub struct Meter {
    label: SharedString,
    value: SharedString,
    bar: (f32, Rgba),
}

impl Meter {
    /// `fraction` (0 to 1) fills the bar in `color`.
    pub fn new(
        label: impl Into<SharedString>,
        value: impl Into<SharedString>,
        fraction: f32,
        color: Rgba,
    ) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            bar: (fraction, color),
        }
    }

    fn render(self, cx: &App) -> AnyElement {
        let theme = Theme::global(cx);
        div()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(2.0))
            .font_family(theme.font_mono)
            .text_size(px(11.0))
            .line_height(px(14.0))
            .text_color(theme.color_text_secondary)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(6.0))
                    .child(div().min_w(px(0.0)).child(fade(self.label)))
                    .child(div().flex_none().child(self.value)),
            )
            .child(progress_bar_sm(self.bar.0, self.bar.1, cx))
            .into_any_element()
    }
}

/// Meters in an even grid of as many columns of at least 96px as `width` holds.
pub fn meters(items: Vec<Meter>, width: f32, cx: &App) -> AnyElement {
    div()
        .w_full()
        .grid()
        .grid_cols(grid_fit(width, 96.0, 8.0))
        .gap(px(8.0))
        .children(items.into_iter().map(|item| item.render(cx)))
        .into_any_element()
}

/// A row that picks a series: see the module's docs.
#[derive(IntoElement)]
pub struct PickRow {
    id: ElementId,
    cells: Vec<SharedString>,
    selected: bool,
    on_click: Option<Handler>,
}

impl PickRow {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            cells: Vec::new(),
            selected: false,
            on_click: None,
        }
    }

    /// The cells in order; the first is the name.
    pub fn cells<S: Into<SharedString>>(mut self, cells: impl IntoIterator<Item = S>) -> Self {
        self.cells = cells.into_iter().map(Into::into).collect();
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn on_click(mut self, handler: Handler) -> Self {
        self.on_click = Some(handler);
        self
    }
}

impl RenderOnce for PickRow {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let mut cells = self.cells.into_iter();
        let name = cells.next().unwrap_or_default();
        let on_click = self.on_click;
        div()
            .id(self.id)
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(12.0))
            .px(px(8.0))
            .py(px(4.0))
            .rounded(px(theme.radius_control))
            .cursor_pointer()
            .font(tabular(sans(&theme, FontWeight::NORMAL)))
            .text_size(px(12.0))
            .line_height(px(16.0))
            .whitespace_nowrap()
            .map(|row| {
                if self.selected {
                    row.bg(theme.color_surface_selected)
                        .text_color(theme.color_text_primary)
                } else {
                    row.text_color(theme.color_text_secondary).hover(|row| {
                        row.bg(theme.color_surface_hover)
                            .text_color(theme.color_text_primary)
                    })
                }
            })
            .child(div().flex_1().min_w(px(0.0)).child(fade(name)))
            .children(cells.map(|cell| div().flex_none().child(cell)))
            .when_some(on_click, |row, handler| {
                row.on_click(move |_, window, cx| handler(window, cx))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_and_readings() {
        assert_eq!(Level::of(10.0, 60.0, 85.0), Level::Ok);
        assert_eq!(Level::of(60.0, 60.0, 85.0), Level::Warn);
        assert_eq!(Level::of(99.0, 60.0, 85.0), Level::Hot);
        let split = Reading::split("154.4 MB", "Swap");
        assert_eq!(split.value.as_ref(), "154.4");
        assert_eq!(split.unit.as_ref().map(|unit| unit.as_ref()), Some("MB"));
        assert_eq!(Reading::split("3d", "Uptime").unit, None);
        assert_eq!(grid_fit(310.0, 72.0, 10.0), 3);
    }
}
