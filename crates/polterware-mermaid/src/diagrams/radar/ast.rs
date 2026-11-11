//! The radar database after parsing: what upstream's `RadarDB` getters return (`db.ts`
//! `getAxes`, `getCurves`, `getOptions`, `getDiagramTitle`, `getAccTitle`,
//! `getAccDescription`).
//!
//! `getConfig` is left out because this crate owns no configuration. The axis and curve
//! labels are already resolved the way `setAxes` and `setCurves` resolve them: a missing
//! label falls back to the name.

/// One axis (`RadarAxis` in `types.ts`).
#[derive(Clone, Debug, PartialEq)]
pub struct Axis {
    /// `name`, the axis id the curves reference.
    pub name: String,
    /// `label`, the quoted label or the name when the axis has none.
    pub label: String,
}

/// One data series (`RadarCurve` in `types.ts`).
#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    pub name: String,
    /// The quoted label or the name when the curve has none.
    pub label: String,
    /// One value per axis, in axis order, after `computeCurveEntries` reordered the entries.
    pub entries: Vec<f64>,
}

/// The graticule shape (`'circle' | 'polygon'` in `types.ts`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Graticule {
    #[default]
    Circle,
    Polygon,
}

/// The rendering options (`RadarOptions` in `types.ts`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Options {
    /// `showLegend`, true by default.
    pub show_legend: bool,
    /// `ticks`, 5 by default and clamped to `MAX_TICKS` (32).
    ///
    /// Upstream keeps a JavaScript number here, so a fractional `ticks` survives parsing.
    pub ticks: f64,
    /// `max`, `null` by default.
    pub max: Option<f64>,
    /// `min`, 0 by default.
    pub min: f64,
    /// `graticule`, a circle by default.
    pub graticule: Graticule,
}

impl Default for Options {
    /// `defaultOptions` in `db.ts`.
    fn default() -> Self {
        Self {
            show_legend: true,
            ticks: 5.0,
            max: None,
            min: 0.0,
            graticule: Graticule::Circle,
        }
    }
}

/// What upstream's radar database holds after parsing one document.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ast {
    /// `getAxes()`, in declaration order.
    pub axes: Vec<Axis>,
    /// `getCurves()`, in declaration order.
    pub curves: Vec<Curve>,
    /// `getOptions()`, with the defaults filled in.
    pub options: Options,
    /// `getDiagramTitle()`; empty when the diagram has no title.
    pub diagram_title: String,
    /// `getAccTitle()`; empty when the diagram has no accessible title.
    pub acc_title: String,
    /// `getAccDescription()`; empty when the diagram has no accessible description.
    pub acc_description: String,
}
