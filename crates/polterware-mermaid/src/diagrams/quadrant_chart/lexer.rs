//! The quadrant chart lexer: a port of the `%lex` block of `parser/quadrant.jison`.
//!
//! Two jison details decide the token stream, and both are reproduced here:
//!
//! * Without `%options flex`, jison's lexer takes the **first** rule that matches at the
//!   current offset, not the longest match. Rule order is therefore part of the grammar: the
//!   newline rule wins over a `point_start` whose `\s*` could swallow it, and `x-axis` wins
//!   over the `ALPHA` that would otherwise take `x`.
//! * `%options case-insensitive` compiles every pattern with the `i` flag, and `lex-parser`
//!   appends `\b` to the rules that are a bare word (`title`, `classDef`), so `titles` is an
//!   `ALPHA` while `title:` starts a title.
//!
//! The modes are the `%x` start conditions. A rule can push a mode (`begin`) or pop one
//! (`popState`). Points push `point_start`, then `point_x`, then `point_y`; `point_y` pops back
//! to `point_start`, whose `\s*\] *` rule consumes the closing bracket and pops again. The
//! closing bracket is why `[0.1, 0.4]   ` leaves no trailing `SPACE` tokens behind.
//!
//! At end of input jison answers its `<<EOF>>` rule (`EOF`) only in the `INITIAL` mode; inside
//! any other mode the generated lexer answers `$end`, which no grammar rule accepts as an end
//! of line. That distinction is observable: `title` without a trailing newline is a parse
//! error, `title` followed by one is not. [`TokenKind::End`] is that `$end`.

use crate::common::text::is_line_terminator;
use crate::error::{ParseError, Span};
use crate::preprocess::is_js_whitespace;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    /// `NEWLINE`, a run of `\n` and `\r`.
    Newline,
    /// `title`.
    Title,
    /// `title_value`, the rest of the title line.
    TitleValue,
    /// `acc_title`.
    AccTitle,
    /// `acc_title_value`.
    AccTitleValue,
    /// `acc_descr`.
    AccDescr,
    /// `acc_descr_value`.
    AccDescrValue,
    /// `acc_descr_multiline_value`.
    AccDescrMultilineValue,
    /// `X-AXIS`.
    XAxis,
    /// `Y-AXIS`.
    YAxis,
    /// `AXIS-TEXT-DELIMITER`, two or more dashes and a `>`.
    AxisTextDelimiter,
    /// `QUADRANT_1`.
    Quadrant1,
    /// `QUADRANT_2`.
    Quadrant2,
    /// `QUADRANT_3`.
    Quadrant3,
    /// `QUADRANT_4`.
    Quadrant4,
    /// `CLASSDEF`.
    ClassDef,
    /// `MD_STR`.
    MdStr,
    /// `STR`.
    Str,
    /// `class_name`.
    ClassName,
    /// `point_start`.
    PointStart,
    /// `point_x`.
    PointX,
    /// `point_y`.
    PointY,
    /// `QUADRANT`, the `quadrantChart` header.
    Quadrant,
    /// `ALPHA`, a run of ASCII letters.
    Alpha,
    /// `UNICODE_TEXT`, a run of non-ASCII characters.
    UnicodeText,
    /// `COLON`.
    Colon,
    /// `PLUS`.
    Plus,
    /// `COMMA`.
    Comma,
    /// `EQUALS`.
    Equals,
    /// `MULT`.
    Mult,
    /// `BRKT`, a `#`.
    Brkt,
    /// `UNDERSCORE`.
    Underscore,
    /// `DOT`.
    Dot,
    /// `AMP`.
    Amp,
    /// `MINUS`.
    Minus,
    /// `NUM`, a run of digits.
    Num,
    /// `SPACE`, one JavaScript whitespace character.
    Space,
    /// `SEMI`.
    Semi,
    /// `PUNCTUATION`.
    Punctuation,
    /// `EOF`, at end of input in the `INITIAL` mode.
    Eof,
    /// `$end`, at end of input while another mode is still active.
    End,
}

impl TokenKind {
    /// The token name upstream's generated parser prints in an error.
    pub(crate) fn name(self) -> &'static str {
        match self {
            TokenKind::Newline => "NEWLINE",
            TokenKind::Title => "title",
            TokenKind::TitleValue => "title_value",
            TokenKind::AccTitle => "acc_title",
            TokenKind::AccTitleValue => "acc_title_value",
            TokenKind::AccDescr => "acc_descr",
            TokenKind::AccDescrValue => "acc_descr_value",
            TokenKind::AccDescrMultilineValue => "acc_descr_multiline_value",
            TokenKind::XAxis => "X-AXIS",
            TokenKind::YAxis => "Y-AXIS",
            TokenKind::AxisTextDelimiter => "AXIS-TEXT-DELIMITER",
            TokenKind::Quadrant1 => "QUADRANT_1",
            TokenKind::Quadrant2 => "QUADRANT_2",
            TokenKind::Quadrant3 => "QUADRANT_3",
            TokenKind::Quadrant4 => "QUADRANT_4",
            TokenKind::ClassDef => "CLASSDEF",
            TokenKind::MdStr => "MD_STR",
            TokenKind::Str => "STR",
            TokenKind::ClassName => "class_name",
            TokenKind::PointStart => "point_start",
            TokenKind::PointX => "point_x",
            TokenKind::PointY => "point_y",
            TokenKind::Quadrant => "QUADRANT",
            TokenKind::Alpha => "ALPHA",
            TokenKind::UnicodeText => "UNICODE_TEXT",
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
            TokenKind::Space => "SPACE",
            TokenKind::Semi => "SEMI",
            TokenKind::Punctuation => "PUNCTUATION",
            TokenKind::Eof => "EOF",
            TokenKind::End => "end of input",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Token {
    pub(crate) kind: TokenKind,
    /// The matched text; empty for [`TokenKind::Eof`] and [`TokenKind::End`].
    pub(crate) text: String,
    pub(crate) span: Span,
}

/// A `%x` start condition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Initial,
    Title,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
    Str,
    MdStr,
    PointStart,
    PointX,
    PointY,
    ClassName,
}

/// What a matching rule does: change the mode stack and/or return a token.
#[derive(Clone, Copy, Debug)]
struct Action {
    pop: bool,
    push: Option<Mode>,
    token: Option<TokenKind>,
}

impl Action {
    const fn skip() -> Self {
        Self {
            pop: false,
            push: None,
            token: None,
        }
    }

    const fn token(kind: TokenKind) -> Self {
        Self {
            pop: false,
            push: None,
            token: Some(kind),
        }
    }

    const fn push(mode: Mode) -> Self {
        Self {
            pop: false,
            push: Some(mode),
            token: None,
        }
    }

    const fn push_token(mode: Mode, kind: TokenKind) -> Self {
        Self {
            pop: false,
            push: Some(mode),
            token: Some(kind),
        }
    }

    const fn pop() -> Self {
        Self {
            pop: true,
            push: None,
            token: None,
        }
    }

    const fn pop_token(kind: TokenKind) -> Self {
        Self {
            pop: true,
            push: None,
            token: Some(kind),
        }
    }

    const fn pop_push(mode: Mode) -> Self {
        Self {
            pop: true,
            push: Some(mode),
            token: None,
        }
    }
}

pub(crate) struct Lexer<'a> {
    source: &'a str,
    offset: usize,
    line: u32,
    column: u32,
    modes: Vec<Mode>,
}

impl<'a> Lexer<'a> {
    pub(crate) fn new(source: &'a str) -> Self {
        Self {
            source,
            offset: 0,
            line: 1,
            column: 1,
            modes: vec![Mode::Initial],
        }
    }

    pub(crate) fn next_token(&mut self) -> Result<Token, ParseError> {
        loop {
            if self.offset >= self.source.len() {
                let kind = if self.mode() == Mode::Initial {
                    TokenKind::Eof
                } else {
                    TokenKind::End
                };
                return Ok(self.token(kind, 0));
            }
            let rest = &self.source[self.offset..];
            let Some((len, action)) = self.matched(rest) else {
                return Err(self.lexical_error(rest));
            };
            let span = self.span_to(self.offset + len);
            let text = rest[..len].to_string();
            if action.pop {
                self.pop_mode();
            }
            if let Some(mode) = action.push {
                self.modes.push(mode);
            }
            if len > 0 {
                self.advance(self.offset + len);
            } else if !action.pop {
                // Every empty match upstream pops its mode; this guards a future edit from
                // looping forever on the same offset.
                return Err(self.lexical_error(rest));
            }
            if let Some(kind) = action.token {
                return Ok(Token { kind, text, span });
            }
        }
    }

    fn mode(&self) -> Mode {
        *self.modes.last().expect("the stack keeps INITIAL")
    }

    fn pop_mode(&mut self) {
        if self.modes.len() > 1 {
            self.modes.pop();
        }
    }

    fn matched(&self, rest: &str) -> Option<(usize, Action)> {
        match self.mode() {
            Mode::Initial => initial_rule(rest),
            Mode::Title => Some((
                line_value_len(rest),
                Action::pop_token(TokenKind::TitleValue),
            )),
            Mode::AccTitle => Some((
                line_value_len(rest),
                Action::pop_token(TokenKind::AccTitleValue),
            )),
            Mode::AccDescr => Some((
                line_value_len(rest),
                Action::pop_token(TokenKind::AccDescrValue),
            )),
            Mode::AccDescrMultiline => {
                if rest.starts_with('}') {
                    Some((1, Action::pop()))
                } else {
                    Some((
                        rest.find('}').unwrap_or(rest.len()),
                        Action::token(TokenKind::AccDescrMultilineValue),
                    ))
                }
            }
            Mode::Str => {
                if rest.starts_with('"') {
                    Some((1, Action::pop()))
                } else {
                    Some((
                        rest.find('"').unwrap_or(rest.len()),
                        Action::token(TokenKind::Str),
                    ))
                }
            }
            Mode::MdStr => {
                let run = rest.find(['`', '"']).unwrap_or(rest.len());
                if run > 0 {
                    Some((run, Action::token(TokenKind::MdStr)))
                } else if rest.starts_with("`\"") {
                    Some((2, Action::pop()))
                } else {
                    None
                }
            }
            Mode::PointStart => match point_value(rest) {
                Some(len) => Some((len, Action::push_token(Mode::PointX, TokenKind::PointX))),
                None => closing_bracket(rest).map(|len| (len, Action::pop())),
            },
            Mode::PointX => comma(rest).map(|len| (len, Action::pop_push(Mode::PointY))),
            Mode::PointY => {
                point_value(rest).map(|len| (len, Action::pop_token(TokenKind::PointY)))
            }
            Mode::ClassName => {
                word_run(rest).map(|len| (len, Action::pop_token(TokenKind::ClassName)))
            }
        }
    }

    fn token(&self, kind: TokenKind, len: usize) -> Token {
        Token {
            kind,
            text: String::new(),
            span: self.span_to(self.offset + len),
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

    fn lexical_error(&self, rest: &str) -> ParseError {
        let ch = rest.chars().next().expect("non-empty remainder");
        ParseError::at(
            format!("unexpected character '{ch}'"),
            self.span_to(self.offset + ch.len_utf8()),
        )
    }
}

/// The `INITIAL` mode rules, in the order the jison file lists them.
fn initial_rule(rest: &str) -> Option<(usize, Action)> {
    // `\%\%(?!\{)[^\n]*`: a comment line.
    if let Some(after) = rest
        .strip_prefix("%%")
        .filter(|after| !after.starts_with('{'))
    {
        return Some((2 + line_value_len(after), Action::skip()));
    }
    // `[^\}]\%\%[^\n]*`: an inline comment, which swallows the character before it.
    let first_len = rest.chars().next().map_or(0, char::len_utf8);
    if first_len > 0 && !rest.starts_with('}') {
        let after = &rest[first_len..];
        if let Some(after_comment) = after.strip_prefix("%%") {
            return Some((
                first_len + 2 + line_value_len(after_comment),
                Action::skip(),
            ));
        }
    }
    // `[\n\r]+`.
    let newline_len = rest
        .bytes()
        .take_while(|byte| *byte == b'\n' || *byte == b'\r')
        .count();
    if newline_len > 0 {
        return Some((newline_len, Action::token(TokenKind::Newline)));
    }
    // `\%\%[^\n]*`: the rest of a comment, including a one-line `%%{...}%%`.
    if let Some(after) = rest.strip_prefix("%%") {
        return Some((2 + line_value_len(after), Action::skip()));
    }
    // `title\b`.
    if let Some(len) = word_keyword(rest, "title") {
        return Some((len, Action::push_token(Mode::Title, TokenKind::Title)));
    }
    // `accTitle\s*":"\s*`.
    if let Some(len) = accessibility_keyword(rest, "accTitle", ':') {
        return Some((len, Action::push_token(Mode::AccTitle, TokenKind::AccTitle)));
    }
    // `accDescr\s*":"\s*`.
    if let Some(len) = accessibility_keyword(rest, "accDescr", ':') {
        return Some((len, Action::push_token(Mode::AccDescr, TokenKind::AccDescr)));
    }
    // `accDescr\s*"{"\s*`.
    if let Some(len) = accessibility_keyword(rest, "accDescr", '{') {
        return Some((len, Action::push(Mode::AccDescrMultiline)));
    }
    // `" "*"x-axis"" "*` and `" "*"y-axis"" "*`.
    if let Some(len) = spaced_keyword(rest, "x-axis") {
        return Some((len, Action::token(TokenKind::XAxis)));
    }
    if let Some(len) = spaced_keyword(rest, "y-axis") {
        return Some((len, Action::token(TokenKind::YAxis)));
    }
    // `" "*\-\-+\>" "*`.
    if let Some(len) = axis_delimiter(rest) {
        return Some((len, Action::token(TokenKind::AxisTextDelimiter)));
    }
    // `" "*"quadrant-1"" "*` through `quadrant-4`.
    for (word, kind) in [
        ("quadrant-1", TokenKind::Quadrant1),
        ("quadrant-2", TokenKind::Quadrant2),
        ("quadrant-3", TokenKind::Quadrant3),
        ("quadrant-4", TokenKind::Quadrant4),
    ] {
        if let Some(len) = spaced_keyword(rest, word) {
            return Some((len, Action::token(kind)));
        }
    }
    // `"classDef"`.
    if let Some(len) = word_keyword(rest, "classDef") {
        return Some((len, Action::token(TokenKind::ClassDef)));
    }
    // `["][`]`.
    if rest.starts_with("\"`") {
        return Some((2, Action::push(Mode::MdStr)));
    }
    // `["]`.
    if rest.starts_with('"') {
        return Some((1, Action::push(Mode::Str)));
    }
    // `\:\:\:`.
    if rest.starts_with(":::") {
        return Some((3, Action::push(Mode::ClassName)));
    }
    // `\s*\:\s*\[\s*`.
    if let Some(len) = point_start(rest) {
        return Some((
            len,
            Action::push_token(Mode::PointStart, TokenKind::PointStart),
        ));
    }
    // `" "*"quadrantChart"" "*`.
    if let Some(len) = spaced_keyword(rest, "quadrantChart") {
        return Some((len, Action::token(TokenKind::Quadrant)));
    }
    // `[A-Za-z]+`.
    if let Some(len) = alpha_run(rest) {
        return Some((len, Action::token(TokenKind::Alpha)));
    }
    // `[^\x00-\x7F]+`.
    if let Some(len) = unicode_run(rest) {
        return Some((len, Action::token(TokenKind::UnicodeText)));
    }
    let ch = rest.chars().next().expect("non-empty remainder");
    let single = |kind| Some((ch.len_utf8(), Action::token(kind)));
    match ch {
        ':' => single(TokenKind::Colon),
        '+' => single(TokenKind::Plus),
        ',' => single(TokenKind::Comma),
        '=' => single(TokenKind::Equals),
        '*' => single(TokenKind::Mult),
        '#' => single(TokenKind::Brkt),
        '_' => single(TokenKind::Underscore),
        '.' => single(TokenKind::Dot),
        '&' => single(TokenKind::Amp),
        '-' => single(TokenKind::Minus),
        '0'..='9' => {
            let len = rest.bytes().take_while(u8::is_ascii_digit).count();
            Some((len, Action::token(TokenKind::Num)))
        }
        _ if is_js_whitespace(ch) => single(TokenKind::Space),
        ';' => single(TokenKind::Semi),
        // The rest of `[!"#$%&'*+,-.`?\\_/]`; the other characters of that set have a
        // dedicated rule above.
        '!' | '$' | '%' | '\'' | '`' | '?' | '\\' | '/' => single(TokenKind::Punctuation),
        _ => None,
    }
}

/// ` *word *`, with literal spaces only, case-insensitively.
fn spaced_keyword(rest: &str, word: &str) -> Option<usize> {
    let mut start = 0;
    while rest[start..].starts_with(' ') {
        start += 1;
    }
    let head = rest.get(start..start + word.len())?;
    if !head.eq_ignore_ascii_case(word) {
        return None;
    }
    let mut end = start + word.len();
    while rest[end..].starts_with(' ') {
        end += 1;
    }
    Some(end)
}

/// A bare word rule, which `lex-parser` compiles with a trailing `\b`.
fn word_keyword(rest: &str, word: &str) -> Option<usize> {
    let head = rest.get(..word.len())?;
    if !head.eq_ignore_ascii_case(word) {
        return None;
    }
    let boundary = rest[word.len()..]
        .chars()
        .next()
        .is_none_or(|ch| !is_word_char(ch));
    boundary.then_some(word.len())
}

/// `word\s*delimiter\s*`.
fn accessibility_keyword(rest: &str, word: &str, delimiter: char) -> Option<usize> {
    let head = rest.get(..word.len())?;
    if !head.eq_ignore_ascii_case(word) {
        return None;
    }
    let mut end = word.len();
    end += js_space_run(&rest[end..]);
    if !rest[end..].starts_with(delimiter) {
        return None;
    }
    end += delimiter.len_utf8();
    end += js_space_run(&rest[end..]);
    Some(end)
}

/// ` *--+> *`: two or more dashes, then `>`, with literal spaces around.
fn axis_delimiter(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    let mut end = 0;
    while bytes.get(end) == Some(&b' ') {
        end += 1;
    }
    let dashes = end;
    while bytes.get(end) == Some(&b'-') {
        end += 1;
    }
    if end - dashes < 2 || bytes.get(end) != Some(&b'>') {
        return None;
    }
    end += 1;
    while bytes.get(end) == Some(&b' ') {
        end += 1;
    }
    Some(end)
}

/// `\s*\:\s*\[\s*`.
fn point_start(rest: &str) -> Option<usize> {
    let mut end = js_space_run(rest);
    if !rest[end..].starts_with(':') {
        return None;
    }
    end += 1;
    end += js_space_run(&rest[end..]);
    if !rest[end..].starts_with('[') {
        return None;
    }
    end += 1;
    end += js_space_run(&rest[end..]);
    Some(end)
}

/// `\s*\] *`, the rule that closes a point and pops back to `INITIAL`.
fn closing_bracket(rest: &str) -> Option<usize> {
    let mut end = js_space_run(rest);
    if !rest[end..].starts_with(']') {
        return None;
    }
    end += 1;
    while rest[end..].starts_with(' ') {
        end += 1;
    }
    Some(end)
}

/// `\s*\,\s*`.
fn comma(rest: &str) -> Option<usize> {
    let mut end = js_space_run(rest);
    if !rest[end..].starts_with(',') {
        return None;
    }
    end += 1;
    end += js_space_run(&rest[end..]);
    Some(end)
}

/// `(1)|(0(.\d+)?)`, in that alternation order.
///
/// The `.` is unescaped in upstream's pattern, so it matches any character but a line
/// terminator: `[0x5, 0.2]` reads `0x5` as a value, and `[0,0]` reads `0,0` as one.
fn point_value(rest: &str) -> Option<usize> {
    if rest.starts_with('1') {
        return Some(1);
    }
    if !rest.starts_with('0') {
        return None;
    }
    let after_zero = &rest[1..];
    if let Some(ch) = after_zero
        .chars()
        .next()
        .filter(|ch| !is_line_terminator(*ch))
    {
        let after_any = 1 + ch.len_utf8();
        let digits = rest[after_any..]
            .bytes()
            .take_while(u8::is_ascii_digit)
            .count();
        if digits > 0 {
            return Some(after_any + digits);
        }
    }
    Some(1)
}

/// `\w+`, `\w` being ASCII letters, digits, and `_`.
fn word_run(rest: &str) -> Option<usize> {
    let len: usize = rest
        .chars()
        .take_while(|ch| is_word_char(*ch))
        .map(char::len_utf8)
        .sum();
    (len > 0).then_some(len)
}

/// `[A-Za-z]+`.
fn alpha_run(rest: &str) -> Option<usize> {
    let len = rest.bytes().take_while(u8::is_ascii_alphabetic).count();
    (len > 0).then_some(len)
}

/// `[^\x00-\x7F]+`.
fn unicode_run(rest: &str) -> Option<usize> {
    let len: usize = rest
        .chars()
        .take_while(|ch| !ch.is_ascii())
        .map(char::len_utf8)
        .sum();
    (len > 0).then_some(len)
}

/// The length of `[^\n]*` at the start of `text`; `\r` is part of the value.
fn line_value_len(text: &str) -> usize {
    text.find('\n').unwrap_or(text.len())
}

/// The length of `\s*` at the start of `text`.
fn js_space_run(text: &str) -> usize {
    text.char_indices()
        .find(|&(_, ch)| !is_js_whitespace(ch))
        .map_or(text.len(), |(index, _)| index)
}

fn is_word_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}
