//! The packet grammar: a hand-written recursive-descent port of `packet.langium` with the
//! `TitleAndAccessibilities` fragment and the `EOL` rule of `common.langium`.
//!
//! ```text
//! Packet:
//!   NEWLINE* ("packet" | "packet-beta")
//!   ( TitleAndAccessibilities | blocks+=PacketBlock | NEWLINE )*
//! PacketBlock:
//!   ( start=INT ("-" end=INT)? | "+" bits=INT ) ":" label=STRING EOL
//! EOL:
//!   NEWLINE+ | EOF
//! TitleAndAccessibilities:
//!   ((accDescr=ACC_DESCR | accTitle=ACC_TITLE | title=TITLE) EOL)+
//! ```
//!
//! Title, accessible title, and accessible description are single-valued assignments, so a later
//! line overwrites an earlier one, as Langium's AST builder does. `INT` and `STRING` go through
//! Langium 4.4's `DefaultValueConverter`: `parseInt` for `INT`, and quote stripping with
//! backslash escapes for `STRING`. The regex-driven trimming of `TITLE`, `ACC_TITLE`, and
//! `ACC_DESCR` is `common/valueConverter.ts`'s.
//!
//! A block's span is its first number token: this crate's [`ParseError`] carries one where
//! upstream throws its database errors without a position. The whole document is parsed before
//! the database is populated, as `packet/parser.ts` does, so a syntax error later in the text
//! wins over a database error earlier in it.

use super::ast::Ast;
use super::db;
use super::lexer::{Lexer, Token, TokenKind};
use crate::common::text::{convert_string, trim_horizontal};
use crate::error::{ParseError, Span};

/// Parses the diagram body and populates the database.
pub(crate) fn parse(body: &str) -> Result<Ast, ParseError> {
    let parsed = Parser::new(body).parse_packet()?;
    db::build(parsed)
}

/// The `Packet` AST node after parsing, before the database is populated.
pub(crate) struct ParsedPacket {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_description: Option<String>,
    pub(crate) blocks: Vec<ParsedBlock>,
}

/// One `PacketBlock`; `start`/`end` come from the `INT "-" INT` form and `bits` from the
/// `"+" INT` form, so exactly one side is set.
pub(crate) struct ParsedBlock {
    pub(crate) start: Option<i64>,
    pub(crate) end: Option<i64>,
    pub(crate) bits: Option<i64>,
    pub(crate) label: String,
    /// The span of the first number token, where database errors are reported.
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

    fn parse_packet(mut self) -> Result<ParsedPacket, ParseError> {
        while self.peek()? == TokenKind::Newline {
            self.bump()?;
        }
        match self.peek()? {
            TokenKind::Packet | TokenKind::PacketBeta => {
                self.bump()?;
            }
            _ => return Err(self.unexpected("packet or packet-beta")),
        }

        let mut parsed = ParsedPacket {
            title: None,
            acc_title: None,
            acc_description: None,
            blocks: Vec::new(),
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
                TokenKind::Int | TokenKind::Plus => parsed.blocks.push(self.parse_block()?),
                _ => {
                    return Err(self.unexpected(
                        "a title, an accessibility line, a packet block, or a newline",
                    ));
                }
            }
        }
        Ok(parsed)
    }

    fn parse_block(&mut self) -> Result<ParsedBlock, ParseError> {
        let (start, end, bits, span) = match self.peek()? {
            TokenKind::Int => {
                let start = self.bump()?;
                let end = if self.peek()? == TokenKind::Dash {
                    self.bump()?;
                    Some(self.expect(TokenKind::Int)?)
                } else {
                    None
                };
                (
                    Some(parse_int(&start.text)),
                    end.map(|token| parse_int(&token.text)),
                    None,
                    start.span,
                )
            }
            TokenKind::Plus => {
                self.bump()?;
                let bits = self.expect(TokenKind::Int)?;
                (None, None, Some(parse_int(&bits.text)), bits.span)
            }
            _ => return Err(self.unexpected("a packet block")),
        };
        self.expect(TokenKind::Colon)?;
        let label = self.expect(TokenKind::Str)?;
        self.expect_eol()?;
        Ok(ParsedBlock {
            start,
            end,
            bits,
            label: convert_string(&label.text),
            span,
        })
    }
}

/// `parseInt` over the digits the lexer matched. A value that does not fit saturates, so a
/// hostile bit index cannot wrap around.
fn parse_int(text: &str) -> i64 {
    text.parse::<i64>().unwrap_or(i64::MAX)
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
