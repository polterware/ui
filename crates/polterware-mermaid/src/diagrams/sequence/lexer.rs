//! The lexer of the sequence grammar, copied rule by rule from upstream
//! `sequenceDiagram.jison`: the start conditions (`%x ID ALIAS LINE CONFIG acc_title
//! acc_descr acc_descr_multiline`) and the token regexes, matched in declaration order
//! because jison breaks on the first rule that matches.
//!
//! Two jison details are reproduced on purpose: with `%options case-insensitive` every
//! literal matches regardless of case, and in non-flex mode a rule whose pattern ends in a
//! word character gets a trailing `\b` (so `par` does not match the start of `par_over`).

use crate::error::{ParseError, Span};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Initial,
    Id,
    Alias,
    Line,
    Config,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
}

/// The arrow tokens of the grammar, in `signaltype` order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Arrow {
    Solid,
    BidirectionalSolid,
    Dotted,
    BidirectionalDotted,
    SolidOpen,
    DottedOpen,
    SolidCross,
    DottedCross,
    SolidPoint,
    DottedPoint,
    SolidTopDotted,
    SolidBottomDotted,
    StickTopDotted,
    StickBottomDotted,
    SolidTopReverseDotted,
    SolidBottomReverseDotted,
    StickTopReverseDotted,
    StickBottomReverseDotted,
    SolidTop,
    SolidBottom,
    StickTop,
    StickBottom,
    SolidTopReverse,
    SolidBottomReverse,
    StickTopReverse,
    StickBottomReverse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    Eof,
    Newline,
    Num,
    ConfigStart,
    ConfigContent,
    ConfigEnd,
    Actor,
    Invalid,
    Box,
    Participant,
    ParticipantActor,
    Create,
    Destroy,
    As,
    Loop,
    Rect,
    Opt,
    Alt,
    Else,
    Par,
    ParOver,
    And,
    Critical,
    Option,
    Break,
    RestOfLine,
    End,
    LeftOf,
    RightOf,
    Links,
    Link,
    Properties,
    Details,
    Over,
    Note,
    Activate,
    Deactivate,
    Title,
    LegacyTitle,
    AccTitle,
    AccTitleValue,
    AccDescr,
    AccDescrValue,
    AccDescrMultilineValue,
    Sd,
    Autonumber,
    Off,
    Comma,
    Arrow(Arrow),
    Txt,
    Plus,
    Minus,
    Parens,
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
    eof_token_emitted: bool,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Self {
            src,
            pos: 0,
            line: 1,
            column: 1,
            modes: vec![Mode::Initial],
            eof_token_emitted: false,
        }
    }

    pub fn next_token(&mut self) -> Result<Token, ParseError> {
        loop {
            if self.pos >= self.src.len() {
                return Ok(self.eof_token());
            }
            match self.mode() {
                Mode::Initial => {
                    if let Some(token) = self.initial()? {
                        return Ok(token);
                    }
                }
                Mode::Id => {
                    if let Some(token) = self.id()? {
                        return Ok(token);
                    }
                }
                Mode::Alias => {
                    if let Some(token) = self.alias() {
                        return Ok(token);
                    }
                }
                Mode::Line => return Ok(self.line()),
                Mode::Config => {
                    if let Some(token) = self.config() {
                        return Ok(token);
                    }
                }
                Mode::AccTitle | Mode::AccDescr => return Ok(self.acc_value()),
                Mode::AccDescrMultiline => {
                    if let Some(token) = self.acc_descr_multiline() {
                        return Ok(token);
                    }
                }
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

    fn make_text(&self, kind: TokenKind, text: impl Into<String>, mark: Mark) -> Token {
        Token {
            kind,
            text: text.into(),
            span: self.span_from(mark),
        }
    }

    fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
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

    fn eof_token(&mut self) -> Token {
        let mark = self.mark();
        if self.eof_token_emitted {
            return self.make(TokenKind::Eof, mark);
        }
        match self.mode() {
            Mode::Initial => {
                self.eof_token_emitted = true;
                self.make(TokenKind::Newline, mark)
            }
            Mode::Alias => {
                self.eof_token_emitted = true;
                self.pop();
                self.pop();
                self.make(TokenKind::Newline, mark)
            }
            Mode::Line => {
                self.eof_token_emitted = true;
                self.pop();
                self.make_text(TokenKind::RestOfLine, "", mark)
            }
            Mode::AccTitle => {
                self.eof_token_emitted = true;
                self.pop();
                self.make_text(TokenKind::AccTitleValue, "", mark)
            }
            Mode::AccDescr => {
                self.eof_token_emitted = true;
                self.pop();
                self.make_text(TokenKind::AccDescrValue, "", mark)
            }
            Mode::AccDescrMultiline => {
                self.eof_token_emitted = true;
                self.make_text(TokenKind::AccDescrMultilineValue, "", mark)
            }
            Mode::Id | Mode::Config => {
                self.eof_token_emitted = true;
                self.make(TokenKind::Eof, mark)
            }
        }
    }

    fn lexical_error(&self, start: Mark) -> ParseError {
        ParseError::at(
            format!("Lexical error on line {}.\nUnrecognized text.", start.line),
            self.span_from(start),
        )
    }

    // -- INITIAL ------------------------------------------------------------------------

    fn initial(&mut self) -> Result<Option<Token>, ParseError> {
        let rest = self.rest();
        // [\n]+
        if rest.starts_with('\n') {
            let mark = self.mark();
            let len = run_len(rest, |ch| ch == '\n');
            self.bump(len);
            return Ok(Some(self.make(TokenKind::Newline, mark)));
        }
        // \s+
        if let Some(len) = leading_run(rest, is_space) {
            self.bump(len);
            return Ok(None);
        }
        // \#[^\n]*
        if rest.starts_with('#') {
            self.skip_line();
            return Ok(None);
        }
        // \%%(?!\{)[^\n]*
        if rest.starts_with("%%") && !rest.starts_with("%%{") {
            self.skip_line();
            return Ok(None);
        }
        // [^\}]\%\%[^\n]*
        if let Some(len) = pre_percent_len(rest) {
            self.bump(len);
            return Ok(None);
        }
        // NUM, with its `(?=[ \n]+)` lookahead
        if let Some(len) = num_len(rest) {
            let mark = self.mark();
            self.bump(len);
            return Ok(Some(self.make(TokenKind::Num, mark)));
        }
        Ok(self.initial_keyword())
    }

    fn initial_keyword(&mut self) -> Option<Token> {
        if self.at_keyword("box") {
            return Some(self.keyword(TokenKind::Box, "box", Some(Mode::Line)));
        }
        if self.at_keyword("participant") {
            return Some(self.keyword(TokenKind::Participant, "participant", Some(Mode::Id)));
        }
        if self.at_keyword("actor") {
            return Some(self.keyword(TokenKind::ParticipantActor, "actor", Some(Mode::Id)));
        }
        if self.at_keyword("create") {
            return Some(self.keyword(TokenKind::Create, "create", None));
        }
        if self.at_keyword("destroy") {
            return Some(self.keyword(TokenKind::Destroy, "destroy", Some(Mode::Id)));
        }
        if self.at_keyword("loop") {
            return Some(self.keyword(TokenKind::Loop, "loop", Some(Mode::Line)));
        }
        if self.at_keyword("rect") {
            return Some(self.keyword(TokenKind::Rect, "rect", Some(Mode::Line)));
        }
        if self.at_keyword("opt") {
            return Some(self.keyword(TokenKind::Opt, "opt", Some(Mode::Line)));
        }
        if self.at_keyword("alt") {
            return Some(self.keyword(TokenKind::Alt, "alt", Some(Mode::Line)));
        }
        if self.at_keyword("else") {
            return Some(self.keyword(TokenKind::Else, "else", Some(Mode::Line)));
        }
        // "par" comes before "par_over" upstream; the trailing \b of non-flex jison keeps
        // them apart.
        if self.at_keyword("par") {
            return Some(self.keyword(TokenKind::Par, "par", Some(Mode::Line)));
        }
        if self.at_keyword("par_over") {
            return Some(self.keyword(TokenKind::ParOver, "par_over", Some(Mode::Line)));
        }
        if self.at_keyword("and") {
            return Some(self.keyword(TokenKind::And, "and", Some(Mode::Line)));
        }
        if self.at_keyword("critical") {
            return Some(self.keyword(TokenKind::Critical, "critical", Some(Mode::Line)));
        }
        if self.at_keyword("option") {
            return Some(self.keyword(TokenKind::Option, "option", Some(Mode::Line)));
        }
        if self.at_keyword("break") {
            return Some(self.keyword(TokenKind::Break, "break", Some(Mode::Line)));
        }
        if self.at_keyword("end") {
            return Some(self.keyword(TokenKind::End, "end", None));
        }
        if self.at_keyword("left of") {
            return Some(self.keyword(TokenKind::LeftOf, "left of", None));
        }
        if self.at_keyword("right of") {
            return Some(self.keyword(TokenKind::RightOf, "right of", None));
        }
        for (word, kind) in [
            ("links", TokenKind::Links),
            ("link", TokenKind::Link),
            ("properties", TokenKind::Properties),
            ("details", TokenKind::Details),
        ] {
            if let Some(len) = self.guarded_keyword_len(word) {
                let after = &self.rest()[len..];
                if let Some(space) = leading_run(after, |ch| ch == ' ' || ch == '\t')
                    && let Some(ch) = after[space..].chars().next()
                    && !matches!(
                        ch,
                        '/' | '\\' | '+' | '(' | ')' | '<' | '>' | ':' | '\n' | ',' | ';'
                    )
                    && !is_space(ch)
                    && ch != '-'
                {
                    let mark = self.mark();
                    self.bump(len);
                    return Some(self.make(kind, mark));
                }
            }
        }
        if self.at_keyword("over") {
            return Some(self.keyword(TokenKind::Over, "over", None));
        }
        if self.at_keyword("note") {
            return Some(self.keyword(TokenKind::Note, "note", None));
        }
        if self.at_keyword("activate") {
            return Some(self.keyword(TokenKind::Activate, "activate", Some(Mode::Id)));
        }
        if self.at_keyword("deactivate") {
            return Some(self.keyword(TokenKind::Deactivate, "deactivate", Some(Mode::Id)));
        }
        // "title"\s[^#\n;]+
        if let Some(len) = self.title_len("title") {
            let mark = self.mark();
            self.bump(len);
            return Some(self.make(TokenKind::Title, mark));
        }
        // "title:"\s[^#\n;]+
        if let Some(len) = self.title_len("title:") {
            let mark = self.mark();
            self.bump(len);
            return Some(self.make(TokenKind::LegacyTitle, mark));
        }
        if let Some(len) = self.acc_len("accTitle") {
            let mark = self.mark();
            self.bump(len);
            self.push(Mode::AccTitle);
            return Some(self.make(TokenKind::AccTitle, mark));
        }
        if let Some(len) = self.acc_len("accDescr") {
            let mark = self.mark();
            self.bump(len);
            self.push(Mode::AccDescr);
            return Some(self.make(TokenKind::AccDescr, mark));
        }
        if let Some(len) = self.acc_brace_len("accDescr") {
            self.bump(len);
            self.push(Mode::AccDescrMultiline);
            return None;
        }
        if self.at_keyword("sequenceDiagram") {
            return Some(self.keyword(TokenKind::Sd, "sequenceDiagram", None));
        }
        if self.at_keyword("autonumber") {
            return Some(self.keyword(TokenKind::Autonumber, "autonumber", None));
        }
        if self.at_keyword("off") {
            return Some(self.keyword(TokenKind::Off, "off", None));
        }
        if self.rest().starts_with(',') {
            let mark = self.mark();
            self.bump(1);
            return Some(self.make(TokenKind::Comma, mark));
        }
        if self.rest().starts_with(';') {
            let mark = self.mark();
            self.bump(1);
            return Some(self.make(TokenKind::Newline, mark));
        }
        if let Some(len) = actor_gulp_len(self.rest()) {
            let mark = self.mark();
            self.bump(len);
            let text = js_trim(&self.src[mark.pos..self.pos]).to_string();
            return Some(self.make_text(TokenKind::Actor, text, mark));
        }
        for (literal, arrow) in ARROWS {
            if self.starts_with_ci(literal) {
                let mark = self.mark();
                self.bump(literal.len());
                return Some(self.make(TokenKind::Arrow(*arrow), mark));
            }
        }
        if self.rest().starts_with(':') {
            let mark = self.mark();
            let mut len = 1;
            let after = &self.rest()[1..];
            if let Some(prefix) = wrap_prefix_len(after) {
                len += prefix;
            }
            let tail = &self.rest()[len..];
            len += run_len(tail, |ch| !matches!(ch, '#' | '\n' | ';'));
            self.bump(len);
            return Some(self.make(TokenKind::Txt, mark));
        }
        if self.rest().starts_with('+') {
            let mark = self.mark();
            self.bump(1);
            return Some(self.make(TokenKind::Plus, mark));
        }
        if self.rest().starts_with('-') {
            let mark = self.mark();
            self.bump(1);
            return Some(self.make(TokenKind::Minus, mark));
        }
        if self.rest().starts_with("()") {
            let mark = self.mark();
            self.bump(2);
            return Some(self.make(TokenKind::Parens, mark));
        }
        // `.` catch-all
        let mark = self.mark();
        let len = self.peek().map_or(1, char::len_utf8);
        self.bump(len);
        Some(self.make(TokenKind::Invalid, mark))
    }

    // -- ID -----------------------------------------------------------------------------

    fn id(&mut self) -> Result<Option<Token>, ParseError> {
        let rest = self.rest();
        // <ID,ALIAS,LINE>((?!\n)\s)+
        if let Some(len) = leading_run(rest, |ch| is_space(ch) && ch != '\n') {
            self.bump(len);
            return Ok(None);
        }
        // <INITIAL,ID,ALIAS,LINE>\#[^\n]*
        if rest.starts_with('#') {
            self.skip_line();
            return Ok(None);
        }
        // <ID>\@\{
        if rest.starts_with("@{") {
            let mark = self.mark();
            self.bump(2);
            self.push(Mode::Config);
            return Ok(Some(self.make(TokenKind::ConfigStart, mark)));
        }
        let id_chars =
            |ch: char| !matches!(ch, '<' | '>' | ':' | '\n' | ',' | ';' | '@') && !is_space(ch);
        let run = leading_run(rest, id_chars).unwrap_or(0);
        if run > 0 {
            let after = &rest[run..];
            let spaces = leading_run(after, is_space).unwrap_or(0);
            let follows_at = after[spaces..].starts_with("@{");
            let follows_as = spaces > 0
                && starts_with_as(&after[spaces..])
                && follows_as_whitespace(&after[spaces..]);
            if follows_at || follows_as {
                let mark = self.mark();
                self.bump(run);
                let text = js_trim(&self.src[mark.pos..self.pos]).to_string();
                if follows_as && !follows_at {
                    self.push(Mode::Alias);
                }
                return Ok(Some(self.make_text(TokenKind::Actor, text, mark)));
            }
            let wide = leading_run(rest, |ch| {
                !matches!(ch, '<' | '>' | ':' | '\n' | ',' | ';' | '@')
            })
            .unwrap_or(0);
            let after = &rest[wide..];
            // `(?=\s*[\n;#]|$)`: `\s*` may stop before a newline, so check the first
            // non-newline whitespace gap and then the terminator.
            let spaces = leading_run(after, |ch| is_space(ch) && ch != '\n').unwrap_or(0);
            let tail = &after[spaces..];
            let terminated = tail.is_empty()
                || tail.starts_with('\n')
                || tail.starts_with('#')
                || tail.starts_with(';');
            if terminated {
                let mark = self.mark();
                self.bump(wide);
                let text = js_trim(&self.src[mark.pos..self.pos]).to_string();
                self.pop();
                return Ok(Some(self.make_text(TokenKind::Actor, text, mark)));
            }
        }
        // <ID>[^<>:\n,;@]*\<[^\n]*
        let prefix = leading_run(rest, |ch| {
            !matches!(ch, '<' | '>' | ':' | '\n' | ',' | ';' | '@')
        })
        .unwrap_or(0);
        if rest[prefix..].starts_with('<') {
            let mark = self.mark();
            let len = run_len(rest, |ch| ch != '\n');
            self.bump(len);
            let text = js_trim(&self.src[mark.pos..self.pos]).to_string();
            self.pop();
            return Ok(Some(self.make_text(TokenKind::Invalid, text, mark)));
        }
        // <ID>[^\n]+
        if let Some(len) = leading_run(rest, |ch| ch != '\n') {
            let mark = self.mark();
            self.bump(len);
            let text = js_trim(&self.src[mark.pos..self.pos]).to_string();
            self.pop();
            return Ok(Some(self.make_text(TokenKind::Invalid, text, mark)));
        }
        Err(self.lexical_error(self.mark()))
    }

    // -- ALIAS --------------------------------------------------------------------------

    fn alias(&mut self) -> Option<Token> {
        let rest = self.rest();
        if let Some(len) = leading_run(rest, |ch| is_space(ch) && ch != '\n') {
            self.bump(len);
            return None;
        }
        if rest.starts_with('#') {
            self.skip_line();
            return None;
        }
        if self.at_keyword("as") {
            let mark = self.mark();
            self.bump(2);
            self.pop();
            self.pop();
            self.push(Mode::Line);
            return Some(self.make(TokenKind::As, mark));
        }
        // <ALIAS>(?:)
        let mark = self.mark();
        self.pop();
        self.pop();
        Some(self.make_text(TokenKind::Newline, "", mark))
    }

    // -- LINE ---------------------------------------------------------------------------

    fn line(&mut self) -> Token {
        loop {
            let rest = self.rest();
            if let Some(len) = leading_run(rest, |ch| is_space(ch) && ch != '\n') {
                self.bump(len);
                continue;
            }
            if rest.starts_with('#') {
                self.skip_line();
                continue;
            }
            break;
        }
        let mark = self.mark();
        let mut len = wrap_prefix_len(self.rest()).unwrap_or(0);
        let tail = &self.rest()[len..];
        len += run_len(tail, |ch| !matches!(ch, '#' | '\n' | ';'));
        self.bump(len);
        self.pop();
        self.make(TokenKind::RestOfLine, mark)
    }

    // -- CONFIG -------------------------------------------------------------------------

    fn config(&mut self) -> Option<Token> {
        let rest = self.rest();
        if let Some(after) = rest.strip_prefix('}') {
            let spaces = leading_run(after, is_space).unwrap_or(0);
            let followed_by_as = spaces > 0
                && starts_with_as(&after[spaces..])
                && follows_as_whitespace(&after[spaces..]);
            let mark = self.mark();
            self.bump(1);
            if followed_by_as {
                self.pop();
                self.push(Mode::Alias);
            } else {
                self.pop();
                self.pop();
            }
            return Some(self.make(TokenKind::ConfigEnd, mark));
        }
        let len = run_len(rest, |ch| ch != '}');
        Some({
            let mark = self.mark();
            self.bump(len);
            self.make(TokenKind::ConfigContent, mark)
        })
    }

    // -- acc_title / acc_descr / acc_descr_multiline ------------------------------------

    fn acc_value(&mut self) -> Token {
        let mark = self.mark();
        let kind = if self.mode() == Mode::AccTitle {
            TokenKind::AccTitleValue
        } else {
            TokenKind::AccDescrValue
        };
        let len = run_len(self.rest(), |ch| ch != '\n');
        self.bump(len);
        self.pop();
        self.make(kind, mark)
    }

    fn acc_descr_multiline(&mut self) -> Option<Token> {
        if self.rest().starts_with('}') {
            self.bump(1);
            self.pop();
            return None;
        }
        let mark = self.mark();
        let len = run_len(self.rest(), |ch| ch != '}');
        self.bump(len);
        Some(self.make(TokenKind::AccDescrMultilineValue, mark))
    }

    // -- helpers ------------------------------------------------------------------------

    fn skip_line(&mut self) {
        let len = run_len(self.rest(), |ch| ch != '\n');
        self.bump(len);
    }

    fn starts_with_ci(&self, literal: &str) -> bool {
        let rest = self.rest();
        rest.get(..literal.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(literal))
    }

    fn guarded_keyword_len(&self, word: &str) -> Option<usize> {
        if !self.starts_with_ci(word) {
            return None;
        }
        let rest = self.rest();
        match rest[word.len()..].chars().next() {
            Some(ch) if is_word_char(ch) => None,
            _ => Some(word.len()),
        }
    }

    fn at_keyword(&self, word: &str) -> bool {
        self.guarded_keyword_len(word).is_some()
    }

    fn keyword(&mut self, kind: TokenKind, literal: &str, mode: Option<Mode>) -> Token {
        let mark = self.mark();
        self.bump(literal.len());
        if let Some(mode) = mode {
            self.push(mode);
        }
        self.make(kind, mark)
    }

    /// `"title"\s[^#\n;]+` and `"title:"\s[^#\n;]+`.
    fn title_len(&self, literal: &str) -> Option<usize> {
        if !self.starts_with_ci(literal) {
            return None;
        }
        let rest = self.rest();
        let after = &rest[literal.len()..];
        let ch = after.chars().next()?;
        if !is_space(ch) {
            return None;
        }
        let mut len = literal.len() + ch.len_utf8();
        let tail = &after[ch.len_utf8()..];
        let first = tail.chars().next()?;
        if matches!(first, '#' | '\n' | ';') {
            return None;
        }
        let mut consumed = first.len_utf8();
        for next in tail[first.len_utf8()..].chars() {
            if matches!(next, '#' | '\n' | ';') {
                break;
            }
            consumed += next.len_utf8();
        }
        len += consumed;
        Some(len)
    }

    /// `accTitle\s*":"\s*` and `accDescr\s*":"\s*`.
    fn acc_len(&self, word: &str) -> Option<usize> {
        if !self.starts_with_ci(word) {
            return None;
        }
        let rest = self.rest();
        let mut len = word.len();
        let tail = &rest[len..];
        len += leading_run(tail, is_space).unwrap_or(0);
        if !rest[len..].starts_with(':') {
            return None;
        }
        len += 1;
        let tail = &rest[len..];
        len += leading_run(tail, is_space).unwrap_or(0);
        Some(len)
    }

    /// `accDescr\s*"{"\s*`.
    fn acc_brace_len(&self, word: &str) -> Option<usize> {
        if !self.starts_with_ci(word) {
            return None;
        }
        let rest = self.rest();
        let mut len = word.len();
        len += leading_run(&rest[len..], is_space).unwrap_or(0);
        if !rest[len..].starts_with('{') {
            return None;
        }
        len += 1;
        len += leading_run(&rest[len..], is_space).unwrap_or(0);
        Some(len)
    }
}

/// The arrow literals in lexer order: jison returns the first rule that matches.
const ARROWS: &[(&str, Arrow)] = &[
    ("->>", Arrow::Solid),
    ("<<->>", Arrow::BidirectionalSolid),
    ("-->>", Arrow::Dotted),
    ("<<-->>", Arrow::BidirectionalDotted),
    ("->", Arrow::SolidOpen),
    ("-->", Arrow::DottedOpen),
    ("-x", Arrow::SolidCross),
    ("--x", Arrow::DottedCross),
    ("-)", Arrow::SolidPoint),
    ("--)", Arrow::DottedPoint),
    ("--|\\", Arrow::SolidTopDotted),
    ("--|/", Arrow::SolidBottomDotted),
    ("--\\\\", Arrow::StickTopDotted),
    ("--//", Arrow::StickBottomDotted),
    ("/|--", Arrow::SolidTopReverseDotted),
    ("\\|--", Arrow::SolidBottomReverseDotted),
    ("//--", Arrow::StickTopReverseDotted),
    ("\\\\--", Arrow::StickBottomReverseDotted),
    ("-|\\", Arrow::SolidTop),
    ("-|/", Arrow::SolidBottom),
    ("-\\\\", Arrow::StickTop),
    ("-//", Arrow::StickBottom),
    ("/|-", Arrow::SolidTopReverse),
    ("\\|-", Arrow::SolidBottomReverse),
    ("//-", Arrow::StickTopReverse),
    ("\\\\-", Arrow::StickBottomReverse),
];

/// The arrow prefixes the actor rule refuses to swallow (its negative lookahead).
const ACTOR_STOPPERS: &[&str] = &[
    "-x", "--x", "-)", "--)", "-|\\", "-\\", "-/", "-//", "-|/", "/|-", "\\|-", "//-", "\\\\-",
    "--|\\", "--", "()",
];

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

/// JavaScript's `\s` (ECMAScript WhiteSpace and LineTerminator).
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

pub(crate) fn js_trim(text: &str) -> &str {
    text.trim_matches(is_space)
}

fn is_word_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn starts_with_as(s: &str) -> bool {
    s.get(..2)
        .is_some_and(|head| head.eq_ignore_ascii_case("as"))
}

fn follows_as_whitespace(s: &str) -> bool {
    s.get(2..)
        .and_then(|tail| tail.chars().next())
        .is_some_and(is_space)
}

/// `([0-9]+(\.[0-9]{1,2})?|\.[0-9]{1,2})(?=[ \n]+)`.
fn num_len(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut index = 0;
    let mut digits = 0;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
        digits += 1;
    }
    if digits > 0 {
        if index + 1 < bytes.len() && bytes[index] == b'.' && bytes[index + 1].is_ascii_digit() {
            index += 1;
            let mut fraction = 0;
            while index < bytes.len() && bytes[index].is_ascii_digit() && fraction < 2 {
                index += 1;
                fraction += 1;
            }
        }
    } else if bytes.first() == Some(&b'.') && bytes.get(1).is_some_and(u8::is_ascii_digit) {
        index = 1;
        while index < bytes.len() && bytes[index].is_ascii_digit() && index < 3 {
            index += 1;
        }
    } else {
        return None;
    }
    match bytes.get(index) {
        Some(b' ') | Some(b'\n') => Some(index),
        _ => None,
    }
}

/// `[^\}]\%\%[^\n]*`.
fn pre_percent_len(s: &str) -> Option<usize> {
    let mut chars = s.char_indices();
    let (_, first) = chars.next()?;
    if first == '}' {
        return None;
    }
    let after_first = first.len_utf8();
    if !s[after_first..].starts_with("%%") {
        return None;
    }
    Some(after_first + run_len(&s[after_first..], |ch| ch != '\n'))
}

/// The optional `:?(?:no)?wrap:` prefix shared by `restOfLine` and `TXT`.
fn wrap_prefix_len(s: &str) -> Option<usize> {
    for prefix in [":wrap:", ":nowrap:", "wrap:", "nowrap:"] {
        if s.get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
        {
            return Some(prefix.len());
        }
    }
    None
}

/// The INITIAL `ACTOR` gulp: `[^\/\\\+()<>\-:\n,;]+` followed by repeated
/// `(?!arrow)[-]*[^+<>\-:\n,;]+` groups.
fn actor_gulp_len(s: &str) -> Option<usize> {
    let mut index = run_len(s, |ch| {
        !matches!(
            ch,
            '/' | '\\' | '+' | '(' | ')' | '<' | '>' | '-' | ':' | '\n' | ',' | ';'
        )
    });
    if index == 0 {
        return None;
    }
    loop {
        let rest = &s[index..];
        if rest.is_empty() {
            break;
        }
        if ACTOR_STOPPERS
            .iter()
            .any(|stopper| starts_with_ci_at(rest, stopper))
        {
            break;
        }
        let mut after = run_len(rest, |ch| ch == '-');
        let run = run_len(&rest[after..], |ch| {
            !matches!(ch, '+' | '<' | '>' | '-' | ':' | '\n' | ',' | ';')
        });
        if run == 0 {
            break;
        }
        after += run;
        index += after;
    }
    Some(index)
}

fn starts_with_ci_at(s: &str, literal: &str) -> bool {
    s.get(..literal.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(literal))
}
