//! The lexer, a hand port of the `%lex` section of upstream's
//! `diagrams/gantt/parser/gantt.jison`.
//!
//! Jison tries its rules in source order and takes the first that matches: the gantt grammar
//! does not set `%options flex`, which would ask for flex's longest match. Every rule keeps
//! the line of the jison file it comes from as its [`Candidate::order`], so the rule order
//! and the start-condition stack behave as upstream's do. `%x` declares exclusive start
//! conditions: a rule without a state prefix applies only in `INITIAL`.
//!
//! Three dead or unreachable upstream patterns are not implemented: `\%%[^\n]*` (line 34)
//! duplicates the comment rule at line 28, `SPACE` (used only by a `line` alternative) is
//! never returned by any rule, and `%%{` enters a start condition that this grammar never
//! declares, which makes upstream's lexer throw; here it is a parse error.
//!
//! The `(?!\n|;|#)*` prefix of the `acc_title`/`acc_descr` value rules is a quantified
//! zero-width lookahead: it matches the empty string and constrains nothing, so the values
//! are the rest of the line.

use crate::error::{ParseError, Span};
use crate::preprocess::is_js_whitespace;

/// The jison token names that survive into the grammar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    AccDescr,
    AccDescrMultilineValue,
    AccDescrValue,
    /// `accDescription`, which upstream's grammar has no rule for (a syntax error).
    AccDescription,
    AccTitle,
    AccTitleValue,
    AxisFormat,
    CallbackArgs,
    CallbackName,
    Click,
    Colon,
    Date,
    DateFormat,
    Eof,
    Excludes,
    Gantt,
    Href,
    Includes,
    InclusiveEndDates,
    Invalid,
    Newline,
    Section,
    TaskData,
    TaskTxt,
    TickInterval,
    Title,
    TodayMarker,
    TopAxis,
    Weekday,
    Weekend,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Token {
    pub kind: Kind,
    pub text: String,
    pub span: Span,
}

/// A lexer start condition (`%x` in the jison file).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    AccTitle,
    AccDescr,
    AccDescrMultiline,
    Href,
    CallbackName,
    CallbackArgs,
    Click,
}

#[derive(Clone, Copy, Debug)]
struct Candidate {
    /// The jison line of the rule; the lowest line wins.
    order: u16,
    len: usize,
    kind: Option<Kind>,
    push: Option<State>,
    pop: bool,
    error: Option<&'static str>,
}

impl Candidate {
    fn new(order: u16, len: usize) -> Self {
        Self {
            order,
            len,
            kind: None,
            push: None,
            pop: false,
            error: None,
        }
    }

    fn token(order: u16, len: usize, kind: Kind) -> Self {
        Self {
            kind: Some(kind),
            ..Self::new(order, len)
        }
    }

    fn entering(mut self, state: State) -> Self {
        self.push = Some(state);
        self
    }

    fn popping(mut self) -> Self {
        self.pop = true;
        self
    }

    fn failing(mut self, message: &'static str) -> Self {
        self.error = Some(message);
        self
    }
}

pub(super) fn lex(src: &str) -> Result<Vec<Token>, ParseError> {
    let mut lexer = Lexer {
        src,
        pos: 0,
        line: 1,
        line_start: 0,
        states: Vec::new(),
        tokens: Vec::new(),
    };
    lexer.run()?;
    Ok(lexer.tokens)
}

struct Lexer<'a> {
    src: &'a str,
    pos: usize,
    line: u32,
    line_start: usize,
    states: Vec<State>,
    tokens: Vec<Token>,
}

impl Lexer<'_> {
    fn run(&mut self) -> Result<(), ParseError> {
        loop {
            if self.pos >= self.src.len() {
                let span = self.span(0);
                self.tokens.push(Token {
                    kind: Kind::Eof,
                    text: String::new(),
                    span,
                });
                return Ok(());
            }
            let rest = &self.src[self.pos..];
            let candidate = self.best_candidate(rest);
            if let Some(message) = candidate.error {
                return Err(ParseError::at(message, self.span(candidate.len)));
            }
            if candidate.len == 0 && candidate.kind.is_none() {
                return Err(ParseError::at("Unrecognized text", self.span(0)));
            }
            if let Some(kind) = candidate.kind {
                self.tokens.push(Token {
                    kind,
                    text: rest[..candidate.len].to_string(),
                    span: self.span(candidate.len),
                });
            }
            if candidate.pop {
                self.states.pop();
            }
            if let Some(state) = candidate.push {
                self.states.push(state);
            }
            self.advance(candidate.len);
        }
    }

    fn span(&self, len: usize) -> Span {
        let column = self.src[self.line_start..self.pos].chars().count() as u32 + 1;
        Span {
            line: self.line,
            column,
            start: self.pos,
            end: self.pos + len,
        }
    }

    fn advance(&mut self, len: usize) {
        if len == 0 {
            return;
        }
        let text = &self.src[self.pos..self.pos + len];
        self.line += text.matches('\n').count() as u32;
        if let Some(offset) = text.rfind('\n') {
            self.line_start = self.pos + offset + 1;
        }
        self.pos += len;
    }

    /// The first rule that matches, which is how Jison's lexer behaves without
    /// `%options flex` (this grammar does not ask for flex's longest match).
    fn best_candidate(&self, rest: &str) -> Candidate {
        let mut candidates: Vec<Candidate> = Vec::new();
        let mut consider = |candidate: Option<Candidate>| {
            if let Some(candidate) = candidate {
                candidates.push(candidate);
            }
        };
        match self.states.last().copied() {
            None => self.consider_initial(rest, &mut consider),
            Some(state) => self.consider_state(state, rest, &mut consider),
        }
        candidates
            .into_iter()
            .min_by_key(|candidate| candidate.order)
            .unwrap_or_else(|| Candidate::new(u16::MAX, 0))
    }

    /// The rules of the `INITIAL` start condition, in jison order.
    fn consider_initial(&self, rest: &str, consider: &mut impl FnMut(Option<Candidate>)) {
        if rest.starts_with("%%{") {
            consider(Some(
                Candidate::new(18, 3).failing("Unexpected '%%{': directives are removed first"),
            ));
        }
        consider(
            keyword_colon(rest, "accTitle")
                .map(|len| Candidate::token(20, len, Kind::AccTitle).entering(State::AccTitle)),
        );
        consider(
            keyword_colon(rest, "accDescr")
                .map(|len| Candidate::token(22, len, Kind::AccDescr).entering(State::AccDescr)),
        );
        consider(
            keyword_brace(rest, "accDescr")
                .map(|len| Candidate::new(24, len).entering(State::AccDescrMultiline)),
        );
        // A `%%` line that does not open a directive, without its line break.
        if rest.starts_with("%%") {
            consider(Some(Candidate::new(28, line_run(rest))));
        }
        // `[^\}]\%\%*[^\n]*`: a line whose second character is `%`.
        consider(percent_after_one(rest).map(|len| Candidate::new(29, len)));
        // `\%\%*[^\n]*[\n]*`: a line that starts with `%`, plus the line breaks after it.
        consider(percent_line(rest).map(|len| Candidate::new(30, len)));
        consider(nl_run(rest).map(|len| Candidate::token(32, len, Kind::Newline)));
        consider(ws_run(rest).map(|len| Candidate::new(33, len)));
        consider(href_begin(rest));
        consider(call_begin(rest));
        consider(click_begin(rest));
        consider(ci_len(rest, "gantt").map(|len| Candidate::token(71, len, Kind::Gantt)));
        consider(
            keyword_value(rest, "dateFormat", false)
                .map(|len| Candidate::token(72, len, Kind::DateFormat)),
        );
        consider(
            ci_len(rest, "inclusiveEndDates")
                .map(|len| Candidate::token(73, len, Kind::InclusiveEndDates)),
        );
        consider(ci_len(rest, "topAxis").map(|len| Candidate::token(74, len, Kind::TopAxis)));
        consider(
            keyword_value(rest, "axisFormat", false)
                .map(|len| Candidate::token(75, len, Kind::AxisFormat)),
        );
        consider(
            keyword_value(rest, "tickInterval", false)
                .map(|len| Candidate::token(76, len, Kind::TickInterval)),
        );
        consider(
            keyword_value(rest, "includes", false)
                .map(|len| Candidate::token(77, len, Kind::Includes)),
        );
        consider(
            keyword_value(rest, "excludes", false)
                .map(|len| Candidate::token(78, len, Kind::Excludes)),
        );
        consider(
            keyword_value(rest, "todayMarker", true)
                .map(|len| Candidate::token(79, len, Kind::TodayMarker)),
        );
        for (day, order) in [
            ("monday", 80),
            ("tuesday", 81),
            ("wednesday", 82),
            ("thursday", 83),
            ("friday", 84),
            ("saturday", 85),
            ("sunday", 86),
        ] {
            consider(
                day_rule(rest, "weekday", day)
                    .map(|len| Candidate::token(order, len, Kind::Weekday)),
            );
        }
        consider(
            day_rule(rest, "weekend", "friday").map(|len| Candidate::token(87, len, Kind::Weekend)),
        );
        consider(
            day_rule(rest, "weekend", "saturday")
                .map(|len| Candidate::token(88, len, Kind::Weekend)),
        );
        consider(date_rule(rest).map(|len| Candidate::token(89, len, Kind::Date)));
        consider(keyword_line(rest, "title").map(|len| Candidate::token(90, len, Kind::Title)));
        consider(
            keyword_value(rest, "accDescription", false)
                .map(|len| Candidate::token(91, len, Kind::AccDescription)),
        );
        consider(keyword_line(rest, "section").map(|len| Candidate::token(92, len, Kind::Section)));
        consider(task_text(rest).map(|len| Candidate::token(93, len, Kind::TaskTxt)));
        consider(task_data(rest).map(|len| Candidate::token(94, len, Kind::TaskData)));
        if rest.starts_with(':') {
            consider(Some(Candidate::token(95, 1, Kind::Colon)));
        }
        consider(invalid(rest).map(|len| Candidate::token(97, len, Kind::Invalid)));
    }

    /// The rules of an exclusive start condition.
    fn consider_state(
        &self,
        state: State,
        rest: &str,
        consider: &mut impl FnMut(Option<Candidate>),
    ) {
        match state {
            State::AccTitle => consider(Some(
                Candidate::token(21, line_run(rest), Kind::AccTitleValue).popping(),
            )),
            State::AccDescr => consider(Some(
                Candidate::token(23, line_run(rest), Kind::AccDescrValue).popping(),
            )),
            State::AccDescrMultiline => {
                if rest.starts_with('}') {
                    consider(Some(Candidate::new(25, 1).popping()));
                } else {
                    consider(Some(Candidate::token(
                        26,
                        run_until(rest, '}'),
                        Kind::AccDescrMultilineValue,
                    )));
                }
            }
            State::Href => {
                if rest.starts_with('"') {
                    consider(Some(Candidate::new(43, 1).popping()));
                } else {
                    consider(Some(Candidate::token(44, run_until(rest, '"'), Kind::Href)));
                }
            }
            State::CallbackName => {
                if let Some(len) = callback_close(rest) {
                    consider(Some(Candidate::new(55, len).popping()));
                } else if rest.starts_with('(') {
                    consider(Some(
                        Candidate::new(56, 1)
                            .popping()
                            .entering(State::CallbackArgs),
                    ));
                } else {
                    consider(Some(Candidate::token(
                        57,
                        run_until(rest, '('),
                        Kind::CallbackName,
                    )));
                }
            }
            State::CallbackArgs => {
                if rest.starts_with(')') {
                    consider(Some(Candidate::new(58, 1).popping()));
                } else {
                    consider(Some(Candidate::token(
                        59,
                        run_until(rest, ')'),
                        Kind::CallbackArgs,
                    )));
                }
            }
            State::Click => {
                if let Some(c) = rest.chars().next()
                    && is_js_whitespace(c)
                {
                    consider(Some(Candidate::new(68, c.len_utf8()).popping()));
                }
                let id = rest
                    .chars()
                    .take_while(|c| !is_js_whitespace(*c))
                    .map(char::len_utf8)
                    .sum::<usize>();
                consider(Some(Candidate::token(69, id, Kind::Click)));
            }
        }
    }
}

/// `accTitle\s*":"\s*` and `accDescr\s*":"\s*`, case-insensitively.
fn keyword_colon(rest: &str, keyword: &str) -> Option<usize> {
    let len = ci_len(rest, keyword)?;
    let after = &rest[len..];
    let head = ws_run(after).unwrap_or(0);
    let after = after[head..].strip_prefix(':')?;
    let tail = ws_run(after).unwrap_or(0);
    Some(len + head + 1 + tail)
}

/// `accDescr\s*"{"\s*`, case-insensitively.
fn keyword_brace(rest: &str, keyword: &str) -> Option<usize> {
    let len = ci_len(rest, keyword)?;
    let after = &rest[len..];
    let head = ws_run(after).unwrap_or(0);
    let after = after[head..].strip_prefix('{')?;
    let tail = ws_run(after).unwrap_or(0);
    Some(len + head + 1 + tail)
}

/// `keyword\s<value>`, with one whitespace character after the keyword.
fn keyword_value(rest: &str, keyword: &str, allow_hash: bool) -> Option<usize> {
    let len = ci_len(rest, keyword)?;
    let first = rest[len..].chars().next()?;
    if !is_js_whitespace(first) {
        return None;
    }
    let start = len + first.len_utf8();
    let count = rest[start..]
        .chars()
        .take_while(|c| !matches!(c, '\n' | ';') && (allow_hash || *c != '#'))
        .map(char::len_utf8)
        .sum::<usize>();
    (count > 0).then_some(start + count)
}

/// `keyword\s<line>`, with one whitespace character after the keyword.
fn keyword_line(rest: &str, keyword: &str) -> Option<usize> {
    let len = ci_len(rest, keyword)?;
    let first = rest[len..].chars().next()?;
    if !is_js_whitespace(first) {
        return None;
    }
    let start = len + first.len_utf8();
    let count = run_until(&rest[start..], '\n');
    // The pattern is `[^\n]+`, so at least one character must follow.
    (count > 0).then_some(start + count)
}

/// `weekday\s+monday` and its siblings, case-insensitively.
fn day_rule(rest: &str, keyword: &str, day: &str) -> Option<usize> {
    let len = ci_len(rest, keyword)?;
    let ws = ws_run(&rest[len..])?;
    let day_len = ci_len(&rest[len + ws..], day)?;
    Some(len + ws + day_len)
}

/// `"href"[\s]+["]`: enters the `href` condition.
fn href_begin(rest: &str) -> Option<Candidate> {
    let len = ci_len(rest, "href")?;
    let ws = ws_run(&rest[len..])?;
    if !rest[len + ws..].starts_with('"') {
        return None;
    }
    Some(Candidate::new(42, len + ws + 1).entering(State::Href))
}

/// `"call"[\s]+`: enters the `callbackname` condition.
fn call_begin(rest: &str) -> Option<Candidate> {
    let len = ci_len(rest, "call")?;
    let ws = ws_run(&rest[len..])?;
    Some(Candidate::new(54, len + ws).entering(State::CallbackName))
}

/// `"click"[\s]+`: enters the `click` condition.
fn click_begin(rest: &str) -> Option<Candidate> {
    let len = ci_len(rest, "click")?;
    let ws = ws_run(&rest[len..])?;
    Some(Candidate::new(67, len + ws).entering(State::Click))
}

/// `<callbackname>\([\s]*\)`.
fn callback_close(rest: &str) -> Option<usize> {
    let after = rest.strip_prefix('(')?;
    let ws = ws_run(after).unwrap_or(0);
    after[ws..].starts_with(')').then_some(1 + ws + 1)
}

/// `\d\d\d\d"-"\d\d"-"\d\d`.
fn date_rule(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    if bytes.len() < 10 {
        return None;
    }
    let is_digit = |index: usize| bytes[index].is_ascii_digit();
    let matches = (0..4).all(is_digit)
        && bytes[4] == b'-'
        && is_digit(5)
        && is_digit(6)
        && bytes[7] == b'-'
        && is_digit(8)
        && is_digit(9);
    matches.then_some(10)
}

/// `[^:\n]+`.
fn task_text(rest: &str) -> Option<usize> {
    let len = run_until(rest, ':').min(run_until(rest, '\n'));
    (len > 0).then_some(len)
}

/// `":"[^#\n;]+`.
fn task_data(rest: &str) -> Option<usize> {
    let after = rest.strip_prefix(':')?;
    let len = after
        .chars()
        .take_while(|c| !matches!(c, '#' | '\n' | ';'))
        .map(char::len_utf8)
        .sum::<usize>();
    (len > 0).then_some(1 + len)
}

/// `.`, the `INVALID` token.
fn invalid(rest: &str) -> Option<usize> {
    let c = rest.chars().next()?;
    (c != '\n').then_some(c.len_utf8())
}

/// `[^\}]\%\%*[^\n]*`: a match that starts with any character but `}`, then a `%`, then the
/// rest of that line. The first character may be a line break, which is why this rule can
/// swallow the break before a comment line (it comes before the `[\n]+` rule).
fn percent_after_one(rest: &str) -> Option<usize> {
    let first = rest.chars().next()?;
    if first == '}' {
        return None;
    }
    let after_first = &rest[first.len_utf8()..];
    if !after_first.starts_with('%') {
        return None;
    }
    let percents = after_first.chars().take_while(|c| *c == '%').count();
    let tail = &after_first[percents..];
    Some(first.len_utf8() + percents + run_until(tail, '\n'))
}

/// `\%\%*[^\n]*[\n]*`: a line that starts with `%`, with the line breaks after it.
fn percent_line(rest: &str) -> Option<usize> {
    if !rest.starts_with('%') {
        return None;
    }
    let mut len = line_run(rest);
    while rest[len..].starts_with('\n') {
        len += 1;
    }
    Some(len)
}

/// `\s*` is not needed here: `ws_run` answers zero when there is no whitespace.
fn ci_len(rest: &str, prefix: &str) -> Option<usize> {
    let head = rest.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix).then_some(prefix.len())
}

fn ws_run(rest: &str) -> Option<usize> {
    let len = rest
        .chars()
        .take_while(|c| is_js_whitespace(*c))
        .map(char::len_utf8)
        .sum::<usize>();
    (len > 0).then_some(len)
}

fn nl_run(rest: &str) -> Option<usize> {
    let len = rest.chars().take_while(|c| *c == '\n').count();
    (len > 0).then_some(len)
}

fn line_run(rest: &str) -> usize {
    run_until(rest, '\n')
}

/// The byte length up to the next `stop` (possibly zero).
fn run_until(rest: &str, stop: char) -> usize {
    rest.find(stop).unwrap_or(rest.len())
}
