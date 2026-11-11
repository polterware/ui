//! The lexer of the state grammar, copied rule by rule from upstream
//! `parser/stateDiagram.jison`: the start conditions (`%x STATE STATE_STRING STATE_ID SCALE
//! acc_title acc_descr acc_descr_multiline CLASSDEF CLASSDEFID CLASS CLASS_STYLE STYLE
//! STYLE_IDS STYLEDEF_STYLES STYLEDEF_STYLEOPTS NOTE NOTE_ID NOTE_TEXT FLOATING_NOTE
//! FLOATING_NOTE_ID struct`) and the token regexes.
//!
//! Two jison details are reproduced on purpose: with `%options case-insensitive` every
//! literal matches regardless of case, and without `%options flex` the first rule of the
//! active condition that matches wins (jison breaks on the first match instead of looking
//! for the longest). Rules without a `<condition>` prefix belong to `INITIAL` only, because
//! every condition declared here is exclusive (`%x`).
//!
//! The start conditions upstream declares but never enters (`ID`, `FORK_STATE`, `ALIAS`,
//! `LINE`) have no rules and no transitions, so they are not modeled.

use crate::error::{ParseError, Span};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Initial,
    State,
    StateString,
    StateId,
    Scale,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
    ClassDef,
    ClassDefId,
    Class,
    ClassStyle,
    Style,
    StyleDefStyles,
    Note,
    NoteId,
    NoteText,
    FloatingNote,
    FloatingNoteId,
    Struct,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    AccDescr,
    AccDescrMultilineValue,
    AccDescrValue,
    AccTitle,
    AccTitleValue,
    Arrow,
    As,
    Class,
    ClassDef,
    ClassDefId,
    ClassDefStyleOptions,
    ClassEntityIds,
    Choice,
    Click,
    CompositState,
    Concurrent,
    Default,
    DefaultClassDefId,
    Descr,
    DirectionBt,
    DirectionLr,
    DirectionRl,
    DirectionTb,
    EdgeState,
    Eof,
    Fork,
    HideEmpty,
    Href,
    Id,
    Invalid,
    Join,
    LeftOf,
    Newline,
    Note,
    NoteText,
    RightOf,
    Scale,
    Sd,
    StateDescr,
    String,
    StructStart,
    StructStop,
    Style,
    StyleClass,
    StyleIds,
    StyleDefStyleOptions,
    StyleSeparator,
    Width,
}

impl TokenKind {
    /// The name jison prints for the terminal in a parse error.
    pub fn name(self) -> &'static str {
        match self {
            TokenKind::AccDescr => "acc_descr",
            TokenKind::AccDescrMultilineValue => "acc_descr_multiline_value",
            TokenKind::AccDescrValue => "acc_descr_value",
            TokenKind::AccTitle => "acc_title",
            TokenKind::AccTitleValue => "acc_title_value",
            TokenKind::Arrow => "-->",
            TokenKind::As => "AS",
            TokenKind::Class => "class",
            TokenKind::ClassDef => "classDef",
            TokenKind::ClassDefId => "CLASSDEF_ID",
            TokenKind::ClassDefStyleOptions => "CLASSDEF_STYLEOPTS",
            TokenKind::ClassEntityIds => "CLASSENTITY_IDS",
            TokenKind::Choice => "CHOICE",
            TokenKind::Click => "CLICK",
            TokenKind::CompositState => "COMPOSIT_STATE",
            TokenKind::Concurrent => "CONCURRENT",
            TokenKind::Default => "DEFAULT",
            TokenKind::DefaultClassDefId => "DEFAULT_CLASSDEF_ID",
            TokenKind::Descr => "DESCR",
            TokenKind::DirectionBt => "direction_bt",
            TokenKind::DirectionLr => "direction_lr",
            TokenKind::DirectionRl => "direction_rl",
            TokenKind::DirectionTb => "direction_tb",
            TokenKind::EdgeState => "EDGE_STATE",
            TokenKind::Eof => "end of input",
            TokenKind::Fork => "FORK",
            TokenKind::HideEmpty => "HIDE_EMPTY",
            TokenKind::Href => "HREF",
            TokenKind::Id => "ID",
            TokenKind::Invalid => "INVALID",
            TokenKind::Join => "JOIN",
            TokenKind::LeftOf => "left_of",
            TokenKind::Newline => "NL",
            TokenKind::Note => "note",
            TokenKind::NoteText => "NOTE_TEXT",
            TokenKind::RightOf => "right_of",
            TokenKind::Scale => "scale",
            TokenKind::Sd => "SD",
            TokenKind::StateDescr => "STATE_DESCR",
            TokenKind::String => "STRING",
            TokenKind::StructStart => "STRUCT_START",
            TokenKind::StructStop => "STRUCT_STOP",
            TokenKind::Style => "style",
            TokenKind::StyleClass => "STYLECLASS",
            TokenKind::StyleIds => "STYLE_IDS",
            TokenKind::StyleDefStyleOptions => "STYLEDEF_STYLEOPTS",
            TokenKind::StyleSeparator => "STYLE_SEPARATOR",
            TokenKind::Width => "WIDTH",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub span: Span,
}

/// What a matched rule does to the lexer and the parser.
enum Step {
    Skip(usize),
    Token(TokenKind, usize),
    Text(TokenKind, usize, String),
    Push(Mode, usize),
    PushToken(Mode, TokenKind, usize),
    Pop(usize),
    PopToken(TokenKind, usize),
    PopText(TokenKind, usize, String),
    PopPushText(Mode, TokenKind, usize, String),
    PopPushToken(Mode, TokenKind, usize),
    Replace(Mode, usize),
    ReplaceToken(Mode, TokenKind, usize),
}

pub(crate) struct Lexer<'a> {
    src: &'a str,
    pos: usize,
    line: u32,
    column: u32,
    modes: Vec<Mode>,
    eof_newline: bool,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Self {
            src,
            pos: 0,
            line: 1,
            column: 1,
            modes: vec![Mode::Initial],
            eof_newline: false,
        }
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

    fn span_at(&self, pos: usize) -> Span {
        Span {
            line: self.line,
            column: self.column,
            start: pos,
            end: pos,
        }
    }

    fn span_to(&self, end: usize) -> Span {
        Span {
            line: self.line,
            column: self.column,
            start: self.pos,
            end,
        }
    }

    fn advance(&mut self, end: usize) {
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

    fn lexical_error(&self) -> ParseError {
        ParseError::at(
            format!("Lexical error on line {}. Unrecognized text.", self.line),
            self.span_at(self.pos),
        )
    }

    pub(crate) fn next_token(&mut self) -> Result<Token, ParseError> {
        loop {
            if self.pos >= self.src.len() {
                let span = self.span_at(self.pos);
                if self.mode() == Mode::Initial && !self.eof_newline {
                    self.eof_newline = true;
                    return Ok(Token {
                        kind: TokenKind::Newline,
                        text: String::new(),
                        span,
                    });
                }
                return Ok(Token {
                    kind: TokenKind::Eof,
                    text: String::new(),
                    span,
                });
            }
            let mode = self.mode();
            // `step` works on the remaining source, so every offset is relative to `base`.
            let base = self.pos;
            let step = self.step(mode)?;
            match step {
                Step::Skip(end) => self.advance(base + end),
                Step::Token(kind, end) => return Ok(self.finish(kind, base + end, None)),
                Step::Text(kind, end, text) => {
                    return Ok(self.finish(kind, base + end, Some(text)));
                }
                Step::Push(mode, end) => {
                    self.advance(base + end);
                    self.push(mode);
                }
                Step::PushToken(mode, kind, end) => {
                    let token = self.finish(kind, base + end, None);
                    self.push(mode);
                    return Ok(token);
                }
                Step::Pop(end) => {
                    self.advance(base + end);
                    self.pop();
                }
                Step::PopToken(kind, end) => {
                    let token = self.finish(kind, base + end, None);
                    self.pop();
                    return Ok(token);
                }
                Step::PopText(kind, end, text) => {
                    let token = self.finish(kind, base + end, Some(text));
                    self.pop();
                    return Ok(token);
                }
                Step::PopPushText(mode, kind, end, text) => {
                    let token = self.finish(kind, base + end, Some(text));
                    self.pop();
                    self.push(mode);
                    return Ok(token);
                }
                Step::PopPushToken(mode, kind, end) => {
                    let token = self.finish(kind, base + end, None);
                    self.pop();
                    self.push(mode);
                    return Ok(token);
                }
                Step::Replace(mode, end) => {
                    self.advance(base + end);
                    self.pop();
                    self.push(mode);
                }
                Step::ReplaceToken(mode, kind, end) => {
                    let token = self.finish(kind, base + end, None);
                    self.pop();
                    self.push(mode);
                    return Ok(token);
                }
            }
        }
    }

    fn finish(&mut self, kind: TokenKind, end: usize, text: Option<String>) -> Token {
        let span = self.span_to(end);
        let text = text.unwrap_or_else(|| self.src[self.pos..end].to_string());
        self.advance(end);
        Token { kind, text, span }
    }

    fn step(&self, mode: Mode) -> Result<Step, ParseError> {
        let s = &self.src[self.pos..];
        match mode {
            Mode::Initial => self.step_initial(s),
            Mode::State => self.step_state(s),
            Mode::StateString => self.step_state_string(s),
            Mode::StateId => self.step_state_id(s),
            Mode::Scale => self.step_scale(s),
            Mode::AccTitle => Ok(Step::PopToken(TokenKind::AccTitleValue, line_end(s))),
            Mode::AccDescr => Ok(Step::PopToken(TokenKind::AccDescrValue, line_end(s))),
            Mode::AccDescrMultiline => self.step_acc_descr_multiline(s),
            Mode::ClassDef => self.step_class_def(s),
            Mode::ClassDefId => Ok(Step::PopToken(TokenKind::ClassDefStyleOptions, line_end(s))),
            Mode::Class => self.step_class(s),
            Mode::ClassStyle => Ok(Step::PopToken(TokenKind::StyleClass, line_end(s))),
            Mode::Style => self.step_style(s),
            Mode::StyleDefStyles => {
                Ok(Step::PopToken(TokenKind::StyleDefStyleOptions, line_end(s)))
            }
            Mode::Note => self.step_note(s),
            Mode::NoteId => self.step_note_id(s),
            Mode::NoteText => self.step_note_text(s),
            Mode::FloatingNote => self.step_floating_note(s),
            Mode::FloatingNoteId => self.step_floating_note_id(s),
            Mode::Struct => self.step_struct(s),
        }
    }

    fn step_initial(&self, s: &str) -> Result<Step, ParseError> {
        if let Some(n) = literal(s, "click") {
            return Ok(Step::Token(TokenKind::Click, n));
        }
        if let Some(n) = literal(s, "href") {
            return Ok(Step::Token(TokenKind::Href, n));
        }
        if let Some(n) = quoted(s) {
            return Ok(Step::Token(TokenKind::String, n));
        }
        if let Some(n) = literal(s, "default") {
            return Ok(Step::Token(TokenKind::Default, n));
        }
        for (dir, kind) in [
            ("TB", TokenKind::DirectionTb),
            ("BT", TokenKind::DirectionBt),
            ("RL", TokenKind::DirectionRl),
            ("LR", TokenKind::DirectionLr),
        ] {
            if let Some(n) = direction_end(s, dir) {
                return Ok(Step::Token(kind, n));
            }
        }
        if let Some(n) = one_plus(s, |c| c == '\n') {
            return Ok(Step::Token(TokenKind::Newline, n));
        }
        if let Some(n) = one_plus(s, char::is_whitespace) {
            return Ok(Step::Skip(n));
        }
        if s.starts_with('#') {
            return Ok(Step::Skip(line_end(s)));
        }
        if let Some(n) = percent_comment(s) {
            return Ok(Step::Skip(n));
        }
        if let Some(n) = keyword_ws(s, "scale") {
            return Ok(Step::PushToken(Mode::Scale, TokenKind::Scale, n));
        }
        if let Some(n) = acc_head(s, "accTitle", ':') {
            return Ok(Step::PushToken(Mode::AccTitle, TokenKind::AccTitle, n));
        }
        if let Some(n) = acc_head(s, "accDescr", ':') {
            return Ok(Step::PushToken(Mode::AccDescr, TokenKind::AccDescr, n));
        }
        if let Some(n) = acc_head(s, "accDescr", '{') {
            return Ok(Step::Push(Mode::AccDescrMultiline, n));
        }
        if let Some(n) = keyword_ws(s, "classDef") {
            return Ok(Step::PushToken(Mode::ClassDef, TokenKind::ClassDef, n));
        }
        if let Some(n) = keyword_ws(s, "class") {
            return Ok(Step::PushToken(Mode::Class, TokenKind::Class, n));
        }
        if let Some(n) = keyword_ws(s, "style") {
            return Ok(Step::PushToken(Mode::Style, TokenKind::Style, n));
        }
        if let Some(n) = keyword_ws(s, "state") {
            return Ok(Step::Push(Mode::State, n));
        }
        if let Some(n) = keyword_ws(s, "note") {
            return Ok(Step::PushToken(Mode::Note, TokenKind::Note, n));
        }
        if let Some(n) = keyword_ws(s, "stateDiagram-v2") {
            return Ok(Step::Token(TokenKind::Sd, n));
        }
        if let Some(n) = keyword_ws(s, "stateDiagram") {
            return Ok(Step::Token(TokenKind::Sd, n));
        }
        if let Some(n) = literal(s, "hide empty description") {
            return Ok(Step::Token(TokenKind::HideEmpty, n));
        }
        if let Some(n) = literal(s, "[*]") {
            return Ok(Step::Token(TokenKind::EdgeState, n));
        }
        if let Some(n) = one_plus(s, id_char) {
            return Ok(id_token(s, n));
        }
        if let Some(n) = descr(s) {
            return Ok(Step::Token(TokenKind::Descr, n));
        }
        if let Some(n) = literal(s, "-->") {
            return Ok(Step::Token(TokenKind::Arrow, n));
        }
        if let Some(n) = literal(s, ":::") {
            return Ok(Step::Token(TokenKind::StyleSeparator, n));
        }
        let ch = s.chars().next().expect("non-empty source");
        Ok(Step::Token(TokenKind::Invalid, ch.len_utf8()))
    }

    fn step_struct(&self, s: &str) -> Result<Step, ParseError> {
        if let Some(n) = one_plus(s, |c| c.is_whitespace() && c != '\n') {
            return Ok(Step::Skip(n));
        }
        if s.starts_with('#') {
            return Ok(Step::Skip(line_end(s)));
        }
        if let Some(n) = percent_comment(s) {
            return Ok(Step::Skip(n));
        }
        if let Some(n) = keyword_ws(s, "classDef") {
            return Ok(Step::PushToken(Mode::ClassDef, TokenKind::ClassDef, n));
        }
        if let Some(n) = keyword_ws(s, "class") {
            return Ok(Step::PushToken(Mode::Class, TokenKind::Class, n));
        }
        if let Some(n) = keyword_ws(s, "style") {
            return Ok(Step::PushToken(Mode::Style, TokenKind::Style, n));
        }
        if let Some(n) = keyword_ws(s, "state") {
            return Ok(Step::Push(Mode::State, n));
        }
        for (dir, kind) in [
            ("TB", TokenKind::DirectionTb),
            ("BT", TokenKind::DirectionBt),
            ("RL", TokenKind::DirectionRl),
            ("LR", TokenKind::DirectionLr),
        ] {
            if let Some(n) = direction_end(s, dir) {
                return Ok(Step::Token(kind, n));
            }
        }
        if s.starts_with('}') {
            return Ok(Step::PopToken(TokenKind::StructStop, 1));
        }
        if s.starts_with('\n') {
            return Ok(Step::Skip(1));
        }
        if let Some(n) = keyword_ws(s, "note") {
            return Ok(Step::PushToken(Mode::Note, TokenKind::Note, n));
        }
        if let Some(n) = literal(s, "[*]") {
            return Ok(Step::Token(TokenKind::EdgeState, n));
        }
        if let Some(n) = one_plus(s, id_char) {
            return Ok(id_token(s, n));
        }
        if let Some(n) = descr(s) {
            return Ok(Step::Token(TokenKind::Descr, n));
        }
        if let Some(n) = literal(s, "-->") {
            return Ok(Step::Token(TokenKind::Arrow, n));
        }
        if let Some(n) = literal(s, "--") {
            return Ok(Step::Token(TokenKind::Concurrent, n));
        }
        if let Some(n) = literal(s, ":::") {
            return Ok(Step::Token(TokenKind::StyleSeparator, n));
        }
        Err(self.lexical_error())
    }

    fn step_state(&self, s: &str) -> Result<Step, ParseError> {
        if let Some(n) = one_plus(s, |c| c.is_whitespace() && c != '\n') {
            return Ok(Step::Skip(n));
        }
        if s.starts_with('#') {
            return Ok(Step::Skip(line_end(s)));
        }
        if let Some(n) = percent_comment(s) {
            return Ok(Step::Skip(n));
        }
        for (marker, kind) in [
            ("<<fork>>", TokenKind::Fork),
            ("<<join>>", TokenKind::Join),
            ("<<choice>>", TokenKind::Choice),
            ("[[fork]]", TokenKind::Fork),
            ("[[join]]", TokenKind::Join),
            ("[[choice]]", TokenKind::Choice),
        ] {
            if let Some((end, text)) = state_shape_marker(s, marker) {
                return Ok(Step::PopText(kind, end, text));
            }
        }
        if s.starts_with('"') {
            return Ok(Step::Push(Mode::StateString, 1));
        }
        if let Some(n) = state_as(s) {
            return Ok(Step::PushToken(Mode::StateId, TokenKind::As, n));
        }
        if let Some((end, text)) = state_name_error(s) {
            return Err(ParseError::at(
                format!("Error: State name must be a single word. Found: \"{text}\""),
                self.span_to(end),
            ));
        }
        if let Some(n) = one_plus(s, |c| c != '\n' && !c.is_whitespace() && c != '{') {
            return Ok(Step::Token(TokenKind::CompositState, n));
        }
        if s.starts_with('\n') {
            return Ok(Step::Pop(1));
        }
        if s.starts_with('{') {
            return Ok(Step::PopPushToken(Mode::Struct, TokenKind::StructStart, 1));
        }
        Err(self.lexical_error())
    }

    fn step_state_string(&self, s: &str) -> Result<Step, ParseError> {
        if s.starts_with('"') {
            return Ok(Step::Pop(1));
        }
        Ok(Step::Token(
            TokenKind::StateDescr,
            run_while(s, |c| c != '"'),
        ))
    }

    fn step_state_id(&self, s: &str) -> Result<Step, ParseError> {
        let n = run_while(s, |c| c != '\n' && c != '{');
        match process_id(&s[..n]) {
            None => Ok(Step::Skip(n)),
            Some((text, consumed)) => Ok(Step::PopText(TokenKind::Id, consumed, text)),
        }
    }

    fn step_scale(&self, s: &str) -> Result<Step, ParseError> {
        if let Some(n) = one_plus(s, |c| c.is_ascii_digit()) {
            return Ok(Step::Token(TokenKind::Width, n));
        }
        let mut i = skip_ws(s, 0);
        if i > 0
            && let Some(n) = literal(&s[i..], "width")
        {
            i += n;
            return Ok(Step::Pop(i));
        }
        Err(self.lexical_error())
    }

    fn step_acc_descr_multiline(&self, s: &str) -> Result<Step, ParseError> {
        if s.starts_with('}') {
            return Ok(Step::Pop(1));
        }
        Ok(Step::Token(
            TokenKind::AccDescrMultilineValue,
            run_while(s, |c| c != '}'),
        ))
    }

    fn step_class_def(&self, s: &str) -> Result<Step, ParseError> {
        if let Some(n) = keyword_ws(s, "default") {
            return Ok(Step::ReplaceToken(
                Mode::ClassDefId,
                TokenKind::DefaultClassDefId,
                n,
            ));
        }
        if let Some(n) = word_ws(s) {
            return Ok(Step::ReplaceToken(
                Mode::ClassDefId,
                TokenKind::ClassDefId,
                n,
            ));
        }
        Err(self.lexical_error())
    }

    fn step_class(&self, s: &str) -> Result<Step, ParseError> {
        if let Some(n) = class_entity_ids(s) {
            return Ok(Step::ReplaceToken(
                Mode::ClassStyle,
                TokenKind::ClassEntityIds,
                n,
            ));
        }
        Err(self.lexical_error())
    }

    fn step_style(&self, s: &str) -> Result<Step, ParseError> {
        let run = run_while(s, |c| word_char(c) || c == ',');
        if run > 0 {
            let ws = skip_ws(s, run);
            if ws > run {
                return Ok(Step::ReplaceToken(
                    Mode::StyleDefStyles,
                    TokenKind::StyleIds,
                    ws,
                ));
            }
        }
        Err(self.lexical_error())
    }

    fn step_note(&self, s: &str) -> Result<Step, ParseError> {
        if let Some(n) = literal(s, "left of") {
            return Ok(Step::ReplaceToken(Mode::NoteId, TokenKind::LeftOf, n));
        }
        if let Some(n) = literal(s, "right of") {
            return Ok(Step::ReplaceToken(Mode::NoteId, TokenKind::RightOf, n));
        }
        if s.starts_with('"') {
            return Ok(Step::Replace(Mode::FloatingNote, 1));
        }
        Err(self.lexical_error())
    }

    fn step_note_id(&self, s: &str) -> Result<Step, ParseError> {
        let lead = skip_ws(s, 0);
        let Some(run) = one_plus(&s[lead..], |c| {
            c != ':' && c != '\n' && !c.is_whitespace() && c != '-'
        }) else {
            return Err(self.lexical_error());
        };
        let end = lead + run;
        match process_id(&s[..end]) {
            None => Ok(Step::Skip(end)),
            Some((text, consumed)) => Ok(Step::PopPushText(
                Mode::NoteText,
                TokenKind::Id,
                consumed,
                text,
            )),
        }
    }

    fn step_note_text(&self, s: &str) -> Result<Step, ParseError> {
        if let Some((end, text)) = note_text_colon(s) {
            return Ok(Step::PopText(TokenKind::NoteText, end, text));
        }
        if let Some((end, text)) = end_note(s) {
            return Ok(Step::PopText(TokenKind::NoteText, end, text));
        }
        Err(self.lexical_error())
    }

    fn step_floating_note(&self, s: &str) -> Result<Step, ParseError> {
        if let Some(n) = floating_as(s) {
            return Ok(Step::ReplaceToken(Mode::FloatingNoteId, TokenKind::As, n));
        }
        if s.starts_with('"') {
            return Ok(Step::Skip(1));
        }
        Ok(Step::Token(TokenKind::NoteText, run_while(s, |c| c != '"')))
    }

    fn step_floating_note_id(&self, s: &str) -> Result<Step, ParseError> {
        let n = line_end(s);
        match process_id(&s[..n]) {
            None => Ok(Step::Skip(n)),
            Some((text, consumed)) => Ok(Step::PopText(TokenKind::Id, consumed, text)),
        }
    }
}

/// Length of the input up to the next newline (excluded).
fn line_end(s: &str) -> usize {
    s.find('\n').unwrap_or(s.len())
}

fn skip_ws(s: &str, mut i: usize) -> usize {
    while let Some(ch) = s[i..].chars().next() {
        if ch.is_whitespace() {
            i += ch.len_utf8();
        } else {
            break;
        }
    }
    i
}

fn run_while(s: &str, pred: impl Fn(char) -> bool) -> usize {
    let mut i = 0;
    for ch in s.chars() {
        if pred(ch) {
            i += ch.len_utf8();
        } else {
            break;
        }
    }
    i
}

fn one_plus(s: &str, pred: impl Fn(char) -> bool) -> Option<usize> {
    let n = run_while(s, pred);
    (n > 0).then_some(n)
}

fn literal(s: &str, lit: &str) -> Option<usize> {
    if s.len() >= lit.len()
        && s.is_char_boundary(lit.len())
        && s[..lit.len()].eq_ignore_ascii_case(lit)
    {
        Some(lit.len())
    } else {
        None
    }
}

fn keyword_ws(s: &str, lit: &str) -> Option<usize> {
    let n = literal(s, lit)?;
    let i = skip_ws(s, n);
    (i > n).then_some(i)
}

/// `\w+\s+` of the `classDef` id.
fn word_ws(s: &str) -> Option<usize> {
    let run = run_while(s, word_char);
    if run == 0 {
        return None;
    }
    let ws = skip_ws(s, run);
    (ws > run).then_some(ws)
}

/// The `INITIAL`/`struct` id rule with upstream `processId`.
fn id_token(s: &str, n: usize) -> Step {
    match process_id(&s[..n]) {
        None => Step::Skip(n),
        Some((text, consumed)) => Step::Text(TokenKind::Id, consumed, text),
    }
}

fn word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// `[^:\n\s\-\{]+`, the `INITIAL`/`struct` id rule.
fn id_char(c: char) -> bool {
    c != ':' && c != '\n' && !c.is_whitespace() && c != '-' && c != '{'
}

fn quoted(s: &str) -> Option<usize> {
    if !s.starts_with('"') {
        return None;
    }
    let close = s[1..].find('"')?;
    Some(1 + close + 1)
}

fn percent_comment(s: &str) -> Option<usize> {
    if s.starts_with("%%") && !s.starts_with("%%{") {
        Some(line_end(s))
    } else {
        None
    }
}

/// `.*direction\s+TB[^\n]*` and its siblings: the rule consumes to the end of the line that
/// holds the direction, because `[^\n]*` is greedy. `\s+` may cross a newline.
fn direction_end(s: &str, dir: &str) -> Option<usize> {
    let line = &s[..line_end(s)];
    let lower = line.to_ascii_lowercase();
    let mut from = 0;
    let mut found = None;
    while let Some(rel) = lower[from..].find("direction") {
        let start = from + rel;
        let after = start + "direction".len();
        let mut j = after;
        while let Some(ch) = s[j..].chars().next() {
            if ch.is_whitespace() {
                j += ch.len_utf8();
            } else {
                break;
            }
        }
        if j > after
            && s.len() >= j + dir.len()
            && s.get(j..j + dir.len())
                .is_some_and(|tail| tail.eq_ignore_ascii_case(dir))
        {
            let dir_end = j + dir.len();
            found = Some(dir_end + line_end(&s[dir_end..]));
        }
        from = start + 1;
    }
    found
}

/// `accTitle\s*":"\s*` or `accDescr\s*"{"\s*`.
fn acc_head(s: &str, keyword: &str, marker: char) -> Option<usize> {
    let n = literal(s, keyword)?;
    let mut i = skip_ws(s, n);
    if s[i..].starts_with(marker) {
        i += marker.len_utf8();
        i = skip_ws(s, i);
        Some(i)
    } else {
        None
    }
}

/// `\s*":"(?:[^:\n;]|":"[^:\n;])+`.
fn descr(s: &str) -> Option<usize> {
    let mut i = skip_ws(s, 0);
    if !s[i..].starts_with(':') {
        return None;
    }
    i += 1;
    let start = i;
    loop {
        let rest = &s[i..];
        let mut chars = rest.chars();
        match chars.next() {
            Some(c) if c != ':' && c != '\n' && c != ';' => i += c.len_utf8(),
            Some(':') => match rest[1..].chars().next() {
                Some(c) if c != ':' && c != '\n' && c != ';' => i += 1 + c.len_utf8(),
                _ => break,
            },
            _ => break,
        }
    }
    (i > start).then_some(i)
}

/// `<STATE>.*"<<fork>>"` and its siblings, returning the text before the marker, trimmed.
fn state_shape_marker(s: &str, marker: &str) -> Option<(usize, String)> {
    let line = &s[..line_end(s)];
    let lower = line.to_ascii_lowercase();
    let marker_lower = marker.to_ascii_lowercase();
    let idx = lower.rfind(&marker_lower)?;
    let end = idx + marker.len();
    Some((end, s[..idx].trim().to_string()))
}

/// `<STATE>\w+\s+\w+.*?\{`, the two-word state name error.
fn state_name_error(s: &str) -> Option<(usize, String)> {
    let first = run_while(s, word_char);
    if first == 0 {
        return None;
    }
    let after_ws = skip_ws(s, first);
    if after_ws == first {
        return None;
    }
    let second = run_while(&s[after_ws..], word_char);
    if second == 0 {
        return None;
    }
    let tail = after_ws + second;
    let end_of_line = tail + line_end(&s[tail..]);
    let brace = s[tail..end_of_line].find('{')?;
    let end = tail + brace + 1;
    Some((end, s[..end].trim().to_string()))
}

/// `(\w+)+((","\s*\w+)*)`, the `class` id list.
fn class_entity_ids(s: &str) -> Option<usize> {
    let mut i = run_while(s, word_char);
    if i == 0 {
        return None;
    }
    loop {
        if !s[i..].starts_with(',') {
            break;
        }
        let mut j = i + 1;
        j = skip_ws(s, j);
        let word = run_while(&s[j..], word_char);
        if word == 0 {
            break;
        }
        i = j + word;
    }
    Some(i)
}

/// `\s*"as"\s*` of the floating-note id.
fn floating_as(s: &str) -> Option<usize> {
    let mut i = skip_ws(s, 0);
    i += literal(&s[i..], "as")?;
    Some(skip_ws(s, i))
}

/// `\s*"as"\s+` of the state id.
fn state_as(s: &str) -> Option<usize> {
    let i = skip_ws(s, 0);
    let i = i + literal(&s[i..], "as")?;
    let j = skip_ws(s, i);
    (j > i).then_some(j)
}

/// `\s*":"[^:\n;]+`: the note text, cut the way upstream's `substr(2).trim()` cuts it.
fn note_text_colon(s: &str) -> Option<(usize, String)> {
    let mut i = skip_ws(s, 0);
    if !s[i..].starts_with(':') {
        return None;
    }
    i += 1;
    let run = one_plus(&s[i..], |c| c != ':' && c != '\n' && c != ';')?;
    i += run;
    let text: String = s[..i]
        .chars()
        .skip(2)
        .collect::<String>()
        .trim()
        .to_string();
    Some((i, text))
}

/// `[\s\S]*?\n\s*"end note"`, cut the way upstream's `slice(0, -8).trim()` cuts it.
fn end_note(s: &str) -> Option<(usize, String)> {
    let lower = s.to_ascii_lowercase();
    let marker = "end note";
    let mut search = 0;
    while let Some(rel) = lower[search..].find(marker) {
        let idx = search + rel;
        if let Some(newline) = s[..idx].rfind('\n')
            && s[newline + 1..idx].chars().all(char::is_whitespace)
        {
            let end = idx + marker.len();
            return Some((end, s[..idx].trim().to_string()));
        }
        search = idx + 1;
    }
    None
}

/// Upstream `processId`: split an id from an inline `%%` comment, re-lexing the comment.
/// `None` means the whole match was a comment and no token is emitted; the first value is
/// the token text and the second the number of bytes to consume.
fn process_id(text: &str) -> Option<(String, usize)> {
    match text.find("%%") {
        None => Some((text.to_string(), text.len())),
        Some(0) => None,
        Some(idx) => Some((text[..idx].to_string(), idx)),
    }
}
