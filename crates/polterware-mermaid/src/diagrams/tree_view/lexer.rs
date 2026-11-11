//! The treeView lexer: a port of the terminals `treeView.langium` declares, in the order
//! Langium's `DefaultTokenBuilder` puts them.
//!
//! The grammar imports nothing, so its own declaration order is the token order. Chevrotain
//! takes the first token pattern that matches at the current offset, not the longest, which
//! makes the order observable:
//!
//! 1. keywords: `treeView-beta`. `TreeViewTokenBuilder` rewrites its pattern to
//!    `treeView-beta(?:(?=%%)|(?!\S))`, so the keyword must be followed by `%%`, by
//!    whitespace, or by the end of input. Langium also gives the keyword the `BARE_NAME`
//!    terminal as its `LONGER_ALT`, and Chevrotain prefers an alternative that matches a
//!    strictly longer image: `treeView-beta wall` is therefore a name, not the keyword;
//! 2. terminals in grammar order: `ACC_DESCR`, `ACC_TITLE`, `TITLE`, `CLASS_ANNOTATION`,
//!    `ICON_ANNOTATION`, `DESC_ANNOTATION`, `INDENTATION`, `QUOTED_NAME`;
//! 3. hidden terminals: `WS`, `ML_COMMENT`, `NL`;
//! 4. `BARE_NAME`, declared last so that a `%%` comment line wins over a name.
//!
//! `WS` is unreachable in this order, because `INDENTATION` matches every run of spaces or
//! tabs before it. That matters: a run of spaces is always a visible `INDENTATION` token,
//! never skipped whitespace, which is how the indentation reaches the parser. Only `NL` and
//! `ML_COMMENT` are skipped here.
//!
//! The annotation terminals include their leading whitespace, and `BARE_NAME` stops before
//! one of them, so `index.js :::highlight` is a name and an annotation rather than one name.

use crate::error::{ParseError, Span};

/// The diagram's keyword (`TreeViewTokenBuilder`).
pub(crate) const KEYWORD: &str = "treeView-beta";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    /// The `treeView-beta` keyword.
    Keyword,
    /// `ACC_DESCR`.
    AccDescr,
    /// `ACC_TITLE`.
    AccTitle,
    /// `TITLE`.
    Title,
    /// `CLASS_ANNOTATION`: `:::class`.
    ClassAnnotation,
    /// `ICON_ANNOTATION`: `icon(name)`.
    IconAnnotation,
    /// `DESC_ANNOTATION`: `## description`.
    DescAnnotation,
    /// `INDENTATION`: one or more spaces or tabs.
    Indentation,
    /// `QUOTED_NAME`: `"name"` or `'name'`.
    QuotedName,
    /// `BARE_NAME`: an unquoted name.
    BareName,
    Eof,
}

impl TokenKind {
    /// The token name upstream's generated parser prints in an error.
    pub(crate) fn name(self) -> &'static str {
        match self {
            TokenKind::Keyword => KEYWORD,
            TokenKind::AccDescr => "ACC_DESCR",
            TokenKind::AccTitle => "ACC_TITLE",
            TokenKind::Title => "TITLE",
            TokenKind::ClassAnnotation => "CLASS_ANNOTATION",
            TokenKind::IconAnnotation => "ICON_ANNOTATION",
            TokenKind::DescAnnotation => "DESC_ANNOTATION",
            TokenKind::Indentation => "INDENTATION",
            TokenKind::QuotedName => "QUOTED_NAME",
            TokenKind::BareName => "BARE_NAME",
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
            if let Some(end) = keyword(rest) {
                // `LONGER_ALT: [BARE_NAME]`: Langium makes every terminal whose pattern can
                // match the keyword a longer alternative of it, so a strictly longer name
                // starting at the same offset wins over the keyword token.
                if let Some(name) = bare_name(rest)
                    && name > end
                {
                    return Ok(self.finish(TokenKind::BareName, name));
                }
                return Ok(self.finish(TokenKind::Keyword, end));
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
            if let Some(end) = class_annotation(rest) {
                return Ok(self.finish(TokenKind::ClassAnnotation, end));
            }
            if let Some(end) = icon_annotation(rest) {
                return Ok(self.finish(TokenKind::IconAnnotation, end));
            }
            if let Some(end) = desc_annotation(rest) {
                return Ok(self.finish(TokenKind::DescAnnotation, end));
            }
            if let Some(end) = indentation(rest) {
                return Ok(self.finish(TokenKind::Indentation, end));
            }
            if let Some(end) = quoted_name(rest) {
                return Ok(self.finish(TokenKind::QuotedName, end));
            }
            if let Some(end) = comment(rest) {
                self.advance(self.offset + end);
                continue;
            }
            if let Some(end) = newline(rest) {
                self.advance(self.offset + end);
                continue;
            }
            if let Some(end) = bare_name(rest) {
                return Ok(self.finish(TokenKind::BareName, end));
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

/// `treeView-beta(?:(?=%%)|(?!\S))`, the keyword boundary `TreeViewTokenBuilder` adds.
fn keyword(rest: &str) -> Option<usize> {
    let after = rest.strip_prefix(KEYWORD)?;
    let boundary = after.starts_with("%%")
        || after
            .chars()
            .next()
            .is_none_or(crate::preprocess::is_js_whitespace);
    boundary.then_some(KEYWORD.len())
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

/// `CLASS_ANNOTATION: /[ \t]+:::[ \t]*[A-Za-z_][\w-]*/`.
fn class_annotation(rest: &str) -> Option<usize> {
    let mut end = horizontal_space(rest)?;
    rest[end..].strip_prefix(":::")?;
    end += 3;
    end += horizontal_space(&rest[end..]).unwrap_or(0);
    let first = rest[end..].chars().next()?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    end += first.len_utf8();
    while let Some(ch) = rest[end..].chars().next() {
        if is_word_char(ch) || ch == '-' {
            end += ch.len_utf8();
        } else {
            break;
        }
    }
    Some(end)
}

/// `ICON_ANNOTATION: /[ \t]+icon\([\w-]*(?::[\w-]+)?\)/`.
fn icon_annotation(rest: &str) -> Option<usize> {
    let mut end = horizontal_space(rest)?;
    rest[end..].strip_prefix("icon(")?;
    end += "icon(".len();
    end = word_or_dash_end(rest, end);
    if rest[end..].starts_with(':') {
        let start = end + 1;
        let after = word_or_dash_end(rest, start);
        // `[\w-]+` needs at least one character after the colon.
        if after == start {
            return None;
        }
        end = after;
    }
    rest[end..].strip_prefix(')')?;
    Some(end + 1)
}

/// `DESC_ANNOTATION: /[ \t]+##[^\n\r]*/`.
fn desc_annotation(rest: &str) -> Option<usize> {
    let mut end = horizontal_space(rest)?;
    rest[end..].strip_prefix("##")?;
    end += 2;
    Some(end + line_end(&rest[end..]))
}

/// `INDENTATION: /[ \t]{1,}/`.
fn indentation(rest: &str) -> Option<usize> {
    horizontal_space(rest)
}

/// `QUOTED_NAME: /"[^"]*"|'[^']*'/`. There are no escapes: the name ends at the next quote
/// of the same kind, newlines included.
fn quoted_name(rest: &str) -> Option<usize> {
    let quote = rest.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let close = rest[quote.len_utf8()..].find(quote)?;
    Some(quote.len_utf8() + close + quote.len_utf8())
}

/// `BARE_NAME: /(?!:::|icon\(|##)[^ \t\n\r"'](?:(?![ \t]+:::[ \t]*[A-Za-z_]|
/// [ \t]+icon\(|[ \t]+##)[^\n\r])*/`.
fn bare_name(rest: &str) -> Option<usize> {
    if rest.starts_with(":::") || rest.starts_with("icon(") || rest.starts_with("##") {
        return None;
    }
    let first = rest.chars().next()?;
    if matches!(first, ' ' | '\t' | '\n' | '\r' | '"' | '\'') {
        return None;
    }
    let mut end = first.len_utf8();
    while !annotation_marker(&rest[end..]) {
        let Some(ch) = rest[end..].chars().next() else {
            break;
        };
        if ch == '\n' || ch == '\r' {
            break;
        }
        end += ch.len_utf8();
    }
    Some(end)
}

/// The `(?![ \t]+:::[ \t]*[A-Za-z_]|[ \t]+icon\(|[ \t]+##)` lookahead of `BARE_NAME`: the
/// skip the parser only performs when an annotation follows.
fn annotation_marker(text: &str) -> bool {
    let Some(spaces) = horizontal_space(text) else {
        return false;
    };
    let rest = &text[spaces..];
    if let Some(after_class) = rest.strip_prefix(":::") {
        let after_space = &after_class[horizontal_space(after_class).unwrap_or(0)..];
        return after_space
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_');
    }
    rest.starts_with("icon(") || rest.starts_with("##")
}

/// `ML_COMMENT: /\%\%[^\n]*/`, a hidden token.
fn comment(rest: &str) -> Option<usize> {
    rest.strip_prefix("%%")?;
    Some(rest.find(['\r', '\n']).unwrap_or(rest.len()))
}

/// `NL: /\r?\n/`, a hidden token.
fn newline(rest: &str) -> Option<usize> {
    if rest.starts_with("\r\n") {
        Some(2)
    } else if rest.starts_with('\n') {
        Some(1)
    } else {
        None
    }
}

/// `[ \t]+` at the start of `text`, as a byte length.
fn horizontal_space(text: &str) -> Option<usize> {
    let end = text
        .char_indices()
        .find(|&(_, ch)| ch != ' ' && ch != '\t')
        .map_or(text.len(), |(index, _)| index);
    (end > 0).then_some(end)
}

/// `[\w-]*` at `start`, as the byte offset after it.
fn word_or_dash_end(text: &str, start: usize) -> usize {
    let mut end = start;
    while let Some(ch) = text[end..].chars().next() {
        if is_word_char(ch) || ch == '-' {
            end += ch.len_utf8();
        } else {
            break;
        }
    }
    end
}

/// The length of `[^\n\r]*?(?=%%)|[^\n\r]*` at the start of `text`.
fn line_before_comment(text: &str) -> usize {
    let end = line_end(text);
    text[..end].find("%%").unwrap_or(end)
}

/// The length of `[^\n\r]*` at the start of `text`.
fn line_end(text: &str) -> usize {
    text.find(['\r', '\n']).unwrap_or(text.len())
}

/// The length of `\s*` at the start of `text`.
fn js_space(text: &str) -> usize {
    text.char_indices()
        .find(|&(_, ch)| !crate::preprocess::is_js_whitespace(ch))
        .map_or(text.len(), |(index, _)| index)
}

/// JavaScript's `\w`: `[A-Za-z0-9_]`.
fn is_word_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}
