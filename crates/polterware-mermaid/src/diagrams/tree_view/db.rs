//! The treeView database's writing side, ported from `treeView/db.ts` and the node loop of
//! `treeView/parser.ts` (`populate`).
//!
//! `parser.ts` parses the whole document first and then populates the database, so the order
//! of the writes is the one `populate` uses: the accessibility fields and the title, then
//! each node in grammar order. `populate` is also where a trailing `/` becomes a directory,
//! where an empty `icon()` becomes `none`, and where the description is sanitized.
//!
//! `db.ts` walks the nodes with a stack of the open ones: each new node pops every node whose
//! level is not lower than its own, then becomes the last child of what remains and joins the
//! stack. The stack always holds the synthetic root at level `-1`, so a level `0` node is a
//! child of the root.

use super::ast::{Ast, Node, NodeType};
use super::parser::{ParsedNode, ParsedTreeView};
use super::sanitize;

/// `populate`: applies the parsed document to a fresh database.
pub(crate) fn build(parsed: ParsedTreeView) -> Ast {
    let mut flat = vec![FlatNode {
        id: 0,
        level: -1,
        name: "/".to_string(),
        node_type: NodeType::Directory,
        icon: None,
        css_class: None,
        description: None,
        children: Vec::new(),
    }];
    let mut stack = vec![0usize];
    let mut count = 1usize;

    for node in parsed.nodes {
        let level = i32::try_from(node.level).unwrap_or(i32::MAX);
        // `while (level <= stack[stack.length - 1].level) stack.pop()`. The root stays: the
        // grammar's indentation is never negative.
        while stack.len() > 1 && flat[*stack.last().expect("never empty")].level >= level {
            stack.pop();
        }
        let index = flat.len();
        flat.push(node.into_flat(count, level));
        count += 1;
        flat[*stack.last().expect("never empty")]
            .children
            .push(index);
        stack.push(index);
    }

    Ast {
        root: nest(&flat, 0),
        count,
        diagram_title: sanitize::sanitize_text(parsed.title.as_deref().unwrap_or_default()),
        acc_title: sanitize::set_acc_title(parsed.acc_title.as_deref().unwrap_or_default()),
        acc_description: sanitize::set_acc_description(
            parsed.acc_description.as_deref().unwrap_or_default(),
        ),
    }
}

/// A node before its children are nested, indexed by position in the vector.
struct FlatNode {
    id: usize,
    level: i32,
    name: String,
    node_type: NodeType,
    icon: Option<String>,
    css_class: Option<String>,
    description: Option<String>,
    children: Vec<usize>,
}

impl ParsedNode {
    /// The `Node` `populate` builds: a trailing `/` makes a directory, an empty `icon()`
    /// means `none`, and an empty class or description stays unset.
    fn into_flat(self, id: usize, level: i32) -> FlatNode {
        let (name, node_type) = if self.name.ends_with('/') {
            let mut name = self.name;
            name.pop();
            (name, NodeType::Directory)
        } else {
            (self.name, NodeType::File)
        };
        let icon = self.icon_annotation.map(|raw| {
            if raw.is_empty() {
                "none".to_string()
            } else {
                raw
            }
        });
        let css_class = self.class_annotation.filter(|class| !class.is_empty());
        let description = self
            .description
            .filter(|text| !text.is_empty())
            .map(|text| sanitize::sanitize_text(&text));
        FlatNode {
            id,
            level,
            name,
            node_type,
            icon,
            css_class,
            description,
            children: Vec::new(),
        }
    }
}

/// Turns the flat parents-and-children graph into the tree the database exposes.
fn nest(flat: &[FlatNode], index: usize) -> Node {
    let node = &flat[index];
    Node {
        id: node.id,
        level: node.level,
        name: node.name.clone(),
        node_type: node.node_type,
        icon: node.icon.clone(),
        css_class: node.css_class.clone(),
        description: node.description.clone(),
        children: node
            .children
            .iter()
            .map(|&child| nest(flat, child))
            .collect(),
    }
}
