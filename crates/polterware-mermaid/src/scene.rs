//! What a laid-out diagram asks a painter to draw, without SVG.
//!
//! Upstream renders every diagram as an SVG DOM: `rendering-util/rendering-elements/*` and
//! each diagram's own renderer (`sequence/svgDraw.js`, `pie/pieRenderer.ts`,
//! `gantt/ganttRenderer.js`, ...) append `<g>`, `<rect>`, `<circle>`, `<path>`, `<text>`/`<tspan>`,
//! `<image>`, `<marker>` and `<defs>` nodes and let CSS and `getBBox()` finish the layout. The
//! painter here is GPUI: it fills and strokes paths, draws rounded rectangles, shapes and text
//! runs, and clips to rectangles, so this module is the hand-off between the two.
//!
//! A [`Scene`] is a tree of [`Item`]s (groups, paths, rectangles, ellipses, text, images) in
//! diagram coordinates, with the whole drawing's [`Scene::bounds`] and the fitted
//! [`Scene::view_box`]. Coordinates keep upstream's SVG convention: y grows down, lengths are
//! in user units (px at zoom 1), and a group's [`Transform`] is the SVG `transform` matrix.
//!
//! The pieces of SVG that have no faithful painter equivalent are resolved here instead of
//! deferred:
//!
//! - Text is already wrapped and split into lines of styled spans ([`TextLine`]), so the
//!   painter never measures glyphs to decide a break. `createText.ts`'s `foreignObject`
//!   HTML-label path is not represented: a span is text plus bold/italic/code/link flags.
//! - Markers are reusable [`MarkerDef`] path definitions in [`Scene::defs`], referenced from a
//!   path's [`PathShape::start_markers`] / [`PathShape::end_markers`] with an orientation; the
//!   painter places them at the path ends, as SVG's `marker-start`/`marker-end` does.
//! - A clip is one rectangle in the group's local coordinates, because GPUI clips to
//!   rectangles (SVG `clipPath` is general).
//! - Colors are the CSS strings upstream's theme variables and compiled styles carry
//!   (`#ECECFF`, `transparent`, ...); the painter parses them against its own palette.
//!
//! Every element carries an optional [`SourceRef`] naming the diagram node, edge, or cluster
//! it belongs to, so a hit test can select the diagram element that drew the pixel and an
//! editor can map a click back to its source.

use serde::{Deserialize, Serialize};

use crate::measure::Font;

/// Default font weight, `calculateTextDimensions`' `fontWeight` default.
pub const NORMAL_WEIGHT: u16 = 400;
/// Weight of a `strong` markdown span (`createText.ts` writes `font-weight: bold`).
pub const BOLD_WEIGHT: u16 = 700;

/// A point in diagram coordinates. The fields are also the serialized names, so a path
/// command reads as `{"op":"lineTo","x":10,"y":20}`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const ZERO: Point = Point { x: 0.0, y: 0.0 };

    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn translated(self, dx: f64, dy: f64) -> Self {
        Self::new(self.x + dx, self.y + dy)
    }

    /// Rotates the point clockwise around the origin, in degrees (SVG's positive rotation).
    pub fn rotated(self, degrees: f64) -> Self {
        let radians = degrees.to_radians();
        let (sin, cos) = radians.sin_cos();
        Self::new(self.x * cos - self.y * sin, self.x * sin + self.y * cos)
    }

    pub fn distance_to(self, other: Point) -> f64 {
        (self.x - other.x).hypot(self.y - other.y)
    }
}

impl From<(f64, f64)> for Point {
    fn from((x, y): (f64, f64)) -> Self {
        Self::new(x, y)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

impl Size {
    pub const ZERO: Size = Size {
        width: 0.0,
        height: 0.0,
    };

    pub const fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }
}

/// An axis-aligned rectangle. `x`/`y` are the top-left corner, as SVG's `<rect>`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub const ZERO: Rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 0.0,
    };

    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// The rectangle around a center with the given size, which is how upstream places every
    /// node's label box and `<rect>` shape.
    pub fn centered(center: Point, size: Size) -> Self {
        Self::new(
            center.x - size.width / 2.0,
            center.y - size.height / 2.0,
            size.width,
            size.height,
        )
    }

    pub fn size(self) -> Size {
        Size::new(self.width, self.height)
    }

    pub fn center(self) -> Point {
        Point::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    pub fn right(self) -> f64 {
        self.x + self.width
    }

    pub fn bottom(self) -> f64 {
        self.y + self.height
    }

    /// Grows the rectangle by `padding` on every side (`setupViewPortForSVG.ts` adds the SVG
    /// padding twice, once per axis).
    pub fn expanded(self, padding: f64) -> Self {
        Self::new(
            self.x - padding,
            self.y - padding,
            self.width + 2.0 * padding,
            self.height + 2.0 * padding,
        )
    }

    /// The smallest rectangle containing both.
    pub fn union(self, other: Rect) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = self.right().max(other.right());
        let bottom = self.bottom().max(other.bottom());
        Self::new(x, y, right - x, bottom - y)
    }

    pub fn contains(self, point: Point) -> bool {
        point.x >= self.x
            && point.x <= self.right()
            && point.y >= self.y
            && point.y <= self.bottom()
    }
}

/// An SVG `transform` matrix: `matrix(a b c d e f)` applies
/// `(x', y') = (a·x + c·y + e, b·x + d·y + f)`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Transform {
    pub const IDENTITY: Transform = Transform {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    pub const fn translate(x: f64, y: f64) -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: x,
            f: y,
        }
    }

    pub const fn scale(x: f64, y: f64) -> Self {
        Self {
            a: x,
            b: 0.0,
            c: 0.0,
            d: y,
            e: 0.0,
            f: 0.0,
        }
    }

    /// Clockwise rotation in degrees, around the origin.
    pub fn rotate(degrees: f64) -> Self {
        let (sin, cos) = degrees.to_radians().sin_cos();
        Self {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            e: 0.0,
            f: 0.0,
        }
    }

    /// Clockwise rotation in degrees around `center`, the shape of the swimlane cluster label's
    /// `translate(...) rotate(-90) translate(...)`.
    pub fn rotate_about(degrees: f64, center: Point) -> Self {
        Transform::translate(-center.x, -center.y)
            .then(Transform::rotate(degrees))
            .then(Transform::translate(center.x, center.y))
    }

    /// Composes two transforms the way SVG nests groups: `self` applies first, then `other`.
    pub fn then(self, other: Transform) -> Self {
        Self {
            a: other.a * self.a + other.c * self.b,
            b: other.b * self.a + other.d * self.b,
            c: other.a * self.c + other.c * self.d,
            d: other.b * self.c + other.d * self.d,
            e: other.a * self.e + other.c * self.f + other.e,
            f: other.b * self.e + other.d * self.f + other.f,
        }
    }

    pub fn apply(self, point: Point) -> Point {
        Point::new(
            self.a * point.x + self.c * point.y + self.e,
            self.b * point.x + self.d * point.y + self.f,
        )
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// A CSS color string, exactly as upstream's theme variables and compiled styles write it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Color(pub String);

impl Color {
    pub fn new(css: impl Into<String>) -> Self {
        Self(css.into())
    }

    /// SVG's `none`: nothing painted.
    pub fn none() -> Self {
        Self::new("none")
    }

    /// CSS `currentColor`, which a marker inherits from the path that references it.
    pub fn current() -> Self {
        Self::new("currentColor")
    }
}

impl From<&str> for Color {
    fn from(css: &str) -> Self {
        Self::new(css)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LineCap {
    #[default]
    Butt,
    Round,
    Square,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LineJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}

/// How a path is painted: SVG's `fill`, `stroke`, `stroke-width`, `stroke-dasharray`,
/// `stroke-linecap`, `stroke-linejoin`, and `opacity`, with the fill and stroke opacities kept
/// apart because compiled mermaid styles set them independently.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PathStyle {
    pub fill: Option<Color>,
    pub fill_opacity: f64,
    pub stroke: Option<Color>,
    pub stroke_opacity: f64,
    pub stroke_width: f64,
    /// `stroke-dasharray`; empty is a solid line.
    pub dash: Vec<f64>,
    pub line_cap: LineCap,
    pub line_join: LineJoin,
    pub opacity: f64,
}

impl PathStyle {
    /// SVG's default paint: black fill, no stroke, opaque.
    pub fn filled(fill: impl Into<Color>) -> Self {
        Self {
            fill: Some(fill.into()),
            ..Self::default()
        }
    }

    /// A stroked, unfilled path, the shape of every edge and most node outlines.
    pub fn stroked(stroke: impl Into<Color>, width: f64) -> Self {
        Self {
            fill: None,
            stroke: Some(stroke.into()),
            stroke_width: width,
            ..Self::default()
        }
    }

    pub fn with_fill(mut self, fill: impl Into<Color>) -> Self {
        self.fill = Some(fill.into());
        self
    }

    pub fn with_stroke(mut self, stroke: impl Into<Color>, width: f64) -> Self {
        self.stroke = Some(stroke.into());
        self.stroke_width = width;
        self
    }

    pub fn with_dash(mut self, dash: impl IntoIterator<Item = f64>) -> Self {
        self.dash = dash.into_iter().collect();
        self
    }

    pub fn with_opacity(mut self, opacity: f64) -> Self {
        self.opacity = opacity;
        self
    }
}

impl Default for PathStyle {
    fn default() -> Self {
        Self {
            fill: Some(Color::new("#000000")),
            fill_opacity: 1.0,
            stroke: None,
            stroke_opacity: 1.0,
            stroke_width: 1.0,
            dash: Vec::new(),
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
            opacity: 1.0,
        }
    }
}

/// One SVG path command. The arithmetic is the painter's; this enum is the `d` attribute
/// without its string form, covering what upstream emits: straight segments, d3's cubic and
/// quadratic curves, `a` arcs (`pieRenderer.ts`, the cylinder and folder shapes), and `z`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum PathCommand {
    MoveTo {
        x: f64,
        y: f64,
    },
    LineTo {
        x: f64,
        y: f64,
    },
    /// `C`: two control points and the end point.
    CubicTo {
        c1x: f64,
        c1y: f64,
        c2x: f64,
        c2y: f64,
        x: f64,
        y: f64,
    },
    /// `Q`: one control point and the end point.
    QuadTo {
        cx: f64,
        cy: f64,
        x: f64,
        y: f64,
    },
    /// `A`: an elliptical arc. `rotation` is the x-axis rotation in degrees, and the radii are
    /// as written (SVG scales them up when they cannot span the endpoints).
    ArcTo {
        rx: f64,
        ry: f64,
        rotation: f64,
        large_arc: bool,
        sweep: bool,
        x: f64,
        y: f64,
    },
    /// `Z`.
    Close,
}

impl PathCommand {
    pub fn move_to(point: Point) -> Self {
        Self::MoveTo {
            x: point.x,
            y: point.y,
        }
    }

    pub fn line_to(point: Point) -> Self {
        Self::LineTo {
            x: point.x,
            y: point.y,
        }
    }

    pub fn cubic_to(control1: Point, control2: Point, to: Point) -> Self {
        Self::CubicTo {
            c1x: control1.x,
            c1y: control1.y,
            c2x: control2.x,
            c2y: control2.y,
            x: to.x,
            y: to.y,
        }
    }

    pub fn quad_to(control: Point, to: Point) -> Self {
        Self::QuadTo {
            cx: control.x,
            cy: control.y,
            x: to.x,
            y: to.y,
        }
    }

    pub fn arc_to(
        rx: f64,
        ry: f64,
        rotation: f64,
        large_arc: bool,
        sweep: bool,
        to: Point,
    ) -> Self {
        Self::ArcTo {
            rx,
            ry,
            rotation,
            large_arc,
            sweep,
            x: to.x,
            y: to.y,
        }
    }

    /// The end point of the command, when it moves the pen.
    pub fn end_point(self) -> Option<Point> {
        match self {
            PathCommand::MoveTo { x, y }
            | PathCommand::LineTo { x, y }
            | PathCommand::CubicTo { x, y, .. }
            | PathCommand::QuadTo { x, y, .. }
            | PathCommand::ArcTo { x, y, .. } => Some(Point::new(x, y)),
            PathCommand::Close => None,
        }
    }
}

/// Path geometry, the `d` attribute as data.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Path {
    pub commands: Vec<PathCommand>,
}

impl Path {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    pub fn move_to(&mut self, point: Point) -> &mut Self {
        self.commands.push(PathCommand::move_to(point));
        self
    }

    pub fn line_to(&mut self, point: Point) -> &mut Self {
        self.commands.push(PathCommand::line_to(point));
        self
    }

    pub fn cubic_to(&mut self, control1: Point, control2: Point, to: Point) -> &mut Self {
        self.commands
            .push(PathCommand::cubic_to(control1, control2, to));
        self
    }

    pub fn quad_to(&mut self, control: Point, to: Point) -> &mut Self {
        self.commands.push(PathCommand::quad_to(control, to));
        self
    }

    pub fn arc_to(
        &mut self,
        rx: f64,
        ry: f64,
        rotation: f64,
        large_arc: bool,
        sweep: bool,
        to: Point,
    ) -> &mut Self {
        self.commands
            .push(PathCommand::arc_to(rx, ry, rotation, large_arc, sweep, to));
        self
    }

    pub fn close(&mut self) -> &mut Self {
        self.commands.push(PathCommand::Close);
        self
    }

    /// `createPathFromPoints`: `M p0 L p1 L p2 Z`, upstream's polygon.
    pub fn polygon(points: impl IntoIterator<Item = Point>) -> Self {
        let mut path = Path::new();
        for (index, point) in points.into_iter().enumerate() {
            if index == 0 {
                path.move_to(point);
            } else {
                path.line_to(point);
            }
        }
        path.close();
        path
    }

    /// An open `M p0 L p1 ...` polyline, the d3 `curveLinear` edge.
    pub fn polyline(points: impl IntoIterator<Item = Point>) -> Self {
        let mut path = Path::new();
        for (index, point) in points.into_iter().enumerate() {
            if index == 0 {
                path.move_to(point);
            } else {
                path.line_to(point);
            }
        }
        path
    }

    /// SVG's `<rect>` as a path, with elliptical corners of `rx`/`ry` (rounded rectangles).
    pub fn rect(rect: Rect, rx: f64, ry: f64) -> Self {
        let mut path = Path::new();
        if rx <= 0.0 || ry <= 0.0 {
            path.move_to(Point::new(rect.x, rect.y));
            path.line_to(Point::new(rect.right(), rect.y));
            path.line_to(Point::new(rect.right(), rect.bottom()));
            path.line_to(Point::new(rect.x, rect.bottom()));
            path.close();
            return path;
        }
        let rx = rx.min(rect.width / 2.0);
        let ry = ry.min(rect.height / 2.0);
        path.move_to(Point::new(rect.x + rx, rect.y));
        path.line_to(Point::new(rect.right() - rx, rect.y));
        path.arc_to(
            rx,
            ry,
            0.0,
            false,
            true,
            Point::new(rect.right(), rect.y + ry),
        );
        path.line_to(Point::new(rect.right(), rect.bottom() - ry));
        path.arc_to(
            rx,
            ry,
            0.0,
            false,
            true,
            Point::new(rect.right() - rx, rect.bottom()),
        );
        path.line_to(Point::new(rect.x + rx, rect.bottom()));
        path.arc_to(
            rx,
            ry,
            0.0,
            false,
            true,
            Point::new(rect.x, rect.bottom() - ry),
        );
        path.line_to(Point::new(rect.x, rect.y + ry));
        path.arc_to(rx, ry, 0.0, false, true, Point::new(rect.x + rx, rect.y));
        path.close();
        path
    }
}

/// The diagram element an item belongs to: a node, an edge, or a cluster. `id` is the
/// diagram's own id (the flowchart node id, the edge id, the subgraph id), so a painter can
/// select it and an editor can print an edit back to the text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRef {
    pub kind: SourceKind,
    pub id: String,
}

impl SourceRef {
    pub fn node(id: impl Into<String>) -> Self {
        Self {
            kind: SourceKind::Node,
            id: id.into(),
        }
    }

    pub fn edge(id: impl Into<String>) -> Self {
        Self {
            kind: SourceKind::Edge,
            id: id.into(),
        }
    }

    pub fn cluster(id: impl Into<String>) -> Self {
        Self {
            kind: SourceKind::Cluster,
            id: id.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceKind {
    Node,
    Edge,
    Cluster,
}

/// A reusable marker definition: SVG's `<marker>` in `<defs>`. Upstream's `markers.js`
/// defines arrowheads, circles, crosses, composition/aggregation diamonds, and the ER and
/// class cardinality markers this way, one per diagram type, and paints them from CSS
/// classes.
///
/// The marker's identity is its `id` and `class`; its look is `shapes`. Every upstream marker
/// maps onto those two: `point`/`extension` (the filled arrowhead), `openArrow`, `barb`,
/// `hierarchy`, `circle` and `hollowCircle`, `cross`, `composition` and `aggregation` (class
/// diagram diamonds), `dependency` and `lollipop`, the ER cardinality markers `only_one`,
/// `zero_or_one`, `one_or_more`, `zero_or_more` (and their `_neo` variants), and the
/// requirement markers `requirement_arrow` and `requirement_contains`. A `<polygon>` or
/// `<circle>` child becomes a [`PathShape`] in `shapes`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MarkerDef {
    /// The id a [`MarkerRef`] names.
    pub id: String,
    /// The marker's own coordinate system (SVG `viewBox`).
    pub view_box: Rect,
    /// `refX`/`refY`: the point of the marker that sits on the path end.
    pub reference: Point,
    /// `markerWidth`.
    pub width: f64,
    /// `markerHeight`.
    pub height: f64,
    pub units: MarkerUnits,
    pub orient: MarkerOrient,
    /// Upstream's marker class (`marker extension flowchart-v2`), which CSS keys on; kept so a
    /// painter can theme by marker kind.
    pub class: Option<String>,
    /// The painted shapes. `<path>`, `<polygon>` and `<circle>` children are all paths here.
    /// A [`Color::current`] fill or stroke takes the color of the path that places the marker.
    pub shapes: Vec<PathShape>,
}

impl MarkerDef {
    pub fn new(id: impl Into<String>, view_box: Rect) -> Self {
        Self {
            id: id.into(),
            view_box,
            ..Self::default()
        }
    }

    pub fn with_reference(mut self, reference: Point) -> Self {
        self.reference = reference;
        self
    }

    pub fn with_size(mut self, width: f64, height: f64) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn with_orient(mut self, orient: MarkerOrient) -> Self {
        self.orient = orient;
        self
    }

    pub fn with_class(mut self, class: impl Into<String>) -> Self {
        self.class = Some(class.into());
        self
    }

    pub fn push(&mut self, shape: PathShape) {
        self.shapes.push(shape);
    }
}

impl Default for MarkerDef {
    fn default() -> Self {
        Self {
            id: String::new(),
            view_box: Rect::ZERO,
            reference: Point::ZERO,
            width: 3.0,
            height: 3.0,
            units: MarkerUnits::default(),
            orient: MarkerOrient::default(),
            class: None,
            shapes: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MarkerUnits {
    #[default]
    StrokeWidth,
    UserSpaceOnUse,
}

/// SVG `orient`: `auto` turns the marker with the path's tangent at the vertex; each marker in
/// upstream's defs sets it, and the cardinality markers rely on it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MarkerOrient {
    #[default]
    Auto,
    AutoStartReverse,
    Fixed {
        degrees: f64,
    },
}

/// A marker placed on a path's start or end, SVG's `marker-start`/`marker-end`. `color` is the
/// paint for the marker's [`Color::current`] shapes: the referencing path's stroke, as CSS
/// would give it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MarkerRef {
    /// The [`MarkerDef::id`] this placement draws.
    pub def: String,
    pub color: Option<Color>,
}

impl MarkerRef {
    pub fn new(def: impl Into<String>) -> Self {
        Self {
            def: def.into(),
            color: None,
        }
    }

    pub fn with_color(mut self, color: impl Into<Color>) -> Self {
        self.color = Some(color.into());
        self
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextAnchor {
    #[default]
    Start,
    Middle,
    End,
}

/// SVG `dominant-baseline`, the values the renderers use (`svgDraw.js` sets `middle` and
/// `central`). `Alphabetic` is SVG's default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextBaseline {
    #[default]
    Alphabetic,
    Middle,
    Central,
    Hanging,
    Top,
    Bottom,
}

/// One run of text inside a line, carrying `createText.ts`'s inner-tspan styling. `code` and
/// `link` are part of the model even though `handle-markdown-text.ts` only emits bold and
/// italic: a renderer that reads code spans or links has one place to put them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextSpan {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub link: Option<String>,
}

impl TextSpan {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    pub fn bold(text: impl Into<String>) -> Self {
        Self {
            bold: true,
            ..Self::plain(text)
        }
    }

    pub fn italic(text: impl Into<String>) -> Self {
        Self {
            italic: true,
            ..Self::plain(text)
        }
    }

    pub fn code(text: impl Into<String>) -> Self {
        Self {
            code: true,
            ..Self::plain(text)
        }
    }

    pub fn link(mut self, href: impl Into<String>) -> Self {
        self.link = Some(href.into());
        self
    }

    /// The span's text in the given style, for merge tests and painters.
    pub fn is_styled_like(&self, other: &TextSpan) -> bool {
        self.bold == other.bold
            && self.italic == other.italic
            && self.code == other.code
            && self.link == other.link
    }
}

/// A resolved line: its spans already carry the joining spaces between words, so a painter
/// draws them left to right without inserting anything.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextLine {
    pub spans: Vec<TextSpan>,
}

impl TextLine {
    pub fn new(spans: Vec<TextSpan>) -> Self {
        Self { spans }
    }

    pub fn text(&self) -> String {
        self.spans.iter().map(|span| span.text.as_str()).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }
}

/// Wrapped text: the lines the painter draws and the box they occupy. `line_height` is the
/// distance between baselines (`createFormattedText` uses 1.1em).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextBlock {
    pub lines: Vec<TextLine>,
    pub size: Size,
    pub line_height: f64,
}

impl TextBlock {
    pub fn new(lines: Vec<TextLine>, size: Size, line_height: f64) -> Self {
        Self {
            lines,
            size,
            line_height,
        }
    }
}

/// A text run: SVG's `<text>` with its `<tspan>` lines. The wrapping is already resolved
/// ([`crate::measure`] produced [`TextRun::lines`]); `wrapping_width` is kept so an editor can
/// re-wrap at the same width.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextRun {
    pub source: Option<SourceRef>,
    /// The anchor point, SVG's `<text x>`/`<text y>` (the first line's baseline).
    pub position: Point,
    pub anchor: TextAnchor,
    pub baseline: TextBaseline,
    /// Clockwise rotation in degrees around `position` (SVG `rotate`), used by the swimlane
    /// cluster title.
    pub rotation: f64,
    pub font: Font,
    pub fill: Option<Color>,
    pub opacity: f64,
    /// Distance between baselines in px.
    pub line_height: f64,
    pub lines: Vec<TextLine>,
    /// The measured box, for label placement and hit testing.
    pub size: Size,
    pub wrapping_width: Option<f64>,
}

impl TextRun {
    /// A run at `position` with an already measured block, anchored at the start of its
    /// alphabetic baseline like SVG's default.
    pub fn new(position: Point, font: Font, block: TextBlock) -> Self {
        Self {
            source: None,
            position,
            anchor: TextAnchor::Start,
            baseline: TextBaseline::Alphabetic,
            rotation: 0.0,
            font,
            fill: None,
            opacity: 1.0,
            line_height: block.line_height,
            lines: block.lines,
            size: block.size,
            wrapping_width: None,
        }
    }

    /// Upstream's edge labels: `createText` centers when `isNode` is false
    /// (`text-anchor: middle`), node labels start at x=0.
    pub fn centered(position: Point, font: Font, block: TextBlock) -> Self {
        Self {
            anchor: TextAnchor::Middle,
            ..Self::new(position, font, block)
        }
    }

    pub fn with_source(mut self, source: SourceRef) -> Self {
        self.source = Some(source);
        self
    }

    pub fn with_fill(mut self, fill: impl Into<Color>) -> Self {
        self.fill = Some(fill.into());
        self
    }

    pub fn with_rotation(mut self, degrees: f64) -> Self {
        self.rotation = degrees;
        self
    }

    /// The box of the run in its own coordinates: `size` (the measured block) placed under
    /// `position` for the anchor and baseline. Rotated runs are the painter's to transform.
    pub fn bounds(&self) -> Rect {
        let x = match self.anchor {
            TextAnchor::Start => self.position.x,
            TextAnchor::Middle => self.position.x - self.size.width / 2.0,
            TextAnchor::End => self.position.x - self.size.width,
        };
        let baseline = self.position.y;
        let y = match self.baseline {
            TextBaseline::Alphabetic | TextBaseline::Bottom => baseline - self.size.height,
            TextBaseline::Middle | TextBaseline::Central => baseline - self.size.height / 2.0,
            TextBaseline::Hanging | TextBaseline::Top => baseline,
        };
        Rect::new(x, y, self.size.width, self.size.height)
    }
}

impl Default for TextRun {
    fn default() -> Self {
        Self {
            source: None,
            position: Point::ZERO,
            anchor: TextAnchor::Start,
            baseline: TextBaseline::Alphabetic,
            rotation: 0.0,
            font: Font::default(),
            fill: None,
            opacity: 1.0,
            line_height: 0.0,
            lines: Vec::new(),
            size: Size::ZERO,
            wrapping_width: None,
        }
    }
}

/// A stroked or filled path with optional end markers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PathShape {
    pub source: Option<SourceRef>,
    pub path: Path,
    pub style: PathStyle,
    pub fill_rule: FillRule,
    pub start_markers: Vec<MarkerRef>,
    pub end_markers: Vec<MarkerRef>,
}

impl PathShape {
    pub fn new(path: Path, style: PathStyle) -> Self {
        Self {
            source: None,
            path,
            style,
            ..Self::default()
        }
    }

    pub fn with_source(mut self, source: SourceRef) -> Self {
        self.source = Some(source);
        self
    }

    pub fn with_end_marker(mut self, marker: MarkerRef) -> Self {
        self.end_markers.push(marker);
        self
    }

    pub fn with_start_marker(mut self, marker: MarkerRef) -> Self {
        self.start_markers.push(marker);
        self
    }
}

impl Default for PathShape {
    fn default() -> Self {
        Self {
            source: None,
            path: Path::new(),
            style: PathStyle::default(),
            fill_rule: FillRule::default(),
            start_markers: Vec::new(),
            end_markers: Vec::new(),
        }
    }
}

/// SVG's `<rect rx ry>`, the shape of node boxes, note edges, and clusters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RectShape {
    pub source: Option<SourceRef>,
    pub rect: Rect,
    /// The corner radii; equal values are a rounded rectangle, `0` a square one.
    pub rx: f64,
    pub ry: f64,
    pub style: PathStyle,
}

impl RectShape {
    pub fn new(rect: Rect, radius: f64, style: PathStyle) -> Self {
        Self {
            source: None,
            rect,
            rx: radius,
            ry: radius,
            style,
        }
    }

    pub fn with_source(mut self, source: SourceRef) -> Self {
        self.source = Some(source);
        self
    }
}

impl Default for RectShape {
    fn default() -> Self {
        Self {
            source: None,
            rect: Rect::ZERO,
            rx: 0.0,
            ry: 0.0,
            style: PathStyle::default(),
        }
    }
}

/// SVG's `<circle>`/`<ellipse>`: actor heads, double circles, the pie's ring, ER attribute
/// keys, and the filled start/end states.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EllipseShape {
    pub source: Option<SourceRef>,
    pub center: Point,
    pub radius_x: f64,
    pub radius_y: f64,
    pub style: PathStyle,
}

impl EllipseShape {
    pub fn new(center: Point, radius_x: f64, radius_y: f64, style: PathStyle) -> Self {
        Self {
            source: None,
            center,
            radius_x,
            radius_y,
            style,
        }
    }

    pub fn with_source(mut self, source: SourceRef) -> Self {
        self.source = Some(source);
        self
    }
}

impl Default for EllipseShape {
    fn default() -> Self {
        Self {
            source: None,
            center: Point::ZERO,
            radius_x: 0.0,
            radius_y: 0.0,
            style: PathStyle::default(),
        }
    }
}

/// SVG's `<image>`: an icon (`icon: fa:fa-user`, a registered icon pack, BPMN glyphs) or an
/// image node. The painter resolves `name` in its own asset table; the scene carries no bytes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImageRef {
    pub source: Option<SourceRef>,
    /// The named asset: an icon pack name (`fa:fa-bell`) or an image URL/path.
    pub name: String,
    pub rect: Rect,
    pub opacity: f64,
    /// A monochrome mark: `name` is an SVG asset path the painter draws in this color.
    pub tint: Option<Color>,
}

impl ImageRef {
    pub fn new(name: impl Into<String>, rect: Rect) -> Self {
        Self {
            source: None,
            name: name.into(),
            rect,
            opacity: 1.0,
            tint: None,
        }
    }

    pub fn with_source(mut self, source: SourceRef) -> Self {
        self.source = Some(source);
        self
    }

    /// Draws the named SVG asset as a monochrome mark in `color`, like an icon.
    pub fn tinted(mut self, color: Color) -> Self {
        self.tint = Some(color);
        self
    }
}

impl Default for ImageRef {
    fn default() -> Self {
        Self {
            source: None,
            name: String::new(),
            rect: Rect::ZERO,
            opacity: 1.0,
            tint: None,
        }
    }
}

/// A group: SVG's `<g>`. `clip` is a rectangle in the group's coordinates (before
/// `transform`), and `opacity` multiplies everything inside.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Group {
    /// The SVG id (`legend`, `flowchart-A-3`), when upstream sets one.
    pub id: Option<String>,
    pub source: Option<SourceRef>,
    /// Upstream's CSS classes (`node default`, `cluster`, `edgeLabel`), for theming.
    pub class: Option<String>,
    pub transform: Transform,
    pub clip: Option<Rect>,
    pub opacity: f64,
    pub items: Vec<Item>,
}

impl Group {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn translated(x: f64, y: f64) -> Self {
        Self {
            transform: Transform::translate(x, y),
            ..Self::default()
        }
    }

    pub fn with_source(mut self, source: SourceRef) -> Self {
        self.source = Some(source);
        self
    }

    pub fn with_transform(mut self, transform: Transform) -> Self {
        self.transform = transform;
        self
    }

    pub fn with_class(mut self, class: impl Into<String>) -> Self {
        self.class = Some(class.into());
        self
    }

    pub fn with_clip(mut self, clip: Rect) -> Self {
        self.clip = Some(clip);
        self
    }

    pub fn push(&mut self, item: impl Into<Item>) {
        self.items.push(item.into());
    }
}

impl Default for Group {
    fn default() -> Self {
        Self {
            id: None,
            source: None,
            class: None,
            transform: Transform::IDENTITY,
            clip: None,
            opacity: 1.0,
            items: Vec::new(),
        }
    }
}

impl From<Group> for Item {
    fn from(group: Group) -> Self {
        Item::Group(group)
    }
}

impl From<PathShape> for Item {
    fn from(shape: PathShape) -> Self {
        Item::Path(shape)
    }
}

impl From<RectShape> for Item {
    fn from(shape: RectShape) -> Self {
        Item::Rect(shape)
    }
}

impl From<EllipseShape> for Item {
    fn from(shape: EllipseShape) -> Self {
        Item::Ellipse(shape)
    }
}

impl From<TextRun> for Item {
    fn from(text: TextRun) -> Self {
        Item::Text(text)
    }
}

impl From<ImageRef> for Item {
    fn from(image: ImageRef) -> Self {
        Item::Image(image)
    }
}

/// Anything a painter draws.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Item {
    Group(Group),
    Path(PathShape),
    Rect(RectShape),
    Ellipse(EllipseShape),
    Text(TextRun),
    Image(ImageRef),
}

impl Item {
    pub fn source(&self) -> Option<&SourceRef> {
        match self {
            Item::Group(group) => group.source.as_ref(),
            Item::Path(path) => path.source.as_ref(),
            Item::Rect(rect) => rect.source.as_ref(),
            Item::Ellipse(ellipse) => ellipse.source.as_ref(),
            Item::Text(text) => text.source.as_ref(),
            Item::Image(image) => image.source.as_ref(),
        }
    }

    pub fn set_source(&mut self, source: Option<SourceRef>) {
        match self {
            Item::Group(group) => group.source = source,
            Item::Path(path) => path.source = source,
            Item::Rect(rect) => rect.source = source,
            Item::Ellipse(ellipse) => ellipse.source = source,
            Item::Text(text) => text.source = source,
            Item::Image(image) => image.source = source,
        }
    }
}

/// A complete drawing: the marker definitions, the item tree, and the box it fills.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Scene {
    /// SVG's `viewBox`: the visible window in diagram coordinates, content bounds plus padding
    /// (`setupViewPortForSVG.ts`).
    pub view_box: Rect,
    /// The content's bounds before padding, for scroll extents and fit-to-view.
    pub bounds: Rect,
    /// The theme's diagram background, when it has one.
    pub background: Option<Color>,
    /// Reusable marker definitions, SVG's `<defs>`.
    pub defs: Vec<MarkerDef>,
    pub items: Vec<Item>,
}

impl Scene {
    pub fn new(view_box: Rect) -> Self {
        Self {
            view_box,
            bounds: view_box,
            ..Self::default()
        }
    }

    /// The scene for content `bounds` with SVG's padding: `viewBox` is `bounds` grown by
    /// `padding` on every side, exactly `createViewBox(x - padding, y - padding, w + 2p, h + 2p)`.
    pub fn fitted(bounds: Rect, padding: f64) -> Self {
        Self {
            view_box: bounds.expanded(padding),
            bounds,
            ..Self::default()
        }
    }

    pub fn push(&mut self, item: impl Into<Item>) {
        self.items.push(item.into());
    }

    pub fn define(&mut self, marker: MarkerDef) {
        self.defs.push(marker);
    }

    pub fn marker_def(&self, id: &str) -> Option<&MarkerDef> {
        self.defs.iter().find(|def| def.id == id)
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Every source id in the tree, for building a hit-test index.
    pub fn source_refs(&self) -> Vec<&SourceRef> {
        fn walk<'a>(items: &'a [Item], out: &mut Vec<&'a SourceRef>) {
            for item in items {
                if let Some(source) = item.source() {
                    out.push(source);
                }
                if let Item::Group(group) = item {
                    walk(&group.items, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.items, &mut out);
        out
    }
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            view_box: Rect::ZERO,
            bounds: Rect::ZERO,
            background: None,
            defs: Vec::new(),
            items: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn transform_composes_in_svg_group_order() {
        let transform = Transform::translate(10.0, 20.0).then(Transform::scale(2.0, 2.0));
        assert_eq!(
            transform.apply(Point::new(1.0, 1.0)),
            Point::new(22.0, 42.0)
        );
    }

    #[test]
    fn rotate_about_turns_a_point_around_a_center() {
        let transform = Transform::rotate_about(-90.0, Point::new(5.0, 5.0));
        let rotated = transform.apply(Point::new(5.0, 0.0));
        assert!((rotated.x - 0.0).abs() < 1e-9);
        assert!((rotated.y - 5.0).abs() < 1e-9);
    }

    #[test]
    fn rect_path_rounds_corners_with_arcs() {
        let path = Path::rect(Rect::new(0.0, 0.0, 10.0, 10.0), 2.0, 2.0);
        assert_eq!(
            path.commands.first(),
            Some(&PathCommand::move_to(Point::new(2.0, 0.0)))
        );
        assert_eq!(
            path.commands
                .iter()
                .filter(|command| matches!(command, PathCommand::ArcTo { .. }))
                .count(),
            4
        );
        assert_eq!(path.commands.last(), Some(&PathCommand::Close));
    }

    #[test]
    fn rect_path_is_a_square_polygon_without_a_radius() {
        let path = Path::rect(Rect::new(0.0, 0.0, 10.0, 4.0), 0.0, 0.0);
        assert_eq!(path.commands.len(), 5);
        assert!(
            path.commands
                .iter()
                .all(|command| !matches!(command, PathCommand::ArcTo { .. }))
        );
    }

    #[test]
    fn polygon_closes_and_polyline_does_not() {
        let points = [
            Point::new(0.0, 0.0),
            Point::new(4.0, 0.0),
            Point::new(2.0, 3.0),
        ];
        assert_eq!(Path::polygon(points).commands.len(), 4);
        let polyline = Path::polyline(points);
        assert_eq!(polyline.commands.len(), 3);
        assert_eq!(polyline.commands[1], PathCommand::line_to(points[1]));
    }

    #[test]
    fn fitted_scene_expands_the_view_box_and_keeps_the_bounds() {
        let bounds = Rect::new(10.0, 20.0, 100.0, 50.0);
        let scene = Scene::fitted(bounds, 8.0);
        assert_eq!(scene.bounds, bounds);
        assert_eq!(scene.view_box, Rect::new(2.0, 12.0, 116.0, 66.0));
    }

    #[test]
    fn source_refs_walks_nested_groups() {
        let mut inner = Group::new().with_source(SourceRef::node("B"));
        inner.push(TextRun::default().with_source(SourceRef::edge("A-B")));
        let mut outer = Group::new().with_source(SourceRef::cluster("sub"));
        outer.push(inner);
        let mut scene = Scene::default();
        scene.push(outer);
        assert_eq!(
            scene.source_refs(),
            vec![
                &SourceRef::cluster("sub"),
                &SourceRef::node("B"),
                &SourceRef::edge("A-B"),
            ]
        );
    }

    #[test]
    fn centered_text_bounds_follow_anchor_and_baseline() {
        let block = TextBlock::new(Vec::new(), Size::new(20.0, 10.0), 13.2);
        let text = TextRun::centered(Point::new(50.0, 40.0), Font::default(), block);
        assert_eq!(text.bounds(), Rect::new(40.0, 30.0, 20.0, 10.0));
    }

    #[test]
    fn scene_round_trips_through_json() {
        let mut scene = Scene::fitted(Rect::new(0.0, 0.0, 10.0, 10.0), 8.0);
        scene.background = Some(Color::new("#FFFFFF"));
        scene.define(
            MarkerDef::new("m", Rect::new(0.0, 0.0, 10.0, 10.0))
                .with_reference(Point::new(10.0, 5.0))
                .with_size(10.0, 12.0)
                .with_orient(MarkerOrient::Fixed { degrees: 45.0 })
                .with_class("marker extension flowchart-v2"),
        );
        let mut path = Path::new();
        path.move_to(Point::new(0.0, 0.0))
            .line_to(Point::new(4.0, 4.0));
        scene.push(
            PathShape::new(path, PathStyle::stroked("#333333", 2.0))
                .with_end_marker(MarkerRef::new("m").with_color("#333333")),
        );
        scene.push(
            RectShape::new(
                Rect::new(1.0, 2.0, 3.0, 4.0),
                5.0,
                PathStyle::filled("#ECECFF"),
            )
            .with_source(SourceRef::node("A")),
        );
        let mut group = Group::translated(10.0, 10.0)
            .with_source(SourceRef::cluster("sub"))
            .with_clip(Rect::new(0.0, 0.0, 200.0, 100.0));
        let block = TextBlock::new(
            vec![TextLine::new(vec![TextSpan::bold("label")])],
            Size::new(30.0, 14.0),
            13.2,
        );
        group.push(
            TextRun::centered(Point::new(5.0, 6.0), Font::default(), block)
                .with_source(SourceRef::node("B")),
        );
        group.push(ImageRef::new("fa:fa-bell", Rect::new(0.0, 0.0, 8.0, 8.0)));
        group.push(EllipseShape::new(
            Point::new(1.0, 1.0),
            4.0,
            2.0,
            PathStyle::stroked("#333333", 1.0),
        ));
        scene.push(group);
        let json = serde_json::to_string(&scene).expect("scene serializes");
        let back: Scene = serde_json::from_str(&json).expect("scene deserializes");
        assert_eq!(back, scene);
    }

    #[test]
    fn path_command_serializes_as_a_tagged_object() {
        let json = serde_json::to_value(PathCommand::line_to(Point::new(1.0, 2.0)))
            .expect("command serializes");
        assert_eq!(
            json,
            serde_json::json!({ "op": "lineTo", "x": 1.0, "y": 2.0 })
        );
    }
}
