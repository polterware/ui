//! Source-to-[`Scene`] rendering for xycharts: the Rust side of `xychartRenderer.ts` and its
//! `chartBuilder/` tree.
//!
//! An xychart does not go through the unified layout pipeline: `xychartRenderer.ts` builds its
//! own components and draws them with plain d3 scales. This module ports that path line by
//! line: [`Orchestrator`] reserves space as `orchestrator.ts` does, the [`ChartTitle`], the
//! [`Plot`], the [`ChartLegend`] and the two [`ChartAxis`] components produce the same
//! [`DrawableElem`]s, and the renderer assembles the nested `<g>` tree (`main`, `chart-title`,
//! `plot`, `legend`, `left-axis`, `bottom-axis`, `top-axis`) into scene groups.
//!
//! The scales are d3's: a `scaleBand` with `paddingInner(1)`, `paddingOuter(0)` and
//! `align(0.5)` for a band axis ([`BandScale`]), a `scaleLinear` for a value axis
//! ([`LinearScale`]), and d3-array's `ticks` ([`ticks`], including `tickSpec`'s negative-power
//! branch) for the linear axis labels. The bar and line geometry, the rotation offset, the
//! bar-padding recalculation and the data-label fitting loops follow `barPlot.ts`,
//! `linePlot.ts`, `baseAxis.ts` and `xychartRenderer.ts` operation for operation.
//!
//! Text is measured in the `monospace` family, the rule the fixture harness renders with and
//! that [`FallbackMeasure`](crate::measure::FallbackMeasure) reproduces. `getMaxDimension`
//! measures with `getBBox` (the widest line and the 1.15em ink height), so [`max_dimension`]
//! uses [`TextMeasure::width`] and [`TextMeasure::line_height`], not
//! [`TextMeasure::dimensions`]' rounded two-family result.
//!
//! Where the port stops short of upstream:
//!
//! * Colors and sizes come from the `xyChart` object of the active theme's variables, as
//!   `getChartThemeConfig` merges them. `getChartConfig` is [`Config::xy_chart`].
//!   `chartOrientation` is read from [`Ast::orientation`], which the grammar's header sets and
//!   whose default is vertical, as `setOrientation` leaves it; the configuration field alone
//!   does not switch the drawing.
//! * The SVG DOM is replaced by the scene: each `<text>` becomes a [`TextRun`] at its
//!   `translate(x, y)` (or its `x`/`y` attributes for bar data labels) with its `rotate`
//!   angle, and each `d` becomes a [`Path`]. CSS class names are kept as group classes.
//! * The background rectangle is an ordinary scene rectangle, not [`Scene::background`], so
//!   the painter draws exactly the elements upstream appends.
//! * `getDrawableElem` throws `No Plot to render, please provide a plot with some data` when
//!   there is no plot; [`draw_ast`] reports that as a [`ParseError`] with upstream's message.
//! * A right axis is never drawn: upstream throws `Drawing of right axis is not implemented`,
//!   and the orchestrator only selects `left`, `bottom` and `top`.
//! * A band category with no plotted value is `undefined` upstream and a `None` here; the
//!   fitting loop would divide by zero on its empty label, so such data labels are skipped.
//! * The `axisl-line` class of the left axis line is kept as upstream spells it (a typo in
//!   `baseAxis.ts`); a scene carries it but paints nothing for it.
//! * `plotColorPalette` index selection keeps upstream's `plotIndex % length` expression,
//!   including its first-entry special case.
//! * Text is not wrapped: the renderer only emits single-line labels and titles, which is
//!   what `createFormattedText`'s `getBBox` measurement above reproduces.

use std::collections::HashMap;

use crate::config::{Config, XYChartAxisConfig, XYChartConfig};
use crate::error::ParseError;
use crate::layout::sankey::number::format_number;
use crate::measure::{Font, TextMeasure, TextOptions, shape_text};
use crate::scene::{
    Color, Group, Item, Path, PathShape, Point, Rect, RectShape, Scene, TextAnchor, TextBaseline,
    TextBlock, TextRun,
};

use super::Ast;
use super::ast::{Axis as DataAxis, Orientation, PlotKind};

/// `BAR_WIDTH_TO_TICK_WIDTH_RATIO` in `baseAxis.ts`.
const BAR_WIDTH_TO_TICK_WIDTH_RATIO: f64 = 0.7;
/// `MAX_OUTER_PADDING_PERCENT_FOR_WRT_LABEL` in `baseAxis.ts`.
const MAX_OUTER_PADDING_PERCENT_FOR_WRT_LABEL: f64 = 0.2;
/// `barPaddingPercent` in `barPlot.ts`.
const BAR_PADDING_PERCENT: f64 = 0.05;
/// `labelOffset` in `linePlot.ts`.
const POINT_LABEL_OFFSET: f64 = 10.0;
/// `fontSize` in `linePlot.ts`.
const POINT_LABEL_FONT_SIZE: f64 = 12.0;
/// `LEGEND_MARKER_TO_FONT_RATIO` in `legend.ts`.
const LEGEND_MARKER_TO_FONT_RATIO: f64 = 0.75;
/// `LEGEND_ITEM_SPACING_TO_FONT_RATIO` in `legend.ts`.
const LEGEND_ITEM_SPACING_TO_FONT_RATIO: f64 = 0.5;
/// `LEGEND_MARKER_SPACING_TO_FONT_RATIO` in `legend.ts`.
const LEGEND_MARKER_SPACING_TO_FONT_RATIO: f64 = 0.35;
/// The xychart renderer measures every label in the harness's monospace family.
const FONT_FAMILY: &str = "monospace";

/// Renders an xychart body (the text [`super::parse`] accepts) to a [`Scene`].
///
/// `config` is the diagram configuration after the source's frontmatter and directives have
/// been applied, as `getConfig()` returns it to `draw`.
pub fn draw(body: &str, config: &Config, measure: &dyn TextMeasure) -> Result<Scene, ParseError> {
    let ast = super::parse(body)?;
    draw_ast(&ast, config, measure)
}

/// Renders a body with the crate's monospace fallback measurement.
pub fn render(body: &str, config: &Config) -> Result<Scene, ParseError> {
    draw(body, config, &crate::measure::FallbackMeasure)
}

/// Renders an already parsed xychart database.
pub fn draw_ast(
    ast: &Ast,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<Scene, ParseError> {
    if ast.plots.is_empty() {
        // `getDrawableElem`'s throw.
        return Err(ParseError::new(
            "No Plot to render, please provide a plot with some data",
            None,
        ));
    }

    let theme = ChartTheme::from_theme(&config.theme_variables);
    let chart = Chart {
        config: &config.xy_chart,
        ast,
        theme: &theme,
        measure,
        orientation: ast.orientation,
    };
    let shapes = Orchestrator::new(&chart).drawables(&chart);
    Ok(build_scene(&chart, shapes))
}

/// The theme values `getChartThemeConfig` hands the components.
struct ChartTheme {
    background: String,
    title: String,
    data_label: String,
    legend_text: String,
    x_axis: AxisTheme,
    y_axis: AxisTheme,
    plot_color_palette: Vec<String>,
}

/// `XYChartAxisThemeConfig` for one axis.
#[derive(Clone)]
struct AxisTheme {
    title: String,
    label: String,
    tick: String,
    axis_line: String,
}

impl ChartTheme {
    /// `getChartThemeConfig`: the active theme's `xyChart` object.
    fn from_theme(theme: &serde_json::Map<String, serde_json::Value>) -> Self {
        let xy = theme
            .get("xyChart")
            .and_then(serde_json::Value::as_object)
            .cloned()
            .unwrap_or_default();
        let string = |key: &str| {
            xy.get(key)
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let axis = |prefix: &str| AxisTheme {
            title: string(&format!("{prefix}TitleColor")),
            label: string(&format!("{prefix}LabelColor")),
            tick: string(&format!("{prefix}TickColor")),
            axis_line: string(&format!("{prefix}LineColor")),
        };
        let plot_color_palette = string("plotColorPalette")
            .split(',')
            .map(|color| color.trim().to_string())
            .collect();
        Self {
            background: string("backgroundColor"),
            title: string("titleColor"),
            data_label: string("dataLabelColor"),
            legend_text: string("legendTextColor"),
            x_axis: axis("xAxis"),
            y_axis: axis("yAxis"),
            plot_color_palette,
        }
    }

    /// `getPlotColorFromPalette`.
    fn plot_color(&self, plot_index: usize) -> String {
        if self.plot_color_palette.is_empty() {
            return String::new();
        }
        let index = if plot_index == 0 {
            0
        } else {
            plot_index % self.plot_color_palette.len()
        };
        self.plot_color_palette[index].clone()
    }
}

/// What every component can ask about the chart.
struct Chart<'a> {
    config: &'a XYChartConfig,
    ast: &'a Ast,
    theme: &'a ChartTheme,
    measure: &'a dyn TextMeasure,
    orientation: Orientation,
}

impl Chart<'_> {
    fn is_horizontal(&self) -> bool {
        self.orientation == Orientation::Horizontal
    }

    /// `getMaxDimension(texts, fontSize)` from `textDimensionCalculator.ts`.
    fn max_dimension(&self, texts: &[String], font_size: f64) -> Dimension {
        let font = Font::new(FONT_FAMILY, font_size);
        let mut dimension = Dimension::default();
        for text in texts {
            if text.is_empty() {
                continue;
            }
            dimension.width = dimension.width.max(self.measure.width(text, &font));
            dimension.height = dimension.height.max(self.measure.line_height(&font));
        }
        dimension
    }
}

/// `Dimension` in `interfaces.ts`.
#[derive(Clone, Copy, Debug, Default)]
struct Dimension {
    width: f64,
    height: f64,
}

/// `Point` in `interfaces.ts`, kept separate from [`scene::Point`] for the layout code.
#[derive(Clone, Copy, Debug, Default)]
struct Position {
    x: f64,
    y: f64,
}

/// `RectElem`.
struct RectElem {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    fill: String,
    stroke_width: f64,
    stroke_fill: Option<String>,
}

/// `TextElem`'s horizontal anchor.
#[derive(Clone, Copy, PartialEq, Eq)]
enum HorizontalPosition {
    Left,
    Center,
    Right,
}

/// `TextElem`'s vertical anchor.
#[derive(Clone, Copy, PartialEq, Eq)]
enum VerticalPosition {
    Top,
    Middle,
    Hanging,
    /// The bar data-label `auto` baseline, which SVG resolves to alphabetic.
    Auto,
}

/// `TextElem`.
struct TextElem {
    x: f64,
    y: f64,
    text: String,
    fill: String,
    vertical_pos: VerticalPosition,
    horizontal_pos: HorizontalPosition,
    font_size: f64,
    rotation: f64,
}

/// `PathElem`.
struct PathElem {
    path: Path,
    fill: Option<String>,
    stroke_width: f64,
    stroke_fill: String,
}

/// One `DrawableElem`'s data.
enum DrawableData {
    Rect(Vec<RectElem>),
    Text(Vec<TextElem>),
    Path(Vec<PathElem>),
}

/// `DrawableElem`.
struct DrawableElem {
    group_texts: Vec<String>,
    data: DrawableData,
}

impl DrawableElem {
    fn is_empty(&self) -> bool {
        match &self.data {
            DrawableData::Rect(data) => data.is_empty(),
            DrawableData::Text(data) => data.is_empty(),
            DrawableData::Path(data) => data.is_empty(),
        }
    }
}

// ---------------------------------------------------------------------------
// d3 scales
// ---------------------------------------------------------------------------

/// d3's `scaleBand` restricted to the xychart's `paddingInner(1)`, `paddingOuter(0)`,
/// `align(0.5)` and no rounding.
struct BandScale {
    /// The ordinal domain without duplicates, which is what d3's `InternMap` keeps.
    domain: Vec<String>,
    start: f64,
    stop: f64,
}

impl BandScale {
    fn new(categories: &[String], range: (f64, f64)) -> Self {
        let mut domain = Vec::new();
        for category in categories {
            if !domain.contains(category) {
                domain.push(category.clone());
            }
        }
        let (start, stop) = if range.1 < range.0 {
            (range.1, range.0)
        } else {
            (range.0, range.1)
        };
        Self {
            domain,
            start,
            stop,
        }
    }

    /// `scale(value)`, falling back to the range's start for an unknown category.
    fn value(&self, category: &str, range: (f64, f64)) -> f64 {
        let Some(index) = self.domain.iter().position(|entry| entry == category) else {
            return range.0;
        };
        let n = self.domain.len() as f64;
        let step = (self.stop - self.start) / (n - 1.0).max(1.0);
        let start = self.start + (self.stop - self.start - step * (n - 1.0)) * 0.5;
        let mut values: Vec<f64> = (0..self.domain.len())
            .map(|i| start + step * i as f64)
            .collect();
        if range.1 < range.0 {
            values.reverse();
        }
        values.remove(index)
    }
}

/// JavaScript's `Math.round`: half values round towards positive infinity, unlike Rust's
/// `f64::round`.
fn js_round(value: f64) -> f64 {
    (value + 0.5).floor()
}

/// `ticks.js`'s `tickSpec`, with `count` fixed by `scaleLinear`'s default of ten.
fn tick_spec(start: f64, stop: f64, count: f64) -> (f64, f64, f64) {
    let step = (stop - start) / count.max(0.0);
    let power = step.log10().floor();
    let error = step / 10f64.powf(power);
    let factor = if error >= 50f64.sqrt() {
        10.0
    } else if error >= 10f64.sqrt() {
        5.0
    } else if error >= 2f64.sqrt() {
        2.0
    } else {
        1.0
    };
    let (mut i1, mut i2, inc);
    if power < 0.0 {
        let mut scale = 10f64.powf(-power) / factor;
        i1 = js_round(start * scale);
        i2 = js_round(stop * scale);
        if i1 / scale < start {
            i1 += 1.0;
        }
        if i2 / scale > stop {
            i2 -= 1.0;
        }
        scale = -scale;
        inc = scale;
    } else {
        let scale = 10f64.powf(power) * factor;
        i1 = js_round(start / scale);
        i2 = js_round(stop / scale);
        if i1 * scale < start {
            i1 += 1.0;
        }
        if i2 * scale > stop {
            i2 -= 1.0;
        }
        inc = scale;
    }
    if i2 < i1 && (0.5..2.0).contains(&count) {
        return tick_spec(start, stop, count * 2.0);
    }
    (i1, i2, inc)
}

/// d3-array's `ticks(start, stop, count)`.
fn ticks(start: f64, stop: f64, count: f64) -> Vec<f64> {
    if count.is_nan() || count <= 0.0 {
        return Vec::new();
    }
    if start == stop {
        return vec![start];
    }
    let reverse = stop < start;
    let (i1, i2, inc) = if reverse {
        tick_spec(stop, start, count)
    } else {
        tick_spec(start, stop, count)
    };
    if i2.is_nan() || i1.is_nan() || i2 < i1 {
        return Vec::new();
    }
    let n = (i2 - i1 + 1.0) as i64;
    let mut values = Vec::with_capacity(n.max(0) as usize);
    if reverse {
        if inc < 0.0 {
            for i in 0..n {
                values.push((i2 - i as f64) / -inc);
            }
        } else {
            for i in 0..n {
                values.push((i2 - i as f64) * inc);
            }
        }
    } else if inc < 0.0 {
        for i in 0..n {
            values.push((i1 + i as f64) / -inc);
        }
    } else {
        for i in 0..n {
            values.push((i1 + i as f64) * inc);
        }
    }
    values
}

/// d3's `scaleLinear`: a normalized linear interpolation, including the constant-domain
/// `0.5` branch.
struct LinearScale {
    domain: (f64, f64),
    range: (f64, f64),
}

impl LinearScale {
    fn new(domain: (f64, f64), range: (f64, f64)) -> Self {
        Self { domain, range }
    }

    fn value(&self, value: f64) -> f64 {
        let (a, b) = (self.domain.0, self.domain.1);
        let normalized = if b - a != 0.0 {
            (value - a) / (b - a)
        } else {
            0.5
        };
        self.range.0 + (self.range.1 - self.range.0) * normalized
    }

    /// `scale.ticks()`: the domain as the scale currently holds it.
    fn ticks(&self) -> Vec<f64> {
        ticks(self.domain.0, self.domain.1, 10.0)
    }
}

// ---------------------------------------------------------------------------
// Axes
// ---------------------------------------------------------------------------

/// `AxisPosition` in `components/axis/index.ts`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum AxisPosition {
    Left,
    Right,
    Top,
    Bottom,
}

/// `AxisDataType`.
enum AxisData {
    Band(Vec<String>),
    Linear { min: f64, max: f64 },
}

/// `BaseAxis`: the shared space calculation, scale reads and `<g>` elements.
struct ChartAxis {
    axis_config: XYChartAxisConfig,
    theme: AxisTheme,
    title: String,
    data: AxisData,
    position: AxisPosition,
    range: (f64, f64),
    bounding: Rect,
    show_title: bool,
    show_label: bool,
    show_tick: bool,
    show_axis_line: bool,
    outer_padding: f64,
    title_text_height: f64,
    label_text_height: f64,
    normalized_label_rotation_in_rad: f64,
}

impl ChartAxis {
    fn new(
        data: AxisData,
        title: String,
        axis_config: XYChartAxisConfig,
        theme: AxisTheme,
    ) -> Self {
        let normalized_label_rotation_in_rad =
            if (-90.0..=90.0).contains(&axis_config.label_rotation) {
                axis_config.label_rotation.to_radians()
            } else {
                0.0
            };
        Self {
            axis_config,
            theme,
            title,
            data,
            position: AxisPosition::Left,
            range: (0.0, 10.0),
            bounding: Rect::ZERO,
            show_title: false,
            show_label: false,
            show_tick: false,
            show_axis_line: false,
            outer_padding: 0.0,
            title_text_height: 0.0,
            label_text_height: 0.0,
            normalized_label_rotation_in_rad,
        }
    }

    fn set_range(&mut self, range: (f64, f64)) {
        self.range = range;
        if self.position == AxisPosition::Left || self.position == AxisPosition::Right {
            self.bounding.height = range.1 - range.0;
        } else {
            self.bounding.width = range.1 - range.0;
        }
    }

    fn get_range(&self) -> (f64, f64) {
        (
            self.range.0 + self.outer_padding,
            self.range.1 - self.outer_padding,
        )
    }

    fn set_axis_position(&mut self, position: AxisPosition) {
        self.position = position;
        self.set_range(self.range);
    }

    fn set_bounding_box_xy(&mut self, position: Position) {
        self.bounding.x = position.x;
        self.bounding.y = position.y;
    }

    fn tick_values(&self) -> Vec<String> {
        match &self.data {
            AxisData::Band(categories) => categories.clone(),
            AxisData::Linear { min, max } => {
                let domain = if self.position == AxisPosition::Left {
                    (*max, *min)
                } else {
                    (*min, *max)
                };
                LinearScale::new(domain, self.get_range())
                    .ticks()
                    .into_iter()
                    .map(format_number)
                    .collect()
            }
        }
    }

    /// `getTickDistance`.
    fn get_tick_distance(&self) -> f64 {
        let range = self.get_range();
        (range.0 - range.1).abs() / self.tick_values().len() as f64
    }

    /// `getAxisOuterPadding`.
    fn get_axis_outer_padding(&self) -> f64 {
        self.outer_padding
    }

    /// `getLabelDimension`.
    fn label_dimension(&self, chart: &Chart) -> Dimension {
        chart.max_dimension(&self.tick_values(), self.axis_config.label_font_size)
    }

    /// `recalculateOuterPaddingToDrawBar`.
    fn recalculate_outer_padding_to_draw_bar(&mut self) {
        let distance = self.get_tick_distance();
        let width = BAR_WIDTH_TO_TICK_WIDTH_RATIO * distance;
        if width > self.outer_padding * 2.0 {
            self.outer_padding = (width / 2.0).floor();
        }
    }

    /// `setRange` plus `recalculateScale`: the scale is rebuilt from the current range.
    fn band_scale(&self) -> BandScale {
        BandScale::new(
            match &self.data {
                AxisData::Band(categories) => categories,
                AxisData::Linear { .. } => &[],
            },
            self.get_range(),
        )
    }

    fn linear_scale(&self) -> LinearScale {
        match &self.data {
            AxisData::Linear { min, max } => {
                let domain = if self.position == AxisPosition::Left {
                    (*max, *min)
                } else {
                    (*min, *max)
                };
                LinearScale::new(domain, self.get_range())
            }
            AxisData::Band(_) => LinearScale::new((0.0, 1.0), self.get_range()),
        }
    }

    /// `getScaleValue` of a band or linear axis, from a plotted label.
    fn scale_value_label(&self, label: &str) -> f64 {
        match &self.data {
            AxisData::Band(_) => self.band_scale().value(label, self.get_range()),
            AxisData::Linear { .. } => {
                let value = label.parse::<f64>().unwrap_or(f64::NAN);
                self.linear_scale().value(value)
            }
        }
    }

    /// `getScaleValue` of a linear axis, from a plotted value.
    fn scale_value_number(&self, value: f64) -> f64 {
        match &self.data {
            AxisData::Linear { .. } => self.linear_scale().value(value),
            AxisData::Band(_) => value,
        }
    }

    fn calculate_space(&mut self, chart: &Chart, available: Dimension) -> Dimension {
        if self.position == AxisPosition::Left || self.position == AxisPosition::Right {
            self.calculate_space_if_drawn_vertically(chart, available);
        } else {
            self.calculate_space_if_drawn_horizontally(chart, available);
        }
        Dimension {
            width: self.bounding.width,
            height: self.bounding.height,
        }
    }

    fn calculate_space_if_drawn_horizontally(&mut self, chart: &Chart, available: Dimension) {
        let mut available_height = available.height;
        if self.axis_config.show_axis_line && available_height > self.axis_config.axis_line_width {
            available_height -= self.axis_config.axis_line_width;
            self.show_axis_line = true;
        }
        if self.axis_config.show_label {
            let space_required = self.label_dimension(chart);
            let max_padding = MAX_OUTER_PADDING_PERCENT_FOR_WRT_LABEL * available.width;
            self.outer_padding = (space_required.width / 2.0).min(max_padding);

            let mut height_required = space_required.height;
            if self.position == AxisPosition::Bottom && self.normalized_label_rotation_in_rad != 0.0
            {
                let (sin, cos) = self.normalized_label_rotation_in_rad.sin_cos();
                height_required = height_required
                    .max((sin * space_required.width).abs() + (cos * space_required.height).abs());
            }
            height_required += self.axis_config.label_padding * 2.0;

            self.label_text_height = space_required.height;
            if height_required <= available_height {
                available_height -= height_required;
                self.show_label = true;
            }
        }
        if self.axis_config.show_tick && available_height >= self.axis_config.tick_length {
            self.show_tick = true;
            available_height -= self.axis_config.tick_length;
        }
        if self.axis_config.show_title && !self.title.is_empty() {
            let space_required = chart.max_dimension(
                std::slice::from_ref(&self.title),
                self.axis_config.title_font_size,
            );
            let height_required = space_required.height + self.axis_config.title_padding * 2.0;
            self.title_text_height = space_required.height;
            if height_required <= available_height {
                available_height -= height_required;
                self.show_title = true;
            }
        }
        self.bounding.width = available.width;
        self.bounding.height = available.height - available_height;
    }

    fn calculate_space_if_drawn_vertically(&mut self, chart: &Chart, available: Dimension) {
        let mut available_width = available.width;
        if self.axis_config.show_axis_line && available_width > self.axis_config.axis_line_width {
            available_width -= self.axis_config.axis_line_width;
            self.show_axis_line = true;
        }
        if self.axis_config.show_label {
            let space_required = self.label_dimension(chart);
            let max_padding = MAX_OUTER_PADDING_PERCENT_FOR_WRT_LABEL * available.height;
            self.outer_padding = (space_required.height / 2.0).min(max_padding);
            let width_required = space_required.width + self.axis_config.label_padding * 2.0;
            if width_required <= available_width {
                available_width -= width_required;
                self.show_label = true;
            }
        }
        if self.axis_config.show_tick && available_width >= self.axis_config.tick_length {
            self.show_tick = true;
            available_width -= self.axis_config.tick_length;
        }
        if self.axis_config.show_title && !self.title.is_empty() {
            let space_required = chart.max_dimension(
                std::slice::from_ref(&self.title),
                self.axis_config.title_font_size,
            );
            let width_required = space_required.height + self.axis_config.title_padding * 2.0;
            self.title_text_height = space_required.height;
            if width_required <= available_width {
                available_width -= width_required;
                self.show_title = true;
            }
        }
        self.bounding.width = available.width - available_width;
        self.bounding.height = available.height;
    }

    /// `calculateOffsetByRotation`.
    fn calculate_offset_by_rotation(&self, chart: &Chart, reference: DimensionReference) -> f64 {
        let rotation = self.normalized_label_rotation_in_rad;
        if rotation == 0.0 {
            return 0.0;
        }
        let dimension = self.label_dimension(chart);
        let value = match reference {
            DimensionReference::Width => dimension.width,
            DimensionReference::Height => dimension.height,
        };
        (rotation.sin() * value) / 2.0
    }

    fn drawables(&self, chart: &Chart) -> Vec<DrawableElem> {
        match self.position {
            AxisPosition::Left => self.drawables_for_left_axis(),
            AxisPosition::Bottom => self.drawables_for_bottom_axis(chart),
            AxisPosition::Top => self.drawables_for_top_axis(),
            AxisPosition::Right => Vec::new(),
        }
    }

    fn drawables_for_left_axis(&self) -> Vec<DrawableElem> {
        let mut elements = Vec::new();
        if self.show_axis_line {
            let x = self.bounding.x + self.bounding.width - self.axis_config.axis_line_width / 2.0;
            let mut path = Path::new();
            path.move_to(Point::new(x, self.bounding.y));
            path.line_to(Point::new(x, self.bounding.y + self.bounding.height));
            elements.push(path_drawable(
                vec!["left-axis".into(), "axisl-line".into()],
                PathElem {
                    path,
                    fill: None,
                    stroke_width: self.axis_config.axis_line_width,
                    stroke_fill: self.theme.axis_line.clone(),
                },
            ));
        }
        if self.show_label {
            let x = self.bounding.x + self.bounding.width
                - self.axis_config.label_padding
                - if self.show_tick {
                    self.axis_config.tick_length
                } else {
                    0.0
                }
                - if self.show_axis_line {
                    self.axis_config.axis_line_width
                } else {
                    0.0
                };
            let data = self
                .tick_values()
                .into_iter()
                .map(|tick| TextElem {
                    x,
                    y: self.scale_value_label(&tick),
                    text: tick,
                    fill: self.theme.label.clone(),
                    vertical_pos: VerticalPosition::Middle,
                    horizontal_pos: HorizontalPosition::Right,
                    font_size: self.axis_config.label_font_size,
                    rotation: 0.0,
                })
                .collect();
            elements.push(text_drawable(
                vec!["left-axis".into(), "label".into()],
                data,
            ));
        }
        if self.show_tick {
            let x = self.bounding.x + self.bounding.width
                - if self.show_axis_line {
                    self.axis_config.axis_line_width
                } else {
                    0.0
                };
            let data = self
                .tick_values()
                .into_iter()
                .map(|tick| {
                    let y = self.scale_value_label(&tick);
                    let mut path = Path::new();
                    path.move_to(Point::new(x, y));
                    path.line_to(Point::new(x - self.axis_config.tick_length, y));
                    PathElem {
                        path,
                        fill: None,
                        stroke_width: self.axis_config.tick_width,
                        stroke_fill: self.theme.tick.clone(),
                    }
                })
                .collect();
            elements.push(paths_drawable(
                vec!["left-axis".into(), "ticks".into()],
                data,
            ));
        }
        if self.show_title {
            elements.push(text_drawable(
                vec!["left-axis".into(), "title".into()],
                vec![TextElem {
                    x: self.bounding.x + self.axis_config.title_padding,
                    y: self.bounding.y + self.bounding.height / 2.0,
                    text: self.title.clone(),
                    fill: self.theme.title.clone(),
                    font_size: self.axis_config.title_font_size,
                    rotation: 270.0,
                    vertical_pos: VerticalPosition::Top,
                    horizontal_pos: HorizontalPosition::Center,
                }],
            ));
        }
        elements
    }

    fn drawables_for_bottom_axis(&self, chart: &Chart) -> Vec<DrawableElem> {
        let mut elements = Vec::new();
        if self.show_axis_line {
            let y = self.bounding.y + self.axis_config.axis_line_width / 2.0;
            let mut path = Path::new();
            path.move_to(Point::new(self.bounding.x, y));
            path.line_to(Point::new(self.bounding.x + self.bounding.width, y));
            elements.push(path_drawable(
                vec!["bottom-axis".into(), "axis-line".into()],
                PathElem {
                    path,
                    fill: None,
                    stroke_width: self.axis_config.axis_line_width,
                    stroke_fill: self.theme.axis_line.clone(),
                },
            ));
        }
        if self.show_label {
            let data = self
                .tick_values()
                .into_iter()
                .map(|tick| {
                    let y = self.bounding.y
                        + self.axis_config.label_padding
                        + if self.show_tick {
                            self.axis_config.tick_length
                        } else {
                            0.0
                        }
                        + if self.show_axis_line {
                            self.axis_config.axis_line_width
                        } else {
                            0.0
                        }
                        + self
                            .calculate_offset_by_rotation(chart, DimensionReference::Width)
                            .abs();
                    TextElem {
                        x: self.scale_value_label(&tick)
                            + self.calculate_offset_by_rotation(chart, DimensionReference::Height),
                        y,
                        text: tick,
                        fill: self.theme.label.clone(),
                        font_size: self.axis_config.label_font_size,
                        rotation: self.normalized_label_rotation_in_rad.to_degrees(),
                        vertical_pos: VerticalPosition::Top,
                        horizontal_pos: HorizontalPosition::Center,
                    }
                })
                .collect();
            elements.push(text_drawable(
                vec!["bottom-axis".into(), "label".into()],
                data,
            ));
        }
        if self.show_tick {
            let y = self.bounding.y
                + if self.show_axis_line {
                    self.axis_config.axis_line_width
                } else {
                    0.0
                };
            let data = self
                .tick_values()
                .into_iter()
                .map(|tick| {
                    let x = self.scale_value_label(&tick);
                    let mut path = Path::new();
                    path.move_to(Point::new(x, y));
                    path.line_to(Point::new(x, y + self.axis_config.tick_length));
                    PathElem {
                        path,
                        fill: None,
                        stroke_width: self.axis_config.tick_width,
                        stroke_fill: self.theme.tick.clone(),
                    }
                })
                .collect();
            elements.push(paths_drawable(
                vec!["bottom-axis".into(), "ticks".into()],
                data,
            ));
        }
        if self.show_title {
            elements.push(text_drawable(
                vec!["bottom-axis".into(), "title".into()],
                vec![TextElem {
                    x: self.range.0 + (self.range.1 - self.range.0) / 2.0,
                    y: self.bounding.y + self.bounding.height
                        - self.axis_config.title_padding
                        - self.title_text_height,
                    text: self.title.clone(),
                    fill: self.theme.title.clone(),
                    font_size: self.axis_config.title_font_size,
                    rotation: 0.0,
                    vertical_pos: VerticalPosition::Top,
                    horizontal_pos: HorizontalPosition::Center,
                }],
            ));
        }
        elements
    }

    fn drawables_for_top_axis(&self) -> Vec<DrawableElem> {
        let mut elements = Vec::new();
        if self.show_axis_line {
            let y = self.bounding.y + self.bounding.height - self.axis_config.axis_line_width / 2.0;
            let mut path = Path::new();
            path.move_to(Point::new(self.bounding.x, y));
            path.line_to(Point::new(self.bounding.x + self.bounding.width, y));
            elements.push(path_drawable(
                vec!["top-axis".into(), "axis-line".into()],
                PathElem {
                    path,
                    fill: None,
                    stroke_width: self.axis_config.axis_line_width,
                    stroke_fill: self.theme.axis_line.clone(),
                },
            ));
        }
        if self.show_label {
            let data = self
                .tick_values()
                .into_iter()
                .map(|tick| TextElem {
                    x: self.scale_value_label(&tick),
                    y: self.bounding.y
                        + if self.show_title {
                            self.title_text_height + self.axis_config.title_padding * 2.0
                        } else {
                            0.0
                        }
                        + self.axis_config.label_padding,
                    text: tick,
                    fill: self.theme.label.clone(),
                    font_size: self.axis_config.label_font_size,
                    rotation: 0.0,
                    vertical_pos: VerticalPosition::Top,
                    horizontal_pos: HorizontalPosition::Center,
                })
                .collect();
            elements.push(text_drawable(vec!["top-axis".into(), "label".into()], data));
        }
        if self.show_tick {
            let y = self.bounding.y;
            let data = self
                .tick_values()
                .into_iter()
                .map(|tick| {
                    let x = self.scale_value_label(&tick);
                    let mut path = Path::new();
                    path.move_to(Point::new(
                        x,
                        y + self.bounding.height
                            - if self.show_axis_line {
                                self.axis_config.axis_line_width
                            } else {
                                0.0
                            },
                    ));
                    path.line_to(Point::new(
                        x,
                        y + self.bounding.height
                            - self.axis_config.tick_length
                            - if self.show_axis_line {
                                self.axis_config.axis_line_width
                            } else {
                                0.0
                            },
                    ));
                    PathElem {
                        path,
                        fill: None,
                        stroke_width: self.axis_config.tick_width,
                        stroke_fill: self.theme.tick.clone(),
                    }
                })
                .collect();
            elements.push(paths_drawable(
                vec!["top-axis".into(), "ticks".into()],
                data,
            ));
        }
        if self.show_title {
            elements.push(text_drawable(
                vec!["top-axis".into(), "title".into()],
                vec![TextElem {
                    x: self.bounding.x + self.bounding.width / 2.0,
                    y: self.bounding.y + self.axis_config.title_padding,
                    text: self.title.clone(),
                    fill: self.theme.title.clone(),
                    font_size: self.axis_config.title_font_size,
                    rotation: 0.0,
                    vertical_pos: VerticalPosition::Top,
                    horizontal_pos: HorizontalPosition::Center,
                }],
            ));
        }
        elements
    }
}

/// Which dimension `calculateOffsetByRotation` reads.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DimensionReference {
    Width,
    Height,
}

// ---------------------------------------------------------------------------
// Chart title
// ---------------------------------------------------------------------------

/// `ChartTitle` from `chartTitle.ts`.
struct ChartTitle {
    bounding: Rect,
    show: bool,
    title_width: f64,
}

impl ChartTitle {
    fn new() -> Self {
        Self {
            bounding: Rect::ZERO,
            show: false,
            title_width: 0.0,
        }
    }

    fn calculate_space(&mut self, chart: &Chart, available: Dimension) -> Dimension {
        let title_dimension = chart.max_dimension(
            std::slice::from_ref(&chart.ast.diagram_title),
            chart.config.title_font_size,
        );
        let width_required = title_dimension.width.max(available.width);
        let height_required = title_dimension.height + 2.0 * chart.config.title_padding;
        if height_required <= available.height
            && chart.config.show_title
            && !chart.ast.diagram_title.is_empty()
        {
            self.bounding.width = width_required;
            self.bounding.height = height_required;
            self.title_width = title_dimension.width;
            self.show = true;
        }
        Dimension {
            width: self.bounding.width,
            height: self.bounding.height,
        }
    }

    /// Centre the title over `[start, end]`, shifted as needed to keep it inside the chart.
    fn align_to(&mut self, chart: &Chart, start: f64, end: f64) {
        let half_width = self.title_width.min(chart.config.width) / 2.0;
        let center = ((start + end) / 2.0)
            .max(half_width)
            .min(chart.config.width - half_width);
        self.bounding.x = center - half_width;
        self.bounding.width = half_width * 2.0;
    }

    fn drawables(&self, chart: &Chart) -> Vec<DrawableElem> {
        if !self.show {
            return Vec::new();
        }
        vec![text_drawable(
            vec!["chart-title".into()],
            vec![TextElem {
                x: self.bounding.x + self.bounding.width / 2.0,
                y: self.bounding.y + self.bounding.height / 2.0,
                text: chart.ast.diagram_title.clone(),
                fill: chart.theme.title.clone(),
                font_size: chart.config.title_font_size,
                rotation: 0.0,
                vertical_pos: VerticalPosition::Middle,
                horizontal_pos: HorizontalPosition::Center,
            }],
        )]
    }
}

// ---------------------------------------------------------------------------
// Legend
// ---------------------------------------------------------------------------

/// `ChartLegend` from `legend.ts`.
struct ChartLegend {
    bounding: Rect,
    visible_plots: Vec<usize>,
}

impl ChartLegend {
    fn new() -> Self {
        Self {
            bounding: Rect::ZERO,
            visible_plots: Vec::new(),
        }
    }

    fn set_bounding_box_xy(&mut self, position: Position) {
        self.bounding.x = position.x;
        self.bounding.y = position.y;
    }

    fn layout(font_size: f64) -> (f64, f64, f64) {
        (
            font_size * LEGEND_MARKER_TO_FONT_RATIO,
            font_size * LEGEND_MARKER_SPACING_TO_FONT_RATIO,
            font_size * LEGEND_ITEM_SPACING_TO_FONT_RATIO,
        )
    }

    fn calculate_space(&mut self, chart: &Chart, available: Dimension) -> Dimension {
        self.visible_plots = if chart.config.show_legend {
            chart
                .ast
                .plots
                .iter()
                .enumerate()
                .filter(|(_, plot)| !plot.title.is_empty())
                .map(|(index, _)| index)
                .collect()
        } else {
            Vec::new()
        };

        if self.visible_plots.is_empty() {
            self.bounding.width = 0.0;
            self.bounding.height = 0.0;
            return Dimension::default();
        }

        let font_size = chart.config.legend_font_size;
        let (marker_size, marker_spacing, item_spacing) = Self::layout(font_size);
        let titles: Vec<String> = self
            .visible_plots
            .iter()
            .map(|index| chart.ast.plots[*index].title.clone())
            .collect();
        let text_dimension = chart.max_dimension(&titles, font_size);
        let count = self.visible_plots.len() as f64;
        let width_required =
            chart.config.legend_padding * 2.0 + marker_size + marker_spacing + text_dimension.width;
        let height_required =
            chart.config.legend_padding * 2.0 + count * font_size + (count - 1.0) * item_spacing;

        if width_required <= available.width && height_required <= available.height {
            self.bounding.width = width_required;
            self.bounding.height = height_required;
        } else {
            self.visible_plots.clear();
            self.bounding.width = 0.0;
            self.bounding.height = 0.0;
        }
        Dimension {
            width: self.bounding.width,
            height: self.bounding.height,
        }
    }

    fn drawables(&self, chart: &Chart) -> Vec<DrawableElem> {
        if self.visible_plots.is_empty() {
            return Vec::new();
        }
        let font_size = chart.config.legend_font_size;
        let (marker_size, marker_spacing, item_spacing) = Self::layout(font_size);
        let row_height = font_size + item_spacing;
        let start_x = self.bounding.x + chart.config.legend_padding;
        let start_y = self.bounding.y + chart.config.legend_padding;

        let mut bar_markers = Vec::new();
        let mut line_markers = Vec::new();
        let mut labels = Vec::new();
        for (index, plot_index) in self.visible_plots.iter().enumerate() {
            let plot = &chart.ast.plots[*plot_index];
            let color = chart.theme.plot_color(*plot_index);
            if plot.kind == PlotKind::Bar {
                bar_markers.push(RectElem {
                    x: start_x,
                    y: start_y + index as f64 * row_height,
                    width: marker_size,
                    height: marker_size,
                    fill: color.clone(),
                    stroke_fill: Some(color),
                    stroke_width: 0.0,
                });
            } else {
                let marker_y = start_y + index as f64 * row_height + marker_size / 2.0;
                let mut path = Path::new();
                path.move_to(Point::new(start_x, marker_y));
                path.line_to(Point::new(start_x + marker_size, marker_y));
                line_markers.push(PathElem {
                    path,
                    fill: None,
                    stroke_width: 2.0,
                    stroke_fill: color,
                });
            }
            labels.push(TextElem {
                text: plot.title.clone(),
                x: start_x + marker_size + marker_spacing,
                y: start_y + index as f64 * row_height + marker_size / 2.0,
                fill: chart.theme.legend_text.clone(),
                font_size,
                rotation: 0.0,
                vertical_pos: VerticalPosition::Middle,
                horizontal_pos: HorizontalPosition::Left,
            });
        }

        vec![
            rect_drawable(vec!["legend".into(), "markers".into()], bar_markers),
            paths_drawable(vec!["legend".into(), "markers".into()], line_markers),
            text_drawable(vec!["legend".into(), "label".into()], labels),
        ]
    }
}

// ---------------------------------------------------------------------------
// Plots
// ---------------------------------------------------------------------------

/// `BasePlot`: the plot area and its line/bar children.
struct Plot {
    bounding: Rect,
}

impl Plot {
    fn new() -> Self {
        Self {
            bounding: Rect::ZERO,
        }
    }

    fn calculate_space(&mut self, available: Dimension) -> Dimension {
        self.bounding.width = available.width;
        self.bounding.height = available.height;
        Dimension {
            width: self.bounding.width,
            height: self.bounding.height,
        }
    }

    fn set_bounding_box_xy(&mut self, position: Position) {
        self.bounding.x = position.x;
        self.bounding.y = position.y;
    }

    fn drawables(
        &self,
        chart: &Chart,
        x_axis: &ChartAxis,
        y_axis: &ChartAxis,
    ) -> Vec<DrawableElem> {
        let mut elements = Vec::new();
        for (index, plot) in chart.ast.plots.iter().enumerate() {
            let final_data: Vec<(f64, f64)> = plot
                .data
                .iter()
                .map(|point| {
                    let value = point.value.unwrap_or(f64::NAN);
                    (
                        x_axis.scale_value_label(&point.label),
                        y_axis.scale_value_number(value),
                    )
                })
                .collect();
            match plot.kind {
                PlotKind::Line => {
                    elements.extend(self.line_drawables(chart, plot, index, &final_data));
                }
                PlotKind::Bar => {
                    elements.extend(self.bar_drawables(chart, x_axis, index, &final_data));
                }
            }
        }
        elements
    }

    fn bar_drawables(
        &self,
        chart: &Chart,
        x_axis: &ChartAxis,
        plot_index: usize,
        final_data: &[(f64, f64)],
    ) -> Vec<DrawableElem> {
        let color = chart.theme.plot_color(plot_index);
        let bar_width = chart_bar_width(x_axis);
        let rects: Vec<RectElem> = final_data
            .iter()
            .map(|data| self.bar_rect(chart, *data, bar_width, &color))
            .collect();
        let group_texts = vec!["plot".to_string(), format!("bar-plot-{plot_index}")];
        let mut elements = vec![rect_drawable(group_texts.clone(), rects)];

        if chart.config.show_data_label
            && let Some(labels) = self.bar_data_labels(chart, final_data, bar_width, group_texts)
        {
            elements.push(labels);
        }
        elements
    }

    /// One `<rect>` of a bar plot, as `BarPlot.getDrawableElement` maps `finalData`.
    fn bar_rect(&self, chart: &Chart, data: (f64, f64), bar_width: f64, color: &str) -> RectElem {
        let bar_width_half = bar_width / 2.0;
        if chart.is_horizontal() {
            RectElem {
                x: self.bounding.x,
                y: data.0 - bar_width_half,
                width: data.1 - self.bounding.x,
                height: bar_width,
                fill: color.to_string(),
                stroke_width: 0.0,
                stroke_fill: Some(color.to_string()),
            }
        } else {
            RectElem {
                x: data.0 - bar_width_half,
                y: data.1,
                width: bar_width,
                height: self.bounding.y + self.bounding.height - data.1,
                fill: color.to_string(),
                stroke_width: 0.0,
                stroke_fill: Some(color.to_string()),
            }
        }
    }

    /// `xychartRenderer.ts`'s `showDataLabel` block for a rect shape.
    fn bar_data_labels(
        &self,
        chart: &Chart,
        final_data: &[(f64, f64)],
        bar_width: f64,
        group_texts: Vec<String>,
    ) -> Option<DrawableElem> {
        let show_outside = chart.config.show_data_label_outside_bar;
        // `labelData` is the first plot's values, whatever plot the rects belong to.
        let label_data: Vec<String> = chart.ast.plots[0]
            .data
            .iter()
            .map(|point| point.value.map(format_number).unwrap_or_default())
            .collect();
        let items: Vec<(RectElem, String)> = final_data
            .iter()
            .enumerate()
            .filter_map(|(index, data)| {
                let label = label_data.get(index)?.clone();
                let rect = self.bar_rect(chart, *data, bar_width, "");
                // A zero-width or zero-height bar is not labelled.
                if rect.width > 0.0 && rect.height > 0.0 && !label.is_empty() {
                    Some((rect, label))
                } else {
                    None
                }
            })
            .collect();
        if items.is_empty() {
            return None;
        }

        let text = |x: f64,
                    y: f64,
                    label: String,
                    vertical: VerticalPosition,
                    horizontal: HorizontalPosition| {
            TextElem {
                x,
                y,
                text: label,
                fill: chart.theme.data_label.clone(),
                font_size: 0.0,
                rotation: 0.0,
                vertical_pos: vertical,
                horizontal_pos: horizontal,
            }
        };

        if chart.is_horizontal() {
            // Factor to approximate each character's width.
            let char_width_factor = 0.7;
            let right_margin = 10.0;
            let fits_horizontally = |item: &(RectElem, String), font_size: f64| {
                let text_width =
                    font_size * item.1.encode_utf16().count() as f64 * char_width_factor;
                text_width <= item.0.width - right_margin
            };
            let candidate_font_sizes: Vec<f64> = items
                .iter()
                .map(|item| {
                    let mut font_size = item.0.height * 0.7;
                    while !fits_horizontally(item, font_size) && font_size > 0.0 {
                        font_size -= 1.0;
                    }
                    font_size
                })
                .collect();
            let uniform_font_size = candidate_font_sizes
                .iter()
                .copied()
                .fold(f64::INFINITY, f64::min)
                .floor();
            let data: Vec<TextElem> = items
                .into_iter()
                .map(|(rect, label)| {
                    let mut elem = text(
                        if show_outside {
                            rect.x + rect.width + right_margin
                        } else {
                            rect.x + rect.width - right_margin
                        },
                        rect.y + rect.height / 2.0,
                        label,
                        VerticalPosition::Middle,
                        if show_outside {
                            HorizontalPosition::Left
                        } else {
                            HorizontalPosition::Right
                        },
                    );
                    elem.font_size = uniform_font_size;
                    elem
                })
                .collect();
            return Some(text_drawable(group_texts, data));
        }

        let y_offset = 10.0;
        let fits_in_bar = |item: &(RectElem, String), font_size: f64, y_offset: f64| {
            let char_width_factor = 0.7;
            let text_width = font_size * item.1.encode_utf16().count() as f64 * char_width_factor;
            let center_x = item.0.x + item.0.width / 2.0;
            let left_edge = center_x - text_width / 2.0;
            let right_edge = center_x + text_width / 2.0;
            let horizontal_fits = left_edge >= item.0.x && right_edge <= item.0.x + item.0.width;
            // With `dominant-baseline: hanging`, y marks the top of the text.
            let vertical_fits = item.0.y + y_offset + font_size <= item.0.y + item.0.height;
            horizontal_fits && vertical_fits
        };
        let candidate_font_sizes: Vec<f64> = items
            .iter()
            .map(|item| {
                let mut font_size = item.0.width / (item.1.encode_utf16().count() as f64 * 0.7);
                while !fits_in_bar(item, font_size, y_offset) && font_size > 0.0 {
                    font_size -= 1.0;
                }
                font_size
            })
            .collect();
        let uniform_font_size = candidate_font_sizes
            .iter()
            .copied()
            .fold(f64::INFINITY, f64::min)
            .floor();
        let data: Vec<TextElem> = items
            .into_iter()
            .map(|(rect, label)| {
                let mut elem = text(
                    rect.x + rect.width / 2.0,
                    if show_outside {
                        rect.y - y_offset
                    } else {
                        rect.y + y_offset
                    },
                    label,
                    if show_outside {
                        VerticalPosition::Auto
                    } else {
                        VerticalPosition::Hanging
                    },
                    HorizontalPosition::Center,
                );
                elem.font_size = uniform_font_size;
                elem
            })
            .collect();
        Some(text_drawable(group_texts, data))
    }

    fn line_drawables(
        &self,
        chart: &Chart,
        plot: &super::ast::Plot,
        plot_index: usize,
        final_data: &[(f64, f64)],
    ) -> Vec<DrawableElem> {
        if final_data.is_empty() {
            return Vec::new();
        }
        let mut path = Path::new();
        let points: Vec<(f64, f64)> = if chart.is_horizontal() {
            final_data.iter().map(|(x, y)| (*y, *x)).collect()
        } else {
            final_data.to_vec()
        };
        path.move_to(Point::new(points[0].0, points[0].1));
        for point in &points[1..] {
            path.line_to(Point::new(point.0, point.1));
        }
        let color = chart.theme.plot_color(plot_index);
        let group_texts = vec!["plot".to_string(), format!("line-plot-{plot_index}")];
        let mut elements = vec![path_drawable(
            group_texts.clone(),
            PathElem {
                path,
                fill: None,
                stroke_width: 2.0,
                stroke_fill: color.clone(),
            },
        )];

        if let Some(point_labels) = &plot.point_labels
            && !point_labels.is_empty()
        {
            let mut data = Vec::new();
            for (index, (px, py)) in final_data.iter().enumerate() {
                let Some(label) = point_labels.get(index) else {
                    continue;
                };
                if label.is_empty() {
                    continue;
                }
                data.push(if chart.is_horizontal() {
                    TextElem {
                        x: py + POINT_LABEL_OFFSET,
                        y: *px,
                        text: label.clone(),
                        fill: color.clone(),
                        vertical_pos: VerticalPosition::Middle,
                        horizontal_pos: HorizontalPosition::Left,
                        font_size: POINT_LABEL_FONT_SIZE,
                        rotation: 0.0,
                    }
                } else {
                    TextElem {
                        x: *px,
                        y: py - POINT_LABEL_OFFSET,
                        text: label.clone(),
                        fill: color.clone(),
                        vertical_pos: VerticalPosition::Middle,
                        horizontal_pos: HorizontalPosition::Center,
                        font_size: POINT_LABEL_FONT_SIZE,
                        rotation: 0.0,
                    }
                });
            }
            if !data.is_empty() {
                elements.push(text_drawable(
                    vec![
                        "plot".into(),
                        format!("line-plot-{plot_index}"),
                        "labels".into(),
                    ],
                    data,
                ));
            }
        }
        elements
    }
}

// ---------------------------------------------------------------------------
// Orchestrator
// ---------------------------------------------------------------------------

/// `Orchestrator`: reserves space and collects the components' elements.
struct Orchestrator {
    title: ChartTitle,
    plot: Plot,
    legend: ChartLegend,
    x_axis: ChartAxis,
    y_axis: ChartAxis,
}

impl Orchestrator {
    fn new(chart: &Chart) -> Self {
        let data_axis = |axis: &DataAxis| match axis {
            DataAxis::Band { categories, .. } => AxisData::Band(categories.clone()),
            DataAxis::Linear { min, max, .. } => AxisData::Linear {
                min: *min,
                max: *max,
            },
        };
        Self {
            title: ChartTitle::new(),
            plot: Plot::new(),
            legend: ChartLegend::new(),
            x_axis: ChartAxis::new(
                data_axis(&chart.ast.x_axis),
                chart.ast.x_axis.title().to_string(),
                chart.config.x_axis.clone(),
                chart.theme.x_axis.clone(),
            ),
            y_axis: ChartAxis::new(
                data_axis(&chart.ast.y_axis),
                chart.ast.y_axis.title().to_string(),
                chart.config.y_axis.clone(),
                chart.theme.y_axis.clone(),
            ),
        }
    }

    fn drawables(&mut self, chart: &Chart) -> Vec<DrawableElem> {
        self.calculate_space(chart);
        let mut drawable = Vec::new();
        drawable.extend(self.title.drawables(chart));
        drawable.extend(self.plot.drawables(chart, &self.x_axis, &self.y_axis));
        drawable.extend(self.legend.drawables(chart));
        drawable.extend(self.x_axis.drawables(chart));
        drawable.extend(self.y_axis.drawables(chart));
        drawable
    }

    fn calculate_space(&mut self, chart: &Chart) {
        if chart.is_horizontal() {
            self.calculate_horizontal_space(chart);
        } else {
            self.calculate_vertical_space(chart);
        }
    }

    fn calculate_vertical_space(&mut self, chart: &Chart) {
        let mut available_width = chart.config.width;
        let mut available_height = chart.config.height;
        let mut chart_width =
            (available_width * chart.config.plot_reserved_space_percent / 100.0).floor();
        let mut chart_height =
            (available_height * chart.config.plot_reserved_space_percent / 100.0).floor();

        let space_used = self
            .plot
            .calculate_space(dimension(chart_width, chart_height));
        available_width -= space_used.width;
        available_height -= space_used.height;

        let space_used = self
            .title
            .calculate_space(chart, dimension(chart.config.width, available_height));
        let plot_y = space_used.height;
        available_height -= space_used.height;

        self.x_axis.set_axis_position(AxisPosition::Bottom);
        let space_used = self
            .x_axis
            .calculate_space(chart, dimension(available_width, available_height));
        available_height -= space_used.height;

        self.y_axis.set_axis_position(AxisPosition::Left);
        let space_used = self
            .y_axis
            .calculate_space(chart, dimension(available_width, available_height));
        let plot_x = space_used.width;
        available_width -= space_used.width;

        let legend_space = self
            .legend
            .calculate_space(chart, dimension(available_width, chart_height));
        available_width -= legend_space.width;

        if available_width > 0.0 {
            chart_width += available_width;
        }
        if available_height > 0.0 {
            chart_height += available_height;
        }
        self.plot
            .calculate_space(dimension(chart_width, chart_height));

        self.plot.set_bounding_box_xy(Position {
            x: plot_x,
            y: plot_y,
        });
        self.title.align_to(chart, plot_x, plot_x + chart_width);
        self.legend.set_bounding_box_xy(Position {
            x: plot_x + chart_width,
            y: plot_y + ((chart_height - legend_space.height) / 2.0).max(0.0),
        });
        self.x_axis.set_range((plot_x, plot_x + chart_width));
        self.x_axis.set_bounding_box_xy(Position {
            x: plot_x,
            y: plot_y + chart_height,
        });
        self.y_axis.set_range((plot_y, plot_y + chart_height));
        self.y_axis
            .set_bounding_box_xy(Position { x: 0.0, y: plot_y });
        if chart
            .ast
            .plots
            .iter()
            .any(|plot| plot.kind == PlotKind::Bar)
        {
            self.x_axis.recalculate_outer_padding_to_draw_bar();
        }
    }

    fn calculate_horizontal_space(&mut self, chart: &Chart) {
        let mut available_width = chart.config.width;
        let mut available_height = chart.config.height;
        let mut chart_width =
            (available_width * chart.config.plot_reserved_space_percent / 100.0).floor();
        let mut chart_height =
            (available_height * chart.config.plot_reserved_space_percent / 100.0).floor();

        let space_used = self
            .plot
            .calculate_space(dimension(chart_width, chart_height));
        available_width -= space_used.width;
        available_height -= space_used.height;

        let space_used = self
            .title
            .calculate_space(chart, dimension(chart.config.width, available_height));
        let title_y_end = space_used.height;
        available_height -= space_used.height;

        self.x_axis.set_axis_position(AxisPosition::Left);
        let space_used = self
            .x_axis
            .calculate_space(chart, dimension(available_width, available_height));
        available_width -= space_used.width;
        let plot_x = space_used.width;

        self.y_axis.set_axis_position(AxisPosition::Top);
        let space_used = self
            .y_axis
            .calculate_space(chart, dimension(available_width, available_height));
        available_height -= space_used.height;
        let plot_y = title_y_end + space_used.height;

        let legend_space = self
            .legend
            .calculate_space(chart, dimension(available_width, chart_height));
        available_width -= legend_space.width;

        if available_width > 0.0 {
            chart_width += available_width;
        }
        if available_height > 0.0 {
            chart_height += available_height;
        }
        self.plot
            .calculate_space(dimension(chart_width, chart_height));

        self.plot.set_bounding_box_xy(Position {
            x: plot_x,
            y: plot_y,
        });
        self.title.align_to(chart, plot_x, plot_x + chart_width);
        self.legend.set_bounding_box_xy(Position {
            x: plot_x + chart_width,
            y: plot_y + ((chart_height - legend_space.height) / 2.0).max(0.0),
        });
        self.y_axis.set_range((plot_x, plot_x + chart_width));
        self.y_axis.set_bounding_box_xy(Position {
            x: plot_x,
            y: title_y_end,
        });
        self.x_axis.set_range((plot_y, plot_y + chart_height));
        self.x_axis
            .set_bounding_box_xy(Position { x: 0.0, y: plot_y });
        if chart
            .ast
            .plots
            .iter()
            .any(|plot| plot.kind == PlotKind::Bar)
        {
            self.x_axis.recalculate_outer_padding_to_draw_bar();
        }
    }
}

/// `BarPlot.getDrawableElement`'s bar width: twice the axis's outer padding capped by the tick
/// distance, minus the 5% bar padding.
fn chart_bar_width(x_axis: &ChartAxis) -> f64 {
    (x_axis.get_axis_outer_padding() * 2.0).min(x_axis.get_tick_distance())
        * (1.0 - BAR_PADDING_PERCENT)
}

// ---------------------------------------------------------------------------
// Scene assembly
// ---------------------------------------------------------------------------

/// `dimension(width, height)`.
fn dimension(width: f64, height: f64) -> Dimension {
    Dimension { width, height }
}

fn rect_drawable(group_texts: Vec<String>, data: Vec<RectElem>) -> DrawableElem {
    DrawableElem {
        group_texts,
        data: DrawableData::Rect(data),
    }
}

fn text_drawable(group_texts: Vec<String>, data: Vec<TextElem>) -> DrawableElem {
    DrawableElem {
        group_texts,
        data: DrawableData::Text(data),
    }
}

fn path_drawable(group_texts: Vec<String>, data: PathElem) -> DrawableElem {
    paths_drawable(group_texts, vec![data])
}

fn paths_drawable(group_texts: Vec<String>, data: Vec<PathElem>) -> DrawableElem {
    DrawableElem {
        group_texts,
        data: DrawableData::Path(data),
    }
}

/// Builds the `<g>` tree `xychartRenderer.ts` appends, then the [`Scene`].
fn build_scene(chart: &Chart, shapes: Vec<DrawableElem>) -> Scene {
    // `getGroup`: groups are created on first use and keyed by the concatenated group texts.
    let mut nodes = vec![Node {
        class: "main".to_string(),
        children: Vec::new(),
    }];
    let mut groups: HashMap<String, usize> = HashMap::new();

    nodes[0]
        .children
        .push(Entry::Item(Box::new(background_item(chart))));
    for shape in shapes {
        if shape.is_empty() {
            continue;
        }
        let group = get_group(&mut nodes, &mut groups, &shape.group_texts);
        let item = match shape.data {
            DrawableData::Rect(data) => {
                let mut rects = Vec::new();
                for elem in data {
                    rects.push(Entry::Item(Box::new(Item::Rect(rect_shape(elem)))));
                }
                rects
            }
            DrawableData::Text(data) => data
                .into_iter()
                .map(|elem| Entry::Item(Box::new(Item::Text(text_run(chart, elem)))))
                .collect(),
            DrawableData::Path(data) => data
                .into_iter()
                .map(|elem| Entry::Item(Box::new(Item::Path(path_shape(elem)))))
                .collect(),
        };
        nodes[group].children.extend(item);
    }

    let main = build_group(&nodes, 0);
    let mut scene = Scene::new(Rect::new(0.0, 0.0, chart.config.width, chart.config.height));
    scene.push(main);
    scene
}

/// `group` / `entries` in the SVG assembly: a group or a drawable item.
enum Entry {
    Node(usize),
    Item(Box<Item>),
}

/// One assembled `<g>`.
struct Node {
    class: String,
    children: Vec<Entry>,
}

/// `getGroup(gList)` from `xychartRenderer.ts`.
fn get_group(
    nodes: &mut Vec<Node>,
    groups: &mut HashMap<String, usize>,
    texts: &[String],
) -> usize {
    let mut parent = 0;
    let mut prefix = String::new();
    let mut elem = 0;
    for (index, text) in texts.iter().enumerate() {
        if index > 0
            && let Some(&group) = groups.get(&prefix)
        {
            parent = group;
        }
        prefix.push_str(text);
        elem = match groups.get(&prefix) {
            Some(&group) => group,
            None => {
                let group = nodes.len();
                nodes.push(Node {
                    class: text.clone(),
                    children: Vec::new(),
                });
                nodes[parent].children.push(Entry::Node(group));
                groups.insert(prefix.clone(), group);
                group
            }
        };
    }
    elem
}

fn build_group(nodes: &[Node], index: usize) -> Group {
    let mut group = Group::new().with_class(nodes[index].class.clone());
    for child in &nodes[index].children {
        match child {
            Entry::Node(child) => group.push(build_group(nodes, *child)),
            Entry::Item(item) => group.push((**item).clone()),
        }
    }
    group
}

/// The `rect.background` the renderer appends before any component.
fn background_item(chart: &Chart) -> Item {
    Item::Rect(RectShape::new(
        Rect::new(0.0, 0.0, chart.config.width, chart.config.height),
        0.0,
        crate::scene::PathStyle {
            fill: Some(Color::new(chart.theme.background.clone())),
            stroke: None,
            stroke_width: 0.0,
            ..crate::scene::PathStyle::default()
        },
    ))
}

fn rect_shape(elem: RectElem) -> RectShape {
    RectShape::new(
        Rect::new(elem.x, elem.y, elem.width, elem.height),
        0.0,
        crate::scene::PathStyle {
            fill: Some(Color::new(elem.fill)),
            stroke: elem.stroke_fill.map(Color::new),
            stroke_width: elem.stroke_width,
            ..crate::scene::PathStyle::default()
        },
    )
}

fn path_shape(elem: PathElem) -> PathShape {
    PathShape::new(
        elem.path,
        crate::scene::PathStyle {
            fill: elem.fill.map(Color::new),
            stroke: Some(Color::new(elem.stroke_fill)),
            stroke_width: elem.stroke_width,
            ..crate::scene::PathStyle::default()
        },
    )
}

fn text_run(chart: &Chart, elem: TextElem) -> TextRun {
    let font = Font::new(FONT_FAMILY, elem.font_size);
    let options = TextOptions {
        width: f64::INFINITY,
        markdown: false,
        min_width: 0.0,
    };
    let block: TextBlock = shape_text(&elem.text, &options, &font, chart.measure)
        .unwrap_or_else(|_| TextBlock::default());
    let mut run = TextRun::new(Point::new(elem.x, elem.y), font, block);
    run.anchor = match elem.horizontal_pos {
        HorizontalPosition::Left => TextAnchor::Start,
        HorizontalPosition::Center => TextAnchor::Middle,
        HorizontalPosition::Right => TextAnchor::End,
    };
    run.baseline = match elem.vertical_pos {
        VerticalPosition::Top => TextBaseline::Top,
        VerticalPosition::Middle => TextBaseline::Middle,
        VerticalPosition::Hanging => TextBaseline::Hanging,
        VerticalPosition::Auto => TextBaseline::Alphabetic,
    };
    run.rotation = elem.rotation;
    run.fill = Some(Color::new(elem.fill));
    run
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOLERANCE: f64 = 0.01;

    fn fixtures() -> Vec<(String, serde_json::Value)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/diagrams/xychart/tests/render");
        let mut fixtures: Vec<(String, serde_json::Value)> = std::fs::read_dir(&dir)
            .unwrap_or_else(|error| panic!("read {}: {error}", dir.display()))
            .map(|entry| entry.expect("dir entry").path())
            .filter(|path| {
                path.extension().and_then(|extension| extension.to_str()) == Some("json")
            })
            .map(|path| {
                let text = std::fs::read_to_string(&path).expect("read fixture");
                let value: serde_json::Value = serde_json::from_str(&text).expect("parse fixture");
                (
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or_default()
                        .to_string(),
                    value,
                )
            })
            .collect();
        fixtures.sort_by(|a, b| a.0.cmp(&b.0));
        fixtures
    }

    fn close(actual: f64, expected: f64, what: &str, file: &str) {
        assert!(
            (actual - expected).abs() <= TOLERANCE,
            "{file}: {what}: actual {actual} vs expected {expected}"
        );
    }

    fn close_value(
        actual: &serde_json::Value,
        expected: &serde_json::Value,
        what: &str,
        file: &str,
    ) {
        match (actual, expected) {
            (serde_json::Value::Number(actual), serde_json::Value::Number(expected)) => close(
                actual.as_f64().expect("number"),
                expected.as_f64().expect("number"),
                what,
                file,
            ),
            (serde_json::Value::Array(actual), serde_json::Value::Array(expected)) => {
                assert_eq!(actual.len(), expected.len(), "{file}: {what}: length");
                for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                    close_value(actual, expected, &format!("{what}[{index}]"), file);
                }
            }
            (serde_json::Value::Object(actual), serde_json::Value::Object(expected)) => {
                assert_eq!(actual.len(), expected.len(), "{file}: {what}: keys");
                for (key, expected) in expected {
                    close_value(
                        actual.get(key).unwrap_or(&serde_json::Value::Null),
                        expected,
                        &format!("{what}.{key}"),
                        file,
                    );
                }
            }
            _ => assert_eq!(actual, expected, "{file}: {what}"),
        }
    }

    fn rect_json(rect: Rect) -> serde_json::Value {
        serde_json::json!({
            "x": rect.x,
            "y": rect.y,
            "width": rect.width,
            "height": rect.height,
        })
    }

    fn color(style: &crate::scene::PathStyle, filled: bool) -> serde_json::Value {
        match if filled { &style.fill } else { &style.stroke } {
            Some(color) => serde_json::Value::String(color.0.clone()),
            None => serde_json::Value::Null,
        }
    }

    fn item_json(item: &Item) -> serde_json::Value {
        match item {
            Item::Group(group) => group_json(group),
            Item::Rect(rect) => serde_json::json!({
                "type": "rect",
                "x": rect.rect.x,
                "y": rect.rect.y,
                "width": rect.rect.width,
                "height": rect.rect.height,
                "fill": color(&rect.style, true),
                "stroke": color(&rect.style, false),
                "strokeWidth": rect.style.stroke_width,
            }),
            Item::Path(path) => serde_json::json!({
                "type": "path",
                "commands": serde_json::to_value(&path.path.commands).expect("path commands"),
                "fill": color(&path.style, true),
                "stroke": color(&path.style, false),
                "strokeWidth": path.style.stroke_width,
            }),
            Item::Text(text) => {
                let content = text
                    .lines
                    .iter()
                    .map(|line| line.text())
                    .collect::<Vec<_>>()
                    .join("\n");
                serde_json::json!({
                    "type": "text",
                    "text": content,
                    "x": text.position.x,
                    "y": text.position.y,
                    "rotation": text.rotation,
                    "fontSize": text.font.size,
                    "fill": text.fill.as_ref().map(|color| color.0.clone()),
                    "anchor": match text.anchor {
                        TextAnchor::Start => "start",
                        TextAnchor::Middle => "middle",
                        TextAnchor::End => "end",
                    },
                    "baseline": match text.baseline {
                        TextBaseline::Alphabetic => "auto",
                        TextBaseline::Middle => "middle",
                        TextBaseline::Central => "central",
                        TextBaseline::Hanging => "hanging",
                        TextBaseline::Top => "text-before-edge",
                        TextBaseline::Bottom => "text-after-edge",
                    },
                })
            }
            other => panic!("unexpected xychart scene item: {other:?}"),
        }
    }

    fn group_json(group: &Group) -> serde_json::Value {
        serde_json::json!({
            "class": group.class,
            "items": group.items.iter().map(item_json).collect::<Vec<_>>(),
        })
    }

    fn scene_geometry(scene: &Scene) -> serde_json::Value {
        serde_json::json!({
            "viewBox": rect_json(scene.view_box),
            "groups": scene
                .items
                .iter()
                .filter(|item| matches!(item, Item::Group(_)))
                .map(item_json)
                .collect::<Vec<_>>(),
        })
    }

    #[test]
    fn renders_recorded_xychart_fixtures() {
        let fixtures = fixtures();
        assert!(
            fixtures.len() >= 8,
            "expected at least 8 fixtures, found {}",
            fixtures.len()
        );

        for (file, fixture) in fixtures {
            let source = fixture["source"].as_str().expect("fixture source");
            let preprocessed = crate::preprocess::preprocess(source);
            assert!(
                preprocessed.error.is_none(),
                "{file}: preprocess: {:?}",
                preprocessed.error
            );
            let config = Config::from_source_config(&preprocessed.config)
                .unwrap_or_else(|error| panic!("{file}: config: {error}"));
            let ast = super::super::parse(&preprocessed.body)
                .unwrap_or_else(|error| panic!("{file}: parse: {error}"));
            let scene = draw_ast(&ast, &config, &crate::measure::FallbackMeasure)
                .unwrap_or_else(|error| panic!("{file}: render: {error}"));
            close_value(
                &scene_geometry(&scene),
                &fixture["expected"],
                "expected",
                &file,
            );
        }
    }

    #[test]
    fn render_diagram_dispatches_xychart() {
        let scene = crate::render_diagram(
            "xychart-beta\n  x-axis [a, b]\n  y-axis 0 --> 10\n  bar [3, 8]\n",
            &Config::default(),
        )
        .expect("xychart renders");
        assert!(!scene.items.is_empty());
    }

    #[test]
    fn reports_a_chart_without_plots() {
        let error = crate::render_diagram("xychart-beta\n  title \"Empty\"\n", &Config::default())
            .expect_err("a chart without plots is not drawn");
        assert_eq!(
            error.to_string(),
            "No Plot to render, please provide a plot with some data"
        );
    }
}
