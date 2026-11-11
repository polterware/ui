//! The flowchart lexer: a hand port of the `%lex` section of upstream `parser/flow.jison`.
//!
//! The grammar uses exclusive start conditions, a stack of them (`begin`/`pushState` push,
//! `popState` pops), and jison's default lexing rule: the *first* matching rule of the current
//! condition wins, not the longest. Both are reproduced here, including the quirks jison's
//! regex translation introduced (for example `"\|"` matches a backslash and a pipe, so a bare
//! `|` reaches the `<*>` rule that opens the `text` state).

use crate::error::{ParseError, Span};

/// What follows the header's `graph`/`flowchart` keyword in a `direction` statement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Tb,
    Bt,
    Rl,
    Lr,
    Td,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    AccTitle,
    AccTitleValue,
    AccDescr,
    AccDescrValue,
    AccDescrMultilineValue,
    ShapeData,
    CallbackName,
    CallbackArgs,
    MdStr,
    Str,
    Style,
    Default,
    LinkStyle,
    Interpolate,
    ClassDef,
    Class,
    Href,
    Click,
    LinkTarget,
    Graph,
    Subgraph,
    End,
    Nodir,
    Dir,
    Direction(Direction),
    LinkId,
    Num,
    Brkt,
    StyleSeparator,
    Colon,
    Amp,
    Semi,
    Comma,
    Mult,
    Link,
    StartLink,
    EdgeText,
    NodeString,
    UnicodeText,
    Text,
    Pipe,
    Quote,
    Sqs,
    Sqe,
    Ps,
    Pe,
    DiamondStart,
    DiamondStop,
    StadiumStart,
    StadiumEnd,
    SubroutineStart,
    SubroutineEnd,
    VertexWithPropsStart,
    CylinderStart,
    CylinderEnd,
    DoubleCircleStart,
    DoubleCircleEnd,
    TrapStart,
    TrapEnd,
    InvTrapStart,
    InvTrapEnd,
    TagStart,
    TagEnd,
    Up,
    Sep,
    Down,
    Minus,
    EllipseStart,
    EllipseEnd,
    Newline,
    Space,
    Eof,
}

impl TokenKind {
    /// The terminal name upstream's parser reports in an error message.
    pub fn name(self) -> &'static str {
        match self {
            TokenKind::AccTitle => "acc_title",
            TokenKind::AccTitleValue => "acc_title_value",
            TokenKind::AccDescr => "acc_descr",
            TokenKind::AccDescrValue => "acc_descr_value",
            TokenKind::AccDescrMultilineValue => "acc_descr_multiline_value",
            TokenKind::ShapeData => "SHAPE_DATA",
            TokenKind::CallbackName => "CALLBACKNAME",
            TokenKind::CallbackArgs => "CALLBACKARGS",
            TokenKind::MdStr => "MD_STR",
            TokenKind::Str => "STR",
            TokenKind::Style => "STYLE",
            TokenKind::Default => "DEFAULT",
            TokenKind::LinkStyle => "LINKSTYLE",
            TokenKind::Interpolate => "INTERPOLATE",
            TokenKind::ClassDef => "CLASSDEF",
            TokenKind::Class => "CLASS",
            TokenKind::Href => "HREF",
            TokenKind::Click => "CLICK",
            TokenKind::LinkTarget => "LINK_TARGET",
            TokenKind::Graph => "GRAPH",
            TokenKind::Subgraph => "subgraph",
            TokenKind::End => "end",
            TokenKind::Nodir => "NODIR",
            TokenKind::Dir => "DIR",
            TokenKind::Direction(_) => "direction_tb",
            TokenKind::LinkId => "LINK_ID",
            TokenKind::Num => "NUM",
            TokenKind::Brkt => "BRKT",
            TokenKind::StyleSeparator => "STYLE_SEPARATOR",
            TokenKind::Colon => "COLON",
            TokenKind::Amp => "AMP",
            TokenKind::Semi => "SEMI",
            TokenKind::Comma => "COMMA",
            TokenKind::Mult => "MULT",
            TokenKind::Link => "LINK",
            TokenKind::StartLink => "START_LINK",
            TokenKind::EdgeText => "EDGE_TEXT",
            TokenKind::NodeString => "NODE_STRING",
            TokenKind::UnicodeText => "UNICODE_TEXT",
            TokenKind::Text => "TEXT",
            TokenKind::Pipe => "PIPE",
            TokenKind::Quote => "QUOTE",
            TokenKind::Sqs => "SQS",
            TokenKind::Sqe => "SQE",
            TokenKind::Ps => "PS",
            TokenKind::Pe => "PE",
            TokenKind::DiamondStart => "DIAMOND_START",
            TokenKind::DiamondStop => "DIAMOND_STOP",
            TokenKind::StadiumStart => "STADIUMSTART",
            TokenKind::StadiumEnd => "STADIUMEND",
            TokenKind::SubroutineStart => "SUBROUTINESTART",
            TokenKind::SubroutineEnd => "SUBROUTINEEND",
            TokenKind::VertexWithPropsStart => "VERTEX_WITH_PROPS_START",
            TokenKind::CylinderStart => "CYLINDERSTART",
            TokenKind::CylinderEnd => "CYLINDEREND",
            TokenKind::DoubleCircleStart => "DOUBLECIRCLESTART",
            TokenKind::DoubleCircleEnd => "DOUBLECIRCLEEND",
            TokenKind::TrapStart => "TRAPSTART",
            TokenKind::TrapEnd => "TRAPEND",
            TokenKind::InvTrapStart => "INVTRAPSTART",
            TokenKind::InvTrapEnd => "INVTRAPEND",
            TokenKind::TagStart => "TAGSTART",
            TokenKind::TagEnd => "TAGEND",
            TokenKind::Up => "UP",
            TokenKind::Sep => "SEP",
            TokenKind::Down => "DOWN",
            TokenKind::Minus => "MINUS",
            TokenKind::EllipseStart => "(-",
            TokenKind::EllipseEnd => "-)",
            TokenKind::Newline => "NEWLINE",
            TokenKind::Space => "SPACE",
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
    AccTitle,
    AccDescr,
    AccDescrMultiline,
    Dir,
    EdgeText,
    ThickEdgeText,
    DottedEdgeText,
    Click,
    CallbackName,
    CallbackArgs,
    ShapeData,
    ShapeDataStr,
    Text,
    EllipseText,
    TrapText,
}

/// The rules of each start condition, in jison's test order.
fn rules(mode: Mode) -> &'static [u16] {
    match mode {
        Mode::Initial => &[
            0, 2, 4, 7, 13, 21, 24, 25, 26, 27, 28, 29, 30, 31, 32, 35, 36, 37, 38, 39, 40, 41, 42,
            43, 44, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 73, 74, 76, 77,
            79, 82, 84, 86, 87, 88, 90, 92, 96, 97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107,
            108, 110, 112, 114, 116, 118, 119, 120, 121,
        ],
        Mode::String => &[
            21, 22, 23, 24, 79, 82, 84, 86, 90, 92, 96, 97, 110, 112, 114, 116,
        ],
        Mode::MdString => &[
            19, 20, 21, 24, 79, 82, 84, 86, 90, 92, 96, 97, 110, 112, 114, 116,
        ],
        Mode::AccTitle => &[
            1, 21, 24, 79, 82, 84, 86, 90, 92, 96, 97, 110, 112, 114, 116,
        ],
        Mode::AccDescr => &[
            3, 21, 24, 79, 82, 84, 86, 90, 92, 96, 97, 110, 112, 114, 116,
        ],
        Mode::AccDescrMultiline => &[
            5, 6, 21, 24, 79, 82, 84, 86, 90, 92, 96, 97, 110, 112, 114, 116,
        ],
        Mode::Dir => &[
            21, 24, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 79, 82, 84, 86, 90, 92, 96, 97,
            110, 112, 114, 116,
        ],
        Mode::EdgeText => &[
            21, 24, 70, 72, 79, 82, 84, 86, 90, 92, 96, 97, 110, 112, 114, 116,
        ],
        Mode::ThickEdgeText => &[
            21, 24, 73, 75, 79, 82, 84, 86, 90, 92, 96, 97, 110, 112, 114, 116,
        ],
        Mode::DottedEdgeText => &[
            21, 24, 76, 78, 79, 82, 84, 86, 90, 92, 96, 97, 110, 112, 114, 116,
        ],
        Mode::Click => &[
            21, 24, 33, 34, 79, 82, 84, 86, 90, 92, 96, 97, 110, 112, 114, 116,
        ],
        Mode::CallbackName => &[
            14, 15, 16, 21, 24, 79, 82, 84, 86, 90, 92, 96, 97, 110, 112, 114, 116,
        ],
        Mode::CallbackArgs => &[
            17, 18, 21, 24, 79, 82, 84, 86, 90, 92, 96, 97, 110, 112, 114, 116,
        ],
        Mode::ShapeData => &[
            8, 11, 12, 21, 24, 79, 82, 84, 86, 90, 92, 96, 97, 110, 112, 114, 116,
        ],
        Mode::ShapeDataStr => &[
            9, 10, 21, 24, 79, 82, 84, 86, 90, 92, 96, 97, 110, 112, 114, 116,
        ],
        Mode::Text => &[
            21, 24, 79, 82, 83, 84, 85, 86, 89, 90, 91, 92, 96, 97, 109, 110, 111, 112, 113, 114,
            115, 116, 117,
        ],
        Mode::EllipseText => &[
            21, 24, 79, 80, 81, 82, 84, 86, 90, 92, 96, 97, 110, 112, 114, 116,
        ],
        Mode::TrapText => &[
            21, 24, 79, 82, 84, 86, 90, 92, 93, 94, 95, 96, 97, 110, 112, 114, 116,
        ],
    }
}

pub struct Lexer<'a> {
    source: &'a str,
    text: String,
    /// For every byte of `text`, the byte it came from in `source`.
    origin: Vec<usize>,
    line_starts: Vec<usize>,
    pos: usize,
    modes: Vec<Mode>,
    first_graph: bool,
    zero_length_at: Option<usize>,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        let (text, origin) = clean_source(source);
        let mut line_starts = vec![0];
        for (index, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(index + 1);
            }
        }
        Self {
            source,
            text,
            origin,
            line_starts,
            pos: 0,
            modes: vec![Mode::Initial],
            first_graph: true,
            zero_length_at: None,
        }
    }

    /// Upstream `FlowDB.firstGraph`: true only for the first graph keyword.
    fn first_graph(&mut self) -> bool {
        if self.first_graph {
            self.first_graph = false;
            true
        } else {
            false
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
        self.text[index..].chars().next()
    }

    fn byte_at(&self, index: usize) -> Option<u8> {
        self.text.as_bytes().get(index).copied()
    }

    fn starts_with(&self, index: usize, needle: &str) -> bool {
        self.text[index..].starts_with(needle)
    }

    fn word_boundary(&self, index: usize) -> bool {
        match self.char_at(index) {
            Some(ch) => !(ch.is_ascii_alphanumeric() || ch == '_'),
            None => true,
        }
    }

    fn skip_whitespace(&self, mut index: usize) -> usize {
        while let Some(ch) = self.char_at(index) {
            if ch.is_whitespace() {
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

    fn span(&self, start: usize, end: usize) -> Span {
        let origin_start = self.origin.get(start).copied().unwrap_or(self.source.len());
        let origin_end = self
            .origin
            .get(end)
            .copied()
            .unwrap_or(self.source.len())
            .max(origin_start);
        let line = self
            .line_starts
            .partition_point(|&offset| offset <= origin_start) as u32;
        let line_start = self.line_starts[line.saturating_sub(1) as usize];
        let column = self.source[line_start..origin_start].chars().count() as u32 + 1;
        Span {
            line,
            column,
            start: origin_start,
            end: origin_end,
        }
    }

    pub fn next_token(&mut self) -> Result<Token, ParseError> {
        loop {
            let mode = self.mode();
            let mut found = None;
            for &rule in rules(mode) {
                if let Some(length) = self.match_rule(rule) {
                    found = Some((rule, length));
                    break;
                }
            }
            let Some((rule, length)) = found else {
                if self.pos >= self.text.len() {
                    let span = self.span(self.pos, self.pos);
                    return Ok(Token {
                        kind: TokenKind::Eof,
                        text: String::new(),
                        span,
                    });
                }
                let span = self.span(self.pos, self.pos);
                let ch = self
                    .char_at(self.pos)
                    .map_or("<eof>".to_string(), |ch| ch.to_string());
                return Err(ParseError::at(format!("unexpected character '{ch}'"), span));
            };
            if length == 0 && rule != 121 {
                if self.zero_length_at == Some(self.pos) {
                    let span = self.span(self.pos, self.pos);
                    return Err(ParseError::at("unexpected character", span));
                }
                self.zero_length_at = Some(self.pos);
            } else {
                self.zero_length_at = None;
            }
            let start = self.pos;
            self.pos += length;
            let text = self.text[start..self.pos].to_string();
            let span = self.span(start, self.pos);
            if let Some(kind) = self.action(rule, start)? {
                return Ok(Token {
                    kind,
                    text: token_text(rule, &text),
                    span,
                });
            }
        }
    }

    fn match_rule(&self, rule: u16) -> Option<usize> {
        let at = self.pos;
        let text = self.rest();
        match rule {
            0 => literal_prefix(text, "accTitle").and_then(|len| {
                let after = self.skip_whitespace(at + len);
                (self.byte_at(after) == Some(b':')).then(|| self.skip_whitespace(after + 1) - at)
            }),
            1 | 3 => Some(text.find('\n').unwrap_or(text.len())),
            2 => literal_prefix(text, "accDescr").and_then(|len| {
                let after = self.skip_whitespace(at + len);
                (self.byte_at(after) == Some(b':')).then(|| self.skip_whitespace(after + 1) - at)
            }),
            4 => literal_prefix(text, "accDescr").and_then(|len| {
                let after = self.skip_whitespace(at + len);
                (self.byte_at(after) == Some(b'{')).then(|| self.skip_whitespace(after + 1) - at)
            }),
            5 => self.single(at, '}'),
            6 => Some(self.run_while(at, |ch| ch != '}') - at),
            7 => self.starts_with(at, "@{").then_some(2),
            8 | 9 | 23 | 24 | 118 => self.single(at, '"'),
            10 => {
                let end = self.run_while(at, |ch| ch != '"');
                (end > at).then_some(end - at)
            }
            11 => {
                let end = self.run_while(at, |ch| ch != '}' && ch != '"');
                (end > at).then_some(end - at)
            }
            12 => self.single(at, '}'),
            13 => literal_prefix(text, "call").and_then(|len| {
                let end = self.run_while(at + len, |ch| ch.is_whitespace());
                (end > at + len).then_some(end - at)
            }),
            14 => {
                if self.byte_at(at) != Some(b'(') {
                    return None;
                }
                let after = self.skip_whitespace(at + 1);
                (self.byte_at(after) == Some(b')')).then_some(after + 1 - at)
            }
            15 | 112 => self.single(at, '('),
            16 => Some(self.run_while(at, |ch| ch != '(') - at),
            17 | 111 => self.single(at, ')'),
            18 => Some(self.run_while(at, |ch| ch != ')') - at),
            19 => {
                let end = self.run_while(at, |ch| ch != '`' && ch != '"');
                (end > at).then_some(end - at)
            }
            20 => self.starts_with(at, "`\"").then_some(2),
            21 => self.starts_with(at, "\"`").then_some(2),
            22 => {
                let end = self.run_while(at, |ch| ch != '"');
                (end > at).then_some(end - at)
            }
            25..=30 => {
                let keyword = match rule {
                    25 => "style",
                    26 => "default",
                    27 => "linkStyle",
                    28 => "interpolate",
                    29 => "classDef",
                    _ => "class",
                };
                literal_prefix(text, keyword).filter(|&len| self.word_boundary(at + len))
            }
            31 => literal_prefix(text, "href").and_then(|len| {
                self.char_at(at + len)
                    .filter(|ch| ch.is_whitespace())
                    .map(|ch| len + ch.len_utf8())
            }),
            32 => literal_prefix(text, "click").and_then(|len| {
                let end = self.run_while(at + len, |ch| ch.is_whitespace());
                (end > at + len).then_some(end - at)
            }),
            33 => self
                .char_at(at)
                .filter(|ch| ch.is_whitespace())
                .map(char::len_utf8),
            34 => Some(self.run_while(at, |ch| !ch.is_whitespace()) - at),
            35..=39 => {
                let keyword = match rule {
                    35 => "flowchart-elk",
                    36 => "swimlane-beta",
                    37 => "graph",
                    38 => "flowchart",
                    _ => "subgraph",
                };
                literal_prefix(text, keyword).filter(|&len| self.word_boundary(at + len))
            }
            40 => literal_prefix(text, "end")
                .filter(|&len| self.word_boundary(at + len))
                .map(|len| self.run_while(at + len, |ch| ch.is_whitespace()) - at),
            41..=44 => {
                let keyword = match rule {
                    41 => "_self",
                    42 => "_blank",
                    43 => "_parent",
                    _ => "_top",
                };
                literal_prefix(text, keyword).filter(|&len| self.word_boundary(at + len))
            }
            45 => {
                let mut index = at;
                let mut last_newline = None;
                while let Some(ch) = self.char_at(index) {
                    if !ch.is_whitespace() {
                        break;
                    }
                    if ch == '\n' {
                        last_newline = Some(index);
                    }
                    index += ch.len_utf8();
                }
                last_newline.map(|newline| newline + 1 - at)
            }
            46..=51 => {
                let keyword = match rule {
                    46 => "LR",
                    47 => "RL",
                    48 => "TB",
                    49 => "BT",
                    50 => "TD",
                    _ => "BR",
                };
                let after = self.skip_whitespace(at);
                literal_prefix(&self.text[after..], keyword)
                    .filter(|&len| self.word_boundary(after + len))
                    .map(|len| after + len - at)
            }
            52..=54 => {
                let expected = match rule {
                    52 => '<',
                    53 => '>',
                    _ => '^',
                };
                let after = self.skip_whitespace(at);
                (self.char_at(after) == Some(expected)).then_some(after + 1 - at)
            }
            55 => {
                let after = self.skip_whitespace(at);
                literal_prefix(&self.text[after..], "v")
                    .filter(|&len| self.word_boundary(after + len))
                    .map(|len| after + len - at)
            }
            56..=60 => {
                let keyword = match rule {
                    56 => "TB",
                    57 => "BT",
                    58 => "RL",
                    59 => "LR",
                    _ => "TD",
                };
                let line_end = text.find('\n').unwrap_or(text.len());
                let line = &text[..line_end];
                for (offset, _) in line.match_indices("direction") {
                    let after = offset + "direction".len();
                    let skipped = self.skip_whitespace(at + after) - at;
                    if skipped > after && skipped < line_end && line[skipped..].starts_with(keyword)
                    {
                        return Some(line_end);
                    }
                }
                None
            }
            61 => match_link_id(text),
            62 => {
                let end = self.run_while(at, |ch| ch.is_ascii_digit());
                (end > at).then_some(end - at)
            }
            63 | 104 => self.single(at, '#'),
            64 => self.starts_with(at, ":::").then_some(3),
            65 => self.single(at, ':'),
            66 | 105 => self.single(at, '&'),
            67 => self.single(at, ';'),
            68 => self.single(at, ','),
            69 | 103 => self.single(at, '*'),
            70 => match_dashed_link(self, at, b'-'),
            71 => match_dashed_start(self, at, b'-'),
            72 => match_edge_text_char(self, at, b'-'),
            73 => match_dashed_link(self, at, b'='),
            74 => match_dashed_start(self, at, b'='),
            75 => self.char_at(at).filter(|&ch| ch != '=').map(char::len_utf8),
            76 => match_dotted_link(self, at),
            77 => match_dotted_start(self, at),
            78 => self.char_at(at).filter(|&ch| ch != '.').map(char::len_utf8),
            79 => match_invisible_link(self, at),
            80 => match_ellipse_end(self, at),
            81 => {
                let ch = self.char_at(at)?;
                if !matches!(ch, '(' | ')' | '[' | ']' | '{' | '}') {
                    Some(ch.len_utf8())
                } else if ch == '!' {
                    let end = self.run_while(at + 1, |ch| ch == ')');
                    (end > at + 1).then_some(end - at)
                } else {
                    None
                }
            }
            82 => self.starts_with(at, "(-").then_some(2),
            83 => self.starts_with(at, "])").then_some(2),
            84 => self.starts_with(at, "([").then_some(2),
            85 => self.starts_with(at, "]]").then_some(2),
            86 => self.starts_with(at, "[[").then_some(2),
            87 => self.starts_with(at, "[|").then_some(2),
            88 | 99 => self.single(at, '>'),
            89 => self.starts_with(at, ")]").then_some(2),
            90 => self.starts_with(at, "[(").then_some(2),
            91 => self.starts_with(at, ")))").then_some(3),
            92 => self.starts_with(at, "(((").then_some(3),
            93 => {
                let ch = self.char_at(at)?;
                (matches!(ch, '\\' | '(' | '?' | '=' | ']' | ')')
                    && self.char_at(at + ch.len_utf8()) == Some(']'))
                .then(|| ch.len_utf8() + 1)
            }
            94 => self.starts_with(at, "/]").then_some(2),
            95 => {
                let ch = self.char_at(at)?;
                if ch == '/' || ch == '\\' {
                    (self.char_at(at + 1) != Some(']')).then_some(1)
                } else if matches!(ch, '[' | ']' | '(' | ')' | '{' | '}' | '/') {
                    None
                } else {
                    let end = self.run_while(at, |ch| {
                        !matches!(ch, '\\' | '[' | ']' | '(' | ')' | '{' | '}' | '/')
                    });
                    (end > at).then_some(end - at)
                }
            }
            96 => self.starts_with(at, "[/").then_some(2),
            97 => self.starts_with(at, "[\\").then_some(2),
            98 => self.single(at, '<'),
            100 => self.single(at, '^'),
            101 => self.starts_with(at, "\\|").then_some(2),
            102 => literal_prefix(text, "v").filter(|&len| self.word_boundary(at + len)),
            106 => match_node_string(text),
            107 => self.single(at, '-'),
            108 => self
                .char_at(at)
                .filter(|ch| !ch.is_ascii())
                .map(char::len_utf8),
            109 | 110 => self.single(at, '|'),
            113 => self.single(at, ']'),
            114 => self.single(at, '['),
            115 => self.single(at, '}'),
            116 => self.single(at, '{'),
            117 => {
                let end = self.run_while(at, |ch| {
                    !matches!(ch, '[' | ']' | '(' | ')' | '{' | '}' | '|' | '"')
                });
                (end > at).then_some(end - at)
            }
            119 => {
                let mut index = at;
                let mut count = 0;
                loop {
                    if self.starts_with(index, "\r\n") {
                        index += 2;
                        count += 1;
                    } else if self.byte_at(index) == Some(b'\n') {
                        index += 1;
                        count += 1;
                    } else {
                        break;
                    }
                }
                (count > 0).then_some(index - at)
            }
            120 => {
                let end = self.run_while(at, |ch| ch.is_whitespace() && ch != '\n' && ch != '\r');
                (end > at).then_some(end - at)
            }
            121 => (at == self.text.len()).then_some(0),
            _ => None,
        }
    }

    fn single(&self, at: usize, expected: char) -> Option<usize> {
        (self.char_at(at) == Some(expected)).then_some(expected.len_utf8())
    }

    fn action(&mut self, rule: u16, _start: usize) -> Result<Option<TokenKind>, ParseError> {
        let kind = match rule {
            0 => {
                self.push_mode(Mode::AccTitle);
                Some(TokenKind::AccTitle)
            }
            1 => {
                self.pop_mode();
                Some(TokenKind::AccTitleValue)
            }
            2 => {
                self.push_mode(Mode::AccDescr);
                Some(TokenKind::AccDescr)
            }
            3 => {
                self.pop_mode();
                Some(TokenKind::AccDescrValue)
            }
            4 => {
                self.push_mode(Mode::AccDescrMultiline);
                None
            }
            5 => {
                self.pop_mode();
                None
            }
            6 => Some(TokenKind::AccDescrMultilineValue),
            7 => {
                self.push_mode(Mode::ShapeData);
                Some(TokenKind::ShapeData)
            }
            8 => {
                self.push_mode(Mode::ShapeDataStr);
                Some(TokenKind::ShapeData)
            }
            9 => {
                self.pop_mode();
                Some(TokenKind::ShapeData)
            }
            10 | 11 => Some(TokenKind::ShapeData),
            12 => {
                self.pop_mode();
                None
            }
            13 => {
                self.push_mode(Mode::CallbackName);
                None
            }
            14 => {
                self.pop_mode();
                None
            }
            15 => {
                self.pop_mode();
                self.push_mode(Mode::CallbackArgs);
                None
            }
            16 => Some(TokenKind::CallbackName),
            17 => {
                self.pop_mode();
                None
            }
            18 => Some(TokenKind::CallbackArgs),
            19 => Some(TokenKind::MdStr),
            20 => {
                self.pop_mode();
                None
            }
            21 => {
                self.push_mode(Mode::MdString);
                None
            }
            22 => Some(TokenKind::Str),
            23 => {
                self.pop_mode();
                None
            }
            24 => {
                self.push_mode(Mode::String);
                None
            }
            25 => Some(TokenKind::Style),
            26 => Some(TokenKind::Default),
            27 => Some(TokenKind::LinkStyle),
            28 => Some(TokenKind::Interpolate),
            29 => Some(TokenKind::ClassDef),
            30 => Some(TokenKind::Class),
            31 => Some(TokenKind::Href),
            32 => {
                self.push_mode(Mode::Click);
                None
            }
            33 => {
                self.pop_mode();
                None
            }
            34 => Some(TokenKind::Click),
            35..=38 => {
                if self.first_graph() {
                    self.push_mode(Mode::Dir);
                }
                Some(TokenKind::Graph)
            }
            39 => Some(TokenKind::Subgraph),
            40 => Some(TokenKind::End),
            41..=44 => Some(TokenKind::LinkTarget),
            45 => {
                self.pop_mode();
                Some(TokenKind::Nodir)
            }
            46..=55 => {
                self.pop_mode();
                Some(TokenKind::Dir)
            }
            56..=60 => {
                let direction = match rule {
                    56 => Direction::Tb,
                    57 => Direction::Bt,
                    58 => Direction::Rl,
                    59 => Direction::Lr,
                    _ => Direction::Td,
                };
                Some(TokenKind::Direction(direction))
            }
            61 => Some(TokenKind::LinkId),
            62 => Some(TokenKind::Num),
            63 => Some(TokenKind::Brkt),
            64 => Some(TokenKind::StyleSeparator),
            65 => Some(TokenKind::Colon),
            66 => Some(TokenKind::Amp),
            67 => Some(TokenKind::Semi),
            68 => Some(TokenKind::Comma),
            69 => Some(TokenKind::Mult),
            70 => {
                self.pop_mode();
                Some(TokenKind::Link)
            }
            71 => {
                self.push_mode(Mode::EdgeText);
                Some(TokenKind::StartLink)
            }
            72 => Some(TokenKind::EdgeText),
            73 => {
                self.pop_mode();
                Some(TokenKind::Link)
            }
            74 => {
                self.push_mode(Mode::ThickEdgeText);
                Some(TokenKind::StartLink)
            }
            75 => Some(TokenKind::EdgeText),
            76 => {
                self.pop_mode();
                Some(TokenKind::Link)
            }
            77 => {
                self.push_mode(Mode::DottedEdgeText);
                Some(TokenKind::StartLink)
            }
            78 => Some(TokenKind::EdgeText),
            79 => {
                self.pop_mode();
                Some(TokenKind::Link)
            }
            80 => {
                self.pop_mode();
                Some(TokenKind::EllipseEnd)
            }
            81 => Some(TokenKind::Text),
            82 => {
                self.push_mode(Mode::EllipseText);
                Some(TokenKind::EllipseStart)
            }
            83 => {
                self.pop_mode();
                Some(TokenKind::StadiumEnd)
            }
            84 => {
                self.push_mode(Mode::Text);
                Some(TokenKind::StadiumStart)
            }
            85 => {
                self.pop_mode();
                Some(TokenKind::SubroutineEnd)
            }
            86 => {
                self.push_mode(Mode::Text);
                Some(TokenKind::SubroutineStart)
            }
            87 => Some(TokenKind::VertexWithPropsStart),
            88 => {
                self.push_mode(Mode::Text);
                Some(TokenKind::TagEnd)
            }
            89 => {
                self.pop_mode();
                Some(TokenKind::CylinderEnd)
            }
            90 => {
                self.push_mode(Mode::Text);
                Some(TokenKind::CylinderStart)
            }
            91 => {
                self.pop_mode();
                Some(TokenKind::DoubleCircleEnd)
            }
            92 => {
                self.push_mode(Mode::Text);
                Some(TokenKind::DoubleCircleStart)
            }
            93 => {
                self.pop_mode();
                Some(TokenKind::TrapEnd)
            }
            94 => {
                self.pop_mode();
                Some(TokenKind::InvTrapEnd)
            }
            95 => Some(TokenKind::Text),
            96 => {
                self.push_mode(Mode::TrapText);
                Some(TokenKind::TrapStart)
            }
            97 => {
                self.push_mode(Mode::TrapText);
                Some(TokenKind::InvTrapStart)
            }
            98 => Some(TokenKind::TagStart),
            99 => Some(TokenKind::TagEnd),
            100 => Some(TokenKind::Up),
            101 => Some(TokenKind::Sep),
            102 => Some(TokenKind::Down),
            103 => Some(TokenKind::Mult),
            104 => Some(TokenKind::Brkt),
            105 => Some(TokenKind::Amp),
            106 => Some(TokenKind::NodeString),
            107 => Some(TokenKind::Minus),
            108 => Some(TokenKind::UnicodeText),
            109 => {
                self.pop_mode();
                Some(TokenKind::Pipe)
            }
            110 => {
                self.push_mode(Mode::Text);
                Some(TokenKind::Pipe)
            }
            111 => {
                self.pop_mode();
                Some(TokenKind::Pe)
            }
            112 => {
                self.push_mode(Mode::Text);
                Some(TokenKind::Ps)
            }
            113 => {
                self.pop_mode();
                Some(TokenKind::Sqe)
            }
            114 => {
                self.push_mode(Mode::Text);
                Some(TokenKind::Sqs)
            }
            115 => {
                self.pop_mode();
                Some(TokenKind::DiamondStop)
            }
            116 => {
                self.push_mode(Mode::Text);
                Some(TokenKind::DiamondStart)
            }
            117 => Some(TokenKind::Text),
            118 => Some(TokenKind::Quote),
            119 => Some(TokenKind::Newline),
            120 => Some(TokenKind::Space),
            121 => Some(TokenKind::Eof),
            _ => return Err(ParseError::new("unhandled lexer rule", None)),
        };
        Ok(kind)
    }
}

/// The text a token carries: the matched text, except for the two rules that rewrite it.
fn token_text(rule: u16, text: &str) -> String {
    match rule {
        7 => String::new(),
        10 => collapse_newlines(text),
        _ => text.to_string(),
    }
}

/// Upstream's `yytext.replace(/\n\s*/g, '<br/>')` for quoted shape data.
fn collapse_newlines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\n' {
            out.push_str("<br/>");
            while chars.peek().is_some_and(|next| next.is_whitespace()) {
                chars.next();
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn literal_prefix(text: &str, literal: &str) -> Option<usize> {
    text.starts_with(literal).then_some(literal.len())
}

fn match_node_string(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut index = 0;
    while let Some(&byte) = bytes.get(index) {
        let take = match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' => true,
            b'!' | b'"' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+' | b'.' | b'`' | b'?'
            | b'\\' | b'_' | b'/' => true,
            b'-' => {
                matches!(bytes.get(index + 1), Some(next) if !matches!(next, b'>' | b'-' | b'.'))
            }
            _ => break,
        };
        if !take {
            break;
        }
        index += 1;
    }
    (index > 0).then_some(index)
}

fn match_link_id(text: &str) -> Option<usize> {
    let mut run_end = 0;
    let mut at_positions = Vec::new();
    for (offset, ch) in text.char_indices() {
        if ch.is_whitespace() || matches!(ch, '"' | '[' | ']' | '(' | ')' | '{' | '}' | '|') {
            break;
        }
        if ch == '@' && offset > 0 {
            at_positions.push(offset);
        }
        run_end = offset + ch.len_utf8();
    }
    let _ = run_end;
    for position in at_positions.into_iter().rev() {
        match text[position + 1..].chars().next() {
            Some('{') | Some('"') | None => continue,
            Some(_) => return Some(position + 1),
        }
    }
    None
}

fn match_dashed_link(lexer: &Lexer, at: usize, dash: u8) -> Option<usize> {
    let mut index = lexer.skip_whitespace(at);
    if lexer
        .char_at(index)
        .is_some_and(|ch| matches!(ch, 'x' | 'o' | '<'))
    {
        index += 1;
    }
    let start = index;
    while lexer.byte_at(index) == Some(dash) {
        index += 1;
    }
    let run = index - start;
    if run < 2 {
        return None;
    }
    let valid = |byte: u8| match dash {
        b'-' => matches!(byte, b'-' | b'x' | b'o' | b'>'),
        _ => matches!(byte, b'=' | b'x' | b'o' | b'>'),
    };
    // `--+` is greedy: it may keep every dash and let a following arrow maker be the last
    // character, or it may give its last dash to the `[-xo>]` class.
    let end = match lexer.byte_at(index) {
        Some(next) if valid(next) => index + 1,
        _ if run >= 3 => index,
        _ => return None,
    };
    Some(lexer.skip_whitespace(end) - at)
}

fn match_dashed_start(lexer: &Lexer, at: usize, dash: u8) -> Option<usize> {
    let mut index = lexer.skip_whitespace(at);
    if lexer
        .char_at(index)
        .is_some_and(|ch| matches!(ch, 'x' | 'o' | '<'))
    {
        index += 1;
    }
    if lexer.byte_at(index) != Some(dash) || lexer.byte_at(index + 1) != Some(dash) {
        return None;
    }
    Some(lexer.skip_whitespace(index + 2) - at)
}

fn match_edge_text_char(lexer: &Lexer, at: usize, dash: u8) -> Option<usize> {
    let ch = lexer.char_at(at)?;
    if ch as u32 > 0x7f {
        return Some(ch.len_utf8());
    }
    if ch as u8 == dash {
        (lexer.byte_at(at + 1) != Some(dash)).then_some(1)
    } else {
        Some(1)
    }
}

fn match_dotted_link(lexer: &Lexer, at: usize) -> Option<usize> {
    let mut index = lexer.skip_whitespace(at);
    if lexer
        .char_at(index)
        .is_some_and(|ch| matches!(ch, 'x' | 'o' | '<'))
    {
        index += 1;
    }
    if lexer.byte_at(index) == Some(b'-') {
        index += 1;
    }
    let dots = index;
    while lexer.byte_at(index) == Some(b'.') {
        index += 1;
    }
    if index == dots || lexer.byte_at(index) != Some(b'-') {
        return None;
    }
    index += 1;
    if lexer
        .char_at(index)
        .is_some_and(|ch| matches!(ch, 'x' | 'o' | '>'))
    {
        index += 1;
    }
    Some(lexer.skip_whitespace(index) - at)
}

fn match_dotted_start(lexer: &Lexer, at: usize) -> Option<usize> {
    let mut index = lexer.skip_whitespace(at);
    if lexer
        .char_at(index)
        .is_some_and(|ch| matches!(ch, 'x' | 'o' | '<'))
    {
        index += 1;
    }
    if !lexer.starts_with(index, "-.") {
        return None;
    }
    Some(lexer.skip_whitespace(index + 2) - at)
}

fn match_invisible_link(lexer: &Lexer, at: usize) -> Option<usize> {
    let mut index = lexer.skip_whitespace(at);
    if !lexer.starts_with(index, "~~") {
        return None;
    }
    index += 2;
    let start = index;
    while lexer.byte_at(index) == Some(b'~') {
        index += 1;
    }
    if index == start {
        return None;
    }
    Some(lexer.skip_whitespace(index) - at)
}

fn match_ellipse_end(lexer: &Lexer, at: usize) -> Option<usize> {
    let ch = lexer.char_at(at)?;
    (matches!(ch, '-' | '/' | ')') && lexer.char_at(at + 1) == Some(')')).then_some(2)
}

/// Upstream's parser wrapper removes whitespace after a closing brace at the end of a line
/// (`flowParser.ts`: `src.replace(/}\s*\n/g, '}\n')`). The lexer sees the cleaned text so the
/// shape-data rules behave; [`Lexer::span`] maps positions back to the source.
fn clean_source(source: &str) -> (String, Vec<usize>) {
    let mut text = String::with_capacity(source.len());
    let mut origin = Vec::with_capacity(source.len());
    let mut index = 0;
    while index < source.len() {
        let ch = source[index..]
            .chars()
            .next()
            .expect("index is on a char boundary");
        let length = ch.len_utf8();
        text.push(ch);
        for offset in 0..length {
            origin.push(index + offset);
        }
        index += length;
        if ch == '}' {
            let mut cursor = index;
            let mut last_newline = None;
            while let Some(next) = source[cursor..].chars().next() {
                if !next.is_whitespace() {
                    break;
                }
                if next == '\n' {
                    last_newline = Some(cursor);
                }
                cursor += next.len_utf8();
            }
            if let Some(newline) = last_newline {
                text.push('\n');
                origin.push(newline);
                index = newline + 1;
            }
        }
    }
    (text, origin)
}
