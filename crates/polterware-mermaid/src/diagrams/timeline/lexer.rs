//! The timeline lexer: a hand port of the `%lex` section of upstream's
//! `diagrams/timeline/parser/timeline.jison`.
//!
//! Jison tries its rules in source order and takes the first that matches: the grammar does
//! not ask for `flex`'s longest match. Every branch of [`initial`] is one rule of the jison
//! file in that order, with the line it comes from. `%x` declares exclusive start
//! conditions, so in `acc_title`, `acc_descr`, and `acc_descr_multiline` only the
//! state-prefixed rules apply, and a rule without a prefix applies only in `INITIAL`.
//!
//! Three engine details are reproduced because they are observable:
//!
//! * jison-lex appends `\b` to a rule whose pattern ends in a word character. Only the three
//!   `timeline` rules end that way, so `timelineTD` is a `period` (a syntax error where the
//!   header is expected) and `timeline LRx` is the bare header followed by a period `LRx`.
//! * `%options case-insensitive` applies to every rule, including the keywords.
//! * the grammar's `EOF` token (the `$` rule) and the parser's own `$end` are different
//!   tokens. Upstream's `next()` returns `$end` once the input was already empty when a call
//!   started, so when the last token is the zero-width accessibility value at the end of the
//!   input, the `EOF` token is never delivered and the parse fails. [`Kind::EndOfInput`]
//!   keeps that distinction and [`super::parser`] rejects it.
//!
//! The `(?!\n|;|#)*` prefix of the `acc_title`/`acc_descr` value rules is a quantified
//! zero-width lookahead: it matches the empty string and constrains nothing, so a value is
//! the rest of the line. The `SPACE` terminal the grammar's `line` rule mentions is never
//! returned by any lexer rule, so no token for it exists here.

use crate::error::Span;
use crate::preprocess::is_js_whitespace;

/// The jison tokens that survive into the grammar, plus the two end-of-input markers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    /// `timeline` followed by `LR`.
    TimelineLr,
    /// `timeline` followed by `TD`.
    TimelineTd,
    /// `timeline`.
    Timeline,
    /// `title` and the rest of its line.
    Title,
    /// `accTitle` and the whitespace around its colon, which enters `acc_title`.
    AccTitle,
    /// The `acc_title` value, the rest of the line.
    AccTitleValue,
    /// `accDescr` and the whitespace around its colon, which enters `acc_descr`.
    AccDescr,
    /// The `acc_descr` value, the rest of the line.
    AccDescrValue,
    /// The text between `accDescr {` and its closing `}`.
    AccDescrMultilineValue,
    /// `section` and its label.
    Section,
    /// A `: event` token.
    Event,
    /// A task's period text.
    Period,
    /// A run of line feeds.
    Newline,
    /// The `.` rule, which only a colon no `event` rule accepts can reach.
    Invalid,
    /// The `$` rule, the end of the input the grammar calls `EOF`.
    Eof,
    /// The parser's `$end`, answered instead of [`Kind::Eof`] once a zero-width token was
    /// built at the end of the input.
    EndOfInput,
}

impl Kind {
    /// The token name upstream's generated parser prints in an error.
    pub(super) fn name(self) -> &'static str {
        match self {
            Kind::TimelineLr => "timeline_lr",
            Kind::TimelineTd => "timeline_td",
            Kind::Timeline => "timeline",
            Kind::Title => "title",
            Kind::AccTitle => "acc_title",
            Kind::AccTitleValue => "acc_title_value",
            Kind::AccDescr => "acc_descr",
            Kind::AccDescrValue => "acc_descr_value",
            Kind::AccDescrMultilineValue => "acc_descr_multiline_value",
            Kind::Section => "section",
            Kind::Event => "event",
            Kind::Period => "period",
            Kind::Newline => "NEWLINE",
            Kind::Invalid => "INVALID",
            Kind::Eof => "EOF",
            Kind::EndOfInput => "$end",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Token {
    pub(super) kind: Kind,
    /// The matched text, empty for [`Kind::Eof`] and [`Kind::EndOfInput`].
    pub(super) text: String,
    pub(super) span: Span,
}

/// A lexer start condition (`%x` in the jison file).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Initial,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
}

pub(super) struct Lexer<'a> {
    src: &'a str,
    offset: usize,
    line: u32,
    column: u32,
    states: Vec<State>,
    /// jison-lex's `done`: set when a call starts on empty input, before the rules run.
    done: bool,
}

impl<'a> Lexer<'a> {
    pub(super) fn new(src: &'a str) -> Self {
        Self {
            src,
            offset: 0,
            line: 1,
            column: 1,
            states: vec![State::Initial],
            done: false,
        }
    }

    pub(super) fn next_token(&mut self) -> Token {
        loop {
            let rest = &self.src[self.offset..];
            if rest.is_empty() {
                if self.done {
                    return self.finish(Kind::EndOfInput, 0);
                }
                self.done = true;
            }
            match self.state() {
                State::Initial => {
                    if rest.is_empty() {
                        // The `$` rule (line 38) runs before `.`.
                        return self.finish(Kind::Eof, 0);
                    }
                    let candidate = initial(rest);
                    match candidate.action {
                        Action::Skip => {
                            self.advance(self.offset + candidate.len);
                            continue;
                        }
                        Action::Token(kind) => return self.finish(kind, candidate.len),
                        Action::Begin(state, token) => {
                            self.states.push(state);
                            match token {
                                Some(kind) => return self.finish(kind, candidate.len),
                                None => {
                                    self.advance(self.offset + candidate.len);
                                    continue;
                                }
                            }
                        }
                    }
                }
                State::AccTitle => {
                    let token = self.finish(Kind::AccTitleValue, line_run(rest));
                    self.pop();
                    return token;
                }
                State::AccDescr => {
                    let token = self.finish(Kind::AccDescrValue, line_run(rest));
                    self.pop();
                    return token;
                }
                State::AccDescrMultiline => {
                    if rest.starts_with('}') {
                        self.advance(self.offset + 1);
                        self.pop();
                        continue;
                    }
                    // `[^}]*` runs to the next `}` (or the end of the input) and does not
                    // leave the state; the `}` rule above does that.
                    return self.finish(Kind::AccDescrMultilineValue, until_close_brace(rest));
                }
            }
        }
    }

    fn state(&self) -> State {
        *self.states.last().expect("the stack starts with INITIAL")
    }

    /// `popState()`: the base `INITIAL` state is never popped.
    fn pop(&mut self) {
        if self.states.len() > 1 {
            self.states.pop();
        }
    }

    fn finish(&mut self, kind: Kind, len: usize) -> Token {
        let end = self.offset + len;
        let span = Span {
            line: self.line,
            column: self.column,
            start: self.offset,
            end,
        };
        let text = self.src[self.offset..end].to_string();
        self.advance(end);
        Token { kind, text, span }
    }

    fn advance(&mut self, end: usize) {
        for ch in self.src[self.offset..end].chars() {
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

enum Action {
    /// A rule whose action only consumes text (whitespace or a comment).
    Skip,
    Token(Kind),
    /// `begin(state)`, optionally returning a token first.
    Begin(State, Option<Kind>),
}

struct Candidate {
    len: usize,
    action: Action,
}

impl Candidate {
    fn skip(len: usize) -> Self {
        Self {
            len,
            action: Action::Skip,
        }
    }

    fn token(kind: Kind, len: usize) -> Self {
        Self {
            len,
            action: Action::Token(kind),
        }
    }

    fn begin(state: State, kind: Option<Kind>, len: usize) -> Self {
        Self {
            len,
            action: Action::Begin(state, kind),
        }
    }
}

/// The `INITIAL` rules in jison source order; the first match wins.
fn initial(rest: &str) -> Candidate {
    // line 14: `\%%(?!\{)[^\n]*`
    if let Some(len) = percent_comment(rest) {
        return Candidate::skip(len);
    }
    // line 15: `[^\}]\%\%[^\n]*`
    if let Some(len) = inline_percent_comment(rest) {
        return Candidate::skip(len);
    }
    // line 16: `[\n]+`
    if let Some(len) = newlines(rest) {
        return Candidate::token(Kind::Newline, len);
    }
    // line 17: `\s+`
    if let Some(len) = whitespace(rest) {
        return Candidate::skip(len);
    }
    // line 18: `\#[^\n]*`
    if let Some(len) = hash_comment(rest) {
        return Candidate::skip(len);
    }
    // line 20: `"timeline"[ \t]+LR`
    if let Some(len) = timeline_direction(rest, "LR") {
        return Candidate::token(Kind::TimelineLr, len);
    }
    // line 21: `"timeline"[ \t]+TD`
    if let Some(len) = timeline_direction(rest, "TD") {
        return Candidate::token(Kind::TimelineTd, len);
    }
    // line 22: `"timeline"`
    if let Some(len) = timeline(rest) {
        return Candidate::token(Kind::Timeline, len);
    }
    // line 23: `"title"\s[^\n]+`
    if let Some(len) = title(rest) {
        return Candidate::token(Kind::Title, len);
    }
    // line 24: `accTitle\s*":"\s*`
    if let Some(len) = accessibility_keyword(rest, "accTitle", ':') {
        return Candidate::begin(State::AccTitle, Some(Kind::AccTitle), len);
    }
    // line 26: `accDescr\s*":"\s*`
    if let Some(len) = accessibility_keyword(rest, "accDescr", ':') {
        return Candidate::begin(State::AccDescr, Some(Kind::AccDescr), len);
    }
    // line 28: `accDescr\s*"{"\s*`
    if let Some(len) = accessibility_keyword(rest, "accDescr", '{') {
        return Candidate::begin(State::AccDescrMultiline, None, len);
    }
    // line 31: `"section"\s[^:\n]+`
    if let Some(len) = section(rest) {
        return Candidate::token(Kind::Section, len);
    }
    // line 34: `":"\s(?:[^:\n]|":"(?!\s))+`
    if let Some(len) = event(rest) {
        return Candidate::token(Kind::Event, len);
    }
    // line 35: `[^#:\n]+`
    if let Some(len) = period(rest) {
        return Candidate::token(Kind::Period, len);
    }
    // line 39: `.` (every line terminator was already taken by the whitespace rules).
    let len = rest
        .chars()
        .next()
        .expect("caller checked for empty")
        .len_utf8();
    Candidate::token(Kind::Invalid, len)
}

/// The length of `[^\n]*` at the start of `text`.
fn line_run(text: &str) -> usize {
    text.find('\n').unwrap_or(text.len())
}

/// The length of `[^}]*` at the start of `text`.
fn until_close_brace(text: &str) -> usize {
    text.find('}').unwrap_or(text.len())
}

/// The `\%` of `\%%(?!\{)[^\n]*` compiles to a single `%` in jison-lex's generated regex, so
/// the rule is one percent not followed by `{`, then the rest of the line. A `%%{...}%%`
/// line therefore starts with a `%` whose next character is `%`, and the whole line is a
/// comment.
fn percent_comment(rest: &str) -> Option<usize> {
    rest.strip_prefix('%')?;
    let after = &rest[1..];
    if after.starts_with('{') {
        return None;
    }
    Some(1 + line_run(after))
}

/// `[^\}]\%\%[^\n]*`: one character that is not `}`, then `%%`, then the rest of the line.
/// Starting one character early is why a `%%` comment also eats the line break before it.
fn inline_percent_comment(rest: &str) -> Option<usize> {
    let mut chars = rest.char_indices();
    let (_, first) = chars.next()?;
    if first == '}' {
        return None;
    }
    let after = &rest[first.len_utf8()..];
    after.strip_prefix("%%")?;
    Some(first.len_utf8() + line_run(after))
}

/// `[\n]+`.
fn newlines(rest: &str) -> Option<usize> {
    let run = rest.chars().take_while(|ch| *ch == '\n').count();
    (run > 0).then_some(run)
}

/// `\s+`, matching JavaScript's whitespace set.
fn whitespace(rest: &str) -> Option<usize> {
    let run = js_space(rest);
    (run > 0).then_some(run)
}

/// `\#[^\n]*`.
fn hash_comment(rest: &str) -> Option<usize> {
    rest.strip_prefix('#')?;
    Some(1 + line_run(&rest[1..]))
}

/// `"timeline"[ \t]+LR\b` or `"timeline"[ \t]+TD\b`.
fn timeline_direction(rest: &str, direction: &str) -> Option<usize> {
    starts_with_ignore_case(rest, "timeline")?;
    let mut pos = "timeline".len();
    let spaces = rest[pos..]
        .char_indices()
        .take_while(|(_, ch)| *ch == ' ' || *ch == '\t')
        .count();
    if spaces == 0 {
        return None;
    }
    pos += spaces;
    starts_with_ignore_case(&rest[pos..], direction)?;
    pos += direction.len();
    word_boundary(rest, pos).then_some(pos)
}

/// `"timeline"\b`.
fn timeline(rest: &str) -> Option<usize> {
    starts_with_ignore_case(rest, "timeline")?;
    let len = "timeline".len();
    word_boundary(rest, len).then_some(len)
}

/// `"title"\s[^\n]+`.
fn title(rest: &str) -> Option<usize> {
    starts_with_ignore_case(rest, "title")?;
    let space = rest["title".len()..].chars().next()?;
    if !is_js_whitespace(space) {
        return None;
    }
    let after = "title".len() + space.len_utf8();
    let run = line_run(&rest[after..]);
    (run > 0).then_some(after + run)
}

/// `accTitle\s*":"\s*` and `accDescr\s*":"\s*` / `accDescr\s*"{"\s*`.
fn accessibility_keyword(rest: &str, keyword: &str, terminator: char) -> Option<usize> {
    starts_with_ignore_case(rest, keyword)?;
    let mut pos = skip_js_space(rest, keyword.len());
    if !rest[pos..].starts_with(terminator) {
        return None;
    }
    pos += terminator.len_utf8();
    Some(skip_js_space(rest, pos))
}

/// `"section"\s[^:\n]+`.
fn section(rest: &str) -> Option<usize> {
    starts_with_ignore_case(rest, "section")?;
    let space = rest["section".len()..].chars().next()?;
    if !is_js_whitespace(space) {
        return None;
    }
    let start = "section".len() + space.len_utf8();
    let run: usize = rest[start..]
        .chars()
        .take_while(|ch| *ch != ':' && *ch != '\n')
        .map(char::len_utf8)
        .sum();
    (run > 0).then_some(start + run)
}

/// `":"\s(?:[^:\n]|":"(?!\s))+`: a colon, one whitespace character, then at least one unit
/// of anything but a newline, or a colon that is not followed by whitespace.
fn event(rest: &str) -> Option<usize> {
    if !rest.starts_with(':') {
        return None;
    }
    let space = rest[1..].chars().next()?;
    if !is_js_whitespace(space) {
        return None;
    }
    let mut pos = 1 + space.len_utf8();
    let mut units = 0;
    while let Some(ch) = rest[pos..].chars().next() {
        match ch {
            '\n' => break,
            ':' if rest[pos + 1..].chars().next().is_some_and(is_js_whitespace) => {
                break;
            }
            _ => {
                pos += ch.len_utf8();
                units += 1;
            }
        }
    }
    (units > 0).then_some(pos)
}

/// `[^#:\n]+`.
fn period(rest: &str) -> Option<usize> {
    let run: usize = rest
        .chars()
        .take_while(|ch| !matches!(ch, '#' | ':' | '\n'))
        .map(char::len_utf8)
        .sum();
    (run > 0).then_some(run)
}

/// `starts_with` with the regex `i` flag.
fn starts_with_ignore_case(text: &str, prefix: &str) -> Option<()> {
    text.get(..prefix.len())
        .filter(|head| head.eq_ignore_ascii_case(prefix))
        .map(|_| ())
}

/// The `\b` jison-lex adds after a rule ending in a word character. Every caller's match
/// ends in an ASCII letter, so only the following character matters.
fn word_boundary(text: &str, at: usize) -> bool {
    text[at..].chars().next().is_none_or(|ch| !is_word_char(ch))
}

fn is_word_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

/// The length of `\s*` at the start of the text.
fn skip_js_space(text: &str, from: usize) -> usize {
    from + js_space(&text[from..])
}

/// The length of `\s*` at the start of the text.
fn js_space(text: &str) -> usize {
    text.char_indices()
        .find(|(_, ch)| !is_js_whitespace(*ch))
        .map_or(text.len(), |(index, _)| index)
}
