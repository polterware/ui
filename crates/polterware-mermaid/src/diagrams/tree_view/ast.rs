//! The treeView database after parsing: what upstream's `TreeViewDB` getters return
//! (`treeView/db.ts` `getRoot`, `getCount`, `getDiagramTitle`, `getAccTitle`,
//! `getAccDescription`).
//!
//! `db.ts` keeps a stack of the open nodes and appends each parsed line under the last node
//! whose level is lower, so the database is a tree with a synthetic root at level `-1` and
//! name `/`. Ids are assigned by `addNode` from a counter that starts at 1, which is why
//! [`Ast::count`] is the number of real nodes plus the root.
//!
//! `Node.BBox` is left out: the renderer assigns it, and nothing here lays out.
//! `getConfig` is left out because this crate owns no configuration.

/// `NodeType` in `treeView/types.ts`: a name ending in `/` is a directory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeType {
    File,
    Directory,
}

/// One node, as `addNode` stores it (`Node` in `treeView/types.ts`).
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// `addNode`'s id; the synthetic root is 0.
    pub id: usize,
    /// `indent` in the grammar: the length of the indentation token, `0` when absent.
    /// The synthetic root is `-1`.
    pub level: i32,
    /// The name without its trailing `/`; the root is `/`.
    pub name: String,
    pub node_type: NodeType,
    /// `icon( ... )`: an iconify reference (`pack:name`), a built-in icon (`file`/`folder`),
    /// or `none`. `None` when the line has no annotation; the default is chosen at render
    /// time, which is out of scope here.
    pub icon: Option<String>,
    /// `:::class` annotation.
    pub css_class: Option<String>,
    /// `## description`, sanitized. `None` when the line has no annotation.
    pub description: Option<String>,
    pub children: Vec<Node>,
}

/// What upstream's treeView database holds after parsing.
#[derive(Clone, Debug, PartialEq)]
pub struct Ast {
    /// `getRoot()`: the synthetic `/` directory holding the parsed roots.
    pub root: Node,
    /// `getCount()`: `1` for an empty diagram, plus one per parsed node.
    pub count: usize,
    /// `getDiagramTitle()`; empty when the diagram has no title.
    pub diagram_title: String,
    /// `getAccTitle()`; empty when the diagram has no accessible title.
    pub acc_title: String,
    /// `getAccDescription()`; empty when the diagram has no accessible description.
    pub acc_description: String,
}

impl Default for Ast {
    /// `db.ts`'s state before any line is parsed: the synthetic root and a count of one.
    fn default() -> Self {
        Self {
            root: Node {
                id: 0,
                level: -1,
                name: "/".to_string(),
                node_type: NodeType::Directory,
                icon: None,
                css_class: None,
                description: None,
                children: Vec::new(),
            },
            count: 1,
            diagram_title: String::new(),
            acc_title: String::new(),
            acc_description: String::new(),
        }
    }
}
