//! Mermaid's `rendering-elements/shapes.ts` registry and the first reusable shape handlers.
//! Geometry is deterministic Scene data; RoughJS `handDrawn` jitter remains a renderer TODO.

use crate::measure::{Font, TextMeasure};
use crate::render::intersect::{self, Intersectable};
use crate::render::labels::{LabelOptions, create_label};
use crate::scene::{Color, Group, Item, Path, PathStyle, Point, Rect, SourceRef, Transform};
use std::collections::HashMap;

pub mod anchor;
pub mod bang;
pub mod block_arrow;
pub mod bow_tie_rect;
pub mod bpmn_activity;
pub mod bpmn_annotation;
pub mod bpmn_boundary;
pub mod bpmn_data_object;
pub mod bpmn_data_store;
pub mod bpmn_end;
pub mod bpmn_event;
pub mod bpmn_gateway;
pub mod bpmn_icons;
pub mod bpmn_intermediate;
pub mod bpmn_shape_core;
pub mod bpmn_start;
pub mod browser;
pub mod bucket;
pub mod c4_label_helper;
pub mod card;
pub mod choice;
pub mod circle;
pub mod class_box;
pub mod cloud;
pub mod collapsed_group;
pub mod composite;
pub mod console;
pub mod crossed_circle;
pub mod curly_brace_left;
pub mod curly_brace_right;
pub mod curly_braces;
pub mod curved_trapezoid;
pub mod cylinder;
pub mod datastore;
pub mod default_mindmap_node;
pub mod divided_rect;
pub mod document;
pub mod double_circle;
pub mod draw_rect;
pub mod er_box;
pub mod filled_circle;
pub mod flipped_triangle;
pub mod folder;
pub mod fork_join;
pub mod half_rounded_rectangle;
pub mod hand_drawn_shape_styles;
pub mod hexagon;
pub mod hourglass;
pub mod icon;
pub mod icon_circle;
pub mod icon_rounded;
pub mod icon_square;
pub mod image_square;
pub mod insert_polygon_shape;
pub mod inverted_trapezoid;
pub mod kanban_item;
pub mod label_image_utils;
pub mod label_rect;
pub mod lean_left;
pub mod lean_right;
pub mod lightning_bolt;
pub mod lined_cylinder;
pub mod lined_wave_edged_rect;
pub mod mindmap_circle;
pub mod multi_rect;
pub mod multi_wave_edged_rectangle;
pub mod note;
pub mod person;
pub mod question;
pub mod rect_left_inv_arrow;
pub mod rect_with_title;
pub mod requirement_box;
pub mod rounded_rect;
pub mod rounded_rect_path;
pub mod shaded_process;
pub mod sloped_rect;
pub mod square_rect;
pub mod stadium;
pub mod state;
pub mod state_end;
pub mod state_start;
pub mod subroutine;
pub mod tagged_rect;
pub mod tagged_wave_edged_rectangle;
pub mod text;
pub mod tilted_cylinder;
pub mod trapezoid;
pub mod trapezoidal_pentagon;
pub mod triangle;
pub mod usecase_actor;
pub mod usecase_actor_awesome;
pub mod usecase_actor_hollow;
pub mod usecase_actor_icon;
pub mod usecase_business;
pub mod usecase_ellipse;
pub mod usecase_json_table;
pub mod wave_edged_rectangle;
pub mod wave_rectangle;
pub mod window_pane;

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

    // Upstream Node fields
    pub icon: Option<String>,
    pub img: Option<String>,
    pub asset_width: Option<f64>,
    pub asset_height: Option<f64>,
    pub pos: Option<String>,
    pub constraint: Option<String>,

    // Diagram payload fields
    pub members: Vec<String>,
    pub methods: Vec<String>,
    pub annotations: Vec<String>,
    pub attributes: Vec<String>,
    pub requirement_id: Option<String>,
    pub requirement_type: Option<String>,
    pub risk: Option<String>,
    pub verify_method: Option<String>,
    pub ticket: Option<String>,
    pub assigned: Option<String>,
    pub priority: Option<String>,
    pub container_type: Option<String>,
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

            icon: None,
            img: None,
            asset_width: None,
            asset_height: None,
            pos: None,
            constraint: None,

            members: Vec::new(),
            methods: Vec::new(),
            annotations: Vec::new(),
            attributes: Vec::new(),
            requirement_id: None,
            requirement_type: None,
            risk: None,
            verify_method: None,
            ticket: None,
            assigned: None,
            priority: None,
            container_type: None,
        }
    }
}

/// The shape-owned intersection behavior assigned by each shape handler.
#[derive(Clone, Debug, PartialEq)]
pub enum IntersectKind {
    Rect,
    Circle,
    Ellipse {
        rx: f64,
        ry: f64,
    },
    Polygon(Vec<Point>),
    Ring {
        diameter: f64,
    },
    Cylinder {
        rx: f64,
        ry: f64,
    },
    HorizontalCylinder {
        rx: f64,
        ry: f64,
    },
    FaceProject {
        width: f64,
        height: f64,
        face: Option<bpmn_shape_core::ShapeFace>,
    },
    FaceCentre {
        width: f64,
        height: f64,
    },
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
            IntersectKind::Ring { diameter } => {
                bpmn_shape_core::ring_intersect(self.bounds.center(), *diameter, point)
            }
            IntersectKind::FaceProject {
                width,
                height,
                face,
            } => bpmn_shape_core::face_project_intersect_at(
                self.bounds.center(),
                *width,
                *height,
                point,
                *face,
            ),
            IntersectKind::FaceCentre { width, height } => {
                bpmn_shape_core::face_centre_intersect(self.bounds.center(), *width, *height, point)
            }
            IntersectKind::Cylinder { rx, ry } => {
                let mut pos = intersect::intersect_rect(self.bounds, point);
                let center = self.bounds.center();
                let x = pos.x - center.x;
                let half_w = self.bounds.width / 2.0;
                let half_h = self.bounds.height / 2.0;
                if *rx != 0.0
                    && (x.abs() < half_w
                        || (x.abs() == half_w && (pos.y - center.y).abs() > half_h - *ry))
                {
                    let mut y = ry * ry * (1.0 - (x * x) / (rx * rx));
                    if y > 0.0 {
                        y = y.sqrt();
                    }
                    let mut offset = ry - y;
                    if point.y - center.y > 0.0 {
                        offset = -offset;
                    }
                    pos.y += offset;
                }
                pos
            }
            IntersectKind::HorizontalCylinder { rx, ry } => {
                let mut pos = intersect::intersect_rect(self.bounds, point);
                let center = self.bounds.center();
                let y = pos.y - center.y;
                let half_w = self.bounds.width / 2.0;
                let half_h = self.bounds.height / 2.0;
                if *ry != 0.0
                    && (y.abs() < half_h
                        || (y.abs() == half_h && (pos.x - center.x).abs() > half_w - *rx))
                {
                    let mut x = rx * rx * (1.0 - (y * y) / (ry * ry));
                    if x != 0.0 {
                        x = x.abs().sqrt();
                    }
                    let mut offset = rx - x;
                    if point.x - center.x > 0.0 {
                        offset = -offset;
                    }
                    pos.x += offset;
                }
                pos
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
        internal_aliases: &["lean_right", "leanRight"],
        upstream_file: "shapes/leanRight.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Data Input/Output",
        name: "Lean Left",
        short_name: "lean-l",
        description: "Represents output or input",
        aliases: &["lean-left", "out-in"],
        internal_aliases: &["lean_left", "leanLeft"],
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
        internal_aliases: &["inv_trapezoid", "invertedTrapezoid"],
        upstream_file: "shapes/invertedTrapezoid.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Stop",
        name: "Double Circle",
        short_name: "dbl-circ",
        description: "Represents a stop point",
        aliases: &["double-circle"],
        internal_aliases: &["doublecircle", "doubleCircle"],
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
        internal_aliases: &["dividedRect"],
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
        internal_aliases: &["rect_left_inv_arrow", "rectLeftInvArrow"],
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
        semantic_name: "Bucket",
        name: "Bucket",
        short_name: "bucket",
        description: "Object storage bucket",
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
        semantic_name: "Browser",
        name: "Browser",
        short_name: "browser",
        description: "Browser window",
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
        semantic_name: "Person",
        name: "Person",
        short_name: "person",
        description: "Person (circular head above a rounded body)",
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
        semantic_name: "Data Store",
        name: "Data Store",
        short_name: "datastore",
        description: "Data flow diagram data store",
        aliases: &["data-store"],
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
        semantic_name: "Folder",
        name: "Folder",
        short_name: "folder",
        description: "Folder or directory",
        aliases: &["directory"],
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
        semantic_name: "Collate",
        name: "Hourglass",
        short_name: "hourglass",
        description: "Represents a collate operation",
        aliases: &["hourglass", "collate"],
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
        semantic_name: "Text Block",
        name: "Text Block",
        short_name: "text",
        description: "Text block",
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
        semantic_name: "Bang",
        name: "Bang",
        short_name: "bang",
        description: "Bang",
        aliases: &["bang"],
        internal_aliases: &[],
        upstream_file: "shapes/bang.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "Cloud",
        name: "Cloud",
        short_name: "cloud",
        description: "cloud",
        aliases: &["cloud"],
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
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "drawRect",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/drawRect.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "document",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/document.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "handDrawnShapeStyles",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/handDrawnShapeStyles.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "bpmnEvent",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/bpmnEvent.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "roundedRectPath",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/roundedRectPath.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "c4LabelHelper",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/c4LabelHelper.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "insertPolygonShape",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/insertPolygonShape.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "labelImageUtils",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/labelImageUtils.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "bpmnIcons",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/bpmnIcons.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "bpmnShapeCore",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/bpmnShapeCore.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "bpmn-start",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/bpmnStart.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "bpmn-intermediate",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/bpmnIntermediate.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "bpmn-boundary",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/bpmnBoundary.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "bpmn-end",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/bpmnEnd.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "bpmn-gateway",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/bpmnGateway.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "bpmn-activity",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/bpmnActivity.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "bpmn-data",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/bpmnDataObject.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "bpmn-data-store",
        description: "",
        aliases: &[],
        internal_aliases: &[],
        upstream_file: "shapes/bpmnDataStore.js",
        kind: ShapeKind::Unported,
    },
    ShapeDefinition {
        semantic_name: "",
        name: "",
        short_name: "bpmn-annotation",
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

/// Inputs shared by every shape handler. The node carries its label, classes and look;
/// the theme supplies style variables and the measure shapes labels.
pub struct ShapeContext<'a> {
    pub measure: &'a dyn TextMeasure,
    pub theme: &'a crate::theme::ThemeVariables,
}

fn prepare(
    node: &Node,
    ctx: &ShapeContext<'_>,
) -> Result<(Group, Group, crate::scene::Size), ShapeError> {
    let group = Group::new()
        .with_class(get_node_classes(node, None))
        .with_source(SourceRef::node(node.id.clone()));
    let font_size = ctx
        .theme
        .color("fontSize")
        .and_then(|s| s.trim_end_matches("px").parse::<f64>().ok())
        .or_else(|| ctx.theme.number("fontSize"))
        .unwrap_or(12.0);
    let font_family = ctx.theme.color("fontFamily").unwrap_or("Arial").to_string();
    let font = Font {
        family: font_family,
        size: font_size,
        weight: crate::scene::NORMAL_WEIGHT,
        italic: false,
    };
    let label = create_label(
        &LabelOptions {
            text: node.label.clone(),
            font,
            markdown: false,
            width: f64::INFINITY,
            centered: false,
            class: "label".into(),
            source: Some(SourceRef::node(node.id.clone())),
        },
        ctx.measure,
    )
    .map_err(|e| ShapeError {
        message: e.to_string(),
    })?;
    let text_size = match label.items.first() {
        Some(Item::Text(text)) => text.size,
        _ => crate::scene::Size::ZERO,
    };
    Ok((group, label, text_size))
}

fn shape_style() -> PathStyle {
    PathStyle {
        fill: Some(Color::new("#ECECFF")),
        stroke: Some(Color::new("#9370DB")),
        ..Default::default()
    }
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
    if node.shape == "rect" {
        node.shape = if node.rx != 0.0 && node.ry != 0.0 {
            "roundedRect"
        } else {
            "squareRect"
        }
        .into();
    }
    let ctx = ShapeContext { measure, theme };
    match shape.upstream_file {
        "shapes/anchor.js" | "shapes/anchor.ts" => anchor::draw(&node, &ctx),
        "shapes/bang.js" | "shapes/bang.ts" => bang::draw(&node, &ctx),
        "shapes/blockArrow.js" | "shapes/blockArrow.ts" => block_arrow::draw(&node, &ctx),
        "shapes/bowTieRect.js" | "shapes/bowTieRect.ts" => bow_tie_rect::draw(&node, &ctx),
        "shapes/bpmnActivity.js" | "shapes/bpmnActivity.ts" => bpmn_activity::draw(&node, &ctx),
        "shapes/bpmnAnnotation.js" | "shapes/bpmnAnnotation.ts" => {
            bpmn_annotation::draw(&node, &ctx)
        }
        "shapes/bpmnBoundary.js" | "shapes/bpmnBoundary.ts" => bpmn_boundary::draw(&node, &ctx),
        "shapes/bpmnDataObject.js" | "shapes/bpmnDataObject.ts" => {
            bpmn_data_object::draw(&node, &ctx)
        }
        "shapes/bpmnDataStore.js" | "shapes/bpmnDataStore.ts" => bpmn_data_store::draw(&node, &ctx),
        "shapes/bpmnEnd.js" | "shapes/bpmnEnd.ts" => bpmn_end::draw(&node, &ctx),
        "shapes/bpmnEvent.js" | "shapes/bpmnEvent.ts" => bpmn_event::draw(&node, &ctx),
        "shapes/bpmnGateway.js" | "shapes/bpmnGateway.ts" => bpmn_gateway::draw(&node, &ctx),
        "shapes/bpmnIcons.js" | "shapes/bpmnIcons.ts" => bpmn_icons::draw(&node, &ctx),
        "shapes/bpmnIntermediate.js" | "shapes/bpmnIntermediate.ts" => {
            bpmn_intermediate::draw(&node, &ctx)
        }
        "shapes/bpmnShapeCore.js" | "shapes/bpmnShapeCore.ts" => bpmn_shape_core::draw(&node, &ctx),
        "shapes/bpmnStart.js" | "shapes/bpmnStart.ts" => bpmn_start::draw(&node, &ctx),
        "shapes/browser.js" | "shapes/browser.ts" => browser::draw(&node, &ctx),
        "shapes/bucket.js" | "shapes/bucket.ts" => bucket::draw(&node, &ctx),
        "shapes/c4LabelHelper.js" | "shapes/c4LabelHelper.ts" => c4_label_helper::draw(&node, &ctx),
        "shapes/card.js" | "shapes/card.ts" => card::draw(&node, &ctx),
        "shapes/choice.js" | "shapes/choice.ts" => choice::draw(&node, &ctx),
        "shapes/circle.js" | "shapes/circle.ts" => circle::draw(&node, &ctx),
        "shapes/classBox.js" | "shapes/classBox.ts" => class_box::draw(&node, &ctx),
        "shapes/cloud.js" | "shapes/cloud.ts" => cloud::draw(&node, &ctx),
        "shapes/collapsedGroup.js" | "shapes/collapsedGroup.ts" => {
            collapsed_group::draw(&node, &ctx)
        }
        "shapes/composite.js" | "shapes/composite.ts" => composite::draw(&node, &ctx),
        "shapes/console.js" | "shapes/console.ts" => console::draw(&node, &ctx),
        "shapes/crossedCircle.js" | "shapes/crossedCircle.ts" => crossed_circle::draw(&node, &ctx),
        "shapes/curlyBraceLeft.js" | "shapes/curlyBraceLeft.ts" => {
            curly_brace_left::draw(&node, &ctx)
        }
        "shapes/curlyBraceRight.js" | "shapes/curlyBraceRight.ts" => {
            curly_brace_right::draw(&node, &ctx)
        }
        "shapes/curlyBraces.js" | "shapes/curlyBraces.ts" => curly_braces::draw(&node, &ctx),
        "shapes/curvedTrapezoid.js" | "shapes/curvedTrapezoid.ts" => {
            curved_trapezoid::draw(&node, &ctx)
        }
        "shapes/cylinder.js" | "shapes/cylinder.ts" => cylinder::draw(&node, &ctx),
        "shapes/datastore.js" | "shapes/datastore.ts" => datastore::draw(&node, &ctx),
        "shapes/defaultMindmapNode.js" | "shapes/defaultMindmapNode.ts" => {
            default_mindmap_node::draw(&node, &ctx)
        }
        "shapes/dividedRect.js" | "shapes/dividedRect.ts" => divided_rect::draw(&node, &ctx),
        "shapes/document.js" | "shapes/document.ts" => document::draw(&node, &ctx),
        "shapes/doubleCircle.js" | "shapes/doubleCircle.ts" => double_circle::draw(&node, &ctx),
        "shapes/drawRect.js" | "shapes/drawRect.ts" => draw_rect::draw(&node, &ctx),
        "shapes/erBox.js" | "shapes/erBox.ts" => er_box::draw(&node, &ctx),
        "shapes/filledCircle.js" | "shapes/filledCircle.ts" => filled_circle::draw(&node, &ctx),
        "shapes/flippedTriangle.js" | "shapes/flippedTriangle.ts" => {
            flipped_triangle::draw(&node, &ctx)
        }
        "shapes/folder.js" | "shapes/folder.ts" => folder::draw(&node, &ctx),
        "shapes/forkJoin.js" | "shapes/forkJoin.ts" => fork_join::draw(&node, &ctx),
        "shapes/halfRoundedRectangle.js" | "shapes/halfRoundedRectangle.ts" => {
            half_rounded_rectangle::draw(&node, &ctx)
        }
        "shapes/handDrawnShapeStyles.js" | "shapes/handDrawnShapeStyles.ts" => {
            hand_drawn_shape_styles::draw(&node, &ctx)
        }
        "shapes/hexagon.js" | "shapes/hexagon.ts" => hexagon::draw(&node, &ctx),
        "shapes/hourglass.js" | "shapes/hourglass.ts" => hourglass::draw(&node, &ctx),
        "shapes/icon.js" | "shapes/icon.ts" => icon::draw(&node, &ctx),
        "shapes/iconCircle.js" | "shapes/iconCircle.ts" => icon_circle::draw(&node, &ctx),
        "shapes/iconRounded.js" | "shapes/iconRounded.ts" => icon_rounded::draw(&node, &ctx),
        "shapes/iconSquare.js" | "shapes/iconSquare.ts" => icon_square::draw(&node, &ctx),
        "shapes/imageSquare.js" | "shapes/imageSquare.ts" => image_square::draw(&node, &ctx),
        "shapes/insertPolygonShape.js" | "shapes/insertPolygonShape.ts" => {
            insert_polygon_shape::draw(&node, &ctx)
        }
        "shapes/invertedTrapezoid.js" | "shapes/invertedTrapezoid.ts" => {
            inverted_trapezoid::draw(&node, &ctx)
        }
        "shapes/kanbanItem.js" | "shapes/kanbanItem.ts" => kanban_item::draw(&node, &ctx),
        "shapes/labelImageUtils.js" | "shapes/labelImageUtils.ts" => {
            label_image_utils::draw(&node, &ctx)
        }
        "shapes/labelRect.js" | "shapes/labelRect.ts" => label_rect::draw(&node, &ctx),
        "shapes/leanLeft.js" | "shapes/leanLeft.ts" => lean_left::draw(&node, &ctx),
        "shapes/leanRight.js" | "shapes/leanRight.ts" => lean_right::draw(&node, &ctx),
        "shapes/lightningBolt.js" | "shapes/lightningBolt.ts" => lightning_bolt::draw(&node, &ctx),
        "shapes/linedCylinder.js" | "shapes/linedCylinder.ts" => lined_cylinder::draw(&node, &ctx),
        "shapes/linedWaveEdgedRect.js" | "shapes/linedWaveEdgedRect.ts" => {
            lined_wave_edged_rect::draw(&node, &ctx)
        }
        "shapes/mindmapCircle.js" | "shapes/mindmapCircle.ts" => mindmap_circle::draw(&node, &ctx),
        "shapes/multiRect.js" | "shapes/multiRect.ts" => multi_rect::draw(&node, &ctx),
        "shapes/multiWaveEdgedRectangle.js" | "shapes/multiWaveEdgedRectangle.ts" => {
            multi_wave_edged_rectangle::draw(&node, &ctx)
        }
        "shapes/note.js" | "shapes/note.ts" => note::draw(&node, &ctx),
        "shapes/person.js" | "shapes/person.ts" => person::draw(&node, &ctx),
        "shapes/question.js" | "shapes/question.ts" => question::draw(&node, &ctx),
        "shapes/rectLeftInvArrow.js" | "shapes/rectLeftInvArrow.ts" => {
            rect_left_inv_arrow::draw(&node, &ctx)
        }
        "shapes/rectWithTitle.js" | "shapes/rectWithTitle.ts" => rect_with_title::draw(&node, &ctx),
        "shapes/requirementBox.js" | "shapes/requirementBox.ts" => {
            requirement_box::draw(&node, &ctx)
        }
        "shapes/roundedRect.js" | "shapes/roundedRect.ts" => rounded_rect::draw(&node, &ctx),
        "shapes/roundedRectPath.js" | "shapes/roundedRectPath.ts" => {
            rounded_rect_path::draw(&node, &ctx)
        }
        "shapes/shadedProcess.js" | "shapes/shadedProcess.ts" => shaded_process::draw(&node, &ctx),
        "shapes/slopedRect.js" | "shapes/slopedRect.ts" => sloped_rect::draw(&node, &ctx),
        "shapes/squareRect.js" | "shapes/squareRect.ts" => square_rect::draw(&node, &ctx),
        "shapes/stadium.js" | "shapes/stadium.ts" => stadium::draw(&node, &ctx),
        "shapes/state.js" | "shapes/state.ts" => state::draw(&node, &ctx),
        "shapes/stateEnd.js" | "shapes/stateEnd.ts" => state_end::draw(&node, &ctx),
        "shapes/stateStart.js" | "shapes/stateStart.ts" => state_start::draw(&node, &ctx),
        "shapes/subroutine.js" | "shapes/subroutine.ts" => subroutine::draw(&node, &ctx),
        "shapes/taggedRect.js" | "shapes/taggedRect.ts" => tagged_rect::draw(&node, &ctx),
        "shapes/taggedWaveEdgedRectangle.js" | "shapes/taggedWaveEdgedRectangle.ts" => {
            tagged_wave_edged_rectangle::draw(&node, &ctx)
        }
        "shapes/text.js" | "shapes/text.ts" => text::draw(&node, &ctx),
        "shapes/tiltedCylinder.js" | "shapes/tiltedCylinder.ts" => {
            tilted_cylinder::draw(&node, &ctx)
        }
        "shapes/trapezoid.js" | "shapes/trapezoid.ts" => trapezoid::draw(&node, &ctx),
        "shapes/trapezoidalPentagon.js" | "shapes/trapezoidalPentagon.ts" => {
            trapezoidal_pentagon::draw(&node, &ctx)
        }
        "shapes/triangle.js" | "shapes/triangle.ts" => triangle::draw(&node, &ctx),
        "shapes/usecaseActor.js" | "shapes/usecaseActor.ts" => usecase_actor::draw(&node, &ctx),
        "shapes/usecaseActorAwesome.js" | "shapes/usecaseActorAwesome.ts" => {
            usecase_actor_awesome::draw(&node, &ctx)
        }
        "shapes/usecaseActorHollow.js" | "shapes/usecaseActorHollow.ts" => {
            usecase_actor_hollow::draw(&node, &ctx)
        }
        "shapes/usecaseActorIcon.js" | "shapes/usecaseActorIcon.ts" => {
            usecase_actor_icon::draw(&node, &ctx)
        }
        "shapes/usecaseBusiness.js" | "shapes/usecaseBusiness.ts" => {
            usecase_business::draw(&node, &ctx)
        }
        "shapes/usecaseEllipse.js" | "shapes/usecaseEllipse.ts" => {
            usecase_ellipse::draw(&node, &ctx)
        }
        "shapes/usecaseJsonTable.js" | "shapes/usecaseJsonTable.ts" => {
            usecase_json_table::draw(&node, &ctx)
        }
        "shapes/waveEdgedRectangle.js" | "shapes/waveEdgedRectangle.ts" => {
            wave_edged_rectangle::draw(&node, &ctx)
        }
        "shapes/waveRectangle.js" | "shapes/waveRectangle.ts" => wave_rectangle::draw(&node, &ctx),
        "shapes/windowPane.js" | "shapes/windowPane.ts" => window_pane::draw(&node, &ctx),
        _ => Err(ShapeError {
            message: format!(
                "shape not ported: {} (rendering-elements/{})",
                node.shape, shape.upstream_file
            ),
        }),
    }
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
    fn upstream_names_dispatch_to_their_files() {
        let entries = &[
            ("rect", "squareRect"),
            ("proc", "squareRect"),
            ("process", "squareRect"),
            ("rectangle", "squareRect"),
            ("squareRect", "squareRect"),
            ("rounded", "roundedRect"),
            ("event", "roundedRect"),
            ("roundedRect", "roundedRect"),
            ("stadium", "stadium"),
            ("terminal", "stadium"),
            ("pill", "stadium"),
            ("fr-rect", "subroutine"),
            ("subprocess", "subroutine"),
            ("subproc", "subroutine"),
            ("framed-rectangle", "subroutine"),
            ("subroutine", "subroutine"),
            ("cyl", "cylinder"),
            ("db", "cylinder"),
            ("database", "cylinder"),
            ("cylinder", "cylinder"),
            ("datastore", "datastore"),
            ("data-store", "datastore"),
            ("folder", "folder"),
            ("directory", "folder"),
            ("bucket", "bucket"),
            ("console", "console"),
            ("browser", "browser"),
            ("person", "person"),
            ("circle", "circle"),
            ("circ", "circle"),
            ("bang", "bang"),
            ("bang", "bang"),
            ("cloud", "cloud"),
            ("cloud", "cloud"),
            ("diam", "question"),
            ("decision", "question"),
            ("diamond", "question"),
            ("question", "question"),
            ("hex", "hexagon"),
            ("hexagon", "hexagon"),
            ("prepare", "hexagon"),
            ("lean-r", "leanRight"),
            ("lean-right", "leanRight"),
            ("in-out", "leanRight"),
            ("lean_right", "leanRight"),
            ("lean-l", "leanLeft"),
            ("lean-left", "leanLeft"),
            ("out-in", "leanLeft"),
            ("lean_left", "leanLeft"),
            ("trap-b", "trapezoid"),
            ("priority", "trapezoid"),
            ("trapezoid-bottom", "trapezoid"),
            ("trapezoid", "trapezoid"),
            ("trap-t", "invertedTrapezoid"),
            ("manual", "invertedTrapezoid"),
            ("trapezoid-top", "invertedTrapezoid"),
            ("inv-trapezoid", "invertedTrapezoid"),
            ("inv_trapezoid", "invertedTrapezoid"),
            ("dbl-circ", "doubleCircle"),
            ("double-circle", "doubleCircle"),
            ("doublecircle", "doubleCircle"),
            ("text", "text"),
            ("notch-rect", "card"),
            ("card", "card"),
            ("notched-rectangle", "card"),
            ("lin-rect", "shadedProcess"),
            ("lined-rectangle", "shadedProcess"),
            ("lined-process", "shadedProcess"),
            ("lin-proc", "shadedProcess"),
            ("shaded-process", "shadedProcess"),
            ("sm-circ", "stateStart"),
            ("start", "stateStart"),
            ("small-circle", "stateStart"),
            ("stateStart", "stateStart"),
            ("fr-circ", "stateEnd"),
            ("stop", "stateEnd"),
            ("framed-circle", "stateEnd"),
            ("stateEnd", "stateEnd"),
            ("fork", "forkJoin"),
            ("join", "forkJoin"),
            ("forkJoin", "forkJoin"),
            ("hourglass", "hourglass"),
            ("hourglass", "hourglass"),
            ("collate", "hourglass"),
            ("brace", "curlyBraceLeft"),
            ("comment", "curlyBraceLeft"),
            ("brace-l", "curlyBraceLeft"),
            ("brace-r", "curlyBraceRight"),
            ("braces", "curlyBraces"),
            ("bolt", "lightningBolt"),
            ("com-link", "lightningBolt"),
            ("lightning-bolt", "lightningBolt"),
            ("doc", "waveEdgedRectangle"),
            ("doc", "waveEdgedRectangle"),
            ("document", "waveEdgedRectangle"),
            ("delay", "halfRoundedRectangle"),
            ("half-rounded-rectangle", "halfRoundedRectangle"),
            ("h-cyl", "tiltedCylinder"),
            ("das", "tiltedCylinder"),
            ("horizontal-cylinder", "tiltedCylinder"),
            ("lin-cyl", "linedCylinder"),
            ("disk", "linedCylinder"),
            ("lined-cylinder", "linedCylinder"),
            ("curv-trap", "curvedTrapezoid"),
            ("curved-trapezoid", "curvedTrapezoid"),
            ("display", "curvedTrapezoid"),
            ("div-rect", "dividedRect"),
            ("div-proc", "dividedRect"),
            ("divided-rectangle", "dividedRect"),
            ("divided-process", "dividedRect"),
            ("tri", "triangle"),
            ("extract", "triangle"),
            ("triangle", "triangle"),
            ("win-pane", "windowPane"),
            ("internal-storage", "windowPane"),
            ("window-pane", "windowPane"),
            ("f-circ", "filledCircle"),
            ("junction", "filledCircle"),
            ("filled-circle", "filledCircle"),
            ("notch-pent", "trapezoidalPentagon"),
            ("loop-limit", "trapezoidalPentagon"),
            ("notched-pentagon", "trapezoidalPentagon"),
            ("flip-tri", "flippedTriangle"),
            ("manual-file", "flippedTriangle"),
            ("flipped-triangle", "flippedTriangle"),
            ("sl-rect", "slopedRect"),
            ("manual-input", "slopedRect"),
            ("sloped-rectangle", "slopedRect"),
            ("docs", "multiWaveEdgedRectangle"),
            ("documents", "multiWaveEdgedRectangle"),
            ("st-doc", "multiWaveEdgedRectangle"),
            ("stacked-document", "multiWaveEdgedRectangle"),
            ("st-rect", "multiRect"),
            ("procs", "multiRect"),
            ("processes", "multiRect"),
            ("stacked-rectangle", "multiRect"),
            ("bow-rect", "bowTieRect"),
            ("stored-data", "bowTieRect"),
            ("bow-tie-rectangle", "bowTieRect"),
            ("cross-circ", "crossedCircle"),
            ("summary", "crossedCircle"),
            ("crossed-circle", "crossedCircle"),
            ("tag-doc", "taggedWaveEdgedRectangle"),
            ("tag-doc", "taggedWaveEdgedRectangle"),
            ("tagged-document", "taggedWaveEdgedRectangle"),
            ("tag-rect", "taggedRect"),
            ("tagged-rectangle", "taggedRect"),
            ("tag-proc", "taggedRect"),
            ("tagged-process", "taggedRect"),
            ("flag", "waveRectangle"),
            ("paper-tape", "waveRectangle"),
            ("odd", "rectLeftInvArrow"),
            ("rect_left_inv_arrow", "rectLeftInvArrow"),
            ("lin-doc", "linedWaveEdgedRect"),
            ("lined-document", "linedWaveEdgedRect"),
            ("bpmn-start", "bpmnStart"),
            ("bpmn-intermediate", "bpmnIntermediate"),
            ("bpmn-boundary", "bpmnBoundary"),
            ("bpmn-end", "bpmnEnd"),
            ("bpmn-gateway", "bpmnGateway"),
            ("bpmn-activity", "bpmnActivity"),
            ("bpmn-data", "bpmnDataObject"),
            ("bpmn-data-store", "bpmnDataStore"),
            ("bpmn-annotation", "bpmnAnnotation"),
        ];
        for &(name, file) in entries {
            let definition = shape_definition(name).unwrap_or_else(|| panic!("missing {name}"));
            assert!(
                definition.upstream_file == format!("shapes/{file}.js")
                    || definition.upstream_file == format!("shapes/{file}.ts"),
                "{name}"
            );
            let result = render_node(
                Node::new("n", "label", name),
                &FallbackMeasure,
                &crate::theme::ThemeVariables::default(),
            );
            // A placeholder file still answers with an error naming its upstream file;
            // a ported one renders. Either way the name reached the right file.
            if let Err(error) = result {
                assert!(
                    definition.kind == ShapeKind::Unported
                        && error.message.contains(&format!("shapes/{file}.js")),
                    "{name}: {}",
                    error.message
                );
            }
        }
    }

    #[test]
    fn resolves_upstream_aliases() {
        assert_eq!(
            shape_definition("process").unwrap().upstream_file,
            "shapes/squareRect.ts"
        );
        assert_eq!(shape_definition("question").unwrap().short_name, "diam");
        assert!(
            render_node(
                Node::new("n", "x", "cyl"),
                &FallbackMeasure,
                &crate::theme::ThemeVariables::default(),
            )
            .is_ok()
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

    #[derive(serde::Deserialize)]
    struct ShapeFixture {
        shape: String,
        #[serde(default)]
        look: String,
        cases: Vec<ShapeCase>,
    }

    #[derive(serde::Deserialize)]
    struct ShapeCase {
        label: String,
        width: f64,
        height: f64,
        #[allow(dead_code)]
        #[serde(rename = "labelTransform")]
        label_transform: Option<String>,
        #[serde(default)]
        intersects: Vec<PointProbe>,
        #[serde(default)]
        probes: Vec<PointProbe>,
    }

    #[derive(serde::Deserialize)]
    struct PointProbe {
        x: f64,
        y: f64,
    }

    #[test]
    fn matches_upstream_shape_fixtures() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/render/testdata/shapes");
        let mut entries: Vec<_> = std::fs::read_dir(&dir)
            .expect("shapes testdata dir exists")
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
            .collect();
        entries.sort_by_key(|e| e.file_name());

        let mut overrides = serde_json::Map::new();
        overrides.insert("fontFamily".into(), serde_json::json!("monospace"));
        overrides.insert("fontSize".into(), serde_json::json!("16px"));
        let theme = crate::theme::Theme::Default.variables(&overrides).unwrap();

        let mut tested_count = 0;
        let mut differences = Vec::new();

        for entry in entries {
            let content = std::fs::read_to_string(entry.path()).expect("read fixture");
            let Ok(fixture) = serde_json::from_str::<ShapeFixture>(&content) else {
                continue;
            };
            if fixture.cases.is_empty()
                || fixture.cases[0].width == 0.0
                || fixture.cases[0].probes.is_empty()
            {
                continue;
            }

            for case in &fixture.cases {
                let mut node = Node::new("n", &case.label, &fixture.shape);
                node.padding = 8.0;
                node.look = if fixture.look.is_empty() {
                    "classic".into()
                } else {
                    fixture.look.clone()
                };
                if fixture.shape == "kanbanItem" || fixture.shape == "kanban_item" {
                    node.width = Some(200.0);
                }

                let rendered = match render_node(node, &FallbackMeasure, &theme) {
                    Ok(r) => r,
                    Err(e) => {
                        differences.push(format!("{}: render error: {}", fixture.shape, e.message));
                        continue;
                    }
                };

                let w_diff = (rendered.node.bounds.width - case.width).abs();
                let h_diff = (rendered.node.bounds.height - case.height).abs();
                if w_diff > 0.01 || h_diff > 0.01 {
                    differences.push(format!(
                        "{} ('{}'): size expected {}x{}, got {}x{} (diff {:.3}x{:.3})",
                        fixture.shape,
                        case.label,
                        case.width,
                        case.height,
                        rendered.node.bounds.width,
                        rendered.node.bounds.height,
                        w_diff,
                        h_diff
                    ));
                }

                for (probe, expected) in case.probes.iter().zip(&case.intersects) {
                    let hit = rendered.node.intersect(Point::new(probe.x, probe.y));
                    let x_diff = (hit.x - expected.x).abs();
                    let y_diff = (hit.y - expected.y).abs();
                    if x_diff > 0.01 || y_diff > 0.01 {
                        differences.push(format!(
                            "{} ('{}'): probe ({}, {}) expected ({}, {}), got ({:.3}, {:.3})",
                            fixture.shape,
                            case.label,
                            probe.x,
                            probe.y,
                            expected.x,
                            expected.y,
                            hit.x,
                            hit.y
                        ));
                    }
                }
            }
            tested_count += 1;
        }

        println!("Tested {tested_count} shape fixtures against upstream.");
        if !differences.is_empty() {
            panic!(
                "Differences in {} fixtures:\n{}",
                differences.len(),
                differences.join("\n")
            );
        }
    }
}
