//! The radar lexer: a port of the keywords and terminals `radar.langium`, its
//! `tokenBuilder.ts`, and `common.langium` declare, in the order Langium's
//! `DefaultTokenBuilder` puts them.
//!
//! The order is observable, because Chevrotain takes the first token pattern that matches at
//! the current offset, not the longest. The one exception is a keyword token's
//! `LONGER_ALT`: when an `ID` matches more characters than the keyword at the same offset,
//! the `ID` wins.
//!
//! 1. keywords, sorted by descending length (first-seen order for equal lengths):
//!    `radar-beta:`, `radar-beta`, `showLegend`, `graticule`, `curve`, `ticks`, `axis`,
//!    `max`, `min`, `:`, `,`, `[`, `]`, `{`, `}`. `RadarTokenBuilder` restricts
//!    `radar-beta` (but not `radar-beta:`) to be followed by `%%`, whitespace, or end of
//!    input;
//! 2. terminals in grammar order: `GRATICULE` (the only terminal `radar.langium` declares),
//!    then the imported `BOOLEAN`, `ACC_DESCR`, `ACC_TITLE`, `TITLE`, `NUMBER`, `STRING`,
//!    `ID`, `NEWLINE`;
//! 3. hidden terminals: `WHITESPACE` (skipped, group `SKIPPED`), then `YAML`, `DIRECTIVE`,
//!    and `SINGLE_LINE_COMMENT` (group `hidden`).
//!
//! `FLOAT` and `INT` are not tokens: `NUMBER` inlines both, and Langium's reachability walk
//! (`getAllReachableRules(grammar, false)`) does not descend into terminal-rule references,
//! so the composed regex is the only token. `YAML` and `DIRECTIVE` can only appear in text
//! preprocessing has not cleaned, so only `SINGLE_LINE_COMMENT` is reproduced from the hidden
//! rules.
//!
//! Two consequences of the order deserve a note. Every word keyword defers to an `ID` that
//! matches more characters (`axes2` is an `ID`), but an equal-length `ID` loses (`axis` is
//! always the keyword). `GRATICULE` has no such wind-back, so `circle2` lexes as `GRATICULE`
//! followed by `NUMBER`, exactly as the generated lexer does.
//!
//! The JavaScript regexes backtrack to satisfy their `(?!\.)` guards, so `60.67.3` tokenizes
//! as `60.6` (the fraction gives one digit back) rather than failing: the matcher below
//! shortens the match the same way.

use crate::error::{ParseError, Span};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    /// The `radar-beta` keyword.
    RadarBeta,
    /// The `radar-beta:` keyword.
    RadarBetaColon,
    /// The `axis` keyword.
    Axis,
    /// The `curve` keyword.
    Curve,
    /// The `showLegend` option name.
    ShowLegend,
    /// The `ticks` option name.
    Ticks,
    /// The `max` option name.
    Max,
    /// The `min` option name.
    Min,
    /// The `graticule` option name.
    Graticule,
    /// `GRATICULE`: the `circle` or `polygon` option value.
    GraticuleValue,
    /// The `:` keyword of `DetailedEntry` and of the header's `radar-beta :` form.
    Colon,
    /// The `,` keyword between axes, curves, entries, and options.
    Comma,
    /// The `[` of a label.
    LBracket,
    /// The `]` of a label.
    RBracket,
    /// The `{` that opens a curve's entries.
    LBrace,
    /// The `}` that closes a curve's entries.
    RBrace,
    /// `BOOLEAN`.
    Boolean,
    /// `ACC_DESCR`.
    AccDescr,
    /// `ACC_TITLE`.
    AccTitle,
    /// `TITLE`.
    Title,
    /// `NUMBER`.
    Number,
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
            TokenKind::RadarBeta => "radar-beta",
            TokenKind::RadarBetaColon => "radar-beta:",
            TokenKind::Axis => "axis",
            TokenKind::Curve => "curve",
            TokenKind::ShowLegend => "showLegend",
            TokenKind::Ticks => "ticks",
            TokenKind::Max => "max",
            TokenKind::Min => "min",
            TokenKind::Graticule => "graticule",
            TokenKind::GraticuleValue => "GRATICULE",
            TokenKind::Colon => ":",
            TokenKind::Comma => ",",
            TokenKind::LBracket => "[",
            TokenKind::RBracket => "]",
            TokenKind::LBrace => "{",
            TokenKind::RBrace => "}",
            TokenKind::Boolean => "BOOLEAN",
            TokenKind::AccDescr => "ACC_DESCR",
            TokenKind::AccTitle => "ACC_TITLE",
            TokenKind::Title => "TITLE",
            TokenKind::Number => "NUMBER",
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
            if rest.starts_with("radar-beta:") {
                return Ok(self.finish(TokenKind::RadarBetaColon, "radar-beta:".len()));
            }
            if let Some(end) = restricted_keyword(rest, "radar-beta") {
                return Ok(self.finish(TokenKind::RadarBeta, end));
            }
            if let Some((kind, end)) = keyword_or_id(rest, "showLegend", TokenKind::ShowLegend) {
                return Ok(self.finish(kind, end));
            }
            if let Some((kind, end)) = keyword_or_id(rest, "graticule", TokenKind::Graticule) {
                return Ok(self.finish(kind, end));
            }
            if let Some((kind, end)) = keyword_or_id(rest, "curve", TokenKind::Curve) {
                return Ok(self.finish(kind, end));
            }
            if let Some((kind, end)) = keyword_or_id(rest, "ticks", TokenKind::Ticks) {
                return Ok(self.finish(kind, end));
            }
            if let Some((kind, end)) = keyword_or_id(rest, "axis", TokenKind::Axis) {
                return Ok(self.finish(kind, end));
            }
            if let Some((kind, end)) = keyword_or_id(rest, "max", TokenKind::Max) {
                return Ok(self.finish(kind, end));
            }
            if let Some((kind, end)) = keyword_or_id(rest, "min", TokenKind::Min) {
                return Ok(self.finish(kind, end));
            }
            if rest.starts_with(':') {
                return Ok(self.finish(TokenKind::Colon, 1));
            }
            if rest.starts_with(',') {
                return Ok(self.finish(TokenKind::Comma, 1));
            }
            if rest.starts_with('[') {
                return Ok(self.finish(TokenKind::LBracket, 1));
            }
            if rest.starts_with(']') {
                return Ok(self.finish(TokenKind::RBracket, 1));
            }
            if rest.starts_with('{') {
                return Ok(self.finish(TokenKind::LBrace, 1));
            }
            if rest.starts_with('}') {
                return Ok(self.finish(TokenKind::RBrace, 1));
            }
            if let Some(end) = graticule_value(rest) {
                return Ok(self.finish(TokenKind::GraticuleValue, end));
            }
            if let Some(end) = boolean(rest) {
                return Ok(self.finish(TokenKind::Boolean, end));
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
            if let Some(end) = number(rest) {
                return Ok(self.finish(TokenKind::Number, end));
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

/// The length of the literal `word` at the start of `rest`.
fn keyword_end(rest: &str, word: &str) -> Option<usize> {
    rest.starts_with(word).then_some(word.len())
}

/// `RadarTokenBuilder`: `radar-beta` must be followed by `%%`, whitespace, or end of input.
fn restricted_keyword(rest: &str, word: &str) -> Option<usize> {
    let end = keyword_end(rest, word)?;
    let after = &rest[end..];
    let boundary = after.is_empty()
        || after.starts_with("%%")
        || after
            .chars()
            .next()
            .is_some_and(crate::preprocess::is_js_whitespace);
    boundary.then_some(end)
}

/// A keyword token with its `LONGER_ALT` terminal, `ID`, as `DefaultTokenBuilder` builds it:
/// the identifier wins only when it matches strictly more characters.
fn keyword_or_id(rest: &str, word: &str, kind: TokenKind) -> Option<(TokenKind, usize)> {
    let end = keyword_end(rest, word)?;
    match id(rest) {
        Some(id_end) if id_end > end => Some((TokenKind::Id, id_end)),
        _ => Some((kind, end)),
    }
}

/// `GRATICULE: 'circle' | 'polygon'`.
fn graticule_value(rest: &str) -> Option<usize> {
    if rest.starts_with("circle") {
        Some("circle".len())
    } else if rest.starts_with("polygon") {
        Some("polygon".len())
    } else {
        None
    }
}

/// `BOOLEAN: 'true' | 'false'`.
fn boolean(rest: &str) -> Option<usize> {
    if rest.starts_with("true") {
        Some("true".len())
    } else if rest.starts_with("false") {
        Some("false".len())
    } else {
        None
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

/// `NUMBER: FLOAT | INT`, where `FLOAT` is `/[0-9]+\.[0-9]+(?!\.)/` and `INT` is
/// `/0|[1-9][0-9]*(?!\.)/`, with the backtracking the guards allow.
fn number(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    let mut integer_end = 0;
    while bytes.get(integer_end).is_some_and(u8::is_ascii_digit) {
        integer_end += 1;
    }
    if integer_end > 0 && bytes.get(integer_end) == Some(&b'.') {
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
    let digit = *bytes.first()?;
    if !digit.is_ascii_digit() {
        return None;
    }
    if digit == b'0' {
        return Some(1);
    }
    let mut end = 0;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    if bytes.get(end) == Some(&b'.') {
        // The guard backtracks one digit; a single digit cannot give one back.
        if end > 1 { Some(end - 1) } else { None }
    } else {
        Some(end)
    }
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

/// `ID: /[\w]([-\w]*\w)?/`.
fn id(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    if !is_word(*bytes.first()?) {
        return None;
    }
    let mut last_word_end = 1;
    let mut index = 1;
    while let Some(&byte) = bytes.get(index) {
        if is_word(byte) {
            last_word_end = index + 1;
            index += 1;
        } else if byte == b'-' {
            index += 1;
        } else {
            break;
        }
    }
    Some(last_word_end)
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
        .find(|&(_, ch)| !crate::preprocess::is_js_whitespace(ch))
        .map_or(text.len(), |(index, _)| index)
}

/// JavaScript's `\w`: an ASCII letter, digit, or underscore.
fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn is_line_terminator(ch: char) -> bool {
    matches!(ch, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}
