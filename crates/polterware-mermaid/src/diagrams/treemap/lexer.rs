//! The treemap lexer: a port of the terminals `treemap.langium` declares, in the order
//! Langium's `DefaultTokenBuilder` puts them.
//!
//! The order is observable, because Chevrotain takes the first token pattern that matches at
//! the current offset, not the longest:
//!
//! 1. no keyword tokens: the grammar's parser rules contain no `'literal'`, so Langium's
//!    generated `KeywordNames` is `never`. `TREEMAP_KEYWORD`, `STYLE_SEPARATOR`, `SEPARATOR`,
//!    and `COMMA` are terminal rules whose bodies are keywords, and Langium turns a terminal
//!    into one regex token named after the rule, not into keyword tokens. `TreemapTokenBuilder`
//!    restricts a keyword named `treemap`, which therefore never exists here;
//! 2. terminals in grammar order: `ACC_DESCR`, `ACC_TITLE`, `TITLE`, `TREEMAP_KEYWORD`,
//!    `CLASS_DEF`, `STYLE_SEPARATOR`, `SEPARATOR`, `COMMA`, `INDENTATION`, then the hidden
//!    `WS` (skipped), `ML_COMMENT`, and `NL` (skipped), then `ID2`, `NUMBER2`, `STRING2`.
//!
//! `BOOLEAN` is not reachable from the entry rule and is not a token. `WS` is unreachable in
//! practice: `INDENTATION` has the same `[ \t]+` pattern and comes first, so every horizontal
//! whitespace run is an `INDENTATION` token. That is why the grammar's comment that whitespace
//! is ignored before `treemap` does not hold for a header preceded by spaces (see
//! [`super::parser`]).
//!
//! `ML_COMMENT` is a hidden token here, but mermaid removes whole `%%` comment lines before
//! Langium ever sees them (`diagram-api/comments.ts` `cleanupComments`), and
//! [`crate::preprocess`] does the same. A body handed to this lexer directly may still keep
//! comments, so it removes those lines itself, leading indentation included, which keeps an
//! indented comment from leaving an `INDENTATION` token behind.
//!
//! Every JavaScript regex is matched by hand, including the backtracking the patterns allow.

use crate::common::text::newline;
use crate::error::{ParseError, Span};
use crate::preprocess::is_js_whitespace;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    /// `ACC_DESCR`.
    AccDescr,
    /// `ACC_TITLE`.
    AccTitle,
    /// `TITLE`.
    Title,
    /// `TREEMAP_KEYWORD` (`treemap-beta` or `treemap`).
    Treemap,
    /// `CLASS_DEF`.
    ClassDef,
    /// `STYLE_SEPARATOR` (`:::`).
    StyleSeparator,
    /// `SEPARATOR` (`:`).
    Separator,
    /// `COMMA` (`,`).
    Comma,
    /// `INDENTATION` (`[ \t]+`).
    Indentation,
    /// `STRING2`.
    Str,
    /// `ID2`.
    Id,
    /// `NUMBER2`.
    Number,
    Eof,
}

impl TokenKind {
    /// The token name upstream's generated parser prints in an error.
    pub(crate) fn name(self) -> &'static str {
        match self {
            TokenKind::AccDescr => "ACC_DESCR",
            TokenKind::AccTitle => "ACC_TITLE",
            TokenKind::Title => "TITLE",
            TokenKind::Treemap => "TREEMAP_KEYWORD",
            TokenKind::ClassDef => "CLASS_DEF",
            TokenKind::StyleSeparator => "STYLE_SEPARATOR",
            TokenKind::Separator => "SEPARATOR",
            TokenKind::Comma => "COMMA",
            TokenKind::Indentation => "INDENTATION",
            TokenKind::Str => "STRING2",
            TokenKind::Id => "ID2",
            TokenKind::Number => "NUMBER2",
            TokenKind::Eof => "EOF",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Token {
    pub(crate) kind: TokenKind,
    /// The matched text, empty for [`TokenKind::Eof`].
    pub(crate) text: String,
    pub(crate) span: Span,
}

pub(crate) struct Lexer<'a> {
    source: &'a str,
    offset: usize,
    line: u32,
    column: u32,
    /// True while the current line holds only horizontal whitespace so far.
    line_start: bool,
}

impl<'a> Lexer<'a> {
    pub(crate) fn new(source: &'a str) -> Self {
        Self {
            source,
            offset: 0,
            line: 1,
            column: 1,
            line_start: true,
        }
    }

    pub(crate) fn next_token(&mut self) -> Result<Token, ParseError> {
        loop {
            let rest = &self.source[self.offset..];
            if rest.is_empty() {
                return Ok(Token {
                    kind: TokenKind::Eof,
                    text: String::new(),
                    span: self.span_to(self.offset),
                });
            }
            // `cleanupComments`: a line made of whitespace and a `%%` comment (not a
            // directive) disappears whole, indentation and trailing newline included.
            if self.line_start
                && let Some(end) = whole_line_comment(rest)
            {
                self.advance(self.offset + end);
                continue;
            }
            if let Some(end) = acc_descr(rest) {
                return Ok(self.finish(TokenKind::AccDescr, end));
            }
            if let Some(end) = acc_title(rest) {
                return Ok(self.finish(TokenKind::AccTitle, end));
            }
            if let Some(end) = title(rest) {
                return Ok(self.finish(TokenKind::Title, end));
            }
            if let Some(end) = treemap_keyword(rest) {
                return Ok(self.finish(TokenKind::Treemap, end));
            }
            if let Some(end) = class_def(rest) {
                return Ok(self.finish(TokenKind::ClassDef, end));
            }
            if rest.starts_with(":::") {
                return Ok(self.finish(TokenKind::StyleSeparator, 3));
            }
            if rest.starts_with(':') {
                return Ok(self.finish(TokenKind::Separator, 1));
            }
            if rest.starts_with(',') {
                return Ok(self.finish(TokenKind::Comma, 1));
            }
            if let Some(end) = horizontal_space(rest) {
                return Ok(self.finish(TokenKind::Indentation, end));
            }
            if rest.starts_with("%%") {
                self.advance(self.offset + comment(rest));
                continue;
            }
            if let Some(end) = newline(rest) {
                self.advance(self.offset + end);
                continue;
            }
            if let Some(end) = id(rest) {
                return Ok(self.finish(TokenKind::Id, end));
            }
            if let Some(end) = number(rest) {
                return Ok(self.finish(TokenKind::Number, end));
            }
            if let Some(end) = string(rest) {
                return Ok(self.finish(TokenKind::Str, end));
            }
            let ch = rest.chars().next().expect("non-empty remainder");
            return Err(ParseError::at(
                format!("unexpected character '{ch}'"),
                self.span_to(self.offset),
            ));
        }
    }

    fn finish(&mut self, kind: TokenKind, end: usize) -> Token {
        let end = self.offset + end;
        let span = self.span_to(end);
        let text = self.source[self.offset..end].to_string();
        self.advance(end);
        Token { kind, text, span }
    }

    fn span_to(&self, end: usize) -> Span {
        Span {
            line: self.line,
            column: self.column,
            start: self.offset,
            end,
        }
    }

    fn advance(&mut self, end: usize) {
        for ch in self.source[self.offset..end].chars() {
            if ch == '\n' {
                self.line += 1;
                self.column = 1;
                self.line_start = true;
            } else {
                self.column += 1;
                if ch != ' ' && ch != '\t' {
                    self.line_start = false;
                }
            }
        }
        self.offset = end;
    }
}

/// `cleanupComments`' `^\s*%%(?!{)[^\n]+\n?` at the start of a line: the length to skip,
/// through the line's newline when there is one.
fn whole_line_comment(rest: &str) -> Option<usize> {
    let indent = horizontal_space(rest).unwrap_or(0);
    let after = rest[indent..].strip_prefix("%%")?;
    if after.starts_with('{') || after.is_empty() || after.starts_with('\n') {
        return None;
    }
    let line = after.find('\n').map_or(after.len(), |index| index + 1);
    Some(indent + 2 + line)
}

/// The length of `[\t ]*` at the start of `text`.
fn horizontal_space(text: &str) -> Option<usize> {
    let end = text
        .char_indices()
        .find(|&(_, ch)| ch != ' ' && ch != '\t')
        .map_or(text.len(), |(index, _)| index);
    (end > 0).then_some(end)
}

/// The length of `\s*` at the start of `text`, JavaScript's whitespace.
fn js_space(text: &str) -> usize {
    text.char_indices()
        .find(|&(_, ch)| !is_js_whitespace(ch))
        .map_or(text.len(), |(index, _)| index)
}

/// `ACC_DESCR: /[\t ]*accDescr(?:[\t ]*:([^\n\r]*?(?=%%)|[^\n\r]*)|\s*{([^}]*)})/`.
fn acc_descr(rest: &str) -> Option<usize> {
    let mut start = horizontal_space(rest).unwrap_or(0);
    rest[start..].strip_prefix("accDescr")?;
    start += "accDescr".len();
    let colon = start + horizontal_space(&rest[start..]).unwrap_or(0);
    if rest[colon..].starts_with(':') {
        let value = colon + 1;
        return Some(value + line_before_comment(&rest[value..]));
    }
    let brace = start + js_space(&rest[start..]);
    rest[brace..].strip_prefix('{')?;
    let close = rest[brace + 1..].find('}')?;
    Some(brace + 1 + close + 1)
}

/// `ACC_TITLE: /[\t ]*accTitle[\t ]*:(?:[^\n\r]*?(?=%%)|[^\n\r]*)/`.
fn acc_title(rest: &str) -> Option<usize> {
    let mut start = horizontal_space(rest).unwrap_or(0);
    rest[start..].strip_prefix("accTitle")?;
    start += "accTitle".len();
    start += horizontal_space(&rest[start..]).unwrap_or(0);
    rest[start..].strip_prefix(':')?;
    start += 1;
    Some(start + line_before_comment(&rest[start..]))
}

/// `TITLE: /[\t ]*title(?:[\t ][^\n\r]*?(?=%%)|[\t ][^\n\r]*|)/`.
fn title(rest: &str) -> Option<usize> {
    let mut start = horizontal_space(rest).unwrap_or(0);
    rest[start..].strip_prefix("title")?;
    start += "title".len();
    if rest[start..].starts_with(' ') || rest[start..].starts_with('\t') {
        start += 1;
        start += line_before_comment(&rest[start..]);
    }
    Some(start)
}

/// `TREEMAP_KEYWORD: 'treemap-beta' | 'treemap'`.
fn treemap_keyword(rest: &str) -> Option<usize> {
    if rest.starts_with("treemap-beta") {
        Some("treemap-beta".len())
    } else if rest.starts_with("treemap") {
        Some("treemap".len())
    } else {
        None
    }
}

/// `CLASS_DEF: /classDef\s+([a-zA-Z_][a-zA-Z0-9_]+)(?:\s+([^;\r\n]*))?(?:;)?/`.
fn class_def(rest: &str) -> Option<usize> {
    let after = rest.strip_prefix("classDef")?;
    let space = js_space(after);
    if space == 0 {
        return None;
    }
    let name = &after[space..];
    let bytes = name.as_bytes();
    if !matches!(bytes.first(), Some(byte) if byte.is_ascii_alphabetic() || *byte == b'_') {
        return None;
    }
    let mut end = 1;
    while bytes
        .get(end)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    {
        end += 1;
    }
    // `[a-zA-Z_][a-zA-Z0-9_]+` needs at least two characters.
    if end < 2 {
        return None;
    }
    let mut pos = "classDef".len() + space + end;
    // `(?:\s+([^;\r\n]*))?`: only when whitespace follows the name. The capture may be empty.
    let style_space = js_space(&rest[pos..]);
    if style_space > 0 {
        pos += style_space;
        while !matches!(rest.as_bytes().get(pos), Some(b';' | b'\r' | b'\n') | None) {
            pos += 1;
        }
    }
    if rest.as_bytes().get(pos) == Some(&b';') {
        pos += 1;
    }
    Some(pos)
}

/// `ML_COMMENT: /\%\%[^\n]*/`, a hidden token: the comment to the end of its line.
fn comment(rest: &str) -> usize {
    rest.find('\n').unwrap_or(rest.len())
}

/// `ID2: /[a-zA-Z_][a-zA-Z0-9_]*/`.
fn id(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    if !matches!(bytes.first(), Some(byte) if byte.is_ascii_alphabetic() || *byte == b'_') {
        return None;
    }
    let mut end = 1;
    while bytes
        .get(end)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    {
        end += 1;
    }
    Some(end)
}

/// `NUMBER2: /[0-9_\.\,]+/`.
fn number(rest: &str) -> Option<usize> {
    let end = rest
        .char_indices()
        .find(|&(_, ch)| !ch.is_ascii_digit() && ch != '_' && ch != '.' && ch != ',')
        .map_or(rest.len(), |(index, _)| index);
    (end > 0).then_some(end)
}

/// `STRING2: /"[^"]*"|'[^']*'/`.
///
/// There are no escapes: the first matching quote closes the token, and a quote that never
/// closes makes the terminal fail.
fn string(rest: &str) -> Option<usize> {
    let quote = rest.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let end = rest[quote.len_utf8()..].find(quote)?;
    Some(quote.len_utf8() + end + quote.len_utf8())
}

/// The length of `[^\n\r]*?(?=%%)|[^\n\r]*` at the start of `text`.
fn line_before_comment(text: &str) -> usize {
    let end = text.find(['\r', '\n']).unwrap_or(text.len());
    text[..end].find("%%").unwrap_or(end)
}
