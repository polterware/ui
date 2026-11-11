//! The railroad lexer: the terminals and keywords `railroad.langium` declares, in the order
//! Langium's `DefaultTokenBuilder` puts them.
//!
//! The order is observable, because Chevrotain takes the first token pattern that matches at
//! the current offset, not the longest:
//!
//! 1. keywords, sorted by descending length (stable, so equal lengths keep grammar order):
//!    `railroad-beta`, `nonterminal`, `zeroOrMore`, `oneOrMore`, `sequence`, `optional`,
//!    `terminal`, `special`, `choice`, `=`, `;`, `(`, `,`, `)`. `RailroadTokenBuilder`
//!    restricts `railroad-beta` to be followed by whitespace, the end of the input, or `%%`,
//!    so `railroad-beta-rule` stays an identifier;
//! 2. terminals in grammar order: `TITLE`, `ACC_TITLE`, `ACC_DESCR`, `RR_ID`, `RR_STRING`,
//!    then the hidden `RR_WHITESPACE` (skipped), `RR_YAML`, `RR_DIRECTIVE`,
//!    `RR_SINGLE_LINE_COMMENT`, and `RR_BLOCK_COMMENT` (skipped under the `hidden` group).
//!
//! `RR_WHITESPACE` is `[\t \r\n]+`, so there is no newline token: rules and metadata lines are
//! separated only by skipped whitespace. `RR_YAML` and `RR_DIRECTIVE` can only appear in text
//! [`crate::preprocess`] has not cleaned, so only the comment terminals are reproduced from
//! the hidden rules.
//!
//! Every word keyword is fully matched by `RR_ID`, so `RR_ID` is its Chevrotain `LONGER_ALT`
//! (`DefaultTokenBuilder.findLongerAlt`): `sequenceRule` lexes as one `RR_ID` while a bare
//! `sequence` stays the keyword.
//!
//! `TITLE`, `ACC_TITLE`, and `ACC_DESCR` come before `RR_ID`, so a line that starts with
//! `title` is metadata even where the grammar expects a rule name. None of the three matches
//! leading whitespace: `RR_WHITESPACE` has already skipped it at the previous position.

use crate::common::text::is_line_terminator;
use crate::error::{ParseError, Span};
use crate::preprocess::is_js_whitespace;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    /// The `railroad-beta` header keyword.
    RailroadBeta,
    NonTerminal,
    ZeroOrMore,
    OneOrMore,
    Sequence,
    Optional,
    Terminal,
    Special,
    Choice,
    Equals,
    Semicolon,
    LParen,
    Comma,
    RParen,
    /// `TITLE`.
    Title,
    /// `ACC_TITLE`.
    AccTitle,
    /// `ACC_DESCR`.
    AccDescr,
    /// `RR_ID`.
    Id,
    /// `RR_STRING`.
    Str,
    Eof,
}

impl TokenKind {
    /// The token name upstream's generated parser prints in an error.
    pub(crate) fn name(self) -> &'static str {
        match self {
            TokenKind::RailroadBeta => "railroad-beta",
            TokenKind::NonTerminal => "nonterminal",
            TokenKind::ZeroOrMore => "zeroOrMore",
            TokenKind::OneOrMore => "oneOrMore",
            TokenKind::Sequence => "sequence",
            TokenKind::Optional => "optional",
            TokenKind::Terminal => "terminal",
            TokenKind::Special => "special",
            TokenKind::Choice => "choice",
            TokenKind::Equals => "=",
            TokenKind::Semicolon => ";",
            TokenKind::LParen => "(",
            TokenKind::Comma => ",",
            TokenKind::RParen => ")",
            TokenKind::Title => "TITLE",
            TokenKind::AccTitle => "ACC_TITLE",
            TokenKind::AccDescr => "ACC_DESCR",
            TokenKind::Id => "RR_ID",
            TokenKind::Str => "RR_STRING",
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

/// The keywords Langium collects from the reachable rules, in the order it sorts them:
/// descending length, stable in grammar order.
const KEYWORDS: &[(&str, TokenKind)] = &[
    ("railroad-beta", TokenKind::RailroadBeta),
    ("nonterminal", TokenKind::NonTerminal),
    ("zeroOrMore", TokenKind::ZeroOrMore),
    ("oneOrMore", TokenKind::OneOrMore),
    ("sequence", TokenKind::Sequence),
    ("optional", TokenKind::Optional),
    ("terminal", TokenKind::Terminal),
    ("special", TokenKind::Special),
    ("choice", TokenKind::Choice),
    ("=", TokenKind::Equals),
    (";", TokenKind::Semicolon),
    ("(", TokenKind::LParen),
    (",", TokenKind::Comma),
    (")", TokenKind::RParen),
];

pub(crate) struct Lexer<'a> {
    source: &'a str,
    offset: usize,
    line: u32,
    column: u32,
}

impl<'a> Lexer<'a> {
    pub(crate) fn new(source: &'a str) -> Self {
        Self {
            source,
            offset: 0,
            line: 1,
            column: 1,
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
            if let Some((kind, end)) = keyword(rest) {
                return Ok(self.finish(kind, end));
            }
            if let Some(end) = title(rest) {
                return Ok(self.finish(TokenKind::Title, end));
            }
            if let Some(end) = acc_title(rest) {
                return Ok(self.finish(TokenKind::AccTitle, end));
            }
            if let Some(end) = acc_descr(rest) {
                return Ok(self.finish(TokenKind::AccDescr, end));
            }
            if let Some(end) = id(rest) {
                return Ok(self.finish(TokenKind::Id, end));
            }
            if let Some(end) = string(rest) {
                return Ok(self.finish(TokenKind::Str, end));
            }
            if let Some(end) = whitespace(rest) {
                self.advance(self.offset + end);
                continue;
            }
            if let Some(end) = comment(rest) {
                self.advance(self.offset + end);
                continue;
            }
            if let Some(end) = block_comment(rest) {
                self.advance(self.offset + end);
                continue;
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
            } else {
                self.column += 1;
            }
        }
        self.offset = end;
    }
}

/// The first keyword that matches, with its `LONGER_ALT` `RR_ID` applied.
fn keyword(rest: &str) -> Option<(TokenKind, usize)> {
    for (word, kind) in KEYWORDS {
        if !rest.starts_with(word) {
            continue;
        }
        if *kind == TokenKind::RailroadBeta && !header_boundary(&rest[word.len()..]) {
            continue;
        }
        // `findLongerAlt`: an `RR_ID` that matches the whole keyword is a longer alternative.
        if id(word) == Some(word.len())
            && let Some(longer) = id(rest)
            && longer > word.len()
        {
            return Some((TokenKind::Id, longer));
        }
        return Some((*kind, word.len()));
    }
    None
}

/// `RailroadTokenBuilder`'s restriction on `railroad-beta`: `(?:(?=%%)|(?!\S))`.
fn header_boundary(after: &str) -> bool {
    after.starts_with("%%") || after.chars().next().is_none_or(is_js_whitespace)
}

/// `ACC_DESCR: /accDescr(?:[\t ]*:([^\n\r]*?(?=%%)|[^\n\r]*)|\s*{([^}]*)})/`.
fn acc_descr(rest: &str) -> Option<usize> {
    let mut start = 0;
    rest.strip_prefix("accDescr")?;
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

/// `ACC_TITLE: /accTitle[\t ]*:(?:[^\n\r]*?(?=%%)|[^\n\r]*)/`.
fn acc_title(rest: &str) -> Option<usize> {
    let mut start = 0;
    rest.strip_prefix("accTitle")?;
    start += "accTitle".len();
    start += horizontal_space(&rest[start..]).unwrap_or(0);
    rest[start..].strip_prefix(':')?;
    start += 1;
    Some(start + line_before_comment(&rest[start..]))
}

/// `TITLE: /title(?:[\t ][^\n\r]*?(?=%%)|[\t ][^\n\r]*|)/`.
fn title(rest: &str) -> Option<usize> {
    let mut start = 0;
    rest.strip_prefix("title")?;
    start += "title".len();
    if rest[start..].starts_with([' ', '\t']) {
        start += 1;
        start += line_before_comment(&rest[start..]);
    }
    Some(start)
}

/// `RR_ID: /[A-Z_a-z][\w-]*/`.
fn id(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    let first = *bytes.first()?;
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return None;
    }
    let mut end = 1;
    while bytes
        .get(end)
        .is_some_and(|byte| is_word(*byte) || *byte == b'-')
    {
        end += 1;
    }
    Some(end)
}

/// `RR_STRING: /"([^"\\]|\\.)*"|'([^'\\]|\\.)*'/`.
///
/// The `.` of `\\.` does not match a line terminator, so a backslash before one makes the
/// whole string fail to tokenize.
fn string(rest: &str) -> Option<usize> {
    let quote = rest.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let mut chars = rest.char_indices();
    chars.next();
    while let Some((index, ch)) = chars.next() {
        if ch == quote {
            return Some(index + ch.len_utf8());
        }
        if ch == '\\' {
            match chars.next() {
                Some((_, escaped)) if !is_line_terminator(escaped) => {}
                _ => return None,
            }
        }
    }
    None
}

/// `RR_WHITESPACE: /[\t \r\n]+/`, a skipped hidden token.
fn whitespace(rest: &str) -> Option<usize> {
    let end = rest
        .char_indices()
        .find(|&(_, ch)| !matches!(ch, '\t' | ' ' | '\r' | '\n'))
        .map_or(rest.len(), |(index, _)| index);
    (end > 0).then_some(end)
}

/// `RR_SINGLE_LINE_COMMENT: /[\t ]*%%[^\n\r]*/`, a hidden token. The leading whitespace is
/// skipped as `RR_WHITESPACE` first, because that token comes earlier in the lexer.
fn comment(rest: &str) -> Option<usize> {
    rest.strip_prefix("%%")?;
    Some(rest.find(['\r', '\n']).unwrap_or(rest.len()))
}

/// `RR_BLOCK_COMMENT: /\/\*[\s\S]*?\*\//`, a hidden token; it stops at the first `*/` and
/// spans lines. An unclosed comment is not a token, so the `/` is an unexpected character.
fn block_comment(rest: &str) -> Option<usize> {
    let body = rest.strip_prefix("/*")?;
    let close = body.find("*/")?;
    Some(2 + close + 2)
}

/// The length of `[^\n\r]*?(?=%%)|[^\n\r]*` at the start of `text`.
fn line_before_comment(text: &str) -> usize {
    let end = text.find(['\r', '\n']).unwrap_or(text.len());
    text[..end].find("%%").unwrap_or(end)
}

/// The length of `\s*` at the start of `text`.
fn js_space(text: &str) -> usize {
    text.char_indices()
        .find(|&(_, ch)| !is_js_whitespace(ch))
        .map_or(text.len(), |(index, _)| index)
}

/// `[\t ]*`.
fn horizontal_space(rest: &str) -> Option<usize> {
    let end = rest
        .char_indices()
        .find(|&(_, ch)| ch != ' ' && ch != '\t')
        .map_or(rest.len(), |(index, _)| index);
    (end > 0).then_some(end)
}

/// `\w`, which JavaScript without the `u` flag reads as ASCII.
fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}
