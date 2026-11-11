//! The pie grammar: a hand-written recursive-descent port of `pie.langium` with the
//! `TitleAndAccessibilities` fragment and the `EOL` rule of `common.langium`.
//!
//! ```text
//! Pie:
//!   NEWLINE*
//!   "pie" showData?="showData"?
//!   ( TitleAndAccessibilities | sections+=PieSection | NEWLINE )*
//! PieSection:
//!   label=STRING ":" value=NUMBER_PIE EOL
//! EOL:
//!   NEWLINE+ | EOF
//! TitleAndAccessibilities:
//!   ((accDescr=ACC_DESCR | accTitle=ACC_TITLE | title=TITLE) EOL)+
//! ```
//!
//! Title, accessible title, and accessible description are single-valued assignments, so a
//! later line overwrites an earlier one, as Langium's AST builder does. The terminal values go
//! through `common/valueConverter.ts`: the default `STRING` strip-and-unescape for labels, the
//! default `NUMBER` conversion (`Number(input)`) for values, and the regex-driven trimming of
//! `TITLE`, `ACC_TITLE`, and `ACC_DESCR`. `pie/valueConverter.ts`'s `PIE_SECTION_LABEL` branch
//! is dead here, because the grammar assigns `label=STRING`.

use super::ast::Ast;
use super::db;
use super::lexer::{Lexer, Token, TokenKind};
use crate::error::{ParseError, Span};

/// Parses the diagram body and populates the database.
pub(crate) fn parse(body: &str) -> Result<Ast, ParseError> {
    let parsed = Parser::new(body).parse_pie()?;
    db::build(parsed)
}

/// The `Pie` AST node after parsing, before the database is populated.
pub(crate) struct ParsedPie {
    pub(crate) show_data: bool,
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_description: Option<String>,
    pub(crate) sections: Vec<ParsedSection>,
}

/// One `PieSection`.
pub(crate) struct ParsedSection {
    pub(crate) label: String,
    pub(crate) value: f64,
    /// The `NUMBER_PIE` token, where a negative value is reported.
    pub(crate) span: Span,
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

    fn parse_pie(mut self) -> Result<ParsedPie, ParseError> {
        while self.peek()? == TokenKind::Newline {
            self.bump()?;
        }
        self.expect(TokenKind::Pie)?;
        let show_data = if self.peek()? == TokenKind::ShowData {
            self.bump()?;
            true
        } else {
            false
        };

        let mut parsed = ParsedPie {
            show_data,
            title: None,
            acc_title: None,
            acc_description: None,
            sections: Vec::new(),
        };
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
                TokenKind::Str => parsed.sections.push(self.parse_section()?),
                _ => {
                    return Err(
                        self.unexpected("a title, an accessibility line, a section, or a newline")
                    );
                }
            }
        }
        Ok(parsed)
    }

    fn parse_section(&mut self) -> Result<ParsedSection, ParseError> {
        let label = self.expect(TokenKind::Str)?;
        let label = convert_string(&label.text);
        self.expect(TokenKind::Colon)?;
        let value = self.expect(TokenKind::Number)?;
        let number = value
            .text
            .parse::<f64>()
            .map_err(|_| ParseError::at(format!("invalid number '{}'", value.text), value.span))?;
        self.expect_eol()?;
        Ok(ParsedSection {
            label,
            value: number,
            span: value.span,
        })
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
    let rest = trim_horizontal(&raw[start + "accTitle".len()..]);
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
    let after_colon_space = trim_horizontal(rest);
    if let Some(value) = after_colon_space.strip_prefix(':') {
        return collapse(&value[..line_end(value)]);
    }
    let Some(value) = rest.trim_start_matches(is_js_space).strip_prefix('{') else {
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
        let line = trim(line);
        if line.is_empty() {
            continue;
        }
        lines.push(collapse(line));
    }
    lines.join("\n")
}

/// `String.prototype.trim`, close enough: JavaScript's `\s` also has U+FEFF.
fn trim(text: &str) -> &str {
    text.trim_matches(is_js_space)
}

fn trim_horizontal(text: &str) -> &str {
    text.trim_start_matches([' ', '\t'])
}

/// `.replace(/[\t ]{2,}/gm, ' ')` on an already trimmed string.
fn collapse(text: &str) -> String {
    let text = trim(text);
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

fn is_js_space(ch: char) -> bool {
    ch.is_whitespace() || ch == '\u{feff}'
}
