//! The treemap grammar: a hand-written recursive-descent port of `treemap.langium`.
//!
//! ```text
//! Treemap:
//!   TREEMAP_KEYWORD ( TitleAndAccessibilities | TreemapRows+=TreemapRow )*
//! TitleAndAccessibilities:
//!   ((accDescr=ACC_DESCR | accTitle=ACC_TITLE | title=TITLE))+
//! TreemapRow:
//!   indent=INDENTATION? (item=Item | ClassDef)
//! Item:
//!   Leaf | Section
//! Section:
//!   name=STRING2 (STYLE_SEPARATOR classSelector=ID2)?
//! Leaf:
//!   name=STRING2 INDENTATION? (":" | ",") INDENTATION? value=MyNumber
//!   (STYLE_SEPARATOR classSelector=ID2)?
//! ClassDef returns string:
//!   CLASS_DEF
//! ```
//!
//! Newlines are hidden terminals, so the grammar has no line structure at all: a row's
//! indentation is whatever visible `INDENTATION` token sits between two names, and that token
//! also decides whether a name starts a `Leaf` or a `Section`. A name followed by an
//! `INDENTATION` is a leaf only when a `:` or `,` follows the indentation; otherwise the
//! indentation belongs to the next row, so the name is a section. That is the two-token
//! lookahead Langium computes for the `Leaf | Section` alternative.
//!
//! The root rule has no action, so every assignment lands on one `Treemap` node. Title,
//! accessible title, and accessible description are single-valued, so a later line overwrites
//! an earlier one. The terminal values go through `common/valueConverter.ts` and
//! `treemap/valueConverter.ts`: `STRING2` drops its quotes without interpreting escapes,
//! `NUMBER2` is `parseFloat(input.replace(/,/g, ''))` inside the `MyNumber` data type rule,
//! `INDENTATION` converts to its length, the accessibility terminals use the regex-driven
//! trimming of the common converter, and `ClassDef` is the converter's `classDefRegex` object.
//!
//! A `ClassDef` row is a `TreemapRow` whose unassigned `ClassDef` call returns that object;
//! Langium replaces the row node with it (`$type: 'ClassDefStatement'`), and when the regex
//! does not match, the rule value falls back to the raw string and the row keeps no item.
//! Here that is [`ParsedRow::ClassDef`] and [`ParsedRow::Skipped`].
//!
//! The whitespace before the header is skipped: mermaid's `cleanupComments` ends with
//! `.trimStart()`, so the text Langium actually sees starts at the header. A header line with
//! trailing whitespace still leaves an `INDENTATION` token and fails, exactly as it does
//! upstream, because `INDENTATION` matches before the dead `WS` terminal.

use std::collections::VecDeque;

use super::ast::Ast;
use super::db;
use super::lexer::{Lexer, Token, TokenKind};
use crate::error::ParseError;
use crate::preprocess::is_js_whitespace;

/// Parses the diagram body and populates the database.
pub(crate) fn parse(body: &str) -> Result<Ast, ParseError> {
    Ok(db::build(Parser::new(body).parse_treemap()?))
}

/// The `Treemap` AST node after parsing, before the database is populated.
#[derive(Default)]
pub(crate) struct ParsedTreemap {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_description: Option<String>,
    pub(crate) rows: Vec<ParsedRow>,
}

/// One `TreemapRow`.
pub(crate) enum ParsedRow {
    Item {
        /// `row.indent`, the `INDENTATION` token's length.
        indent: Option<usize>,
        item: Item,
    },
    /// A `ClassDefStatement`, className and styleText as its value converter extracts them.
    ClassDef {
        class_name: String,
        style_text: String,
    },
    /// A `ClassDef` row the converter's regex rejected: the row keeps no item and
    /// `populate` skips it.
    Skipped,
}

/// `Section` or `Leaf`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ItemKind {
    Section,
    Leaf,
}

/// One `Item` (`Section` or `Leaf`).
pub(crate) struct Item {
    pub(crate) name: String,
    pub(crate) kind: ItemKind,
    /// `Leaf.value`; `Section` has none.
    pub(crate) value: Option<f64>,
    pub(crate) class_selector: Option<String>,
}

struct Parser<'a> {
    lexer: Lexer<'a>,
    lookahead: VecDeque<Token>,
}

impl<'a> Parser<'a> {
    fn new(body: &'a str) -> Self {
        Self {
            lexer: Lexer::new(body),
            lookahead: VecDeque::new(),
        }
    }

    fn fill(&mut self, count: usize) -> Result<(), ParseError> {
        while self.lookahead.len() < count {
            self.lookahead.push_back(self.lexer.next_token()?);
        }
        Ok(())
    }

    fn peek(&mut self) -> Result<TokenKind, ParseError> {
        self.fill(1)?;
        Ok(self.lookahead[0].kind)
    }

    /// The token after the next one, for the `Leaf | Section` decision.
    fn peek_second(&mut self) -> Result<TokenKind, ParseError> {
        self.fill(2)?;
        Ok(self.lookahead[1].kind)
    }

    fn bump(&mut self) -> Result<Token, ParseError> {
        self.fill(1)?;
        Ok(self.lookahead.pop_front().expect("filled above"))
    }

    fn expect(&mut self, expected: TokenKind) -> Result<Token, ParseError> {
        if self.peek()? != expected {
            return Err(self.unexpected(expected.name()));
        }
        self.bump()
    }

    fn unexpected(&mut self, expected: &str) -> ParseError {
        match self.fill(1) {
            Ok(()) => {
                let token = &self.lookahead[0];
                ParseError::at(
                    format!("expected {expected}, got '{}'", token.kind.name()),
                    token.span,
                )
            }
            Err(error) => error,
        }
    }

    fn parse_treemap(mut self) -> Result<ParsedTreemap, ParseError> {
        // `cleanupComments` trims the whole document's leading whitespace.
        while self.peek()? == TokenKind::Indentation {
            self.bump()?;
        }
        self.expect(TokenKind::Treemap)?;

        let mut parsed = ParsedTreemap::default();
        loop {
            match self.peek()? {
                TokenKind::Eof => break,
                TokenKind::AccDescr => {
                    let token = self.bump()?;
                    parsed.acc_description = Some(acc_descr_value(&token.text));
                }
                TokenKind::AccTitle => {
                    let token = self.bump()?;
                    parsed.acc_title = Some(acc_title_value(&token.text));
                }
                TokenKind::Title => {
                    let token = self.bump()?;
                    parsed.title = Some(title_value(&token.text));
                }
                TokenKind::Indentation | TokenKind::Str | TokenKind::ClassDef => {
                    parsed.rows.push(self.parse_row()?);
                }
                _ => {
                    return Err(self.unexpected(
                        "a title, an accessibility line, a quoted node, or a classDef",
                    ));
                }
            }
        }
        Ok(parsed)
    }

    fn parse_row(&mut self) -> Result<ParsedRow, ParseError> {
        let indent = if self.peek()? == TokenKind::Indentation {
            Some(self.bump()?.text.chars().count())
        } else {
            None
        };
        match self.peek()? {
            TokenKind::Str => self.parse_item(indent),
            TokenKind::ClassDef => {
                let token = self.bump()?;
                Ok(class_def_row(&token.text))
            }
            _ => Err(self.unexpected("a quoted node or a classDef")),
        }
    }

    fn parse_item(&mut self, indent: Option<usize>) -> Result<ParsedRow, ParseError> {
        let token = self.expect(TokenKind::Str)?;
        let name = string_value(&token.text);

        // `Leaf` may consume one `INDENTATION` before its `:` or `,`; `Section` cannot. Only
        // take it when the separator follows, or the indentation belongs to the next row.
        if self.peek()? == TokenKind::Indentation
            && matches!(self.peek_second()?, TokenKind::Separator | TokenKind::Comma)
        {
            self.bump()?;
        }

        if matches!(self.peek()?, TokenKind::Separator | TokenKind::Comma) {
            self.bump()?;
            if self.peek()? == TokenKind::Indentation {
                self.bump()?;
            }
            let value = self.expect(TokenKind::Number)?;
            let class_selector = self.parse_class_selector()?;
            Ok(ParsedRow::Item {
                indent,
                item: Item {
                    name,
                    kind: ItemKind::Leaf,
                    value: Some(convert_number(&value.text)),
                    class_selector,
                },
            })
        } else {
            let class_selector = self.parse_class_selector()?;
            Ok(ParsedRow::Item {
                indent,
                item: Item {
                    name,
                    kind: ItemKind::Section,
                    value: None,
                    class_selector,
                },
            })
        }
    }

    /// `(STYLE_SEPARATOR classSelector=ID2)?`.
    fn parse_class_selector(&mut self) -> Result<Option<String>, ParseError> {
        if self.peek()? != TokenKind::StyleSeparator {
            return Ok(None);
        }
        self.bump()?;
        Ok(Some(self.expect(TokenKind::Id)?.text))
    }
}

/// `treemap/valueConverter.ts`'s `ClassDef` branch: `classDefRegex.exec(input)`.
///
/// `classDefRegex = /classDef\s+([A-Z_a-z]\w+)(?:\s+([^\n\r;]*))?;?/`. A match always exists
/// when the `CLASS_DEF` terminal matched, because both patterns have the same shape; when the
/// regex finds nothing, upstream's default conversion returns the raw string and the row
/// keeps no item.
fn class_def_row(input: &str) -> ParsedRow {
    let Some(after) = input.strip_prefix("classDef") else {
        return ParsedRow::Skipped;
    };
    let space = js_space_len(after);
    if space == 0 {
        return ParsedRow::Skipped;
    }
    let rest = &after[space..];
    let bytes = rest.as_bytes();
    if !matches!(bytes.first(), Some(byte) if byte.is_ascii_alphabetic() || *byte == b'_') {
        return ParsedRow::Skipped;
    }
    let mut end = 1;
    while bytes
        .get(end)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    {
        end += 1;
    }
    if end < 2 {
        return ParsedRow::Skipped;
    }
    let class_name = rest[..end].to_string();
    let rest = &rest[end..];
    let style_space = js_space_len(rest);
    let style_text = if style_space == 0 {
        String::new()
    } else {
        let value = &rest[style_space..];
        let stop = value.find([';', '\r', '\n']).unwrap_or(value.len());
        value[..stop].to_string()
    };
    ParsedRow::ClassDef {
        class_name,
        style_text,
    }
}

/// Langium's `STRING2` conversion: the text between the quotes, escapes untouched.
fn string_value(raw: &str) -> String {
    raw[1..raw.len() - 1].to_string()
}

/// The `MyNumber` data type rule: the custom `NUMBER2` conversion
/// (`parseFloat(input.replace(/,/g, ''))`) becomes a string, and the `number` return type then
/// converts it with `Number`, which is the same value.
fn convert_number(raw: &str) -> f64 {
    let without_commas: String = raw.chars().filter(|ch| *ch != ',').collect();
    js_parse_float(&without_commas)
}

/// JavaScript's `parseFloat` for a string of digits, `_`, `.`, and `,` (after commas were
/// removed): the longest prefix `digits ('.' digits?)?` or `'.' digits`, `NaN` without digits.
/// A value too large for an `f64` parses to infinity, as `parseFloat` does.
fn js_parse_float(text: &str) -> f64 {
    let bytes = text.as_bytes();
    let mut end = 0;
    let mut digits = 0;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
        digits += 1;
    }
    if bytes.get(end) == Some(&b'.') {
        end += 1;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return f64::NAN;
    }
    text[..end].parse::<f64>().unwrap_or(f64::NAN)
}

/// `common/valueConverter.ts`'s `titleRegex`: `/title([\t ][^\n\r]*|)/`, capture trimmed and
/// with runs of two or more spaces or tabs collapsed to one.
fn title_value(raw: &str) -> String {
    let Some(start) = raw.find("title") else {
        return String::new();
    };
    let rest = &raw[start + "title".len()..];
    if rest.starts_with(' ') || rest.starts_with('\t') {
        collapse(rest)
    } else {
        String::new()
    }
}

/// `accessibilityTitleRegex`: `/accTitle[\t ]*:([^\n\r]*)/`.
fn acc_title_value(raw: &str) -> String {
    let Some(start) = raw.find("accTitle") else {
        return String::new();
    };
    let rest = trim_horizontal(&raw[start + "accTitle".len()..]);
    let Some(value) = rest.strip_prefix(':') else {
        return String::new();
    };
    collapse(&value[..line_end(value)])
}

/// `accessibilityDescrRegex`: `/accDescr(?:[\t ]*:([^\n\r]*)|\s*{([^}]*)})/`.
fn acc_descr_value(raw: &str) -> String {
    let Some(start) = raw.find("accDescr") else {
        return String::new();
    };
    let rest = &raw[start + "accDescr".len()..];
    let after_colon_space = trim_horizontal(rest);
    if let Some(value) = after_colon_space.strip_prefix(':') {
        return collapse(&value[..line_end(value)]);
    }
    let Some(value) = rest.trim_start_matches(is_js_whitespace).strip_prefix('{') else {
        return String::new();
    };
    let capture = &value[..value.find('}').unwrap_or(value.len())];
    normalize_multiline(capture)
}

/// The multiline branch of the accessible description conversion: strip whitespace at both
/// ends of every line, collapse runs of two or more spaces or tabs, collapse line breaks, and
/// collapse consecutive blank lines. For the regex sequence
/// `.replace(/^\s*/gm,'').replace(/\s+$/gm,'').replace(/[\t ]{2,}/gm,' ').replace(/[\n\r]{2,}/gm,'\n')`
/// this is the same result, including the blank lines the first replacement swallows.
fn normalize_multiline(capture: &str) -> String {
    let mut lines = Vec::new();
    for line in capture.split(['\n', '\r']) {
        let line = trim(line);
        if line.is_empty() {
            continue;
        }
        lines.push(collapse(line));
    }
    lines.join("\n")
}

/// `String.prototype.trim`, close enough: JavaScript's `\s` also has U+FEFF.
fn trim(text: &str) -> &str {
    text.trim_matches(is_js_whitespace)
}

fn trim_horizontal(text: &str) -> &str {
    text.trim_start_matches([' ', '\t'])
}

/// `.replace(/[\t ]{2,}/gm, ' ')` on an already trimmed string.
fn collapse(text: &str) -> String {
    let text = trim(text);
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b' ' || bytes[i] == b'\t' {
            let start = i;
            while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
                i += 1;
            }
            if i - start > 1 {
                out.push(' ');
            } else {
                out.push_str(&text[start..i]);
            }
        } else {
            let ch = text[i..].chars().next().expect("non-empty remainder");
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

fn line_end(text: &str) -> usize {
    text.find(['\r', '\n']).unwrap_or(text.len())
}

/// The length of `\s+` at the start of `text`.
fn js_space_len(text: &str) -> usize {
    text.char_indices()
        .find(|&(_, ch)| !is_js_whitespace(ch))
        .map_or(text.len(), |(index, _)| index)
}
