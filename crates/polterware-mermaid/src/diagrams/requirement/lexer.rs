//! The requirement lexer: a hand port of the `%lex` section of upstream
//! `parser/requirementDiagram.jison`.
//!
//! The grammar is case-insensitive (`%options case-insensitive`) and uses exclusive start
//! conditions (`%x`), with a stack: `begin` pushes, `popState` pops. Jison's default lexing
//! rule is reproduced: the first rule that matches at the current offset wins, not the
//! longest. Three quirks are part of that behavior:
//!
//! * A literal keyword rule takes a `\b` word boundary after it, so `id:` is `ID` while
//!   `idx` is an `unqString`.
//! * `\s+` comes after `(\r?\n)+`, so a run of whitespace that starts with a space swallows
//!   the newline that follows it; a line of only spaces is therefore not a `NEWLINE`.
//! * The accessible-description and accessible-title value rules are `[^\n]*`, the
//!   semicolons and `#` in the original lookaheads being read as comments by jison's lexer
//!   generator.
//!
//! `%%` (and a bare `%`) and `#` comments are skipped in the initial state only; in the
//! style state `#` and `%` are style components. Text preprocessing has already removed
//! frontmatter and `%%{init}%%` directives, and normalized line endings.

use crate::error::{ParseError, Span};
use crate::preprocess::is_js_whitespace;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    /// The `title` line, which no grammar production consumes.
    Title,
    AccTitle,
    AccTitleValue,
    AccDescr,
    AccDescrValue,
    AccDescrMultilineValue,
    DirectionTb,
    DirectionBt,
    DirectionRl,
    DirectionLr,
    Newline,
    Eof,
    /// `requirementDiagram`.
    RequirementDiagram,
    StructStart,
    StructStop,
    StyleSeparator,
    /// The initial state's `:` (`COLONSEP`).
    ColonSep,
    Id,
    Text,
    Risk,
    VerifyMethod,
    Requirement,
    FunctionalRequirement,
    InterfaceRequirement,
    PerformanceRequirement,
    PhysicalRequirement,
    DesignConstraint,
    LowRisk,
    MediumRisk,
    HighRisk,
    VerifyAnalysis,
    VerifyDemonstration,
    VerifyInspection,
    VerifyTest,
    Element,
    Contains,
    Copies,
    Derives,
    Satisfies,
    Verifies,
    Refines,
    Traces,
    Type,
    DocRef,
    /// `style`, which enters the style state.
    Style,
    /// `classDef`, which enters the style state.
    ClassDef,
    /// `class`, which enters the style state.
    Class,
    EndArrowL,
    EndArrowR,
    Line,
    /// `\w+` in the style state and the `<*>` rule.
    Alpha,
    /// `:` in the style state (`COLON`).
    StyleColon,
    Semicolon,
    Percent,
    Minus,
    Brkt,
    UnqString,
    QStr,
    Num,
    Comma,
}

impl TokenKind {
    /// The terminal name upstream's parser reports in an error message.
    pub(crate) fn name(self) -> &'static str {
        match self {
            TokenKind::Title => "title",
            TokenKind::AccTitle => "acc_title",
            TokenKind::AccTitleValue => "acc_title_value",
            TokenKind::AccDescr => "acc_descr",
            TokenKind::AccDescrValue => "acc_descr_value",
            TokenKind::AccDescrMultilineValue => "acc_descr_multiline_value",
            TokenKind::DirectionTb => "direction_tb",
            TokenKind::DirectionBt => "direction_bt",
            TokenKind::DirectionRl => "direction_rl",
            TokenKind::DirectionLr => "direction_lr",
            TokenKind::Newline => "NEWLINE",
            TokenKind::Eof => "EOF",
            TokenKind::RequirementDiagram => "RD",
            TokenKind::StructStart => "STRUCT_START",
            TokenKind::StructStop => "STRUCT_STOP",
            TokenKind::StyleSeparator => "STYLE_SEPARATOR",
            TokenKind::ColonSep => "COLONSEP",
            TokenKind::Id => "ID",
            TokenKind::Text => "TEXT",
            TokenKind::Risk => "RISK",
            TokenKind::VerifyMethod => "VERIFY_MTHD",
            TokenKind::Requirement => "REQUIREMENT",
            TokenKind::FunctionalRequirement => "FUNCTIONAL_REQUIREMENT",
            TokenKind::InterfaceRequirement => "INTERFACE_REQUIREMENT",
            TokenKind::PerformanceRequirement => "PERFORMANCE_REQUIREMENT",
            TokenKind::PhysicalRequirement => "PHYSICAL_REQUIREMENT",
            TokenKind::DesignConstraint => "DESIGN_CONSTRAINT",
            TokenKind::LowRisk => "LOW_RISK",
            TokenKind::MediumRisk => "MED_RISK",
            TokenKind::HighRisk => "HIGH_RISK",
            TokenKind::VerifyAnalysis => "VERIFY_ANALYSIS",
            TokenKind::VerifyDemonstration => "VERIFY_DEMONSTRATION",
            TokenKind::VerifyInspection => "VERIFY_INSPECTION",
            TokenKind::VerifyTest => "VERIFY_TEST",
            TokenKind::Element => "ELEMENT",
            TokenKind::Contains => "CONTAINS",
            TokenKind::Copies => "COPIES",
            TokenKind::Derives => "DERIVES",
            TokenKind::Satisfies => "SATISFIES",
            TokenKind::Verifies => "VERIFIES",
            TokenKind::Refines => "REFINES",
            TokenKind::Traces => "TRACES",
            TokenKind::Type => "TYPE",
            TokenKind::DocRef => "DOCREF",
            TokenKind::Style => "STYLE",
            TokenKind::ClassDef => "CLASSDEF",
            TokenKind::Class => "CLASS",
            TokenKind::EndArrowL => "END_ARROW_L",
            TokenKind::EndArrowR => "END_ARROW_R",
            TokenKind::Line => "LINE",
            TokenKind::Alpha => "ALPHA",
            TokenKind::StyleColon => "COLON",
            TokenKind::Semicolon => "SEMICOLON",
            TokenKind::Percent => "PERCENT",
            TokenKind::Minus => "MINUS",
            TokenKind::Brkt => "BRKT",
            TokenKind::UnqString => "unqString",
            TokenKind::QStr => "qString",
            TokenKind::Num => "NUM",
            TokenKind::Comma => "COMMA",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Token {
    pub(crate) kind: TokenKind,
    /// The matched text: `unqString` is trimmed and `qString` has no quotes; other tokens
    /// carry the raw match.
    pub(crate) text: String,
    pub(crate) span: Span,
}

/// One exclusive start condition. `Initial` is the grammar's `INITIAL`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Initial,
    Style,
    Str,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
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
            states: vec![State::Initial],
        }
    }

    pub(crate) fn next_token(&mut self) -> Result<Token, ParseError> {
        loop {
            if self.offset >= self.source.len() {
                return match self.state() {
                    State::Initial => Ok(self.make_token(TokenKind::Eof, 0)),
                    // `[^\n]*` matches the empty string, returns its value, then pops.
                    State::AccTitle => {
                        self.pop_state();
                        Ok(self.make_token(TokenKind::AccTitleValue, 0))
                    }
                    State::AccDescr => {
                        self.pop_state();
                        Ok(self.make_token(TokenKind::AccDescrValue, 0))
                    }
                    // An unclosed style, string, or accessibility block only reaches jison's
                    // built-in EOF, which the grammar does not accept.
                    _ => Err(ParseError::at(
                        "unexpected end of input",
                        self.span_to(self.offset),
                    )),
                };
            }
            let state = self.state();
            let next = match state {
                State::Initial => self.initial_token()?,
                State::Style => self.style_token()?,
                State::Str => self.string_token()?,
                State::AccTitle | State::AccDescr => {
                    let source = self.source;
                    let rest = &source[self.offset..];
                    let len = line_len(rest);
                    let kind = if state == State::AccTitle {
                        TokenKind::AccTitleValue
                    } else {
                        TokenKind::AccDescrValue
                    };
                    self.pop_state();
                    Some(self.make_token(kind, len))
                }
                State::AccDescrMultiline => {
                    let source = self.source;
                    let rest = &source[self.offset..];
                    if rest.starts_with('}') {
                        self.advance(self.offset + 1);
                        self.pop_state();
                        None
                    } else {
                        let len = rest.find('}').unwrap_or(rest.len());
                        Some(self.make_token(TokenKind::AccDescrMultilineValue, len))
                    }
                }
            };
            if let Some(token) = next {
                return Ok(token);
            }
        }
    }

    fn state(&self) -> State {
        *self
            .states
            .last()
            .expect("the initial state is never popped")
    }

    fn push_state(&mut self, state: State) {
        self.states.push(state);
    }

    fn pop_state(&mut self) {
        if self.states.len() > 1 {
            self.states.pop();
        }
    }

    /// The rules of the `INITIAL` condition, in jison's order.
    fn initial_token(&mut self) -> Result<Option<Token>, ParseError> {
        let source = self.source;
        let rest = &source[self.offset..];

        if let Some(len) = title_len(rest) {
            return Ok(Some(self.make_token(TokenKind::Title, len)));
        }
        if let Some(len) = acc_prefix_len(rest, "accTitle") {
            let token = self.make_token(TokenKind::AccTitle, len);
            self.push_state(State::AccTitle);
            return Ok(Some(token));
        }
        if let Some(len) = acc_prefix_len(rest, "accDescr") {
            let token = self.make_token(TokenKind::AccDescr, len);
            self.push_state(State::AccDescr);
            return Ok(Some(token));
        }
        if let Some(len) = acc_brace_len(rest, "accDescr") {
            self.advance(self.offset + len);
            self.push_state(State::AccDescrMultiline);
            return Ok(None);
        }
        if let Some((kind, len)) = direction(rest) {
            return Ok(Some(self.make_token(kind, len)));
        }
        if let Some(len) = newline_len(rest) {
            return Ok(Some(self.make_token(TokenKind::Newline, len)));
        }
        if let Some(len) = js_space_len(rest) {
            self.advance(self.offset + len);
            return Ok(None);
        }
        if rest.starts_with('#') {
            self.advance(self.offset + line_len(rest));
            return Ok(None);
        }
        if rest.starts_with('%') {
            self.advance(self.offset + line_len(rest));
            return Ok(None);
        }

        if let Some(len) = keyword_len(rest, "requirementDiagram") {
            return Ok(Some(self.make_token(TokenKind::RequirementDiagram, len)));
        }
        if rest.starts_with('{') {
            return Ok(Some(self.make_token(TokenKind::StructStart, 1)));
        }
        if rest.starts_with('}') {
            return Ok(Some(self.make_token(TokenKind::StructStop, 1)));
        }
        if rest.starts_with(":::") {
            return Ok(Some(self.make_token(TokenKind::StyleSeparator, 3)));
        }
        if rest.starts_with(':') {
            return Ok(Some(self.make_token(TokenKind::ColonSep, 1)));
        }
        for (keyword, kind) in [
            ("id", TokenKind::Id),
            ("text", TokenKind::Text),
            ("risk", TokenKind::Risk),
            ("verifyMethod", TokenKind::VerifyMethod),
            ("requirement", TokenKind::Requirement),
            ("functionalRequirement", TokenKind::FunctionalRequirement),
            ("interfaceRequirement", TokenKind::InterfaceRequirement),
            ("performanceRequirement", TokenKind::PerformanceRequirement),
            ("physicalRequirement", TokenKind::PhysicalRequirement),
            ("designConstraint", TokenKind::DesignConstraint),
            ("low", TokenKind::LowRisk),
            ("medium", TokenKind::MediumRisk),
            ("high", TokenKind::HighRisk),
            ("analysis", TokenKind::VerifyAnalysis),
            ("demonstration", TokenKind::VerifyDemonstration),
            ("inspection", TokenKind::VerifyInspection),
            ("test", TokenKind::VerifyTest),
            ("element", TokenKind::Element),
            ("contains", TokenKind::Contains),
            ("copies", TokenKind::Copies),
            ("derives", TokenKind::Derives),
            ("satisfies", TokenKind::Satisfies),
            ("verifies", TokenKind::Verifies),
            ("refines", TokenKind::Refines),
            ("traces", TokenKind::Traces),
            ("type", TokenKind::Type),
            ("docref", TokenKind::DocRef),
        ] {
            if let Some(len) = keyword_len(rest, keyword) {
                return Ok(Some(self.make_token(kind, len)));
            }
        }
        if let Some(len) = keyword_len(rest, "style") {
            let token = self.make_token(TokenKind::Style, len);
            self.push_state(State::Style);
            return Ok(Some(token));
        }
        if let Some(len) = keyword_len(rest, "classDef") {
            let token = self.make_token(TokenKind::ClassDef, len);
            self.push_state(State::Style);
            return Ok(Some(token));
        }
        if let Some(len) = keyword_len(rest, "class") {
            let token = self.make_token(TokenKind::Class, len);
            self.push_state(State::Style);
            return Ok(Some(token));
        }
        if rest.starts_with("<-") {
            return Ok(Some(self.make_token(TokenKind::EndArrowL, 2)));
        }
        if rest.starts_with("->") {
            return Ok(Some(self.make_token(TokenKind::EndArrowR, 2)));
        }
        if rest.starts_with('-') {
            return Ok(Some(self.make_token(TokenKind::Line, 1)));
        }
        if rest.starts_with('"') {
            self.advance(self.offset + 1);
            self.push_state(State::Str);
            return Ok(None);
        }
        if let Some(len) = unquoted_len(rest) {
            return Ok(Some(self.make_token(TokenKind::UnqString, len)));
        }
        if let Some(len) = word_len(rest) {
            return Ok(Some(self.make_token(TokenKind::Alpha, len)));
        }
        if let Some(len) = digits_len(rest) {
            return Ok(Some(self.make_token(TokenKind::Num, len)));
        }
        if rest.starts_with(',') {
            return Ok(Some(self.make_token(TokenKind::Comma, 1)));
        }

        let ch = rest.chars().next().expect("non-empty remainder");
        Err(ParseError::at(
            format!("unexpected character '{ch}'"),
            self.span_to(self.offset),
        ))
    }

    /// The rules of the `style` condition, followed by the `<*>` rules.
    fn style_token(&mut self) -> Result<Option<Token>, ParseError> {
        let source = self.source;
        let rest = &source[self.offset..];

        if let Some(len) = word_len(rest) {
            return Ok(Some(self.make_token(TokenKind::Alpha, len)));
        }
        if rest.starts_with(':') {
            return Ok(Some(self.make_token(TokenKind::StyleColon, 1)));
        }
        if rest.starts_with(';') {
            return Ok(Some(self.make_token(TokenKind::Semicolon, 1)));
        }
        if rest.starts_with('%') {
            return Ok(Some(self.make_token(TokenKind::Percent, 1)));
        }
        if rest.starts_with('-') {
            return Ok(Some(self.make_token(TokenKind::Minus, 1)));
        }
        if rest.starts_with('#') {
            return Ok(Some(self.make_token(TokenKind::Brkt, 1)));
        }
        if rest.starts_with(' ') {
            self.advance(self.offset + 1);
            return Ok(None);
        }
        if rest.starts_with('"') {
            self.advance(self.offset + 1);
            self.push_state(State::Str);
            return Ok(None);
        }
        if rest.starts_with('\n') {
            self.advance(self.offset + 1);
            self.pop_state();
            return Ok(None);
        }
        if let Some(len) = digits_len(rest) {
            return Ok(Some(self.make_token(TokenKind::Num, len)));
        }
        if rest.starts_with(',') {
            return Ok(Some(self.make_token(TokenKind::Comma, 1)));
        }

        let ch = rest.chars().next().expect("non-empty remainder");
        Err(ParseError::at(
            format!("unexpected character '{ch}'"),
            self.span_to(self.offset),
        ))
    }

    /// The `string` condition: a closing quote pops, everything else is `qString`.
    fn string_token(&mut self) -> Result<Option<Token>, ParseError> {
        let source = self.source;
        let rest = &source[self.offset..];

        if rest.starts_with('"') {
            self.advance(self.offset + 1);
            self.pop_state();
            return Ok(None);
        }
        let len = rest.find('"').unwrap_or(rest.len());
        if len == 0 {
            return Err(ParseError::at(
                "unexpected character '\"'",
                self.span_to(self.offset),
            ));
        }
        Ok(Some(self.make_token(TokenKind::QStr, len)))
    }

    fn make_token(&mut self, kind: TokenKind, len: usize) -> Token {
        let start = self.offset;
        let end = start + len;
        let span = self.span_to(end);
        let raw = &self.source[start..end];
        let text = if kind == TokenKind::UnqString {
            raw.trim_matches(is_js_whitespace).to_string()
        } else {
            raw.to_string()
        };
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

/// `^(?:keyword)\b`, case-insensitive as `%options case-insensitive` makes it.
fn keyword_len(text: &str, keyword: &str) -> Option<usize> {
    if !starts_with_ci(text, keyword) {
        return None;
    }
    let rest = text.get(keyword.len()..)?;
    rest.chars()
        .next()
        .is_none_or(|ch| !is_word_char(ch))
        .then_some(keyword.len())
}

/// `^(?:keyword)`, case-insensitive and without a word boundary.
fn starts_with_ci(text: &str, keyword: &str) -> bool {
    text.as_bytes()
        .get(..keyword.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(keyword.as_bytes()))
}

/// `"title"\s[^#\n;]+`.
fn title_len(text: &str) -> Option<usize> {
    if !starts_with_ci(text, "title") {
        return None;
    }
    let after = &text["title".len()..];
    let space = after.chars().next()?;
    if !is_js_whitespace(space) {
        return None;
    }
    let content = "title".len() + space.len_utf8();
    let mut end = content;
    for (index, ch) in text[content..].char_indices() {
        if ch == '#' || ch == '\n' || ch == ';' {
            break;
        }
        end = content + index + ch.len_utf8();
    }
    (end > content).then_some(end)
}

/// `accTitle`/`accDescr` followed by `\s*` `:` `\s*`.
fn acc_prefix_len(text: &str, keyword: &str) -> Option<usize> {
    if !starts_with_ci(text, keyword) {
        return None;
    }
    let mut pos = skip_js_whitespace(text, keyword.len());
    if !text[pos..].starts_with(':') {
        return None;
    }
    pos += 1;
    Some(skip_js_whitespace(text, pos))
}

/// `accDescr\s*"{"\s*`.
fn acc_brace_len(text: &str, keyword: &str) -> Option<usize> {
    if !starts_with_ci(text, keyword) {
        return None;
    }
    let mut pos = skip_js_whitespace(text, keyword.len());
    if !text[pos..].starts_with('{') {
        return None;
    }
    pos += 1;
    Some(skip_js_whitespace(text, pos))
}

/// `^.*direction\s+TB[^\n]*` (and the other three directions), in rule order.
///
/// `.*` stops at a line terminator, so the word `direction` must be on the current line;
/// the `\s+` after it may cross line breaks, and `[^\n]*` then runs to the end of the
/// direction's line. Each rule searches for the rightmost `direction` it can satisfy.
fn direction(text: &str) -> Option<(TokenKind, usize)> {
    let line_end = text
        .find(['\n', '\r', '\u{2028}', '\u{2029}'])
        .unwrap_or(text.len());
    let line = &text[..line_end];
    let mut candidates = Vec::new();
    for (index, _) in line.char_indices() {
        if starts_with_ci(&line[index..], "direction") {
            candidates.push(index);
        }
    }
    for (word, kind) in [
        ("TB", TokenKind::DirectionTb),
        ("BT", TokenKind::DirectionBt),
        ("RL", TokenKind::DirectionRl),
        ("LR", TokenKind::DirectionLr),
    ] {
        for &start in candidates.iter().rev() {
            let mut pos = start + "direction".len();
            match text[pos..].chars().next() {
                Some(ch) if is_js_whitespace(ch) => {}
                _ => continue,
            }
            pos = skip_js_whitespace(text, pos);
            if starts_with_ci(&text[pos..], word) {
                let end = text[pos..]
                    .find('\n')
                    .map_or(text.len(), |offset| pos + offset);
                return Some((kind, end));
            }
        }
    }
    None
}

/// `(\r?\n)+`.
fn newline_len(text: &str) -> Option<usize> {
    let mut pos = 0;
    loop {
        if text[pos..].starts_with("\r\n") {
            pos += 2;
        } else if text[pos..].starts_with('\n') {
            pos += 1;
        } else {
            break;
        }
    }
    (pos > 0).then_some(pos)
}

/// `\s+`.
fn js_space_len(text: &str) -> Option<usize> {
    let end = text
        .char_indices()
        .find(|&(_, ch)| !is_js_whitespace(ch))
        .map_or(text.len(), |(index, _)| index);
    (end > 0).then_some(end)
}

/// `\w+`.
fn word_len(text: &str) -> Option<usize> {
    let end = text
        .char_indices()
        .find(|&(_, ch)| !is_word_char(ch))
        .map_or(text.len(), |(index, _)| index);
    (end > 0).then_some(end)
}

/// `[0-9]+`.
fn digits_len(text: &str) -> Option<usize> {
    let end = text
        .bytes()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    (end > 0).then_some(end)
}

/// `[\w][^:,\r\n\{\<\>\-\=]*`.
fn unquoted_len(text: &str) -> Option<usize> {
    let mut chars = text.char_indices();
    let (_, first) = chars.next()?;
    if !is_word_char(first) {
        return None;
    }
    let mut end = first.len_utf8();
    for (index, ch) in chars {
        if matches!(ch, ':' | ',' | '\r' | '\n' | '{' | '<' | '>' | '-' | '=') {
            break;
        }
        end = index + ch.len_utf8();
    }
    Some(end)
}

/// `[^\n]*`.
fn line_len(text: &str) -> usize {
    text.find('\n').unwrap_or(text.len())
}

fn skip_js_whitespace(text: &str, mut pos: usize) -> usize {
    while let Some(ch) = text[pos..].chars().next() {
        if !is_js_whitespace(ch) {
            break;
        }
        pos += ch.len_utf8();
    }
    pos
}

fn is_word_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}
