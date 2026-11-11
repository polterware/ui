//! The pie database's writing side, ported from `pieDb.ts` and
//! `diagrams/common/populateCommonDb.ts`.
//!
//! `pieParser.ts` parses the whole document first and then populates the database, so a
//! negative slice value is reported only after the grammar accepted every line. The order of
//! the writes is the one `populateDb` uses: the accessibility fields, then `showData`, then
//! each section in grammar order.

use super::ast::{Ast, Section};
use super::parser::ParsedPie;
use super::sanitize;
use crate::error::{ParseError, Span};

/// `populateDb`: applies the parsed document to a fresh database.
pub(crate) fn build(parsed: ParsedPie) -> Result<Ast, ParseError> {
    let mut db = Db::new();
    if let Some(value) = parsed.acc_description.filter(|value| !value.is_empty()) {
        db.set_acc_description(value);
    }
    if let Some(value) = parsed.acc_title.filter(|value| !value.is_empty()) {
        db.set_acc_title(value);
    }
    if let Some(value) = parsed.title.filter(|value| !value.is_empty()) {
        db.set_diagram_title(value);
    }
    db.set_show_data(parsed.show_data);
    for section in parsed.sections {
        db.add_section(section.label, section.value, section.span)?;
    }
    Ok(db.into_ast())
}

/// The module state `pieDb.ts` keeps.
struct Db {
    sections: Vec<Section>,
    show_data: bool,
    diagram_title: String,
    acc_title: String,
    acc_description: String,
}

impl Db {
    fn new() -> Self {
        Self {
            sections: Vec::new(),
            show_data: false,
            diagram_title: String::new(),
            acc_title: String::new(),
            acc_description: String::new(),
        }
    }

    /// `setDiagramTitle`: `sanitizeText(txt)`.
    fn set_diagram_title(&mut self, title: String) {
        self.diagram_title = sanitize::sanitize_text(&title);
    }

    /// `setAccTitle`: `sanitizeText(txt).replace(/^\s+/g, '')`.
    fn set_acc_title(&mut self, title: String) {
        self.acc_title = sanitize::set_acc_title(&title);
    }

    /// `setAccDescription`: `sanitizeText(txt).replace(/\n\s+/g, '\n')`.
    fn set_acc_description(&mut self, description: String) {
        self.acc_description = sanitize::set_acc_description(&description);
    }

    /// `setShowData`.
    fn set_show_data(&mut self, toggle: bool) {
        self.show_data = toggle;
    }

    /// `addSection`: a negative value is an error, and an existing label keeps its value.
    fn add_section(&mut self, label: String, value: f64, span: Span) -> Result<(), ParseError> {
        if value < 0.0 {
            return Err(ParseError::at(
                format!(
                    "\"{label}\" has invalid value: {}. Negative values are not allowed in pie charts. All slice values must be >= 0.",
                    format_number(value)
                ),
                span,
            ));
        }
        if !self.sections.iter().any(|section| section.label == label) {
            self.sections.push(Section { label, value });
        }
        Ok(())
    }

    fn into_ast(self) -> Ast {
        Ast {
            sections: self.sections,
            show_data: self.show_data,
            diagram_title: self.diagram_title,
            acc_title: self.acc_title,
            acc_description: self.acc_description,
        }
    }
}

/// `${value}` in the error message: JavaScript's `Number::toString`, so that the message is
/// the one upstream throws. The ordinary decimals a slice can hold print the same in Rust;
/// the scientific form only differs in the exponent's sign and padding, which this adds.
fn format_number(value: f64) -> String {
    let magnitude = value.abs();
    if magnitude != 0.0 && !(1e-6..1e21).contains(&magnitude) {
        let scientific = format!("{value:e}");
        if let Some((mantissa, exponent)) = scientific.split_once('e')
            && !exponent.starts_with('-')
        {
            return format!("{mantissa}e+{exponent}");
        }
        scientific
    } else {
        format!("{value}")
    }
}
