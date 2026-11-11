//! The architecture lexer: the terminals and keywords `architecture.langium`, `arch.langium`,
//! and `common.langium` declare, in the order Langium's `DefaultTokenBuilder` puts them.
//!
//! The order is observable, because Chevrotain takes the first token pattern that matches at
//! the current offset, not the longest:
//!
//! 1. keywords, sorted by descending length (stable, so equal lengths keep grammar order):
//!    `architecture-beta`, `junction`, `service`, `column`, `group`, `align`, `row`, `--`,
//!    `in`, `:`, `-`. `ArchitectureTokenBuilder` restricts the keyword named `architecture`,
//!    and this grammar's only long keyword is `architecture-beta`, so no keyword carries the
//!    `(?:(?=%%)|(?!\S))` boundary here;
//! 2. terminals in the merged grammar's order: `ARROW_DIRECTION`, `ARROW_GROUP`,
//!    `ARROW_INTO` from `architecture.langium`, then the reachable common terminals
//!    `ACC_DESCR`, `ACC_TITLE`, `TITLE`, `STRING`, `ID`, `NEWLINE`, then the hidden
//!    `WHITESPACE` (skipped), `YAML`, `DIRECTIVE`, and `SINGLE_LINE_COMMENT` (group `hidden`),
//!    and last `ARCH_ICON` and `ARCH_TITLE` from `arch.langium`, which the import list places
//!    after `common`.
//!
//! The grammar reaches no other common terminal, so `BOOLEAN`, `FLOAT`, `INT`, and `NUMBER`
//! are not tokens, exactly as upstream's generated lexer has it. `YAML` and `DIRECTIVE` can
//! only appear in text preprocessing has not cleaned, so only `SINGLE_LINE_COMMENT` is
//! reproduced from the hidden rules. `WHITESPACE` and `SINGLE_LINE_COMMENT` are skipped in the
//! loop, so the matchers below still run in the order above.
//!
//! Every keyword that the `ID` pattern fully matches gets `ID` as its Chevrotain
//! `LONGER_ALT`, so `rowspan` lexes as one `ID` rather than `row` and `span`, while a bare
//! `row` stays the keyword. That list is what `DefaultTokenBuilder.findLongerAlt` computes:
//! a terminal whose pattern can match the whole keyword.
//!
//! The JavaScript regexes are matched by hand, including the backtracking the
//! `ARCH_TITLE` alternation allows.

use crate::common::text::{is_line_terminator, newline};
use crate::error::{ParseError, Span};
use crate::preprocess::is_js_whitespace;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    /// The `architecture-beta` header keyword.
    ArchitectureBeta,
    Junction,
    Service,
    Column,
    Group,
    Align,
    Row,
    In,
    /// The `--` of an untitled edge body.
    DoubleDash,
    Colon,
    /// The `-` around an edge title.
    Dash,
    /// `ARROW_DIRECTION`.
    Direction,
    /// `ARROW_GROUP` (`{group}`).
    GroupModifier,
    /// `ARROW_INTO` (`<` or `>`).
    Into,
    /// `ARCH_ICON`.
    Icon,
    /// `ARCH_TITLE`.
    BracketTitle,
    /// `ACC_DESCR`.
    AccDescr,
    /// `ACC_TITLE`.
    AccTitle,
    /// `TITLE`.
    Title,
    /// `STRING`.
    Str,
    /// `ID`.
    Id,
    /// `NEWLINE`.
    Newline,
    Eof,
}

impl TokenKind {
    /// The token name upstream's generated parser prints in an error.
    pub(crate) fn name(self) -> &'static str {
        match self {
            TokenKind::ArchitectureBeta => "architecture-beta",
            TokenKind::Junction => "junction",
            TokenKind::Service => "service",
            TokenKind::Column => "column",
            TokenKind::Group => "group",
            TokenKind::Align => "align",
            TokenKind::Row => "row",
            TokenKind::In => "in",
            TokenKind::DoubleDash => "--",
            TokenKind::Colon => ":",
            TokenKind::Dash => "-",
            TokenKind::Direction => "ARROW_DIRECTION",
            TokenKind::GroupModifier => "ARROW_GROUP",
            TokenKind::Into => "ARROW_INTO",
            TokenKind::Icon => "ARCH_ICON",
            TokenKind::BracketTitle => "ARCH_TITLE",
            TokenKind::AccDescr => "ACC_DESCR",
            TokenKind::AccTitle => "ACC_TITLE",
            TokenKind::Title => "TITLE",
            TokenKind::Str => "STRING",
            TokenKind::Id => "ID",
            TokenKind::Newline => "NEWLINE",
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
    ("architecture-beta", TokenKind::ArchitectureBeta),
    ("junction", TokenKind::Junction),
    ("service", TokenKind::Service),
    ("column", TokenKind::Column),
    ("group", TokenKind::Group),
    ("align", TokenKind::Align),
    ("row", TokenKind::Row),
    ("--", TokenKind::DoubleDash),
    ("in", TokenKind::In),
    (":", TokenKind::Colon),
    ("-", TokenKind::Dash),
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
            if let Some(end) = direction(rest) {
                return Ok(self.finish(TokenKind::Direction, end));
            }
            if let Some(end) = group_modifier(rest) {
                return Ok(self.finish(TokenKind::GroupModifier, end));
            }
            if let Some(end) = into(rest) {
                return Ok(self.finish(TokenKind::Into, end));
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
            if let Some(end) = string(rest) {
                return Ok(self.finish(TokenKind::Str, end));
            }
            if let Some(end) = id(rest) {
                return Ok(self.finish(TokenKind::Id, end));
            }
            if let Some(end) = newline(rest) {
                return Ok(self.finish(TokenKind::Newline, end));
            }
            if let Some(end) = horizontal_space(rest) {
                self.advance(self.offset + end);
                continue;
            }
            if let Some(end) = comment(rest) {
                self.advance(self.offset + end);
                continue;
            }
            if let Some(end) = icon(rest) {
                return Ok(self.finish(TokenKind::Icon, end));
            }
            if let Some(end) = bracket_title(rest) {
                return Ok(self.finish(TokenKind::BracketTitle, end));
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

/// The first keyword that matches, with its `LONGER_ALT` `ID` applied.
fn keyword(rest: &str) -> Option<(TokenKind, usize)> {
    for (word, kind) in KEYWORDS {
        if !rest.starts_with(word) {
            continue;
        }
        // `findLongerAlt`: an `ID` that matches the whole keyword is a longer alternative.
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

/// `ARROW_DIRECTION: 'L' | 'R' | 'T' | 'B'`.
fn direction(rest: &str) -> Option<usize> {
    match rest.chars().next()? {
        'L' | 'R' | 'T' | 'B' => Some(1),
        _ => None,
    }
}

/// `ARROW_GROUP: /\{group\}/`.
fn group_modifier(rest: &str) -> Option<usize> {
    rest.starts_with("{group}").then_some(7)
}

/// `ARROW_INTO: /<|>/`.
fn into(rest: &str) -> Option<usize> {
    matches!(rest.chars().next()?, '<' | '>').then_some(1)
}

/// `ARCH_ICON: /\([\w-:]+\)/`.
fn icon(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    if bytes.first() != Some(&b'(') {
        return None;
    }
    let mut end = 1;
    while bytes
        .get(end)
        .is_some_and(|byte| is_word(*byte) || *byte == b'-' || *byte == b':')
    {
        end += 1;
    }
    if end == 1 || bytes.get(end) != Some(&b')') {
        return None;
    }
    Some(end + 1)
}

/// `ARCH_TITLE: /\[(?:"([^"\\]|\\.)*"|'([^'\\]|\\.)*'|[^\[\]\r\n]+)\]/`.
///
/// The quoted alternatives are tried first and allow a `]` inside the quotes; the unquoted
/// alternative takes one or more characters that are not brackets or line breaks. As in the
/// regular expression, a quoted alternative that does not end at a `]` lets the unquoted one
/// try instead.
fn bracket_title(rest: &str) -> Option<usize> {
    let body = rest.strip_prefix('[')?;
    if let Some(quote) = body.chars().next().filter(|ch| *ch == '"' || *ch == '\'')
        && let Some(quoted) = quoted_len(body, quote)
        && body[quoted..].starts_with(']')
    {
        return Some(1 + quoted + 1);
    }
    let mut end = 0;
    for ch in body.chars() {
        if matches!(ch, '[' | ']' | '\r' | '\n') {
            break;
        }
        end += ch.len_utf8();
    }
    (end > 0 && body[end..].starts_with(']')).then_some(1 + end + 1)
}

/// The length of `"([^"\\]|\\.)*"` with its closing quote, or `None` when it does not close.
/// The `.` of `\\.` does not match a line terminator, so a backslash before one fails.
fn quoted_len(text: &str, quote: char) -> Option<usize> {
    let mut chars = text.char_indices();
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
    if rest[start..].starts_with([' ', '\t']) {
        start += 1;
        start += line_before_comment(&rest[start..]);
    }
    Some(start)
}

/// `STRING: /"([^"\\]|\\.)*"|'([^'\\]|\\.)*'/`.
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

/// `ID: /[\w]([-\w]*\w)?/`: word characters and dashes, not ending with a dash.
fn id(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    if !bytes.first().is_some_and(|byte| is_word(*byte)) {
        return None;
    }
    let mut end = 1;
    while bytes
        .get(end)
        .is_some_and(|byte| is_word(*byte) || *byte == b'-')
    {
        end += 1;
    }
    while end > 1 && bytes[end - 1] == b'-' {
        end -= 1;
    }
    Some(end)
}

/// `WHITESPACE: /[\t ]+/`, a skipped hidden token.
fn horizontal_space(rest: &str) -> Option<usize> {
    let end = rest
        .char_indices()
        .find(|&(_, ch)| ch != ' ' && ch != '\t')
        .map_or(rest.len(), |(index, _)| index);
    (end > 0).then_some(end)
}

/// `SINGLE_LINE_COMMENT: /[\t ]*%%[^\n\r]*/`, a hidden token. The leading whitespace is
/// skipped as `WHITESPACE` first, because that token comes earlier in the lexer.
fn comment(rest: &str) -> Option<usize> {
    rest.strip_prefix("%%")?;
    Some(rest.find(['\r', '\n']).unwrap_or(rest.len()))
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

/// `\w`, which JavaScript without the `u` flag reads as ASCII.
fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}
