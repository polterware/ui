//! The venn database's writing side, ported from `vennDB.ts`.
//!
//! The jison grammar's actions run while parsing, so this database is also the parser state
//! the lexer consults: `addSubsetData` records the sets an indented `text` line attaches to
//! (`currentSets`), and `knownSets` is what `union` validates its identifiers against. The
//! parser applies each statement here as it finishes it, in grammar order.

use super::ast::{Ast, StyleEntry, Subset, TextNode};
use super::sanitize;
use crate::error::{ParseError, Span};

/// The module state `vennDB.ts` keeps.
#[derive(Default)]
pub(crate) struct Db {
    diagram_title: String,
    subsets: Vec<Subset>,
    texts: Vec<TextNode>,
    styles: Vec<StyleEntry>,
    known_sets: Vec<String>,
    current_sets: Option<Vec<String>>,
}

impl Db {
    /// `setDiagramTitle`: `sanitizeText(txt)`.
    pub(crate) fn set_diagram_title(&mut self, title: String) {
        self.diagram_title = sanitize::sanitize_text(&title);
    }

    /// `addSubsetData`: the sets are normalized and sorted, the missing size falls back to
    /// `10 / identifiers.length²`, and a single-set statement joins the known sets.
    pub(crate) fn add_subset(
        &mut self,
        identifiers: &[String],
        label: Option<String>,
        size: Option<f64>,
    ) {
        let sets = sorted_normalized(identifiers);
        let resolved_size = size.unwrap_or_else(|| 10.0 / (identifiers.len() as f64).powi(2));
        self.current_sets = Some(sets.clone());
        if sets.len() == 1 && !self.known_sets.contains(&sets[0]) {
            self.known_sets.push(sets[0].clone());
        }
        self.subsets.push(Subset {
            sets,
            size: resolved_size,
            label: normalized_label(label),
        });
    }

    /// `addTextData`: the identifiers and the id are normalized, the label too when present.
    pub(crate) fn add_text(&mut self, identifiers: &[String], id: String, label: Option<String>) {
        self.texts.push(TextNode {
            sets: sorted_normalized(identifiers),
            id: normalize_text(&id),
            label: normalized_label(label),
        });
    }

    /// `addStyleData`: an object assignment per `(key, value)` pair, so a repeated key keeps its
    /// first position and takes the last value.
    pub(crate) fn add_style(&mut self, identifiers: &[String], data: Vec<(String, String)>) {
        let mut styles: Vec<(String, String)> = Vec::with_capacity(data.len());
        for (key, value) in data {
            let value = if value.is_empty() {
                value
            } else {
                normalize_text(&value)
            };
            match styles.iter_mut().find(|(name, _)| *name == key) {
                Some((_, existing)) => *existing = value,
                None => styles.push((key, value)),
            }
        }
        self.styles.push(StyleEntry {
            targets: sorted_normalized(identifiers),
            styles,
        });
    }

    /// `validateUnionIdentifiers`: every identifier must be a set declared by `set` before it.
    pub(crate) fn validate_union_identifiers(
        &self,
        identifiers: &[String],
        span: Span,
    ) -> Result<(), ParseError> {
        let unknown: Vec<String> = identifiers
            .iter()
            .map(|identifier| normalize_text(identifier))
            .filter(|identifier| !self.known_sets.contains(identifier))
            .collect();
        if unknown.is_empty() {
            Ok(())
        } else {
            Err(ParseError::at(
                format!("unknown set identifier: {}", unknown.join(", ")),
                span,
            ))
        }
    }

    /// `getCurrentSets`, the sets an indented `text` line belongs to.
    pub(crate) fn current_sets(&self) -> Option<Vec<String>> {
        self.current_sets.clone()
    }

    pub(crate) fn into_ast(self) -> Ast {
        Ast {
            diagram_title: self.diagram_title,
            subsets: self.subsets,
            texts: self.texts,
            styles: self.styles,
        }
    }
}

/// `label ? normalizeText(label) : undefined`: an empty label is falsy upstream.
fn normalized_label(label: Option<String>) -> Option<String> {
    label
        .filter(|value| !value.is_empty())
        .map(|value| normalize_text(&value))
}

/// `normalizeIdentifierList(...).sort()`: normalize every name, then sort it the way
/// JavaScript's default `sort` compares strings, by UTF-16 code units.
fn sorted_normalized(identifiers: &[String]) -> Vec<String> {
    let mut sets: Vec<String> = identifiers
        .iter()
        .map(|identifier| normalize_text(identifier))
        .collect();
    sets.sort_by(|left, right| left.encode_utf16().cmp(right.encode_utf16()));
    sets
}

/// `normalizeText`: trim, then drop one pair of surrounding double quotes.
fn normalize_text(text: &str) -> String {
    let trimmed = text.trim_matches(crate::preprocess::is_js_whitespace);
    let chars: Vec<char> = trimmed.chars().collect();
    if chars.len() >= 2 && chars[0] == '"' && chars[chars.len() - 1] == '"' {
        chars[1..chars.len() - 1].iter().collect()
    } else {
        trimmed.to_string()
    }
}
