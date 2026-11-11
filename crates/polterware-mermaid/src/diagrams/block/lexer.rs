//! The block lexer: a hand port of the `%lex` section of upstream `parser/block.jison`.
//!
//! The grammar uses exclusive start conditions with a stack (`pushState`/`popState`), and
//! jison's default lexing rule: the *first* matching rule of the current condition wins, not
//! the longest. The rule order, the rule numbers, and each rule's generated regular
//! expression are upstream's, transcribed from what `jison` builds for `block.jison`
//! (quoted strings are literal here, and a quoted string that ends the pattern in a word
//! character gets a trailing `\b`, as lex-parser adds one).
//!
//! Three consequences of that exact order are easy to miss and are reproduced here:
//!
//! * `[\s]+` comes before the `<INITIAL>` newline rule, so it swallows every newline and
//!   the `NL` token is never produced. The grammar's `NL`/`SPACE` rules are therefore dead.
//! * `NODE_ID` comes before the link rules, so a block id swallows `~` and the `~~` link,
//!   and an `o`/`x` that starts a link, like the `o` in `A o-- B`.
//! * `[\s]+` is an `INITIAL` rule, so whitespace is not skipped inside a node's shape:
//!   `A[ "x"]` is a lexical error upstream, and here.
//!
//! Comments are removed before the parser runs upstream (`cleanupComments` in
//! `diagram-api/comments.ts`, applied by `preprocessDiagram`); this crate keeps comments in
//! the body, so the lexer blanks the text of every comment line and keeps its newline, which
//! leaves both the tokens and the line numbers of the source intact.
//!
//! mermaid parses `cleaned + '\n'` (`Diagram.fromText`), so the lexer reads the body plus one
//! trailing newline. That matters: at the end of a `classDef`/`style` line the last rule
//! wants whitespace after the id, and at end of input jison fires one zero-length match
//! before it answers `EOF`.

use crate::error::{ParseError, Span};
use crate::preprocess::is_js_whitespace;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    BlockDiagramKey,
    IdBlock,
    Columns,
    SpaceBlock,
    Default,
    LinkStyle,
    Interpolate,
    ClassDef,
    DefaultClassDefId,
    ClassDefId,
    ClassDefStyleOpts,
    Class,
    ClassEntityIds,
    StyleClass,
    Style,
    StyleEntityIds,
    StyleDefinitionData,
    AccTitle,
    AccTitleValue,
    AccDescr,
    AccDescrValue,
    AccDescrMultilineValue,
    End,
    NodeDStart,
    NodeDEnd,
    BlockArrowStart,
    BlockArrowEnd,
    NodeId,
    MdStr,
    NodeDescr,
    Str,
    Dir,
    Link,
    StartLink,
    LinkLabel,
    Size,
    Eof,
}

impl TokenKind {
    /// The terminal name upstream's parser reports in an error message.
    pub fn name(self) -> &'static str {
        match self {
            TokenKind::BlockDiagramKey => "BLOCK_DIAGRAM_KEY",
            TokenKind::IdBlock => "id-block",
            TokenKind::Columns => "COLUMNS",
            TokenKind::SpaceBlock => "SPACE_BLOCK",
            TokenKind::Default => "DEFAULT",
            TokenKind::LinkStyle => "LINKSTYLE",
            TokenKind::Interpolate => "INTERPOLATE",
            TokenKind::ClassDef => "classDef",
            TokenKind::DefaultClassDefId => "DEFAULT_CLASSDEF_ID",
            TokenKind::ClassDefId => "CLASSDEF_ID",
            TokenKind::ClassDefStyleOpts => "CLASSDEF_STYLEOPTS",
            TokenKind::Class => "class",
            TokenKind::ClassEntityIds => "CLASSENTITY_IDS",
            TokenKind::StyleClass => "STYLECLASS",
            TokenKind::Style => "style",
            TokenKind::StyleEntityIds => "STYLE_ENTITY_IDS",
            TokenKind::StyleDefinitionData => "STYLE_DEFINITION_DATA",
            TokenKind::AccTitle => "acc_title",
            TokenKind::AccTitleValue => "acc_title_value",
            TokenKind::AccDescr => "acc_descr",
            TokenKind::AccDescrValue => "acc_descr_value",
            TokenKind::AccDescrMultilineValue => "acc_descr_multiline_value",
            TokenKind::End => "end",
            TokenKind::NodeDStart => "NODE_DSTART",
            TokenKind::NodeDEnd => "NODE_DEND",
            TokenKind::BlockArrowStart => "BLOCK_ARROW_START",
            TokenKind::BlockArrowEnd => "BLOCK_ARROW_END",
            TokenKind::NodeId => "NODE_ID",
            TokenKind::MdStr => "MD_STR",
            TokenKind::NodeDescr => "NODE_DESCR",
            TokenKind::Str => "STR",
            TokenKind::Dir => "DIR",
            TokenKind::Link => "LINK",
            TokenKind::StartLink => "START_LINK",
            TokenKind::LinkLabel => "LINK_LABEL",
            TokenKind::Size => "SIZE",
            TokenKind::Eof => "EOF",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Initial,
    String,
    MdString,
    Node,
    BlockArrow,
    ArrowDir,
    LabelLink,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
    ClassDef,
    ClassDefId,
    Class,
    ClassStyle,
    StyleStatement,
    StyleDefinition,
}

/// The rules of each start condition, in jison's test order.
fn rules(mode: Mode) -> &'static [u16] {
    match mode {
        Mode::Initial => &[
            0, 1, 2, 3, 4, 5, 6, 7, 8, 11, 14, 15, 16, 17, 18, 19, 23, 26, 29, 31, 33, 36, 54, 55,
            56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 92, 93,
            94, 95, 96, 97, 98, 104,
        ],
        Mode::String => &[12, 13, 82, 83],
        Mode::MdString => &[9, 10, 78, 79],
        Mode::Node => &[
            37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 77, 80,
        ],
        Mode::BlockArrow => &[76, 81, 84],
        Mode::ArrowDir => &[85, 86, 87, 88, 89, 90, 91],
        Mode::LabelLink => &[99, 100, 101, 102, 103],
        Mode::AccTitle => &[30],
        Mode::AccDescr => &[32],
        Mode::AccDescrMultiline => &[34, 35],
        Mode::ClassDef => &[20, 21],
        Mode::ClassDefId => &[22],
        Mode::Class => &[24],
        Mode::ClassStyle => &[25],
        Mode::StyleStatement => &[27],
        Mode::StyleDefinition => &[28],
    }
}

pub struct Lexer {
    /// The body with comment lines blanked, plus mermaid's trailing newline. Every byte of
    /// the body keeps its offset, so a span is a body span.
    text: String,
    body_len: usize,
    line_starts: Vec<usize>,
    pos: usize,
    modes: Vec<Mode>,
    done: bool,
}

impl Lexer {
    pub fn new(body: &str) -> Self {
        let mut text = blank_comments(body);
        text.push('\n');
        let mut line_starts = vec![0];
        for (index, byte) in body.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(index + 1);
            }
        }
        Self {
            text,
            body_len: body.len(),
            line_starts,
            pos: 0,
            modes: vec![Mode::Initial],
            done: false,
        }
    }

    fn mode(&self) -> Mode {
        *self
            .modes
            .last()
            .expect("the mode stack always keeps INITIAL")
    }

    fn push_mode(&mut self, mode: Mode) {
        self.modes.push(mode);
    }

    fn pop_mode(&mut self) {
        if self.modes.len() > 1 {
            self.modes.pop();
        }
    }

    fn rest(&self) -> &str {
        &self.text[self.pos..]
    }

    fn char_at(&self, index: usize) -> Option<char> {
        if index < self.text.len() {
            self.text[index..].chars().next()
        } else {
            None
        }
    }

    fn word_boundary(&self, index: usize) -> bool {
        self.char_at(index)
            .is_none_or(|ch| !(ch.is_ascii_alphanumeric() || ch == '_'))
    }

    fn skip_whitespace(&self, mut index: usize) -> usize {
        while let Some(ch) = self.char_at(index) {
            if is_js_whitespace(ch) {
                index += ch.len_utf8();
            } else {
                break;
            }
        }
        index
    }

    fn run_while<F: Fn(char) -> bool>(&self, mut index: usize, keep: F) -> usize {
        while let Some(ch) = self.char_at(index) {
            if keep(ch) {
                index += ch.len_utf8();
            } else {
                break;
            }
        }
        index
    }

    fn literal(&self, needle: &str) -> Option<usize> {
        self.rest().starts_with(needle).then_some(needle.len())
    }

    fn literal_word(&self, needle: &str) -> Option<usize> {
        self.literal(needle)
            .filter(|&length| self.word_boundary(self.pos + length))
    }

    fn span(&self, start: usize, end: usize) -> Span {
        let start = start.min(self.body_len);
        let end = end.clamp(start, self.body_len);
        let line = self.line_starts.partition_point(|&offset| offset <= start) as u32;
        let line_start = self.line_starts[line.saturating_sub(1) as usize];
        let column = self.text[line_start..start].chars().count() as u32 + 1;
        Span {
            line,
            column,
            start,
            end,
        }
    }

    pub fn next_token(&mut self) -> Result<Token, ParseError> {
        loop {
            if self.done {
                return Ok(self.eof());
            }
            if self.pos >= self.text.len() {
                self.done = true;
            }
            let mut found = None;
            for &rule in rules(self.mode()) {
                if let Some(length) = self.match_rule(rule) {
                    found = Some((rule, length));
                    break;
                }
            }
            let Some((rule, length)) = found else {
                if self.pos >= self.text.len() {
                    return Ok(self.eof());
                }
                let span = self.span(self.pos, self.pos);
                let ch = self
                    .char_at(self.pos)
                    .map_or("<eof>".to_string(), |ch| ch.to_string());
                return Err(ParseError::at(format!("unexpected character '{ch}'"), span));
            };
            let start = self.pos;
            self.pos += length;
            let span = self.span(start, self.pos);
            let matched = self.text[start..self.pos].to_string();
            if let Some(kind) = self.action(rule)? {
                return Ok(Token {
                    kind,
                    text: token_text(rule, &matched),
                    span,
                });
            }
        }
    }

    fn eof(&self) -> Token {
        Token {
            kind: TokenKind::Eof,
            text: String::new(),
            span: self.span(self.pos, self.pos),
        }
    }

    fn match_rule(&self, rule: u16) -> Option<usize> {
        if let Some(needle) = node_literal(rule) {
            return self.literal(needle);
        }
        if let Some(keyword) = arrow_direction(rule) {
            return self.match_arrow_direction(keyword);
        }
        let at = self.pos;
        match rule {
            0 => self.literal_word("block-beta"),
            1 => self.literal("block:"),
            2 => self.literal_word("block"),
            3 => {
                let end = self.run_while(at, is_js_whitespace);
                (end > at).then_some(end - at)
            }
            4 => {
                let end = self.run_while(at, |ch| ch == '\n');
                (end > at).then_some(end - at)
            }
            5 => self.literal("\r\n").or_else(|| self.literal("\n")),
            6 => {
                let length = self.literal("columns")?;
                let after_whitespace = self.skip_whitespace(at + length);
                if after_whitespace == at + length {
                    return None;
                }
                let end = self.literal_at(after_whitespace, "auto")?;
                self.word_boundary(end).then_some(end - at)
            }
            7 => {
                let length = self.literal("columns")?;
                let after_whitespace = self.skip_whitespace(at + length);
                if after_whitespace == at + length {
                    return None;
                }
                let end = self.run_while(after_whitespace, |ch| ch.is_ascii_digit());
                (end > after_whitespace).then_some(end - at)
            }
            8 | 76 | 77 | 99 => self.literal("\"`"),
            9 | 78 => {
                let end = self.run_while(at, |ch| ch != '`' && ch != '"');
                (end > at).then_some(end - at)
            }
            10 | 79 => self.literal("`\""),
            11 | 12 | 80 | 81 | 83 | 100 => self.literal("\""),
            13 => Some(self.run_while(at, |ch| ch != '"') - at),
            14 => self.literal("space").and_then(|length| {
                (self.char_at(at + length) == Some(':')).then(|| {
                    let digits = self.run_while(at + length + 1, |ch| ch.is_ascii_digit());
                    (digits > at + length + 1).then_some(digits - at)
                })?
            }),
            15 => self.literal_word("space"),
            16 => self.literal_word("default"),
            17 => self.literal_word("linkStyle"),
            18 => self.literal_word("interpolate"),
            19 => self.literal("classDef").and_then(|length| {
                let end = self.run_while(at + length, is_js_whitespace);
                (end > at + length).then_some(end - at)
            }),
            20 => self.literal("DEFAULT").and_then(|length| {
                let end = self.run_while(at + length, is_js_whitespace);
                (end > at + length).then_some(end - at)
            }),
            21 => {
                let word = self.run_while(at, is_word_char);
                if word == at {
                    return None;
                }
                let end = self.run_while(word, is_js_whitespace);
                (end > word).then_some(end - at)
            }
            22 | 25 | 28 | 30 | 32 => Some(self.line_end(at) - at),
            23 => self.literal("class").and_then(|length| {
                let end = self.run_while(at + length, is_js_whitespace);
                (end > at + length).then_some(end - at)
            }),
            24 | 27 => self.match_entity_ids(at),
            26 => self.literal("style").and_then(|length| {
                let end = self.run_while(at + length, is_js_whitespace);
                (end > at + length).then_some(end - at)
            }),
            29 => self.match_acc_statement("accTitle", at, ':'),
            31 => self.match_acc_statement("accDescr", at, ':'),
            33 => self.match_acc_statement("accDescr", at, '{'),
            34 => self.literal("}"),
            35 => {
                let end = self.run_while(at, |ch| ch != '}');
                (end > at).then_some(end - at)
            }
            36 => self
                .literal_word("end")
                .map(|length| self.run_while(at + length, is_js_whitespace) - at),
            54 => self.literal("-)"),
            55 => self.literal("(-"),
            56 => self.literal("))"),
            57 => self.literal(")"),
            58 => self.literal("((("),
            59 => self.literal("(("),
            60 => self.literal("{{"),
            61 => self.literal("{"),
            62 => self.literal(">"),
            63 => self.literal("(["),
            64 => self.literal("("),
            65 => self.literal("[["),
            66 => self.literal("[|"),
            67 => self.literal("[("),
            68 => self.literal(")))"),
            69 | 71 => self.literal("[\\"),
            70 => self.literal("[/"),
            72 => self.literal("["),
            73 => self.literal("<["),
            74 => {
                let end = self.run_while(at, is_node_id_char);
                (end > at).then_some(end - at)
            }
            75 => (at >= self.text.len()).then_some(0),
            82 => {
                let end = self.run_while(at, |ch| ch != '"');
                (end > at).then_some(end - at)
            }
            84 => self.literal("]>").and_then(|length| {
                let after_whitespace = self.skip_whitespace(at + length);
                (self.char_at(after_whitespace) == Some('(')).then_some(after_whitespace + 1 - at)
            }),
            91 => self
                .literal(")")
                .map(|length| self.skip_whitespace(at + length) - at),
            92 | 101 => self.match_dash_link(at),
            93 | 102 => self.match_equals_link(at),
            94 | 103 => self.match_dotted_link(at),
            95 => self.match_tilde_link(at),
            96 => self.match_start_link(at, "--"),
            97 => self.match_start_link(at, "=="),
            98 => self.match_start_link(at, "-."),
            104 => {
                if self.char_at(at) != Some(':') {
                    return None;
                }
                let end = self.run_while(at + 1, |ch| ch.is_ascii_digit());
                (end > at + 1).then_some(end - at)
            }
            _ => None,
        }
    }

    fn literal_at(&self, index: usize, needle: &str) -> Option<usize> {
        self.text[index..]
            .starts_with(needle)
            .then_some(index + needle.len())
    }

    fn line_end(&self, index: usize) -> usize {
        self.text[index..]
            .find('\n')
            .map_or(self.text.len(), |offset| index + offset)
    }

    fn match_entity_ids(&self, at: usize) -> Option<usize> {
        let word = self.run_while(at, is_word_char);
        if word == at {
            return None;
        }
        let mut end = word;
        while self.char_at(end) == Some(',') {
            let after_comma = self.skip_whitespace(end + 1);
            let after_word = self.run_while(after_comma, is_word_char);
            if after_word == after_comma {
                break;
            }
            end = after_word;
        }
        Some(end - at)
    }

    /// `accTitle\s*":"\s*` and `accDescr\s*":"\s*` / `accDescr\s*"{"\s*`.
    fn match_acc_statement(&self, keyword: &str, at: usize, delimiter: char) -> Option<usize> {
        let length = self.literal(keyword)?;
        let after_whitespace = self.skip_whitespace(at + length);
        if self.char_at(after_whitespace) != Some(delimiter) {
            return None;
        }
        let end = self.skip_whitespace(after_whitespace + 1);
        Some(end - at)
    }

    /// `\s*[xo<]?--+[-xo>]\s*`: at least two dashes, the last of which (or the character
    /// after them) is the arrow head.
    fn match_dash_link(&self, at: usize) -> Option<usize> {
        let start = self.skip_whitespace(at);
        let mut index = self.skip_link_marker(start);
        let dashes = self.run_while(index, |ch| ch == '-');
        if dashes - index < 2 {
            return None;
        }
        // `--+` is greedy, so it keeps every dash but lets `[-xo>]` take a dash as well.
        index = if dashes - index >= 3 {
            dashes
        } else {
            match self.char_at(dashes) {
                Some(ch @ ('-' | 'x' | 'o' | '>')) => dashes + ch.len_utf8(),
                _ => return None,
            }
        };
        Some(self.skip_whitespace(index) - at)
    }

    /// `\s*[xo<]?==+[=xo>]\s*`.
    fn match_equals_link(&self, at: usize) -> Option<usize> {
        let start = self.skip_whitespace(at);
        let mut index = self.skip_link_marker(start);
        let equals = self.run_while(index, |ch| ch == '=');
        if equals - index < 2 {
            return None;
        }
        index = if equals - index >= 3 {
            equals
        } else {
            match self.char_at(equals) {
                Some(ch @ ('=' | 'x' | 'o' | '>')) => equals + ch.len_utf8(),
                _ => return None,
            }
        };
        Some(self.skip_whitespace(index) - at)
    }

    /// `\s*[xo<]?\-?\.+\-[xo>]?\s*`.
    fn match_dotted_link(&self, at: usize) -> Option<usize> {
        let start = self.skip_whitespace(at);
        let mut index = self.skip_link_marker(start);
        if self.char_at(index) == Some('-') {
            index += 1;
        }
        let dots = self.run_while(index, |ch| ch == '.');
        if dots == index || self.char_at(dots) != Some('-') {
            return None;
        }
        let mut end = dots + 1;
        if let Some(ch) = self.char_at(end)
            && matches!(ch, 'x' | 'o' | '>')
        {
            end += ch.len_utf8();
        }
        Some(self.skip_whitespace(end) - at)
    }

    /// `\s*~~[\~]+\s*`: the lexer's invisible link, dead because `NODE_ID` matches first.
    fn match_tilde_link(&self, at: usize) -> Option<usize> {
        let start = self.skip_whitespace(at);
        let index = self.literal_at(start, "~~")?;
        let end = self.run_while(index, |ch| ch == '~');
        if end == index {
            return None;
        }
        Some(self.skip_whitespace(end) - at)
    }

    /// `\s*[xo<]?--\s*`, `\s*[xo<]?==\s*`, and `\s*[xo<]?-\.\s*`.
    fn match_start_link(&self, at: usize, dashes: &str) -> Option<usize> {
        let start = self.skip_whitespace(at);
        let index = self.skip_link_marker(start);
        let end = self.literal_at(index, dashes)?;
        Some(self.skip_whitespace(end) - at)
    }

    fn skip_link_marker(&self, index: usize) -> usize {
        match self.char_at(index) {
            Some(ch @ ('x' | 'o' | '<')) => index + ch.len_utf8(),
            _ => index,
        }
    }

    fn match_arrow_direction(&self, keyword: &str) -> Option<usize> {
        let at = self.pos;
        let start = self.skip_whitespace(at);
        // `","?\s*right\s*`: the comma is optional, and without it the leading whitespace
        // stays in the token.
        let index = if self.char_at(start) == Some(',') {
            self.skip_whitespace(start + 1)
        } else {
            start
        };
        let end = self.literal_at(index, keyword)?;
        Some(self.skip_whitespace(end) - at)
    }

    fn action(&mut self, rule: u16) -> Result<Option<TokenKind>, ParseError> {
        let kind = match rule {
            0 | 2 => TokenKind::BlockDiagramKey,
            1 => TokenKind::IdBlock,
            3..=5 => return Ok(None),
            6 | 7 => TokenKind::Columns,
            8 | 76 | 77 | 99 => {
                self.push_mode(Mode::MdString);
                return Ok(None);
            }
            9 | 78 => TokenKind::MdStr,
            10 | 79 => {
                self.pop_mode();
                return Ok(None);
            }
            11 | 80 | 81 => {
                self.push_mode(Mode::String);
                return Ok(None);
            }
            12 | 83 => {
                self.pop_mode();
                return Ok(None);
            }
            13 => TokenKind::Str,
            14 | 15 => TokenKind::SpaceBlock,
            16 => TokenKind::Default,
            17 => TokenKind::LinkStyle,
            18 => TokenKind::Interpolate,
            19 => {
                self.push_mode(Mode::ClassDef);
                TokenKind::ClassDef
            }
            20 => {
                self.pop_mode();
                self.push_mode(Mode::ClassDefId);
                TokenKind::DefaultClassDefId
            }
            21 => {
                self.pop_mode();
                self.push_mode(Mode::ClassDefId);
                TokenKind::ClassDefId
            }
            22 => {
                self.pop_mode();
                TokenKind::ClassDefStyleOpts
            }
            23 => {
                self.push_mode(Mode::Class);
                TokenKind::Class
            }
            24 | 27 => {
                self.pop_mode();
                self.push_mode(if rule == 24 {
                    Mode::ClassStyle
                } else {
                    Mode::StyleDefinition
                });
                if rule == 24 {
                    TokenKind::ClassEntityIds
                } else {
                    TokenKind::StyleEntityIds
                }
            }
            25 => {
                self.pop_mode();
                TokenKind::StyleClass
            }
            26 => {
                self.push_mode(Mode::StyleStatement);
                TokenKind::Style
            }
            28 => {
                self.pop_mode();
                TokenKind::StyleDefinitionData
            }
            29 => {
                self.push_mode(Mode::AccTitle);
                TokenKind::AccTitle
            }
            30 => {
                self.pop_mode();
                TokenKind::AccTitleValue
            }
            31 => {
                self.push_mode(Mode::AccDescr);
                TokenKind::AccDescr
            }
            32 => {
                self.pop_mode();
                TokenKind::AccDescrValue
            }
            33 => {
                self.push_mode(Mode::AccDescrMultiline);
                return Ok(None);
            }
            34 => {
                self.pop_mode();
                return Ok(None);
            }
            35 => TokenKind::AccDescrMultilineValue,
            36 => TokenKind::End,
            37..=53 => {
                self.pop_mode();
                TokenKind::NodeDEnd
            }
            54..=73 => {
                self.push_mode(if rule == 73 {
                    Mode::BlockArrow
                } else {
                    Mode::Node
                });
                if rule == 73 {
                    TokenKind::BlockArrowStart
                } else {
                    TokenKind::NodeDStart
                }
            }
            74 => TokenKind::NodeId,
            75 => TokenKind::Eof,
            82 => TokenKind::NodeDescr,
            84 => {
                self.push_mode(Mode::ArrowDir);
                return Ok(None);
            }
            85..=90 => TokenKind::Dir,
            91 => {
                self.pop_mode();
                self.pop_mode();
                TokenKind::BlockArrowEnd
            }
            92..=95 => TokenKind::Link,
            96..=98 => {
                self.push_mode(Mode::LabelLink);
                TokenKind::StartLink
            }
            100 => {
                self.push_mode(Mode::String);
                TokenKind::LinkLabel
            }
            101..=103 => {
                self.pop_mode();
                TokenKind::Link
            }
            104 => TokenKind::Size,
            _ => return Ok(None),
        };
        Ok(Some(kind))
    }
}

/// The token text after the rule's action rewrote `yytext`.
fn token_text(rule: u16, matched: &str) -> String {
    match rule {
        6 => "-1".to_string(),
        7 => matched
            .find(|ch: char| ch.is_ascii_digit())
            .map_or_else(String::new, |index| matched[index..].to_string()),
        14 => matched
            .find(':')
            .map_or_else(String::new, |index| matched[index + 1..].to_string()),
        15 => "1".to_string(),
        85..=90 => match matched.strip_prefix(',') {
            Some(rest) => rest.trim_start_matches(is_js_whitespace).to_string(),
            None => matched.to_string(),
        },
        91 => "]>".to_string(),
        104 => matched[1..].to_string(),
        _ => matched.to_string(),
    }
}

fn is_word_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

/// `[^\(\[\n\-\)\{\}\s\<\>:=]`.
fn is_node_id_char(ch: char) -> bool {
    !matches!(
        ch,
        '(' | '[' | '\n' | ')' | '-' | '{' | '}' | '<' | '>' | ':' | '='
    ) && !is_js_whitespace(ch)
}

/// The literal patterns among the node-shape rules, in upstream's order.
fn node_literal(rule: u16) -> Option<&'static str> {
    Some(match rule {
        37 => "(((",
        38 => ")))",
        39 => "))",
        40 => "}}",
        41 => "}",
        42 => "(-",
        43 => "-)",
        44 => "((",
        45 => "]]",
        46 => "(",
        47 => "])",
        48 => "\\]",
        49 => "/]",
        50 => ")]",
        51 => ")",
        52 => "]>",
        53 => "]",
        _ => return None,
    })
}

/// `<ARROW_DIR>","?\s*(right|left|x|y|up|down)\s*`.
fn arrow_direction(rule: u16) -> Option<&'static str> {
    Some(match rule {
        85 => "right",
        86 => "left",
        87 => "x",
        88 => "y",
        89 => "up",
        90 => "down",
        _ => return None,
    })
}

/// `cleanupComments`: blank the text of every `%%` line that is not a `%%{` line, keeping
/// its newline and its byte count so that offsets and line numbers still address the source.
fn blank_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        let (line, newline) = match rest.find('\n') {
            Some(index) => (&rest[..index], 1),
            None => (rest, 0),
        };
        if is_comment_line(line) {
            out.extend(std::iter::repeat_n(' ', line.len()));
        } else {
            out.push_str(line);
        }
        if newline == 1 {
            out.push('\n');
        }
        rest = &rest[line.len() + newline..];
    }
    out
}

/// The `^\s*%%(?!{)[^\n]+` test, one line at a time.
fn is_comment_line(line: &str) -> bool {
    let content = line.trim_start_matches(is_js_whitespace);
    match content.strip_prefix("%%") {
        Some(rest) => !rest.is_empty() && !rest.starts_with('{'),
        None => false,
    }
}
