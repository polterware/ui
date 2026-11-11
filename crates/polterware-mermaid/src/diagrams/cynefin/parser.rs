//! The cynefin grammar: a hand-written recursive-descent port of `cynefin.langium` with the
//! `TitleAndAccessibilities` fragment and the `EOL` rule of `common.langium`.
//!
//! ```text
//! Cynefin:
//!   NEWLINE*
//!   ('cynefin-beta' | 'cynefin-beta:')
//!   NEWLINE*
//!   ( TitleAndAccessibilities | domains+=DomainBlock | transitions+=Transition | NEWLINE )*
//! DomainBlock:
//!   domain=DOMAIN_NAME NEWLINE* (items+=DomainItem NEWLINE*)*
//! DomainItem:
//!   label=STRING
//! Transition:
//!   from=DOMAIN_NAME '-->' to=DOMAIN_NAME (':' label=STRING)? EOL
//! EOL:
//!   NEWLINE+ | EOF
//! TitleAndAccessibilities:
//!   ((accDescr=ACC_DESCR | accTitle=ACC_TITLE | title=TITLE) EOL)+
//! ```
//!
//! Langium's generated parser decides between `DomainBlock` and `Transition` by lookahead:
//! both start with `DOMAIN_NAME`, and only a following `-->` belongs to `Transition`. The
//! other two alternatives have disjoint first sets. Title, accessible title, and accessible
//! description are single-valued assignments, so a later line overwrites an earlier one, as
//! Langium's AST builder does.
//!
//! The terminal values go through `common/valueConverter.ts`: the default `STRING`
//! strip-and-unescape for labels, and the regex-driven trimming of `TITLE`, `ACC_TITLE`, and
//! `ACC_DESCR`. `DomainItem` and `Transition` also assign `STRING`, so they use the same
//! default conversion. `cynefin/` has no value converter of its own.

use super::ast::DomainName;
use super::lexer::{Lexer, Token, TokenKind};
use crate::error::ParseError;

/// The `Cynefin` AST node after parsing, before the database is populated.
pub(crate) struct ParsedCynefin {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_description: Option<String>,
    pub(crate) domains: Vec<ParsedDomain>,
    pub(crate) transitions: Vec<ParsedTransition>,
}

/// One `DomainBlock`.
pub(crate) struct ParsedDomain {
    pub(crate) name: DomainName,
    /// `DomainItem` labels in grammar order.
    pub(crate) items: Vec<String>,
}

/// One `Transition`.
pub(crate) struct ParsedTransition {
    pub(crate) from: DomainName,
    pub(crate) to: DomainName,
    pub(crate) label: Option<String>,
}

/// Parses the diagram body into the AST the database is populated from.
pub(crate) fn parse(body: &str) -> Result<ParsedCynefin, ParseError> {
    Parser::new(body).parse_cynefin()
}

struct Parser<'a> {
    lexer: Lexer<'a>,
    lookahead: Option<Token>,
    /// The token after the lookahead, fetched only when the `DomainBlock`/`Transition`
    /// decision needs it.
    second: Option<Token>,
}

impl<'a> Parser<'a> {
    fn new(body: &'a str) -> Self {
        Self {
            lexer: Lexer::new(body),
            lookahead: None,
            second: None,
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

    /// The kind of the token after the lookahead.
    fn peek_second(&mut self) -> Result<TokenKind, ParseError> {
        self.fill()?;
        if self.second.is_none() {
            self.second = Some(self.lexer.next_token()?);
        }
        Ok(self.second.as_ref().expect("just filled").kind)
    }

    fn bump(&mut self) -> Result<Token, ParseError> {
        self.fill()?;
        let token = self.lookahead.take().expect("filled above");
        self.lookahead = self.second.take();
        Ok(token)
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

    fn parse_cynefin(mut self) -> Result<ParsedCynefin, ParseError> {
        while self.peek()? == TokenKind::Newline {
            self.bump()?;
        }
        match self.peek()? {
            TokenKind::CynefinBeta | TokenKind::CynefinBetaColon => {
                self.bump()?;
            }
            _ => return Err(self.unexpected("'cynefin-beta'")),
        }
        while self.peek()? == TokenKind::Newline {
            self.bump()?;
        }

        let mut parsed = ParsedCynefin {
            title: None,
            acc_title: None,
            acc_description: None,
            domains: Vec::new(),
            transitions: Vec::new(),
        };
        loop {
            match self.peek()? {
                TokenKind::Eof => break,
                TokenKind::Newline => {
                    self.bump()?;
                }
                TokenKind::AccDescr | TokenKind::AccTitle | TokenKind::Title => {
                    // One `(accDescr | accTitle | title) EOL` line of `TitleAndAccessibilities`.
                    let token = self.bump()?;
                    if token.kind == TokenKind::AccDescr {
                        parsed.acc_description = Some(acc_descr_value(&token.text));
                    } else if token.kind == TokenKind::AccTitle {
                        parsed.acc_title = Some(acc_title_value(&token.text));
                    } else {
                        parsed.title = Some(title_value(&token.text));
                    }
                    self.expect_eol()?;
                }
                TokenKind::DomainName => {
                    if self.peek_second()? == TokenKind::Arrow {
                        parsed.transitions.push(self.parse_transition()?);
                    } else {
                        parsed.domains.push(self.parse_domain_block()?);
                    }
                }
                _ => {
                    return Err(self
                        .unexpected("a title, an accessibility line, a domain, or a transition"));
                }
            }
        }
        Ok(parsed)
    }

    fn parse_domain_block(&mut self) -> Result<ParsedDomain, ParseError> {
        let domain = self.expect(TokenKind::DomainName)?;
        let name = domain_name(&domain)?;
        while self.peek()? == TokenKind::Newline {
            self.bump()?;
        }
        let mut items = Vec::new();
        while self.peek()? == TokenKind::Str {
            let item = self.bump()?;
            items.push(convert_string(&item.text));
            while self.peek()? == TokenKind::Newline {
                self.bump()?;
            }
        }
        Ok(ParsedDomain { name, items })
    }

    fn parse_transition(&mut self) -> Result<ParsedTransition, ParseError> {
        let from = self.expect(TokenKind::DomainName)?;
        let from = domain_name(&from)?;
        self.expect(TokenKind::Arrow)?;
        let to = self.expect(TokenKind::DomainName)?;
        let to = domain_name(&to)?;
        let label = if self.peek()? == TokenKind::Colon {
            self.bump()?;
            let label = self.expect(TokenKind::Str)?;
            Some(convert_string(&label.text))
        } else {
            None
        };
        self.expect_eol()?;
        Ok(ParsedTransition { from, to, label })
    }
}

fn domain_name(token: &Token) -> Result<DomainName, ParseError> {
    DomainName::from_keyword(&token.text)
        .ok_or_else(|| ParseError::at(format!("unknown domain '{}'", token.text), token.span))
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
/// ends of every line, collapse runs of two or more spaces or tabs, and collapse line breaks.
/// For the regex sequence `.replace(/^\s*/gm,'')`, `.replace(/\s+$/gm,'')`,
/// `.replace(/[\t ]{2,}/gm,' ')`, `.replace(/[\n\r]{2,}/gm,'\n')` this is the same result,
/// including the blank lines the first replacement swallows.
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
