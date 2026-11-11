//! The pie lexer: a port of the terminals and keywords `pie.langium` and `common.langium`
//! declare, in the order Langium's `DefaultTokenBuilder` puts them.
//!
//! The order is observable, because Chevrotain takes the first token pattern that matches at
//! the current offset, not the longest:
//!
//! 1. keywords, sorted by descending length: `showData`, `pie`, `:`. `PieTokenBuilder`
//!    restricts `pie` and `showData` to be followed by whitespace, end of input, or `%%`;
//! 2. terminals in grammar order: `NUMBER_PIE` (whose regex inlines `FLOAT_PIE` and
//!    `INT_PIE`), `ACC_DESCR`, `ACC_TITLE`, `TITLE`, `STRING`, `NEWLINE`;
//! 3. hidden terminals: `WHITESPACE` (skipped, group `SKIPPED`), then `YAML`, `DIRECTIVE`,
//!    and `SINGLE_LINE_COMMENT` (group `hidden`).
//!
//! `YAML` and `DIRECTIVE` can only appear in text preprocessing has not cleaned, so only
//! `SINGLE_LINE_COMMENT` is reproduced from the hidden rules. The common terminals no parser
//! rule reaches (`BOOLEAN`, `FLOAT`, `INT`, `NUMBER`, `ID`) are not tokens, matching Langium's
//! reachability filter.
//!
//! The JavaScript regexes backtrack to satisfy their `(?!\.)` guards, so `60.67.3` tokenizes
//! as `60.6` rather than failing: the matchers below shorten the match the same way.

use crate::error::{ParseError, Span};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    /// The `showData` keyword.
    ShowData,
    /// The `pie` keyword.
    Pie,
    /// The `:` keyword of `PieSection`.
    Colon,
    /// `NUMBER_PIE`.
    Number,
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
            TokenKind::ShowData => "showData",
            TokenKind::Pie => "pie",
            TokenKind::Colon => ":",
            TokenKind::Number => "NUMBER_PIE",
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
            if let Some(end) = keyword(rest, "showData") {
                return Ok(self.finish(TokenKind::ShowData, end));
            }
            if let Some(end) = keyword(rest, "pie") {
                return Ok(self.finish(TokenKind::Pie, end));
            }
            if rest.starts_with(':') {
                return Ok(self.finish(TokenKind::Colon, 1));
            }
            if let Some(end) = number(rest) {
                return Ok(self.finish(TokenKind::Number, end));
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

/// `PieTokenBuilder`: the keyword followed by nothing, `%%`, or whitespace.
fn keyword(rest: &str, word: &str) -> Option<usize> {
    let after = rest.strip_prefix(word)?;
    let boundary = match after.chars().next() {
        None => true,
        Some(ch) => ch == '%' && after.starts_with("%%") || is_js_space(ch),
    };
    boundary.then_some(word.len())
}

/// `NUMBER_PIE: FLOAT_PIE | INT_PIE`, with the backtracking the `(?!\.)` guards allow.
fn number(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    let sign = usize::from(bytes.first() == Some(&b'-'));
    let mut integer_end = sign;
    while bytes.get(integer_end).is_some_and(u8::is_ascii_digit) {
        integer_end += 1;
    }
    // FLOAT_PIE: /-?[0-9]+\.[0-9]+(?!\.)/
    if integer_end > sign && bytes.get(integer_end) == Some(&b'.') {
        let mut fraction_end = integer_end + 1;
        while bytes.get(fraction_end).is_some_and(u8::is_ascii_digit) {
            fraction_end += 1;
        }
        let fraction = fraction_end - integer_end - 1;
        if fraction > 0 {
            if bytes.get(fraction_end) == Some(&b'.') {
                // Give one digit back so the lookahead sees a digit, or fail when there is
                // no digit to give back.
                if fraction > 1 {
                    return Some(fraction_end - 1);
                }
            } else {
                return Some(fraction_end);
            }
        }
    }
    // INT_PIE: /-?(0|[1-9][0-9]*)(?!\.)/
    let digit = bytes.get(sign)?;
    if !digit.is_ascii_digit() {
        return None;
    }
    if bytes.get(sign + 1) == Some(&b'.') {
        return None;
    }
    if digit == &b'0' {
        return Some(sign + 1);
    }
    let mut end = sign;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    if bytes.get(end) == Some(&b'.') {
        // The guard backtracks one digit; a single digit cannot give one back.
        if end - sign > 1 { Some(end - 1) } else { None }
    } else {
        Some(end)
    }
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
