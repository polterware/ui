//! The agentflow lexer: a hand port of the `%lex` section of upstream
//! `parser/agentflow.jison`, which is itself a near-copy of flowchart's lexer.
//!
//! The grammar uses exclusive start conditions with a stack (`%x`, `begin`, `pushState`,
//! `popState`), and jison's default rule choice is the *first* matching rule of the current
//! condition, not the longest. Both are reproduced here. The divergences the upstream header
//! documents (keyword vocabulary, the narrowed `--` family, no thick/dotted arrow states, and
//! the added `%%` rules in label states) are those of the grammar itself.
//!
//! Positions map straight back to the source: unlike flowchart's wrapper, agentflow's wrapper
//! only strips horizontal whitespace after a closing `}`, which this parser does not need
//! because it reads separators explicitly. Spans therefore stay in source space and the
//! blank-line fold of issue #56 cannot occur.

use crate::error::{ParseError, Span};

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
    Flow,
    Connector,
    Global,
    End,
    Nodir,
    Dir,
    DirectionTb,
    DirectionBt,
    DirectionRl,
    DirectionLr,
    DirectionTd,
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
    VertexWithPropsStart,
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
    Comment,
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
            TokenKind::Flow => "flow",
            TokenKind::Connector => "connector",
            TokenKind::Global => "global",
            TokenKind::End => "end",
            TokenKind::Nodir => "NODIR",
            TokenKind::Dir => "DIR",
            TokenKind::DirectionTb => "direction_tb",
            TokenKind::DirectionBt => "direction_bt",
            TokenKind::DirectionRl => "direction_rl",
            TokenKind::DirectionLr => "direction_lr",
            TokenKind::DirectionTd => "direction_td",
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
            TokenKind::VertexWithPropsStart => "VERTEX_WITH_PROPS_START",
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
            TokenKind::Comment => "COMMENT",
            TokenKind::Eof => "EOF",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub span: Span,
    /// The 1-based line of the token's last byte.
    pub end_line: u32,
    /// The 0-based, exclusive column of the token's last byte.
    pub end_column: u32,
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
fn rules(mode: Mode) -> &'static [Rule] {
    use Rule::*;
    match mode {
        Mode::Initial => &[
            AccTitleStart,
            AccDescrStart,
            AccDescrMultilineStart,
            ShapeDataStart,
            CallStart,
            MdStringStart,
            StringStart,
            Style,
            Default,
            LinkStyle,
            Interpolate,
            ClassDef,
            Class,
            Href,
            ClickStart,
            Graph,
            FlowKeyword,
            ConnectorKeyword,
            GlobalKeyword,
            EndKeyword,
            LinkSelf,
            LinkBlank,
            LinkParent,
            LinkTop,
            DirectionTb,
            DirectionBt,
            DirectionRl,
            DirectionLr,
            DirectionTd,
            LinkId,
            Num,
            Brkt,
            StyleSeparator,
            Colon,
            Amp,
            Semi,
            Comma,
            Mult,
            LinkArrow,
            StartLink,
            LinkDotted,
            EllipseStart,
            StadiumStart,
            SubroutineStart,
            VertexWithPropsStart,
            TagEndPush,
            CylinderStart,
            DoubleCircleStart,
            TrapStart,
            InvTrapStart,
            TagStart,
            Up,
            Sep,
            Down,
            Comment,
            NodeString,
            Minus,
            Unicode,
            PipeOpen,
            Ps,
            Sqs,
            DiamondStart,
            Quote,
            Newline,
            Space,
            Eof,
        ],
        Mode::String => &[
            MdStringStart,
            StringText,
            StringEnd,
            StringStart,
            Quote,
            Newline,
            Space,
            Eof,
        ],
        Mode::MdString => &[
            MdStringText,
            MdStringEnd,
            MdStringStart,
            StringStart,
            Quote,
            Newline,
            Space,
            Eof,
        ],
        Mode::AccTitle => &[
            AccTitleValue,
            MdStringStart,
            StringStart,
            Quote,
            Newline,
            Space,
            Eof,
        ],
        Mode::AccDescr => &[
            AccDescrValue,
            MdStringStart,
            StringStart,
            Quote,
            Newline,
            Space,
            Eof,
        ],
        Mode::AccDescrMultiline => &[
            AccDescrMultilineClose,
            AccDescrMultilineText,
            MdStringStart,
            StringStart,
            Quote,
            Newline,
            Space,
            Eof,
        ],
        Mode::Dir => &[
            MdStringStart,
            StringStart,
            DirNodir,
            DirLr,
            DirRl,
            DirTb,
            DirBt,
            DirTd,
            DirBr,
            DirLt,
            DirGt,
            DirUp,
            DirDown,
            Quote,
            Newline,
            Space,
            Eof,
        ],
        Mode::EdgeText => &[
            MdStringStart,
            StringStart,
            LinkArrow,
            EdgeTextChar,
            Quote,
            Newline,
            Space,
            Eof,
        ],
        Mode::Click => &[
            MdStringStart,
            StringStart,
            ClickEnd,
            ClickId,
            Quote,
            Newline,
            Space,
            Eof,
        ],
        Mode::CallbackName => &[
            CallbackEmptyEnd,
            CallbackArgsStart,
            CallbackNameText,
            MdStringStart,
            StringStart,
            Quote,
            Newline,
            Space,
            Eof,
        ],
        Mode::CallbackArgs => &[
            CallbackArgsEnd,
            CallbackArgsText,
            MdStringStart,
            StringStart,
            Quote,
            Newline,
            Space,
            Eof,
        ],
        Mode::ShapeData => &[
            ShapeDataQuoteStart,
            ShapeDataText,
            ShapeDataClose,
            MdStringStart,
            StringStart,
            EllipseStart,
            StadiumStart,
            SubroutineStart,
            CylinderStart,
            DoubleCircleStart,
            TrapStart,
            InvTrapStart,
            PipeOpen,
            Ps,
            Sqs,
            DiamondStart,
            Quote,
            Newline,
            Space,
            Eof,
        ],
        Mode::ShapeDataStr => &[
            ShapeDataQuoteEnd,
            ShapeDataStrText,
            MdStringStart,
            StringStart,
            Quote,
            Newline,
            Space,
            Eof,
        ],
        Mode::Text => &[
            MdStringStart,
            StringStart,
            EllipseStart,
            StadiumEnd,
            StadiumStart,
            SubroutineEnd,
            SubroutineStart,
            CylinderEnd,
            CylinderStart,
            DoubleCircleEnd,
            DoubleCircleStart,
            TrapStart,
            InvTrapStart,
            TextPipe,
            PipeOpen,
            TextPe,
            Ps,
            TextSqe,
            Sqs,
            TextDiamondStop,
            DiamondStart,
            TextComment,
            Text,
            Quote,
            Newline,
            Space,
            Eof,
        ],
        Mode::EllipseText => &[
            MdStringStart,
            StringStart,
            EllipseComment,
            EllipseEnd,
            EllipseTextChar,
            EllipseStart,
            StadiumStart,
            SubroutineStart,
            CylinderStart,
            DoubleCircleStart,
            TrapStart,
            InvTrapStart,
            PipeOpen,
            Ps,
            Sqs,
            DiamondStart,
            Quote,
            Newline,
            Space,
            Eof,
        ],
        Mode::TrapText => &[
            MdStringStart,
            StringStart,
            TrapComment,
            TrapEnd,
            InvTrapEnd,
            TrapTextChar,
            EllipseStart,
            StadiumStart,
            SubroutineStart,
            CylinderStart,
            DoubleCircleStart,
            TrapStart,
            InvTrapStart,
            PipeOpen,
            Ps,
            Sqs,
            DiamondStart,
            Quote,
            Newline,
            Space,
            Eof,
        ],
    }
}

/// One `%lex` rule. Names follow the token the rule returns, or its action when it returns
/// none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rule {
    AccTitleStart,
    AccTitleValue,
    AccDescrStart,
    AccDescrValue,
    AccDescrMultilineStart,
    AccDescrMultilineClose,
    AccDescrMultilineText,
    ShapeDataStart,
    ShapeDataQuoteStart,
    ShapeDataQuoteEnd,
    ShapeDataStrText,
    ShapeDataText,
    ShapeDataClose,
    CallStart,
    CallbackEmptyEnd,
    CallbackArgsStart,
    CallbackNameText,
    CallbackArgsEnd,
    CallbackArgsText,
    MdStringText,
    MdStringEnd,
    MdStringStart,
    StringText,
    StringEnd,
    StringStart,
    Style,
    Default,
    LinkStyle,
    Interpolate,
    ClassDef,
    Class,
    Href,
    ClickStart,
    ClickEnd,
    ClickId,
    Graph,
    FlowKeyword,
    ConnectorKeyword,
    GlobalKeyword,
    EndKeyword,
    LinkSelf,
    LinkBlank,
    LinkParent,
    LinkTop,
    DirNodir,
    DirLr,
    DirRl,
    DirTb,
    DirBt,
    DirTd,
    DirBr,
    DirLt,
    DirGt,
    DirUp,
    DirDown,
    DirectionTb,
    DirectionBt,
    DirectionRl,
    DirectionLr,
    DirectionTd,
    LinkId,
    Num,
    Brkt,
    StyleSeparator,
    Colon,
    Amp,
    Semi,
    Comma,
    Mult,
    LinkArrow,
    StartLink,
    EdgeTextChar,
    LinkDotted,
    EllipseComment,
    EllipseEnd,
    EllipseTextChar,
    EllipseStart,
    StadiumEnd,
    StadiumStart,
    SubroutineEnd,
    SubroutineStart,
    VertexWithPropsStart,
    TagEndPush,
    CylinderEnd,
    CylinderStart,
    DoubleCircleEnd,
    DoubleCircleStart,
    TrapComment,
    TrapEnd,
    InvTrapEnd,
    TrapTextChar,
    TrapStart,
    InvTrapStart,
    TagStart,
    Up,
    Sep,
    Down,
    Comment,
    NodeString,
    Minus,
    Unicode,
    TextPipe,
    PipeOpen,
    TextPe,
    Ps,
    TextSqe,
    Sqs,
    TextDiamondStop,
    DiamondStart,
    TextComment,
    Text,
    Quote,
    Newline,
    Space,
    Eof,
}

pub struct Lexer<'a> {
    source: &'a str,
    line_starts: Vec<usize>,
    pos: usize,
    modes: Vec<Mode>,
    first_graph: bool,
    zero_length_at: Option<usize>,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        let mut line_starts = vec![0];
        for (index, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(index + 1);
            }
        }
        Self {
            source,
            line_starts,
            pos: 0,
            modes: vec![Mode::Initial],
            first_graph: true,
            zero_length_at: None,
        }
    }

    /// Upstream `AgentFlowDB.firstGraph`: true only for the first graph keyword.
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
        &self.source[self.pos..]
    }

    fn char_at(&self, index: usize) -> Option<char> {
        self.source[index..].chars().next()
    }

    fn byte_at(&self, index: usize) -> Option<u8> {
        self.source.as_bytes().get(index).copied()
    }

    fn starts_with(&self, index: usize, needle: &str) -> bool {
        self.source[index..].starts_with(needle)
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

    fn position(&self, index: usize) -> (u32, u32) {
        let line = self.line_starts.partition_point(|&offset| offset <= index) as u32;
        let line_start = self.line_starts[line.saturating_sub(1) as usize];
        let column = self.source[line_start..index].chars().count() as u32 + 1;
        (line, column)
    }

    fn span(&self, start: usize, end: usize) -> Span {
        let (line, column) = self.position(start);
        Span {
            line,
            column,
            start,
            end: end.max(start),
        }
    }

    pub fn next_token(&mut self) -> Result<Token, ParseError> {
        loop {
            let mode = self.mode();
            // Jison takes the first matching rule of the condition, not the longest.
            let mut found = None;
            for &rule in rules(mode) {
                if let Some(length) = self.match_rule(rule) {
                    found = Some((rule, length));
                    break;
                }
            }
            let Some((rule, length)) = found else {
                if self.pos >= self.source.len() {
                    let span = self.span(self.pos, self.pos);
                    let (line, column) = self.position(self.pos);
                    return Ok(Token {
                        kind: TokenKind::Eof,
                        text: String::new(),
                        span,
                        end_line: line,
                        end_column: column.saturating_sub(1),
                    });
                }
                let span = self.span(self.pos, self.pos);
                let ch = self
                    .char_at(self.pos)
                    .map_or("<eof>".to_string(), |ch| ch.to_string());
                return Err(ParseError::at(format!("unexpected character '{ch}'"), span));
            };
            if length == 0 && rule != Rule::Eof {
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
            let text = self.source[start..self.pos].to_string();
            let span = self.span(start, self.pos);
            if let Some(kind) = self.action(rule)? {
                let (end_line, end_column) = self.position(self.pos);
                return Ok(Token {
                    kind,
                    text: token_text(rule, &text),
                    span,
                    end_line,
                    end_column: end_column.saturating_sub(1),
                });
            }
        }
    }

    fn match_rule(&self, rule: Rule) -> Option<usize> {
        use Rule::*;
        let at = self.pos;
        let text = self.rest();
        match rule {
            AccTitleStart => literal_prefix(text, "accTitle").and_then(|len| {
                let after = self.skip_whitespace(at + len);
                (self.byte_at(after) == Some(b':')).then(|| self.skip_whitespace(after + 1) - at)
            }),
            AccTitleValue | AccDescrValue => Some(text.find('\n').unwrap_or(text.len())),
            AccDescrStart => literal_prefix(text, "accDescr").and_then(|len| {
                let after = self.skip_whitespace(at + len);
                (self.byte_at(after) == Some(b':')).then(|| self.skip_whitespace(after + 1) - at)
            }),
            AccDescrMultilineStart => literal_prefix(text, "accDescr").and_then(|len| {
                let after = self.skip_whitespace(at + len);
                (self.byte_at(after) == Some(b'{')).then(|| self.skip_whitespace(after + 1) - at)
            }),
            AccDescrMultilineClose => self.single(at, '}'),
            AccDescrMultilineText => Some(self.run_while(at, |ch| ch != '}') - at),
            ShapeDataStart => self.starts_with(at, "@{").then_some(2),
            ShapeDataQuoteStart | ShapeDataQuoteEnd => self.single(at, '"'),
            ShapeDataStrText => {
                let end = self.run_while(at, |ch| ch != '"');
                (end > at).then_some(end - at)
            }
            ShapeDataText => {
                let end = self.run_while(at, |ch| ch != '}' && ch != '"' && ch != '^');
                (end > at).then_some(end - at)
            }
            ShapeDataClose => self.single(at, '}'),
            CallStart => literal_prefix(text, "call").and_then(|len| {
                let end = self.run_while(at + len, |ch| ch.is_whitespace());
                (end > at + len).then_some(end - at)
            }),
            CallbackEmptyEnd => {
                if self.byte_at(at) != Some(b'(') {
                    return None;
                }
                let after = self.skip_whitespace(at + 1);
                (self.byte_at(after) == Some(b')')).then_some(after + 1 - at)
            }
            CallbackArgsStart | Ps => self.single(at, '('),
            CallbackNameText => Some(self.run_while(at, |ch| ch != '(') - at),
            CallbackArgsEnd | TextPe => self.single(at, ')'),
            CallbackArgsText => Some(self.run_while(at, |ch| ch != ')') - at),
            MdStringText => {
                let end = self.run_while(at, |ch| ch != '`' && ch != '"');
                (end > at).then_some(end - at)
            }
            MdStringEnd => self.starts_with(at, "`\"").then_some(2),
            MdStringStart => self.starts_with(at, "\"`").then_some(2),
            StringText => {
                let end = self.run_while(at, |ch| ch != '"');
                (end > at).then_some(end - at)
            }
            StringEnd => self.single(at, '"'),
            StringStart => self.single(at, '"'),
            Style => literal_keyword(text, "style"),
            Default => literal_keyword(text, "default"),
            LinkStyle => literal_keyword(text, "linkStyle"),
            Interpolate => literal_keyword(text, "interpolate"),
            ClassDef => literal_keyword(text, "classDef"),
            Class => literal_keyword(text, "class"),
            Href => literal_prefix(text, "href").and_then(|len| {
                self.char_at(at + len)
                    .filter(|ch| ch.is_whitespace())
                    .map(|ch| len + ch.len_utf8())
            }),
            ClickStart => literal_prefix(text, "click").and_then(|len| {
                let end = self.run_while(at + len, |ch| ch.is_whitespace());
                (end > at + len).then_some(end - at)
            }),
            ClickEnd => self
                .char_at(at)
                .filter(|ch| ch.is_whitespace())
                .map(char::len_utf8),
            ClickId => Some(self.run_while(at, |ch| !ch.is_whitespace()) - at),
            Graph => literal_keyword(text, "agentflow-beta"),
            FlowKeyword => literal_keyword(text, "flow"),
            ConnectorKeyword => literal_keyword(text, "connector"),
            GlobalKeyword => literal_prefix(text, "global")
                .filter(|&len| self.word_boundary(at + len))
                .map(|len| self.run_while(at + len, |ch| matches!(ch, ' ' | '\t')) - at),
            EndKeyword => literal_prefix(text, "end")
                .filter(|&len| self.word_boundary(at + len))
                .map(|len| self.run_while(at + len, |ch| matches!(ch, ' ' | '\t')) - at),
            LinkSelf => literal_keyword(text, "_self"),
            LinkBlank => literal_keyword(text, "_blank"),
            LinkParent => literal_keyword(text, "_parent"),
            LinkTop => literal_keyword(text, "_top"),
            DirNodir => {
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
            DirLr | DirRl | DirTb | DirBt | DirTd | DirBr => {
                let keyword = match rule {
                    DirLr => "LR",
                    DirRl => "RL",
                    DirTb => "TB",
                    DirBt => "BT",
                    DirTd => "TD",
                    _ => "BR",
                };
                let after = self.skip_whitespace(at);
                literal_prefix(&self.source[after..], keyword)
                    .filter(|&len| self.word_boundary(after + len))
                    .map(|len| after + len - at)
            }
            DirLt | DirGt | DirUp => {
                let expected = match rule {
                    DirLt => '<',
                    DirGt => '>',
                    _ => '^',
                };
                let after = self.skip_whitespace(at);
                (self.char_at(after) == Some(expected)).then_some(after + 1 - at)
            }
            DirDown => {
                let after = self.skip_whitespace(at);
                literal_prefix(&self.source[after..], "v")
                    .filter(|&len| self.word_boundary(after + len))
                    .map(|len| after + len - at)
            }
            DirectionTb | DirectionBt | DirectionRl | DirectionLr | DirectionTd => {
                let keyword = match rule {
                    DirectionTb => "TB",
                    DirectionBt => "BT",
                    DirectionRl => "RL",
                    DirectionLr => "LR",
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
            LinkId => match_link_id(text),
            Num => {
                let end = self.run_while(at, |ch| ch.is_ascii_digit());
                (end > at).then_some(end - at)
            }
            Brkt => self.single(at, '#'),
            StyleSeparator => self.starts_with(at, ":::").then_some(3),
            Colon => self.single(at, ':'),
            Amp => self.single(at, '&'),
            Semi => self.single(at, ';'),
            Comma => self.single(at, ','),
            Mult => self.single(at, '*'),
            LinkArrow => match_dashed_link(self, at),
            StartLink => match_dashed_start(self, at),
            EdgeTextChar => match_edge_text_char(self, at),
            LinkDotted => match_dotted_link(self, at),
            EllipseComment | TrapComment | Comment | TextComment => match_comment(self, at, rule),
            EllipseEnd => match_ellipse_end(self, at),
            EllipseTextChar | Text => match_text_char(self, at, rule),
            EllipseStart => self.starts_with(at, "(-").then_some(2),
            StadiumEnd => self.starts_with(at, "])").then_some(2),
            StadiumStart => self.starts_with(at, "([").then_some(2),
            SubroutineEnd => self.starts_with(at, "]]").then_some(2),
            SubroutineStart => self.starts_with(at, "[[").then_some(2),
            VertexWithPropsStart => self.starts_with(at, "[|").then_some(2),
            TagEndPush => self.single(at, '>'),
            CylinderEnd => self.starts_with(at, ")]").then_some(2),
            CylinderStart => self.starts_with(at, "[(").then_some(2),
            DoubleCircleEnd => self.starts_with(at, ")))").then_some(3),
            DoubleCircleStart => self.starts_with(at, "(((").then_some(3),
            TrapEnd => {
                let ch = self.char_at(at)?;
                (matches!(ch, '\\' | '(' | '?' | '=' | ']' | ')')
                    && self.char_at(at + ch.len_utf8()) == Some(']'))
                .then(|| ch.len_utf8() + 1)
            }
            InvTrapEnd => self.starts_with(at, "/]").then_some(2),
            TrapTextChar => {
                let ch = self.char_at(at)?;
                if ch == '/' || ch == '\\' {
                    (self.char_at(at + 1) != Some(']')).then_some(1)
                } else if matches!(ch, '[' | ']' | '(' | ')' | '{' | '}' | '/' | '%') {
                    if ch == '%' && self.char_at(at + 1) != Some('%') {
                        Some(1)
                    } else {
                        None
                    }
                } else {
                    let end = self.run_while(at, |ch| {
                        !matches!(ch, '\\' | '[' | ']' | '(' | ')' | '{' | '}' | '/' | '%')
                    });
                    (end > at).then_some(end - at)
                }
            }
            TrapStart => self.starts_with(at, "[/").then_some(2),
            InvTrapStart => self.starts_with(at, "[\\").then_some(2),
            TagStart => self.single(at, '<'),
            Up => self.single(at, '^'),
            Sep => self.starts_with(at, "\\|").then_some(2),
            Down => literal_keyword(text, "v"),
            NodeString => match_node_string(text),
            Minus => self.single(at, '-'),
            Unicode => self
                .char_at(at)
                .filter(|ch| !ch.is_ascii())
                .map(char::len_utf8),
            TextPipe => self.single(at, '|'),
            PipeOpen => self.single(at, '|'),
            TextSqe => self.single(at, ']'),
            Sqs => self.single(at, '['),
            TextDiamondStop => self.single(at, '}'),
            DiamondStart => self.single(at, '{'),
            Quote => self.single(at, '"'),
            Newline => {
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
            Space => {
                let end = self.run_while(at, |ch| ch.is_whitespace() && ch != '\n' && ch != '\r');
                (end > at).then_some(end - at)
            }
            Eof => (at == self.source.len()).then_some(0),
        }
    }

    fn single(&self, at: usize, expected: char) -> Option<usize> {
        (self.char_at(at) == Some(expected)).then_some(expected.len_utf8())
    }

    fn action(&mut self, rule: Rule) -> Result<Option<TokenKind>, ParseError> {
        use Rule::*;
        let kind = match rule {
            AccTitleStart => {
                self.push_mode(Mode::AccTitle);
                Some(TokenKind::AccTitle)
            }
            AccTitleValue => {
                self.pop_mode();
                Some(TokenKind::AccTitleValue)
            }
            AccDescrStart => {
                self.push_mode(Mode::AccDescr);
                Some(TokenKind::AccDescr)
            }
            AccDescrValue => {
                self.pop_mode();
                Some(TokenKind::AccDescrValue)
            }
            AccDescrMultilineStart => {
                self.push_mode(Mode::AccDescrMultiline);
                None
            }
            AccDescrMultilineClose => {
                self.pop_mode();
                None
            }
            AccDescrMultilineText => Some(TokenKind::AccDescrMultilineValue),
            ShapeDataStart => {
                self.push_mode(Mode::ShapeData);
                Some(TokenKind::ShapeData)
            }
            ShapeDataQuoteStart => {
                self.push_mode(Mode::ShapeDataStr);
                Some(TokenKind::ShapeData)
            }
            ShapeDataQuoteEnd => {
                self.pop_mode();
                Some(TokenKind::ShapeData)
            }
            ShapeDataStrText | ShapeDataText => Some(TokenKind::ShapeData),
            ShapeDataClose => {
                self.pop_mode();
                None
            }
            CallStart => {
                self.push_mode(Mode::CallbackName);
                None
            }
            CallbackEmptyEnd => {
                self.pop_mode();
                None
            }
            CallbackArgsStart => {
                self.pop_mode();
                self.push_mode(Mode::CallbackArgs);
                None
            }
            CallbackNameText => Some(TokenKind::CallbackName),
            CallbackArgsEnd => {
                self.pop_mode();
                None
            }
            CallbackArgsText => Some(TokenKind::CallbackArgs),
            MdStringText => Some(TokenKind::MdStr),
            MdStringEnd => {
                self.pop_mode();
                None
            }
            MdStringStart => {
                self.push_mode(Mode::MdString);
                None
            }
            StringText => Some(TokenKind::Str),
            StringEnd => {
                self.pop_mode();
                None
            }
            StringStart => {
                self.push_mode(Mode::String);
                None
            }
            Style => Some(TokenKind::Style),
            Default => Some(TokenKind::Default),
            LinkStyle => Some(TokenKind::LinkStyle),
            Interpolate => Some(TokenKind::Interpolate),
            ClassDef => Some(TokenKind::ClassDef),
            Class => Some(TokenKind::Class),
            Href => Some(TokenKind::Href),
            ClickStart => {
                self.push_mode(Mode::Click);
                None
            }
            ClickEnd => {
                self.pop_mode();
                None
            }
            ClickId => Some(TokenKind::Click),
            Graph => {
                if self.first_graph() {
                    self.push_mode(Mode::Dir);
                }
                Some(TokenKind::Graph)
            }
            FlowKeyword => Some(TokenKind::Flow),
            ConnectorKeyword => Some(TokenKind::Connector),
            GlobalKeyword => Some(TokenKind::Global),
            EndKeyword => Some(TokenKind::End),
            LinkSelf | LinkBlank | LinkParent | LinkTop => Some(TokenKind::LinkTarget),
            DirNodir => {
                self.pop_mode();
                Some(TokenKind::Nodir)
            }
            DirLr | DirRl | DirTb | DirBt | DirTd | DirBr | DirLt | DirGt | DirUp | DirDown => {
                self.pop_mode();
                Some(TokenKind::Dir)
            }
            DirectionTb => Some(TokenKind::DirectionTb),
            DirectionBt => Some(TokenKind::DirectionBt),
            DirectionRl => Some(TokenKind::DirectionRl),
            DirectionLr => Some(TokenKind::DirectionLr),
            DirectionTd => Some(TokenKind::DirectionTd),
            LinkId => Some(TokenKind::LinkId),
            Num => Some(TokenKind::Num),
            Brkt => Some(TokenKind::Brkt),
            StyleSeparator => Some(TokenKind::StyleSeparator),
            Colon => Some(TokenKind::Colon),
            Amp => Some(TokenKind::Amp),
            Semi => Some(TokenKind::Semi),
            Comma => Some(TokenKind::Comma),
            Mult => Some(TokenKind::Mult),
            LinkArrow => {
                self.pop_mode();
                Some(TokenKind::Link)
            }
            StartLink => {
                self.push_mode(Mode::EdgeText);
                Some(TokenKind::StartLink)
            }
            EdgeTextChar => Some(TokenKind::EdgeText),
            LinkDotted => Some(TokenKind::Link),
            EllipseComment => None,
            EllipseEnd => {
                self.pop_mode();
                Some(TokenKind::EllipseEnd)
            }
            EllipseTextChar | TrapTextChar | Text => Some(TokenKind::Text),
            EllipseStart => {
                self.push_mode(Mode::EllipseText);
                Some(TokenKind::EllipseStart)
            }
            StadiumEnd => {
                self.pop_mode();
                Some(TokenKind::StadiumEnd)
            }
            StadiumStart => {
                self.push_mode(Mode::Text);
                Some(TokenKind::StadiumStart)
            }
            SubroutineEnd => {
                self.pop_mode();
                Some(TokenKind::SubroutineEnd)
            }
            SubroutineStart => {
                self.push_mode(Mode::Text);
                Some(TokenKind::SubroutineStart)
            }
            VertexWithPropsStart => Some(TokenKind::VertexWithPropsStart),
            TagEndPush => {
                self.push_mode(Mode::Text);
                Some(TokenKind::TagEnd)
            }
            CylinderEnd => {
                self.pop_mode();
                Some(TokenKind::CylinderEnd)
            }
            CylinderStart => {
                self.push_mode(Mode::Text);
                Some(TokenKind::CylinderStart)
            }
            DoubleCircleEnd => {
                self.pop_mode();
                Some(TokenKind::DoubleCircleEnd)
            }
            DoubleCircleStart => {
                self.push_mode(Mode::Text);
                Some(TokenKind::DoubleCircleStart)
            }
            TrapComment => None,
            TrapEnd => {
                self.pop_mode();
                Some(TokenKind::TrapEnd)
            }
            InvTrapEnd => {
                self.pop_mode();
                Some(TokenKind::InvTrapEnd)
            }
            TrapStart => {
                self.push_mode(Mode::TrapText);
                Some(TokenKind::TrapStart)
            }
            InvTrapStart => {
                self.push_mode(Mode::TrapText);
                Some(TokenKind::InvTrapStart)
            }
            TagStart => Some(TokenKind::TagStart),
            Up => Some(TokenKind::Up),
            Sep => Some(TokenKind::Sep),
            Down => Some(TokenKind::Down),
            Comment => Some(TokenKind::Comment),
            NodeString => Some(TokenKind::NodeString),
            Minus => Some(TokenKind::Minus),
            Unicode => Some(TokenKind::UnicodeText),
            TextPipe => {
                self.pop_mode();
                Some(TokenKind::Pipe)
            }
            PipeOpen => {
                self.push_mode(Mode::Text);
                Some(TokenKind::Pipe)
            }
            TextPe => {
                self.pop_mode();
                Some(TokenKind::Pe)
            }
            Ps => {
                self.push_mode(Mode::Text);
                Some(TokenKind::Ps)
            }
            TextSqe => {
                self.pop_mode();
                Some(TokenKind::Sqe)
            }
            Sqs => {
                self.push_mode(Mode::Text);
                Some(TokenKind::Sqs)
            }
            TextDiamondStop => {
                self.pop_mode();
                Some(TokenKind::DiamondStop)
            }
            DiamondStart => {
                self.push_mode(Mode::Text);
                Some(TokenKind::DiamondStart)
            }
            TextComment => None,
            Quote => Some(TokenKind::Quote),
            Newline => Some(TokenKind::Newline),
            Space => Some(TokenKind::Space),
            Eof => Some(TokenKind::Eof),
        };
        Ok(kind)
    }
}

/// The text a token carries: the matched text, except for the two rules that rewrite it.
fn token_text(rule: Rule, text: &str) -> String {
    match rule {
        Rule::ShapeDataStart => String::new(),
        Rule::ShapeDataStrText => collapse_newlines(text),
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

/// A literal rule jison anchors with `\b` because it ends in a word character.
fn literal_keyword(text: &str, literal: &str) -> Option<usize> {
    literal_prefix(text, literal).filter(|&len| {
        text[len..]
            .chars()
            .next()
            .is_none_or(|next| !(next.is_ascii_alphanumeric() || next == '_'))
    })
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
            b'=' => !matches!(bytes.get(index + 1), Some(b'=')),
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
    let mut at_positions = Vec::new();
    for (offset, ch) in text.char_indices() {
        if ch.is_whitespace() || matches!(ch, '"' | '[' | ']' | '(' | ')' | '{' | '}' | '|') {
            break;
        }
        if ch == '@' && offset > 0 {
            at_positions.push(offset);
        }
    }
    for position in at_positions.into_iter().rev() {
        match text[position + 1..].chars().next() {
            Some('{') | Some('"') | None => continue,
            Some(_) => return Some(position + 1),
        }
    }
    None
}

fn match_comment(lexer: &Lexer, at: usize, rule: Rule) -> Option<usize> {
    let start = match rule {
        Rule::Comment => lexer.run_while(at, |ch| matches!(ch, ' ' | '\t')),
        _ => at,
    };
    if !lexer.starts_with(start, "%%") {
        return None;
    }
    if lexer.char_at(start + 2) == Some('{') {
        return None;
    }
    let line_end = lexer.source[start + 2..]
        .find('\n')
        .map_or(lexer.source.len(), |offset| start + 2 + offset);
    Some(line_end - at)
}

fn match_dashed_link(lexer: &Lexer, at: usize) -> Option<usize> {
    let mut index = lexer.skip_whitespace(at);
    if lexer.byte_at(index) != Some(b'-') || lexer.byte_at(index + 1) != Some(b'-') {
        return None;
    }
    while lexer.byte_at(index) == Some(b'-') {
        index += 1;
    }
    // `--+[x>]` needs a marker; a bare `--` is a START_LINK, not a complete link.
    let next = lexer.byte_at(index)?;
    if !matches!(next, b'x' | b'>') {
        return None;
    }
    index += 1;
    Some(lexer.skip_whitespace(index) - at)
}

fn match_dashed_start(lexer: &Lexer, at: usize) -> Option<usize> {
    let index = lexer.skip_whitespace(at);
    if !lexer.starts_with(index, "--") {
        return None;
    }
    Some(lexer.skip_whitespace(index + 2) - at)
}

fn match_edge_text_char(lexer: &Lexer, at: usize) -> Option<usize> {
    let ch = lexer.char_at(at)?;
    if ch as u32 > 0x7f {
        return Some(ch.len_utf8());
    }
    if ch == '-' {
        (lexer.byte_at(at + 1) != Some(b'-')).then_some(1)
    } else {
        Some(ch.len_utf8())
    }
}

fn match_text_char(lexer: &Lexer, at: usize, rule: Rule) -> Option<usize> {
    let ch = lexer.char_at(at)?;
    if rule == Rule::EllipseTextChar {
        if matches!(ch, '(' | ')' | '[' | ']' | '{' | '}') {
            if ch == '!' {
                let end = lexer.run_while(at + 1, |ch| ch == ')');
                return (end > at + 1).then_some(end - at);
            }
            return None;
        }
        return Some(ch.len_utf8());
    }
    if matches!(ch, '[' | ']' | '(' | ')' | '{' | '}' | '|' | '"') {
        None
    } else if ch == '%' {
        if lexer.char_at(at + 1) == Some('%') {
            None
        } else {
            Some(1)
        }
    } else {
        let end = lexer.run_while(at, |ch| {
            !matches!(ch, '[' | ']' | '(' | ')' | '{' | '}' | '|' | '"' | '%')
        });
        (end > at).then_some(end - at)
    }
}

fn match_dotted_link(lexer: &Lexer, at: usize) -> Option<usize> {
    let mut index = lexer.skip_whitespace(at);
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
    Some(lexer.skip_whitespace(index) - at)
}

fn match_ellipse_end(lexer: &Lexer, at: usize) -> Option<usize> {
    let ch = lexer.char_at(at)?;
    (matches!(ch, '-' | '/' | ')') && lexer.char_at(at + 1) == Some(')')).then_some(2)
}
