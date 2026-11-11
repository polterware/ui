//! The cynefin database after parsing: what upstream's `CynefinDB` getters return
//! (`cynefinDb.ts` `getDomains`, `getTransitions`, `getDiagramTitle`, `getAccTitle`,
//! `getAccDescription`).
//!
//! `getDomains()` is a JavaScript `Map` keyed by the domain name. `setDomains` calls `set` for
//! every block in grammar order, so a domain declared twice keeps the position of its first
//! block and takes the items of the last one. [`Ast::domains`] is that map in insertion order,
//! with [`Ast::domain`] as the `get` lookup.
//!
//! A transition's label is `label: t.label || undefined` in `setTransitions`: an absent or
//! empty label is `None`.
//!
//! `getConfig` is left out because this crate owns no configuration.

/// The five domain names `types.ts` declares as `DomainName`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DomainName {
    Complex,
    Complicated,
    Clear,
    Chaotic,
    Confusion,
}

impl DomainName {
    /// The keyword the grammar spells this domain with.
    pub fn as_str(self) -> &'static str {
        match self {
            DomainName::Complex => "complex",
            DomainName::Complicated => "complicated",
            DomainName::Clear => "clear",
            DomainName::Chaotic => "chaotic",
            DomainName::Confusion => "confusion",
        }
    }

    /// The domain a `DOMAIN_NAME` token spells, if it is one of the five.
    pub(crate) fn from_keyword(keyword: &str) -> Option<Self> {
        match keyword {
            "complex" => Some(DomainName::Complex),
            "complicated" => Some(DomainName::Complicated),
            "clear" => Some(DomainName::Clear),
            "chaotic" => Some(DomainName::Chaotic),
            "confusion" => Some(DomainName::Confusion),
            _ => None,
        }
    }
}

/// One bullet of a domain (`CynefinItem` in `types.ts`).
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// The `STRING` label with its quotes removed.
    pub label: String,
}

/// One domain block (`CynefinDomain` in `types.ts`).
#[derive(Clone, Debug, PartialEq)]
pub struct Domain {
    pub name: DomainName,
    /// The block's items in grammar order; empty when the block has none.
    pub items: Vec<Item>,
}

/// One parsed transition (`CynefinTransition` in `types.ts`).
#[derive(Clone, Debug, PartialEq)]
pub struct Transition {
    pub from: DomainName,
    pub to: DomainName,
    /// The `STRING` after the colon, quotes removed; `None` without a label.
    pub label: Option<String>,
}

/// What upstream's cynefin database holds after parsing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ast {
    /// `getDomains()`, one entry per domain name in first-declaration order.
    pub domains: Vec<Domain>,
    /// `getTransitions()`, self-loops dropped, in grammar order.
    pub transitions: Vec<Transition>,
    /// `getDiagramTitle()`; empty when the diagram has no title.
    pub diagram_title: String,
    /// `getAccTitle()`; empty when the diagram has no accessible title.
    pub acc_title: String,
    /// `getAccDescription()`; empty when the diagram has no accessible description.
    pub acc_description: String,
}

impl Ast {
    /// The value `getDomains().get(name)` returns.
    pub fn domain(&self, name: DomainName) -> Option<&Domain> {
        self.domains.iter().find(|domain| domain.name == name)
    }
}
