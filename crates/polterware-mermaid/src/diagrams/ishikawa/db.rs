//! The writing side of upstream `ishikawaDb.ts`: `addNode` builds the cause tree as the
//! parser reduces each statement, and the first node also sets the diagram title.
//!
//! The tree is kept as an arena while parsing, because `addNode` walks a stack of open
//! parents: `nodes` holds every cause in insertion order and `children` holds indices into
//! it, which [`Db::into_ast`] folds into the nested tree of [`Ast`].

use super::ast::{Ast, IshikawaNode};
use super::sanitize;

/// The database while parsing.
pub(crate) struct Db {
    /// Every cause in insertion order; index `0` is the root once it exists.
    nodes: Vec<Pending>,
    /// The open parents, bottom first: the root at level `0`, then one per open cause.
    stack: Vec<StackEntry>,
    /// The first cause's indentation (`baseLevel`), which later ones are measured against.
    base_level: Option<usize>,
    diagram_title: String,
}

struct Pending {
    text: String,
    children: Vec<usize>,
}

#[derive(Clone, Copy)]
struct StackEntry {
    level: usize,
    node: usize,
}

impl Db {
    pub(crate) fn new() -> Self {
        Self {
            nodes: Vec::new(),
            stack: Vec::new(),
            base_level: None,
            diagram_title: String::new(),
        }
    }

    /// `addNode(rawLevel, text)`: the first call makes the root and the title; every later
    /// call hangs its node under the closest open parent with a strictly smaller level.
    ///
    /// `baseLevel` comes from the first *cause*, not from the effect: it is set on the first
    /// call after the root, so a more indented effect line does not flatten the causes.
    pub(crate) fn add_node(&mut self, raw_level: usize, text: &str) {
        let label = sanitize::sanitize_text(text);

        if self.nodes.is_empty() {
            self.nodes.push(Pending {
                text: label.clone(),
                children: Vec::new(),
            });
            self.stack = vec![StackEntry { level: 0, node: 0 }];
            // `setDiagramTitle(label)` sanitizes the already sanitized label again.
            self.diagram_title = sanitize::sanitize_text(&label);
            return;
        }

        let base_level = *self.base_level.get_or_insert(raw_level);
        let level = raw_level.saturating_sub(base_level) + 1;

        // Pop until the top has a strictly lower level, which is the parent.
        while self.stack.len() > 1 && self.stack[self.stack.len() - 1].level >= level {
            self.stack.pop();
        }

        let parent = self.stack[self.stack.len() - 1].node;
        let index = self.nodes.len();
        self.nodes.push(Pending {
            text: label,
            children: Vec::new(),
        });
        self.nodes[parent].children.push(index);
        self.stack.push(StackEntry { level, node: index });
    }

    pub(crate) fn into_ast(self) -> Ast {
        let Self {
            nodes,
            diagram_title,
            ..
        } = self;
        if nodes.is_empty() {
            return Ast::default();
        }
        let (links, mut slots): (Vec<Vec<usize>>, Vec<Option<IshikawaNode>>) = nodes
            .into_iter()
            .map(|pending| {
                (
                    pending.children,
                    Some(IshikawaNode {
                        text: pending.text,
                        children: Vec::new(),
                    }),
                )
            })
            .unzip();
        // A cause is always added after its parent, so walking backwards builds every child
        // before the parent that takes it.
        for index in (0..links.len()).rev() {
            let mut children = Vec::with_capacity(links[index].len());
            for &child in &links[index] {
                children.push(
                    slots[child]
                        .take()
                        .expect("a cause is built before its parent takes it"),
                );
            }
            slots[index]
                .as_mut()
                .expect("a cause is taken only once, by its parent")
                .children = children;
        }
        Ast {
            root: slots.into_iter().next().flatten(),
            diagram_title,
        }
    }
}
