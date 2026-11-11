//! The sankey grammar: a hand-written recursive-descent port of upstream `parser/sankey.jison`.
//!
//! ```text
//! start:       SANKEY NEWLINE csv opt_eof
//! csv:         record csv_tail
//! csv_tail:    NEWLINE csv | ;
//! opt_eof:     EOF | ;
//! record:      field COMMA field COMMA field
//! field:       escaped | non_escaped
//! escaped:     DQUOTE ESCAPED_TEXT DQUOTE
//! non_escaped: NON_ESCAPED_TEXT
//! ```
//!
//! Three fields, in `source,target,value` order; a fourth field, a missing field, or an empty
//! field is a parse error, exactly as the grammar has it. The `record` action is ported in
//! [`Parser::parse_record`]: the source and target are trimmed and, when quoted, have their
//! doubled quotes collapsed; the value is trimmed and read as JavaScript's `parseFloat`, so a
//! non-number (`abc`, a comma) is `NaN`, not an error.
//!
//! `sankeyUtils.ts` `prepareTextForParsing` runs before the grammar upstream: it collapses
//! every newline run to one newline and trims the string's ends. The lexer skips leading
//! whitespace and collapses newline runs inside quoted fields; here the body's end is trimmed,
//! and a run of `NEWLINE` tokens stands for the single `NEWLINE` the grammar expects, which is
//! what makes the empty lines the documentation allows work at all.

use super::ast::Ast;
use super::db;
use super::lexer::{self, Lexer, Token, TokenKind};
use crate::error::ParseError;

/// Parses the diagram body and populates the database.
pub(crate) fn parse(body: &str) -> Result<Ast, ParseError> {
    let comments = lexer::comment_ranges(body);
    let source = body.trim_end_matches(crate::preprocess::is_js_whitespace);
    let records = Parser::new(source, &comments).parse_start()?;
    Ok(db::build(records))
}

/// One `record` after its action ran: the two ids and the flow value.
pub(crate) struct ParsedRecord {
    pub(crate) source: String,
    pub(crate) target: String,
    pub(crate) value: f64,
}

/// One `field`: its token text and whether it was quoted.
struct Field {
    text: String,
    escaped: bool,
}

impl Field {
    /// `$field.trim()`, with `replaceAll('""', '"')` for a quoted field.
    fn value(&self) -> String {
        let trimmed = self.text.trim_matches(crate::preprocess::is_js_whitespace);
        if self.escaped {
            replace_doubled_quotes(trimmed)
        } else {
            trimmed.to_string()
        }
    }
}

struct Parser<'a> {
    lexer: Lexer<'a>,
    lookahead: Option<Token>,
}

impl<'a> Parser<'a> {
    fn new(body: &'a str, comments: &'a [std::ops::Range<usize>]) -> Self {
        Self {
            lexer: Lexer::new(body, comments),
            lookahead: None,
        }
    }

    fn fill(&mut self) -> Result<&Token, ParseError> {
        if self.lookahead.is_none() {
            self.lookahead = Some(self.lexer.next_token()?);
        }
        Ok(self.lookahead.as_ref().expect("just filled"))
    }

    fn peek(&mut self) -> Result<TokenKind, ParseError> {
        Ok(self.fill()?.kind)
    }

    fn bump(&mut self) -> Result<Token, ParseError> {
        self.fill()?;
        Ok(self.lookahead.take().expect("filled above"))
    }

    fn expect(&mut self, expected: TokenKind) -> Result<Token, ParseError> {
        if self.peek()? != expected {
            return Err(self.unexpected(expected.name()));
        }
        self.bump()
    }

    fn unexpected(&mut self, expected: &str) -> ParseError {
        match self.fill() {
            Ok(token) => ParseError::at(
                format!("expected {expected}, got '{}'", token.kind.name()),
                token.span,
            ),
            Err(error) => error,
        }
    }

    fn skip_newlines(&mut self) -> Result<(), ParseError> {
        while self.peek()? == TokenKind::Newline {
            self.bump()?;
        }
        Ok(())
    }

    fn parse_start(mut self) -> Result<Vec<ParsedRecord>, ParseError> {
        self.expect(TokenKind::Sankey)?;
        self.expect(TokenKind::Newline)?;
        self.skip_newlines()?;
        let mut records = vec![self.parse_record()?];
        loop {
            match self.peek()? {
                TokenKind::Eof => return Ok(records),
                TokenKind::Newline => {
                    self.skip_newlines()?;
                    if self.peek()? == TokenKind::Eof {
                        return Ok(records);
                    }
                    records.push(self.parse_record()?);
                }
                _ => return Err(self.unexpected("a newline or end of input")),
            }
        }
    }

    fn parse_record(&mut self) -> Result<ParsedRecord, ParseError> {
        let source = self.parse_field()?.value();
        self.expect(TokenKind::Comma)?;
        let target = self.parse_field()?.value();
        self.expect(TokenKind::Comma)?;
        let value = self.parse_field()?.value();
        Ok(ParsedRecord {
            source,
            target,
            value: parse_float(&value),
        })
    }

    fn parse_field(&mut self) -> Result<Field, ParseError> {
        match self.peek()? {
            TokenKind::DQuote => {
                self.bump()?;
                let text = self.expect(TokenKind::EscapedText)?.text;
                self.expect(TokenKind::DQuote)?;
                Ok(Field {
                    text,
                    escaped: true,
                })
            }
            TokenKind::NonEscapedText => Ok(Field {
                text: self.bump()?.text,
                escaped: false,
            }),
            _ => Err(self.unexpected("a field")),
        }
    }
}

/// `String.prototype.replaceAll('""', '"')`, left to right without overlap.
fn replace_doubled_quotes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '"' && chars.peek() == Some(&'"') {
            chars.next();
            out.push('"');
        } else {
            out.push(ch);
        }
    }
    out
}

/// JavaScript's `parseFloat`: read the longest numeric prefix (a decimal, or `Infinity` with
/// an optional sign) and answer `NaN` when there is none. Hex, underscores, and lowercase
/// `infinity` are not numbers, and an exponent without digits is not consumed.
fn parse_float(text: &str) -> f64 {
    let bytes = text.as_bytes();
    let mut index = 0;
    let negative = match bytes.first() {
        Some(b'+') => {
            index = 1;
            false
        }
        Some(b'-') => {
            index = 1;
            true
        }
        _ => false,
    };
    if text[index..].starts_with("Infinity") {
        return if negative {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }

    let mut number = String::with_capacity(text.len() + 1);
    if negative {
        number.push('-');
    }
    let integer_start = index;
    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    let integer_digits = index - integer_start;
    if integer_digits == 0 {
        number.push('0');
    } else {
        number.push_str(&text[integer_start..index]);
    }
    let mut fraction_digits = 0;
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        let fraction_start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        fraction_digits = index - fraction_start;
        if fraction_digits > 0 {
            number.push('.');
            number.push_str(&text[fraction_start..index]);
        }
    }
    if integer_digits == 0 && fraction_digits == 0 {
        return f64::NAN;
    }
    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        let mut end = index + 1;
        if matches!(bytes.get(end), Some(b'+' | b'-')) {
            end += 1;
        }
        let exponent_start = end;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        if end > exponent_start {
            number.push_str(&text[index..end]);
        }
    }
    number.parse::<f64>().unwrap_or(f64::NAN)
}
