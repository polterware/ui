//! The sankey lexer: a port of the `%lex` section of upstream `parser/sankey.jison`.
//!
//! The grammar declares the exclusive start conditions `INITIAL`, `csv`, and `escaped_text`,
//! and jison picks the *first* rule that matches at the current offset, not the longest. Both
//! are reproduced here, including what that order makes unreachable:
//!
//! ```text
//! <INITIAL>"sankey-beta"                          SANKEY, push csv
//! <INITIAL>"sankey"                               SANKEY, push csv
//! <INITIAL,csv><<EOF>>                            EOF
//! <INITIAL,csv>(CRLF|LF)                          NEWLINE
//! <INITIAL,csv>COMMA                              COMMA
//! <INITIAL,csv>DQUOTE                             DQUOTE, push escaped_text
//! <INITIAL,csv>TEXTDATA*                          NON_ESCAPED_TEXT
//! <INITIAL,csv,escaped_text>DQUOTE(?!DQUOTE)      DQUOTE, pop escaped_text
//! <INITIAL,csv,escaped_text>(TEXTDATA|COMMA|CR|LF|DQUOTE DQUOTE)*  ESCAPED_TEXT
//! ```
//!
//! `TEXTDATA` is `[\u0020-\u0021\u0023-\u002B\u002D-\u007E]`: printable ASCII except the
//! double quote and the comma. Anything else — a tab, a non-ASCII character, a bare CR — makes
//! jison fail lexically, and it fails here with the offending character and its span.
//!
//! Because the rules are tried in order, `TEXTDATA*` never returns the empty match it could:
//! at a comma, a quote, a newline, or the end of the input an earlier rule wins. An empty
//! unquoted field is therefore a parse error, and so is an empty quoted one (`DQUOTE DQUOTE`
//! hands the parser a second `DQUOTE` where `ESCAPED_TEXT` is required).
//!
//! Two pieces of upstream's wrapper are folded into the lexer, because the mermaid pipeline
//! applies them before the grammar sees the text:
//!
//! * `diagram-api/comments.ts` `cleanupComments` removes `^\s*%%(?!{)[^\n]+` lines. Comment
//!   lines are skipped here, content included, so a quoted field that spans one behaves as if
//!   the line were not there. Their bytes stay in the body, so every span still points at the
//!   caller's text.
//! * `sankeyUtils.ts` `prepareTextForParsing` collapses every run of `\n`/`\r` to one newline,
//!   everywhere, and the whole-string `trim` removes leading and trailing whitespace. Leading
//!   whitespace is skipped in `INITIAL`; a newline run inside a quoted field collapses to one
//!   `\n`; the parser accepts a run of `NEWLINE` tokens between records and trims the end of
//!   the body. The lexer itself still emits one `NEWLINE` per line, so nothing else changes.

use std::ops::Range;

use crate::error::{ParseError, Span};

/// `%options case-insensitive` makes the header keywords case-insensitive.
fn starts_with_ignore_case(text: &str, prefix: &str) -> bool {
    text.get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    /// `SANKEY`, the header keyword.
    Sankey,
    /// `NEWLINE`, one line break (`\r\n` or `\n`).
    Newline,
    /// `COMMA`.
    Comma,
    /// `DQUOTE`, opening or closing a quoted field.
    DQuote,
    /// `NON_ESCAPED_TEXT`.
    NonEscapedText,
    /// `ESCAPED_TEXT`.
    EscapedText,
    Eof,
}

impl TokenKind {
    /// The token name upstream's generated parser prints in an error.
    pub(crate) fn name(self) -> &'static str {
        match self {
            TokenKind::Sankey => "SANKEY",
            TokenKind::Newline => "NEWLINE",
            TokenKind::Comma => "COMMA",
            TokenKind::DQuote => "DQUOTE",
            TokenKind::NonEscapedText => "NON_ESCAPED_TEXT",
            TokenKind::EscapedText => "ESCAPED_TEXT",
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Initial,
    Csv,
    Escaped,
}

pub(crate) struct Lexer<'a> {
    source: &'a str,
    offset: usize,
    line: u32,
    column: u32,
    state: State,
    /// The content ranges of `%%` comment lines, sorted; see [`comment_ranges`].
    comments: &'a [Range<usize>],
    /// Where the quoted field being read opened, for the unterminated-field error.
    escaped_start: Span,
}

impl<'a> Lexer<'a> {
    pub(crate) fn new(source: &'a str, comments: &'a [Range<usize>]) -> Self {
        Self {
            source,
            offset: 0,
            line: 1,
            column: 1,
            state: State::Initial,
            comments,
            escaped_start: Span::default(),
        }
    }

    pub(crate) fn next_token(&mut self) -> Result<Token, ParseError> {
        loop {
            self.skip_comment_content();
            let Some(ch) = self.current() else {
                return match self.state {
                    State::Initial => Err(ParseError::at(
                        "expected 'sankey' or 'sankey-beta'",
                        Span {
                            line: self.line,
                            column: self.column,
                            start: self.offset,
                            end: self.offset,
                        },
                    )),
                    State::Escaped => Err(ParseError::at(
                        "unterminated quoted field",
                        self.escaped_start,
                    )),
                    State::Csv => Ok(self.eof_token()),
                };
            };
            match self.state {
                State::Initial => {
                    if crate::preprocess::is_js_whitespace(ch) {
                        self.advance(self.offset + ch.len_utf8());
                        continue;
                    }
                    return self.header_token(ch);
                }
                State::Csv => return self.csv_token(ch),
                State::Escaped => return self.escaped_token(ch),
            }
        }
    }

    /// The two `INITIAL` header rules, in their declared order.
    fn header_token(&mut self, ch: char) -> Result<Token, ParseError> {
        let rest = &self.source[self.offset..];
        let length = if starts_with_ignore_case(rest, "sankey-beta") {
            "sankey-beta".len()
        } else if starts_with_ignore_case(rest, "sankey") {
            "sankey".len()
        } else {
            return Err(ParseError::at(
                "expected 'sankey' or 'sankey-beta'",
                self.span_of(ch),
            ));
        };
        self.state = State::Csv;
        Ok(self.finish(TokenKind::Sankey, self.offset + length))
    }

    fn csv_token(&mut self, ch: char) -> Result<Token, ParseError> {
        match ch {
            '\n' => Ok(self.finish(TokenKind::Newline, self.offset + 1)),
            '\r' if self.source[self.offset..].starts_with("\r\n") => {
                Ok(self.finish(TokenKind::Newline, self.offset + 2))
            }
            ',' => Ok(self.finish(TokenKind::Comma, self.offset + 1)),
            '"' => {
                self.state = State::Escaped;
                self.escaped_start = self.span_of(ch);
                Ok(self.finish(TokenKind::DQuote, self.offset + 1))
            }
            _ if is_text_data(ch) => {
                let mut end = self.offset;
                while let Some(next) = self.source[end..].chars().next() {
                    if !is_text_data(next) {
                        break;
                    }
                    end += next.len_utf8();
                }
                Ok(self.finish(TokenKind::NonEscapedText, end))
            }
            _ => Err(ParseError::at(
                format!("unexpected character '{ch}'"),
                self.span_of(ch),
            )),
        }
    }

    /// `escaped_text`: everything up to the first unescaped double quote is one
    /// `ESCAPED_TEXT`, and doubled quotes count as content.
    fn escaped_token(&mut self, ch: char) -> Result<Token, ParseError> {
        if ch == '"' && !self.source[self.offset + 1..].starts_with('"') {
            self.state = State::Csv;
            return Ok(self.finish(TokenKind::DQuote, self.offset + 1));
        }
        let start = self.offset;
        let span = self.span_of(ch);
        let mut text = String::new();
        loop {
            self.skip_comment_content();
            let Some(next) = self.current() else {
                return Err(ParseError::at(
                    "unterminated quoted field",
                    self.escaped_start,
                ));
            };
            if next == '"' {
                if self.source[self.offset + 1..].starts_with('"') {
                    text.push_str("\"\"");
                    self.advance(self.offset + 2);
                    continue;
                }
                break;
            }
            if next == '\n' || next == '\r' {
                // `prepareTextForParsing` collapses every newline run before jison reads it.
                // A removed comment line between two line breaks counts as part of the run.
                self.skip_line_breaks();
                text.push('\n');
                continue;
            }
            if is_text_data(next) || next == ',' {
                text.push(next);
                self.advance(self.offset + next.len_utf8());
                continue;
            }
            return Err(ParseError::at(
                format!("unexpected character '{next}'"),
                self.span_of(next),
            ));
        }
        Ok(Token {
            kind: TokenKind::EscapedText,
            text,
            span: Span {
                line: span.line,
                column: span.column,
                start,
                end: self.offset,
            },
        })
    }

    fn current(&self) -> Option<char> {
        self.source[self.offset..].chars().next()
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

    fn advance(&mut self, end: usize) {
        let end = end.min(self.source.len());
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

    /// Skips the content of a comment line the offset is inside, leaving its line break.
    fn skip_comment_content(&mut self) {
        while let Some(range) = self
            .comments
            .iter()
            .find(|range| range.start <= self.offset && self.offset < range.end)
        {
            let end = range.end.min(self.source.len());
            if end <= self.offset {
                break;
            }
            self.advance(end);
        }
    }

    /// Consumes a run of line breaks and the comment lines inside it. True when it consumed
    /// at least one line break.
    fn skip_line_breaks(&mut self) -> bool {
        let mut seen = false;
        loop {
            self.skip_comment_content();
            match self.current() {
                Some(ch) if ch == '\n' || ch == '\r' => {
                    self.advance(self.offset + ch.len_utf8());
                    seen = true;
                }
                _ => return seen,
            }
        }
    }
}

/// `TEXTDATA`: printable ASCII except `"` and `,`.
fn is_text_data(ch: char) -> bool {
    matches!(
        ch,
        '\u{20}'..='\u{21}' | '\u{23}'..='\u{2b}' | '\u{2d}'..='\u{7e}'
    )
}

/// The content ranges of the lines `cleanupComments` removes: a line whose first
/// non-whitespace characters are `%%` (but not `%%{`) followed by at least one character.
/// The range stops before the line break, which stays so the text the lexer reads keeps its
/// line numbers.
pub(crate) fn comment_ranges(source: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut start = 0;
    for line in source.split_inclusive('\n') {
        let content = line.strip_suffix('\n').unwrap_or(line);
        let content_end = start + content.len();
        let trimmed = content.trim_start_matches(crate::preprocess::is_js_whitespace);
        if let Some(rest) = trimmed.strip_prefix("%%")
            && !rest.is_empty()
            && !rest.starts_with('{')
        {
            ranges.push(start..content_end);
        }
        start += line.len();
    }
    ranges
}
