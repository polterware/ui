//! The kanban database's writing side, ported from `kanbanDb.ts`.
//!
//! `addNode` receives the indentation as the level, sanitizes the id and the description,
//! applies the `@{ ... }` metadata, and then resolves the section from the levels already
//! stored (`getSection`): the first node's level is the section level, a node at that level is
//! a section itself, and any other node belongs to the most recent section-level node. A node
//! indented less than the section level means the document has an item without a section, and
//! the database throws with the section it found. `decorateNode` writes `icon` and `class`
//! onto the most recently added node.
//!
//! The inline attributes upstream reads from the configuration (`mindmap.padding`,
//! `mindmap.maxNodeWidth`) are not set here because this crate owns no configuration.

use super::ast::{Ast, Node};
use super::metadata;
use super::parser::{ParsedKanban, ParsedNode, ParsedShapeData, ParsedStatement};
use super::sanitize;
use crate::error::{ParseError, Span};

/// Applies the parsed document to a fresh database.
pub(crate) fn build(parsed: ParsedKanban) -> Result<Ast, ParseError> {
    let mut db = Db::new();
    for statement in parsed.statements {
        match statement {
            ParsedStatement::Node {
                level,
                node,
                shape_data,
            } => db.add_node(level, node, shape_data)?,
            ParsedStatement::Icon { icon, span } => db.decorate_icon(&icon, span)?,
            ParsedStatement::Class { class, span } => db.decorate_class(&class, span)?,
            ParsedStatement::Blank => {}
        }
    }
    // Upstream pushes the same object into `sections` and `nodes`, so a decoration written
    // after `addNode` is visible through both. The indices keep that sharing in Rust.
    let sections = db
        .section_indices
        .iter()
        .map(|index| db.nodes[*index].clone())
        .collect();
    Ok(Ast {
        sections,
        nodes: db.nodes,
    })
}

/// The module state `kanbanDb.ts` keeps.
struct Db {
    nodes: Vec<Node>,
    /// The `sections` list as indices into `nodes`.
    section_indices: Vec<usize>,
    cnt: usize,
}

impl Db {
    fn new() -> Self {
        Self {
            nodes: Vec::new(),
            section_indices: Vec::new(),
            cnt: 0,
        }
    }

    /// `addNode(level, id, descr, type, shapeData)`.
    fn add_node(
        &mut self,
        level: usize,
        node: ParsedNode,
        shape_data: Option<ParsedShapeData>,
    ) -> Result<(), ParseError> {
        let id = sanitize::sanitize_text(&node.id);
        let id = if id.is_empty() {
            let generated = format!("kbn{}", self.cnt);
            self.cnt += 1;
            generated
        } else {
            id
        };
        let mut item = Node {
            id,
            label: sanitize::sanitize_text(&node.description),
            level,
            parent_id: None,
            node_type: node.node_type,
            shape: None,
            icon: None,
            css_classes: None,
            assigned: None,
            ticket: None,
            priority: None,
        };
        if let Some(shape_data) = shape_data {
            apply_metadata(&mut item, &shape_data)?;
        }
        match self.section(level, node.span)? {
            Some(parent_id) => item.parent_id = Some(parent_id),
            None => self.section_indices.push(self.nodes.len()),
        }
        self.nodes.push(item);
        Ok(())
    }

    /// `getSection(level)`: the section a node at `level` belongs to, `None` when the node is
    /// itself a section, or an error when the document has an item above the section level.
    fn section(&self, level: usize, span: Span) -> Result<Option<String>, ParseError> {
        if self.nodes.is_empty() {
            return Ok(None);
        }
        let section_level = self.nodes[0].level;
        let mut last_section = None;
        for node in self.nodes.iter().rev() {
            if node.level == section_level && last_section.is_none() {
                last_section = Some(node);
            }
            if node.level < section_level {
                return Err(ParseError::at(
                    format!(
                        "Items without section detected, found section (\"{}\")",
                        node.label
                    ),
                    span,
                ));
            }
        }
        Ok(match last_section {
            Some(section) if section.level != level => Some(section.id.clone()),
            _ => None,
        })
    }

    /// `decorateNode({ icon })`.
    fn decorate_icon(&mut self, icon: &str, span: Span) -> Result<(), ParseError> {
        let icon = sanitize::sanitize_text(icon);
        let node = self.last_node(span)?;
        node.icon = Some(icon);
        Ok(())
    }

    /// `decorateNode({ class })`.
    fn decorate_class(&mut self, class: &str, span: Span) -> Result<(), ParseError> {
        let class = sanitize::sanitize_text(class);
        let node = self.last_node(span)?;
        node.css_classes = Some(class);
        Ok(())
    }

    /// `nodes[nodes.length - 1]`. Upstream dereferences it unconditionally, so a decoration
    /// before the first item is a `TypeError`; here it is a [`ParseError`] with a span.
    fn last_node(&mut self, span: Span) -> Result<&mut Node, ParseError> {
        self.nodes.last_mut().ok_or_else(|| {
            ParseError::at(
                "cannot decorate a kanban item before any item is defined",
                span,
            )
        })
    }
}

/// The `shapeData` branch of `addNode`: js-yaml with `JSON_SCHEMA`, then the fields it reads.
fn apply_metadata(item: &mut Node, shape_data: &ParsedShapeData) -> Result<(), ParseError> {
    let doc = metadata::parse(&shape_data.text)
        .map_err(|error| ParseError::at(error, shape_data.span))?;

    if let Some(shape) = doc.get("shape").filter(|value| metadata::truthy(value)) {
        let name = metadata::js_to_string(shape);
        if name != name.to_lowercase() || name.contains('_') {
            return Err(ParseError::at(
                format!("No such shape: {name}. Shape names should be lowercase."),
                shape_data.span,
            ));
        }
        // `doc.shape === 'kanbanItem'` never holds here: that spelling fails the lowercase
        // check just above, so upstream's assignment is unreachable for the same reason.
        if name == "kanbanItem" {
            item.shape = Some(name);
        }
    }
    if let Some(label) = doc.get("label").filter(|value| metadata::truthy(value)) {
        // Upstream assigns the YAML value as is and only stringifies it when the label is
        // rendered; a non-string label would make `sanitizeText` throw, so it is stringified
        // here instead.
        item.label = metadata::js_to_string(label);
    }
    if let Some(icon) = doc.get("icon").filter(|value| metadata::truthy(value)) {
        item.icon = Some(metadata::js_to_string(icon));
    }
    if let Some(assigned) = doc.get("assigned").filter(|value| metadata::truthy(value)) {
        item.assigned = Some(metadata::js_to_string(assigned));
    }
    if let Some(ticket) = doc.get("ticket").filter(|value| metadata::truthy(value)) {
        item.ticket = Some(metadata::js_to_string(ticket));
    }
    if let Some(priority) = doc.get("priority").filter(|value| metadata::truthy(value)) {
        item.priority = Some(priority.clone());
    }
    Ok(())
}
