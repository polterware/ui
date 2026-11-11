//! The railroad database's writing side, ported from `railroadDb.ts` and
//! `railroadParser.ts`'s `populateDb` (with `diagrams/common/populateCommonDb.ts`).
//!
//! `populateDb` parses the whole document first and then populates the database: the
//! accessibility description, then the accessibility title, then the diagram title, then
//! every rule in grammar order. `populateCommonDb` writes the title and `populateDb` writes it
//! again through `setTitle`; both write the same sanitized value into the same field, so one
//! write models the pair.
//!
//! Every write sanitizes its text (`sanitizeText`). A duplicate rule name keeps both rules in
//! the array while the lookup answers the last one, and upstream only logs a warning for it.

use super::ast::{Ast, Node, Rule};
use super::parser::ParsedRailroad;
use super::sanitize;

/// `populateDb`: applies the parsed document to a fresh database.
pub(crate) fn build(parsed: ParsedRailroad) -> Ast {
    let mut db = Db::new();
    if let Some(value) = parsed.acc_description.filter(|value| !value.is_empty()) {
        db.set_acc_description(&value);
    }
    if let Some(value) = parsed.acc_title.filter(|value| !value.is_empty()) {
        db.set_acc_title(&value);
    }
    if let Some(value) = parsed.title.filter(|value| !value.is_empty()) {
        db.set_title(&value);
    }
    for rule in parsed.rules {
        db.add_rule(rule);
    }
    db.into_ast()
}

/// The module state `railroadDb.ts` keeps.
pub(crate) struct Db {
    rules: Vec<Rule>,
    diagram_title: String,
    acc_title: String,
    acc_description: String,
}

impl Db {
    /// A cleared database, the state `clear()` leaves.
    pub(crate) fn new() -> Self {
        Self {
            rules: Vec::new(),
            diagram_title: String::new(),
            acc_title: String::new(),
            acc_description: String::new(),
        }
    }

    /// `clear`: forget every rule, title, and accessibility field.
    #[cfg(test)]
    pub(crate) fn clear(&mut self) {
        *self = Self::new();
    }

    /// `setTitle`: `sanitizeText(text)`.
    pub(crate) fn set_title(&mut self, text: &str) {
        self.diagram_title = sanitize::sanitize_text(text);
    }

    /// `getTitle`: the diagram title.
    #[allow(dead_code)] // used by the ported database specs
    pub(crate) fn title(&self) -> &str {
        &self.diagram_title
    }

    /// `setDiagramTitle`, an alias of `setTitle` upstream.
    #[allow(dead_code)] // used by the ported database specs
    pub(crate) fn set_diagram_title(&mut self, text: &str) {
        self.set_title(text);
    }

    /// `getDiagramTitle`, an alias of `getTitle` upstream.
    #[allow(dead_code)] // used by the ported database specs
    pub(crate) fn diagram_title(&self) -> &str {
        self.title()
    }

    /// `addRule`: the name and definition are sanitized, and the rule is appended even when
    /// its name repeats an earlier one. (`RailroadRule.comment` is sanitized upstream too, but
    /// this grammar never sets it.)
    pub(crate) fn add_rule(&mut self, rule: Rule) {
        self.rules.push(Rule {
            name: sanitize::sanitize_text(&rule.name),
            definition: sanitize_node(rule.definition),
        });
    }

    /// `getRules`, in insertion order.
    #[allow(dead_code)] // used by the ported database specs
    pub(crate) fn rules(&self) -> &[Rule] {
        &self.rules
    }

    /// `getRule`: the last rule added under `name`, like the map `addRule` overwrites.
    #[allow(dead_code)] // used by the ported database specs
    pub(crate) fn rule(&self, name: &str) -> Option<&Rule> {
        self.rules.iter().rev().find(|rule| rule.name == name)
    }

    /// `setAccTitle`: `sanitizeText(text).replace(/^\s+/g, '')`.
    pub(crate) fn set_acc_title(&mut self, text: &str) {
        self.acc_title = sanitize::set_acc_title(text);
    }

    /// `getAccTitle`.
    #[allow(dead_code)] // used by the ported database specs
    pub(crate) fn acc_title(&self) -> &str {
        &self.acc_title
    }

    /// `setAccDescription`: `sanitizeText(text).replace(/\n\s+/g, '\n')`.
    pub(crate) fn set_acc_description(&mut self, text: &str) {
        self.acc_description = sanitize::set_acc_description(text);
    }

    /// `getAccDescription`.
    #[allow(dead_code)] // used by the ported database specs
    pub(crate) fn acc_description(&self) -> &str {
        &self.acc_description
    }

    /// The getters' view of the database.
    pub(crate) fn into_ast(self) -> Ast {
        Ast {
            rules: self.rules,
            diagram_title: self.diagram_title,
            acc_title: self.acc_title,
            acc_description: self.acc_description,
        }
    }
}

/// `sanitizeAstNode`: the same tree with every text sanitized.
fn sanitize_node(node: Node) -> Node {
    match node {
        Node::Terminal { value } => Node::Terminal {
            value: sanitize::sanitize_text(&value),
        },
        Node::NonTerminal { name } => Node::NonTerminal {
            name: sanitize::sanitize_text(&name),
        },
        Node::Sequence { elements } => Node::Sequence {
            elements: elements.into_iter().map(sanitize_node).collect(),
        },
        Node::Choice { alternatives } => Node::Choice {
            alternatives: alternatives.into_iter().map(sanitize_node).collect(),
        },
        Node::Optional { element } => Node::Optional {
            element: Box::new(sanitize_node(*element)),
        },
        Node::Repetition {
            element,
            min,
            max,
            separator,
        } => Node::Repetition {
            element: Box::new(sanitize_node(*element)),
            min,
            max,
            separator: separator.map(|separator| Box::new(sanitize_node(*separator))),
        },
        Node::Special { text } => Node::Special {
            text: sanitize::sanitize_text(&text),
        },
    }
}
