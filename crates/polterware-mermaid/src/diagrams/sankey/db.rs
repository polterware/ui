//! The sankey database's writing side, ported from `sankeyDB.ts`.
//!
//! `findOrCreateNode` sanitizes the id, then reuses an existing node or appends a new one;
//! `addLink` appends a link in record order, duplicates included. The grammar action calls the
//! two in that order for every record, so the node order is first-appearance order and the
//! link order is the CSV order.

use super::ast::{Ast, Link, Node};
use super::parser::ParsedRecord;
use super::sanitize;

/// Applies the parsed records to a fresh database, the way the grammar actions do.
pub(crate) fn build(records: Vec<ParsedRecord>) -> Ast {
    let mut db = Db::default();
    for record in records {
        let source = db.find_or_create_node(&record.source);
        let target = db.find_or_create_node(&record.target);
        db.links.push(Link {
            source,
            target,
            value: record.value,
        });
    }
    Ast {
        nodes: db.nodes,
        links: db.links,
    }
}

/// The module state `sankeyDB.ts` keeps.
#[derive(Default)]
struct Db {
    nodes: Vec<Node>,
    links: Vec<Link>,
}

impl Db {
    /// `findOrCreateNode`: the id is sanitized before the uniqueness check, and the node
    /// keeps the order it was first seen in.
    fn find_or_create_node(&mut self, id: &str) -> String {
        let id = sanitize::sanitize_text(id);
        if !self.nodes.iter().any(|node| node.id == id) {
            self.nodes.push(Node { id: id.clone() });
        }
        id
    }
}
