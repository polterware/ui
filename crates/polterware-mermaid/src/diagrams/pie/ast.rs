//! The pie database after parsing: what upstream's `PieDB` getters return (`pieDb.ts`
//! `getSections`, `getShowData`, `getDiagramTitle`, `getAccTitle`, `getAccDescription`).
//!
//! Upstream stores sections in a `Map` whose insertion order is observable and whose keys
//! are unique: `addSection` ignores a label it already has. Here that is a `Vec` in the same
//! order, with [`Ast::section`] as the `get` lookup. `getConfig` is left out because this crate
//! owns no configuration.

/// One slice, as `addSection` receives it (`D3Section` in `pieTypes.ts`).
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub label: String,
    pub value: f64,
}

/// What upstream's pie database holds after parsing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ast {
    /// `getSections()`, in insertion order.
    pub sections: Vec<Section>,
    /// `getShowData()`.
    pub show_data: bool,
    /// `getDiagramTitle()`; empty when the diagram has no title.
    pub diagram_title: String,
    /// `getAccTitle()`; empty when the diagram has no accessible title.
    pub acc_title: String,
    /// `getAccDescription()`; empty when the diagram has no accessible description.
    pub acc_description: String,
}

impl Ast {
    /// The value `getSections().get(label)` returns.
    pub fn section(&self, label: &str) -> Option<f64> {
        self.sections
            .iter()
            .find(|section| section.label == label)
            .map(|section| section.value)
    }
}
