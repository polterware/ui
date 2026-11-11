//! The database's writing side, ported from `db.ts` (`setAst`) and
//! `diagrams/common/populateCommonDb.ts`.
//!
//! `eventmodeling/parser.ts` parses the whole document, stores it with `setAst`, and then
//! copies the common accessibility fields, in that order. `setAst` clears the previous
//! document and rejects a frame id used twice; the error is the one upstream throws, including
//! the duplicate's 1-based source line. The title and accessibility values are the raw
//! terminal text (see [`super::parser`]), and the common database sanitizes them before a
//! getter returns them.

use std::collections::HashSet;

use super::ast::Ast;
use super::parser::ParsedEventModel;
use crate::common::sanitize;
use crate::error::ParseError;

/// `setAst` followed by `populateCommonDb`: applies a parsed document to a fresh database.
pub(crate) fn build(parsed: ParsedEventModel) -> Result<Ast, ParseError> {
    let mut frame_ids = HashSet::new();
    for parsed_frame in &parsed.frames {
        if !frame_ids.insert(parsed_frame.frame.name.as_str()) {
            let message = format!(
                "Duplicate event modeling frame ID \"{}\" on line {}",
                parsed_frame.frame.name, parsed_frame.span.line
            );
            return Err(ParseError::at(message, parsed_frame.span));
        }
    }

    Ok(Ast {
        frames: parsed
            .frames
            .into_iter()
            .map(|parsed_frame| parsed_frame.frame)
            .collect(),
        data_entities: parsed.data_entities,
        model_entities: parsed.model_entities,
        note_entities: parsed.note_entities,
        gwt_entities: parsed.gwt_entities,
        diagram_title: parsed
            .title
            .filter(|title| !title.is_empty())
            .map(|title| sanitize::sanitize_text(&title))
            .unwrap_or_default(),
        acc_title: parsed
            .acc_title
            .filter(|title| !title.is_empty())
            .map(|title| sanitize::set_acc_title(&title))
            .unwrap_or_default(),
        acc_description: parsed
            .acc_description
            .filter(|description| !description.is_empty())
            .map(|description| sanitize::set_acc_description(&description))
            .unwrap_or_default(),
    })
}
