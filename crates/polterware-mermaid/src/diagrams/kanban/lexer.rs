//! The kanban lexer: a port of the `%lex` section of
//! `packages/mermaid/src/diagrams/kanban/parser/kanban.jison`.
//!
//! Jison matches the rules in source order at the current offset and takes the first one that
//! matches (a JavaScript alternation, not the longest match), so the order below is the
//! upstream order. `%options case-insensitive` makes the two literal words (`kanban`,
//! `::icon(`) case-insensitive; character classes are unaffected.
//!
//! The `%x` (exclusive) states are a stack. `begin` pushes a state and `popState` pops it,
//! which is exactly jison's `conditionStack`: entering a node delimiter pushes `NODE`,
//! `NODE` pushes `NSTR`/`NSTR2` for a description, and the closing token pops back. Only the
//! rules tagged with the active state run there, except at end of input, where jison answers
//! `EOF` from any state.
//!
//! Two rules are worth spelling out because they decide how lines are split:
//!
//! * `[\s]+[\n]` becomes `SPACELINE` and matches the whitespace run up to and including its
//!   *last* newline, so a run made only of a newline and spaces starts with `NL` and then a
//!   `SPACELIST`.
//! * `[\n]+` becomes `NL`, so `NL` is a lone newline (or a newline followed by non-whitespace).
//!
//! The `shapeData` state is the `@{ ... }` block. The opening `@{` yields an empty
//! `SHAPE_DATA`, quotes are yielded as their own `SHAPE_DATA` tokens, and the closing `}` is
//! consumed without a token; the grammar concatenates every token into the YAML text. Inside
//! the quoted state, `/\n\s*/g` becomes `<br/>`, as upstream does.

use crate::common::text::is_line_terminator;
use crate::error::{ParseError, Span};
use crate::preprocess::is_js_whitespace;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    Kanban,
    NodeId,
    NodeDStart,
    NodeDDescr,
    NodeDEnd,
    ShapeData,
    Icon,
    Class,
    SpaceList,
    SpaceLine,
    Newline,
    Eof,
}

impl TokenKind {
    /// The token name upstream's generated parser prints in an error.
    pub(crate) fn name(self) -> &'static str {
        match self {
            TokenKind::Kanban => "KANBAN",
            TokenKind::NodeId => "NODE_ID",
            TokenKind::NodeDStart => "NODE_DSTART",
            TokenKind::NodeDDescr => "NODE_DESCR",
            TokenKind::NodeDEnd => "NODE_DEND",
            TokenKind::ShapeData => "SHAPE_DATA",
            TokenKind::Icon => "ICON",
            TokenKind::Class => "CLASS",
            TokenKind::SpaceList => "SPACELIST",
            TokenKind::SpaceLine => "SPACELINE",
            TokenKind::Newline => "NL",
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

/// The jison start conditions that matter to the grammar (`INITIAL` is `None`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Node,
    NStr,
    NStr2,
    Icon,
    Class,
    ShapeData,
    ShapeDataStr,
}

pub(crate) struct Lexer<'a> {
    source: &'a str,
    offset: usize,
    line: u32,
    column: u32,
    states: Vec<State>,
}

impl<'a> Lexer<'a> {
    pub(crate) fn new(source: &'a str) -> Self {
        Self {
            source,
            offset: 0,
            line: 1,
            column: 1,
            states: Vec::new(),
        }
    }

    pub(crate) fn next_token(&mut self) -> Result<Token, ParseError> {
        loop {
            if self.offset >= self.source.len() {
                return Ok(Token {
                    kind: TokenKind::Eof,
                    text: String::new(),
                    span: self.span_to(self.offset),
                });
            }
            let token = match self.states.last().copied() {
                None => self.initial()?,
                Some(State::Node) => self.node()?,
                Some(State::NStr) => self.node_string()?,
                Some(State::NStr2) => self.node_string2()?,
                Some(State::Icon) => self.icon()?,
                Some(State::Class) => self.class()?,
                Some(State::ShapeData) => self.shape_data()?,
                Some(State::ShapeDataStr) => self.shape_data_string()?,
            };
            if let Some(token) = token {
                return Ok(token);
            }
        }
    }

    fn rest(&self) -> &'a str {
        &self.source[self.offset..]
    }

    /// The `INITIAL` rules, in jison order.
    fn initial(&mut self) -> Result<Option<Token>, ParseError> {
        let rest = self.rest();
        // \@{ — `yytext=""`, so the grammar's shapeData starts empty.
        if rest.starts_with("@{") {
            self.states.push(State::ShapeData);
            let span = self.span_to(self.offset + 2);
            self.advance(self.offset + 2);
            return Ok(Some(Token {
                kind: TokenKind::ShapeData,
                text: String::new(),
                span,
            }));
        }
        // \s*\%%.*
        if let Some(end) = comment(rest) {
            return Ok(Some(self.finish(TokenKind::SpaceLine, end)));
        }
        // "kanban"
        if let Some(end) = keyword_ignore_case(rest, "kanban") {
            return Ok(Some(self.finish(TokenKind::Kanban, end)));
        }
        // ":::"
        if rest.starts_with(":::") {
            self.states.push(State::Class);
            self.advance(self.offset + 3);
            return Ok(None);
        }
        // "::icon("
        if let Some(end) = keyword_ignore_case(rest, "::icon(") {
            self.states.push(State::Icon);
            self.advance(self.offset + end);
            return Ok(None);
        }
        // [\s]+[\n]
        if let Some(end) = space_line(rest) {
            return Ok(Some(self.finish(TokenKind::SpaceLine, end)));
        }
        // [\n]+
        if let Some(end) = newline_run(rest) {
            return Ok(Some(self.finish(TokenKind::Newline, end)));
        }
        // "-)", "(-", "))", ")", "((", "{{", "(", "["
        for (prefix, kind) in NODE_DELIMITERS {
            if rest.starts_with(prefix) {
                self.states.push(State::Node);
                return Ok(Some(self.finish(*kind, prefix.len())));
            }
        }
        // [\s]+
        if let Some(end) = space_list(rest) {
            return Ok(Some(self.finish(TokenKind::SpaceList, end)));
        }
        // [^\(\[\n\)\{\}@]+
        if let Some(end) = node_id(rest) {
            return Ok(Some(self.finish(TokenKind::NodeId, end)));
        }
        Err(self.unexpected(rest))
    }

    /// The `NODE` rules, in jison order.
    fn node(&mut self) -> Result<Option<Token>, ParseError> {
        let rest = self.rest();
        // ["][`]
        if rest.starts_with("\"`") {
            self.states.push(State::NStr2);
            self.advance(self.offset + 2);
            return Ok(None);
        }
        // ["]
        if rest.starts_with('"') {
            self.states.push(State::NStr);
            self.advance(self.offset + 1);
            return Ok(None);
        }
        // [\)]\) , [\)] , [\]] , "}}" , "(-" , "-)" , "((" , "("
        for (prefix, kind) in NODE_END_RULES {
            if rest.starts_with(prefix) {
                self.states.pop();
                return Ok(Some(self.finish(*kind, prefix.len())));
            }
        }
        // [^\)\]\(\}]+
        if let Some(end) = node_description(rest) {
            return Ok(Some(self.finish(TokenKind::NodeDDescr, end)));
        }
        // .+(?!\(\()
        if let Some(end) = node_description_fallback(rest) {
            return Ok(Some(self.finish(TokenKind::NodeDDescr, end)));
        }
        Err(self.unexpected(rest))
    }

    /// The `NSTR` rules: a plain quoted description.
    fn node_string(&mut self) -> Result<Option<Token>, ParseError> {
        let rest = self.rest();
        if let Some(end) = run_while(rest, |ch| ch != '"') {
            return Ok(Some(self.finish(TokenKind::NodeDDescr, end)));
        }
        if rest.starts_with('"') {
            self.states.pop();
            self.advance(self.offset + 1);
            return Ok(None);
        }
        Err(self.unexpected(rest))
    }

    /// The `NSTR2` rules: a backtick-fenced markdown description.
    fn node_string2(&mut self) -> Result<Option<Token>, ParseError> {
        let rest = self.rest();
        if let Some(end) = run_while(rest, |ch| ch != '`' && ch != '"') {
            return Ok(Some(self.finish(TokenKind::NodeDDescr, end)));
        }
        if rest.starts_with("`\"") {
            self.states.pop();
            self.advance(self.offset + 2);
            return Ok(None);
        }
        Err(self.unexpected(rest))
    }

    /// The `ICON` rules: everything up to the closing parenthesis, newlines included.
    fn icon(&mut self) -> Result<Option<Token>, ParseError> {
        let rest = self.rest();
        if let Some(end) = run_while(rest, |ch| ch != ')') {
            return Ok(Some(self.finish(TokenKind::Icon, end)));
        }
        if rest.starts_with(')') {
            self.states.pop();
            self.advance(self.offset + 1);
            return Ok(None);
        }
        Err(self.unexpected(rest))
    }

    /// The `CLASS` rules: one line of class names, or the newline that ends an empty one.
    fn class(&mut self) -> Result<Option<Token>, ParseError> {
        let rest = self.rest();
        if let Some(end) = line_run(rest) {
            self.states.pop();
            return Ok(Some(self.finish(TokenKind::Class, end)));
        }
        if rest.starts_with('\n') {
            self.states.pop();
            self.advance(self.offset + 1);
            return Ok(None);
        }
        Err(self.unexpected(rest))
    }

    /// The `shapeData` rules: the raw text of a `@{ ... }` block.
    fn shape_data(&mut self) -> Result<Option<Token>, ParseError> {
        let rest = self.rest();
        if rest.starts_with('"') {
            self.states.push(State::ShapeDataStr);
            return Ok(Some(self.finish(TokenKind::ShapeData, 1)));
        }
        if let Some(end) = run_while(rest, |ch| ch != '}' && ch != '"') {
            return Ok(Some(self.finish(TokenKind::ShapeData, end)));
        }
        if rest.starts_with('}') {
            self.states.pop();
            self.advance(self.offset + 1);
            return Ok(None);
        }
        Err(self.unexpected(rest))
    }

    /// The `shapeDataStr` rules: a quoted value, whose line breaks become `<br/>`.
    fn shape_data_string(&mut self) -> Result<Option<Token>, ParseError> {
        let rest = self.rest();
        if let Some(end) = run_while(rest, |ch| ch != '"') {
            let text = replace_line_breaks(&rest[..end]);
            let span = self.span_to(self.offset + end);
            self.advance(self.offset + end);
            return Ok(Some(Token {
                kind: TokenKind::ShapeData,
                text,
                span,
            }));
        }
        if rest.starts_with('"') {
            self.states.pop();
            return Ok(Some(self.finish(TokenKind::ShapeData, 1)));
        }
        Err(self.unexpected(rest))
    }

    fn unexpected(&self, rest: &str) -> ParseError {
        let ch = rest.chars().next().expect("non-empty remainder");
        ParseError::at(
            format!("unexpected character '{ch}'"),
            self.span_to(self.offset),
        )
    }

    fn finish(&mut self, kind: TokenKind, end: usize) -> Token {
        let span = self.span_to(self.offset + end);
        let text = self.source[self.offset..self.offset + end].to_string();
        self.advance(self.offset + end);
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

/// The node delimiters of the `INITIAL` state, in jison order. The two-character forms come
/// before the one-character forms they extend.
const NODE_DELIMITERS: &[(&str, TokenKind)] = &[
    ("-)", TokenKind::NodeDStart),
    ("(-", TokenKind::NodeDStart),
    ("))", TokenKind::NodeDStart),
    (")", TokenKind::NodeDStart),
    ("((", TokenKind::NodeDStart),
    ("{{", TokenKind::NodeDStart),
    ("(", TokenKind::NodeDStart),
    ("[", TokenKind::NodeDStart),
];

/// The `NODE_DEND` rules of the `NODE` state, in jison order.
const NODE_END_RULES: &[(&str, TokenKind)] = &[
    ("))", TokenKind::NodeDEnd),
    (")", TokenKind::NodeDEnd),
    ("]", TokenKind::NodeDEnd),
    ("}}", TokenKind::NodeDEnd),
    ("(-", TokenKind::NodeDEnd),
    ("-)", TokenKind::NodeDEnd),
    ("((", TokenKind::NodeDEnd),
    ("(", TokenKind::NodeDEnd),
];

/// `\s*\%\%.*`: the longest whitespace run that is followed directly by `%%`, then the rest of
/// the line (a `.` stops at a line terminator).
fn comment(rest: &str) -> Option<usize> {
    let mut start = 0;
    for (index, ch) in rest.char_indices() {
        if is_js_whitespace(ch) {
            start = index + ch.len_utf8();
        } else {
            break;
        }
    }
    let after = rest[start..].strip_prefix("%%")?;
    Some(start + 2 + line_run(after).unwrap_or(after.len()))
}

/// A literal word matched case-insensitively, as `%options case-insensitive` does.
fn keyword_ignore_case(rest: &str, word: &str) -> Option<usize> {
    let head = rest.get(..word.len())?;
    head.eq_ignore_ascii_case(word).then_some(word.len())
}

/// `[\s]+[\n]`: the whitespace run up to and including its last newline, when the newline is
/// not the first character of the run.
fn space_line(rest: &str) -> Option<usize> {
    let mut last_newline = None;
    for (index, ch) in rest.char_indices() {
        if !is_js_whitespace(ch) {
            break;
        }
        if ch == '\n' {
            last_newline = Some(index);
        }
    }
    last_newline
        .filter(|index| *index > 0)
        .map(|index| index + 1)
}

/// `[\n]+`.
fn newline_run(rest: &str) -> Option<usize> {
    let end = rest.bytes().take_while(|byte| *byte == b'\n').count();
    (end > 0).then_some(end)
}

/// `[\s]+`.
fn space_list(rest: &str) -> Option<usize> {
    run_while(rest, is_js_whitespace)
}

/// `[^\(\[\n\)\{\}@]+`.
fn node_id(rest: &str) -> Option<usize> {
    run_while(rest, |ch| {
        !matches!(ch, '(' | '[' | '\n' | ')' | '{' | '}' | '@')
    })
}

/// `[^\)\]\(\}]+`: a description may contain line breaks and quotes.
fn node_description(rest: &str) -> Option<usize> {
    run_while(rest, |ch| !matches!(ch, ')' | ']' | '(' | '}'))
}

/// `.+(?!\(\()`: only reachable for `}`, the one character the description class excludes
/// and no end rule matches. Since the greedy run stops at a line terminator, the lookahead
/// never fires, so this is the run to the end of the line.
fn node_description_fallback(rest: &str) -> Option<usize> {
    line_run(rest)
}

/// One or more characters that are not a JavaScript line terminator (what `.` matches).
fn line_run(rest: &str) -> Option<usize> {
    run_while(rest, |ch| !is_line_terminator(ch))
}

/// The length of the longest prefix made of characters accepted by `accept`, or `None` when
/// even the first character is rejected.
fn run_while(rest: &str, accept: impl Fn(char) -> bool) -> Option<usize> {
    let mut end = 0;
    for (index, ch) in rest.char_indices() {
        if !accept(ch) {
            break;
        }
        end = index + ch.len_utf8();
    }
    (end > 0).then_some(end)
}

/// `/\n\s*/g` → `<br/>`: every line break with the whitespace after it collapses to one tag.
fn replace_line_breaks(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\n' {
            out.push_str("<br/>");
            while chars.peek().is_some_and(|next| is_js_whitespace(*next)) {
                chars.next();
            }
        } else {
            out.push(ch);
        }
    }
    out
}
