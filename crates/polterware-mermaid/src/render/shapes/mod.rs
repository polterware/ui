//! Mermaid's `rendering-elements/shapes.ts` registry and the first reusable shape handlers.
//! Geometry is deterministic Scene data; RoughJS `handDrawn` jitter remains a renderer TODO.

use crate::measure::{Font, TextMeasure};
use crate::render::intersect::{self, Intersectable};
use crate::render::labels::{LabelOptions, create_label};
use crate::scene::{
    Color, Group, Item, Path, PathShape, PathStyle, Point, Rect, SourceRef, Transform,
};
use std::collections::HashMap;

/// Mutable geometry/state corresponding to upstream rendering `Node`.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub shape: String,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub padding: f64,
    pub rx: f64,
    pub ry: f64,
    pub min_width: Option<f64>,
    pub css_classes: String,
    pub look: String,
    pub bounds: Rect,
    pub intersect: IntersectKind,
}

impl Node {
    pub fn new(id: impl Into<String>, label: impl Into<String>, shape: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            shape: shape.into(),
            width: None,
            height: None,
            padding: 0.0,
            rx: 0.0,
            ry: 0.0,
            min_width: None,
            css_classes: String::new(),
            look: "neo".into(),
            bounds: Rect::ZERO,
            intersect: IntersectKind::Rect,
        }
    }
}

/// The shape-owned intersection behavior assigned by each shape handler.
#[derive(Clone, Debug, PartialEq)]
pub enum IntersectKind {
    Rect,
    Circle,
    Ellipse { rx: f64, ry: f64 },
    Polygon(Vec<Point>),
}

impl Intersectable for Node {
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn intersect(&self, point: Point) -> Point {
        match &self.intersect {
            IntersectKind::Rect => intersect::intersect_rect(self.bounds, point),
            IntersectKind::Circle => {
                intersect::intersect_circle(self.bounds.center(), self.bounds.width / 2.0, point)
            }
            IntersectKind::Ellipse { rx, ry } => {
                intersect::intersect_ellipse(self.bounds.center(), *rx, *ry, point)
            }
            IntersectKind::Polygon(points) => {
                intersect::intersect_polygon(self.bounds, points, point)
            }
        }
    }
}

/// Shape output and generated scene group.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderedNode {
    pub node: Node,
    pub group: Group,
}

/// One registered Mermaid shape. Handler names identify the upstream implementation file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShapeDefinition {
    pub semantic_name: &'static str,
    pub name: &'static str,
    pub short_name: &'static str,
    pub description: &'static str,
    pub aliases: &'static [&'static str],
    pub internal_aliases: &'static [&'static str],
    pub upstream_file: &'static str,
    pub kind: ShapeKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeKind {
    SquareRect,
    RoundedRect,
    Circle,
    Question,
    Stadium,
    Unported,
}

/// The source registry's documented names and aliases. Every remaining registry shape resolves
/// deliberately to a named `shape not ported` error until its upstream geometry is implemented.
pub const SHAPE_DEFINITIONS: &[ShapeDefinition] = &[
    ShapeDefinition {
        semantic_name: "Process",
        name: "Rectangle",
        short_name: "rect",
        description: "Standard process shape",
        aliases: &["proc", "process", "rectangle"],
        internal_aliases: &["squareRect"],
        upstream_file: "shapes/squareRect.ts",
        kind: ShapeKind::SquareRect,
    },
    ShapeDefinition {
        semantic_name: "Event",
        name: "Rounded Rectangle",
        short_name: "rounded",
        description: "Represents an event",
        aliases: &["event"],
        internal_aliases: &["roundedRect"],
        upstream_file: "shapes/roundedRect.ts",
        kind: ShapeKind::RoundedRect,
    },
    ShapeDefinition {
        semantic_name: "Terminal Point",
        name: "Stadium",
        short_name: "stadium",
        description: "Terminal point",
        aliases: &["terminal", "pill"],
        internal_aliases: &[],
        upstream_file: "shapes/stadium.ts",
        kind: ShapeKind::Stadium,
    },
    ShapeDefinition {
        semantic_name: "Start",
        name: "Circle",
        short_name: "circle",
        description: "Starting point",
        aliases: &["circ"],
        internal_aliases: &[],
        upstream_file: "shapes/circle.ts",
        kind: ShapeKind::Circle,
    },
    ShapeDefinition {
        semantic_name: "Decision",
        name: "Diamond",
        short_name: "diam",
        description: "Decision-making step",
        aliases: &["decision", "diamond", "question"],
        internal_aliases: &[],
        upstream_file: "shapes/question.ts",
        kind: ShapeKind::Question,
    },
    ShapeDefinition {
        semantic_name: "Subprocess",
        name: "Framed Rectangle",
        short_name: "fr-rect",
        description: "Subprocess",
        aliases: &["subprocess", "subproc", "framed-rectangle", "subroutine"],
        internal_aliases: &[],
        upstream_file: "shapes/subroutine.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Database",
        name: "Cylinder",
        short_name: "cyl",
        description: "Database storage",
        aliases: &["db", "database", "cylinder"],
        internal_aliases: &[],
        upstream_file: "shapes/cylinder.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Console",
        name: "Console (terminal window)",
        short_name: "console",
        description: "Terminal or console window",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/console.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Prepare Conditional",
        name: "Hexagon",
        short_name: "hex",
        description: "Preparation or condition step",
        aliases: &["hexagon", "prepare"],
        internal_aliases: &[],
        upstream_file: "shapes/hexagon.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Data Input/Output",
        name: "Lean Right",
        short_name: "lean-r",
        description: "Represents input or output",
        aliases: &["lean-right", "in-out"],
        internal_aliases: &["lean_right"],
        upstream_file: "shapes/leanRight.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Data Input/Output",
        name: "Lean Left",
        short_name: "lean-l",
        description: "Represents output or input",
        aliases: &["lean-left", "out-in"],
        internal_aliases: &["lean_left"],
        upstream_file: "shapes/leanLeft.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Priority Action",
        name: "Trapezoid Base Bottom",
        short_name: "trap-b",
        description: "Priority action",
        aliases: &["priority", "trapezoid-bottom", "trapezoid"],
        internal_aliases: &[],
        upstream_file: "shapes/trapezoid.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Manual Operation",
        name: "Trapezoid Base Top",
        short_name: "trap-t",
        description: "Represents a manual task",
        aliases: &["manual", "trapezoid-top", "inv-trapezoid"],
        internal_aliases: &["inv_trapezoid"],
        upstream_file: "shapes/invertedTrapezoid.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Stop",
        name: "Double Circle",
        short_name: "dbl-circ",
        description: "Represents a stop point",
        aliases: &["double-circle"],
        internal_aliases: &["doublecircle"],
        upstream_file: "shapes/doubleCircle.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Card",
        name: "Notched Rectangle",
        short_name: "notch-rect",
        description: "Represents a card",
        aliases: &["card", "notched-rectangle"],
        internal_aliases: &[],
        upstream_file: "shapes/card.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Lined/Shaded Process",
        name: "Lined Rectangle",
        short_name: "lin-rect",
        description: "Lined process shape",
        aliases: &[
            "lined-rectangle",
            "lined-process",
            "lin-proc",
            "shaded-process",
        ],
        internal_aliases: &[],
        upstream_file: "shapes/shadedProcess.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Start",
        name: "Small Circle",
        short_name: "sm-circ",
        description: "Small starting point",
        aliases: &["start", "small-circle"],
        internal_aliases: &["stateStart"],
        upstream_file: "shapes/stateStart.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Stop",
        name: "Framed Circle",
        short_name: "fr-circ",
        description: "Stop point",
        aliases: &["stop", "framed-circle"],
        internal_aliases: &["stateEnd"],
        upstream_file: "shapes/stateEnd.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Fork/Join",
        name: "Filled Rectangle",
        short_name: "fork",
        description: "Fork or join in process flow",
        aliases: &["join"],
        internal_aliases: &["forkJoin"],
        upstream_file: "shapes/forkJoin.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Comment",
        name: "Curly Brace",
        short_name: "brace",
        description: "Adds a comment",
        aliases: &["comment", "brace-l"],
        internal_aliases: &[],
        upstream_file: "shapes/curlyBraceLeft.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Comment Right",
        name: "Curly Brace",
        short_name: "brace-r",
        description: "Adds a comment",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/curlyBraceRight.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Comment with braces on both sides",
        name: "Curly Braces",
        short_name: "braces",
        description: "Adds a comment",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/curlyBraces.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Com Link",
        name: "Lightning Bolt",
        short_name: "bolt",
        description: "Communication link",
        aliases: &["com-link", "lightning-bolt"],
        internal_aliases: &[],
        upstream_file: "shapes/lightningBolt.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Document",
        name: "Document",
        short_name: "doc",
        description: "Represents a document",
        aliases: &["doc", "document"],
        internal_aliases: &[],
        upstream_file: "shapes/waveEdgedRectangle.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Delay",
        name: "Half-Rounded Rectangle",
        short_name: "delay",
        description: "Represents a delay",
        aliases: &["half-rounded-rectangle"],
        internal_aliases: &[],
        upstream_file: "shapes/halfRoundedRectangle.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Direct Access Storage",
        name: "Horizontal Cylinder",
        short_name: "h-cyl",
        description: "Direct access storage",
        aliases: &["das", "horizontal-cylinder"],
        internal_aliases: &[],
        upstream_file: "shapes/tiltedCylinder.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Disk Storage",
        name: "Lined Cylinder",
        short_name: "lin-cyl",
        description: "Disk storage",
        aliases: &["disk", "lined-cylinder"],
        internal_aliases: &[],
        upstream_file: "shapes/linedCylinder.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Display",
        name: "Curved Trapezoid",
        short_name: "curv-trap",
        description: "Represents a display",
        aliases: &["curved-trapezoid", "display"],
        internal_aliases: &[],
        upstream_file: "shapes/curvedTrapezoid.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Divided Process",
        name: "Divided Rectangle",
        short_name: "div-rect",
        description: "Divided process shape",
        aliases: &["div-proc", "divided-rectangle", "divided-process"],
        internal_aliases: &[],
        upstream_file: "shapes/dividedRect.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Extract",
        name: "Triangle",
        short_name: "tri",
        description: "Extraction process",
        aliases: &["extract", "triangle"],
        internal_aliases: &[],
        upstream_file: "shapes/triangle.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Internal Storage",
        name: "Window Pane",
        short_name: "win-pane",
        description: "Internal storage",
        aliases: &["internal-storage", "window-pane"],
        internal_aliases: &[],
        upstream_file: "shapes/windowPane.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Junction",
        name: "Filled Circle",
        short_name: "f-circ",
        description: "Junction point",
        aliases: &["junction", "filled-circle"],
        internal_aliases: &[],
        upstream_file: "shapes/filledCircle.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Loop Limit",
        name: "Trapezoidal Pentagon",
        short_name: "notch-pent",
        description: "Loop limit step",
        aliases: &["loop-limit", "notched-pentagon"],
        internal_aliases: &[],
        upstream_file: "shapes/trapezoidalPentagon.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Manual File",
        name: "Flipped Triangle",
        short_name: "flip-tri",
        description: "Manual file operation",
        aliases: &["manual-file", "flipped-triangle"],
        internal_aliases: &[],
        upstream_file: "shapes/flippedTriangle.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Manual Input",
        name: "Sloped Rectangle",
        short_name: "sl-rect",
        description: "Manual input step",
        aliases: &["manual-input", "sloped-rectangle"],
        internal_aliases: &[],
        upstream_file: "shapes/slopedRect.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Multi-Document",
        name: "Stacked Document",
        short_name: "docs",
        description: "Multiple documents",
        aliases: &["documents", "st-doc", "stacked-document"],
        internal_aliases: &[],
        upstream_file: "shapes/multiWaveEdgedRectangle.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Multi-Process",
        name: "Stacked Rectangle",
        short_name: "st-rect",
        description: "Multiple processes",
        aliases: &["procs", "processes", "stacked-rectangle"],
        internal_aliases: &[],
        upstream_file: "shapes/multiRect.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Stored Data",
        name: "Bow Tie Rectangle",
        short_name: "bow-rect",
        description: "Stored data",
        aliases: &["stored-data", "bow-tie-rectangle"],
        internal_aliases: &[],
        upstream_file: "shapes/bowTieRect.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Summary",
        name: "Crossed Circle",
        short_name: "cross-circ",
        description: "Summary",
        aliases: &["summary", "crossed-circle"],
        internal_aliases: &[],
        upstream_file: "shapes/crossedCircle.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Tagged Document",
        name: "Tagged Document",
        short_name: "tag-doc",
        description: "Tagged document",
        aliases: &["tag-doc", "tagged-document"],
        internal_aliases: &[],
        upstream_file: "shapes/taggedWaveEdgedRectangle.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Tagged Process",
        name: "Tagged Rectangle",
        short_name: "tag-rect",
        description: "Tagged process",
        aliases: &["tagged-rectangle", "tag-proc", "tagged-process"],
        internal_aliases: &[],
        upstream_file: "shapes/taggedRect.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Paper Tape",
        name: "Flag",
        short_name: "flag",
        description: "Paper tape",
        aliases: &["paper-tape"],
        internal_aliases: &[],
        upstream_file: "shapes/waveRectangle.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Odd",
        name: "Odd",
        short_name: "odd",
        description: "Odd shape",
        aliases: &[],
        internal_aliases: &["rect_left_inv_arrow"],
        upstream_file: "shapes/rectLeftInvArrow.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Lined Document",
        name: "Lined Document",
        short_name: "lin-doc",
        description: "Lined document",
        aliases: &["lined-document"],
        internal_aliases: &[],
        upstream_file: "shapes/linedWaveEdgedRect.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "anchor",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/anchor.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "bowTieRect",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/bowTieRect.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "bucket",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/bucket.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "collapsedGroup",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/collapsedGroup.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "block_arrow",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/blockArrow.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "browser",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/browser.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "card",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/card.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "choice",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/choice.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "composite",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/composite.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "consoleWindow",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/console.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "crossedCircle",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/crossedCircle.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "curlyBraceLeft",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/curlyBraceLeft.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "curlyBraceRight",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/curlyBraceRight.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "curlyBraces",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/curlyBraces.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "curvedTrapezoid",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/curvedTrapezoid.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "person",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/person.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "cylinder",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/cylinder.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "datastore",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/datastore.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "dividedRectangle",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/dividedRect.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "doublecircle",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/doubleCircle.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "usecaseEllipse",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/usecaseEllipse.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "filledCircle",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/filledCircle.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "flippedTriangle",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/flippedTriangle.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "folder",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/folder.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "forkJoin",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/forkJoin.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "halfRoundedRectangle",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/halfRoundedRectangle.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "hexagon",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/hexagon.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "hourglass",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/hourglass.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "icon",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/icon.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "iconCircle",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/iconCircle.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "iconRounded",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/iconRounded.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "iconSquare",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/iconSquare.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "imageSquare",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/imageSquare.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "inv_trapezoid",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/invertedTrapezoid.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "labelRect",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/labelRect.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "lean_left",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/leanLeft.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "lean_right",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/leanRight.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "lightningBolt",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/lightningBolt.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "linedCylinder",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/linedCylinder.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "linedWaveEdgedRect",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/linedWaveEdgedRect.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "usecaseActor",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/usecaseActor.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "usecaseActorAwesome",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/usecaseActorAwesome.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "usecaseActorHollow",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/usecaseActorHollow.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "usecaseActorIcon",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/usecaseActorIcon.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "usecaseBusiness",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/usecaseBusiness.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "usecaseJsonTable",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/usecaseJsonTable.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "multiRect",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/multiRect.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "multiWaveEdgedRectangle",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/multiWaveEdgedRectangle.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "note",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/note.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "rect_left_inv_arrow",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/rectLeftInvArrow.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "rectWithTitle",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/rectWithTitle.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "shadedProcess",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/shadedProcess.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "slopedRect",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/slopedRect.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "squareRect",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/squareRect.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "state",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/state.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "stateEnd",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/stateEnd.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "stateStart",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/stateStart.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "subroutine",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/subroutine.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "taggedRect",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/taggedRect.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "taggedWaveEdgedRectangle",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/taggedWaveEdgedRectangle.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "text",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/text.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "tiltedCylinder",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/tiltedCylinder.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "trapezoid",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/trapezoid.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "trapezoidalPentagon",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/trapezoidalPentagon.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "triangle",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/triangle.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "waveEdgedRectangle",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/waveEdgedRectangle.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "waveRectangle",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/waveRectangle.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "windowPane",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/windowPane.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "erBox",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/erBox.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "classBox",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/classBox.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "requirementBox",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/requirementBox.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "kanbanItem",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/kanbanItem.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "bang",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/bang.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "cloud",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/cloud.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "defaultMindmapNode",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/defaultMindmapNode.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "mindmapCircle",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/mindmapCircle.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "bpmnStart",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/bpmnStart.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "bpmnIntermediate",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/bpmnIntermediate.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "bpmnBoundary",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/bpmnBoundary.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "bpmnEnd",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/bpmnEnd.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "bpmnGateway",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/bpmnGateway.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "bpmnActivity",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/bpmnActivity.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "bpmnDataObject",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/bpmnDataObject.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "bpmnDataStore",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/bpmnDataStore.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "bpmnAnnotation",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/bpmnAnnotation.js",
        kind: ShapeKind::Unported,
    },
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShapeError {
    pub message: String,
}
impl std::fmt::Display for ShapeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for ShapeError {}

/// Look up a canonical name or any upstream alias.
pub fn shape_definition(name: &str) -> Option<&'static ShapeDefinition> {
    SHAPE_DEFINITIONS.iter().find(|shape| {
        shape.short_name == name
            || shape.aliases.contains(&name)
            || shape.internal_aliases.contains(&name)
    })
}

/// Upstream `shapes.ts` validity predicate.
pub fn is_valid_shape(name: &str) -> bool {
    shape_definition(name).is_some()
}

/// Registry dispatch, equivalent to `insertNode`'s shape handler selection.
pub fn render_node(
    mut node: Node,
    measure: &dyn TextMeasure,
    theme: &crate::theme::ThemeVariables,
) -> Result<RenderedNode, ShapeError> {
    let shape = shape_definition(&node.shape).ok_or_else(|| ShapeError {
        message: format!("No such shape: {}. Please check your syntax.", node.shape),
    })?;
    if shape.kind == ShapeKind::Unported {
        return Err(ShapeError {
            message: format!(
                "shape not ported: {} (rendering-elements/{})",
                node.shape, shape.upstream_file
            ),
        });
    }
    if node.shape == "rect" {
        node.shape = if node.rx != 0.0 && node.ry != 0.0 {
            "roundedRect"
        } else {
            "squareRect"
        }
        .into();
    }
    let mut group = Group::new()
        .with_class(get_node_classes(&node, None))
        .with_source(SourceRef::node(node.id.clone()));
    let label = create_label(
        &LabelOptions {
            text: node.label.clone(),
            font: Font::default(),
            markdown: false,
            width: f64::INFINITY,
            centered: false,
            class: "label".into(),
            source: Some(SourceRef::node(node.id.clone())),
        },
        measure,
    )
    .map_err(|e| ShapeError {
        message: e.to_string(),
    })?;
    let text_size = match label.items.first() {
        Some(Item::Text(text)) => text.size,
        _ => crate::scene::Size::ZERO,
    };
    let padding = match shape.kind {
        ShapeKind::SquareRect => {
            if node.look == "neo" {
                (16.0, 12.0)
            } else {
                (node.padding * 2.0, node.padding)
            }
        }
        ShapeKind::RoundedRect => (node.padding, node.padding),
        ShapeKind::Stadium => {
            if node.look == "neo" {
                (40.0, 24.0)
            } else {
                (node.padding, node.padding)
            }
        }
        _ => (node.padding, node.padding),
    };
    let (width, height) = match shape.kind {
        ShapeKind::Circle => {
            let r = (text_size.width.powi(2) + text_size.height.powi(2)).sqrt() / 2.0
                + if node.look == "neo" {
                    16.0
                } else {
                    node.padding / 2.0
                };
            (r * 2.0, r * 2.0)
        }
        ShapeKind::Question => {
            let s = text_size.width + text_size.height + node.padding;
            (s, s)
        }
        ShapeKind::Stadium => {
            let h = text_size.height + padding.1;
            (text_size.width + h / 4.0 + padding.0, h)
        }
        _ => (
            (text_size.width + padding.0 * 2.0).max(node.width.unwrap_or(0.0)),
            (text_size.height + padding.1 * 2.0).max(node.height.unwrap_or(0.0)),
        ),
    };
    node.bounds = Rect::centered(Point::ZERO, crate::scene::Size::new(width, height));
    match shape.kind {
        ShapeKind::SquareRect | ShapeKind::RoundedRect => {
            let radius = if shape.kind == ShapeKind::RoundedRect {
                theme.number("borderRadius").unwrap_or(5.0)
            } else {
                0.0
            };
            let mut style = PathStyle::default();
            style.fill = Some(Color::new("#ECECFF"));
            style.stroke = Some(Color::new("#9370DB"));
            group.push(
                crate::scene::RectShape::new(node.bounds, radius, style)
                    .with_source(SourceRef::node(node.id.clone())),
            );
            node.intersect = IntersectKind::Rect;
        }
        ShapeKind::Circle => {
            let radius = width / 2.0;
            let mut style = PathStyle::default();
            style.fill = Some(Color::new("#ECECFF"));
            style.stroke = Some(Color::new("#9370DB"));
            group.push(
                crate::scene::EllipseShape::new(Point::ZERO, radius, radius, style)
                    .with_source(SourceRef::node(node.id.clone())),
            );
            node.intersect = IntersectKind::Circle;
        }
        ShapeKind::Question => {
            let s = width;
            let points = vec![
                Point::new(s / 2.0, 0.0),
                Point::new(s, s / 2.0),
                Point::new(s / 2.0, s),
                Point::new(0.0, s / 2.0),
            ];
            let local: Vec<Point> = points
                .iter()
                .map(|p| p.translated(-s / 2.0, -s / 2.0))
                .collect();
            let mut style = PathStyle::default();
            style.fill = Some(Color::new("#ECECFF"));
            style.stroke = Some(Color::new("#9370DB"));
            group.push(
                PathShape::new(Path::polygon(local.clone()), style)
                    .with_source(SourceRef::node(node.id.clone())),
            );
            node.intersect = IntersectKind::Polygon(points);
        }
        ShapeKind::Stadium => {
            let h = height;
            let r = h / 2.0;
            let x = -width / 2.0;
            let y = -height / 2.0;
            let right = -x;
            let mut path = Path::new();
            path.move_to(Point::new(x + r, y))
                .line_to(Point::new(right - r, y))
                .arc_to(r, r, 0.0, false, true, Point::new(right - r, y + h))
                .line_to(Point::new(x + r, y + h))
                .arc_to(r, r, 0.0, false, true, Point::new(x + r, y))
                .close();
            let mut style = PathStyle::default();
            style.fill = Some(Color::new("#ECECFF"));
            style.stroke = Some(Color::new("#9370DB"));
            group.push(PathShape::new(path, style).with_source(SourceRef::node(node.id.clone())));
            let mut polygon = vec![Point::new(r, 0.0), Point::new(width - r, 0.0)];
            polygon.extend((0..=24).map(|i| {
                let angle = (-90.0 + i as f64 * 180.0 / 24.0).to_radians();
                Point::new(width - r + r * angle.cos(), r + r * angle.sin())
            }));
            polygon.push(Point::new(r, height));
            polygon.extend((0..=24).map(|i| {
                let angle = (90.0 + i as f64 * 180.0 / 24.0).to_radians();
                Point::new(r + r * angle.cos(), r + r * angle.sin())
            }));
            node.intersect = IntersectKind::Polygon(polygon);
        }
        ShapeKind::Unported => unreachable!(),
    }
    group.items.push(Item::Group(label));
    Ok(RenderedNode { node, group })
}

/// Register a node's final element, analogous to `nodes.ts`'s nodeElems map.
pub fn set_node_elem(elements: &mut HashMap<String, Group>, node: &Node, group: Group) {
    elements.insert(node.id.clone(), group);
}

/// Position a node group as `nodes.ts` does; returns the horizontal diff applied to clusters.
pub fn position_node(
    group: &mut Group,
    node: &Node,
    x: f64,
    y: f64,
    cluster: bool,
    diff: f64,
) -> f64 {
    group.transform = if cluster {
        Transform::translate(
            x + diff - node.bounds.width / 2.0,
            y - node.bounds.height / 2.0 - 8.0,
        )
    } else {
        Transform::translate(x, y)
    };
    diff
}

/// Keep upstream CSS class roles for scene styling.
pub fn get_node_classes(node: &Node, extra: Option<&str>) -> String {
    format!(
        "{} {} {}",
        if node.look == "handDrawn" {
            "rough-node"
        } else {
            "node"
        },
        node.css_classes,
        extra.unwrap_or("")
    )
    .trim()
    .to_string()
}

/// `createPathFromPoints`: `M p0 L p1 ... Z`.
pub fn create_path_from_points(points: &[Point]) -> Path {
    Path::polygon(points.iter().copied())
}

/// `generateFullSineWavePoints` samples 50 straight segments.
pub fn generate_full_sine_wave_points(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    amplitude: f64,
    num_cycles: f64,
) -> Vec<Point> {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let cycle = dx / num_cycles;
    let frequency = std::f64::consts::TAU / cycle;
    let mid = y1 + dy / 2.0;
    (0..=50)
        .map(|i| {
            let t = i as f64 / 50.0;
            let x = x1 + t * dx;
            Point::new(x, mid + amplitude * (frequency * (x - x1)).sin())
        })
        .collect()
}

/// `generateCirclePoints` returns negated sampled coordinates as upstream does.
pub fn generate_circle_points(
    cx: f64,
    cy: f64,
    r: f64,
    count: usize,
    start: f64,
    end: f64,
) -> Vec<Point> {
    if count == 0 {
        return Vec::new();
    }
    let step = if count == 1 {
        0.0
    } else {
        (end - start).to_radians() / (count - 1) as f64
    };
    let start = start.to_radians();
    (0..count)
        .map(|i| {
            let a = start + i as f64 * step;
            Point::new(-(cx + r * a.cos()), -(cy + r * a.sin()))
        })
        .collect()
}

/// `updateNodeBounds`: set dimensions to the SVG element box, or supplied exact geometry.
pub fn update_node_bounds(node: &mut Node, bounds: Rect) {
    node.bounds.width = bounds.width;
    node.bounds.height = bounds.height;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure::FallbackMeasure;

    #[test]
    fn resolves_upstream_aliases_and_explicit_unported_errors() {
        assert_eq!(
            shape_definition("process").unwrap().upstream_file,
            "shapes/squareRect.ts"
        );
        assert_eq!(shape_definition("question").unwrap().short_name, "diam");
        let error = render_node(
            Node::new("n", "x", "cyl"),
            &FallbackMeasure,
            &crate::theme::ThemeVariables::default(),
        )
        .unwrap_err();
        assert!(
            error.message.contains("shape not ported")
                && error.message.contains("shapes/cylinder.js")
        );
    }

    #[test]
    fn renders_foundation_shapes_and_positions_nodes() {
        for name in ["squareRect", "roundedRect", "circle", "question", "stadium"] {
            let rendered = render_node(
                Node::new("n", "label", name),
                &FallbackMeasure,
                &crate::theme::ThemeVariables::default(),
            )
            .unwrap();
            assert!(!rendered.group.items.is_empty());
            assert!(rendered.node.bounds.width > 0.0 && rendered.node.bounds.height > 0.0);
        }
        let node = Node::new("n", "x", "rect");
        let mut group = Group::new();
        assert_eq!(
            position_node(&mut group, &node, 10.0, 20.0, false, 3.0),
            3.0
        );
        assert_eq!(group.transform, Transform::translate(10.0, 20.0));
    }

    #[test]
    fn path_helpers_sample_expected_points() {
        assert_eq!(
            create_path_from_points(&[Point::ZERO, Point::new(1.0, 0.0)])
                .commands
                .len(),
            3
        );
        assert_eq!(
            generate_full_sine_wave_points(0.0, 0.0, 10.0, 0.0, 1.0, 1.0).len(),
            51
        );
        assert_eq!(
            generate_circle_points(0.0, 0.0, 2.0, 3, 0.0, 180.0).len(),
            3
        );
    }
}
