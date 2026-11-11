//! The cynefin lexer: a port of the terminals and keywords `cynefin.langium` and
//! `common.langium` declare, in the order Langium's `DefaultTokenBuilder` puts them.
//!
//! 1. keywords, sorted by descending length: `cynefin-beta:`, `cynefin-beta`, `-->`, `:`.
//!    `CynefinTokenBuilder` restricts `cynefin-beta` (but not `cynefin-beta:`) to be followed
//!    by `%%`, whitespace, or end of input;
//! 2. terminals in grammar order: `DOMAIN_NAME`, then the imported `ACC_DESCR`, `ACC_TITLE`,
//!    `TITLE`, `STRING`, `NEWLINE`;
//! 3. hidden terminals: `WHITESPACE` (skipped, group `SKIPPED`), then `YAML`, `DIRECTIVE`,
//!    and `SINGLE_LINE_COMMENT`.
//!
//! `YAML` and `DIRECTIVE` can only appear in text preprocessing has not cleaned, so only
//! `SINGLE_LINE_COMMENT` is reproduced from the hidden rules. The common terminals no parser
//! rule reaches (`BOOLEAN`, `FLOAT`, `INT`, `NUMBER`, `ID`) are not tokens, matching Langium's
//! reachability filter; a word that is not a domain name is therefore junk, exactly as in the
//! generated lexer.
//!
//! Chevrotain takes the longest match at the current offset rather than the first, which is
//! observable inside `DOMAIN_NAME`: `complicated` is one token, not `complex` followed by
//! `icated`. The alternatives are matched longest first here for the same result. Across token
//! types only the leading whitespace of `ACC_DESCR`, `ACC_TITLE`, `TITLE`, and
//! `SINGLE_LINE_COMMENT` overlaps: the first three are tried before `WHITESPACE` so their
//! token keeps it, and skipping a comment's leading whitespace first consumes the same bytes.

use crate::error::{ParseError, Span};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    /// The `cynefin-beta` keyword.
    CynefinBeta,
    /// The `cynefin-beta:` keyword.
    CynefinBetaColon,
    /// The `-->` keyword of `Transition`.
    Arrow,
    /// The `:` keyword of `Transition`.
    Colon,
    /// `DOMAIN_NAME`.
    DomainName,
    /// `ACC_DESCR`.
    AccDescr,
    /// `ACC_TITLE`.
    AccTitle,
    /// `TITLE`.
    Title,
    /// `STRING`.
    Str,
    /// `NEWLINE`.
    Newline,
    Eof,
}

impl TokenKind {
    /// The token name upstream's generated parser prints in an error.
    pub(crate) fn name(self) -> &'static str {
        match self {
            TokenKind::CynefinBeta => "cynefin-beta",
            TokenKind::CynefinBetaColon => "cynefin-beta:",
            TokenKind::Arrow => "-->",
            TokenKind::Colon => ":",
            TokenKind::DomainName => "DOMAIN_NAME",
            TokenKind::AccDescr => "ACC_DESCR",
            TokenKind::AccTitle => "ACC_TITLE",
            TokenKind::Title => "TITLE",
            TokenKind::Str => "STRING",
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

/// `DOMAIN_NAME: 'complex' | 'complicated' | 'clear' | 'chaotic' | 'confusion'`, written in
/// grammar order.
const DOMAIN_NAMES: [&str; 5] = ["complex", "complicated", "clear", "chaotic", "confusion"];

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
            if rest.starts_with("cynefin-beta:") {
                return Ok(self.finish(TokenKind::CynefinBetaColon, "cynefin-beta:".len()));
            }
            if let Some(end) = keyword(rest, "cynefin-beta") {
                return Ok(self.finish(TokenKind::CynefinBeta, end));
            }
            if rest.starts_with("-->") {
                return Ok(self.finish(TokenKind::Arrow, "-->".len()));
            }
            if rest.starts_with(':') {
                return Ok(self.finish(TokenKind::Colon, 1));
            }
            if let Some(end) = domain_name(rest) {
                return Ok(self.finish(TokenKind::DomainName, end));
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

/// `CynefinTokenBuilder`: the `cynefin-beta` keyword followed by `%%`, a non-space, or the end
/// of input, i.e. not directly followed by a visible character.
fn keyword(rest: &str, word: &str) -> Option<usize> {
    let after = rest.strip_prefix(word)?;
    let boundary = match after.chars().next() {
        None => true,
        Some(ch) => (ch == '%' && after.starts_with("%%")) || is_js_space(ch),
    };
    boundary.then_some(word.len())
}

/// `DOMAIN_NAME`, taking the longest alternative as Chevrotain's lexer does.
fn domain_name(rest: &str) -> Option<usize> {
    DOMAIN_NAMES
        .iter()
        .filter(|name| rest.starts_with(*name))
        .map(|name| name.len())
        .max()
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

/// `STRING: /"([^"\\]|\\.)*"|'([^'\\]|\\.)*'/`.
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

/// `NEWLINE: /\r?\n/`.
fn newline(rest: &str) -> Option<usize> {
    if rest.starts_with("\r\n") {
        Some(2)
    } else if rest.starts_with('\n') {
        Some(1)
    } else {
        None
    }
}

/// `WHITESPACE: /[\t ]+/`, a skipped hidden token.
fn horizontal_space(rest: &str) -> Option<usize> {
    let end = rest
        .char_indices()
        .find(|&(_, ch)| ch != ' ' && ch != '\t')
        .map_or(rest.len(), |(index, _)| index);
    (end > 0).then_some(end)
}

/// `SINGLE_LINE_COMMENT: /[\t ]*%%[^\n\r]*/`, a hidden token.
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
        .find(|&(_, ch)| !is_js_space(ch))
        .map_or(text.len(), |(index, _)| index)
}

/// JavaScript's `\s`, close enough for the differ on `\u{feff}`.
fn is_js_space(ch: char) -> bool {
    ch.is_whitespace() || ch == '\u{feff}'
}

fn is_line_terminator(ch: char) -> bool {
    matches!(ch, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}
