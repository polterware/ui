//! The ishikawa lexer: a hand port of the `%lex` section of upstream
//! `packages/mermaid/src/diagrams/ishikawa/parser/ishikawa.jison`.
//!
//! Reproduced from the grammar: `%options case-insensitive`, and the non-flex matcher, which
//! walks the rules in declaration order and takes the first that matches at the current
//! position (not the longest). That order is what makes indentation readable: `[\s]+` claims
//! a line's leading whitespace as a `SPACELIST` before `[^\n]+` could take it as `TEXT`.
//!
//! Jison details kept on purpose:
//!
//! * quoted literals get a trailing `\b` from the lexer generator in non-flex mode, so
//!   `ishikawa-betaX` is not a header: `ishikawa-beta\b` fails and `ishikawa\b` matches the
//!   first eight characters, leaving `-betaX` as `TEXT`;
//! * `\s*\%\%.*` takes the whitespace *before* a comment with it (blank lines included) and
//!   stops at the line's end, so the line break that ends a comment is a separate `NL`;
//! * `[\s]+[\n]` lets its greedy `[\s]+` backtrack to the last line break inside the run, so
//!   a run of blank or whitespace-only lines is one `SPACELINE` and the whitespace after its
//!   last line break is left for the next token; a lone `\n` does not match it and becomes
//!   `NL` instead;
//! * `[^\n]+` stops only at `\n`, so a stray `\r` stays in a `TEXT` token;
//! * `<<EOF>>` is a rule like any other and returns the grammar's `EOF` terminal with empty
//!   text, which is different from the end of the token stream.
//!
//! Every character starts one of these tokens, so the lexer never fails; there is no
//! "Unrecognized text" case to port. JavaScript's `\s` is
//! [`crate::preprocess::is_js_whitespace`]. This crate receives `\n`-normalized text from
//! [`crate::preprocess`], so `\r` only reaches the lexer when a caller passes a body
//! directly.

use crate::common::text::is_line_terminator;
use crate::error::Span;
use crate::preprocess::is_js_whitespace;

/// The grammar's terminals, with the names the jison file spells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    /// `ISHIKAWA`: the `ishikawa` or `ishikawa-beta` header, case-insensitively.
    Ishikawa,
    /// `SPACELINE`: a `%%` comment line, or a whitespace run holding a line break.
    SpaceLine,
    /// `NL`: a run of line breaks.
    Newline,
    /// `SPACELIST`: the indentation before a statement.
    SpaceList,
    /// `TEXT`: the rest of a line.
    Text,
    /// `EOF`: the grammar's end-of-input terminal, produced once by `<<EOF>>`.
    Eof,
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

    pub(crate) fn next_token(&mut self) -> Token {
        let rest = &self.source[self.offset..];
        if rest.is_empty() {
            return Token {
                kind: TokenKind::Eof,
                text: String::new(),
                span: self.span_to(self.offset),
            };
        }
        // `\s*\%\%.*`: the whitespace before the comment, `%%`, and the rest of the line.
        let spaces = run_len(rest, is_js_whitespace);
        if let Some(comment) = rest[spaces..].strip_prefix("%%") {
            let end = spaces + 2 + run_len(comment, |ch| !is_line_terminator(ch));
            return self.finish(TokenKind::SpaceLine, end);
        }
        // `"ishikawa-beta"` / `"ishikawa"`, case-insensitive, each with jison's `\b`.
        if let Some(end) = keyword(rest, "ishikawa-beta").or_else(|| keyword(rest, "ishikawa")) {
            return self.finish(TokenKind::Ishikawa, end);
        }
        // `[\s]+[\n]`
        if let Some(end) = space_line(rest) {
            return self.finish(TokenKind::SpaceLine, end);
        }
        // `[\n]+`
        if let Some(end) = leading_run(rest, |ch| ch == '\n') {
            return self.finish(TokenKind::Newline, end);
        }
        // `[\s]+`
        if spaces > 0 {
            return self.finish(TokenKind::SpaceList, spaces);
        }
        // `[^\n]+`, which matches at least the current character.
        let end = run_len(rest, |ch| ch != '\n');
        self.finish(TokenKind::Text, end)
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

/// `[\s]+[\n]`: the greedy whitespace run backtracks to the last line break in it, which
/// needs at least one whitespace character before it.
fn space_line(rest: &str) -> Option<usize> {
    let spaces = run_len(rest, is_js_whitespace);
    if spaces <= 1 {
        return None;
    }
    let last_break = rest[..spaces].rfind('\n')?;
    (last_break > 0).then_some(last_break + 1)
}

/// A quoted literal rule, case-insensitively, with the `\b` jison appends to a rule whose
/// pattern ends in a word character.
fn keyword(rest: &str, word: &str) -> Option<usize> {
    if !rest.get(..word.len())?.eq_ignore_ascii_case(word) {
        return None;
    }
    match rest[word.len()..].chars().next() {
        Some(ch) if is_word_char(ch) => None,
        _ => Some(word.len()),
    }
}

fn is_word_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn run_len(text: &str, predicate: impl Fn(char) -> bool) -> usize {
    text.chars()
        .take_while(|&ch| predicate(ch))
        .map(char::len_utf8)
        .sum()
}

fn leading_run(text: &str, predicate: impl Fn(char) -> bool) -> Option<usize> {
    let end = run_len(text, predicate);
    (end > 0).then_some(end)
}
