//! The radar database's writing side, ported from `db.ts` and
//! `diagrams/common/populateCommonDb.ts`.
//!
//! `parser.ts` parses the whole document first and then populates the database in one order:
//! the accessibility fields, then `setAxes`, `setCurves`, and `setOptions`. That order is
//! what lets a detailed curve name axes declared later in the document, because
//! `computeCurveEntries` reads the complete axis list, and it is why a missing entry is
//! reported only after the grammar accepted every line.

use super::ast::{Ast, Axis, Curve, Options};
use super::parser::{ParsedCurve, ParsedEntries, ParsedOption, ParsedRadar};
use super::sanitize;
use crate::error::{ParseError, Span};

/// `MAX_TICKS`: more than this makes the gradient unreadable, and unbounded values would let
/// a document ask for millions of rings. Upstream logs a warning and clamps; there is no
/// logger here, so it clamps silently.
const MAX_TICKS: f64 = 32.0;

/// `populate`: applies the parsed document to a fresh database.
pub(crate) fn build(parsed: ParsedRadar) -> Result<Ast, ParseError> {
    let mut db = Db::new();
    // `populateCommonDb`, in its order.
    if let Some(value) = parsed.acc_description.filter(|value| !value.is_empty()) {
        db.set_acc_description(value);
    }
    if let Some(value) = parsed.acc_title.filter(|value| !value.is_empty()) {
        db.set_acc_title(value);
    }
    if let Some(value) = parsed.title.filter(|value| !value.is_empty()) {
        db.set_diagram_title(value);
    }
    db.set_axes(parsed.axes);
    db.set_curves(parsed.curves)?;
    db.set_options(parsed.options);
    Ok(db.into_ast())
}

/// The module state `db.ts` keeps.
struct Db {
    axes: Vec<Axis>,
    curves: Vec<Curve>,
    options: Options,
    diagram_title: String,
    acc_title: String,
    acc_description: String,
}

impl Db {
    fn new() -> Self {
        Self {
            axes: Vec::new(),
            curves: Vec::new(),
            options: Options::default(),
            diagram_title: String::new(),
            acc_title: String::new(),
            acc_description: String::new(),
        }
    }

    /// `setDiagramTitle`: `sanitizeText(txt)`.
    fn set_diagram_title(&mut self, title: String) {
        self.diagram_title = sanitize::sanitize_text(&title);
    }

    /// `setAccTitle`: `sanitizeText(txt).replace(/^\s+/g, '')`.
    fn set_acc_title(&mut self, title: String) {
        self.acc_title = sanitize::set_acc_title(&title);
    }

    /// `setAccDescription`: `sanitizeText(txt).replace(/\n\s+/g, '\n')`.
    fn set_acc_description(&mut self, description: String) {
        self.acc_description = sanitize::set_acc_description(&description);
    }

    /// `setAxes`: a missing label falls back to the name.
    fn set_axes(&mut self, axes: Vec<super::parser::ParsedAxis>) {
        self.axes = axes
            .into_iter()
            .map(|axis| Axis {
                label: axis.label.unwrap_or_else(|| axis.name.clone()),
                name: axis.name,
            })
            .collect();
    }

    /// `setCurves`: a missing label falls back to the name, and detailed entries are
    /// reordered to axis order.
    fn set_curves(&mut self, curves: Vec<ParsedCurve>) -> Result<(), ParseError> {
        for curve in curves {
            let entries = compute_curve_entries(&self.axes, curve.entries, curve.span)?;
            self.curves.push(Curve {
                label: curve.label.unwrap_or_else(|| curve.name.clone()),
                name: curve.name,
                entries,
            });
        }
        Ok(())
    }

    /// `setOptions`: the defaults filled in, the last occurrence of a repeated option
    /// winning, and `ticks` clamped to [`MAX_TICKS`].
    fn set_options(&mut self, options: Vec<ParsedOption>) {
        for option in options {
            match option {
                ParsedOption::ShowLegend(value) => self.options.show_legend = value,
                ParsedOption::Ticks(value) => self.options.ticks = value,
                ParsedOption::Max(value) => self.options.max = Some(value),
                ParsedOption::Min(value) => self.options.min = value,
                ParsedOption::Graticule(value) => self.options.graticule = value,
            }
        }
        if self.options.ticks > MAX_TICKS {
            self.options.ticks = MAX_TICKS;
        }
    }

    fn into_ast(self) -> Ast {
        Ast {
            axes: self.axes,
            curves: self.curves,
            options: self.options,
            diagram_title: self.diagram_title,
            acc_title: self.acc_title,
            acc_description: self.acc_description,
        }
    }
}

/// `computeCurveEntries`: number entries keep source order; detailed entries are looked up
/// by axis, and every axis must have one.
fn compute_curve_entries(
    axes: &[Axis],
    entries: ParsedEntries,
    span: Span,
) -> Result<Vec<f64>, ParseError> {
    match entries {
        ParsedEntries::Numbers(values) => Ok(values),
        ParsedEntries::Detailed(entries) => {
            if axes.is_empty() {
                return Err(ParseError::at(
                    "Axes must be populated before curves for reference entries",
                    span,
                ));
            }
            let mut values = Vec::with_capacity(axes.len());
            for axis in axes {
                let entry = entries.iter().find(|entry| entry.axis == axis.name);
                let Some(entry) = entry else {
                    return Err(ParseError::at(
                        format!("Missing entry for axis {}", axis.label),
                        span,
                    ));
                };
                values.push(entry.value);
            }
            Ok(values)
        }
    }
}
