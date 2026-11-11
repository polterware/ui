//! The xychart database after parsing: what upstream's `XYChartDB` holds when `parse` returns
//! (`xychartDb.ts`'s `xyChartConfig.chartOrientation` and `getXYChartData()`, plus the
//! accessibility fields of `commonDb.ts`).
//!
//! The axes, the plots, and their data are the shapes of `chartBuilder/interfaces.ts`
//! (`AxisDataType`, `PlotData`, `SimplePlotDataType`). The database transforms every plot as
//! its statement arrives (`transformDataWithoutCategory`), so [`Plot::data`] already carries
//! the labels the renderer would receive: band categories, or the linear axis values formatted
//! the way JavaScript's `${number}` formats them.
//!
//! # Where the port stops short of upstream
//!
//! * `strokeFill`, `fill`, and `strokeWidth` are theme and configuration values
//!   (`plotColorPalette`), so they are not parsing state and are left to the painter.
//! * `xyChartData.title` is `getDiagramTitle()` assigned inside `getDrawableElem`; here the
//!   title is [`Ast::diagram_title`], where the database keeps it.
//! * A band axis with fewer categories than the plot's data yields `[category, undefined]` in
//!   JavaScript; here the value is `None`.

/// `XYChartConfig['chartOrientation']`; upstream's default config is `vertical`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Orientation {
    #[default]
    Vertical,
    Horizontal,
}

/// `AxisDataType`: the x axis defaults to an empty band, the y axis to an empty linear range.
#[derive(Clone, Debug, PartialEq)]
pub enum Axis {
    /// `BandAxisDataType`.
    Band {
        title: String,
        categories: Vec<String>,
    },
    /// `LinearAxisDataType`. The defaults are JavaScript's: `Infinity` and `-Infinity`.
    Linear { title: String, min: f64, max: f64 },
}

impl Axis {
    /// The axis title, whichever kind of axis it is.
    pub fn title(&self) -> &str {
        match self {
            Axis::Band { title, .. } | Axis::Linear { title, .. } => title,
        }
    }
}

/// Which kind of `PlotData` a plot is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlotKind {
    Line,
    Bar,
}

/// One entry of `SimplePlotDataType`: the x label and the y value.
#[derive(Clone, Debug, PartialEq)]
pub struct PlotPoint {
    /// A band category, or `String(x)` of the linear axis value.
    pub label: String,
    /// The value; `None` where upstream's `categories.map` produced `undefined`.
    pub value: Option<f64>,
}

/// One `PlotData`, after `transformDataWithoutCategory` ran.
#[derive(Clone, Debug, PartialEq)]
pub struct Plot {
    pub kind: PlotKind,
    /// The sanitized plot title; empty when the statement had none.
    pub title: String,
    pub data: Vec<PlotPoint>,
    /// `LinePlotData.pointLabels`: the sanitized per-point labels, present only when at least
    /// one point carried one. They are not truncated to the plotted data, as upstream keeps
    /// the labels it read before `transformDataWithoutCategory`.
    pub point_labels: Option<Vec<String>>,
}

/// What upstream's xychart database holds after parsing.
#[derive(Clone, Debug, PartialEq)]
pub struct Ast {
    /// `getChartConfig().chartOrientation`.
    pub orientation: Orientation,
    /// `getXYChartData().xAxis`.
    pub x_axis: Axis,
    /// `getXYChartData().yAxis`.
    pub y_axis: Axis,
    /// `getXYChartData().plots`, in statement order.
    pub plots: Vec<Plot>,
    /// `getDiagramTitle()`.
    pub diagram_title: String,
    /// `getAccTitle()`.
    pub acc_title: String,
    /// `getAccDescription()`.
    pub acc_description: String,
}

impl Default for Ast {
    /// `getChartDefaultData`: an empty band x axis, a linear y axis from `Infinity` to
    /// `-Infinity`, no plots, and the default orientation.
    fn default() -> Self {
        Self {
            orientation: Orientation::Vertical,
            x_axis: Axis::Band {
                title: String::new(),
                categories: Vec::new(),
            },
            y_axis: Axis::Linear {
                title: String::new(),
                min: f64::INFINITY,
                max: f64::NEG_INFINITY,
            },
            plots: Vec::new(),
            diagram_title: String::new(),
            acc_title: String::new(),
            acc_description: String::new(),
        }
    }
}
