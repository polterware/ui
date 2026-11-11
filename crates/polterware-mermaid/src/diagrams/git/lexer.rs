//! The git graph lexer: the keywords and terminals `gitGraph.langium`, `reference.langium`,
//! and `common.langium` declare, in the order Langium's `DefaultTokenBuilder` produces them.
//!
//! The grammar declares no lexer modes, so there is one token stream. The order is
//! observable, because Chevrotain takes the first pattern that matches at the current offset:
//!
//! 1. keyword tokens, from every string literal in the grammar rules, deduplicated and sorted
//!    by descending length. `GitGraphTokenBuilder` rewrites only the `gitGraph` keyword so it
//!    must be followed by `%%`, whitespace, or the end of the input;
//! 2. terminals in declaration order: `ACC_DESCR`, `ACC_TITLE`, `TITLE`, `INT`, `STRING`,
//!    `NEWLINE`;
//! 3. hidden terminals: `WHITESPACE` (skipped), then `YAML`, `DIRECTIVE`, and
//!    `SINGLE_LINE_COMMENT` (hidden);
//! 4. `REFERENCE`, which `reference.langium` imports after `common`.
//!
//! The common terminals no parser rule reaches (`BOOLEAN`, `FLOAT`, `NUMBER`, `ID`) are not
//! tokens, matching Langium's reachability filter. `YAML` and `DIRECTIVE` only match text that
//! preprocessing has not cleaned, so only `SINGLE_LINE_COMMENT` is reproduced from the hidden
//! rules.
//!
//! Every keyword token carries `LONGER_ALT` listing the terminals whose regex partially matches
//! it (`DefaultTokenBuilder.findLongerAlt`). For this grammar that is `REFERENCE` alone, and
//! only for the keywords that start with a word character. Chevrotain therefore prefers
//! `REFERENCE` over a keyword whenever it matches a longer run at the same offset, which is what
//! lets `branch branch01` name a branch `branch01` instead of reading a second `branch` keyword.
//!
//! The JavaScript regexes backtrack to satisfy their `(?!\.)` guards, so `12.0` tokenizes as
//! the `INT` `1` followed by the `REFERENCE` `2.0` rather than failing: the matchers below
//! shorten the match the same way.

use crate::error::{ParseError, Span};
use crate::preprocess::is_js_whitespace;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    /// The `gitGraph` keyword, which the token builder bounds.
    GitGraph,
    /// The single `gitGraph:` keyword literal.
    GitGraphColon,
    /// The `:` of the header.
    Colon,
    Lr,
    Tb,
    Bt,
    Commit,
    /// The `id:` keyword.
    IdColon,
    /// The `msg:` keyword.
    MsgColon,
    /// The `tag:` keyword.
    TagColon,
    /// The `type:` keyword.
    TypeColon,
    Normal,
    Reverse,
    Highlight,
    Branch,
    /// The `order:` keyword.
    OrderColon,
    Merge,
    Checkout,
    Switch,
    /// The `cherry-pick` keyword.
    CherryPick,
    /// The `parent:` keyword.
    ParentColon,
    /// `ACC_DESCR`.
    AccDescr,
    /// `ACC_TITLE`.
    AccTitle,
    /// `TITLE`.
    Title,
    /// `INT`.
    Int,
    /// `STRING`.
    Str,
    /// `NEWLINE`.
    Newline,
    /// `REFERENCE`.
    Reference,
    Eof,
}

impl TokenKind {
    /// The token name upstream's generated parser prints in an error.
    pub(crate) fn name(self) -> &'static str {
        match self {
            TokenKind::GitGraph => "gitGraph",
            TokenKind::GitGraphColon => "gitGraph:",
            TokenKind::Colon => ":",
            TokenKind::Lr => "LR",
            TokenKind::Tb => "TB",
            TokenKind::Bt => "BT",
            TokenKind::Commit => "commit",
            TokenKind::IdColon => "id:",
            TokenKind::MsgColon => "msg:",
            TokenKind::TagColon => "tag:",
            TokenKind::TypeColon => "type:",
            TokenKind::Normal => "NORMAL",
            TokenKind::Reverse => "REVERSE",
            TokenKind::Highlight => "HIGHLIGHT",
            TokenKind::Branch => "branch",
            TokenKind::OrderColon => "order:",
            TokenKind::Merge => "merge",
            TokenKind::Checkout => "checkout",
            TokenKind::Switch => "switch",
            TokenKind::CherryPick => "cherry-pick",
            TokenKind::ParentColon => "parent:",
            TokenKind::AccDescr => "ACC_DESCR",
            TokenKind::AccTitle => "ACC_TITLE",
            TokenKind::Title => "TITLE",
            TokenKind::Int => "INT",
            TokenKind::Str => "STRING",
            TokenKind::Newline => "NEWLINE",
            TokenKind::Reference => "REFERENCE",
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

/// The keyword tokens, longest first. Ties never share a prefix, so their relative order is
/// not observable; it follows the grammar.
const KEYWORDS: &[(&str, TokenKind)] = &[
    ("cherry-pick", TokenKind::CherryPick),
    ("gitGraph:", TokenKind::GitGraphColon),
    ("HIGHLIGHT", TokenKind::Highlight),
    ("gitGraph", TokenKind::GitGraph),
    ("checkout", TokenKind::Checkout),
    ("REVERSE", TokenKind::Reverse),
    ("parent:", TokenKind::ParentColon),
    ("commit", TokenKind::Commit),
    ("branch", TokenKind::Branch),
    ("switch", TokenKind::Switch),
    ("order:", TokenKind::OrderColon),
    ("NORMAL", TokenKind::Normal),
    ("merge", TokenKind::Merge),
    ("type:", TokenKind::TypeColon),
    ("msg:", TokenKind::MsgColon),
    ("tag:", TokenKind::TagColon),
    ("id:", TokenKind::IdColon),
    ("LR", TokenKind::Lr),
    ("TB", TokenKind::Tb),
    ("BT", TokenKind::Bt),
    (":", TokenKind::Colon),
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
            if let Some(end) = acc_descr(rest) {
                return Ok(self.finish(TokenKind::AccDescr, end));
            }
            if let Some(end) = acc_title(rest) {
                return Ok(self.finish(TokenKind::AccTitle, end));
            }
            if let Some(end) = title(rest) {
                return Ok(self.finish(TokenKind::Title, end));
            }
            if let Some(end) = int(rest) {
                return Ok(self.finish(TokenKind::Int, end));
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
            if let Some(end) = reference(rest) {
                return Ok(self.finish(TokenKind::Reference, end));
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

/// The keyword that matches at the start of `rest`, with `REFERENCE` preferred over it when it
/// matches a longer run (Chevrotain's `LONGER_ALT`).
fn keyword(rest: &str) -> Option<(TokenKind, usize)> {
    let longer_alt = reference(rest);
    for (word, kind) in KEYWORDS {
        if !rest.starts_with(word) {
            continue;
        }
        if *kind == TokenKind::GitGraph && !git_graph_boundary(&rest[word.len()..]) {
            continue;
        }
        return match longer_alt {
            Some(end) if end > word.len() => Some((TokenKind::Reference, end)),
            _ => Some((*kind, word.len())),
        };
    }
    None
}

/// `GitGraphTokenBuilder`'s suffix: `(?:(?=%%)|(?!\S))`.
fn git_graph_boundary(rest: &str) -> bool {
    match rest.chars().next() {
        None => true,
        Some(ch) => rest.starts_with("%%") || is_js_whitespace(ch),
    }
}

/// `INT: /0|[1-9][0-9]*(?!\.)/`, with the backtracking the guard allows. The `0` alternative
/// has no guard, so `0.5` is the `INT` `0` followed by an unexpected character.
fn int(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    let first = *bytes.first()?;
    if first == b'0' {
        return Some(1);
    }
    if !(b'1'..=b'9').contains(&first) {
        return None;
    }
    let mut end = 1;
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

/// `REFERENCE: /\w([-\./\w]*[-\w])?/`, backtracking so the group's last character is a word
/// character or a dash.
fn reference(rest: &str) -> Option<usize> {
    let (_, first) = rest.char_indices().next()?;
    if !is_word(first) {
        return None;
    }
    let mut end = first.len_utf8();
    for ch in rest[end..].chars() {
        if is_word(ch) || ch == '-' || ch == '.' || ch == '/' {
            end += ch.len_utf8();
        } else {
            break;
        }
    }
    while let Some(ch) = rest[..end].chars().next_back() {
        if ch == '.' || ch == '/' {
            end -= ch.len_utf8();
        } else {
            break;
        }
    }
    Some(end)
}

fn is_word(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
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

fn is_line_terminator(ch: char) -> bool {
    matches!(ch, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}
