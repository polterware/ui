//! The event modeling lexer: the terminals and keywords `event-modeling.langium` declares, in
//! the order Langium's `DefaultTokenBuilder` puts them.
//!
//! The order is observable, because Chevrotain takes the first token pattern that matches at
//! the current offset, not the longest:
//!
//! 1. keywords, sorted by descending length (stable, so equal lengths keep grammar order):
//!    `eventmodeling`, `resetframe`, `readmodel`, `processor`, `timeframe`, `command`, `entity`,
//!    `event`, `jsobj`, `figma`, `given`, `json`, `salt`, `html`, `text`, `data`, `note`,
//!    `when`, `then`, `rmo`, `cmd`, `evt`, `pcr`, `uri`, `->>`, `gwt`, `ui`, `md`, `tf`, `[[`,
//!    `]]`, `rf`, `` ` ``, `.`. `EventModelingTokenBuilder` restricts the keyword named
//!    `eventmodeling` to be followed by `%%`, whitespace, or the end of input;
//! 2. terminals in grammar order: `EM_ID`, `EM_FID`, `EM_DATA_INLINE`, `EM_DATA_BLOCK`,
//!    `EM_ACC_DESCR`, `EM_ACC_TITLE`, `EM_TITLE`, then the hidden `EM_WS` (skipped), `EM_YAML`,
//!    `EM_DIRECTIVE`, `EM_SINGLE_LINE_COMMENT`, `EM_ML_COMMENT`, and `EM_SL_COMMENT`.
//!
//! Every keyword that `EM_ID` fully matches gets `EM_ID` as its Chevrotain `LONGER_ALT`, so
//! `tfa` lexes as one `EM_ID` rather than `tf` and `a`, while a bare `tf` stays the keyword.
//! `EM_EID` and `EM_FI` are parser rules, not terminals, so the token names are `EM_ID` and
//! `EM_FID`.
//!
//! Two consequences of that order are visible in upstream and reproduced here: `EM_ID` comes
//! before the accessibility terminals, so `accTitle`, `accDescr`, and `title` only lex as their
//! own tokens when they start at the current offset (the header line); at the start of a line
//! the skipped `EM_WS` has already eaten the indentation and `EM_ID` wins. And the
//! accessibility terminals are not the rule names `AbstractMermaidValueConverter` looks for
//! (`ACC_TITLE`, not `EM_ACC_TITLE`), so their AST values keep the whole matched line.
//!
//! `EM_YAML` and `EM_DIRECTIVE` can only appear in text preprocessing has not cleaned, but
//! they are still tokens upstream, so they are skipped here too.

use crate::common::text::is_line_terminator;
use crate::error::{ParseError, Span};
use crate::preprocess::is_js_whitespace;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    /// The `eventmodeling` header keyword.
    Eventmodeling,
    Resetframe,
    Readmodel,
    Processor,
    Timeframe,
    Command,
    Entity,
    Event,
    Jsobj,
    Figma,
    Given,
    Json,
    Salt,
    Html,
    Text,
    Data,
    Note,
    When,
    Then,
    Rmo,
    Cmd,
    Evt,
    Pcr,
    Uri,
    /// The `->>` of a source frame reference.
    Arrow,
    Gwt,
    Ui,
    Md,
    Tf,
    /// The `[[` of a data reference.
    OpenBrackets,
    /// The `]]` of a data reference.
    CloseBrackets,
    Rf,
    /// The `` ` `` around a data type.
    Backtick,
    /// The `.` of a qualified name.
    Dot,
    /// `EM_ID`.
    Id,
    /// `EM_FID`.
    Fid,
    /// `EM_DATA_INLINE`.
    DataInline,
    /// `EM_DATA_BLOCK`.
    DataBlock,
    /// `EM_ACC_DESCR`.
    AccDescr,
    /// `EM_ACC_TITLE`.
    AccTitle,
    /// `EM_TITLE`.
    Title,
    Eof,
}

impl TokenKind {
    /// The token name upstream's generated parser prints in an error.
    pub(crate) fn name(self) -> &'static str {
        match self {
            TokenKind::Eventmodeling => "eventmodeling",
            TokenKind::Resetframe => "resetframe",
            TokenKind::Readmodel => "readmodel",
            TokenKind::Processor => "processor",
            TokenKind::Timeframe => "timeframe",
            TokenKind::Command => "command",
            TokenKind::Entity => "entity",
            TokenKind::Event => "event",
            TokenKind::Jsobj => "jsobj",
            TokenKind::Figma => "figma",
            TokenKind::Given => "given",
            TokenKind::Json => "json",
            TokenKind::Salt => "salt",
            TokenKind::Html => "html",
            TokenKind::Text => "text",
            TokenKind::Data => "data",
            TokenKind::Note => "note",
            TokenKind::When => "when",
            TokenKind::Then => "then",
            TokenKind::Rmo => "rmo",
            TokenKind::Cmd => "cmd",
            TokenKind::Evt => "evt",
            TokenKind::Pcr => "pcr",
            TokenKind::Uri => "uri",
            TokenKind::Arrow => "->>",
            TokenKind::Gwt => "gwt",
            TokenKind::Ui => "ui",
            TokenKind::Md => "md",
            TokenKind::Tf => "tf",
            TokenKind::OpenBrackets => "[[",
            TokenKind::CloseBrackets => "]]",
            TokenKind::Rf => "rf",
            TokenKind::Backtick => "`",
            TokenKind::Dot => ".",
            TokenKind::Id => "EM_ID",
            TokenKind::Fid => "EM_FID",
            TokenKind::DataInline => "EM_DATA_INLINE",
            TokenKind::DataBlock => "EM_DATA_BLOCK",
            TokenKind::AccDescr => "EM_ACC_DESCR",
            TokenKind::AccTitle => "EM_ACC_TITLE",
            TokenKind::Title => "EM_TITLE",
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
    ("eventmodeling", TokenKind::Eventmodeling),
    ("resetframe", TokenKind::Resetframe),
    ("readmodel", TokenKind::Readmodel),
    ("processor", TokenKind::Processor),
    ("timeframe", TokenKind::Timeframe),
    ("command", TokenKind::Command),
    ("entity", TokenKind::Entity),
    ("event", TokenKind::Event),
    ("jsobj", TokenKind::Jsobj),
    ("figma", TokenKind::Figma),
    ("given", TokenKind::Given),
    ("json", TokenKind::Json),
    ("salt", TokenKind::Salt),
    ("html", TokenKind::Html),
    ("text", TokenKind::Text),
    ("data", TokenKind::Data),
    ("note", TokenKind::Note),
    ("when", TokenKind::When),
    ("then", TokenKind::Then),
    ("rmo", TokenKind::Rmo),
    ("cmd", TokenKind::Cmd),
    ("evt", TokenKind::Evt),
    ("pcr", TokenKind::Pcr),
    ("uri", TokenKind::Uri),
    ("->>", TokenKind::Arrow),
    ("gwt", TokenKind::Gwt),
    ("ui", TokenKind::Ui),
    ("md", TokenKind::Md),
    ("tf", TokenKind::Tf),
    ("[[", TokenKind::OpenBrackets),
    ("]]", TokenKind::CloseBrackets),
    ("rf", TokenKind::Rf),
    ("`", TokenKind::Backtick),
    (".", TokenKind::Dot),
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
            if let Some(end) = id(rest) {
                return Ok(self.finish(TokenKind::Id, end));
            }
            if let Some(end) = fid(rest) {
                return Ok(self.finish(TokenKind::Fid, end));
            }
            if let Some(end) = data_inline(rest) {
                return Ok(self.finish(TokenKind::DataInline, end));
            }
            if let Some(end) = data_block(rest) {
                return Ok(self.finish(TokenKind::DataBlock, end));
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
            if let Some(end) = whitespace(rest) {
                self.advance(self.offset + end);
                continue;
            }
            if let Some(end) = yaml(rest) {
                self.advance(self.offset + end);
                continue;
            }
            if let Some(end) = directive(rest) {
                self.advance(self.offset + end);
                continue;
            }
            if let Some(end) = single_line_comment(rest) {
                self.advance(self.offset + end);
                continue;
            }
            if let Some(end) = multi_line_comment(rest) {
                self.advance(self.offset + end);
                continue;
            }
            if let Some(end) = slash_line_comment(rest) {
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

/// The first keyword that matches, with its `LONGER_ALT` `EM_ID` applied.
fn keyword(rest: &str) -> Option<(TokenKind, usize)> {
    for (word, kind) in KEYWORDS {
        if !rest.starts_with(word) {
            continue;
        }
        if *kind == TokenKind::Eventmodeling && !keyword_boundary(&rest[word.len()..]) {
            continue;
        }
        // `findLongerAlt`: an `EM_ID` that matches the whole keyword is a longer alternative.
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

/// `EventModelingTokenBuilder`: the keyword followed by `%%`, whitespace, or nothing.
fn keyword_boundary(after: &str) -> bool {
    after.starts_with("%%") || after.chars().next().is_none_or(is_js_whitespace)
}

/// `EM_ID: /[_a-zA-Z][\w_]*/`, with JavaScript's ASCII `\w`.
fn id(rest: &str) -> Option<usize> {
    let mut bytes = rest.bytes();
    let first = bytes.next()?;
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return None;
    }
    let mut end = 1;
    for byte in bytes {
        if byte.is_ascii_alphanumeric() || byte == b'_' {
            end += 1;
        } else {
            break;
        }
    }
    Some(end)
}

/// `EM_FID: /\d{1,3}/`.
fn fid(rest: &str) -> Option<usize> {
    let end = rest.bytes().take(3).take_while(u8::is_ascii_digit).count();
    (end > 0).then_some(end)
}

/// `EM_DATA_INLINE: /\{(.*)\}|"(.*)"|'(.*)'/`.
///
/// The `.*` is greedy and does not cross a line terminator, so the match ends at the last
/// closing brace or quote on the line.
fn data_inline(rest: &str) -> Option<usize> {
    let close = match rest.chars().next()? {
        '{' => '}',
        '"' => '"',
        '\'' => '\'',
        _ => return None,
    };
    let line_end = rest
        .char_indices()
        .find(|&(_, ch)| is_line_terminator(ch))
        .map_or(rest.len(), |(index, _)| index);
    let last = rest[..line_end].rfind(close)?;
    (last > 0).then_some(last + close.len_utf8())
}

/// `EM_DATA_BLOCK: /\{[\t ]*\r?\n(?:[\S\s]*?\r?\n)?\}(?:\r?\n|(?!\S))/`.
///
/// The optional group is preferred over skipping it, and its body is lazy, so the block ends
/// at the first `}` that starts a line after the opening one; only when no later line starts
/// with `}` does the group step aside for a `}` on the line right after the opening brace.
fn data_block(rest: &str) -> Option<usize> {
    let after = rest.strip_prefix('{')?;
    let mut index = 1 + horizontal_space(after).unwrap_or(0);
    index += newline_len(&rest[index..])?;
    let close = first_line_start(&rest[index..], "}")
        .map(|offset| index + offset)
        .or_else(|| rest[index..].starts_with('}').then_some(index))?;
    let end = close + 1;
    line_end_after(rest, end)
}

/// `EM_ACC_DESCR: /[\t ]*accDescr(?:[\t ]*:([^\n\r]*?(?=%%)|[^\n\r]*)|\s*{([^}]*)})/`.
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

/// `EM_ACC_TITLE: /[\t ]*accTitle[\t ]*:(?:[^\n\r]*?(?=%%)|[^\n\r]*)/`.
fn acc_title(rest: &str) -> Option<usize> {
    let mut start = horizontal_space(rest).unwrap_or(0);
    rest[start..].strip_prefix("accTitle")?;
    start += "accTitle".len();
    start += horizontal_space(&rest[start..]).unwrap_or(0);
    rest[start..].strip_prefix(':')?;
    start += 1;
    Some(start + line_before_comment(&rest[start..]))
}

/// `EM_TITLE: /[\t ]*title(?:[\t ][^\n\r]*?(?=%%)|[\t ][^\n\r]*|)/`.
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

/// `EM_WS: /\s+/`, a skipped hidden token.
fn whitespace(rest: &str) -> Option<usize> {
    let end = rest
        .char_indices()
        .find(|&(_, ch)| !is_js_whitespace(ch))
        .map_or(rest.len(), |(index, _)| index);
    (end > 0).then_some(end)
}

/// `EM_YAML: /---[\t ]*\r?\n(?:[\S\s]*?\r?\n)?---(?:\r?\n|(?!\S))/`, a hidden token.
fn yaml(rest: &str) -> Option<usize> {
    let after = rest.strip_prefix("---")?;
    let mut index = 3 + horizontal_space(after).unwrap_or(0);
    index += newline_len(&rest[index..])?;
    let close = first_line_start(&rest[index..], "---")
        .map(|offset| index + offset)
        .or_else(|| rest[index..].starts_with("---").then_some(index))?;
    let end = close + 3;
    line_end_after(rest, end)
}

/// `EM_DIRECTIVE: /[\t ]*%%{[\S\s]*?}%%(?:\r?\n|(?!\S))/`, a hidden token.
fn directive(rest: &str) -> Option<usize> {
    let mut start = horizontal_space(rest).unwrap_or(0);
    rest[start..].strip_prefix("%%{")?;
    start += 3;
    let close = start + rest[start..].find("}%%")?;
    let end = close + 3;
    line_end_after(rest, end)
}

/// `EM_SINGLE_LINE_COMMENT: /[\t ]*%%[^\n\r]*/`, a hidden token.
fn single_line_comment(rest: &str) -> Option<usize> {
    let start = horizontal_space(rest).unwrap_or(0);
    rest[start..].strip_prefix("%%")?;
    let end = rest[start + 2..]
        .find(['\r', '\n'])
        .map_or(rest.len(), |offset| start + 2 + offset);
    Some(end)
}

/// `EM_ML_COMMENT: /\/\*[\s\S]*?\*\//`, a hidden token.
fn multi_line_comment(rest: &str) -> Option<usize> {
    rest.strip_prefix("/*")?;
    let close = rest[2..].find("*/")?;
    Some(2 + close + 2)
}

/// `EM_SL_COMMENT: /\/\/[^\n\r]*/`, a hidden token.
fn slash_line_comment(rest: &str) -> Option<usize> {
    rest.strip_prefix("//")?;
    Some(rest.find(['\r', '\n']).unwrap_or(rest.len()))
}

/// The length of `\r?\n` at the start of `text`.
fn newline_len(text: &str) -> Option<usize> {
    if text.starts_with("\r\n") {
        Some(2)
    } else if text.starts_with('\n') {
        Some(1)
    } else {
        None
    }
}

/// The offset of the start of the first line in `text` that begins with `prefix`.
///
/// This is where the lazy `[\S\s]*?\r?\n` of a block or YAML body stops: the newline may be
/// preceded by a `\r`, which the lazy body absorbs, so only the `\n` matters.
fn first_line_start(text: &str, prefix: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(offset) = text[from..].find('\n') {
        let line_start = from + offset + 1;
        if text[line_start..].starts_with(prefix) {
            return Some(line_start);
        }
        from = line_start;
    }
    None
}

/// The trailing `(?:\r?\n|(?!\S))` of a block terminal.
fn line_end_after(rest: &str, from: usize) -> Option<usize> {
    if let Some(length) = newline_len(&rest[from..]) {
        return Some(from + length);
    }
    match rest[from..].chars().next() {
        None => Some(from),
        Some(ch) if is_js_whitespace(ch) => Some(from),
        Some(_) => None,
    }
}

/// The length of `[^\n\r]*?(?=%%)|[^\n\r]*` at the start of `text`.
fn line_before_comment(text: &str) -> usize {
    let end = text.find(['\r', '\n']).unwrap_or(text.len());
    text[..end].find("%%").unwrap_or(end)
}

/// The length of `[\t ]*` at the start of `text`.
fn horizontal_space(text: &str) -> Option<usize> {
    let end = text
        .char_indices()
        .find(|&(_, ch)| ch != ' ' && ch != '\t')
        .map_or(text.len(), |(index, _)| index);
    (end > 0).then_some(end)
}

/// The length of `\s*` at the start of `text`.
fn js_space(text: &str) -> usize {
    text.char_indices()
        .find(|&(_, ch)| !is_js_whitespace(ch))
        .map_or(text.len(), |(index, _)| index)
}
