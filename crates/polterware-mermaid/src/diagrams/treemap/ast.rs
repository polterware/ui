//! The treemap database after parsing: what upstream's `TreeMapDB` getters return
//! (`db.ts` `getNodes`, `getRoot`, `getClasses`, `getStylesForClass`) plus the `levels` map
//! and the three strings shared with `diagrams/common/commonDb.ts` (`getDiagramTitle`,
//! `getAccTitle`, `getAccDescription`).
//!
//! Upstream keeps `nodes` and `outerNodes` as arrays of the same node objects, and `levels`
//! as a `Map` keyed by node identity. Here [`Ast::nodes`] holds each node with its subtree,
//! [`Ast::levels`] holds one depth per entry of `nodes` in the same order, and
//! [`Ast::root`] is the fresh `{ name: '', children: outerNodes }` object `getRoot` builds.
//! The `parent` property of `TreemapNode` is never set by `buildHierarchy`, so it is absent.

/// One `TreemapNode` from `types.ts`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TreemapNode {
    pub name: String,
    /// `children`: `None` on a leaf, `Some` (possibly empty) on a section.
    pub children: Option<Vec<TreemapNode>>,
    /// `value`: only a leaf carries one.
    pub value: Option<f64>,
    /// `classSelector`, from the `:::class` suffix.
    pub class_selector: Option<String>,
    /// `cssCompiledStyles`: the class's styles, when the class exists.
    pub css_compiled_styles: Option<Vec<String>>,
}

/// One entry of `getClasses()`, a `DiagramStyleClassDef` as `addClass` leaves it.
#[derive(Clone, Debug, PartialEq)]
pub struct StyleClass {
    pub id: String,
    /// The style declarations, in the order `addClass` splits them.
    pub styles: Vec<String>,
    /// The subset of `styles` that `isLabelStyle` recognizes.
    pub text_styles: Vec<String>,
}

/// What upstream's treemap database holds after parsing.
#[derive(Clone, Debug, PartialEq)]
pub struct Ast {
    /// `getNodes()`, in the order `addNodesRecursively` adds them (depth first).
    pub nodes: Vec<TreemapNode>,
    /// `levels`, one depth per entry of [`Ast::nodes`].
    pub levels: Vec<usize>,
    /// `getRoot()`: `{ name: '', children: outerNodes }`.
    pub root: TreemapNode,
    /// `getClasses()`, in insertion order.
    pub classes: Vec<StyleClass>,
    /// `getDiagramTitle()`; empty when the diagram has no title.
    pub diagram_title: String,
    /// `getAccTitle()`; empty when the diagram has no accessible title.
    pub acc_title: String,
    /// `getAccDescription()`; empty when the diagram has no accessible description.
    pub acc_description: String,
}

impl Default for Ast {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            levels: Vec::new(),
            root: TreemapNode {
                name: String::new(),
                children: Some(Vec::new()),
                ..TreemapNode::default()
            },
            classes: Vec::new(),
            diagram_title: String::new(),
            acc_title: String::new(),
            acc_description: String::new(),
        }
    }
}

impl Ast {
    /// `getClasses().get(id)`.
    pub fn class(&self, id: &str) -> Option<&StyleClass> {
        self.classes.iter().find(|class| class.id == id)
    }

    /// `getStylesForClass(classSelector)`, `[]` when the class does not exist.
    pub fn styles_for_class(&self, class_selector: &str) -> &[String] {
        self.class(class_selector)
            .map_or(&[], |class| class.styles.as_slice())
    }
}
