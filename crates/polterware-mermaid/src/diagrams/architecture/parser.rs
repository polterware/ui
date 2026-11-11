//! The architecture grammar: a hand-written recursive-descent port of
//! `architecture.langium` with the `TitleAndAccessibilities` fragment and the `EOL` rule of
//! `common.langium`, plus the terminals of `arch.langium`.
//!
//! ```text
//! Architecture:
//!   NEWLINE*
//!   "architecture-beta"
//!   ( NEWLINE | TitleAndAccessibilities | Statement )*
//! Statement:
//!   Group | Service | Junction | Edge | Alignment
//! Group:
//!   "group" id=ID icon=ARCH_ICON? title=ARCH_TITLE? ("in" in=ID)? EOL
//! Service:
//!   "service" id=ID (iconText=STRING | icon=ARCH_ICON)? title=ARCH_TITLE? ("in" in=ID)? EOL
//! Junction:
//!   "junction" id=ID ("in" in=ID)? EOL
//! Edge:
//!   lhsId=ID lhsGroup?=ARROW_GROUP?
//!   ":" lhsDir=ARROW_DIRECTION lhsInto?=ARROW_INTO?
//!   ("--" | "-" title=ARCH_TITLE "-")
//!   rhsInto?=ARROW_INTO? rhsDir=ARROW_DIRECTION ":" rhsId=ID rhsGroup?=ARROW_GROUP? EOL
//! Alignment:
//!   "align" ( "row" | "column" ) members+=ID (members+=ID)+ EOL
//! TitleAndAccessibilities:
//!   ((accDescr=ACC_DESCR | accTitle=ACC_TITLE | title=TITLE) EOL)+
//! EOL:
//!   NEWLINE+ | EOF
//! ```
//!
//! The root rule has no action, so every assignment lands on one flat `Architecture` node:
//! title, accessible title, accessible description, groups, services, junctions, edges, and
//! alignments. Title, accessible title, and accessible description are single-valued, so a
//! later line overwrites an earlier one, as Langium's AST builder does. The terminal values
//! go through `common/valueConverter.ts` and `architecture/valueConverter.ts`: the default
//! `STRING` strip-and-unescape, the `ID` conversion (which only strips a leading `^`, which
//! the `ID` terminal cannot produce), the regex-driven trimming of `TITLE`, `ACC_TITLE`, and
//! `ACC_DESCR`, and the architecture converter's `ARCH_ICON` and `ARCH_TITLE` branches. Its
//! `ARCH_TEXT_ICON` branch is dead, because no rule or terminal in these grammars has that
//! name.

use super::ast::{AlignmentDirection, Ast, Direction, Edge, Group, Junction, LayoutHint, Service};
use super::db;
use super::lexer::{Lexer, Token, TokenKind};
use crate::error::{ParseError, Span};
use crate::preprocess::is_js_whitespace;

/// Parses the diagram body and populates the database.
pub(crate) fn parse(body: &str) -> Result<Ast, ParseError> {
    let parsed = parse_grammar(body)?;
    db::build(parsed)
}

/// The `Architecture` node after parsing, before the database is populated. Each statement
/// keeps the span of the token it is reported at: its id, or the `align` keyword.
#[derive(Default)]
pub(crate) struct ParsedArchitecture {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_description: Option<String>,
    pub(crate) groups: Vec<(Group, Span)>,
    pub(crate) services: Vec<(Service, Span)>,
    pub(crate) junctions: Vec<(Junction, Span)>,
    pub(crate) edges: Vec<(Edge, Span)>,
    pub(crate) alignments: Vec<(LayoutHint, Span)>,
}

/// Parses the grammar alone, as upstream's `@mermaid-js/parser` tests do, without populating
/// the database.
pub(crate) fn parse_grammar(body: &str) -> Result<ParsedArchitecture, ParseError> {
    Parser::new(body).parse_architecture()
}

struct Parser<'a> {
    lexer: Lexer<'a>,
    lookahead: Option<Token>,
}

impl<'a> Parser<'a> {
    fn new(body: &'a str) -> Self {
        Self {
            lexer: Lexer::new(body),
            lookahead: None,
        }
    }

    fn fill(&mut self) -> Result<&Token, ParseError> {
        if self.lookahead.is_none() {
            self.lookahead = Some(self.lexer.next_token()?);
        }
        Ok(self.lookahead.as_ref().expect("just filled"))
    }

    fn peek(&mut self) -> Result<TokenKind, ParseError> {
        Ok(self.fill()?.kind)
    }

    fn bump(&mut self) -> Result<Token, ParseError> {
        self.fill()?;
        Ok(self.lookahead.take().expect("filled above"))
    }

    fn expect(&mut self, expected: TokenKind) -> Result<Token, ParseError> {
        if self.peek()? != expected {
            return Err(self.unexpected(expected.name()));
        }
        self.bump()
    }

    fn unexpected(&mut self, expected: &str) -> ParseError {
        match self.fill() {
            Ok(token) => ParseError::at(
                format!("expected {expected}, got '{}'", token.kind.name()),
                token.span,
            ),
            Err(error) => error,
        }
    }

    /// `EOL: NEWLINE+ | EOF`.
    fn expect_eol(&mut self) -> Result<(), ParseError> {
        match self.peek()? {
            TokenKind::Eof => Ok(()),
            TokenKind::Newline => {
                while self.peek()? == TokenKind::Newline {
                    self.bump()?;
                }
                Ok(())
            }
            _ => Err(self.unexpected("a newline or end of input")),
        }
    }

    fn parse_architecture(mut self) -> Result<ParsedArchitecture, ParseError> {
        while self.peek()? == TokenKind::Newline {
            self.bump()?;
        }
        self.expect(TokenKind::ArchitectureBeta)?;

        let mut parsed = ParsedArchitecture::default();
        loop {
            match self.peek()? {
                TokenKind::Newline => {
                    self.bump()?;
                }
                TokenKind::Eof => break,
                TokenKind::AccDescr => {
                    let token = self.bump()?;
                    parsed.acc_description = Some(acc_descr_value(&token.text));
                    self.expect_eol()?;
                }
                TokenKind::AccTitle => {
                    let token = self.bump()?;
                    parsed.acc_title = Some(acc_title_value(&token.text));
                    self.expect_eol()?;
                }
                TokenKind::Title => {
                    let token = self.bump()?;
                    parsed.title = Some(title_value(&token.text));
                    self.expect_eol()?;
                }
                TokenKind::Group => {
                    let (group, span) = self.parse_group()?;
                    parsed.groups.push((group, span));
                }
                TokenKind::Service => {
                    let (service, span) = self.parse_service()?;
                    parsed.services.push((service, span));
                }
                TokenKind::Junction => {
                    let (junction, span) = self.parse_junction()?;
                    parsed.junctions.push((junction, span));
                }
                TokenKind::Id => {
                    let (edge, span) = self.parse_edge()?;
                    parsed.edges.push((edge, span));
                }
                TokenKind::Align => {
                    let (hint, span) = self.parse_alignment()?;
                    parsed.alignments.push((hint, span));
                }
                _ => {
                    return Err(self.unexpected("a statement or a newline"));
                }
            }
        }
        Ok(parsed)
    }

    fn parse_group(&mut self) -> Result<(Group, Span), ParseError> {
        self.expect(TokenKind::Group)?;
        let id = self.expect(TokenKind::Id)?;
        let group = Group {
            id: id.text.clone(),
            icon: self.optional_icon()?,
            title: self.optional_title()?,
            container: self.optional_container()?,
        };
        self.expect_eol()?;
        Ok((group, id.span))
    }

    fn parse_service(&mut self) -> Result<(Service, Span), ParseError> {
        self.expect(TokenKind::Service)?;
        let id = self.expect(TokenKind::Id)?;
        let (icon_text, icon) = match self.peek()? {
            TokenKind::Str => (Some(convert_string(&self.bump()?.text)), None),
            TokenKind::Icon => (None, self.optional_icon()?),
            _ => (None, None),
        };
        let service = Service {
            id: id.text.clone(),
            icon,
            icon_text,
            title: self.optional_title()?,
            container: self.optional_container()?,
            edges: Vec::new(),
        };
        self.expect_eol()?;
        Ok((service, id.span))
    }

    fn parse_junction(&mut self) -> Result<(Junction, Span), ParseError> {
        self.expect(TokenKind::Junction)?;
        let id = self.expect(TokenKind::Id)?;
        let junction = Junction {
            id: id.text.clone(),
            container: self.optional_container()?,
            edges: Vec::new(),
        };
        self.expect_eol()?;
        Ok((junction, id.span))
    }

    fn parse_edge(&mut self) -> Result<(Edge, Span), ParseError> {
        let lhs = self.expect(TokenKind::Id)?;
        let lhs_group = self.optional_flag(TokenKind::GroupModifier)?;
        self.expect(TokenKind::Colon)?;
        let lhs_dir = self.expect_direction()?;
        let lhs_into = self.optional_flag(TokenKind::Into)?;
        let title = if self.peek()? == TokenKind::Dash {
            self.bump()?;
            let title = self.expect(TokenKind::BracketTitle)?;
            self.expect(TokenKind::Dash)?;
            Some(convert_bracket_title(&title.text))
        } else {
            self.expect(TokenKind::DoubleDash)?;
            None
        };
        let rhs_into = self.optional_flag(TokenKind::Into)?;
        let rhs_dir = self.expect_direction()?;
        self.expect(TokenKind::Colon)?;
        let rhs = self.expect(TokenKind::Id)?;
        let rhs_group = self.optional_flag(TokenKind::GroupModifier)?;
        self.expect_eol()?;
        let edge = Edge {
            lhs_id: lhs.text.clone(),
            lhs_dir,
            lhs_into,
            lhs_group,
            rhs_id: rhs.text,
            rhs_dir,
            rhs_into,
            rhs_group,
            title,
        };
        Ok((edge, lhs.span))
    }

    fn parse_alignment(&mut self) -> Result<(LayoutHint, Span), ParseError> {
        let align = self.expect(TokenKind::Align)?;
        let direction = match self.peek()? {
            TokenKind::Row => {
                self.bump()?;
                AlignmentDirection::Row
            }
            TokenKind::Column => {
                self.bump()?;
                AlignmentDirection::Column
            }
            _ => return Err(self.unexpected("'row' or 'column'")),
        };
        let first = self.expect(TokenKind::Id)?;
        let mut members = vec![first.text];
        while self.peek()? == TokenKind::Id {
            members.push(self.bump()?.text);
        }
        // `members+=ID (members+=ID)+`: the grammar needs a second member.
        if members.len() < 2 {
            return Err(self.unexpected(TokenKind::Id.name()));
        }
        self.expect_eol()?;
        Ok((LayoutHint { direction, members }, align.span))
    }

    fn optional_flag(&mut self, kind: TokenKind) -> Result<bool, ParseError> {
        if self.peek()? == kind {
            self.bump()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn optional_icon(&mut self) -> Result<Option<String>, ParseError> {
        if self.peek()? == TokenKind::Icon {
            Ok(Some(convert_icon(&self.bump()?.text)))
        } else {
            Ok(None)
        }
    }

    fn optional_title(&mut self) -> Result<Option<String>, ParseError> {
        if self.peek()? == TokenKind::BracketTitle {
            Ok(Some(convert_bracket_title(&self.bump()?.text)))
        } else {
            Ok(None)
        }
    }

    fn optional_container(&mut self) -> Result<Option<String>, ParseError> {
        if self.peek()? == TokenKind::In {
            self.bump()?;
            Ok(Some(self.expect(TokenKind::Id)?.text))
        } else {
            Ok(None)
        }
    }

    fn expect_direction(&mut self) -> Result<Direction, ParseError> {
        let token = self.expect(TokenKind::Direction)?;
        match token.text.as_str() {
            "L" => Ok(Direction::Left),
            "R" => Ok(Direction::Right),
            "T" => Ok(Direction::Top),
            "B" => Ok(Direction::Bottom),
            other => Err(ParseError::at(
                format!("invalid direction '{other}'"),
                token.span,
            )),
        }
    }
}

/// Langium's default `STRING` conversion: drop the quotes and read `\` escapes.
fn convert_string(raw: &str) -> String {
    let inner = &raw[1..raw.len() - 1];
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('b') => out.push('\u{8}'),
            Some('f') => out.push('\u{c}'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('v') => out.push('\u{b}'),
            Some('0') => out.push('\0'),
            Some(other) => out.push(other),
            // The lexer only produces a string when every backslash has a character.
            None => {}
        }
    }
    out
}

/// `ArchitectureValueConverter`'s `ARCH_ICON` branch: `input.replace(/[()]/g, '').trim()`.
fn convert_icon(raw: &str) -> String {
    raw.chars()
        .filter(|ch| *ch != '(' && *ch != ')')
        .collect::<String>()
        .trim_matches(is_js_whitespace)
        .to_string()
}

/// `ArchitectureValueConverter`'s `ARCH_TITLE` branch: strip the brackets, then the outer
/// quotes when both ends have one, unescape `\"` and `\'`, and trim.
fn convert_bracket_title(raw: &str) -> String {
    let mut content = raw;
    if let Some(stripped) = content.strip_prefix('[') {
        content = stripped;
    }
    if let Some(stripped) = content.strip_suffix(']') {
        content = stripped;
    }
    let content = content.trim_matches(is_js_whitespace);
    let quoted = (content.starts_with('"') && content.ends_with('"'))
        || (content.starts_with('\'') && content.ends_with('\''));
    if quoted {
        // JavaScript's `slice(1, -1)` on a one-character string is empty.
        let inner = if content.len() >= 2 {
            &content[1..content.len() - 1]
        } else {
            ""
        };
        inner
            .replace("\\\"", "\"")
            .replace("\\'", "'")
            .trim_matches(is_js_whitespace)
            .to_string()
    } else {
        content.to_string()
    }
}

/// `common/valueConverter.ts`'s `titleRegex`: `/title([\t ][^\n\r]*|)/`, capture trimmed and
/// with runs of two or more spaces or tabs collapsed to one.
fn title_value(raw: &str) -> String {
    let Some(start) = raw.find("title") else {
        return String::new();
    };
    let rest = &raw[start + "title".len()..];
    if rest.starts_with(' ') || rest.starts_with('\t') {
        collapse(rest)
    } else {
        String::new()
    }
}

/// `accessibilityTitleRegex`: `/accTitle[\t ]*:([^\n\r]*)/`.
fn acc_title_value(raw: &str) -> String {
    let Some(start) = raw.find("accTitle") else {
        return String::new();
    };
    let rest = raw[start + "accTitle".len()..].trim_start_matches([' ', '\t']);
    let Some(value) = rest.strip_prefix(':') else {
        return String::new();
    };
    collapse(&value[..line_end(value)])
}

/// `accessibilityDescrRegex`: `/accDescr(?:[\t ]*:([^\n\r]*)|\s*{([^}]*)})/`.
fn acc_descr_value(raw: &str) -> String {
    let Some(start) = raw.find("accDescr") else {
        return String::new();
    };
    let rest = &raw[start + "accDescr".len()..];
    let after_colon_space = rest.trim_start_matches([' ', '\t']);
    if let Some(value) = after_colon_space.strip_prefix(':') {
        return collapse(&value[..line_end(value)]);
    }
    let Some(value) = rest.trim_start_matches(is_js_whitespace).strip_prefix('{') else {
        return String::new();
    };
    let capture = &value[..value.find('}').unwrap_or(value.len())];
    normalize_multiline(capture)
}

/// The multiline branch of the accessible description conversion: strip whitespace at both
/// ends of every line, collapse runs of two or more spaces or tabs, collapse line breaks, and
/// collapse consecutive blank lines. For the regex sequence
/// `.replace(/^\s*/gm,'').replace(/\s+$/gm,'').replace(/[\t ]{2,}/gm,' ').replace(/[\n\r]{2,}/gm,'\n')`
/// this is the same result, including the blank lines the first replacement swallows.
fn normalize_multiline(capture: &str) -> String {
    let mut lines = Vec::new();
    for line in capture.split(['\n', '\r']) {
        let line = line.trim_matches(is_js_whitespace);
        if line.is_empty() {
            continue;
        }
        lines.push(collapse(line));
    }
    lines.join("\n")
}

/// `.replace(/[\t ]{2,}/gm, ' ')` on an already trimmed string.
fn collapse(text: &str) -> String {
    let text = text.trim_matches(is_js_whitespace);
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b' ' || bytes[i] == b'\t' {
            let start = i;
            while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
                i += 1;
            }
            if i - start > 1 {
                out.push(' ');
            } else {
                out.push_str(&text[start..i]);
            }
        } else {
            let ch = text[i..].chars().next().expect("non-empty remainder");
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

fn line_end(text: &str) -> usize {
    text.find(['\r', '\n']).unwrap_or(text.len())
}
