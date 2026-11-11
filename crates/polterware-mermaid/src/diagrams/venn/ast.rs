//! The venn database after parsing: what upstream's `vennDB.ts` getters answer
//! (`getDiagramTitle`, `getSubsetData`, `getTextData`, `getStyleData`), with the data shapes of
//! `vennTypes.ts`.
//!
//! Upstream keeps the three lists in module-level arrays and pushes to them in statement
//! order, so insertion order is observable and the lists are `Vec`s here. `getCurrentSets` and
//! `getIndentMode` are parser state, not part of the parsed database, so they live in
//! [`super::db::Db`] instead of the [`Ast`]. `getConfig` is left out because this crate owns no
//! configuration.

/// One subset (`VennData` in `vennTypes.ts`): a `set` or a `union` and its area.
#[derive(Clone, Debug, PartialEq)]
pub struct Subset {
    /// `sets`, normalized and sorted the way `addSubsetData` stores them.
    pub sets: Vec<String>,
    /// `size`; a statement without a `:N` suffix gets `10 / sets.len()²`.
    pub size: f64,
    /// `label` from a `["..."]` suffix, normalized; `None` when absent or empty.
    pub label: Option<String>,
}

/// One text node (`VennTextData` in `vennTypes.ts`).
#[derive(Clone, Debug, PartialEq)]
pub struct TextNode {
    /// The sets the node belongs to, normalized and sorted.
    pub sets: Vec<String>,
    /// `id`, normalized.
    pub id: String,
    /// `label` from a `["..."]` suffix, normalized; `None` when absent or empty.
    pub label: Option<String>,
}

/// One `style` statement (`VennStyleData` in `vennTypes.ts`).
#[derive(Clone, Debug, PartialEq)]
pub struct StyleEntry {
    /// `targets`, normalized and sorted.
    pub targets: Vec<String>,
    /// `styles` as an object: insertion-ordered `(key, value)` pairs. A repeated key keeps its
    /// first position and takes the last value, like assigning to a JavaScript object.
    pub styles: Vec<(String, String)>,
}

impl StyleEntry {
    /// The value a property was assigned, `styles[key]`.
    pub fn style(&self, key: &str) -> Option<&str> {
        self.styles
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }
}

/// What upstream's venn database holds after parsing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ast {
    /// `getDiagramTitle()`; empty when the diagram has no title. A `title` statement sanitizes
    /// its text, and a later one replaces an earlier one.
    pub diagram_title: String,
    /// `getSubsetData()`, in statement order.
    pub subsets: Vec<Subset>,
    /// `getTextData()`, in statement order.
    pub texts: Vec<TextNode>,
    /// `getStyleData()`, in statement order.
    pub styles: Vec<StyleEntry>,
}
