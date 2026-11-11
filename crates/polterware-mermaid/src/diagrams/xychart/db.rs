//! The xychart database's writing side, ported from `xychartDb.ts` and the accessibility
//! setters of `diagrams/common/commonDb.ts`.
//!
//! The grammar hands the database one action at a time and the database transforms each plot
//! as it arrives, so the result depends on statement order exactly as upstream: a `line` placed
//! before the `x-axis` band is labelled from the default axis (which becomes a linear range
//! from 1 to the number of points), and a later `x-axis` does not relabel it.
//!
//! The plot colours (`plotColorPalette`, `strokeFill`, `fill`) and the `plotIndex` that picks
//! them are configuration and theme, not parsing state, so they are not kept.

use super::ast::{Ast, Axis, Orientation, Plot, PlotKind, PlotPoint};
use super::parser::{ParsedPoint, Statement};
use super::sanitize;
use crate::preprocess::is_js_whitespace;

/// Applies the parsed statements to a fresh database, in order.
pub(crate) fn build(statements: Vec<Statement>) -> Ast {
    let mut db = Db::new();
    for statement in statements {
        db.apply(statement);
    }
    db.into_ast()
}

/// The module state `xychartDb.ts` keeps, without the theme and renderer parts.
struct Db {
    orientation: Orientation,
    x_axis: Axis,
    y_axis: Axis,
    has_set_x_axis: bool,
    has_set_y_axis: bool,
    plots: Vec<Plot>,
    diagram_title: String,
    acc_title: String,
    acc_description: String,
}

impl Db {
    fn new() -> Self {
        let ast = Ast::default();
        Self {
            orientation: ast.orientation,
            x_axis: ast.x_axis,
            y_axis: ast.y_axis,
            has_set_x_axis: false,
            has_set_y_axis: false,
            plots: Vec::new(),
            diagram_title: String::new(),
            acc_title: String::new(),
            acc_description: String::new(),
        }
    }

    fn apply(&mut self, statement: Statement) {
        match statement {
            Statement::Orientation(orientation) => self.set_orientation(&orientation),
            Statement::Title(title) => self.set_diagram_title(&title),
            Statement::AccTitle(title) => self.acc_title = sanitize::set_acc_title(&title),
            Statement::AccDescription(description) => {
                self.acc_description = sanitize::set_acc_description(&description);
            }
            Statement::XAxisTitle(title) => self.set_x_axis_title(&title),
            Statement::XAxisRange(min, max) => self.set_x_axis_range(min, max),
            Statement::XAxisBand(categories) => self.set_x_axis_band(&categories),
            Statement::YAxisTitle(title) => self.set_y_axis_title(&title),
            Statement::YAxisRange(min, max) => self.set_y_axis_range(min, max),
            Statement::Line { title, points } => self.set_line_data(&title, &points),
            Statement::Bar { title, points } => self.set_bar_data(&title, &points),
        }
    }

    /// `setOrientation`: anything but the exact string `horizontal` is vertical.
    fn set_orientation(&mut self, orientation: &str) {
        self.orientation = if orientation == "horizontal" {
            Orientation::Horizontal
        } else {
            Orientation::Vertical
        };
    }

    /// `setDiagramTitle`.
    fn set_diagram_title(&mut self, title: &str) {
        self.diagram_title = sanitize::sanitize_text(title);
    }

    /// `setXAxisTitle`.
    fn set_x_axis_title(&mut self, title: &str) {
        let title = text_sanitizer(title);
        match &mut self.x_axis {
            Axis::Band { title: current, .. } | Axis::Linear { title: current, .. } => {
                *current = title;
            }
        }
    }

    /// `setXAxisRangeData`.
    fn set_x_axis_range(&mut self, min: f64, max: f64) {
        self.x_axis = Axis::Linear {
            title: self.x_axis.title().to_string(),
            min,
            max,
        };
        self.has_set_x_axis = true;
    }

    /// `setXAxisBand`.
    fn set_x_axis_band(&mut self, categories: &[String]) {
        self.x_axis = Axis::Band {
            title: self.x_axis.title().to_string(),
            categories: categories.iter().map(|text| text_sanitizer(text)).collect(),
        };
        self.has_set_x_axis = true;
    }

    /// `setYAxisTitle`.
    fn set_y_axis_title(&mut self, title: &str) {
        let title = text_sanitizer(title);
        match &mut self.y_axis {
            Axis::Band { title: current, .. } | Axis::Linear { title: current, .. } => {
                *current = title;
            }
        }
    }

    /// `setYAxisRangeData`.
    fn set_y_axis_range(&mut self, min: f64, max: f64) {
        self.y_axis = Axis::Linear {
            title: self.y_axis.title().to_string(),
            min,
            max,
        };
        self.has_set_y_axis = true;
    }

    /// `setLineData`.
    fn set_line_data(&mut self, title: &str, points: &[ParsedPoint]) {
        let values: Vec<f64> = points.iter().map(|point| point.value).collect();
        let labels: Vec<String> = points
            .iter()
            .map(|point| {
                if point.label.is_empty() {
                    String::new()
                } else {
                    text_sanitizer(&point.label)
                }
            })
            .collect();
        let data = self.transform_data(&values);
        let point_labels = labels
            .iter()
            .any(|label| !label.is_empty())
            .then_some(labels);
        self.plots.push(Plot {
            kind: PlotKind::Line,
            title: text_sanitizer(title),
            data,
            point_labels,
        });
    }

    /// `setBarData`: the point labels `dataPoint` accepts are not part of a bar plot.
    fn set_bar_data(&mut self, title: &str, points: &[ParsedPoint]) {
        let values: Vec<f64> = points.iter().map(|point| point.value).collect();
        let data = self.transform_data(&values);
        self.plots.push(Plot {
            kind: PlotKind::Bar,
            title: text_sanitizer(title),
            data,
            point_labels: None,
        });
    }

    /// `transformDataWithoutCategory`: the x labels come from the current axis, and a band
    /// axis truncates the data to its categories before the y range is widened.
    fn transform_data(&mut self, values: &[f64]) -> Vec<PlotPoint> {
        if values.is_empty() {
            return Vec::new();
        }
        if !self.has_set_x_axis {
            let (previous_min, previous_max) = match self.x_axis {
                Axis::Linear { min, max, .. } => (min, max),
                Axis::Band { .. } => (f64::INFINITY, f64::NEG_INFINITY),
            };
            self.set_x_axis_range(previous_min.min(1.0), previous_max.max(values.len() as f64));
        }

        let visible = match &self.x_axis {
            Axis::Band { categories, .. } if values.len() > categories.len() => {
                &values[..categories.len()]
            }
            _ => values,
        };

        if !self.has_set_y_axis {
            self.expand_y_axis(visible);
        }

        match &self.x_axis {
            Axis::Band { categories, .. } => categories
                .iter()
                .enumerate()
                .map(|(index, category)| PlotPoint {
                    label: category.clone(),
                    value: visible.get(index).copied(),
                })
                .collect(),
            Axis::Linear { min, max, .. } => {
                if visible.len() == 1 {
                    // For backwards compatibility, avoid the divide-by-zero error.
                    vec![PlotPoint {
                        label: js_number(*min),
                        value: Some(visible[0]),
                    }]
                } else {
                    let step = (max - min) / (visible.len() - 1) as f64;
                    visible
                        .iter()
                        .enumerate()
                        .map(|(index, value)| PlotPoint {
                            label: js_number(min + index as f64 * step),
                            value: Some(*value),
                        })
                        .collect()
                }
            }
        }
    }

    /// `setYAxisRangeFromPlotData`: widen the y range, without marking it as set.
    fn expand_y_axis(&mut self, values: &[f64]) {
        let min_value = values.iter().copied().fold(f64::INFINITY, f64::min);
        let max_value = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let (previous_min, previous_max) = match self.y_axis {
            Axis::Linear { min, max, .. } => (min, max),
            Axis::Band { .. } => (f64::INFINITY, f64::NEG_INFINITY),
        };
        let title = self.y_axis.title().to_string();
        self.y_axis = Axis::Linear {
            title,
            min: previous_min.min(min_value),
            max: previous_max.max(max_value),
        };
    }

    fn into_ast(self) -> Ast {
        Ast {
            orientation: self.orientation,
            x_axis: self.x_axis,
            y_axis: self.y_axis,
            plots: self.plots,
            diagram_title: self.diagram_title,
            acc_title: self.acc_title,
            acc_description: self.acc_description,
        }
    }
}

/// `textSanitizer`: `sanitizeText(text.trim(), config)`, without the configuration.
fn text_sanitizer(text: &str) -> String {
    sanitize::sanitize_text(text.trim_matches(is_js_whitespace))
}

/// `${number}` of a linear axis label: JavaScript's `Number::toString`.
///
/// Rust's shortest round-trip formatting prints the same digits inside `[1e-6, 1e21)`, the
/// range where JavaScript uses the decimal form. Outside it both use the scientific form; the
/// exponent's `+` is added as JavaScript writes it.
fn js_number(value: f64) -> String {
    if value == 0.0 {
        return "0".to_string();
    }
    let magnitude = value.abs();
    if !(1e-6..1e21).contains(&magnitude) {
        let scientific = format!("{value:e}");
        if let Some((mantissa, exponent)) = scientific.split_once('e')
            && !exponent.starts_with('-')
        {
            return format!("{mantissa}e+{exponent}");
        }
        return scientific;
    }
    format!("{value}")
}
