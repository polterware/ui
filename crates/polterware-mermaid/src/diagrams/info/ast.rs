//! The parsed `info` diagram: the `Info` AST node `info.langium` infers, which is everything
//! `infoParser.ts` produces.
//!
//! The generated upstream interface is
//!
//! ```text
//! interface Info extends AstNode {
//!   readonly $type: 'Info';
//!   accDescr?: string;
//!   accTitle?: string;
//!   title?: string;
//! }
//! ```
//!
//! so every field is optional and a later assignment overwrites an earlier one, as Langium's
//! AST builder does. `showInfo` is an unassigned keyword, so the node does not hold it.
//!
//! Upstream's database (`infoDb.ts`) has no parsed state to mirror: its only getter,
//! `getVersion`, returns `injected.version` plus `-tiny` unless the bundle includes large
//! features, and `infoParser.ts` never calls into the database. The fields below are the
//! node's assignments. They are not sanitized upstream and are not sanitized here.

/// What upstream's parser returns for an `info` diagram.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ast {
    /// `title=TITLE`, from a `title` line.
    pub title: Option<String>,
    /// `accTitle=ACC_TITLE`.
    pub acc_title: Option<String>,
    /// `accDescr=ACC_DESCR`.
    pub acc_description: Option<String>,
}
