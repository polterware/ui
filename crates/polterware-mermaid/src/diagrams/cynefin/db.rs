//! The cynefin database's writing side, ported from `cynefinDb.ts` and
//! `diagrams/common/populateCommonDb.ts`.
//!
//! `cynefinParser.ts` parses the whole document first and then populates the database. The
//! order of the writes is the one `populate` uses: the accessibility fields, then `setDomains`,
//! then `setTransitions`.

use super::ast::{Ast, Domain, Item, Transition};
use super::parser::{ParsedCynefin, ParsedDomain, ParsedTransition};
use super::sanitize;

/// `populate`: applies the parsed document to a fresh database.
pub(crate) fn build(parsed: ParsedCynefin) -> Ast {
    let mut db = Db::new();
    if let Some(description) = parsed.acc_description.filter(|value| !value.is_empty()) {
        db.set_acc_description(description);
    }
    if let Some(title) = parsed.acc_title.filter(|value| !value.is_empty()) {
        db.set_acc_title(title);
    }
    if let Some(title) = parsed.title.filter(|value| !value.is_empty()) {
        db.set_diagram_title(title);
    }
    db.set_domains(parsed.domains);
    db.set_transitions(parsed.transitions);
    db.ast
}

/// The module state `cynefinDb.ts` keeps.
struct Db {
    ast: Ast,
}

impl Db {
    fn new() -> Self {
        Self {
            ast: Ast::default(),
        }
    }

    /// `setDiagramTitle`: `sanitizeText(txt)`.
    fn set_diagram_title(&mut self, title: String) {
        self.ast.diagram_title = sanitize::sanitize_text(&title);
    }

    /// `setAccTitle`: `sanitizeText(txt).replace(/^\s+/g, '')`.
    fn set_acc_title(&mut self, title: String) {
        self.ast.acc_title = sanitize::set_acc_title(&title);
    }

    /// `setAccDescription`: `sanitizeText(txt).replace(/\n\s+/g, '\n')`.
    fn set_acc_description(&mut self, description: String) {
        self.ast.acc_description = sanitize::set_acc_description(&description);
    }

    /// `setDomains`: `Map.set` per block, so a repeated domain keeps its first position and
    /// takes the last block's items.
    fn set_domains(&mut self, blocks: Vec<ParsedDomain>) {
        for block in blocks {
            let items = block
                .items
                .into_iter()
                .map(|label| Item { label })
                .collect();
            match self
                .ast
                .domains
                .iter_mut()
                .find(|domain| domain.name == block.name)
            {
                Some(domain) => domain.items = items,
                None => self.ast.domains.push(Domain {
                    name: block.name,
                    items,
                }),
            }
        }
    }

    /// `setTransitions`: a self-loop is skipped and an empty label is `undefined`.
    fn set_transitions(&mut self, transitions: Vec<ParsedTransition>) {
        self.ast.transitions = transitions
            .into_iter()
            .filter(|transition| transition.from != transition.to)
            .map(|transition| Transition {
                from: transition.from,
                to: transition.to,
                label: transition.label.filter(|label| !label.is_empty()),
            })
            .collect();
    }
}
