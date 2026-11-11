//! The wardley database after parsing: what `WardleyBuilder.build()` returns
//! (`WardleyBuildResult` in `wardleyBuilder.ts`), plus the common fields `populateCommonDb`
//! sets.
//!
//! The builder keeps nodes, trends, and pipelines in `Map`s whose insertion order is
//! observable; here they are `Vec`s in the same order. Upstream leaves the optional fields
//! of a node either absent or `undefined`; here they are `Option`s. Coordinates are already
//! percentages: the database conversion in `wardleyParser.ts` (`toCoordinates`) maps a
//! 0-1 value to 0-100 and keeps a 0-100 value as-is.

/// A coordinate on the map, in percent.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// `WardleyBuildResult.size`: the `size [width, height]` directive.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

/// The `(build)`, `(buy)`, `(outsource)`, or `(market)` a component carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceStrategy {
    Build,
    Buy,
    Outsource,
    Market,
}

/// A link's flow, from a `+<>` / `+>` / `+<` port or a `+'label'>` arrow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    Forward,
    Backward,
    Bidirectional,
}

/// `WardleyNode`.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// The map key. Components and anchors use their name; a pipeline component uses
    /// `"{parent}_{name}"`.
    pub id: String,
    pub label: String,
    pub x: f64,
    pub y: f64,
    /// `"anchor"`, `"component"`, `"pipeline-component"`, or `None` after a merge.
    pub class_name: Option<String>,
    pub label_offset_x: Option<f64>,
    pub label_offset_y: Option<f64>,
    /// Set on a pipeline component after `addPipelineComponent`.
    pub in_pipeline: bool,
    /// Set on a pipeline parent by `startPipeline`.
    pub is_pipeline_parent: bool,
    pub inertia: bool,
    pub source_strategy: Option<SourceStrategy>,
}

/// `WardleyLink`.
#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub source: String,
    pub target: String,
    pub dashed: bool,
    pub label: Option<String>,
    pub flow: Option<Flow>,
}

/// `WardleyTrend`: one `evolve` statement resolved against a component.
#[derive(Clone, Debug, PartialEq)]
pub struct Trend {
    pub node_id: String,
    pub target_x: f64,
    pub target_y: f64,
}

/// `WardleyPipeline`: a parent node and the synthetic ids of its components.
#[derive(Clone, Debug, PartialEq)]
pub struct Pipeline {
    pub node_id: String,
    pub component_ids: Vec<String>,
}

/// `WardleyAnnotation`: one numbered note anchored at one or more points.
#[derive(Clone, Debug, PartialEq)]
pub struct Annotation {
    pub number: f64,
    pub coordinates: Vec<Point>,
    pub text: Option<String>,
}

/// `WardleyNote`.
#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub text: String,
    pub x: f64,
    pub y: f64,
}

/// `WardleyAccelerator`; `addAccelerator` receives the coordinates already converted.
#[derive(Clone, Debug, PartialEq)]
pub struct Accelerator {
    pub name: String,
    pub x: f64,
    pub y: f64,
}

/// `WardleyDeaccelerator`.
#[derive(Clone, Debug, PartialEq)]
pub struct Deaccelerator {
    pub name: String,
    pub x: f64,
    pub y: f64,
}

/// `WardleyAxesConfig`. The grammar only writes `stages` and `stageBoundaries`, and only
/// when an `evolution` line is present; both are `None` otherwise.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Axes {
    pub x_label: Option<String>,
    pub y_label: Option<String>,
    pub stages: Option<Vec<String>>,
    pub stage_boundaries: Option<Vec<f64>>,
}

/// What upstream's wardley database holds after parsing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ast {
    pub nodes: Vec<Node>,
    pub links: Vec<Link>,
    pub trends: Vec<Trend>,
    pub pipelines: Vec<Pipeline>,
    pub annotations: Vec<Annotation>,
    pub notes: Vec<Note>,
    pub accelerators: Vec<Accelerator>,
    pub deaccelerators: Vec<Deaccelerator>,
    pub annotations_box: Option<Point>,
    pub axes: Axes,
    pub size: Option<Size>,
    /// `getDiagramTitle()`; empty when the diagram has no title.
    pub diagram_title: String,
    /// `getAccTitle()`; empty when the diagram has no accessible title.
    pub acc_title: String,
    /// `getAccDescription()`; empty when the diagram has no accessible description.
    pub acc_description: String,
}

impl Ast {
    /// The value `getNode(id)` returns.
    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|node| node.id == id)
    }

    /// `resolveNodeId`: an exact id first, then the first node whose label matches, then the
    /// name unchanged.
    pub fn resolve_node_id<'a>(&'a self, name: &'a str) -> &'a str {
        if let Some(node) = self.nodes.iter().find(|node| node.id == name) {
            return &node.id;
        }
        if let Some(node) = self.nodes.iter().find(|node| node.label == name) {
            return &node.id;
        }
        name
    }
}
