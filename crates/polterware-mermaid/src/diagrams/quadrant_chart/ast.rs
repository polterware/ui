//! The quadrant chart database after parsing: what upstream's `quadrantDb.ts` holds once the
//! grammar has run, i.e. the `QuadrantBuilder` data `getQuadrantData` builds from
//! (`quadrantBuilder.ts` `QuadrantBuilderData` and `classes`), plus the common accessibility
//! getters (`getDiagramTitle`, `getAccTitle`, `getAccDescription`).
//!
//! `getQuadrantData` merges the chart configuration and the theme, then calls
//! `quadrantBuilder.build()`, which computes every coordinate: that is layout, out of scope
//! here. What remains is the data the builder was given: the title, the four quadrant labels,
//! the four axis labels, the points, and the classes.
//!
//! Two upstream details are observable in this data:
//!
//! * `addPoints` prepends (`this.data.points = [...points, ...this.data.points]`), so
//!   [`Ast::points`] is in reverse source order, as the builder's `data.points` is.
//! * `addClass` writes to a `Map`: a class name declared twice keeps its first position and
//!   takes the last styles. [`Ast::classes`] is that map in insertion order, with
//!   [`Ast::class`] as the `get` lookup.
//!
//! A point's `x` and `y` stay the token text the grammar handed to `addPoint` (`"0.1"`), as
//! upstream's `QuadrantPointInputType` does at runtime; the number is produced by
//! `scaleLinear` in `getQuadrantPoints`, which is layout.

/// The four style keys `parseStyles` accepts, upstream's `StylesObject`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Styles {
    /// `radius`, `parseInt` of a run of digits: a JavaScript number.
    pub radius: Option<f64>,
    /// `color`, a hex code with or without its `#`.
    pub color: Option<String>,
    /// `stroke-color`, a hex code with or without its `#`.
    pub stroke_color: Option<String>,
    /// `stroke-width`, a run of digits followed by `px`.
    pub stroke_width: Option<String>,
}

/// One point (`QuadrantPointInputType` in `quadrantBuilder.ts`).
#[derive(Clone, Debug, PartialEq)]
pub struct Point {
    /// The `point_x` token text, e.g. `"0.1"`; `scaleLinear` reads it as a number later.
    pub x: String,
    /// The `point_y` token text, e.g. `"0.4"`.
    pub y: String,
    /// The point label, trimmed and sanitized.
    pub text: String,
    /// The `class_name` after `:::`, empty when the point has no class.
    pub class_name: String,
    /// The styles the point declared; upstream spreads them into the point object.
    pub styles: Styles,
}

/// One `classDef` (`Map<string, StylesObject>` entry in the builder).
#[derive(Clone, Debug, PartialEq)]
pub struct ClassStyle {
    pub name: String,
    pub styles: Styles,
}

/// What upstream's quadrant chart database holds after parsing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ast {
    /// `getDiagramTitle()`; empty when the diagram has no title.
    pub diagram_title: String,
    /// `getAccTitle()`; empty when the diagram has no accessible title.
    pub acc_title: String,
    /// `getAccDescription()`; empty when the diagram has no accessible description.
    pub acc_description: String,
    /// `quadrant1Text`, the top right label; empty when the line is absent.
    pub quadrant1_text: String,
    /// `quadrant2Text`, the top left label.
    pub quadrant2_text: String,
    /// `quadrant3Text`, the bottom left label.
    pub quadrant3_text: String,
    /// `quadrant4Text`, the bottom right label.
    pub quadrant4_text: String,
    /// `xAxisLeftText`.
    pub x_axis_left_text: String,
    /// `xAxisRightText`.
    pub x_axis_right_text: String,
    /// `yAxisBottomText`.
    pub y_axis_bottom_text: String,
    /// `yAxisTopText`.
    pub y_axis_top_text: String,
    /// `points`, newest first, because `addPoints` prepends.
    pub points: Vec<Point>,
    /// `classes`, in insertion order.
    pub classes: Vec<ClassStyle>,
}

impl Ast {
    /// The class styles with this name (`classes.get(name)`).
    pub fn class(&self, name: &str) -> Option<&Styles> {
        self.classes
            .iter()
            .find(|class| class.name == name)
            .map(|class| &class.styles)
    }
}
