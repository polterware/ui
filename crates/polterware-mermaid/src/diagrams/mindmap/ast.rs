//! The mindmap database: what upstream's `MindmapDB` holds once its parser has run, read
//! through the db's getters (`getMindmap`, `getType`, `type2Str`) and through the `getData`
//! projection a layout engine consumes.
//!
//! Upstream keeps every node in one array in insertion order and links children by object
//! reference. Here that array is a tree — [`Node::children`], whose preorder is the array's
//! order — and [`Node::id`] stays the insertion index, which `getData` writes into the node
//! and edge ids.
//!
//! Left out because it is configuration or layout output: `getData().config` (the caller's
//! [`crate::preprocess::Source::config`]) and `look`; `rootNode`, which is this tree again
//! ([`Ast::root`]); the `x`/`y` a layout engine writes onto each node, `undefined` at
//! `getData` time; and the `section` a `getData` call assigns, which is derived here per
//! [`ColorBy`] instead of stored, so repeated calls cannot leak a previous mode's colors.

/// Upstream `svgDraw.ts`'s `MAX_SECTIONS`: the palette, one color for the root and
/// `MAX_SECTIONS - 1` for the branches or depths that repeat through a diagram.
pub const MAX_SECTIONS: usize = 12;

/// Upstream's `nodeType` table.
///
/// `DEFAULT` and `NO_BORDER` are both `0`, so one variant covers them; a node without
/// delimiters and a `-)`/`(-` node both land here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NodeType {
    /// `DEFAULT` / `NO_BORDER`, `0`.
    #[default]
    NoBorder,
    /// `1`: `(text)`.
    RoundedRect,
    /// `2`: `[text]`.
    Rect,
    /// `3`: `((text))`.
    Circle,
    /// `4`: `)text(`.
    Cloud,
    /// `5`: `))text((`.
    Bang,
    /// `6`: `{{text}}`.
    Hexagon,
}

impl NodeType {
    /// The number upstream stores on the node (`nodeType`).
    pub fn as_number(self) -> u8 {
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

    /// `type2Str`: the name the renderer's stylesheet keys on, `hexgon` included.
    pub fn type2str(self) -> &'static str {
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

/// `config.mindmap.colorBy`: which palette slot a node takes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorBy {
    /// `branch`, the default: descendants inherit their top-level branch's color.
    #[default]
    Branch,
    /// `depth`: nodes at the same distance from the root share a color, across branches.
    Depth,
}

/// One node of the mindmap (`MindmapNode`).
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// The insertion index, which `getData` turns into the node and edge ids.
    pub id: usize,
    /// The id text (`nodeId`), or the description when the node had no id.
    pub node_id: String,
    /// The indentation, relative to the root's (`baseLevel` is subtracted).
    pub level: i32,
    /// The description as written (`descr`).
    pub descr: String,
    /// Upstream's `type`.
    pub node_type: NodeType,
    /// The children, in source order.
    pub children: Vec<Node>,
    /// The width limit (`config.mindmap.maxNodeWidth`; the default is not read from config
    /// but from upstream's default of `200`).
    pub width: f64,
    /// The height a layout engine measured, unset until then.
    pub height: Option<f64>,
    /// The text padding (`config.mindmap.padding`, default `10`, doubled for rounded,
    /// rectangular, and hexagonal nodes).
    pub padding: f64,
    /// Whether this is the diagram's root.
    pub is_root: bool,
    /// The `::icon(...)` decoration.
    pub icon: Option<String>,
    /// The `:::...` decoration.
    pub class: Option<String>,
}

impl Default for Node {
    fn default() -> Self {
        Self {
            id: 0,
            node_id: String::new(),
            level: 0,
            descr: String::new(),
            node_type: NodeType::NoBorder,
            children: Vec::new(),
            width: DEFAULT_MAX_NODE_WIDTH,
            height: None,
            padding: DEFAULT_PADDING,
            is_root: false,
            icon: None,
            class: None,
        }
    }
}

/// Upstream's `defaultConfig.mindmap.maxNodeWidth`.
pub const DEFAULT_MAX_NODE_WIDTH: f64 = 200.0;

/// Upstream's `defaultConfig.mindmap.padding`.
pub const DEFAULT_PADDING: f64 = 10.0;

/// The parsed diagram: what the database holds after parsing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ast {
    /// The root, `getMindmap`, and through its children the whole tree.
    pub root: Option<Node>,
}

impl Ast {
    /// `getData` with the default `colorBy` (`branch`): the nodes in tree preorder and the
    /// edges from each node to its children.
    pub fn data(&self) -> Data {
        self.data_with(ColorBy::Branch)
    }

    /// `getData` with `config.mindmap.colorBy` passed in, since configuration is not read
    /// here. `assignSections`, `flattenNodes`, and `generateEdges` run in this walk.
    pub fn data_with(&self, color_by: ColorBy) -> Data {
        let mut data = Data {
            nodes: Vec::new(),
            edges: Vec::new(),
            markers: vec!["point".to_string()],
            direction: "TB".to_string(),
            node_spacing: 50.0,
            rank_spacing: 50.0,
            type_name: "mindmap".to_string(),
        };
        if let Some(root) = &self.root {
            walk(root, None, 0, color_by, &mut data);
        }
        data
    }
}

/// `assignSections`: a node's palette slot, from the slot its parent handed it and its
/// distance from the root.
fn section_of(
    node: &Node,
    section_number: Option<usize>,
    depth: usize,
    color_by: ColorBy,
) -> Option<usize> {
    if node.level == 0 {
        None
    } else if color_by == ColorBy::Depth {
        Some(depth.saturating_sub(1) % (MAX_SECTIONS - 1))
    } else {
        section_number
    }
}

/// `assignSections`'s hand-off to a child: the root numbers its children by position, every
/// other node passes its own slot down.
fn child_section_number(node: &Node, section_number: Option<usize>, index: usize) -> Option<usize> {
    if node.level == 0 {
        Some(index % (MAX_SECTIONS - 1))
    } else {
        section_number
    }
}

/// `assignSections` + `flattenNodes` + `generateEdges`, which walk the tree in the same
/// order: an edge is emitted before the subtree it leads into.
fn walk(
    node: &Node,
    section_number: Option<usize>,
    depth: usize,
    color_by: ColorBy,
    data: &mut Data,
) {
    let section = section_of(node, section_number, depth, color_by);
    data.nodes.push(layout_node(node, section));
    for (index, child) in node.children.iter().enumerate() {
        let child_number = child_section_number(node, section_number, index);
        let child_section = section_of(child, child_number, depth + 1, color_by);
        data.edges.push(layout_edge(node, child, child_section));
        walk(child, child_number, depth + 1, color_by, data);
    }
}

/// `flattenNodes` for one node: the layout fields it computes, the `cssClasses` string
/// included.
fn layout_node(node: &Node, section: Option<usize>) -> LayoutNode {
    let mut css_classes = vec!["mindmap-node".to_string()];
    if node.is_root {
        css_classes.push("section-root".to_string());
        css_classes.push("section--1".to_string());
    } else if let Some(section) = section {
        css_classes.push(format!("section-{section}"));
    }
    if let Some(class) = &node.class {
        css_classes.push(class.clone());
    }
    LayoutNode {
        id: node.id.to_string(),
        dom_id: format!("node_{}", node.id),
        label: node.descr.clone(),
        label_type: "markdown".to_string(),
        is_group: false,
        shape: shape_of(node.node_type).to_string(),
        width: node.width,
        height: node.height.unwrap_or(0.0),
        padding: node.padding,
        css_classes: css_classes.join(" "),
        css_styles: Vec::new(),
        center_label: true,
        icon: node.icon.clone(),
        level: node.level,
        node_id: node.node_id.clone(),
        node_type: node.node_type,
        section,
    }
}

/// `getShapeFromType`. A theme can rename the default shape (`redux` draws `rounded`), which
/// is configuration and not read here.
fn shape_of(node_type: NodeType) -> &'static str {
    match node_type {
        NodeType::Circle => "mindmapCircle",
        NodeType::Rect => "rect",
        NodeType::RoundedRect => "rounded",
        NodeType::Cloud => "cloud",
        NodeType::Bang => "bang",
        NodeType::Hexagon => "hexagon",
        NodeType::NoBorder => "defaultMindmapNode",
    }
}

/// `generateEdges` for one parent-child pair.
fn layout_edge(node: &Node, child: &Node, section: Option<usize>) -> LayoutEdge {
    let mut classes = "edge".to_string();
    if let Some(section) = section {
        classes.push_str(&format!(" section-edge-{section}"));
    }
    classes.push_str(&format!(" edge-depth-{}", node.level + 1));
    LayoutEdge {
        id: format!("edge_{}_{}", node.id, child.id),
        start: node.id.to_string(),
        end: child.id.to_string(),
        type_name: "normal".to_string(),
        curve: "basis".to_string(),
        thickness: "normal".to_string(),
        classes,
        depth: node.level,
        section,
    }
}

/// One entry of `getData().nodes`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayoutNode {
    /// `id`, the insertion index as text.
    pub id: String,
    /// `domId`, `node_<id>`.
    pub dom_id: String,
    /// `label`, the description.
    pub label: String,
    /// `labelType`, always `markdown`.
    pub label_type: String,
    /// `isGroup`, always `false`: a mindmap has no clusters.
    pub is_group: bool,
    /// `shape`, from the node type.
    pub shape: String,
    pub width: f64,
    /// `height`, `0` until a layout engine measures the node.
    pub height: f64,
    pub padding: f64,
    /// `cssClasses`: `mindmap-node`, the section classes, and the node's own class.
    pub css_classes: String,
    /// `cssStyles`, which the mindmap grammar never fills.
    pub css_styles: Vec<String>,
    /// `centerLabel`, `true` for every shape (issue #8387).
    pub center_label: bool,
    pub icon: Option<String>,
    pub level: i32,
    pub node_id: String,
    pub node_type: NodeType,
    /// The palette slot from `assignSections`; `None` for the root.
    pub section: Option<usize>,
}

/// One entry of `getData().edges`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayoutEdge {
    /// `id`, `edge_<parent id>_<child id>`.
    pub id: String,
    pub start: String,
    pub end: String,
    /// `type`, always `normal`.
    pub type_name: String,
    /// `curve`, always `basis`.
    pub curve: String,
    /// `thickness`, always `normal`.
    pub thickness: String,
    /// `classes`: `edge`, the child's section class, and the edge's depth class.
    pub classes: String,
    /// `depth`, the parent's level.
    pub depth: i32,
    /// `section`, the child's section (and the edge's `section-edge-` class).
    pub section: Option<usize>,
}

/// `getData()`: what a layout engine receives, minus the configuration it also carries
/// upstream (`config`) and the `rootNode` alias of [`Ast::root`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Data {
    pub nodes: Vec<LayoutNode>,
    pub edges: Vec<LayoutEdge>,
    /// `markers`, always `["point"]`: mindmaps draw no arrow heads.
    pub markers: Vec<String>,
    /// `direction`, always `TB`.
    pub direction: String,
    /// `nodeSpacing`, always `50`.
    pub node_spacing: f64,
    /// `rankSpacing`, always `50`.
    pub rank_spacing: f64,
    /// `type`, always `mindmap`.
    pub type_name: String,
}
