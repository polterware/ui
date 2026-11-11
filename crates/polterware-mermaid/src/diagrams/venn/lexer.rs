//! The venn lexer: a hand port of the `%lex` section of upstream `parser/venn.jison`.
//!
//! The grammar declares one start condition, `bol`. It is inclusive (jison's `%s`), so the
//! unprefixed rules stay active in it, and rule order is the file's order because jison picks
//! the *first* rule that matches at the current offset, not the longest. `%options
//! case-insensitive` applies to every rule, keywords and lookaheads included.
//!
//! In order, the rules are:
//!
//! 1. the two `%%` comment rules, active in both conditions: `%%(?!\{)[^\n]*` and
//!    `[^\}]%%[^\n]*` (the second consumes one character before the `%%`, which is visible at
//!    the start of a line: `A%%x` is a comment and does not leave the `bol` condition);
//! 2. in `bol`, `[ \t]+(?=[\n\r])` (a whitespace-only line), `[ \t]+(?=text\b)` (indentation
//!    before an indented text statement), `[ \t]+` (any other indentation), and `[^ \t\n\r]`
//!    (any other line start, which turns indent mode off and is re-read in `INITIAL`);
//! 3. `[\n\r]+` — one `NEWLINE` token for a whole run of line breaks, then back to `bol`;
//! 4. `%%[^\n]*` (the `%%{...}` form rule 1 excludes), `[ \t]+`, end of input;
//! 5. the keywords `title`, `venn-beta`, `set`, `union`, `text`, `style`;
//! 6. bracket labels, numeric and colour literals, identifiers, strings, comma, colon.
//!
//! The `bol` rules are what lets an indented `text` line attach to the previous `set` or
//! `union`. The grammar's `yy.setIndentMode(true)` (reproduced by [`Lexer::set_indent_mode`])
//! makes the leading whitespace of a line whose first word is `text` an `INDENT_TEXT` token;
//! the following `text` keyword is then swallowed. A line that starts with anything else at
//! column zero turns the mode off again. Like upstream, consecutive indented `text` lines work
//! because indent mode survives a line break, and an indented `set`/`style` still starts a
//! normal statement because only a `text` keyword is special.

use crate::error::{ParseError, Span};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    /// `VENN`, the `venn-beta` header keyword.
    Venn,
    /// `TITLE`, `title` plus the rest of its line.
    Title,
    /// `SET`.
    Set,
    /// `UNION`.
    Union,
    /// `TEXT`.
    Text,
    /// `INDENT_TEXT`, the indentation before an indented `text` line.
    IndentText,
    /// `STYLE`.
    Style,
    /// `BRACKET_LABEL`, a `["..."]` or `[...]` label.
    BracketLabel,
    /// `NUMERIC`.
    Numeric,
    /// `HEXCOLOR`.
    HexColor,
    /// `RGBACOLOR`.
    RgbaColor,
    /// `RGBCOLOR`.
    RgbColor,
    /// `IDENTIFIER`.
    Identifier,
    /// `STRING`, quotes included.
    Str,
    /// `COMMA`.
    Comma,
    /// `COLON`.
    Colon,
    /// `NEWLINE`.
    Newline,
    Eof,
}

impl TokenKind {
    /// The token name upstream's generated parser prints in an error.
    pub(crate) fn name(self) -> &'static str {
        match self {
            TokenKind::Venn => "VENN",
            TokenKind::Title => "TITLE",
            TokenKind::Set => "SET",
            TokenKind::Union => "UNION",
            TokenKind::Text => "TEXT",
            TokenKind::IndentText => "INDENT_TEXT",
            TokenKind::Style => "STYLE",
            TokenKind::BracketLabel => "BRACKET_LABEL",
            TokenKind::Numeric => "NUMERIC",
            TokenKind::HexColor => "HEXCOLOR",
            TokenKind::RgbaColor => "RGBACOLOR",
            TokenKind::RgbColor => "RGBCOLOR",
            TokenKind::Identifier => "IDENTIFIER",
            TokenKind::Str => "STRING",
            TokenKind::Comma => "COMMA",
            TokenKind::Colon => "COLON",
            TokenKind::Newline => "NEWLINE",
            TokenKind::Eof => "EOF",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Token {
    pub(crate) kind: TokenKind,
    /// The matched text, with the bracket label's delimiters trimmed as the jison actions do.
    /// Empty for [`TokenKind::Eof`].
    pub(crate) text: String,
    pub(crate) span: Span,
}

pub(crate) struct Lexer<'a> {
    source: &'a str,
    offset: usize,
    line: u32,
    column: u32,
    /// Whether the last token was a line break: jison's `bol` start condition.
    bol: bool,
    /// `yy.getIndentMode()`.
    indent_mode: bool,
    /// Set when an `INDENT_TEXT` token was returned, so the `text` keyword that follows it is
    /// swallowed instead of becoming a `TEXT` token.
    consume_indent_text: bool,
}

impl<'a> Lexer<'a> {
    pub(crate) fn new(source: &'a str) -> Self {
        Self {
            source,
            offset: 0,
            line: 1,
            column: 1,
            bol: false,
            indent_mode: false,
            consume_indent_text: false,
        }
    }

    /// `yy.setIndentMode(enabled)`, which the grammar calls at the end of `set` and `union`.
    pub(crate) fn set_indent_mode(&mut self, enabled: bool) {
        self.indent_mode = enabled;
    }

    pub(crate) fn next_token(&mut self) -> Result<Token, ParseError> {
        loop {
            if self.offset >= self.source.len() {
                return Ok(self.eof_token());
            }
            // The two comment rules are first in the file and active in every condition.
            if let Some(end) = self.comment_end() {
                self.advance(end);
                continue;
            }
            if self.bol {
                let rest = &self.source[self.offset..];
                if let Some(space) = horizontal_space(rest) {
                    let after = &rest[space..];
                    if after.starts_with(['\n', '\r']) {
                        // `<bol>[ \t]+(?=[\n\r])`: a whitespace-only line.
                        self.advance(self.offset + space);
                        continue;
                    }
                    if starts_with_word(after, "text") && self.indent_mode {
                        // `<bol>[ \t]+(?=text\b)` with indent mode on: the indentation is an
                        // INDENT_TEXT token, and the `text` keyword is swallowed next.
                        self.consume_indent_text = true;
                        self.bol = false;
                        return Ok(self.finish(TokenKind::IndentText, self.offset + space));
                    }
                    // `<bol>[ \t]+`: any other indentation.
                    self.advance(self.offset + space);
                    continue;
                }
                let ch = rest.chars().next().expect("non-empty remainder");
                if ch != '\n' && ch != '\r' {
                    // `<bol>[^ \t\n\r]`: the line starts a statement, so indent mode ends.
                    self.indent_mode = false;
                    self.bol = false;
                    continue;
                }
            }

            let rest = &self.source[self.offset..];
            if let Some(length) = newline_run(rest) {
                self.bol = true;
                return Ok(self.finish(TokenKind::Newline, self.offset + length));
            }
            if rest.starts_with("%%") {
                // `\%\%[^\n]*`, the third comment rule: what rule 1 excludes is `%%{`.
                let end = self.offset + line_length(rest);
                self.advance(end);
                continue;
            }
            if let Some(length) = horizontal_space(rest) {
                self.advance(self.offset + length);
                continue;
            }

            if let Some(length) = title_length(rest) {
                return Ok(self.finish(TokenKind::Title, self.offset + length));
            }
            if starts_with_keyword(rest, "venn-beta") {
                return Ok(self.finish(TokenKind::Venn, self.offset + "venn-beta".len()));
            }
            if starts_with_keyword(rest, "set") {
                return Ok(self.finish(TokenKind::Set, self.offset + "set".len()));
            }
            if starts_with_keyword(rest, "union") {
                return Ok(self.finish(TokenKind::Union, self.offset + "union".len()));
            }
            if starts_with_keyword(rest, "text") {
                if self.consume_indent_text {
                    self.consume_indent_text = false;
                    self.advance(self.offset + "text".len());
                    continue;
                }
                return Ok(self.finish(TokenKind::Text, self.offset + "text".len()));
            }
            if starts_with_keyword(rest, "style") {
                return Ok(self.finish(TokenKind::Style, self.offset + "style".len()));
            }
            if let Some((length, text)) = bracket_label(rest) {
                return Ok(self.bracket_token(length, text));
            }
            if let Some(length) = numeric_length(rest) {
                return Ok(self.finish(TokenKind::Numeric, self.offset + length));
            }
            if let Some(length) = hex_color_length(rest) {
                return Ok(self.finish(TokenKind::HexColor, self.offset + length));
            }
            if let Some(length) = rgb_length(rest, "rgba", 4) {
                return Ok(self.finish(TokenKind::RgbaColor, self.offset + length));
            }
            if let Some(length) = rgb_length(rest, "rgb", 3) {
                return Ok(self.finish(TokenKind::RgbColor, self.offset + length));
            }
            if let Some(length) = identifier_length(rest) {
                return Ok(self.finish(TokenKind::Identifier, self.offset + length));
            }
            if let Some(length) = string_length(rest) {
                return Ok(self.finish(TokenKind::Str, self.offset + length));
            }
            if rest.starts_with(',') {
                return Ok(self.finish(TokenKind::Comma, self.offset + 1));
            }
            if rest.starts_with(':') {
                return Ok(self.finish(TokenKind::Colon, self.offset + 1));
            }
            let ch = rest.chars().next().expect("non-empty remainder");
            return Err(ParseError::at(
                format!("unexpected character '{ch}'"),
                self.span_of(ch),
            ));
        }
    }

    /// The end of a `%%` comment starting at the current offset, when one does. Both comment
    /// rules are checked, in their file order.
    fn comment_end(&self) -> Option<usize> {
        let rest = &self.source[self.offset..];
        if let Some(after) = rest.strip_prefix("%%") {
            if !after.starts_with('{') {
                return Some(self.offset + 2 + line_length(after));
            }
            return None;
        }
        let first = rest.chars().next()?;
        if first == '}' {
            return None;
        }
        let after_first = &rest[first.len_utf8()..];
        let after_percents = after_first.strip_prefix("%%")?;
        Some(self.offset + first.len_utf8() + 2 + line_length(after_percents))
    }

    fn eof_token(&self) -> Token {
        Token {
            kind: TokenKind::Eof,
            text: String::new(),
            span: Span {
                line: self.line,
                column: self.column,
                start: self.offset,
                end: self.offset,
            },
        }
    }

    fn span_of(&self, ch: char) -> Span {
        Span {
            line: self.line,
            column: self.column,
            start: self.offset,
            end: self.offset + ch.len_utf8(),
        }
    }

    fn finish(&mut self, kind: TokenKind, end: usize) -> Token {
        let span = Span {
            line: self.line,
            column: self.column,
            start: self.offset,
            end,
        };
        let text = self.source[self.offset..end].to_string();
        self.advance(end);
        Token { kind, text, span }
    }

    fn bracket_token(&mut self, length: usize, text: String) -> Token {
        let span = Span {
            line: self.line,
            column: self.column,
            start: self.offset,
            end: self.offset + length,
        };
        self.advance(self.offset + length);
        Token {
            kind: TokenKind::BracketLabel,
            text,
            span,
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

/// `%options case-insensitive`: the first `keyword.len()` bytes, compared without case.
fn starts_with_keyword(text: &str, keyword: &str) -> bool {
    text.get(..keyword.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(keyword))
}

/// `text\b`, case-insensitively: the `bol` lookahead before an indented text statement.
fn starts_with_word(text: &str, word: &str) -> bool {
    starts_with_keyword(text, word)
        && text[word.len()..]
            .chars()
            .next()
            .is_none_or(|ch| !(ch.is_ascii_alphanumeric() || ch == '_'))
}

/// `"title"\s[^#\n;]+`, the only multi-line keyword rule and the only one with a value.
fn title_length(text: &str) -> Option<usize> {
    if !starts_with_keyword(text, "title") {
        return None;
    }
    let after = &text["title".len()..];
    let space = after.chars().next()?;
    if !crate::preprocess::is_js_whitespace(space) {
        return None;
    }
    let mut end = "title".len() + space.len_utf8();
    let mut value = 0;
    while let Some(ch) = text[end..].chars().next() {
        if matches!(ch, '#' | '\n' | ';') {
            break;
        }
        end += ch.len_utf8();
        value += 1;
    }
    (value > 0).then_some(end)
}

/// `\["[^"]*"\]` and `\[[^\]"]+\]`, the two bracket label rules. The returned length spans the
/// whole match; the text is what the rule's `yytext` rewriting leaves: the quoted content
/// untouched, the unquoted content trimmed.
fn bracket_label(text: &str) -> Option<(usize, String)> {
    let after_open = text.strip_prefix('[')?;
    if let Some(after_quote) = after_open.strip_prefix('"') {
        let close = after_quote.find('"')?;
        if !after_quote[close + 1..].starts_with(']') {
            return None;
        }
        return Some((close + 4, after_quote[..close].to_string()));
    }
    let close = after_open.find(']')?;
    let content = &after_open[..close];
    if content.is_empty() || content.contains('"') {
        return None;
    }
    Some((
        close + 2,
        content
            .trim_matches(crate::preprocess::is_js_whitespace)
            .to_string(),
    ))
}

/// `[+-]?(\d+(\.\d+)?|\.\d+)`.
fn numeric_length(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut index = 0;
    if matches!(bytes.first(), Some(b'+') | Some(b'-')) {
        index = 1;
    }
    let digits_start = index;
    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    let fraction =
        bytes.get(index) == Some(&b'.') && bytes.get(index + 1).is_some_and(u8::is_ascii_digit);
    if index == digits_start && !fraction {
        return None;
    }
    if fraction {
        index += 1;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
    }
    Some(index)
}

/// `\#[0-9a-fA-F]{3,8}`.
fn hex_color_length(text: &str) -> Option<usize> {
    let rest = text.strip_prefix('#')?;
    let digits = rest.bytes().take_while(u8::is_ascii_hexdigit).count();
    (digits >= 3).then(|| 1 + digits.min(8))
}

/// `rgba\(...\)` or `rgb\(...\)`: three or four `[0-9.]+` components, separated by commas,
/// with `\s*` around each.
fn rgb_length(text: &str, name: &str, components: usize) -> Option<usize> {
    if !starts_with_keyword(text, name) {
        return None;
    }
    let mut index = name.len();
    if !text[index..].starts_with('(') {
        return None;
    }
    index += 1;
    for component in 0..components {
        index = skip_js_whitespace(text, index);
        let start = index;
        while let Some(ch) = text[index..].chars().next() {
            if !(ch.is_ascii_digit() || ch == '.') {
                break;
            }
            index += ch.len_utf8();
        }
        if index == start {
            return None;
        }
        index = skip_js_whitespace(text, index);
        let expected = if component + 1 == components {
            ')'
        } else {
            ','
        };
        if !text[index..].starts_with(expected) {
            return None;
        }
        index += 1;
    }
    Some(index)
}

/// `[A-Za-z_][A-Za-z0-9\-_]*`.
fn identifier_length(text: &str) -> Option<usize> {
    let mut chars = text.char_indices();
    let (_, first) = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    let mut end = first.len_utf8();
    for (index, ch) in chars {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            end = index + ch.len_utf8();
        } else {
            break;
        }
    }
    Some(end)
}

/// `"[^"]*"`. A string may span line breaks, as the character class allows.
fn string_length(text: &str) -> Option<usize> {
    let rest = text.strip_prefix('"')?;
    let close = rest.find('"')?;
    Some(close + 2)
}

/// `[\n\r]+`.
fn newline_run(text: &str) -> Option<usize> {
    let length = text
        .bytes()
        .take_while(|byte| *byte == b'\n' || *byte == b'\r')
        .count();
    (length > 0).then_some(length)
}

/// `[ \t]+`.
fn horizontal_space(text: &str) -> Option<usize> {
    let length = text
        .bytes()
        .take_while(|byte| *byte == b' ' || *byte == b'\t')
        .count();
    (length > 0).then_some(length)
}

/// `String.prototype.trim`'s whitespace, which every upstream token pattern spells `\s`.
fn skip_js_whitespace(text: &str, mut index: usize) -> usize {
    while let Some(ch) = text[index..].chars().next() {
        if !crate::preprocess::is_js_whitespace(ch) {
            break;
        }
        index += ch.len_utf8();
    }
    index
}

/// The length of `[^\n]*` at the start of `text`.
fn line_length(text: &str) -> usize {
    text.find('\n').unwrap_or(text.len())
}
