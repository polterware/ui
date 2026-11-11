//! The lexer, a hand port of the `%lex` section of upstream's
//! `diagrams/class/parser/classDiagram.jison`.
//!
//! Jison's lexer tries its rules in source order and takes the first that matches
//! (this grammar does not set `%options flex`, which would ask for flex's longest match).
//! Every rule here carries the line of the jison file it comes from as its
//! [`Candidate::order`], so the rule order and the start-condition stack behave as
//! upstream's do. `%x` declares exclusive start conditions: a rule without a state prefix
//! applies only in `INITIAL`, and a `<*>` rule applies in every state.
//!
//! Two dead upstream patterns are not implemented: `[0-9]+` (`NUM`, unreachable because
//! the earlier `\w+` already matches digits) and the unused `struct`/`href` start
//! conditions. `accDescr`'s `(?!\n|;|#)*` prefix is a quantified zero-width lookahead,
//! which matches the empty string and therefore constrains nothing.

use crate::error::{ParseError, Span};

/// A token kind: the jison token names that survive into the grammar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Eof,
    EofInStruct,
    /// Jison's own end-of-input signal (`1`): the grammar's `EOF` token has already been
    /// answered, so the parser sees this as an unexpected token.
    EofSignal,
    Newline,
    ClassDiagram,
    EdgeState,
    Style,
    ClassDef,
    Namespace,
    Class,
    CssClass,
    Callback,
    Link,
    Click,
    NoteFor,
    Note,
    AnnotationStart,
    AnnotationEnd,
    Href,
    LinkTarget,
    CallbackName,
    CallbackArgs,
    Str,
    BqString,
    GenericType,
    Member,
    Label,
    StructStart,
    StructStop,
    OpenInStruct,
    StyleSeparator,
    Minus,
    Dot,
    Plus,
    Colon,
    Comma,
    Brkt,
    Pct,
    Equals,
    Alpha,
    Sqs,
    Sqe,
    Punctuation,
    UnicodeText,
    Space,
    Extension,
    Dependency,
    Composition,
    Aggregation,
    Lollipop,
    Line,
    DottedLine,
    DirectionTb,
    DirectionBt,
    DirectionRl,
    DirectionLr,
    AccTitle,
    AccTitleValue,
    AccDescr,
    AccDescrValue,
    AccDescrMultilineValue,
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
    String,
    BqString,
    Generic,
    CallbackName,
    CallbackArgs,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
    Class,
    ClassBody,
    Namespace,
    NamespaceBody,
}

#[derive(Clone, Copy, Debug)]
struct Candidate {
    /// The jison line of the rule. A lower line wins a length tie.
    order: u16,
    len: usize,
    kind: Option<Kind>,
    push: Option<State>,
    pop: u8,
    error: Option<&'static str>,
}

impl Candidate {
    fn new(order: u16, len: usize) -> Self {
        Self {
            order,
            len,
            kind: None,
            push: None,
            pop: 0,
            error: None,
        }
    }

    fn token(order: u16, len: usize, kind: Kind) -> Self {
        Self {
            kind: Some(kind),
            ..Self::new(order, len)
        }
    }

    fn popping(mut self) -> Self {
        self.pop += 1;
        self
    }

    fn entering(mut self, state: State) -> Self {
        self.push = Some(state);
        self
    }
}

pub(super) fn lex(src: &str) -> Result<Vec<Token>, ParseError> {
    let mut lexer = Lexer {
        src,
        pos: 0,
        line: 1,
        line_start: 0,
        done: false,
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
    done: bool,
    states: Vec<State>,
    tokens: Vec<Token>,
}

impl Lexer<'_> {
    fn run(&mut self) -> Result<(), ParseError> {
        loop {
            // `done` mirrors Jison's lexer: a call that starts at the end of the input
            // marks the lexer done, so the rule that matches the empty string there runs
            // once and the call after it answers the parser's end-of-input signal.
            if self.pos >= self.src.len() {
                if self.done {
                    let span = self.span(0);
                    self.tokens.push(Token {
                        kind: Kind::EofSignal,
                        text: String::new(),
                        span,
                    });
                    return Ok(());
                }
                self.done = true;
            }

            let rest = &self.src[self.pos..];
            let candidate = self.best_candidate(rest);
            if let Some(message) = candidate.error {
                return Err(ParseError::at(message, self.span(0)));
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
                if matches!(kind, Kind::Eof | Kind::EofInStruct) {
                    return Ok(());
                }
            }
            for _ in 0..candidate.pop {
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
        if let Some(offset) = text.rfind('\n') {
            self.line += text.matches('\n').count() as u32;
            self.line_start = self.pos + offset + 1;
        }
        self.pos += len;
    }

    fn state(&self) -> Option<State> {
        self.states.last().copied()
    }

    /// The first rule that matches, which is how Jison's lexer behaves without
    /// `%options flex` (the class grammar does not ask for flex's longest match).
    ///
    /// A rule that matches the empty string still wins when it comes first, which is how
    /// upstream's `<<EOF>>` handling and its `*` rules behave.
    fn best_candidate(&self, rest: &str) -> Candidate {
        let mut candidates: Vec<Candidate> = Vec::new();
        let mut collect = |candidate: Option<Candidate>| {
            if let Some(candidate) = candidate {
                candidates.push(candidate);
            }
        };

        self.consider_state(rest, &mut collect);
        self.consider_global(rest, &mut collect);

        candidates
            .into_iter()
            .min_by_key(|candidate| candidate.order)
            .unwrap_or_else(|| Candidate::new(u16::MAX, 0))
    }

    /// The rules of the current start condition, in jison order.
    fn consider_state(&self, rest: &str, consider: &mut impl FnMut(Option<Candidate>)) {
        match self.state() {
            None => self.consider_initial(rest, consider),
            Some(State::String) => {
                if rest.starts_with('"') {
                    consider(Some(Candidate::new(60, 1).popping()));
                }
                consider(string_run(rest, '"').map(|len| Candidate::token(61, len, Kind::Str)));
            }
            Some(State::BqString) => {
                if rest.starts_with('`') {
                    consider(Some(Candidate::new(110, 1).popping()));
                }
                consider(
                    string_run_plus(rest, '`')
                        .map(|len| Candidate::token(111, len, Kind::BqString)),
                );
            }
            Some(State::Generic) => {
                if rest.starts_with('~') {
                    consider(Some(Candidate::new(106, 1).popping()));
                }
                consider(
                    string_run(rest, '~').map(|len| Candidate::token(107, len, Kind::GenericType)),
                );
            }
            Some(State::CallbackName) => {
                consider(callback_close(rest));
                if rest.starts_with('(') {
                    consider(Some(
                        Candidate::new(55, 1)
                            .popping()
                            .entering(State::CallbackArgs),
                    ));
                }
                consider(
                    string_run(rest, '(').map(|len| Candidate::token(56, len, Kind::CallbackName)),
                );
            }
            Some(State::CallbackArgs) => {
                if rest.starts_with(')') {
                    consider(Some(Candidate::new(57, 1).popping()));
                }
                consider(
                    string_run(rest, ')').map(|len| Candidate::token(58, len, Kind::CallbackArgs)),
                );
            }
            Some(State::AccTitle) => consider(Some(
                Candidate::token(31, line_run(rest), Kind::AccTitleValue).popping(),
            )),
            Some(State::AccDescr) => consider(Some(
                Candidate::token(33, line_run(rest), Kind::AccDescrValue).popping(),
            )),
            Some(State::AccDescrMultiline) => {
                if rest.starts_with('}') {
                    consider(Some(Candidate::new(35, 1).popping()));
                }
                consider(
                    string_run(rest, '}')
                        .map(|len| Candidate::token(36, len, Kind::AccDescrMultilineValue)),
                );
            }
            Some(State::Namespace) => {
                consider(
                    newline_len(rest).map(|len| Candidate::token(67, len, Kind::Newline).popping()),
                );
                consider(ws_run(rest).map(|len| Candidate::new(68, len)));
                if rest.starts_with('{') {
                    consider(Some(
                        Candidate::token(69, 1, Kind::StructStart).entering(State::NamespaceBody),
                    ));
                }
                if rest.starts_with('}') {
                    consider(Some(Candidate {
                        error: Some("Unrecognized text"),
                        ..Candidate::new(70, 1).popping()
                    }));
                }
                consider(namespace_keyword(rest));
            }
            Some(State::NamespaceBody) => {
                if rest.starts_with('}') {
                    consider(Some(Candidate::token(71, 1, Kind::StructStop).popping()));
                }
                if rest.is_empty() {
                    consider(Some(Candidate::token(72, 0, Kind::EofInStruct)));
                }
                consider(newline_len(rest).map(|len| Candidate::token(73, len, Kind::Newline)));
                consider(ws_run(rest).map(|len| Candidate::new(74, len)));
                if rest.starts_with("[*]") {
                    consider(Some(Candidate::token(75, 3, Kind::EdgeState)));
                }
                consider(namespace_keyword(rest));
                consider(class_keyword(rest));
            }
            Some(State::Class) => {
                consider(
                    newline_len(rest).map(|len| Candidate::token(78, len, Kind::Newline).popping()),
                );
                consider(ws_run(rest).map(|len| Candidate::new(79, len)));
                if rest.starts_with('}') {
                    consider(Some(Candidate {
                        pop: 2,
                        ..Candidate::token(80, 1, Kind::StructStop)
                    }));
                }
                if rest.starts_with('{') {
                    consider(Some(
                        Candidate::token(81, 1, Kind::StructStart).entering(State::ClassBody),
                    ));
                }
            }
            Some(State::ClassBody) => {
                if rest.starts_with('}') {
                    consider(Some(Candidate::token(82, 1, Kind::StructStop).popping()));
                }
                if rest.is_empty() {
                    consider(Some(Candidate::token(83, 0, Kind::EofInStruct)));
                }
                if rest.starts_with("[*]") {
                    consider(Some(Candidate::token(84, 3, Kind::EdgeState)));
                }
                if rest.starts_with('{') {
                    consider(Some(Candidate::token(85, 1, Kind::OpenInStruct)));
                }
                if rest.starts_with('\n') {
                    consider(Some(Candidate::new(86, 1)));
                }
                consider(member_run(rest).map(|len| Candidate::token(87, len, Kind::Member)));
            }
        }
    }

    /// The rules of the `INITIAL` condition (no prefix, or an explicit `INITIAL`).
    fn consider_initial(&self, rest: &str, consider: &mut impl FnMut(Option<Candidate>)) {
        consider(direction(rest));
        consider(comment(rest));
        consider(
            keyword_colon(rest, "accTitle")
                .map(|len| Candidate::token(30, len, Kind::AccTitle).entering(State::AccTitle)),
        );
        consider(
            keyword_colon(rest, "accDescr")
                .map(|len| Candidate::token(32, len, Kind::AccDescr).entering(State::AccDescr)),
        );
        consider(
            keyword_brace(rest, "accDescr")
                .map(|len| Candidate::new(34, len).entering(State::AccDescrMultiline)),
        );
        consider(newline_len(rest).map(|len| Candidate::token(38, len, Kind::Newline)));
        consider(ws_run(rest).map(|len| Candidate::new(39, len)));
        if let Some(len) = keyword_len(rest, "classDiagram-v2") {
            consider(Some(Candidate::token(41, len, Kind::ClassDiagram)));
        }
        if let Some(len) = keyword_len(rest, "classDiagram") {
            consider(Some(Candidate::token(42, len, Kind::ClassDiagram)));
        }
        if rest.starts_with("[*]") {
            consider(Some(Candidate::token(43, 3, Kind::EdgeState)));
        }
        consider(call_begin(rest));
        if let Some(len) = keyword_len(rest, "style") {
            consider(Some(Candidate::token(63, len, Kind::Style)));
        }
        if let Some(len) = keyword_len(rest, "classDef") {
            consider(Some(Candidate::token(64, len, Kind::ClassDef)));
        }
        if rest.starts_with(':') {
            consider(Some(Candidate::token(133, 1, Kind::Colon)));
        }
        if rest.starts_with(',') {
            consider(Some(Candidate::token(134, 1, Kind::Comma)));
        }
        if rest.starts_with('#') {
            consider(Some(Candidate::token(135, 1, Kind::Brkt)));
        }
        consider(namespace_keyword(rest));
        consider(class_keyword(rest));
    }

    /// The `<*>` rules: active in every start condition.
    fn consider_global(&self, rest: &str, consider: &mut impl FnMut(Option<Candidate>)) {
        for (line, text, kind) in [
            (89, "cssClass", Kind::CssClass),
            (90, "callback", Kind::Callback),
            (91, "link", Kind::Link),
            (92, "click", Kind::Click),
            (93, "note for", Kind::NoteFor),
            (94, "note", Kind::Note),
        ] {
            if let Some(len) = keyword_len(rest, text) {
                consider(Some(Candidate::token(line, len, kind)));
            }
        }
        if rest.starts_with("<<") {
            consider(Some(Candidate::token(95, 2, Kind::AnnotationStart)));
        }
        if rest.starts_with(">>") {
            consider(Some(Candidate::token(96, 2, Kind::AnnotationEnd)));
        }
        if let Some(len) = keyword_len(rest, "href") {
            consider(Some(Candidate::token(104, len, Kind::Href)));
        }
        for (line, text) in [
            (114, "_self"),
            (115, "_blank"),
            (116, "_parent"),
            (117, "_top"),
        ] {
            if let Some(len) = keyword_len(rest, text) {
                consider(Some(Candidate::token(line, len, Kind::LinkTarget)));
            }
        }
        consider(arrow(rest, "<|").map(|len| Candidate::token(119, len, Kind::Extension)));
        consider(arrow(rest, "|>").map(|len| Candidate::token(120, len, Kind::Extension)));
        consider(arrow(rest, ">").map(|len| Candidate::token(121, len, Kind::Dependency)));
        consider(arrow(rest, "<").map(|len| Candidate::token(122, len, Kind::Dependency)));
        consider(arrow(rest, "*").map(|len| Candidate::token(123, len, Kind::Composition)));
        consider(aggregation(rest).map(|len| Candidate::token(124, len, Kind::Aggregation)));
        consider(arrow(rest, "()").map(|len| Candidate::token(125, len, Kind::Lollipop)));
        if rest.starts_with("--") {
            consider(Some(Candidate::token(126, 2, Kind::Line)));
        }
        if rest.starts_with("..") {
            consider(Some(Candidate::token(127, 2, Kind::DottedLine)));
        }
        consider(label_run(rest).map(|len| Candidate::token(128, len, Kind::Label)));
        if rest.starts_with(":::") {
            consider(Some(Candidate::token(129, 3, Kind::StyleSeparator)));
        }
        if rest.starts_with('-') {
            consider(Some(Candidate::token(130, 1, Kind::Minus)));
        }
        if rest.starts_with('.') {
            consider(Some(Candidate::token(131, 1, Kind::Dot)));
        }
        if rest.starts_with('+') {
            consider(Some(Candidate::token(132, 1, Kind::Plus)));
        }
        if rest.starts_with('%') {
            consider(Some(Candidate::token(137, 1, Kind::Pct)));
        }
        if rest.starts_with('=') {
            consider(Some(Candidate::token(138, 1, Kind::Equals)));
        }
        let alpha = alpha_run(rest);
        if alpha > 0 {
            consider(Some(Candidate::token(140, alpha, Kind::Alpha)));
        }
        if rest.starts_with('[') {
            consider(Some(Candidate::token(141, 1, Kind::Sqs)));
        }
        if rest.starts_with(']') {
            consider(Some(Candidate::token(142, 1, Kind::Sqe)));
        }
        consider(punctuation_len(rest).map(|len| Candidate::token(143, len, Kind::Punctuation)));
        consider(unicode_text_len(rest).map(|len| Candidate::token(145, len, Kind::UnicodeText)));
        consider(single_ws_len(rest).map(|len| Candidate::token(207, len, Kind::Space)));
        if rest.is_empty() {
            consider(Some(Candidate::token(209, 0, Kind::Eof)));
        }
        // The transitions that have no token of their own.
        if rest.starts_with('~') {
            consider(Some(Candidate::new(108, 1).entering(State::Generic)));
        }
        if rest.starts_with('`') {
            consider(Some(Candidate::new(112, 1).entering(State::BqString)));
        }
        if rest.starts_with('"') {
            consider(Some(Candidate::new(62, 1).entering(State::String)));
        }
    }
}

/// A keyword rule. Jison appends `\b` to a pattern that ends in a word character
/// (`lex-parser.js`), so the keyword only matches when no word character follows it.
fn keyword_len(rest: &str, text: &str) -> Option<usize> {
    let after = rest.strip_prefix(text)?;
    let boundary = after.chars().next().is_none_or(|c| !is_word_char(c));
    boundary.then_some(text.len())
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Line 66: `namespace` in `INITIAL`, `namespace`, and `namespace-body`.
fn namespace_keyword(rest: &str) -> Option<Candidate> {
    keyword_len(rest, "namespace")
        .map(|len| Candidate::token(66, len, Kind::Namespace).entering(State::Namespace))
}

/// Line 77: `class` in `INITIAL` and `namespace-body`.
fn class_keyword(rest: &str) -> Option<Candidate> {
    keyword_len(rest, "class")
        .map(|len| Candidate::token(77, len, Kind::Class).entering(State::Class))
}

/// `.*direction\s+(TB|BT|RL|LR)[^\n]*`: the whole line becomes one direction token.
/// `.` stops at a newline, so `direction` must be on the current line; `\s+` may cross
/// one, as `\s` does in JavaScript.
fn direction(rest: &str) -> Option<Candidate> {
    for (word, kind, order) in [
        ("TB", Kind::DirectionTb, 24u16),
        ("BT", Kind::DirectionBt, 25),
        ("RL", Kind::DirectionRl, 26),
        ("LR", Kind::DirectionLr, 27),
    ] {
        let line_end = rest.find('\n').unwrap_or(rest.len());
        let line = &rest[..line_end];
        let mut search_from = 0;
        while let Some(found) = line[search_from..].find("direction") {
            let at = search_from + found + "direction".len();
            let ws = ws_run(&rest[at..]).unwrap_or(0);
            if ws > 0 {
                let keyword_at = at + ws;
                if rest[keyword_at..].starts_with(word) {
                    let end = rest[keyword_at + word.len()..]
                        .find('\n')
                        .map(|offset| keyword_at + word.len() + offset)
                        .unwrap_or(rest.len());
                    return Some(Candidate::token(order, end, kind));
                }
            }
            search_from = at;
        }
    }
    None
}

/// `\%\%(?!\{)*[^\n]*(\r?\n?)+` and its twin: a comment line and the newlines after it.
fn comment(rest: &str) -> Option<Candidate> {
    if !rest.starts_with("%%") {
        return None;
    }
    let mut len = rest.find('\n').unwrap_or(rest.len());
    while rest[len..].starts_with('\r') || rest[len..].starts_with('\n') {
        len += 1;
    }
    Some(Candidate::new(28, len))
}

/// `<INITIAL>"call"[\s]+`.
fn call_begin(rest: &str) -> Option<Candidate> {
    let after = rest.strip_prefix("call")?;
    let ws = ws_run(after)?;
    Some(Candidate::new(53, "call".len() + ws).entering(State::CallbackName))
}

/// `<callback_name>\([\s]*\)`.
fn callback_close(rest: &str) -> Option<Candidate> {
    let after = rest.strip_prefix('(')?;
    let ws = ws_run(after).unwrap_or(0);
    after[ws..]
        .starts_with(')')
        .then(|| Candidate::new(54, 1 + ws + 1).popping())
}

/// `accTitle\s*":"\s*` and `accDescr\s*":"\s*`.
fn keyword_colon(rest: &str, keyword: &str) -> Option<usize> {
    let after = rest.strip_prefix(keyword)?;
    let head = ws_run(after).unwrap_or(0);
    let after = after[head..].strip_prefix(':')?;
    let tail = ws_run(after).unwrap_or(0);
    Some(keyword.len() + head + 1 + tail)
}

/// `accDescr\s*"{"\s*`.
fn keyword_brace(rest: &str, keyword: &str) -> Option<usize> {
    let after = rest.strip_prefix(keyword)?;
    let head = ws_run(after).unwrap_or(0);
    let after = after[head..].strip_prefix('{')?;
    let tail = ws_run(after).unwrap_or(0);
    Some(keyword.len() + head + 1 + tail)
}

/// `[^c]*`: the byte length up to the next `c` (possibly zero).
fn string_run(rest: &str, until: char) -> Option<usize> {
    Some(rest.find(until).unwrap_or(rest.len()))
}

/// `[^c]+`: the byte length up to the next `c`, when there is at least one.
fn string_run_plus(rest: &str, until: char) -> Option<usize> {
    string_run(rest, until).filter(|len| *len > 0)
}

/// `<class-body>[^{}\n]*`.
fn member_run(rest: &str) -> Option<usize> {
    Some(rest.find(['{', '}', '\n']).unwrap_or(rest.len()))
}

fn line_run(rest: &str) -> usize {
    rest.find('\n').unwrap_or(rest.len())
}

/// `\s*(\r?\n)+`: the leading whitespace up to and including its last newline.
fn newline_len(rest: &str) -> Option<usize> {
    let mut last = None;
    let mut index = 0;
    for c in rest.chars() {
        if !is_js_whitespace(c) {
            break;
        }
        index += c.len_utf8();
        if c == '\n' {
            last = Some(index);
        }
    }
    last
}

fn ws_run(rest: &str) -> Option<usize> {
    let len = rest
        .chars()
        .take_while(|c| is_js_whitespace(*c))
        .map(char::len_utf8)
        .sum();
    (len > 0).then_some(len)
}

/// `<*>\s`, one whitespace character.
fn single_ws_len(rest: &str) -> Option<usize> {
    let c = rest.chars().next()?;
    is_js_whitespace(c).then_some(c.len_utf8())
}

/// `\s*o\b`: the aggregation marker, which Jison gives the same word boundary as a
/// keyword (so a name like `order` is not an aggregation).
fn aggregation(rest: &str) -> Option<usize> {
    let ws = ws_run(rest).unwrap_or(0);
    let after = rest[ws..].strip_prefix('o')?;
    after
        .chars()
        .next()
        .is_none_or(|c| !is_word_char(c))
        .then_some(ws + 1)
}

/// `\s*<\|`, `\s*\|>`, `\s*>`, `\s*<`, `\s*\*`, `\s*\(\)`.
fn arrow(rest: &str, text: &str) -> Option<usize> {
    let ws = ws_run(rest).unwrap_or(0);
    rest[ws..].starts_with(text).then_some(ws + text.len())
}

/// `":"{1}[^:\n;]+`.
fn label_run(rest: &str) -> Option<usize> {
    let after = rest.strip_prefix(':')?;
    let len = after
        .chars()
        .take_while(|c| *c != ':' && *c != '\n' && *c != ';')
        .map(char::len_utf8)
        .sum::<usize>();
    (len > 0).then_some(1 + len)
}

/// `\w+`.
fn alpha_run(rest: &str) -> usize {
    rest.chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .map(char::len_utf8)
        .sum()
}

/// `[!"#$%&'*+,-.`?\\/]`.
fn punctuation_len(rest: &str) -> Option<usize> {
    let c = rest.chars().next()?;
    const PUNCTUATION: &str = "!\"#$%&'*+,-.`?\\/";
    PUNCTUATION.contains(c).then_some(c.len_utf8())
}

/// The `UNICODE_TEXT` character class of the jison file, as code point ranges.
fn unicode_text_len(rest: &str) -> Option<usize> {
    let c = rest.chars().next()?;
    let code = c as u32;
    let found = UNICODE_TEXT_RANGES
        .binary_search_by(|(start, end)| {
            if code < *start {
                std::cmp::Ordering::Greater
            } else if code > *end {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok();
    found.then_some(c.len_utf8())
}

/// JavaScript's `\s`.
fn is_js_whitespace(c: char) -> bool {
    c.is_whitespace() || c == '\u{feff}'
}

const UNICODE_TEXT_RANGES: &[(u32, u32)] = &[
    (0x00AA, 0x00AA),
    (0x00B5, 0x00B5),
    (0x00BA, 0x00BA),
    (0x00C0, 0x00D6),
    (0x00D8, 0x00F6),
    (0x00F8, 0x02C1),
    (0x02C6, 0x02D1),
    (0x02E0, 0x02E4),
    (0x02EC, 0x02EC),
    (0x02EE, 0x02EE),
    (0x0370, 0x0374),
    (0x0376, 0x0376),
    (0x0377, 0x0377),
    (0x037A, 0x037D),
    (0x0386, 0x0386),
    (0x0388, 0x038A),
    (0x038C, 0x038C),
    (0x038E, 0x03A1),
    (0x03A3, 0x03F5),
    (0x03F7, 0x0481),
    (0x048A, 0x0527),
    (0x0531, 0x0556),
    (0x0559, 0x0559),
    (0x0561, 0x0587),
    (0x05D0, 0x05EA),
    (0x05F0, 0x05F2),
    (0x0620, 0x064A),
    (0x066E, 0x066E),
    (0x066F, 0x066F),
    (0x0671, 0x06D3),
    (0x06D5, 0x06D5),
    (0x06E5, 0x06E5),
    (0x06E6, 0x06E6),
    (0x06EE, 0x06EE),
    (0x06EF, 0x06EF),
    (0x06FA, 0x06FC),
    (0x06FF, 0x06FF),
    (0x0710, 0x0710),
    (0x0712, 0x072F),
    (0x074D, 0x07A5),
    (0x07B1, 0x07B1),
    (0x07CA, 0x07EA),
    (0x07F4, 0x07F4),
    (0x07F5, 0x07F5),
    (0x07FA, 0x07FA),
    (0x0800, 0x0815),
    (0x081A, 0x081A),
    (0x0824, 0x0824),
    (0x0828, 0x0828),
    (0x0840, 0x0858),
    (0x08A0, 0x08A0),
    (0x08A2, 0x08AC),
    (0x0904, 0x0939),
    (0x093D, 0x093D),
    (0x0950, 0x0950),
    (0x0958, 0x0961),
    (0x0971, 0x0977),
    (0x0979, 0x097F),
    (0x0985, 0x098C),
    (0x098F, 0x098F),
    (0x0990, 0x0990),
    (0x0993, 0x09A8),
    (0x09AA, 0x09B0),
    (0x09B2, 0x09B2),
    (0x09B6, 0x09B9),
    (0x09BD, 0x09BD),
    (0x09CE, 0x09CE),
    (0x09DC, 0x09DC),
    (0x09DD, 0x09DD),
    (0x09DF, 0x09E1),
    (0x09F0, 0x09F0),
    (0x09F1, 0x09F1),
    (0x0A05, 0x0A0A),
    (0x0A0F, 0x0A0F),
    (0x0A10, 0x0A10),
    (0x0A13, 0x0A28),
    (0x0A2A, 0x0A30),
    (0x0A32, 0x0A32),
    (0x0A33, 0x0A33),
    (0x0A35, 0x0A35),
    (0x0A36, 0x0A36),
    (0x0A38, 0x0A38),
    (0x0A39, 0x0A39),
    (0x0A59, 0x0A5C),
    (0x0A5E, 0x0A5E),
    (0x0A72, 0x0A74),
    (0x0A85, 0x0A8D),
    (0x0A8F, 0x0A91),
    (0x0A93, 0x0AA8),
    (0x0AAA, 0x0AB0),
    (0x0AB2, 0x0AB2),
    (0x0AB3, 0x0AB3),
    (0x0AB5, 0x0AB9),
    (0x0ABD, 0x0ABD),
    (0x0AD0, 0x0AD0),
    (0x0AE0, 0x0AE0),
    (0x0AE1, 0x0AE1),
    (0x0B05, 0x0B0C),
    (0x0B0F, 0x0B0F),
    (0x0B10, 0x0B10),
    (0x0B13, 0x0B28),
    (0x0B2A, 0x0B30),
    (0x0B32, 0x0B32),
    (0x0B33, 0x0B33),
    (0x0B35, 0x0B39),
    (0x0B3D, 0x0B3D),
    (0x0B5C, 0x0B5C),
    (0x0B5D, 0x0B5D),
    (0x0B5F, 0x0B61),
    (0x0B71, 0x0B71),
    (0x0B83, 0x0B83),
    (0x0B85, 0x0B8A),
    (0x0B8E, 0x0B90),
    (0x0B92, 0x0B95),
    (0x0B99, 0x0B99),
    (0x0B9A, 0x0B9A),
    (0x0B9C, 0x0B9C),
    (0x0B9E, 0x0B9E),
    (0x0B9F, 0x0B9F),
    (0x0BA3, 0x0BA3),
    (0x0BA4, 0x0BA4),
    (0x0BA8, 0x0BAA),
    (0x0BAE, 0x0BB9),
    (0x0BD0, 0x0BD0),
    (0x0C05, 0x0C0C),
    (0x0C0E, 0x0C10),
    (0x0C12, 0x0C28),
    (0x0C2A, 0x0C33),
    (0x0C35, 0x0C39),
    (0x0C3D, 0x0C3D),
    (0x0C58, 0x0C58),
    (0x0C59, 0x0C59),
    (0x0C60, 0x0C60),
    (0x0C61, 0x0C61),
    (0x0C85, 0x0C8C),
    (0x0C8E, 0x0C90),
    (0x0C92, 0x0CA8),
    (0x0CAA, 0x0CB3),
    (0x0CB5, 0x0CB9),
    (0x0CBD, 0x0CBD),
    (0x0CDE, 0x0CDE),
    (0x0CE0, 0x0CE0),
    (0x0CE1, 0x0CE1),
    (0x0CF1, 0x0CF1),
    (0x0CF2, 0x0CF2),
    (0x0D05, 0x0D0C),
    (0x0D0E, 0x0D10),
    (0x0D12, 0x0D3A),
    (0x0D3D, 0x0D3D),
    (0x0D4E, 0x0D4E),
    (0x0D60, 0x0D60),
    (0x0D61, 0x0D61),
    (0x0D7A, 0x0D7F),
    (0x0D85, 0x0D96),
    (0x0D9A, 0x0DB1),
    (0x0DB3, 0x0DBB),
    (0x0DBD, 0x0DBD),
    (0x0DC0, 0x0DC6),
    (0x0E01, 0x0E30),
    (0x0E32, 0x0E32),
    (0x0E33, 0x0E33),
    (0x0E40, 0x0E46),
    (0x0E81, 0x0E81),
    (0x0E82, 0x0E82),
    (0x0E84, 0x0E84),
    (0x0E87, 0x0E87),
    (0x0E88, 0x0E88),
    (0x0E8A, 0x0E8A),
    (0x0E8D, 0x0E8D),
    (0x0E94, 0x0E97),
    (0x0E99, 0x0E9F),
    (0x0EA1, 0x0EA3),
    (0x0EA5, 0x0EA5),
    (0x0EA7, 0x0EA7),
    (0x0EAA, 0x0EAA),
    (0x0EAB, 0x0EAB),
    (0x0EAD, 0x0EB0),
    (0x0EB2, 0x0EB2),
    (0x0EB3, 0x0EB3),
    (0x0EBD, 0x0EBD),
    (0x0EC0, 0x0EC4),
    (0x0EC6, 0x0EC6),
    (0x0EDC, 0x0EDF),
    (0x0F00, 0x0F00),
    (0x0F40, 0x0F47),
    (0x0F49, 0x0F6C),
    (0x0F88, 0x0F8C),
    (0x1000, 0x102A),
    (0x103F, 0x103F),
    (0x1050, 0x1055),
    (0x105A, 0x105D),
    (0x1061, 0x1061),
    (0x1065, 0x1065),
    (0x1066, 0x1066),
    (0x106E, 0x1070),
    (0x1075, 0x1081),
    (0x108E, 0x108E),
    (0x10A0, 0x10C5),
    (0x10C7, 0x10C7),
    (0x10CD, 0x10CD),
    (0x10D0, 0x10FA),
    (0x10FC, 0x1248),
    (0x124A, 0x124D),
    (0x1250, 0x1256),
    (0x1258, 0x1258),
    (0x125A, 0x125D),
    (0x1260, 0x1288),
    (0x128A, 0x128D),
    (0x1290, 0x12B0),
    (0x12B2, 0x12B5),
    (0x12B8, 0x12BE),
    (0x12C0, 0x12C0),
    (0x12C2, 0x12C5),
    (0x12C8, 0x12D6),
    (0x12D8, 0x1310),
    (0x1312, 0x1315),
    (0x1318, 0x135A),
    (0x1380, 0x138F),
    (0x13A0, 0x13F4),
    (0x1401, 0x166C),
    (0x166F, 0x167F),
    (0x1681, 0x169A),
    (0x16A0, 0x16EA),
    (0x1700, 0x170C),
    (0x170E, 0x1711),
    (0x1720, 0x1731),
    (0x1740, 0x1751),
    (0x1760, 0x176C),
    (0x176E, 0x1770),
    (0x1780, 0x17B3),
    (0x17D7, 0x17D7),
    (0x17DC, 0x17DC),
    (0x1820, 0x1877),
    (0x1880, 0x18A8),
    (0x18AA, 0x18AA),
    (0x18B0, 0x18F5),
    (0x1900, 0x191C),
    (0x1950, 0x196D),
    (0x1970, 0x1974),
    (0x1980, 0x19AB),
    (0x19C1, 0x19C7),
    (0x1A00, 0x1A16),
    (0x1A20, 0x1A54),
    (0x1AA7, 0x1AA7),
    (0x1B05, 0x1B33),
    (0x1B45, 0x1B4B),
    (0x1B83, 0x1BA0),
    (0x1BAE, 0x1BAE),
    (0x1BAF, 0x1BAF),
    (0x1BBA, 0x1BE5),
    (0x1C00, 0x1C23),
    (0x1C4D, 0x1C4F),
    (0x1C5A, 0x1C7D),
    (0x1CE9, 0x1CEC),
    (0x1CEE, 0x1CF1),
    (0x1CF5, 0x1CF5),
    (0x1CF6, 0x1CF6),
    (0x1D00, 0x1DBF),
    (0x1E00, 0x1F15),
    (0x1F18, 0x1F1D),
    (0x1F20, 0x1F45),
    (0x1F48, 0x1F4D),
    (0x1F50, 0x1F57),
    (0x1F59, 0x1F59),
    (0x1F5B, 0x1F5B),
    (0x1F5D, 0x1F5D),
    (0x1F5F, 0x1F7D),
    (0x1F80, 0x1FB4),
    (0x1FB6, 0x1FBC),
    (0x1FBE, 0x1FBE),
    (0x1FC2, 0x1FC4),
    (0x1FC6, 0x1FCC),
    (0x1FD0, 0x1FD3),
    (0x1FD6, 0x1FDB),
    (0x1FE0, 0x1FEC),
    (0x1FF2, 0x1FF4),
    (0x1FF6, 0x1FFC),
    (0x2071, 0x2071),
    (0x207F, 0x207F),
    (0x2090, 0x209C),
    (0x2102, 0x2102),
    (0x2107, 0x2107),
    (0x210A, 0x2113),
    (0x2115, 0x2115),
    (0x2119, 0x211D),
    (0x2124, 0x2124),
    (0x2126, 0x2126),
    (0x2128, 0x2128),
    (0x212A, 0x212D),
    (0x212F, 0x2139),
    (0x213C, 0x213F),
    (0x2145, 0x2149),
    (0x214E, 0x214E),
    (0x2183, 0x2183),
    (0x2184, 0x2184),
    (0x2C00, 0x2C2E),
    (0x2C30, 0x2C5E),
    (0x2C60, 0x2CE4),
    (0x2CEB, 0x2CEE),
    (0x2CF2, 0x2CF2),
    (0x2CF3, 0x2CF3),
    (0x2D00, 0x2D25),
    (0x2D27, 0x2D27),
    (0x2D2D, 0x2D2D),
    (0x2D30, 0x2D67),
    (0x2D6F, 0x2D6F),
    (0x2D80, 0x2D96),
    (0x2DA0, 0x2DA6),
    (0x2DA8, 0x2DAE),
    (0x2DB0, 0x2DB6),
    (0x2DB8, 0x2DBE),
    (0x2DC0, 0x2DC6),
    (0x2DC8, 0x2DCE),
    (0x2DD0, 0x2DD6),
    (0x2DD8, 0x2DDE),
    (0x2E2F, 0x2E2F),
    (0x3005, 0x3005),
    (0x3006, 0x3006),
    (0x3031, 0x3035),
    (0x303B, 0x303B),
    (0x303C, 0x303C),
    (0x3041, 0x3096),
    (0x309D, 0x309F),
    (0x30A1, 0x30FA),
    (0x30FC, 0x30FF),
    (0x3105, 0x312D),
    (0x3131, 0x318E),
    (0x31A0, 0x31BA),
    (0x31F0, 0x31FF),
    (0x3400, 0x4DB5),
    (0x4E00, 0x9FCC),
    (0xA000, 0xA48C),
    (0xA4D0, 0xA4FD),
    (0xA500, 0xA60C),
    (0xA610, 0xA61F),
    (0xA62A, 0xA62A),
    (0xA62B, 0xA62B),
    (0xA640, 0xA66E),
    (0xA67F, 0xA697),
    (0xA6A0, 0xA6E5),
    (0xA717, 0xA71F),
    (0xA722, 0xA788),
    (0xA78B, 0xA78E),
    (0xA790, 0xA793),
    (0xA7A0, 0xA7AA),
    (0xA7F8, 0xA801),
    (0xA803, 0xA805),
    (0xA807, 0xA80A),
    (0xA80C, 0xA822),
    (0xA840, 0xA873),
    (0xA882, 0xA8B3),
    (0xA8F2, 0xA8F7),
    (0xA8FB, 0xA8FB),
    (0xA90A, 0xA925),
    (0xA930, 0xA946),
    (0xA960, 0xA97C),
    (0xA984, 0xA9B2),
    (0xA9CF, 0xA9CF),
    (0xAA00, 0xAA28),
    (0xAA40, 0xAA42),
    (0xAA44, 0xAA4B),
    (0xAA60, 0xAA76),
    (0xAA7A, 0xAA7A),
    (0xAA80, 0xAAAF),
    (0xAAB1, 0xAAB1),
    (0xAAB5, 0xAAB5),
    (0xAAB6, 0xAAB6),
    (0xAAB9, 0xAABD),
    (0xAAC0, 0xAAC0),
    (0xAAC2, 0xAAC2),
    (0xAADB, 0xAADD),
    (0xAAE0, 0xAAEA),
    (0xAAF2, 0xAAF4),
    (0xAB01, 0xAB06),
    (0xAB09, 0xAB0E),
    (0xAB11, 0xAB16),
    (0xAB20, 0xAB26),
    (0xAB28, 0xAB2E),
    (0xABC0, 0xABE2),
    (0xAC00, 0xD7A3),
    (0xD7B0, 0xD7C6),
    (0xD7CB, 0xD7FB),
    (0xF900, 0xFA6D),
    (0xFA70, 0xFAD9),
    (0xFB00, 0xFB06),
    (0xFB13, 0xFB17),
    (0xFB1D, 0xFB1D),
    (0xFB1F, 0xFB28),
    (0xFB2A, 0xFB36),
    (0xFB38, 0xFB3C),
    (0xFB3E, 0xFB3E),
    (0xFB40, 0xFB40),
    (0xFB41, 0xFB41),
    (0xFB43, 0xFB43),
    (0xFB44, 0xFB44),
    (0xFB46, 0xFBB1),
    (0xFBD3, 0xFD3D),
    (0xFD50, 0xFD8F),
    (0xFD92, 0xFDC7),
    (0xFDF0, 0xFDFB),
    (0xFE70, 0xFE74),
    (0xFE76, 0xFEFC),
    (0xFF21, 0xFF3A),
    (0xFF41, 0xFF5A),
    (0xFF66, 0xFFBE),
    (0xFFC2, 0xFFC7),
    (0xFFCA, 0xFFCF),
    (0xFFD2, 0xFFD7),
    (0xFFDA, 0xFFDC),
];
