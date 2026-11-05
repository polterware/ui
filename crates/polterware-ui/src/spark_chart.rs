//! `SparkChart` (`spark-chart.tsx`, drawn there by uPlot): a series as a 2px line in a chart
//! color, over the value axis's grid lines in `--color-chart-grid` with compact ticks
//! (`1.2K`, `3.4M`) on the left in muted 10px text, inset 8px at the top and right. A live
//! series fills its slots from the right, so the newest value always sits at the right edge
//! and the line slides left as values come.
//!
//! It behaves like gpui-kit's `LineChart` and `AreaChart` on hover: the pointer marks the
//! nearest point with a vertical line and a dot, and a label over the plot reads its value.
//! It is still otherwise: no draw-in and no motion, so a chart repaints only when its series
//! or the hover changes.

use std::rc::Rc;

use gpui::{
    App, Bounds, Context, ElementId, FontWeight, IntoElement, MouseMoveEvent, PathBuilder, Pixels,
    Point, Rgba, SharedString, Window, canvas, div, fill, point, prelude::*, px, size,
};

use crate::data_table::{sans, tabular, text_width};
use crate::theme::{Theme, fade};

/// uPlot's default height in `SparkChart`.
pub const SPARK_HEIGHT: f32 = 96.0;
const TICK_SIZE: f32 = 10.0;
const TICK_GAP: f32 = 6.0;
const INSET_TOP: f32 = 8.0;
const INSET_RIGHT: f32 = 8.0;
const STROKE: f32 = 2.0;
const DOT: f32 = 6.0;

type Format = Rc<dyn Fn(f32) -> SharedString>;

/// Ticks as uPlot's `compactTick` writes them: `950`, `1.2K`, `3.4M`, `1.1G`.
pub fn compact_tick(value: f32) -> SharedString {
    let magnitude = value.abs();
    for (unit, scale) in [("G", 1e9), ("M", 1e6), ("K", 1e3)] {
        if magnitude >= scale {
            return trim(value / scale, unit).into();
        }
    }
    trim(value, "").into()
}

fn trim(value: f32, unit: &str) -> String {
    let text = format!("{value:.1}");
    let text = text.strip_suffix(".0").unwrap_or(&text);
    format!("{text}{unit}")
}

/// The step of 1, 2, or 5 times a power of ten nearest `raw`.
fn nice_step(raw: f32) -> f32 {
    if !(raw.is_finite() && raw > 0.0) {
        return 1.0;
    }
    let power = 10f32.powf(raw.log10().floor());
    let fraction = raw / power;
    let nice = if fraction < 1.5 {
        1.0
    } else if fraction < 3.0 {
        2.0
    } else if fraction < 7.0 {
        5.0
    } else {
        10.0
    };
    nice * power
}

/// The value range and its grid ticks: `domain` when given, else from 0 (or the lowest value,
/// when it is negative) to the first tick at or over the highest value, in about `count`
/// steps. An empty or flat series still gets a range.
pub fn value_axis(
    values: &[f32],
    domain: Option<(f32, f32)>,
    count: usize,
) -> (f32, f32, Vec<f32>) {
    let count = count.max(1) as f32;
    let (low, high, step) = match domain {
        Some((low, high)) if high > low => (low, high, (high - low) / count),
        _ => {
            let finite = values.iter().copied().filter(|value| value.is_finite());
            let high = finite.clone().fold(0.0f32, f32::max);
            let low = finite.fold(0.0f32, f32::min);
            let step = nice_step((high - low) / count);
            let high = (high / step).ceil() * step;
            let low = (low / step).floor() * step;
            if high > low {
                (low, high, step)
            } else {
                (0.0, step, step)
            }
        }
    };
    let mut ticks = Vec::new();
    let mut tick = low;
    while tick <= high + step * 1e-3 && ticks.len() < 64 {
        ticks.push(tick);
        tick += step;
    }
    (low, high, ticks)
}

/// Where each value sits across `slots` evenly spaced points from the left edge to the right:
/// the values take the last slots, so the newest one lands on the right edge.
fn slot_x(index: usize, len: usize, slots: usize, left: f32, width: f32) -> f32 {
    let slots = slots.max(len).max(2);
    let slot = slots - len + index;
    left + width * slot as f32 / (slots - 1) as f32
}

#[derive(Default)]
struct Hover {
    /// The plot's bounds from the last paint, in window coordinates.
    plot: Option<Bounds<Pixels>>,
    /// The hovered value's index.
    index: Option<usize>,
}

#[derive(IntoElement)]
pub struct SparkChart {
    id: ElementId,
    values: Vec<f32>,
    slots: usize,
    height: f32,
    color: Option<Rgba>,
    area: bool,
    domain: Option<(f32, f32)>,
    axis: bool,
    ticks: usize,
    format: Option<Format>,
    label: Option<SharedString>,
}

impl SparkChart {
    /// The series, oldest first.
    pub fn new(id: impl Into<ElementId>, values: impl IntoIterator<Item = f32>) -> Self {
        Self {
            id: id.into(),
            values: values.into_iter().collect(),
            slots: 0,
            height: SPARK_HEIGHT,
            color: None,
            area: false,
            domain: None,
            axis: true,
            ticks: 2,
            format: None,
            label: None,
        }
    }

    /// How many points the width holds, such as the history window's seconds; a shorter
    /// series sits at the right. By default the series fills the width.
    pub fn slots(mut self, slots: usize) -> Self {
        self.slots = slots;
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    /// The line's color; `--color-chart-red` by default, as `strokeToken` is.
    pub fn color(mut self, color: Rgba) -> Self {
        self.color = Some(color);
        self
    }

    /// Fills under the line with its color, faint.
    pub fn area(mut self) -> Self {
        self.area = true;
        self
    }

    /// A fixed value range, such as 0 to 100 for a percentage.
    pub fn domain(mut self, low: f32, high: f32) -> Self {
        self.domain = Some((low, high));
        self
    }

    /// The value axis's ticks and grid lines; on by default.
    pub fn axis(mut self, axis: bool) -> Self {
        self.axis = axis;
        self
    }

    /// About how many steps the value axis takes (2 by default: bottom, middle, top).
    pub fn ticks(mut self, steps: usize) -> Self {
        self.ticks = steps.max(1);
        self
    }

    /// How a value reads, on the ticks and on hover; [`compact_tick`] by default.
    pub fn format(mut self, format: impl Fn(f32) -> SharedString + 'static) -> Self {
        self.format = Some(Rc::new(format));
        self
    }

    /// What the series is, before the hovered value ("CPU 42%").
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl RenderOnce for SparkChart {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let color = self.color.unwrap_or(theme.chart_red);
        let format = self.format.unwrap_or_else(|| Rc::new(compact_tick));
        let (low, high, ticks) = value_axis(&self.values, self.domain, self.ticks);
        let tick_font = tabular(sans(&theme, FontWeight::NORMAL));
        let labels: Vec<SharedString> = ticks.iter().map(|tick| format(*tick)).collect();
        let gutter = if self.axis {
            labels
                .iter()
                .map(|label| text_width(label, tick_font.clone(), TICK_SIZE, window))
                .fold(0.0, f32::max)
                .ceil()
                + TICK_GAP
        } else {
            0.0
        };
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _: &mut Context<Hover>| {
            Hover::default()
        });
        let hovered = state
            .read(cx)
            .index
            .filter(|index| *index < self.values.len());
        let height = self.height;
        let slots = self.slots;
        let values = Rc::new(self.values);
        let inset_top = if self.axis { INSET_TOP } else { STROKE };
        let plot_height = (height - inset_top - STROKE).max(1.0);
        let to_y = move |value: f32| {
            let share = ((value - low) / (high - low)).clamp(0.0, 1.0);
            inset_top + plot_height * (1.0 - share)
        };

        let paint_values = values.clone();
        let paint_state = state.clone();
        let grid_ticks = ticks.clone();
        let axis = self.axis;
        let area = self.area;
        let grid = theme.chart_grid;
        let surface = theme.color_surface_subtle;
        let plot = canvas(
            move |bounds, _, _| bounds,
            move |_, bounds, window, cx| {
                let left = bounds.origin.x + px(gutter);
                let width = (bounds.size.width - px(gutter) - px(INSET_RIGHT)).max(px(1.0));
                let top = bounds.origin.y;
                let plot_bounds = Bounds::new(
                    point(left, top + px(inset_top)),
                    size(width, px(plot_height)),
                );
                paint_state.update(cx, |hover, _| hover.plot = Some(plot_bounds));
                if axis {
                    for tick in &grid_ticks {
                        let y = top + px(to_y(*tick)).round();
                        window.paint_quad(fill(
                            Bounds::new(point(left, y), size(width, px(1.0))),
                            grid,
                        ));
                    }
                }
                let len = paint_values.len();
                if len == 0 {
                    return;
                }
                let (left, width) = (f32::from(left), f32::from(width));
                let points: Vec<Point<Pixels>> = paint_values
                    .iter()
                    .enumerate()
                    .map(|(index, value)| {
                        point(
                            px(slot_x(index, len, slots, left, width)),
                            top + px(to_y(*value)),
                        )
                    })
                    .collect();
                if area && len > 1 {
                    let mut shape = PathBuilder::fill();
                    let floor = top + px(to_y(low.max(0.0).min(high)));
                    shape.move_to(point(points[0].x, floor));
                    for at in &points {
                        shape.line_to(*at);
                    }
                    shape.line_to(point(points[len - 1].x, floor));
                    shape.close();
                    if let Ok(path) = shape.build() {
                        window.paint_path(path, fade(color, 0.14));
                    }
                }
                if len == 1 {
                    paint_dot(window, points[0], color, surface);
                } else {
                    let mut line = PathBuilder::stroke(px(STROKE));
                    line.move_to(points[0]);
                    for at in &points[1..] {
                        line.line_to(*at);
                    }
                    if let Ok(path) = line.build() {
                        window.paint_path(path, color);
                    }
                }
                if let Some(index) = paint_state.read(cx).index.filter(|index| *index < len) {
                    let at = points[index];
                    window.paint_quad(fill(
                        Bounds::new(
                            point(at.x, top + px(inset_top)),
                            size(px(1.0), px(plot_height)),
                        ),
                        fade(color, 0.45),
                    ));
                    paint_dot(window, at, color, surface);
                }
            },
        )
        .size_full();

        let move_state = state.clone();
        let move_values = values.clone();
        let mut chart = div()
            .id(self.id)
            .relative()
            .w_full()
            .flex_none()
            .h(px(height))
            .child(plot)
            .on_mouse_move(move |event: &MouseMoveEvent, window, cx| {
                let len = move_values.len();
                let index = move_state
                    .read(cx)
                    .plot
                    .and_then(|plot| nearest(event.position, plot, len, slots));
                if move_state.read(cx).index != index {
                    move_state.update(cx, |hover, _| hover.index = index);
                    window.refresh();
                }
            })
            .on_hover(move |hovering, window, cx| {
                if !*hovering && state.read(cx).index.is_some() {
                    state.update(cx, |hover, _| hover.index = None);
                    window.refresh();
                }
            });
        if axis {
            for (tick, label) in ticks.iter().zip(labels) {
                let y = to_y(*tick) - TICK_SIZE * 0.6;
                chart = chart.child(
                    div()
                        .absolute()
                        .left_0()
                        .top(px(y))
                        .w(px(gutter - TICK_GAP))
                        .flex()
                        .justify_end()
                        .font(tick_font.clone())
                        .text_size(px(TICK_SIZE))
                        .line_height(px(TICK_SIZE * 1.2))
                        .text_color(theme.color_text_muted)
                        .whitespace_nowrap()
                        .child(label),
                );
            }
        }
        if let Some(index) = hovered {
            let value = format(values[index]);
            let text: SharedString = match &self.label {
                Some(label) => format!("{label} {value}").into(),
                None => value,
            };
            chart = chart.child(
                div()
                    .absolute()
                    .top_0()
                    .right(px(INSET_RIGHT))
                    .px(px(6.0))
                    .py(px(1.0))
                    .rounded(px(theme.radius_item))
                    .bg(theme.color_surface_raised)
                    .font(tabular(sans(&theme, FontWeight::MEDIUM)))
                    .text_size(px(11.0))
                    .line_height(px(14.0))
                    .text_color(theme.color_text_primary)
                    .child(text),
            );
        }
        chart
    }
}

fn paint_dot(window: &mut Window, at: Point<Pixels>, color: Rgba, surface: Rgba) {
    let half = px(DOT / 2.0);
    let ring = Bounds::new(point(at.x - half, at.y - half), size(px(DOT), px(DOT)));
    window.paint_quad(
        fill(ring, surface)
            .corner_radii(half)
            .border_widths(px(1.5))
            .border_color(color),
    );
}

/// The value nearest the pointer's x over the plot, or none outside it.
fn nearest(at: Point<Pixels>, plot: Bounds<Pixels>, len: usize, slots: usize) -> Option<usize> {
    if len == 0 || !plot.contains(&at) {
        return None;
    }
    let (left, width) = (f32::from(plot.origin.x), f32::from(plot.size.width));
    let x = f32::from(at.x);
    (0..len).min_by(|a, b| {
        let da = (slot_x(*a, len, slots, left, width) - x).abs();
        let db = (slot_x(*b, len, slots, left, width) - x).abs();
        da.total_cmp(&db)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_read_compactly() {
        assert_eq!(compact_tick(0.0), "0");
        assert_eq!(compact_tick(950.0), "950");
        assert_eq!(compact_tick(1_234.0), "1.2K");
        assert_eq!(compact_tick(3_400_000.0), "3.4M");
        assert_eq!(compact_tick(2_000_000_000.0), "2G");
        assert_eq!(compact_tick(12.5), "12.5");
    }

    #[test]
    fn the_axis_rounds_up_to_a_tick() {
        let (low, high, ticks) = value_axis(&[3.0, 41.0, 17.0], None, 2);
        assert_eq!((low, high), (0.0, 60.0));
        assert_eq!(ticks, [0.0, 20.0, 40.0, 60.0]);
        let (low, high, ticks) = value_axis(&[1.0, 2.0], Some((0.0, 100.0)), 2);
        assert_eq!((low, high, ticks), (0.0, 100.0, vec![0.0, 50.0, 100.0]));
        let (low, high, _) = value_axis(&[], None, 2);
        assert!(high > low, "an empty series still has a range");
        let (low, high, _) = value_axis(&[0.0, 0.0], None, 2);
        assert!(high > low, "a flat series still has a range");
        let (low, _, _) = value_axis(&[-5.0, 5.0], None, 2);
        assert!(low < 0.0);
    }

    #[test]
    fn a_short_series_hugs_the_right_edge() {
        assert_eq!(slot_x(0, 1, 5, 0.0, 100.0), 100.0);
        assert_eq!(slot_x(0, 2, 5, 0.0, 100.0), 75.0);
        assert_eq!(slot_x(4, 5, 5, 0.0, 100.0), 100.0);
        assert_eq!(slot_x(0, 5, 5, 0.0, 100.0), 0.0);
        // No slots asked: the series fills the width.
        assert_eq!(slot_x(0, 3, 0, 0.0, 100.0), 0.0);
        assert_eq!(slot_x(2, 3, 0, 0.0, 100.0), 100.0);
    }

    #[test]
    fn the_hover_picks_the_nearest_point() {
        let plot = Bounds::new(point(px(10.0), px(0.0)), size(px(100.0), px(50.0)));
        assert_eq!(nearest(point(px(12.0), px(10.0)), plot, 3, 0), Some(0));
        assert_eq!(nearest(point(px(61.0), px(10.0)), plot, 3, 0), Some(1));
        assert_eq!(nearest(point(px(109.0), px(10.0)), plot, 3, 0), Some(2));
        assert_eq!(nearest(point(px(5.0), px(10.0)), plot, 3, 0), None);
        assert_eq!(nearest(point(px(50.0), px(10.0)), plot, 0, 0), None);
    }
}
