//! The kanban database after parsing: what upstream's `kanbanDb.ts` holds.
//!
//! Upstream keeps two lists, `nodes` in grammar order and `sections` in the order they were
//! found, and every item carries the `parentId` the database resolved from its indentation.
//! [`Node`] is that record: the fields `addNode` and `decorateNode` write, minus the
//! configuration-derived `width` and `padding` (this crate owns no configuration).
//!
//! [`Ast::data`] is the `getData` projection: the sections first, each followed by its items,
//! with the labels sanitized again and the shapes and `rx`/`ry` a renderer reads. It is data
//! only; nothing here lays out or draws.

use serde_json::Value;

use crate::common::sanitize;

/// The grammar's node type numbers (`kanbanDb.ts` `nodeType`). `DEFAULT` and `NO_BORDER` are
/// both 0 upstream, so there is one variant.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NodeType {
    #[default]
    NoBorder,
    RoundedRect,
    Rect,
    Circle,
    Cloud,
    Bang,
    Hexagon,
}

impl NodeType {
    /// The number `addNode` stores (`nodeType`).
    pub fn number(self) -> u8 {
        match self {
            NodeType::NoBorder => 0,
            NodeType::RoundedRect => 1,
            NodeType::Rect => 2,
            NodeType::Circle => 3,
            NodeType::Cloud => 4,
            NodeType::Bang => 5,
            NodeType::Hexagon => 6,
        }
    }

    /// `type2Str`.
    pub fn as_str(self) -> &'static str {
        match self {
            NodeType::NoBorder => "no-border",
            NodeType::Rect => "rect",
            NodeType::RoundedRect => "rounded-rect",
            NodeType::Circle => "circle",
            NodeType::Cloud => "cloud",
            NodeType::Bang => "bang",
            NodeType::Hexagon => "hexgon",
        }
    }
}

/// One database node (`KanbanNode` in `rendering-util/types.ts`, as `addNode` fills it).
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// `sanitizeText(id)`, or the generated `kbn<cnt>` when that is empty.
    pub id: String,
    /// `sanitizeText(descr)`, replaced by the metadata `label` when the block sets one.
    pub label: String,
    /// The indentation of the item, in characters (`SPACELIST.length`).
    pub level: usize,
    /// The section this item was attached to; `None` for a section.
    pub parent_id: Option<String>,
    pub node_type: NodeType,
    /// Only ever `"kanbanItem"`, the one shape the metadata may set.
    pub shape: Option<String>,
    pub icon: Option<String>,
    pub css_classes: Option<String>,
    pub assigned: Option<String>,
    pub ticket: Option<String>,
    /// The metadata `priority` value as written (a string, number, or boolean).
    pub priority: Option<Value>,
}

/// What upstream's kanban database holds after parsing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ast {
    /// `getSections()`, in the order the database found them.
    pub sections: Vec<Node>,
    /// Every node in grammar order, sections included.
    pub nodes: Vec<Node>,
}

impl Ast {
    /// The nodes `getData` attaches to `section_id`, in grammar order.
    pub fn children<'a>(&'a self, section_id: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.nodes
            .iter()
            .filter(move |node| node.parent_id.as_deref() == Some(section_id))
    }

    /// `getData().nodes`: each section followed by its own items.
    ///
    /// The configuration fields upstream copies in (`look`, and the section `width` and
    /// `padding`) are left out because this crate owns no configuration.
    pub fn data(&self) -> Vec<DataNode> {
        let mut data = Vec::new();
        for section in &self.sections {
            data.push(DataNode {
                id: section.id.clone(),
                parent_id: None,
                label: sanitize::sanitize_text(&section.label),
                label_type: "markdown",
                is_group: true,
                ticket: section.ticket.clone(),
                priority: None,
                assigned: None,
                icon: None,
                shape: DataShape::Section,
                level: section.level,
                rx: None,
                ry: None,
                css_styles: Vec::new(),
            });
            for item in self.children(&section.id) {
                data.push(DataNode {
                    id: item.id.clone(),
                    parent_id: Some(section.id.clone()),
                    label: sanitize::sanitize_text(&item.label),
                    label_type: "markdown",
                    is_group: false,
                    ticket: item.ticket.clone(),
                    priority: item.priority.clone(),
                    assigned: item.assigned.clone(),
                    icon: item.icon.clone(),
                    shape: DataShape::Item,
                    level: item.level,
                    rx: Some(5.0),
                    ry: Some(5.0),
                    css_styles: vec!["text-align: left"],
                });
            }
        }
        data
    }
}

/// The `shape` field of a `getData` node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataShape {
    Section,
    Item,
}

impl DataShape {
    pub fn as_str(self) -> &'static str {
        match self {
            DataShape::Section => "kanbanSection",
            DataShape::Item => "kanbanItem",
        }
    }
}

/// One node of [`Ast::data`].
#[derive(Clone, Debug, PartialEq)]
pub struct DataNode {
    pub id: String,
    /// Set on items only, as `getData` does.
    pub parent_id: Option<String>,
    pub label: String,
    /// Always `"markdown"`.
    pub label_type: &'static str,
    pub is_group: bool,
    pub ticket: Option<String>,
    pub priority: Option<Value>,
    pub assigned: Option<String>,
    pub icon: Option<String>,
    pub shape: DataShape,
    pub level: usize,
    pub rx: Option<f64>,
    pub ry: Option<f64>,
    pub css_styles: Vec<&'static str>,
}
