//! The ishikawa database after parsing: what upstream's `IshikawaDB` getters return
//! (`ishikawaDb.ts` `getRoot` and `getDiagramTitle`).
//!
//! `getAccTitle` and `getAccDescription` are not here: `ishikawa.jison` never calls their
//! setters, so they stay empty after a parse. Configuration is not here either; this crate
//! owns none.

/// One cause of the fishbone: `IshikawaNode` in `ishikawaTypes.ts`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IshikawaNode {
    /// The line's text, trimmed and sanitized by `addNode`.
    pub text: String,
    /// The causes indented under this one, in the order they appear.
    pub children: Vec<IshikawaNode>,
}

/// What upstream's ishikawa database holds after parsing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ast {
    /// `getRoot()`: the effect, with every cause under it. `None` when the document had no
    /// statement that adds a node (`ishikawa` followed by blank or comment lines only).
    pub root: Option<IshikawaNode>,
    /// `getDiagramTitle()`: the root's text, which `addNode` also sets as the title; empty
    /// when there is no root.
    pub diagram_title: String,
}
