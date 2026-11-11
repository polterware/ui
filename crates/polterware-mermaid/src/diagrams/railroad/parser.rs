//! The railroad grammar: a hand-written recursive-descent port of `railroad.langium`.
//!
//! ```text
//! Railroad:
//!   "railroad-beta"
//!   (title=TITLE | accTitle=ACC_TITLE | accDescr=ACC_DESCR)*
//!   rules+=RailroadRule*
//! RailroadRule:
//!   name=RR_ID "=" definition=RailroadExpression ";"
//! RailroadExpression:
//!   RailroadSequenceExpr | RailroadChoiceExpr | RailroadOptionalExpr
//!   | RailroadOneOrMoreExpr | RailroadZeroOrMoreExpr | RailroadTerminalExpr
//!   | RailroadNonTerminalExpr | RailroadSpecialExpr
//! RailroadSequenceExpr:
//!   "sequence" "(" elements+=RailroadExpression
//!   ("," elements+=RailroadExpression)* ")"
//! ```
//!
//! and so on for `choice` (`alternatives` in place of `elements`) and the one-element
//! `optional`, `oneOrMore`, and `zeroOrMore` forms. Whitespace, including newlines, is a
//! hidden token, so the grammar has no line structure beyond the metadata tokens' own line
//! limits.
//!
//! Values go through `railroad/valueConverter.ts`: `RR_STRING` is decoded by
//! `decodeEscapedString` (quotes removed, `\n`, `\r`, `\t`, and any other escaped character
//! written as itself), and `TITLE` additionally loses its quotes that way when it has them.
//! `ACC_TITLE` and `ACC_DESCR` are trimmed and collapsed by `common/valueConverter.ts`.
//!
//! `transformExpression` is applied here, when each expression node is built: a `sequence` or
//! `choice` with one child becomes that child, and `oneOrMore`/`zeroOrMore` become a
//! `repetition` with `min` 1/0 and an unbounded `max`.

use super::ast::{Node, Rule};
use super::lexer::{Lexer, Token, TokenKind};
use crate::error::ParseError;

pub(crate) fn parse(body: &str) -> Result<ParsedRailroad, ParseError> {
    Parser::new(body).parse_railroad()
}

/// The `Railroad` document before the database is populated.
pub(crate) struct ParsedRailroad {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_description: Option<String>,
    pub(crate) rules: Vec<Rule>,
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

    /// `Railroad`: the header, the metadata lines, the rules, and nothing else.
    fn parse_railroad(mut self) -> Result<ParsedRailroad, ParseError> {
        self.expect(TokenKind::RailroadBeta)?;
        let mut parsed = ParsedRailroad {
            title: None,
            acc_title: None,
            acc_description: None,
            rules: Vec::new(),
        };
        loop {
            match self.peek()? {
                TokenKind::Title => {
                    let token = self.bump()?;
                    parsed.title = Some(title_value(&token.text));
                }
                TokenKind::AccTitle => {
                    let token = self.bump()?;
                    parsed.acc_title = Some(acc_title_value(&token.text));
                }
                TokenKind::AccDescr => {
                    let token = self.bump()?;
                    parsed.acc_description = Some(acc_descr_value(&token.text));
                }
                _ => break,
            }
        }
        while self.peek()? == TokenKind::Id {
            parsed.rules.push(self.parse_rule()?);
        }
        self.expect(TokenKind::Eof)?;
        Ok(parsed)
    }

    /// `RailroadRule`: `name=RR_ID "=" definition=RailroadExpression ";"`.
    fn parse_rule(&mut self) -> Result<Rule, ParseError> {
        let name = self.expect(TokenKind::Id)?.text;
        self.expect(TokenKind::Equals)?;
        let definition = self.parse_expression()?;
        self.expect(TokenKind::Semicolon)?;
        Ok(Rule { name, definition })
    }

    fn parse_expression(&mut self) -> Result<Node, ParseError> {
        match self.peek()? {
            TokenKind::Sequence => Ok(collapse_sequence(self.parse_list(TokenKind::Sequence)?)),
            TokenKind::Choice => Ok(collapse_choice(self.parse_list(TokenKind::Choice)?)),
            TokenKind::Optional => Ok(Node::Optional {
                element: Box::new(self.parse_element(TokenKind::Optional)?),
            }),
            TokenKind::OneOrMore => self.parse_repetition(TokenKind::OneOrMore, 1),
            TokenKind::ZeroOrMore => self.parse_repetition(TokenKind::ZeroOrMore, 0),
            TokenKind::Terminal => Ok(Node::Terminal {
                value: self.parse_string(TokenKind::Terminal)?,
            }),
            TokenKind::NonTerminal => Ok(Node::NonTerminal {
                name: self.parse_string(TokenKind::NonTerminal)?,
            }),
            TokenKind::Special => Ok(Node::Special {
                text: self.parse_string(TokenKind::Special)?,
            }),
            _ => Err(self.unexpected("a railroad expression")),
        }
    }

    /// `keyword "(" element ")"`.
    fn parse_element(&mut self, keyword: TokenKind) -> Result<Node, ParseError> {
        self.expect(keyword)?;
        self.expect(TokenKind::LParen)?;
        let element = self.parse_expression()?;
        self.expect(TokenKind::RParen)?;
        Ok(element)
    }

    /// `keyword "(" element ("," element)* ")"`.
    fn parse_list(&mut self, keyword: TokenKind) -> Result<Vec<Node>, ParseError> {
        self.expect(keyword)?;
        self.expect(TokenKind::LParen)?;
        let mut elements = vec![self.parse_expression()?];
        while self.peek()? == TokenKind::Comma {
            self.bump()?;
            elements.push(self.parse_expression()?);
        }
        self.expect(TokenKind::RParen)?;
        Ok(elements)
    }

    /// `RailroadOneOrMoreExpr` or `RailroadZeroOrMoreExpr`.
    fn parse_repetition(&mut self, keyword: TokenKind, min: usize) -> Result<Node, ParseError> {
        Ok(Node::Repetition {
            element: Box::new(self.parse_element(keyword)?),
            min,
            max: None,
            separator: None,
        })
    }

    /// `keyword "(" value=RR_STRING ")"`, with the custom string conversion.
    fn parse_string(&mut self, keyword: TokenKind) -> Result<String, ParseError> {
        self.expect(keyword)?;
        self.expect(TokenKind::LParen)?;
        let token = self.expect(TokenKind::Str)?;
        self.expect(TokenKind::RParen)?;
        Ok(decode_escaped_string(&token.text))
    }
}

/// `transformExpression` on a `RailroadSequenceExpr`.
fn collapse_sequence(mut elements: Vec<Node>) -> Node {
    if elements.len() == 1 {
        elements.pop().expect("one element")
    } else {
        Node::Sequence { elements }
    }
}

/// `transformExpression` on a `RailroadChoiceExpr`.
fn collapse_choice(mut alternatives: Vec<Node>) -> Node {
    if alternatives.len() == 1 {
        alternatives.pop().expect("one alternative")
    } else {
        Node::Choice { alternatives }
    }
}

/// `railroad/valueConverter.ts`'s `decodeEscapedString`: drop the surrounding quotes, turn
/// `\n`, `\r`, and `\t` into their characters, and write any other escape as the escaped
/// character itself.
fn decode_escaped_string(raw: &str) -> String {
    let content = if raw.len() >= 2 {
        &raw[1..raw.len() - 1]
    } else {
        ""
    };
    let mut out = String::with_capacity(content.len());
    let mut chars = content.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\'
            && let Some(escaped) = chars.next()
        {
            match escaped {
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                other => out.push(other),
            }
            continue;
        }
        out.push(ch);
    }
    out
}

/// `common/valueConverter.ts`'s `titleRegex`: `/title([\t ][^\n\r]*|)/`, capture trimmed and
/// with runs of two or more spaces or tabs collapsed to one, then `railroad/valueConverter.ts`
/// decoding it as a quoted string when it is quoted.
fn title_value(raw: &str) -> String {
    let Some(start) = raw.find("title") else {
        return String::new();
    };
    let rest = &raw[start + "title".len()..];
    if !rest.starts_with([' ', '\t']) {
        return String::new();
    }
    let value = collapse(rest);
    let trimmed = trim(&value);
    if (trimmed.starts_with('"') && trimmed.ends_with('"'))
        || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
    {
        decode_escaped_string(trimmed)
    } else {
        value
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

/// The multiline branch of the accessible description conversion. For the regex sequence
/// `.replace(/^\s*/gm,'').replace(/\s+$/gm,'').replace(/[\t ]{2,}/gm,' ').replace(/[\n\r]{2,}/gm,'\n')`
/// this is the same result: `^\s*` and `\s+$` swallow the line breaks around every line, so
/// blank lines disappear, and what is left has no runs of two or more line breaks.
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
