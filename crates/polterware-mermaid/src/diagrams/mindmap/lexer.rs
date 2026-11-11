//! The mindmap lexer: a hand port of the `%lex` section of upstream
//! `packages/mermaid/src/diagrams/mindmap/parser/mindmap.jison`.
//!
//! Reproduced from the grammar: the exclusive start conditions (`%x NODE NSTR NSTR2 ICON
//! CLASS`) with their `begin`/`popState` stack, `%options case-insensitive`, and the
//! non-flex matcher, which walks the rules of the current condition in declaration order and
//! takes the first that matches at the current position (not the longest).
//!
//! Two jison details are reproduced on purpose. In non-flex mode a rule whose pattern ends
//! in a word character gets a trailing `\b` from jison's lexer generator, so `mindmap` does
//! not eat the start of a longer word; and `<<EOF>>` is only added to the inclusive
//! conditions, so input that ends inside one of the exclusive modes above yields the end of
//! the input without the grammar's `EOF` terminal and the parse fails — an unclosed `(...)`
//! node, an unclosed `::icon(...)`, or a `:::` line with nothing after it is an error.
//!
//! Two rules keep their upstream shapes: `\s*\%\%.*` swallows the whitespace *before* a
//! comment (blank lines included), and the `NODE` condition's `[^\)\]\(\}]+` does not stop
//! at a line break, so a node description may span lines. JavaScript's `.` does stop at one;
//! that difference is kept too. This crate receives `\n`-normalized text from
//! [`crate::preprocess`], so `\r` only survives in text a caller passes directly.

use crate::error::{ParseError, Span};

/// A jison start condition (`%x`), pushed and popped by the rules below.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Initial,
    /// `<NODE>`: inside a node's delimiters.
    Node,
    /// `<NSTR>`: inside a `"..."` node description.
    NodeQuoted,
    /// `<NSTR2>`: inside a ``"`...`"`` node description.
    NodeQuotedBacktick,
    /// `<ICON>`: inside `::icon(...)`.
    Icon,
    /// `<CLASS>`: after `:::`, to the end of the line.
    Class,
}

/// The grammar's terminals, with the names the jison file spells (`SPACELINE`, `NODE_ID`,
/// `NODE_DSTART`, `NODE_DESCR`, `NODE_DEND`, `MINDMAP`, `NL`, `ICON`, `CLASS`, `EOF`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    /// `MINDMAP`
    Mindmap,
    /// `SPACELINE`: a `%%` comment line, or whitespace running into a line break.
    SpaceLine,
    /// `SPACELIST`: the indentation before a statement.
    SpaceList,
    /// `NL`
    Newline,
    /// `NODE_ID`
    NodeId,
    /// `NODE_DSTART`
    NodeStart,
    /// `NODE_DESCR`
    NodeDescr,
    /// `NODE_DEND`
    NodeEnd,
    /// `ICON`
    Icon,
    /// `CLASS`
    Class,
    /// `EOF`: the grammar's end-of-input terminal, emitted in the `INITIAL` condition only.
    Eof,
    /// The end of the input reached in an exclusive condition, where upstream has no `EOF`.
    EndOfInput,
}

#[derive(Clone, Debug)]
pub(crate) struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub span: Span,
}

#[derive(Clone, Copy)]
struct Mark {
    pos: usize,
    line: u32,
    column: u32,
}

pub(crate) struct Lexer<'a> {
    src: &'a str,
    pos: usize,
    line: u32,
    column: u32,
    modes: Vec<Mode>,
    /// Set once the end of the input has produced its last token; later calls report the end
    /// of the input without the `EOF` terminal, as jison's `done` flag does.
    ended: bool,
}

impl<'a> Lexer<'a> {
    pub(crate) fn new(src: &'a str) -> Self {
        Self {
            src,
            pos: 0,
            line: 1,
            column: 1,
            modes: vec![Mode::Initial],
            ended: false,
        }
    }

    pub(crate) fn next_token(&mut self) -> Result<Token, ParseError> {
        loop {
            if self.pos >= self.src.len() {
                return Ok(self.end_token());
            }
            let token = match self.mode() {
                Mode::Initial => self.initial()?,
                Mode::Node => self.node()?,
                Mode::NodeQuoted => self.node_quoted()?,
                Mode::NodeQuotedBacktick => self.node_quoted_backtick()?,
                Mode::Icon => self.icon(),
                Mode::Class => self.class()?,
            };
            if let Some(token) = token {
                return Ok(token);
            }
        }
    }

    fn mark(&self) -> Mark {
        Mark {
            pos: self.pos,
            line: self.line,
            column: self.column,
        }
    }

    fn span_from(&self, mark: Mark) -> Span {
        Span {
            line: mark.line,
            column: mark.column,
            start: mark.pos,
            end: self.pos,
        }
    }

    fn make(&self, kind: TokenKind, mark: Mark) -> Token {
        Token {
            kind,
            text: self.src[mark.pos..self.pos].to_string(),
            span: self.span_from(mark),
        }
    }

    fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }

    fn bump(&mut self, bytes: usize) {
        let end = self.pos + bytes;
        for ch in self.src[self.pos..end].chars() {
            if ch == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
        }
        self.pos = end;
    }

    fn mode(&self) -> Mode {
        *self.modes.last().unwrap_or(&Mode::Initial)
    }

    fn push(&mut self, mode: Mode) {
        self.modes.push(mode);
    }

    fn pop(&mut self) {
        if self.modes.len() > 1 {
            self.modes.pop();
        }
    }

    /// `<<EOF>>` fires only in the inclusive conditions, so the exclusive modes end the
    /// input without the grammar's `EOF` terminal.
    fn end_token(&mut self) -> Token {
        let mark = self.mark();
        let kind = if !self.ended && self.mode() == Mode::Initial {
            self.ended = true;
            TokenKind::Eof
        } else {
            self.ended = true;
            TokenKind::EndOfInput
        };
        self.make(kind, mark)
    }

    // -- INITIAL ------------------------------------------------------------------------

    fn initial(&mut self) -> Result<Option<Token>, ParseError> {
        let rest = self.rest();
        let spaces = run_len(rest, is_space);
        // \s*\%\%.*
        if rest[spaces..].starts_with("%%") {
            let mark = self.mark();
            let len = spaces + run_len(&rest[spaces..], |ch| !is_line_terminator(ch));
            self.bump(len);
            return Ok(Some(self.make(TokenKind::SpaceLine, mark)));
        }
        // "mindmap", with the `\b` jison appends to a rule ending in a word character.
        if let Some(len) = self.keyword("mindmap") {
            let mark = self.mark();
            self.bump(len);
            return Ok(Some(self.make(TokenKind::Mindmap, mark)));
        }
        // ":::"
        if let Some(len) = self.literal(":::") {
            self.bump(len);
            self.push(Mode::Class);
            return Ok(None);
        }
        // "::icon("
        if let Some(len) = self.literal("::icon(") {
            self.bump(len);
            self.push(Mode::Icon);
            return Ok(None);
        }
        // [\s]+[\n]: the greedy `[\s]+` backtracks to the last line break in the run, which
        // needs at least one whitespace character before it.
        if spaces > 1
            && let Some(index) = rest[..spaces].rfind('\n')
            && index > 0
        {
            let mark = self.mark();
            self.bump(index + 1);
            return Ok(Some(self.make(TokenKind::SpaceLine, mark)));
        }
        // [\n]+
        if let Some(len) = leading_run(rest, |ch| ch == '\n') {
            let mark = self.mark();
            self.bump(len);
            return Ok(Some(self.make(TokenKind::Newline, mark)));
        }
        // The node delimiters, in grammar order. Each one begins the NODE condition.
        for literal in ["-)", "(-", "))", ")", "((", "{{", "(", "["] {
            if let Some(len) = self.literal(literal) {
                let mark = self.mark();
                self.bump(len);
                self.push(Mode::Node);
                return Ok(Some(self.make(TokenKind::NodeStart, mark)));
            }
        }
        // [\s]+
        if spaces > 0 {
            let mark = self.mark();
            self.bump(spaces);
            return Ok(Some(self.make(TokenKind::SpaceList, mark)));
        }
        // [^\(\[\n\)\{\}]+
        let len = run_len(rest, is_node_id_char);
        if len > 0 {
            let mark = self.mark();
            self.bump(len);
            return Ok(Some(self.make(TokenKind::NodeId, mark)));
        }
        // Every character is covered by the rules above; keep the branch honest anyway.
        Err(self.lexical_error())
    }

    // -- NODE / NSTR / NSTR2 ------------------------------------------------------------

    fn node(&mut self) -> Result<Option<Token>, ParseError> {
        let rest = self.rest();
        // ["][`] begins NSTR2.
        if let Some(len) = self.literal("\"`") {
            self.bump(len);
            self.push(Mode::NodeQuotedBacktick);
            return Ok(None);
        }
        // ["]
        if let Some(len) = self.literal("\"") {
            self.bump(len);
            self.push(Mode::NodeQuoted);
            return Ok(None);
        }
        // The closing delimiters, in grammar order. Each one ends the NODE condition.
        for literal in ["))", ")", "]", "}}", "(-", "-)", "((", "("] {
            if let Some(len) = self.literal(literal) {
                let mark = self.mark();
                self.bump(len);
                self.pop();
                return Ok(Some(self.make(TokenKind::NodeEnd, mark)));
            }
        }
        // [^\)\]\(\}]+: no line break in the class, so a description may span lines.
        let len = run_len(rest, |ch| !matches!(ch, ')' | ']' | '(' | '}'));
        if len > 0 {
            let mark = self.mark();
            self.bump(len);
            return Ok(Some(self.make(TokenKind::NodeDescr, mark)));
        }
        // .+(?!\(\(), which only a lone `}` reaches, and which matches to the line's end.
        let len = run_len(rest, |ch| !is_line_terminator(ch));
        if len > 0 {
            let mark = self.mark();
            self.bump(len);
            return Ok(Some(self.make(TokenKind::NodeDescr, mark)));
        }
        Err(self.lexical_error())
    }

    fn node_quoted(&mut self) -> Result<Option<Token>, ParseError> {
        let rest = self.rest();
        // [^"]+
        let len = run_len(rest, |ch| ch != '"');
        if len > 0 {
            let mark = self.mark();
            self.bump(len);
            return Ok(Some(self.make(TokenKind::NodeDescr, mark)));
        }
        // ["]
        if rest.starts_with('"') {
            self.bump(1);
            self.pop();
            return Ok(None);
        }
        Err(self.lexical_error())
    }

    fn node_quoted_backtick(&mut self) -> Result<Option<Token>, ParseError> {
        let rest = self.rest();
        // [^`"]+
        let len = run_len(rest, |ch| ch != '`' && ch != '"');
        if len > 0 {
            let mark = self.mark();
            self.bump(len);
            return Ok(Some(self.make(TokenKind::NodeDescr, mark)));
        }
        // [`]["]
        if let Some(len) = self.literal("`\"") {
            self.bump(len);
            self.pop();
            return Ok(None);
        }
        Err(self.lexical_error())
    }

    // -- ICON / CLASS -------------------------------------------------------------------

    fn icon(&mut self) -> Option<Token> {
        let rest = self.rest();
        // [^\)]+
        let len = run_len(rest, |ch| ch != ')');
        if len > 0 {
            let mark = self.mark();
            self.bump(len);
            return Some(self.make(TokenKind::Icon, mark));
        }
        // [\)]
        self.bump(1);
        self.pop();
        None
    }

    fn class(&mut self) -> Result<Option<Token>, ParseError> {
        let rest = self.rest();
        // .+
        let len = run_len(rest, |ch| !is_line_terminator(ch));
        if len > 0 {
            let mark = self.mark();
            self.bump(len);
            self.pop();
            return Ok(Some(self.make(TokenKind::Class, mark)));
        }
        // \n
        if rest.starts_with('\n') {
            self.bump(1);
            self.pop();
            return Ok(None);
        }
        Err(self.lexical_error())
    }

    // -- helpers ------------------------------------------------------------------------

    fn lexical_error(&self) -> ParseError {
        ParseError::at(
            format!("Lexical error on line {}.\nUnrecognized text.", self.line),
            self.span_from(self.mark()),
        )
    }

    fn starts_with_ci(&self, literal: &str) -> bool {
        self.rest()
            .get(..literal.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(literal))
    }

    /// A literal, matched case-insensitively (`%options case-insensitive`).
    fn literal(&self, literal: &str) -> Option<usize> {
        self.starts_with_ci(literal).then_some(literal.len())
    }

    /// A literal that ends in a word character: jison appends `\b`, so the next character
    /// must not be part of a word.
    fn keyword(&self, literal: &str) -> Option<usize> {
        if !self.starts_with_ci(literal) {
            return None;
        }
        match self.rest()[literal.len()..].chars().next() {
            Some(ch) if is_word_char(ch) => None,
            _ => Some(literal.len()),
        }
    }
}

/// `[^\(\[\n\)\{\}]+`.
fn is_node_id_char(ch: char) -> bool {
    !matches!(ch, '(' | '[' | '\n' | ')' | '{' | '}')
}

fn is_word_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

/// The characters JavaScript's `.` stops at.
fn is_line_terminator(ch: char) -> bool {
    matches!(ch, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

fn run_len(s: &str, predicate: impl Fn(char) -> bool) -> usize {
    s.chars()
        .take_while(|&ch| predicate(ch))
        .map(char::len_utf8)
        .sum()
}

fn leading_run(s: &str, predicate: impl Fn(char) -> bool) -> Option<usize> {
    let len = run_len(s, predicate);
    (len > 0).then_some(len)
}

/// JavaScript's `\s` (ECMAScript WhiteSpace and LineTerminator). Every one of these
/// characters is in the Basic Multilingual Plane, so its JavaScript string length and its
/// count of `char`s agree — the indentation level is counted in these units.
pub(crate) fn is_space(ch: char) -> bool {
    matches!(
        ch,
        '\t' | '\n' | '\u{000b}' | '\u{000c}' | '\r' | ' ' | '\u{00a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}
