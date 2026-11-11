//! The xychart lexer: a port of `parser/xychart.jison`'s `%lex` section, including its start
//! conditions (`%x string`, `%x md_string`, `%x acc_title`, `%x acc_descr`,
//! `%x acc_descr_multiline`, `%s axis_data`, `%s axis_band_data`, `%s data`, `%s data_inner`)
//! and its `%options case-insensitive`.
//!
//! Jison tries every active rule at the current offset and keeps the **longest** match, the
//! earliest rule winning a tie, so the order of the rules below is observable: `xychart-beta`
//! before `xychart`, `"title"` before `[A-Za-z]+`, the state-specific rule before the general
//! one it duplicates. Rules declared without a start condition are active in `INITIAL` and in
//! the inclusive (`%s`) states; the exclusive (`%x`) states only run their own rules. The
//! `%x title` state has no rules upstream and is not modelled.
//!
//! Two upstream quirks are reproduced on purpose:
//!
//! * `\s+` can swallow a newline together with the spaces before it, in which case the
//!   newline token that would pop `axis_data` or `data` is never produced.
//! * In `axis_data`, a digit run is `NUMBER_WITH_DECIMAL`, so an unquoted axis title cannot
//!   contain a digit; band categories and plot data run in inclusive states where the general
//!   `NUM` rule applies instead, so `cat2` is `cat` + `2`.

use crate::error::{ParseError, Span};
use crate::preprocess::is_js_whitespace;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    /// `XYCHART`, from `xychart-beta` or `xychart`.
    Xychart,
    /// `CHART_ORIENTATION`, from `vertical` or `horizontal`.
    ChartOrientation,
    XAxis,
    YAxis,
    /// `SQUARE_BRACES_START`.
    SquareBracesStart,
    /// `SQUARE_BRACES_END`.
    SquareBracesEnd,
    ArrowDelimiter,
    Line,
    Bar,
    /// `NUMBER_WITH_DECIMAL`, the only decimal token the grammar accepts as a value.
    Number,
    /// `ALPHA`.
    Alpha,
    Colon,
    Plus,
    Comma,
    Equals,
    Mult,
    /// `BRKT`, from `#`.
    Brkt,
    Underscore,
    Dot,
    /// `AMP`, from `&`.
    Amp,
    Minus,
    /// `NUM`, a digit run in a state that has no `NUMBER_WITH_DECIMAL`.
    Num,
    Semi,
    Title,
    AccTitle,
    AccTitleValue,
    AccDescr,
    AccDescrValue,
    AccDescrMultilineValue,
    /// `STR`, a `"`-quoted string.
    Str,
    /// `MD_STR`, the text between a `"`` opener and a ``"` closer.
    MdStr,
    Newline,
    Eof,
}

impl TokenKind {
    /// The token name upstream's parser prints in an error.
    pub(crate) fn name(self) -> &'static str {
        match self {
            TokenKind::Xychart => "XYCHART",
            TokenKind::ChartOrientation => "CHART_ORIENTATION",
            TokenKind::XAxis => "X_AXIS",
            TokenKind::YAxis => "Y_AXIS",
            TokenKind::SquareBracesStart => "SQUARE_BRACES_START",
            TokenKind::SquareBracesEnd => "SQUARE_BRACES_END",
            TokenKind::ArrowDelimiter => "ARROW_DELIMITER",
            TokenKind::Line => "LINE",
            TokenKind::Bar => "BAR",
            TokenKind::Number => "NUMBER_WITH_DECIMAL",
            TokenKind::Alpha => "ALPHA",
            TokenKind::Colon => "COLON",
            TokenKind::Plus => "PLUS",
            TokenKind::Comma => "COMMA",
            TokenKind::Equals => "EQUALS",
            TokenKind::Mult => "MULT",
            TokenKind::Brkt => "BRKT",
            TokenKind::Underscore => "UNDERSCORE",
            TokenKind::Dot => "DOT",
            TokenKind::Amp => "AMP",
            TokenKind::Minus => "MINUS",
            TokenKind::Num => "NUM",
            TokenKind::Semi => "SEMI",
            TokenKind::Title => "title",
            TokenKind::AccTitle => "acc_title",
            TokenKind::AccTitleValue => "acc_title_value",
            TokenKind::AccDescr => "acc_descr",
            TokenKind::AccDescrValue => "acc_descr_value",
            TokenKind::AccDescrMultilineValue => "acc_descr_multiline_value",
            TokenKind::Str => "STR",
            TokenKind::MdStr => "MD_STR",
            TokenKind::Newline => "NEWLINE",
            TokenKind::Eof => "EOF",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Token {
    pub(crate) kind: TokenKind,
    /// The matched text; empty for [`TokenKind::Eof`].
    pub(crate) text: String,
    pub(crate) span: Span,
}

/// A `%lex` start condition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Initial,
    String,
    MdString,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
    AxisData,
    AxisBandData,
    Data,
    DataInner,
}

impl State {
    /// `%s` states also run the rules declared without a start condition.
    fn is_inclusive(self) -> bool {
        matches!(
            self,
            State::Initial | State::AxisData | State::AxisBandData | State::Data | State::DataInner
        )
    }
}

/// What a rule does after its text is consumed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    /// The action runs `this.pushState` and returns nothing.
    Push(State),
    /// `this.popState()` and return nothing.
    Pop,
    /// `this.pushState` and return a token.
    PushToken(State, TokenKind),
    /// `this.popState()` and return a token.
    PopToken(TokenKind),
    /// A token without a state change.
    Token(TokenKind),
    /// A rule whose action produces no token (comments and whitespace).
    Skip,
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
                return Ok(Token {
                    kind: TokenKind::Eof,
                    text: String::new(),
                    span: self.span_to(self.offset),
                });
            }
            let rest = &self.source[self.offset..];
            let state = *self.states.last().expect("the stack starts with INITIAL");
            let Some((length, action)) = rule(rest, state) else {
                let ch = rest.chars().next().expect("non-empty remainder");
                return Err(ParseError::at(
                    format!("unexpected character '{ch}'"),
                    self.span_to(self.offset),
                ));
            };
            if length == 0 && !matches!(action, Action::Pop | Action::PopToken(_)) {
                let ch = rest.chars().next().expect("non-empty remainder");
                return Err(ParseError::at(
                    format!("unexpected character '{ch}'"),
                    self.span_to(self.offset),
                ));
            }
            let start = self.offset;
            let end = start + length;
            let text = self.source[start..end].to_string();
            let span = self.span_to(end);
            self.advance(end);
            match action {
                Action::Skip => continue,
                Action::Push(state) => {
                    self.states.push(state);
                    continue;
                }
                Action::Pop => {
                    self.pop_state();
                    continue;
                }
                Action::PushToken(state, kind) => {
                    self.states.push(state);
                    return Ok(Token { kind, text, span });
                }
                Action::PopToken(kind) => {
                    self.pop_state();
                    return Ok(Token { kind, text, span });
                }
                Action::Token(kind) => {
                    return Ok(Token { kind, text, span });
                }
            }
        }
    }

    /// `popState` never pops the last state.
    fn pop_state(&mut self) {
        if self.states.len() > 1 {
            self.states.pop();
        }
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

type Match = (usize, Action);

/// The longest match among the rules active in `state`, the earliest rule winning a tie.
fn rule(rest: &str, state: State) -> Option<Match> {
    let mut best: Option<Match> = None;
    let mut consider = |length: Option<usize>, action: Action| {
        let Some(length) = length else { return };
        if best.is_none_or(|(best_length, _)| length > best_length) {
            best = Some((length, action));
        }
    };
    let general = state.is_inclusive();

    // The two comment rules and the state-specific newline rules come first.
    if general {
        consider(comment_not_directive(rest), Action::Skip);
        consider(comment_after_character(rest), Action::Skip);
    }
    if state == State::AxisData {
        consider(state_newline(rest), Action::PopToken(TokenKind::Newline));
    }
    if state == State::Data {
        consider(state_newline(rest), Action::PopToken(TokenKind::Newline));
    }
    if general {
        consider(newlines(rest), Action::Token(TokenKind::Newline));
        consider(comment(rest), Action::Skip);
        consider(literal(rest, "title"), Action::Token(TokenKind::Title));
        consider(
            accessibility(rest, "accTitle"),
            Action::PushToken(State::AccTitle, TokenKind::AccTitle),
        );
    }
    if state == State::AccTitle {
        consider(
            rest_of_line(rest),
            Action::PopToken(TokenKind::AccTitleValue),
        );
    }
    if general {
        consider(
            accessibility(rest, "accDescr"),
            Action::PushToken(State::AccDescr, TokenKind::AccDescr),
        );
    }
    if state == State::AccDescr {
        consider(
            rest_of_line(rest),
            Action::PopToken(TokenKind::AccDescrValue),
        );
    }
    if general {
        consider(
            accessibility_block(rest, "accDescr"),
            Action::Push(State::AccDescrMultiline),
        );
    }
    if state == State::AccDescrMultiline {
        consider(one_char(rest, '}'), Action::Pop);
        consider(
            until_brace(rest),
            Action::Token(TokenKind::AccDescrMultilineValue),
        );
    }
    if general {
        consider(
            literal(rest, "xychart-beta"),
            Action::Token(TokenKind::Xychart),
        );
        consider(literal(rest, "xychart"), Action::Token(TokenKind::Xychart));
        consider(
            orientation(rest),
            Action::Token(TokenKind::ChartOrientation),
        );
        consider(
            literal(rest, "x-axis"),
            Action::PushToken(State::AxisData, TokenKind::XAxis),
        );
        consider(
            literal(rest, "y-axis"),
            Action::PushToken(State::AxisData, TokenKind::YAxis),
        );
    }
    if state == State::AxisData {
        consider(
            one_char(rest, '['),
            Action::PushToken(State::AxisBandData, TokenKind::SquareBracesStart),
        );
        consider(
            literal(rest, "-->"),
            Action::Token(TokenKind::ArrowDelimiter),
        );
    }
    if general {
        consider(
            literal(rest, "line"),
            Action::PushToken(State::Data, TokenKind::Line),
        );
        consider(
            literal(rest, "bar"),
            Action::PushToken(State::Data, TokenKind::Bar),
        );
    }
    if state == State::Data {
        consider(
            one_char(rest, '['),
            Action::PushToken(State::DataInner, TokenKind::SquareBracesStart),
        );
    }
    if state == State::AxisData || state == State::DataInner {
        consider(number(rest), Action::Token(TokenKind::Number));
    }
    if state == State::DataInner || state == State::AxisBandData {
        consider(
            one_char(rest, ']'),
            Action::PopToken(TokenKind::SquareBracesEnd),
        );
    }
    if general {
        consider(md_string_open(rest), Action::Push(State::MdString));
    }
    if state == State::MdString {
        consider(md_string_text(rest), Action::Token(TokenKind::MdStr));
        consider(md_string_close(rest), Action::Pop);
    }
    if general {
        consider(one_char(rest, '"'), Action::Push(State::String));
    }
    if state == State::String {
        consider(one_char(rest, '"'), Action::Pop);
        consider(string_text(rest), Action::Token(TokenKind::Str));
    }
    if general {
        consider(
            one_char(rest, '['),
            Action::Token(TokenKind::SquareBracesStart),
        );
        consider(
            one_char(rest, ']'),
            Action::Token(TokenKind::SquareBracesEnd),
        );
        consider(alpha(rest), Action::Token(TokenKind::Alpha));
        consider(one_char(rest, ':'), Action::Token(TokenKind::Colon));
        consider(one_char(rest, '+'), Action::Token(TokenKind::Plus));
        consider(one_char(rest, ','), Action::Token(TokenKind::Comma));
        consider(one_char(rest, '='), Action::Token(TokenKind::Equals));
        consider(one_char(rest, '*'), Action::Token(TokenKind::Mult));
        consider(one_char(rest, '#'), Action::Token(TokenKind::Brkt));
        consider(one_char(rest, '_'), Action::Token(TokenKind::Underscore));
        consider(one_char(rest, '.'), Action::Token(TokenKind::Dot));
        consider(one_char(rest, '&'), Action::Token(TokenKind::Amp));
        consider(one_char(rest, '-'), Action::Token(TokenKind::Minus));
        consider(digits(rest), Action::Token(TokenKind::Num));
        consider(js_spaces(rest), Action::Skip);
        consider(one_char(rest, ';'), Action::Token(TokenKind::Semi));
    }
    best
}

/// `(?:"vertical"|"horizontal")`.
fn orientation(rest: &str) -> Option<usize> {
    if literal(rest, "horizontal").is_some() {
        Some(10)
    } else if literal(rest, "vertical").is_some() {
        Some(8)
    } else {
        None
    }
}

/// A rule that matches one character.
fn one_char(rest: &str, expected: char) -> Option<usize> {
    rest.starts_with(expected).then_some(expected.len_utf8())
}

/// Case-insensitive literal, the way `%options case-insensitive` compiles string rules.
fn literal(rest: &str, word: &str) -> Option<usize> {
    let head = rest.get(..word.len())?;
    head.eq_ignore_ascii_case(word).then_some(word.len())
}

/// `\%\%(?!\{)[^\n]*`: a `%%` comment that is not a directive.
fn comment_not_directive(rest: &str) -> Option<usize> {
    let after = rest.strip_prefix("%%")?;
    if after.starts_with('{') {
        return None;
    }
    Some(2 + line_length(after))
}

/// `[^\}]\%\%[^\n]*`: an inline comment introduced by `%%`.
fn comment_after_character(rest: &str) -> Option<usize> {
    let first = rest.chars().next()?;
    if first == '}' {
        return None;
    }
    let after = &rest[first.len_utf8()..];
    let tail = after.strip_prefix("%%")?;
    Some(first.len_utf8() + 2 + line_length(tail))
}

/// `\%\%[^\n]*`: the directive form the preprocessing removes, here consumed quietly.
fn comment(rest: &str) -> Option<usize> {
    rest.strip_prefix("%%")?;
    Some(2 + line_length(&rest[2..]))
}

/// `<axis_data>(\r?\n)` and `<data>(\r?\n)`.
fn state_newline(rest: &str) -> Option<usize> {
    if rest.starts_with("\r\n") {
        Some(2)
    } else if rest.starts_with('\n') {
        Some(1)
    } else {
        None
    }
}

/// `[\n\r]+`.
fn newlines(rest: &str) -> Option<usize> {
    let length = rest
        .bytes()
        .take_while(|byte| *byte == b'\n' || *byte == b'\r')
        .count();
    (length > 0).then_some(length)
}

/// `"accTitle"\s*":"\s*` and `"accDescr"\s*":"\s*`.
fn accessibility(rest: &str, word: &str) -> Option<usize> {
    let mut length = literal(rest, word)?;
    length += js_spaces(&rest[length..]).unwrap_or(0);
    if !rest[length..].starts_with(':') {
        return None;
    }
    length += 1;
    length += js_spaces(&rest[length..]).unwrap_or(0);
    Some(length)
}

/// `"accDescr"\s*"{"\s*`.
fn accessibility_block(rest: &str, word: &str) -> Option<usize> {
    let mut length = literal(rest, word)?;
    length += js_spaces(&rest[length..]).unwrap_or(0);
    if !rest[length..].starts_with('{') {
        return None;
    }
    length += 1;
    length += js_spaces(&rest[length..]).unwrap_or(0);
    Some(length)
}

/// `(?!\n|;|#)*[^\n]*` in the accessibility value states: the rest of the line.
fn rest_of_line(rest: &str) -> Option<usize> {
    Some(rest.find('\n').unwrap_or(rest.len()))
}

/// `<acc_descr_multiline>[^\}]*`.
fn until_brace(rest: &str) -> Option<usize> {
    Some(rest.find('}').unwrap_or(rest.len()))
}

/// `[+-]?(?:\d+(?:\.\d+)?|\.\d+)`.
fn number(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    let mut index = usize::from(matches!(bytes.first(), Some(b'+') | Some(b'-')));
    let integer_start = index;
    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    if index > integer_start {
        if bytes.get(index) == Some(&b'.') {
            let mut fraction_end = index + 1;
            while bytes.get(fraction_end).is_some_and(u8::is_ascii_digit) {
                fraction_end += 1;
            }
            if fraction_end > index + 1 {
                return Some(fraction_end);
            }
        }
        return Some(index);
    }
    if bytes.get(index) == Some(&b'.') {
        let mut fraction_end = index + 1;
        while bytes.get(fraction_end).is_some_and(u8::is_ascii_digit) {
            fraction_end += 1;
        }
        if fraction_end > index + 1 {
            return Some(fraction_end);
        }
    }
    None
}

/// `(?:"`)`: a `"` followed by a backtick opens a markdown string.
fn md_string_open(rest: &str) -> Option<usize> {
    rest.starts_with("\"`").then_some(2)
}

/// `<md_string>(?:(?!`\").)+`: everything up to the backtick-quote closer, at least one char.
fn md_string_text(rest: &str) -> Option<usize> {
    let length = rest.find("`\"").unwrap_or(rest.len());
    (length > 0).then_some(length)
}

/// `<md_string>(?:`")`.
fn md_string_close(rest: &str) -> Option<usize> {
    rest.starts_with("`\"").then_some(2)
}

/// `<string>[^"]*`.
fn string_text(rest: &str) -> Option<usize> {
    Some(rest.find('"').unwrap_or(rest.len()))
}

/// `[A-Za-z]+`.
fn alpha(rest: &str) -> Option<usize> {
    let length = rest.bytes().take_while(u8::is_ascii_alphabetic).count();
    (length > 0).then_some(length)
}

/// `[0-9]+`.
fn digits(rest: &str) -> Option<usize> {
    let length = rest.bytes().take_while(u8::is_ascii_digit).count();
    (length > 0).then_some(length)
}

/// `\s+`, JavaScript's whitespace set.
fn js_spaces(rest: &str) -> Option<usize> {
    let length = rest
        .char_indices()
        .find(|&(_, ch)| !is_js_whitespace(ch))
        .map_or(rest.len(), |(index, _)| index);
    (length > 0).then_some(length)
}

/// The length of `[^\n]*` at the start of `text`.
fn line_length(text: &str) -> usize {
    text.find('\n').unwrap_or(text.len())
}
