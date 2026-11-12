//! Port of upstream `rendering-util/types.ts`: data types for diagram layout and rendering.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::scene::Point;

pub type MarkdownWordType = String;

/// One line of a class box: a member or a method, as `classBox.ts` reads it from the
/// `ClassNode` that `getData()` spreads into the layout node. `text` is the member's
/// display text and `css_style` its `parseClassifier()` style.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassMemberData {
    pub text: String,
    #[serde(default)]
    pub css_style: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MarkdownWord {
    pub content: String,
    pub r#type: MarkdownWordType,
}

pub type MarkdownLine = Vec<MarkdownWord>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelBBox {
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GroupTitleRect {
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TerminalLabelSize {
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeAttribute {
    /// The declared type (`Attribute.type`).
    #[serde(rename = "type")]
    pub type_name: String,
    pub name: String,
    #[serde(default)]
    pub keys: Vec<String>,
    #[serde(default)]
    pub comment: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequirementNodeData {
    /// `Requirement.type`, the requirement kind's display string.
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(default)]
    pub requirement_id: String,
    #[serde(default)]
    pub text: String,
    /// `Requirement.risk`; empty when the requirement has none.
    #[serde(default)]
    pub risk: String,
    /// `Requirement.verifyMethod`; empty when the requirement has none.
    #[serde(default)]
    pub verify_method: String,
}

/// `Element`'s fields, as `requirementBox.ts` reads them from the layout node.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementNodeData {
    /// `Element.type`.
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(default)]
    pub doc_ref: String,
}

/// One `UsecaseJsonRow`: the visible key, the key repeated for assistive technology, and
/// the value.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonRowData {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub accessible_key: String,
    #[serde(default)]
    pub value: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    /// `Requirement`'s payload when the node is a requirement; `None` for an element.
    #[serde(default)]
    pub requirement: Option<RequirementNodeData>,
    /// `Element`'s payload when the node is an element; `None` for a requirement.
    #[serde(default)]
    pub element: Option<ElementNodeData>,
    /// `ClassNode.text`: the label with the generic type escaped, which the class box draws
    /// instead of `label`.
    #[serde(default)]
    pub text: Option<String>,
    /// `ClassNode.members` and `ClassNode.methods`, the rows a class box draws.
    #[serde(default)]
    pub members: Vec<ClassMemberData>,
    #[serde(default)]
    pub methods: Vec<ClassMemberData>,
    /// `ClassNode.annotations`, the `«interface»` row.
    #[serde(default)]
    pub annotations: Vec<String>,
    /// `ClassNode.styles`, the `style`/`classDef` declarations applied to the box.
    #[serde(default)]
    pub styles: Vec<String>,
    /// `config.class.padding`; `classBox.ts` reads it from the configuration, not the node.
    #[serde(default)]
    pub class_padding: Option<f64>,
    /// `config.class.hideEmptyMembersBox`.
    #[serde(default)]
    pub hide_empty_members_box: Option<bool>,
    #[serde(default)]
    pub description: Option<Vec<String>>,
    #[serde(default)]
    pub stereotype: Option<String>,
    /// `UsecaseLayoutNode.business`: a business actor or use case.
    #[serde(default)]
    pub business: Option<bool>,
    /// `UsecaseLayoutNode.jsonRows`: the key/value rows a JSON table draws.
    #[serde(default)]
    pub json_rows: Vec<JsonRowData>,
    /// `UsecaseLayoutCluster.boundaryType`: the geometry a system boundary draws.
    #[serde(default)]
    pub boundary_type: Option<String>,
    /// The label's font size when the diagram's stylesheet gives this node a size of its
    /// own (`--mermaid-usecase-actor-font-size` and `--mermaid-usecase-font-size`); absent
    /// means the theme's `fontSize`.
    #[serde(default)]
    pub label_font_size: Option<f64>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub position: Option<String>,
    #[serde(default)]
    pub css_styles: Option<Vec<String>>,
    #[serde(default)]
    pub css_compiled_styles: Option<Vec<String>>,
    #[serde(default)]
    pub css_classes: Option<String>,
    #[serde(default)]
    pub label_style: Option<String>,
    #[serde(default)]
    pub label_type: Option<String>,
    #[serde(default)]
    pub dom_id: Option<String>,
    #[serde(default)]
    pub dir: Option<String>,
    #[serde(default)]
    pub have_callback: Option<bool>,
    #[serde(default)]
    pub link: Option<String>,
    #[serde(default)]
    pub link_target: Option<String>,
    #[serde(default)]
    pub tooltip: Option<String>,
    #[serde(default)]
    pub padding: Option<f64>,
    #[serde(default)]
    pub is_group: Option<bool>,
    #[serde(default)]
    pub width: Option<f64>,
    #[serde(default)]
    pub height: Option<f64>,
    #[serde(default)]
    pub wrapping_width: Option<f64>,
    #[serde(default)]
    pub min_width: Option<f64>,
    #[serde(default)]
    pub spread_ports: Option<bool>,
    #[serde(default)]
    pub label_b_box: Option<LabelBBox>,
    #[serde(default)]
    pub group_title_rect: Option<GroupTitleRect>,
    #[serde(default)]
    pub rx: Option<f64>,
    #[serde(default)]
    pub ry: Option<f64>,
    #[serde(default)]
    pub use_html_labels: Option<bool>,
    #[serde(default)]
    pub center_label: Option<bool>,
    #[serde(default)]
    pub background_color: Option<String>,
    #[serde(default)]
    pub border_color: Option<String>,
    #[serde(default)]
    pub border_style: Option<String>,
    #[serde(default)]
    pub border_width: Option<f64>,
    #[serde(default)]
    pub label_text_color: Option<String>,
    #[serde(default)]
    pub label_padding_x: Option<f64>,
    #[serde(default)]
    pub label_padding_y: Option<f64>,
    #[serde(default)]
    pub x: Option<f64>,
    #[serde(default)]
    pub y: Option<f64>,
    #[serde(default)]
    pub look: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub pos: Option<String>,
    #[serde(default)]
    pub img: Option<String>,
    #[serde(default)]
    pub asset_width: Option<f64>,
    #[serde(default)]
    pub asset_height: Option<f64>,
    #[serde(default)]
    pub default_width: Option<f64>,
    #[serde(default)]
    pub image_aspect_ratio: Option<f64>,
    #[serde(default)]
    pub constraint: Option<String>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub layer: Option<f64>,
    #[serde(default)]
    pub order: Option<f64>,
    #[serde(default)]
    pub node_id: Option<String>,
    #[serde(default)]
    pub level: Option<f64>,
    #[serde(default)]
    pub descr: Option<String>,
    #[serde(default)]
    pub r#type: Option<f64>,
    #[serde(default)]
    pub radius: Option<f64>,
    #[serde(default)]
    pub taper: Option<f64>,
    #[serde(default)]
    pub stroke: Option<String>,
    #[serde(default)]
    pub color_index: Option<usize>,
    /// ER entity attributes (`EntityNode.attributes`); empty for every other diagram.
    #[serde(default)]
    pub attributes: Vec<NodeAttribute>,
    /// The ER entity alias from `name[alias]`, if the diagram declared one.
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub shape: Option<String>,
    #[serde(default)]
    pub is_edge_label: Option<bool>,
    #[serde(default)]
    pub edge_start: Option<String>,
    #[serde(default)]
    pub edge_end: Option<String>,
    #[serde(default)]
    pub is_dummy: Option<bool>,
    #[serde(default)]
    pub diff: Option<f64>,
    #[serde(default)]
    pub cluster_node: Option<bool>,
    #[serde(default)]
    pub bounds: crate::scene::Rect,
}

impl Node {
    pub fn new(id: impl Into<String>, label: impl Into<String>, shape: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: Some(label.into()),
            shape: Some(shape.into()),
            ..Default::default()
        }
    }

    pub fn new_group(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: Some(label.into()),
            is_group: Some(true),
            shape: Some("rect".into()),
            ..Default::default()
        }
    }
}

pub type ClusterNode = Node;
pub type NonClusterNode = Node;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Edge {
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub classes: Option<String>,
    #[serde(default)]
    pub style: Option<Vec<String>>,
    #[serde(default)]
    pub animate: Option<bool>,
    #[serde(default)]
    pub animation: Option<String>,
    #[serde(default)]
    pub step: Option<f64>,
    #[serde(default)]
    pub step_count: Option<f64>,
    #[serde(default)]
    pub step_duration: Option<f64>,
    #[serde(default)]
    pub show_step_number: Option<bool>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub arrowhead: Option<String>,
    #[serde(default)]
    pub arrowhead_style: Option<String>,
    #[serde(default)]
    pub arrow_type_end: Option<String>,
    #[serde(default)]
    pub arrow_type_start: Option<String>,
    #[serde(default)]
    pub css_compiled_styles: Option<Vec<String>>,
    #[serde(default)]
    pub default_interpolate: Option<String>,
    #[serde(default)]
    pub end: Option<String>,
    #[serde(default)]
    pub interpolate: Option<String>,
    #[serde(default)]
    pub label_type: Option<String>,
    #[serde(default)]
    pub length: Option<f64>,
    #[serde(default)]
    pub start: Option<String>,
    #[serde(default)]
    pub stroke: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub start_label_right: Option<String>,
    #[serde(default)]
    pub start_label_left: Option<String>,
    #[serde(default)]
    pub end_label_left: Option<String>,
    #[serde(default)]
    pub end_label_right: Option<String>,
    #[serde(default)]
    pub terminal_label_sizes: Option<HashMap<String, TerminalLabelSize>>,
    #[serde(default)]
    pub terminal_label_centers: Option<HashMap<String, Point>>,
    #[serde(default)]
    pub curve: Option<String>,
    #[serde(default)]
    pub corner_radius: Option<f64>,
    #[serde(default)]
    pub port_clipping: Option<String>,
    #[serde(default)]
    pub skip_corner_fix: Option<bool>,
    #[serde(default)]
    pub labelpos: Option<String>,
    #[serde(default)]
    pub label_style: Option<Vec<String>>,
    #[serde(default)]
    pub minlen: Option<f64>,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub thickness: Option<String>,
    #[serde(default)]
    pub look: Option<String>,
    #[serde(default)]
    pub is_user_defined_id: Option<bool>,
    #[serde(default)]
    pub show_points: Option<bool>,
    #[serde(default)]
    pub width: Option<f64>,
    #[serde(default)]
    pub height: Option<f64>,
    #[serde(default)]
    pub x: Option<f64>,
    #[serde(default)]
    pub y: Option<f64>,
    #[serde(default)]
    pub points: Option<Vec<Point>>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub dir: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub depth: Option<f64>,
    #[serde(default)]
    pub is_label_edge: Option<bool>,
    #[serde(default)]
    pub label_node_id: Option<String>,
    #[serde(default)]
    pub is_layout_only: Option<bool>,
    #[serde(default)]
    pub from_cluster: Option<String>,
    #[serde(default)]
    pub to_cluster: Option<String>,
}

impl Edge {
    pub fn new(id: impl Into<String>, start: impl Into<String>, end: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            start: Some(start.into()),
            end: Some(end.into()),
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RectOptions {
    pub rx: f64,
    pub ry: f64,
    pub label_padding_x: f64,
    pub label_padding_y: f64,
    pub classes: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MindmapOptions {
    pub padding: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutData {
    #[serde(default)]
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub edges: Vec<Edge>,
    #[serde(default)]
    pub config: Config,
    #[serde(default)]
    pub diagram_id: Option<String>,
    #[serde(default)]
    pub layout_algorithm: Option<String>,
    #[serde(default)]
    pub direction: Option<String>,
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub markers: Vec<String>,
    #[serde(default)]
    pub use_max_width: Option<bool>,
    #[serde(default)]
    pub padding: Option<f64>,
    #[serde(default)]
    pub node_spacing: Option<f64>,
    #[serde(default)]
    pub rank_spacing: Option<f64>,
    #[serde(default)]
    pub root_node: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RenderOptions {
    pub algorithm: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateGraphOptions {
    pub unwrap_group_labels: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShapeRenderOptions {
    pub config: Config,
    pub dir: Option<String>,
    pub padding: Option<f64>,
}
