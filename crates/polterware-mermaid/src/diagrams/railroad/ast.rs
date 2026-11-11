//! The railroad database after parsing: what upstream's `RailroadDB` getters return
//! (`getRules`, `getRule`, `getTitle`/`getDiagramTitle`, `getAccTitle`, and
//! `getAccDescription`).
//!
//! Upstream keeps rules twice: in a `rules` array whose insertion order is observable, and
//! in a `ruleMap` keyed by name. `addRule` pushes to the array and overwrites the map entry,
//! so a duplicate name keeps both rules in order while `getRule` answers the last one. Here
//! [`Ast::rules`] is that array and [`Ast::rule`] is the lookup.
//!
//! `RailroadRule.comment` is part of `railroadTypes.ts`, but neither `railroad.langium` nor
//! `transformRule` ever assigns it, so it is always `undefined` upstream and is omitted here.

/// One rule (`RailroadRule`), as `getRules` returns it.
#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    pub name: String,
    pub definition: Node,
}

/// An expression after `railroadParser.ts`'s `transformExpression` (`ASTNode` in
/// `railroadTypes.ts`).
///
/// A `sequence` or `choice` with a single child is collapsed into that child by
/// `transformExpression`, so both variants always hold at least two children here.
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    /// `RailroadTerminalExpr`: a literal string.
    Terminal { value: String },
    /// `RailroadNonTerminalExpr`: a reference to another rule.
    NonTerminal { name: String },
    /// `RailroadSequenceExpr`: its elements in order.
    Sequence { elements: Vec<Node> },
    /// `RailroadChoiceExpr`: its alternatives in order.
    Choice { alternatives: Vec<Node> },
    /// `RailroadOptionalExpr`.
    Optional { element: Box<Node> },
    /// `RailroadOneOrMoreExpr` and `RailroadZeroOrMoreExpr`.
    Repetition {
        element: Box<Node>,
        /// 0 for `zeroOrMore`, 1 for `oneOrMore`.
        min: usize,
        /// Upstream stores `Infinity`; `None` is that bound.
        max: Option<usize>,
        /// `RailroadRepetitionNode.separator`: this grammar has no form that sets it, so it
        /// is always `None`, exactly as `transformExpression` leaves it.
        separator: Option<Box<Node>>,
    },
    /// `RailroadSpecialExpr`: text that stands for a class of characters.
    Special { text: String },
}

/// What upstream's railroad database holds after parsing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ast {
    /// `getRules()`, in insertion order.
    pub rules: Vec<Rule>,
    /// `getDiagramTitle()`, also answered by `getTitle()`; empty without a title.
    pub diagram_title: String,
    /// `getAccTitle()`; empty when the diagram has no accessible title.
    pub acc_title: String,
    /// `getAccDescription()`; empty when the diagram has no accessible description.
    pub acc_description: String,
}

impl Ast {
    /// The rule `getRule(name)` returns: the last rule added under that name.
    pub fn rule(&self, name: &str) -> Option<&Rule> {
        self.rules.iter().rev().find(|rule| rule.name == name)
    }
}
