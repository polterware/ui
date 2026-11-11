//! The journey lexer: a hand port of the `%lex` section of upstream's
//! `diagrams/user-journey/parser/journey.jison`.
//!
//! Jison tries its rules in source order and takes the first that matches: the grammar does
//! not set `%options flex`, which would ask for the longest match. The order of the matchers
//! in [`Lexer::run`] is the jison file's, so the rule order and the start-condition stack
//! behave as upstream's do. `%x` declares exclusive start conditions: the rules without a
//! state prefix apply only in `INITIAL`.
//!
//! The rules, in order:
//!
//! 1. `%(?!\{)[^\n]*`, a `%` line that is not a directive: skipped;
//! 2. `[^\}]%%[^\n]*`, any character but `}`, then `%%`, then the rest of the line:
//!    skipped, so a line such as `a%% b` disappears;
//! 3. `[\n]+`: `NEWLINE`;
//! 4. `\s+`: skipped (this is what swallows `\r\n` after a line that did not consume the
//!    `\r` itself);
//! 5. `\#[^\n]*`: skipped;
//! 6. `"journey"`: `journey`, with the `\b` Jison appends to a plain string rule;
//! 7. `"title"\s[^#\n;]+`: `title`;
//! 8. `accTitle\s*":"\s*`: `acc_title`, entering `acc_title`;
//! 9. `<acc_title>(?!\n|;|#)*[^\n]*`: `acc_title_value`, popping the state;
//! 10. `accDescr\s*":"\s*`: `acc_descr`, entering `acc_descr`;
//! 11. `<acc_descr>(?!\n|;|#)*[^\n]*`: `acc_descr_value`, popping the state;
//! 12. `accDescr\s*"{"\s*`: enters `acc_descr_multiline` without a token;
//! 13. `<acc_descr_multiline>[\}]`: pops the state without a token;
//! 14. `<acc_descr_multiline>[^\}]*`: `acc_descr_multiline_value`, staying in the state;
//! 15. `"section"\s[^#:\n;]+`: `section`;
//! 16. `[^#:\n;]+`: `taskName`;
//! 17. `":"[^#\n;]+`: `taskData`;
//! 18. `":"`: `':'`;
//! 19. `<<EOF>>`: `EOF`;
//! 20. `.`: `INVALID`.
//!
//! The `(?!\n|;|#)*` prefix of rules 9 and 11 is a quantified zero-width lookahead: it
//! matches the empty string and constrains nothing (Jison's own generated regex is
//! `(?!\n||)*[^\n]*`), so both values are the rest of the line, `\r` included.
//!
//! # Jison's end of input
//!
//! Jison's parser wants the explicit `EOF` token rule 19 returns, not its own end marker,
//! and the lexer only produces that token while `INITIAL` is the current condition. Once the
//! input is empty, the next call to the lexer sets its `done` flag and still runs that call's
//! rules; a later call answers the parser's own end marker instead. When the last token is an
//! empty value consumed in one of the exclusive conditions, the state rule matches the empty
//! string before `INITIAL` can return `EOF`, so the grammar never sees its `EOF` and upstream
//! rejects the diagram. [`Kind::End`] is that end marker, and [`super::parser`] rejects it.
//! For example, `journey\naccTitle:` is a parse error upstream.
//!
//! Three upstream quirks fall out of these rules and are kept:
//! * `\s` in rules 7 and 15 is any JavaScript whitespace, line breaks included, so a
//!   `title` with nothing after it takes the next line's text as its title:
//!   `journey\ntitle\nsection A` titles the diagram `section A`.
//! * Tokens of rules 7, 15, 16, and 17 consume `\r`, so a CRLF line leaves it in the value
//!   (`section A\r`) until the caller trims it. `parse` does not trim the title or a section.
//! * Rule 2 starts with any character but `}`, a line break included, so it can swallow the
//!   break before a `%%` comment line.

use crate::error::Span;
use crate::preprocess::is_js_whitespace;

/// The Jison token names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Journey,
    Title,
    AccTitle,
    AccTitleValue,
    AccDescr,
    AccDescrValue,
    AccDescrMultilineValue,
    Section,
    TaskName,
    TaskData,
    Colon,
    Newline,
    /// The grammar's explicit `EOF` token (rule 19), which `start` requires.
    Eof,
    /// The parser's own end marker, reached only after an exclusive condition consumed the
    /// last text (see the module docs).
    End,
    /// `.`, the `INVALID` token: in practice only `;`.
    Invalid,
}

impl Kind {
    /// The token name upstream's generated parser prints in an error.
    pub(super) fn name(self) -> &'static str {
        match self {
            Kind::Journey => "journey",
            Kind::Title => "title",
            Kind::AccTitle => "acc_title",
            Kind::AccTitleValue => "acc_title_value",
            Kind::AccDescr => "acc_descr",
            Kind::AccDescrValue => "acc_descr_value",
            Kind::AccDescrMultilineValue => "acc_descr_multiline_value",
            Kind::Section => "section",
            Kind::TaskName => "taskName",
            Kind::TaskData => "taskData",
            Kind::Colon => ":",
            Kind::Newline => "NEWLINE",
            Kind::Eof => "EOF",
            Kind::End => "end of input",
            Kind::Invalid => "INVALID",
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct Token {
    pub(super) kind: Kind,
    /// The matched text, empty for [`Kind::Eof`] and [`Kind::End`].
    pub(super) text: String,
    pub(super) span: Span,
}

/// The lexer start conditions (`%x` in the jison file); the names are upstream's, so they
/// keep its `acc_` prefix.
#[allow(clippy::enum_variant_names)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    AccTitle,
    AccDescr,
    AccDescrMultiline,
}

pub(super) fn lex(src: &str) -> Vec<Token> {
    Lexer {
        src,
        pos: 0,
        line: 1,
        line_start: 0,
        state: None,
        done: false,
        tokens: Vec::new(),
    }
    .run()
}

struct Lexer<'a> {
    src: &'a str,
    pos: usize,
    line: u32,
    line_start: usize,
    state: Option<State>,
    done: bool,
    tokens: Vec<Token>,
}

impl Lexer<'_> {
    fn run(mut self) -> Vec<Token> {
        loop {
            if self.done {
                self.push(Kind::End, 0);
                return self.tokens;
            }
            let empty = self.pos >= self.src.len();
            if empty {
                self.done = true;
            }
            let rest = &self.src[self.pos..];
            if let Some(state) = self.state {
                self.state_token(state, rest);
                continue;
            }
            if empty {
                self.push(Kind::Eof, 0);
                continue;
            }
            if let Some(len) = percent_line(rest) {
                self.advance(len);
                continue;
            }
            if let Some(len) = inline_comment(rest) {
                self.advance(len);
                continue;
            }
            if let Some(len) = newlines(rest) {
                self.push(Kind::Newline, len);
                continue;
            }
            if let Some(len) = whitespace(rest) {
                self.advance(len);
                continue;
            }
            if let Some(len) = hash_line(rest) {
                self.advance(len);
                continue;
            }
            if let Some(len) = journey(rest) {
                self.push(Kind::Journey, len);
                continue;
            }
            if let Some(len) = keyword_line(rest, "title", false) {
                self.push(Kind::Title, len);
                continue;
            }
            if let Some(len) = keyword_colon(rest, "accTitle") {
                self.push(Kind::AccTitle, len);
                self.state = Some(State::AccTitle);
                continue;
            }
            if let Some(len) = keyword_colon(rest, "accDescr") {
                self.push(Kind::AccDescr, len);
                self.state = Some(State::AccDescr);
                continue;
            }
            if let Some(len) = keyword_brace(rest, "accDescr") {
                self.advance(len);
                self.state = Some(State::AccDescrMultiline);
                continue;
            }
            if let Some(len) = keyword_line(rest, "section", true) {
                self.push(Kind::Section, len);
                continue;
            }
            if let Some(len) = task_name(rest) {
                self.push(Kind::TaskName, len);
                continue;
            }
            if let Some(len) = task_data(rest) {
                self.push(Kind::TaskData, len);
                continue;
            }
            if rest.starts_with(':') {
                self.push(Kind::Colon, 1);
                continue;
            }
            let ch = rest.chars().next().expect("non-empty remainder");
            self.push(Kind::Invalid, ch.len_utf8());
        }
    }

    /// The rules of the current exclusive start condition; each one always matches, so the
    /// lexer never stalls.
    fn state_token(&mut self, state: State, rest: &str) {
        match state {
            State::AccTitle => {
                self.state = None;
                self.push(Kind::AccTitleValue, line_run(rest));
            }
            State::AccDescr => {
                self.state = None;
                self.push(Kind::AccDescrValue, line_run(rest));
            }
            State::AccDescrMultiline => {
                if rest.starts_with('}') {
                    self.state = None;
                    self.advance(1);
                } else {
                    self.push(Kind::AccDescrMultilineValue, run_until(rest, '}'));
                }
            }
        }
    }

    fn push(&mut self, kind: Kind, len: usize) {
        let span = self.span(len);
        let text = self.src[self.pos..self.pos + len].to_string();
        self.tokens.push(Token { kind, text, span });
        self.advance(len);
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
}

/// `%(?!\{)[^\n]*`: a `%` line that does not open a directive.
fn percent_line(rest: &str) -> Option<usize> {
    let after = rest.strip_prefix('%')?;
    if after.starts_with('{') {
        return None;
    }
    Some(1 + run_until(after, '\n'))
}

/// `[^\}]\%\%[^\n]*`: any character but `}`, then `%%`, then the rest of the line.
fn inline_comment(rest: &str) -> Option<usize> {
    let first = rest.chars().next()?;
    if first == '}' {
        return None;
    }
    let after = &rest[first.len_utf8()..];
    let tail = after.strip_prefix("%%")?;
    Some(first.len_utf8() + 2 + run_until(tail, '\n'))
}

/// `[\n]+`.
fn newlines(rest: &str) -> Option<usize> {
    let len = rest.chars().take_while(|ch| *ch == '\n').count();
    (len > 0).then_some(len)
}

/// `\s+`, the skipped whitespace run (a `\r\n` after a token that left the `\r` lands here).
fn whitespace(rest: &str) -> Option<usize> {
    let len: usize = rest
        .chars()
        .take_while(|ch| is_js_whitespace(*ch))
        .map(char::len_utf8)
        .sum();
    (len > 0).then_some(len)
}

/// `\#[^\n]*`.
fn hash_line(rest: &str) -> Option<usize> {
    rest.strip_prefix('#')?;
    Some(1 + run_until(&rest[1..], '\n'))
}

/// `"journey"`: Jison appends `\b` to a plain string rule, case-insensitively.
fn journey(rest: &str) -> Option<usize> {
    let head = rest.get(.."journey".len())?;
    if !head.eq_ignore_ascii_case("journey") {
        return None;
    }
    let next = rest["journey".len()..].chars().next();
    match next {
        Some(ch) if is_word_char(ch) => None,
        _ => Some("journey".len()),
    }
}

/// `"title"\s[^#\n;]+` or `"section"\s[^#:\n;]+`: one whitespace character after the
/// keyword, then at least one character of the value.
fn keyword_line(rest: &str, keyword: &str, stops_at_colon: bool) -> Option<usize> {
    let head = rest.get(..keyword.len())?;
    if !head.eq_ignore_ascii_case(keyword) {
        return None;
    }
    let after = &rest[keyword.len()..];
    let first = after.chars().next()?;
    if !is_js_whitespace(first) {
        return None;
    }
    let start = keyword.len() + first.len_utf8();
    let value = &rest[start..];
    let len = if stops_at_colon {
        run_until_any(value, &['#', ':', '\n', ';'])
    } else {
        run_until_any(value, &['#', '\n', ';'])
    };
    (len > 0).then_some(start + len)
}

/// `accTitle\s*":"\s*` and `accDescr\s*":"\s*`, case-insensitively.
fn keyword_colon(rest: &str, keyword: &str) -> Option<usize> {
    let head = rest.get(..keyword.len())?;
    if !head.eq_ignore_ascii_case(keyword) {
        return None;
    }
    let mut cursor = keyword.len() + whitespace_len(&rest[keyword.len()..]);
    rest[cursor..].strip_prefix(':')?;
    cursor += 1;
    Some(cursor + whitespace_len(&rest[cursor..]))
}

/// `accDescr\s*"{"\s*`, case-insensitively.
fn keyword_brace(rest: &str, keyword: &str) -> Option<usize> {
    let head = rest.get(..keyword.len())?;
    if !head.eq_ignore_ascii_case(keyword) {
        return None;
    }
    let mut cursor = keyword.len() + whitespace_len(&rest[keyword.len()..]);
    rest[cursor..].strip_prefix('{')?;
    cursor += 1;
    Some(cursor + whitespace_len(&rest[cursor..]))
}

/// `[^#:\n;]+`.
fn task_name(rest: &str) -> Option<usize> {
    let len = run_until_any(rest, &['#', ':', '\n', ';']);
    (len > 0).then_some(len)
}

/// `":"[^#\n;]+`.
fn task_data(rest: &str) -> Option<usize> {
    let after = rest.strip_prefix(':')?;
    let len = run_until_any(after, &['#', '\n', ';']);
    (len > 0).then_some(1 + len)
}

fn run_until(rest: &str, stop: char) -> usize {
    rest.find(stop).unwrap_or(rest.len())
}

fn run_until_any(rest: &str, stops: &[char]) -> usize {
    rest.find(stops).unwrap_or(rest.len())
}

fn line_run(rest: &str) -> usize {
    run_until(rest, '\n')
}

/// `\s*`, whose length may be zero.
fn whitespace_len(rest: &str) -> usize {
    rest.chars()
        .take_while(|ch| is_js_whitespace(*ch))
        .map(char::len_utf8)
        .sum()
}

/// JavaScript's `\b` word characters: ASCII letters, digits, and `_`.
fn is_word_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}
