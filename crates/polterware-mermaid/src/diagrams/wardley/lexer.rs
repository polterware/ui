//! The wardley lexer: the keyword literals and terminals `wardley.langium` declares, with
//! the `EOL`, `TitleAndAccessibilities`, and common terminals it imports from
//! `packages/parser/src/language/common/common.langium`, in the order Langium's
//! `DefaultTokenBuilder` puts them.
//!
//! The order is observable, because Chevrotain takes the first token pattern that matches at
//! the current offset, not the longest:
//!
//! 1. keyword literals, sorted by descending length (stable, so equal lengths keep grammar
//!    order, which for every one of them is length 1): `[`, `,`, `]`, `@`, `/`, `(`, `)`,
//!    `-`, `{`, `}`;
//! 2. terminals in the merged grammar's order: `WARDLEY_NUMBER`, `ARROW`, `LINK_PORT`,
//!    `LINK_ARROW`, `LINK_LABEL`, `STRATEGY`, `KW_WARDLEY`, `KW_SIZE`, `KW_EVOLUTION`,
//!    `KW_ANCHOR`, `KW_COMPONENT`, `KW_LABEL`, `KW_INERTIA`, `KW_EVOLVE`, `KW_PIPELINE`,
//!    `KW_NOTE`, `KW_ANNOTATIONS`, `KW_ANNOTATION`, `KW_ACCELERATOR`, `KW_DEACCELERATOR`,
//!    `NAME_WITH_SPACES`, then the reachable common terminals `ACC_DESCR`, `ACC_TITLE`,
//!    `TITLE`, `INT`, `STRING`, `ID`, `NEWLINE`.
//!
//! `WS` and `WHITESPACE` (`[\t ]+`) are skipped, and `SINGLE_LINE_COMMENT` (`%%` to the end of
//! the line) is a hidden token; both are handled before the matchers below, which is
//! equivalent because no earlier token can start with a space, tab, or `%`. `YAML` and
//! `DIRECTIVE` can only appear in text preprocessing has not cleaned, and the body this crate
//! parses comes from [`crate::preprocess`], so they are not reproduced. `BOOLEAN`, `FLOAT`,
//! and `NUMBER` are reachable in the generated lexer but can never win: `NAME_WITH_SPACES`
//! matches `true`/`false`, `WARDLEY_NUMBER` matches every float, and `INT` matches every
//! integer first.
//!
//! Every keyword terminal is a prefix match with no word boundary, exactly as the generated
//! lexer has it: `sizeof` starts with `KW_SIZE` followed by a name, and a component named
//! `note` tokenizes as `KW_NOTE`. `STRATEGY` comes before the keywords, so `build` is a
//! strategy wherever it appears. `INT` comes before `STRING` and `ID`, so a name that starts
//! with a digit lexes as a number and cannot be a name. `DefaultTokenBuilder.findLongerAlt`
//! gives `-` the terminals that can match it partially, so `->`, `-->`, and `-.->` win over
//! the `-` keyword; that is the only keyword whose longer alternative matters here.
//!
//! The JavaScript regexes are matched by hand, including the backtracking the `INT` lookahead
//! and the `NAME_WITH_SPACES` repetitions allow.

use crate::common::text::{is_line_terminator, newline};
use crate::error::{ParseError, Span};
use crate::preprocess::is_js_whitespace;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    /// The `[` of a coordinate list.
    LBracket,
    /// The `,` of a coordinate list.
    Comma,
    /// The `]` of a coordinate list.
    RBracket,
    /// The `@` of an evolution stage boundary.
    At,
    /// The `/` of a dual-label evolution stage.
    Slash,
    /// The `(` before a decorator or `inertia`.
    LParen,
    /// The `)` after a decorator or `inertia`.
    RParen,
    /// The `-` of a negative label offset.
    Minus,
    /// The `{` of a pipeline block.
    LBrace,
    /// The `}` of a pipeline block.
    RBrace,
    /// `WARDLEY_NUMBER`.
    WardleyNumber,
    /// `ARROW` (`->`).
    Arrow,
    /// `LINK_PORT` (`+<>`, `+>`, `+<`).
    LinkPort,
    /// `LINK_ARROW` (`-->`, `-.->`, `>`, `+'label'<>`, `+'label'<`, `+'label'>`).
    LinkArrow,
    /// `LINK_LABEL` (`;` and the rest of the line).
    LinkLabel,
    /// `STRATEGY` (`build`, `buy`, `outsource`, `market`).
    Strategy,
    /// `KW_WARDLEY` (`wardley-beta`).
    KwWardley,
    /// `KW_SIZE`.
    KwSize,
    /// `KW_EVOLUTION`.
    KwEvolution,
    /// `KW_ANCHOR`.
    KwAnchor,
    /// `KW_COMPONENT`.
    KwComponent,
    /// `KW_LABEL`.
    KwLabel,
    /// `KW_INERTIA`.
    KwInertia,
    /// `KW_EVOLVE`.
    KwEvolve,
    /// `KW_PIPELINE`.
    KwPipeline,
    /// `KW_NOTE`.
    KwNote,
    /// `KW_ANNOTATIONS`.
    KwAnnotations,
    /// `KW_ANNOTATION`.
    KwAnnotation,
    /// `KW_ACCELERATOR`.
    KwAccelerator,
    /// `KW_DEACCELERATOR`.
    KwDeaccelerator,
    /// `NAME_WITH_SPACES`.
    NameWithSpaces,
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
            TokenKind::LBracket => "[",
            TokenKind::Comma => ",",
            TokenKind::RBracket => "]",
            TokenKind::At => "@",
            TokenKind::Slash => "/",
            TokenKind::LParen => "(",
            TokenKind::RParen => ")",
            TokenKind::Minus => "-",
            TokenKind::LBrace => "{",
            TokenKind::RBrace => "}",
            TokenKind::WardleyNumber => "WARDLEY_NUMBER",
            TokenKind::Arrow => "ARROW",
            TokenKind::LinkPort => "LINK_PORT",
            TokenKind::LinkArrow => "LINK_ARROW",
            TokenKind::LinkLabel => "LINK_LABEL",
            TokenKind::Strategy => "STRATEGY",
            TokenKind::KwWardley => "KW_WARDLEY",
            TokenKind::KwSize => "KW_SIZE",
            TokenKind::KwEvolution => "KW_EVOLUTION",
            TokenKind::KwAnchor => "KW_ANCHOR",
            TokenKind::KwComponent => "KW_COMPONENT",
            TokenKind::KwLabel => "KW_LABEL",
            TokenKind::KwInertia => "KW_INERTIA",
            TokenKind::KwEvolve => "KW_EVOLVE",
            TokenKind::KwPipeline => "KW_PIPELINE",
            TokenKind::KwNote => "KW_NOTE",
            TokenKind::KwAnnotations => "KW_ANNOTATIONS",
            TokenKind::KwAnnotation => "KW_ANNOTATION",
            TokenKind::KwAccelerator => "KW_ACCELERATOR",
            TokenKind::KwDeaccelerator => "KW_DEACCELERATOR",
            TokenKind::NameWithSpaces => "NAME_WITH_SPACES",
            TokenKind::AccDescr => "ACC_DESCR",
            TokenKind::AccTitle => "ACC_TITLE",
            TokenKind::Title => "TITLE",
            TokenKind::Int => "INT",
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
            if let Some(end) = horizontal_space(rest) {
                self.advance(self.offset + end);
                continue;
            }
            if let Some(end) = comment(rest) {
                self.advance(self.offset + end);
                continue;
            }
            for &(literal, kind) in KEYWORD_LITERALS {
                if !rest.starts_with(literal) {
                    continue;
                }
                if kind == TokenKind::Minus
                    && let Some((longer, end)) = minus_longer_alt(rest)
                {
                    return Ok(self.finish(longer, end));
                }
                return Ok(self.finish(kind, literal.len()));
            }
            if let Some(end) = wardley_number(rest) {
                return Ok(self.finish(TokenKind::WardleyNumber, end));
            }
            if rest.starts_with("->") {
                return Ok(self.finish(TokenKind::Arrow, 2));
            }
            if let Some(end) = link_port(rest) {
                return Ok(self.finish(TokenKind::LinkPort, end));
            }
            if let Some(end) = link_arrow(rest) {
                return Ok(self.finish(TokenKind::LinkArrow, end));
            }
            if let Some(end) = link_label(rest) {
                return Ok(self.finish(TokenKind::LinkLabel, end));
            }
            if let Some(end) = strategy(rest) {
                return Ok(self.finish(TokenKind::Strategy, end));
            }
            for &(prefix, kind) in KEYWORD_TERMINALS {
                if rest.starts_with(prefix) {
                    return Ok(self.finish(kind, prefix.len()));
                }
            }
            if let Some(end) = name_with_spaces(rest) {
                return Ok(self.finish(TokenKind::NameWithSpaces, end));
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
            if let Some(end) = id(rest) {
                return Ok(self.finish(TokenKind::Id, end));
            }
            if let Some(end) = newline(rest) {
                return Ok(self.finish(TokenKind::Newline, end));
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

/// The keyword literals of the parser rules, in the order the distinct keywords are first
/// seen (all are one character long, so the descending-length sort keeps that order).
const KEYWORD_LITERALS: &[(&str, TokenKind)] = &[
    ("[", TokenKind::LBracket),
    (",", TokenKind::Comma),
    ("]", TokenKind::RBracket),
    ("@", TokenKind::At),
    ("/", TokenKind::Slash),
    ("(", TokenKind::LParen),
    (")", TokenKind::RParen),
    ("-", TokenKind::Minus),
    ("{", TokenKind::LBrace),
    ("}", TokenKind::RBrace),
];

/// The keyword terminals of `wardley.langium`, in grammar order.
const KEYWORD_TERMINALS: &[(&str, TokenKind)] = &[
    ("wardley-beta", TokenKind::KwWardley),
    ("size", TokenKind::KwSize),
    ("evolution", TokenKind::KwEvolution),
    ("anchor", TokenKind::KwAnchor),
    ("component", TokenKind::KwComponent),
    ("label", TokenKind::KwLabel),
    ("inertia", TokenKind::KwInertia),
    ("evolve", TokenKind::KwEvolve),
    ("pipeline", TokenKind::KwPipeline),
    ("note", TokenKind::KwNote),
    ("annotations", TokenKind::KwAnnotations),
    ("annotation", TokenKind::KwAnnotation),
    ("accelerator", TokenKind::KwAccelerator),
    ("deaccelerator", TokenKind::KwDeaccelerator),
];

/// `'->'`, `'-->'`, or `'-.->'` after the `-` keyword matched. These are the first
/// terminals that can match a keyword partially and longer.
fn minus_longer_alt(rest: &str) -> Option<(TokenKind, usize)> {
    if rest.starts_with("->") {
        return Some((TokenKind::Arrow, 2));
    }
    link_arrow(rest).map(|end| (TokenKind::LinkArrow, end))
}

/// `WARDLEY_NUMBER: /[0-9]+\.[0-9]+/`.
fn wardley_number(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    let mut end = 0;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    if end == 0 || bytes.get(end) != Some(&b'.') {
        return None;
    }
    let fraction_start = end + 1;
    end = fraction_start;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    (end > fraction_start).then_some(end)
}

/// `LINK_PORT: '+<>' | '+>' | '+<'`.
fn link_port(rest: &str) -> Option<usize> {
    if rest.starts_with("+<>") {
        Some(3)
    } else if rest.starts_with("+>") || rest.starts_with("+<") {
        Some(2)
    } else {
        None
    }
}

/// `LINK_ARROW: '-->' | '-.->' | '>' | /\+'[^']*'<>/ | /\+'[^']*'</ | /\+'[^']*'>/`.
fn link_arrow(rest: &str) -> Option<usize> {
    if rest.starts_with("-->") {
        return Some(3);
    }
    if rest.starts_with("-.->") {
        return Some(4);
    }
    if rest.starts_with('>') {
        return Some(1);
    }
    let after = rest.strip_prefix("+'")?;
    let close = after.find('\'')?;
    let tail = &after[close + 1..];
    let tail_len = if tail.starts_with("<>") {
        2
    } else if tail.starts_with('<') || tail.starts_with('>') {
        1
    } else {
        return None;
    };
    // `"+-"`, `'`, the label, `'`, and the tail.
    Some(2 + close + 1 + tail_len)
}

/// `LINK_LABEL: /;[^\n\r]+/`.
fn link_label(rest: &str) -> Option<usize> {
    let after = rest.strip_prefix(';')?;
    let end = after.find(['\r', '\n']).unwrap_or(after.len());
    (end > 0).then_some(end + 1)
}

/// `STRATEGY: 'build' | 'buy' | 'outsource' | 'market'`.
fn strategy(rest: &str) -> Option<usize> {
    ["build", "buy", "outsource", "market"]
        .iter()
        .find_map(|word| rest.starts_with(word).then_some(word.len()))
}

/// `NAME_WITH_SPACES`:
/// `/(?!title\s|accTitle|accDescr)[A-Za-z](?:[A-Za-z0-9_()&]|-(?!>))*(?:[ \t]+[A-Za-z(](?:[A-Za-z0-9_()&]|-(?!>))*)*/`.
fn name_with_spaces(rest: &str) -> Option<usize> {
    if rest.starts_with("title")
        && rest["title".len()..]
            .chars()
            .next()
            .is_some_and(is_js_whitespace)
    {
        return None;
    }
    if rest.starts_with("accTitle") || rest.starts_with("accDescr") {
        return None;
    }
    if !rest.starts_with(|ch: char| ch.is_ascii_alphabetic()) {
        return None;
    }
    let mut end = 1;
    loop {
        while let Some(ch) = char_at(rest, end) {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '(' | ')' | '&') {
                end += ch.len_utf8();
            } else if ch == '-' && char_at(rest, end + 1) != Some('>') {
                end += 1;
            } else {
                break;
            }
        }
        let mut probe = end;
        while matches!(char_at(rest, probe), Some(' ' | '\t')) {
            probe += 1;
        }
        match char_at(rest, probe) {
            Some(ch) if probe > end && (ch.is_ascii_alphabetic() || ch == '(') => {
                end = probe + ch.len_utf8();
            }
            _ => break,
        }
    }
    Some(end)
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
    if matches!(rest[start..].chars().next(), Some(' ' | '\t')) {
        start += 1;
        start += line_before_comment(&rest[start..]);
    }
    Some(start)
}

/// `INT: /0|[1-9][0-9]*(?!\.)/`, with the backtracking the lookahead allows.
fn int(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    match bytes.first()? {
        b'0' => Some(1),
        b'1'..=b'9' => {
            let mut end = 1;
            while bytes.get(end).is_some_and(u8::is_ascii_digit) {
                end += 1;
            }
            if bytes.get(end) == Some(&b'.') {
                (end > 1).then_some(end - 1)
            } else {
                Some(end)
            }
        }
        _ => None,
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

/// `ID: /[\w]([-\w]*\w)?/`, where JavaScript's `\w` is ASCII.
fn id(rest: &str) -> Option<usize> {
    if !rest.starts_with(|ch: char| ch.is_ascii_alphanumeric() || ch == '_') {
        return None;
    }
    let mut end = 1;
    while let Some(ch) = char_at(rest, end) {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            end += ch.len_utf8();
        } else {
            break;
        }
    }
    while end > 1 && char_at(rest, end - 1) == Some('-') {
        end -= 1;
    }
    Some(end)
}

/// `WHITESPACE` and `WS`: `[\t ]+`, a skipped hidden token.
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
        .find(|&(_, ch)| !is_js_whitespace(ch))
        .map_or(text.len(), |(index, _)| index)
}

fn char_at(text: &str, offset: usize) -> Option<char> {
    if offset < text.len() {
        text[offset..].chars().next()
    } else {
        None
    }
}
