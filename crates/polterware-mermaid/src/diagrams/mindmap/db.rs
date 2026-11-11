//! The semantics of upstream `mindmapDb.ts`: `addNode`, `decorateNode`, `getParent`,
//! `getMindmap`, `getType`, and the tree `getData` reads, applied by the parser as it
//! reduces each statement.
//!
//! `sanitizeText` (DOMPurify) is not applied to ids, descriptions, icons, or classes;
//! sanitizing belongs to the painter, which knows its security level. Configuration is not
//! read either: a node gets upstream's default width and padding.

use super::ast::{Ast, DEFAULT_MAX_NODE_WIDTH, DEFAULT_PADDING, Node, NodeType};

/// The database while parsing: the nodes in insertion order, with their children as indices
/// into that array, which is upstream's `children` back-reference.
pub(crate) struct Db {
    nodes: Vec<Pending>,
    /// The first node's indentation (`baseLevel`).
    base_level: Option<i32>,
}

struct Pending {
    node: Node,
    children: Vec<usize>,
}

impl Db {
    pub(crate) fn new() -> Self {
        Self {
            nodes: Vec::new(),
            base_level: None,
        }
    }

    /// `addNode`: the first node becomes the root at level `0`; every other node hangs under
    /// the closest preceding node with a smaller level.
    ///
    /// The error is upstream's throw for a node with no parent, message included; the caller
    /// adds the span.
    pub(crate) fn add_node(
        &mut self,
        level: i32,
        id: &str,
        descr: &str,
        node_type: NodeType,
    ) -> Result<(), String> {
        let (level, is_root) = match self.base_level {
            None => {
                self.base_level = Some(level);
                (0, true)
            }
            Some(base_level) => (level - base_level, false),
        };
        let node = Node {
            id: self.nodes.len(),
            node_id: id.to_string(),
            level,
            descr: descr.to_string(),
            node_type,
            children: Vec::new(),
            width: DEFAULT_MAX_NODE_WIDTH,
            height: None,
            padding: padding_for(node_type),
            is_root,
            icon: None,
            class: None,
        };
        match self.parent(level) {
            Some(parent) => {
                self.nodes.push(Pending {
                    node,
                    children: Vec::new(),
                });
                let index = self.nodes.len() - 1;
                self.nodes[parent].children.push(index);
                Ok(())
            }
            None if is_root => {
                self.nodes.push(Pending {
                    node,
                    children: Vec::new(),
                });
                Ok(())
            }
            None => Err(format!(
                "There can be only one root. No parent could be found for (\"{descr}\")"
            )),
        }
    }

    /// `decorateNode({ icon })`: the last node added takes the icon. Upstream indexes into an
    /// empty array (and throws) when no node was added yet; that becomes an error here.
    pub(crate) fn decorate_icon(&mut self, icon: &str) -> Result<(), &'static str> {
        let pending = self.nodes.last_mut().ok_or(NO_NODE)?;
        pending.node.icon = Some(icon.to_string());
        Ok(())
    }

    /// `decorateNode({ class })`: the last node added takes the classes.
    pub(crate) fn decorate_class(&mut self, class: &str) -> Result<(), &'static str> {
        let pending = self.nodes.last_mut().ok_or(NO_NODE)?;
        pending.node.class = Some(class.to_string());
        Ok(())
    }

    /// `getParent`: the closest preceding node with a smaller level.
    fn parent(&self, level: i32) -> Option<usize> {
        self.nodes
            .iter()
            .rposition(|pending| pending.node.level < level)
    }

    pub(crate) fn into_ast(self) -> Ast {
        let Self { nodes, .. } = self;
        if nodes.is_empty() {
            return Ast::default();
        }
        let (children, mut slots): (Vec<Vec<usize>>, Vec<Option<Node>>) = nodes
            .into_iter()
            .map(|pending| (pending.children, Some(pending.node)))
            .unzip();
        let root = build(&children, &mut slots, 0);
        Ast { root: Some(root) }
    }
}

/// The message [`Db::decorate_icon`] and [`Db::decorate_class`] answer with no node.
pub(crate) const NO_NODE: &str = "There is no node to decorate";

/// Folds the insertion-order array into a tree: the children indices always point forward,
/// because a node is added before the nodes under it.
fn build(children: &[Vec<usize>], slots: &mut [Option<Node>], index: usize) -> Node {
    let mut node = slots[index].take().expect("each node is built once");
    node.children = children[index]
        .iter()
        .map(|&child| build(children, slots, child))
        .collect();
    node
}

/// `addNode`'s padding: doubled for the shapes that draw a border with text inside.
fn padding_for(node_type: NodeType) -> f64 {
    match node_type {
        NodeType::RoundedRect | NodeType::Rect | NodeType::Hexagon => DEFAULT_PADDING * 2.0,
        _ => DEFAULT_PADDING,
    }
}

/// `getType`: the node type a pair of delimiters draws.
///
/// `-)` and `(-` have no case, so they fall through to `DEFAULT`, exactly as upstream.
pub(crate) fn node_type(start: &str, end: &str) -> NodeType {
    match start {
        "[" => NodeType::Rect,
        "(" if end == ")" => NodeType::RoundedRect,
        "(" => NodeType::Cloud,
        "((" => NodeType::Circle,
        ")" => NodeType::Cloud,
        "))" => NodeType::Bang,
        "{{" => NodeType::Hexagon,
        _ => NodeType::NoBorder,
    }
}
