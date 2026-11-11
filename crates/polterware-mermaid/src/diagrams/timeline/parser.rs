//! The timeline grammar: a hand-written recursive-descent port of
//! `parser/timeline.jison`, applying the database actions as it goes.
//!
//! ```text
//! start:
//!   timeline_header document EOF
//! timeline_header:
//!   timeline
//!   | timeline_lr
//!   | timeline_td
//! document:
//!   /* empty */
//!   | document line
//! line:
//!   statement
//!   | NEWLINE
//!   | EOF
//! statement:
//!   title
//!   | acc_title acc_title_value
//!   | acc_descr acc_descr_value
//!   | acc_descr_multiline_value
//!   | section
//!   | period
//!   | event
//! ```
//!
//! The `line: SPACE statement` alternative has no lexer rule behind it (`SPACE` is never
//! returned), so it cannot match and is not ported. Leading newlines are skipped before the
//! header because this crate's preprocessing blanks removed frontmatter and directive lines
//! instead of deleting them; upstream's parser never sees those blank lines. `title`,
//! `section`, `period`, and `event` need no lookahead, and the two accessibility values are
//! guaranteed by the lexer's start conditions, so the document is a single loop over tokens.
//!
//! The grammar actions cut the token text by the keyword length: `title.substr(6)` (five
//! letters and one whitespace character), `section.substr(8)` (seven and one), and
//! `event.substr(2)` (the colon and one whitespace character). The cuts count characters,
//! as JavaScript's `substr` counts code units, and keep trailing spaces: only the two
//! accessibility values are trimmed before the database sees them.

use super::ast::{Ast, Direction};
use super::db::Db;
use super::lexer::{Kind, Lexer, Token};
use crate::error::ParseError;
use crate::preprocess::is_js_whitespace;

/// Parses the diagram body and returns the database's answers.
pub(super) fn parse(body: &str) -> Result<Ast, ParseError> {
    let mut lexer = Lexer::new(body);
    let mut db = Db::new();

    // Upstream's grammar wants the header first; this crate's preprocess keeps the lines it
    // removes (frontmatter, directives) as blank lines so spans keep their line numbers, so
    // leading newlines are skipped here, as the gantt, sequence, and state parsers do.
    let mut header = lexer.next_token();
    while header.kind == Kind::Newline {
        header = lexer.next_token();
    }
    match header.kind {
        Kind::Timeline => {}
        Kind::TimelineLr => db.set_direction(Direction::Lr),
        Kind::TimelineTd => db.set_direction(Direction::Td),
        _ => {
            return Err(ParseError::at(
                format!("expected the timeline header, got '{}'", header.kind.name()),
                header.span,
            ));
        }
    }

    loop {
        let token = lexer.next_token();
        match token.kind {
            Kind::Newline => {}
            Kind::Eof => break,
            // The parser's `$end` where the grammar needs its own `EOF` token, which the
            // lexer stops delivering after a zero-width token at the end of the input.
            Kind::EndOfInput => {
                return Err(ParseError::at("unexpected end of input", token.span));
            }
            Kind::Title => db.set_diagram_title(after_chars(&token.text, 6)),
            Kind::AccTitle => {
                db.set_acc_title(js_trim(&accessibility_value(
                    &mut lexer,
                    Kind::AccTitleValue,
                )?));
            }
            Kind::AccDescr => {
                db.set_acc_description(js_trim(&accessibility_value(
                    &mut lexer,
                    Kind::AccDescrValue,
                )?));
            }
            Kind::AccDescrMultilineValue => {
                db.set_acc_description(js_trim(&token.text));
            }
            Kind::Section => db.add_section(after_chars(&token.text, 8)),
            Kind::Period => db.add_task(&token.text),
            Kind::Event => {
                if !db.add_event(after_chars(&token.text, 2)) {
                    // Upstream throws a `TypeError` reading `events` of no task here.
                    return Err(ParseError::at("an event must follow a task", token.span));
                }
            }
            Kind::Invalid => {
                return Err(ParseError::at(
                    format!("unexpected '{}'", token.text),
                    token.span,
                ));
            }
            _ => return Err(ParseError::at(unexpected_statement(&token), token.span)),
        }
    }

    Ok(db.into_ast())
}

/// The value token an accessibility keyword's start condition makes the lexer return.
fn accessibility_value(lexer: &mut Lexer<'_>, expected: Kind) -> Result<String, ParseError> {
    let value = lexer.next_token();
    if value.kind != expected {
        return Err(ParseError::at(
            format!(
                "expected '{}', got '{}'",
                expected.name(),
                value.kind.name()
            ),
            value.span,
        ));
    }
    Ok(value.text)
}

fn unexpected_statement(token: &Token) -> String {
    format!(
        "expected a title, an accessibility line, a section, or a task, got '{}'",
        token.kind.name()
    )
}

/// JavaScript's `substr(count)` for the actions that cut a keyword off a token.
fn after_chars(text: &str, count: usize) -> String {
    text.chars().skip(count).collect()
}

/// `String.prototype.trim`.
fn js_trim(text: &str) -> String {
    text.trim_matches(is_js_whitespace).to_string()
}
