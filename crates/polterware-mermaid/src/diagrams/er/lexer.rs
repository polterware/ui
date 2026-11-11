//! The ER lexer: a hand port of the `%lex` section of upstream `erDiagram.jison`.
//!
//! Reproduced from the jison grammar: the start conditions (`%x block block_bq acc_title
//! acc_descr acc_descr_multiline style`) with their push/pop stack, `%options
//! case-insensitive`, and the non-flex matcher, which walks the rules of the current
//! condition in declaration order and takes the first that matches at the current position
//! (not the longest).
//!
//! Two jison details are reproduced on purpose. In non-flex mode a rule whose pattern ends
//! in a word character gets a trailing `\b` appended by jison's lexer generator, so a
//! keyword such as `to` does not eat the start of an entity name such as `tofu`. And
//! `<<EOF>>` is only added to the inclusive conditions, so input that ends inside one of the
//! exclusive modes above yields the end of input without the grammar's `EOF` terminal, and
//! the parse fails — exactly as upstream fails on a `classDef` line without a trailing
//! newline.
//!
//! JavaScript's `.` stops at a line terminator; this crate receives `\n`-normalized text
//! from [`crate::preprocess`], so the port only treats `\n` as a line break.

use crate::error::{ParseError, Span};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Initial,
    Block,
    BlockBacktick,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
    Style,
}

/// The direction a `direction <dir>` statement declares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Direction {
    Tb,
    Bt,
    Rl,
    Lr,
}

impl Direction {
    pub(crate) fn value(self) -> &'static str {
        match self {
            Direction::Tb => "TB",
            Direction::Bt => "BT",
            Direction::Rl => "RL",
            Direction::Lr => "LR",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    AccTitle,
    AccTitleValue,
    AccDescr,
    AccDescrValue,
    AccDescrMultilineValue,
    Direction(Direction),
    Newline,
    EntityName,
    Word,
    ErDiagram,
    BlockStart,
    Brkt,
    Comma,
    StyleSeparator,
    Colon,
    AttributeKey,
    AttributeWord,
    Comment,
    BlockStop,
    Sqs,
    Sqe,
    Style,
    ClassDef,
    Class,
    Subgraph,
    End,
    ZeroOrOne,
    ZeroOrMore,
    OneOrMore,
    OnlyOne,
    MdParent,
    DecimalNum,
    EntityOne,
    Num,
    NonIdentifying,
    Identifying,
    StyleText,
    Semi,
    UnicodeText,
    /// A token the grammar writes as a literal character (jison's `return yytext[0]`).
    Char(char),
    /// The grammar's `EOF` terminal.
    Eof,
    /// The end of input reached in an exclusive mode, where upstream never emits `EOF`.
    EndOfInput,
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
    /// Set once the end of the input has produced its last token; later calls report the
    /// end of input without the `EOF` terminal, as jison's `done` flag does.
    ended: bool,
}

impl<'a> Lexer<'a> {
    pub(crate) fn new(src: &'a str) -> Self {
        Self {
            src,
            pos: 0,
            line: 1,
            column: 1,
            modes: vec![Mode::Initial],
            ended: false,
        }
    }

    pub(crate) fn next_token(&mut self) -> Result<Token, ParseError> {
        loop {
            if self.pos >= self.src.len() {
                return Ok(self.end_token());
            }
            match self.mode() {
                Mode::Initial => {
                    if let Some(token) = self.initial()? {
                        return Ok(token);
                    }
                }
                Mode::Block => {
                    if let Some(token) = self.block() {
                        return Ok(token);
                    }
                }
                Mode::BlockBacktick => {
                    if let Some(token) = self.block_backtick() {
                        return Ok(token);
                    }
                }
                Mode::AccTitle | Mode::AccDescr => return Ok(self.acc_value()),
                Mode::AccDescrMultiline => {
                    if let Some(token) = self.acc_descr_multiline() {
                        return Ok(token);
                    }
                }
                Mode::Style => {
                    if let Some(token) = self.style()? {
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

    /// `<<EOF>>` fires only in the inclusive conditions. In an exclusive mode a rule that
    /// matches the empty string may fire once (the `acc_title`/`acc_descr` value rules and
    /// the multi-line description rule); otherwise the input simply ends, without the
    /// grammar's `EOF` terminal.
    fn end_token(&mut self) -> Token {
        let mark = self.mark();
        if self.ended {
            return self.make(TokenKind::EndOfInput, mark);
        }
        match self.mode() {
            Mode::Initial => {
                self.ended = true;
                self.make(TokenKind::Eof, mark)
            }
            Mode::AccTitle => {
                self.ended = true;
                self.pop();
                self.make_text(TokenKind::AccTitleValue, "", mark)
            }
            Mode::AccDescr => {
                self.ended = true;
                self.pop();
                self.make_text(TokenKind::AccDescrValue, "", mark)
            }
            Mode::AccDescrMultiline => {
                self.ended = true;
                self.make_text(TokenKind::AccDescrMultilineValue, "", mark)
            }
            Mode::Block | Mode::BlockBacktick | Mode::Style => {
                self.ended = true;
                self.make(TokenKind::EndOfInput, mark)
            }
        }
    }

    // -- INITIAL ------------------------------------------------------------------------

    fn initial(&mut self) -> Result<Option<Token>, ParseError> {
        // accTitle\s*":"\s*
        if let Some(len) = self.acc_len("accTitle") {
            let mark = self.mark();
            self.bump(len);
            self.push(Mode::AccTitle);
            return Ok(Some(self.make(TokenKind::AccTitle, mark)));
        }
        // accDescr\s*":"\s*
        if let Some(len) = self.acc_len("accDescr") {
            let mark = self.mark();
            self.bump(len);
            self.push(Mode::AccDescr);
            return Ok(Some(self.make(TokenKind::AccDescr, mark)));
        }
        // accDescr\s*"{"\s*
        if let Some(len) = self.acc_brace_len("accDescr") {
            self.bump(len);
            self.push(Mode::AccDescrMultiline);
            return Ok(None);
        }
        // .*direction\s+TB[^\n]* (and BT, RL, LR, in that order)
        if let Some((direction, len)) = self.direction_line() {
            let mark = self.mark();
            self.bump(len);
            return Ok(Some(self.make(TokenKind::Direction(direction), mark)));
        }
        // [ \t\r]+
        if let Some(len) = leading_run(self.rest(), |ch| matches!(ch, ' ' | '\t' | '\r')) {
            self.bump(len);
            return Ok(None);
        }
        // [\n]+
        if self.rest().starts_with('\n') {
            let mark = self.mark();
            let len = run_len(self.rest(), |ch| ch == '\n');
            self.bump(len);
            return Ok(Some(self.make(TokenKind::Newline, mark)));
        }
        // \"[^"%\r\n\v\b\\]+\"
        if let Some(len) = quoted_entity_name_len(self.rest()) {
            let mark = self.mark();
            self.bump(len);
            return Ok(Some(self.make(TokenKind::EntityName, mark)));
        }
        // \"[^"]*\"
        if let Some(len) = quoted_word_len(self.rest()) {
            let mark = self.mark();
            self.bump(len);
            return Ok(Some(self.make(TokenKind::Word, mark)));
        }
        // "erDiagram"
        if let Some(len) = self.keyword("erDiagram") {
            let mark = self.mark();
            self.bump(len);
            return Ok(Some(self.make(TokenKind::ErDiagram, mark)));
        }
        // "{" begins the block condition.
        if self.rest().starts_with('{') {
            let mark = self.mark();
            self.bump(1);
            self.push(Mode::Block);
            return Ok(Some(self.make(TokenKind::BlockStart, mark)));
        }
        // \# and "#"
        if self.rest().starts_with('#') {
            let mark = self.mark();
            self.bump(1);
            return Ok(Some(self.make(TokenKind::Brkt, mark)));
        }
        // ","
        if self.rest().starts_with(',') {
            let mark = self.mark();
            self.bump(1);
            return Ok(Some(self.make(TokenKind::Comma, mark)));
        }
        // ":::"
        if self.rest().starts_with(":::") {
            let mark = self.mark();
            self.bump(3);
            return Ok(Some(self.make(TokenKind::StyleSeparator, mark)));
        }
        // ":"
        if self.rest().starts_with(':') {
            let mark = self.mark();
            self.bump(1);
            return Ok(Some(self.make(TokenKind::Colon, mark)));
        }
        // "["
        if self.rest().starts_with('[') {
            let mark = self.mark();
            self.bump(1);
            return Ok(Some(self.make(TokenKind::Sqs, mark)));
        }
        // "]"
        if self.rest().starts_with(']') {
            let mark = self.mark();
            self.bump(1);
            return Ok(Some(self.make(TokenKind::Sqe, mark)));
        }
        // "style" begins the style condition.
        if let Some(len) = self.keyword("style") {
            let mark = self.mark();
            self.bump(len);
            self.push(Mode::Style);
            return Ok(Some(self.make(TokenKind::Style, mark)));
        }
        // "classDef" begins the style condition.
        if let Some(len) = self.keyword("classDef") {
            let mark = self.mark();
            self.bump(len);
            self.push(Mode::Style);
            return Ok(Some(self.make(TokenKind::ClassDef, mark)));
        }
        // "class"
        if let Some(len) = self.keyword("class") {
            let mark = self.mark();
            self.bump(len);
            return Ok(Some(self.make(TokenKind::Class, mark)));
        }
        // "subgraph"
        if let Some(len) = self.keyword("subgraph") {
            let mark = self.mark();
            self.bump(len);
            return Ok(Some(self.make(TokenKind::Subgraph, mark)));
        }
        // "end"\b\s*
        if let Some(len) = self.keyword("end") {
            let mark = self.mark();
            let spaces = leading_run(&self.rest()[len..], is_space).unwrap_or(0);
            self.bump(len + spaces);
            return Ok(Some(self.make(TokenKind::End, mark)));
        }
        // The cardinality rules, in grammar order. A literal ending in a word character
        // carries the `\b` jison appends; the others do not.
        for (literal, boundary, kind) in CARDINALITY_RULES {
            let matched = if *boundary {
                self.keyword(literal)
            } else {
                self.literal(literal)
            };
            if let Some(len) = matched {
                let mark = self.mark();
                self.bump(len);
                return Ok(Some(self.make(*kind, mark)));
            }
        }
        // [0-9]+\.[0-9]+
        if let Some(len) = decimal_len(self.rest()) {
            let mark = self.mark();
            self.bump(len);
            return Ok(Some(self.make(TokenKind::DecimalNum, mark)));
        }
        // "1" followed by \s+[A-Za-z_"'], by \s+[0-9], or by a relationship operator.
        if let Some(len) = entity_one_len(self.rest()) {
            let mark = self.mark();
            self.bump(len);
            return Ok(Some(self.make(TokenKind::OnlyOne, mark)));
        }
        // "1"
        if let Some(len) = self.keyword("1") {
            let mark = self.mark();
            self.bump(len);
            return Ok(Some(self.make(TokenKind::EntityOne, mark)));
        }
        // [0-9]+
        if let Some(len) = leading_run(self.rest(), |ch| ch.is_ascii_digit()) {
            let mark = self.mark();
            self.bump(len);
            return Ok(Some(self.make(TokenKind::Num, mark)));
        }
        // \|\|
        if let Some(len) = self.literal("||") {
            let mark = self.mark();
            self.bump(len);
            return Ok(Some(self.make(TokenKind::OnlyOne, mark)));
        }
        // o\| — ends in a word character, so jison appends \b.
        if let Some(len) = self.keyword("o|") {
            let mark = self.mark();
            self.bump(len);
            return Ok(Some(self.make(TokenKind::ZeroOrOne, mark)));
        }
        // o\{
        if let Some(len) = self.literal("o{") {
            let mark = self.mark();
            self.bump(len);
            return Ok(Some(self.make(TokenKind::ZeroOrMore, mark)));
        }
        // \|\{
        if let Some(len) = self.literal("|{") {
            let mark = self.mark();
            self.bump(len);
            return Ok(Some(self.make(TokenKind::OneOrMore, mark)));
        }
        // u(?=[\.\-\|])
        if self.starts_with_ci("u")
            && self.rest()[1..]
                .chars()
                .next()
                .is_some_and(|ch| matches!(ch, '.' | '-' | '|'))
        {
            let mark = self.mark();
            self.bump(1);
            return Ok(Some(self.make(TokenKind::MdParent, mark)));
        }
        // \.\.
        if let Some(len) = self.literal("..") {
            let mark = self.mark();
            self.bump(len);
            return Ok(Some(self.make(TokenKind::NonIdentifying, mark)));
        }
        // \-\-
        if let Some(len) = self.literal("--") {
            let mark = self.mark();
            self.bump(len);
            return Ok(Some(self.make(TokenKind::Identifying, mark)));
        }
        // "to"
        if let Some(len) = self.keyword("to") {
            let mark = self.mark();
            self.bump(len);
            return Ok(Some(self.make(TokenKind::Identifying, mark)));
        }
        // "optionally to"
        if let Some(len) = self.keyword("optionally to") {
            let mark = self.mark();
            self.bump(len);
            return Ok(Some(self.make(TokenKind::NonIdentifying, mark)));
        }
        // \.\-
        if let Some(len) = self.literal(".-") {
            let mark = self.mark();
            self.bump(len);
            return Ok(Some(self.make(TokenKind::NonIdentifying, mark)));
        }
        // \-\. (written `\-\.` upstream)
        if let Some(len) = self.literal("-.") {
            let mark = self.mark();
            self.bump(len);
            return Ok(Some(self.make(TokenKind::NonIdentifying, mark)));
        }
        // ([^\x00-\x7F]|\w|\-|\*|\.)+
        if let Some(len) = leading_run(self.rest(), is_unicode_text) {
            let mark = self.mark();
            self.bump(len);
            return Ok(Some(self.make(TokenKind::UnicodeText, mark)));
        }
        // `.` catch-all: the grammar sees the character itself.
        let mark = self.mark();
        let len = self.peek().map_or(1, char::len_utf8);
        self.bump(len);
        let ch = self.src[mark.pos..self.pos].chars().next().unwrap_or('\0');
        Ok(Some(self.make(TokenKind::Char(ch), mark)))
    }

    // -- block --------------------------------------------------------------------------

    fn block(&mut self) -> Option<Token> {
        // \s+
        if let Some(len) = leading_run(self.rest(), is_space) {
            self.bump(len);
            return None;
        }
        // \b((?:PK)|(?:FK)|(?:UK))\b
        for key in ["PK", "FK", "UK"] {
            if let Some(len) = self.keyword(key) {
                let mark = self.mark();
                self.bump(len);
                return Some(self.make(TokenKind::AttributeKey, mark));
            }
        }
        // ([^\s]*)[~].*[~]([^\s]*)
        if let Some(len) = generic_type_len(self.rest()) {
            let mark = self.mark();
            self.bump(len);
            return Some(self.make(TokenKind::AttributeWord, mark));
        }
        // ([\*A-Za-z_\u00C0-\uFFFF][A-Za-z0-9\-\_\[\]\(\)\.,\u00C0-\uFFFF\*]*)
        if let Some(len) = attribute_word_len(self.rest()) {
            let mark = self.mark();
            self.bump(len);
            return Some(self.make(TokenKind::AttributeWord, mark));
        }
        // [`] begins the backtick condition.
        if self.rest().starts_with('`') {
            self.bump(1);
            self.push(Mode::BlockBacktick);
            return None;
        }
        // \"[^"]*\"
        if let Some(len) = quoted_word_len(self.rest()) {
            let mark = self.mark();
            self.bump(len);
            return Some(self.make(TokenKind::Comment, mark));
        }
        // [\n]+ is unreachable: \s+ above matches it first.
        // "}"
        if self.rest().starts_with('}') {
            let mark = self.mark();
            self.bump(1);
            self.pop();
            return Some(self.make(TokenKind::BlockStop, mark));
        }
        // `.` catch-all: `?`, `,`, and anything else the attribute rules do not take.
        let mark = self.mark();
        let len = self.peek().map_or(1, char::len_utf8);
        self.bump(len);
        let ch = self.src[mark.pos..self.pos].chars().next().unwrap_or('\0');
        Some(self.make(TokenKind::Char(ch), mark))
    }

    fn block_backtick(&mut self) -> Option<Token> {
        if self.rest().starts_with('`') {
            self.bump(1);
            self.pop();
            return None;
        }
        let mark = self.mark();
        let len = run_len(self.rest(), |ch| ch != '`');
        self.bump(len);
        Some(self.make(TokenKind::AttributeWord, mark))
    }

    // -- acc_title / acc_descr / acc_descr_multiline --------------------------------------

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

    // -- style --------------------------------------------------------------------------

    fn style(&mut self) -> Result<Option<Token>, ParseError> {
        // [\n]+ pops the condition.
        if self.rest().starts_with('\n') {
            let mark = self.mark();
            let len = run_len(self.rest(), |ch| ch == '\n');
            self.bump(len);
            self.pop();
            return Ok(Some(self.make(TokenKind::Newline, mark)));
        }
        // \s+
        if let Some(len) = leading_run(self.rest(), is_space) {
            self.bump(len);
            return Ok(None);
        }
        for (literal, kind) in [
            (":", TokenKind::Colon),
            (",", TokenKind::Comma),
            ("#", TokenKind::Brkt),
        ] {
            if let Some(len) = self.literal(literal) {
                let mark = self.mark();
                self.bump(len);
                return Ok(Some(self.make(kind, mark)));
            }
        }
        // ([^\x00-\x7F]|\w|\-|\*)+
        if let Some(len) = leading_run(self.rest(), is_style_text) {
            let mark = self.mark();
            self.bump(len);
            return Ok(Some(self.make(TokenKind::StyleText, mark)));
        }
        // ';'
        if let Some(len) = self.literal(";") {
            let mark = self.mark();
            self.bump(len);
            return Ok(Some(self.make(TokenKind::Semi, mark)));
        }
        // No catch-all in this exclusive condition: an unrecognized character is a lexical
        // error, as in jison.
        Err(self.lexical_error())
    }

    // -- helpers ------------------------------------------------------------------------

    fn lexical_error(&self) -> ParseError {
        ParseError::at(
            format!("Lexical error on line {}.\nUnrecognized text.", self.line),
            self.span_from(self.mark()),
        )
    }

    fn starts_with_ci(&self, literal: &str) -> bool {
        self.rest()
            .get(..literal.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(literal))
    }

    /// A literal, matched case-insensitively (`%options case-insensitive`).
    fn literal(&self, literal: &str) -> Option<usize> {
        self.starts_with_ci(literal).then_some(literal.len())
    }

    /// A literal that ends in a word character: jison appends `\b`, so the next character
    /// must not be part of a word.
    fn keyword(&self, literal: &str) -> Option<usize> {
        if !self.starts_with_ci(literal) {
            return None;
        }
        match self.rest()[literal.len()..].chars().next() {
            Some(ch) if is_word_char(ch) => None,
            _ => Some(literal.len()),
        }
    }

    /// `accTitle\s*":"\s*` and `accDescr\s*":"\s*`.
    fn acc_len(&self, word: &str) -> Option<usize> {
        if !self.starts_with_ci(word) {
            return None;
        }
        let rest = self.rest();
        let mut len = word.len();
        len += leading_run(&rest[len..], is_space).unwrap_or(0);
        if !rest[len..].starts_with(':') {
            return None;
        }
        len += 1;
        len += leading_run(&rest[len..], is_space).unwrap_or(0);
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

    /// `.*direction\s+TB[^\n]*`, then `BT`, `RL`, and `LR`, taken in that order. The
    /// greedy `.*` finds the last `direction` on the current line that continues with the
    /// wanted word; either way the token runs to the end of the line the word ends on.
    fn direction_line(&self) -> Option<(Direction, usize)> {
        let rest = self.rest();
        let line_end = rest.find('\n').unwrap_or(rest.len());
        for (word, direction) in [
            ("TB", Direction::Tb),
            ("BT", Direction::Bt),
            ("RL", Direction::Rl),
            ("LR", Direction::Lr),
        ] {
            if let Some(len) = direction_match(rest, line_end, word) {
                return Some((direction, len));
            }
        }
        None
    }
}

/// The cardinality rules in grammar order, with whether jison appends a trailing `\b`
/// (it does when the pattern ends in a word character). `"1+"` and `"0+"` end in a
/// quantifier and `many(0)`/`many(1)` in a parenthesis, so they carry none.
const CARDINALITY_RULES: &[(&str, bool, TokenKind)] = &[
    ("one or zero", true, TokenKind::ZeroOrOne),
    ("one or more", true, TokenKind::OneOrMore),
    ("one or many", true, TokenKind::OneOrMore),
    ("1+", false, TokenKind::OneOrMore),
    ("|o", true, TokenKind::ZeroOrOne),
    ("zero or one", true, TokenKind::ZeroOrOne),
    ("zero or more", true, TokenKind::ZeroOrMore),
    ("zero or many", true, TokenKind::ZeroOrMore),
    ("0+", false, TokenKind::ZeroOrMore),
    ("}o", false, TokenKind::ZeroOrMore),
    ("many(0)", false, TokenKind::ZeroOrMore),
    ("many(1)", false, TokenKind::OneOrMore),
    ("many", true, TokenKind::ZeroOrMore),
    ("}|", false, TokenKind::OneOrMore),
    ("one", true, TokenKind::OnlyOne),
    ("only one", true, TokenKind::OnlyOne),
];

fn direction_match(rest: &str, line_end: usize, target: &str) -> Option<usize> {
    let line = &rest[..line_end];
    let mut search_end = line.len();
    while let Some(index) = rfind_ci(&line[..search_end], "direction") {
        let after = &rest[index + "direction".len()..];
        let spaces = leading_run(after, is_space).unwrap_or(0);
        if spaces > 0 && starts_with_ci_at(&after[spaces..], target) {
            let end = index + "direction".len() + spaces + target.len();
            let tail = &rest[end..];
            return Some(end + tail.find('\n').unwrap_or(tail.len()));
        }
        search_end = index;
    }
    None
}

fn rfind_ci(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .char_indices()
        .rev()
        .find(|(index, _)| starts_with_ci_at(&haystack[*index..], needle))
        .map(|(index, _)| index)
}

fn starts_with_ci_at(s: &str, literal: &str) -> bool {
    s.get(..literal.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(literal))
}

/// `\"[^"%\r\n\v\b\\]+\"`.
fn quoted_entity_name_len(s: &str) -> Option<usize> {
    let rest = s.strip_prefix('"')?;
    let mut len = 1;
    for ch in rest.chars() {
        if ch == '"' {
            return (len > 1).then_some(len + 1);
        }
        if matches!(ch, '%' | '\r' | '\n' | '\u{000b}' | '\u{0008}' | '\\') {
            return None;
        }
        len += ch.len_utf8();
    }
    None
}

/// `\"[^"]*\"`.
fn quoted_word_len(s: &str) -> Option<usize> {
    let rest = s.strip_prefix('"')?;
    rest.find('"').map(|index| 1 + index + 1)
}

/// `[0-9]+\.[0-9]+`.
fn decimal_len(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut index = 0;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    if index == 0 || bytes.get(index) != Some(&b'.') {
        return None;
    }
    index += 1;
    let start = index;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    (index > start).then_some(index)
}

/// `"1"(?=\s+[A-Za-z_"'])`, `"1"(?=\s+[0-9])`, and
/// `"1"(?=(\-\-|\.\.|\.\-|\-\.))`.
fn entity_one_len(s: &str) -> Option<usize> {
    let rest = s.strip_prefix('1')?;
    if ["--", "..", ".-", "-."]
        .iter()
        .any(|operator| rest.starts_with(operator))
    {
        return Some(1);
    }
    let spaces = leading_run(rest, is_space).unwrap_or(0);
    if spaces == 0 {
        return None;
    }
    let next = rest[spaces..].chars().next()?;
    if next.is_ascii_alphabetic() || next.is_ascii_digit() || matches!(next, '_' | '"' | '\'') {
        return Some(1);
    }
    None
}

/// `([^\s]*)[~].*[~]([^\s]*)`: a generic type such as `type~T~`, whose body may span
/// spaces up to the last `~` on the line.
fn generic_type_len(s: &str) -> Option<usize> {
    let leading = run_len(s, |ch| !is_space(ch));
    let first = s[..leading].find('~')?;
    let line_end = s.find('\n').unwrap_or(s.len());
    let last = s[..line_end].rfind('~')?;
    if last <= first {
        return None;
    }
    let run = run_len(&s[last + 1..], |ch| !is_space(ch));
    Some(last + 1 + run)
}

/// The attribute word rule: `*`, an ASCII letter, `_`, or `\u00C0`–`\uFFFF` first, then
/// letters, digits, `-`, `_`, brackets, parentheses, `.`, `,`, `*`, or `\u00C0`–`\uFFFF`.
fn attribute_word_len(s: &str) -> Option<usize> {
    let mut chars = s.chars();
    let first = chars.next()?;
    if !is_attribute_start(first) {
        return None;
    }
    let mut len = first.len_utf8();
    for ch in chars {
        if !is_attribute_tail(ch) {
            break;
        }
        len += ch.len_utf8();
    }
    Some(len)
}

fn is_attribute_start(ch: char) -> bool {
    ch == '*' || ch.is_ascii_alphabetic() || ch == '_' || is_wide_char(ch)
}

fn is_attribute_tail(ch: char) -> bool {
    ch.is_ascii_alphanumeric()
        || matches!(ch, '-' | '_' | '[' | ']' | '(' | ')' | '.' | ',' | '*')
        || is_wide_char(ch)
}

/// `\u00C0`–`\uFFFF`, the range both attribute rules spell out. JavaScript matches UTF-16
/// code units, so an astral character reaches the range as a surrogate pair; in Rust every
/// character at or above `\u00C0` is inside it.
fn is_wide_char(ch: char) -> bool {
    ch >= '\u{00c0}'
}

/// `([^\x00-\x7F]|\w|\-|\*)+`.
fn is_style_text(ch: char) -> bool {
    !ch.is_ascii() || is_word_char(ch) || matches!(ch, '-' | '*')
}

/// `([^\x00-\x7F]|\w|\-|\*|\.)+`.
fn is_unicode_text(ch: char) -> bool {
    !ch.is_ascii() || is_word_char(ch) || matches!(ch, '-' | '*' | '.')
}

fn is_word_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

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

/// JavaScript's `\s` (ECMAScript WhiteSpace and LineTerminator), from the shared helper.
pub(crate) use crate::common::is_js_whitespace as is_space;

/// JavaScript's `String.prototype.trim`.
pub(crate) use crate::common::js_trim;
