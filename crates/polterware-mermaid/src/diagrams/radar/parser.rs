//! The radar grammar: a hand-written recursive-descent port of `radar.langium` with the
//! `TitleAndAccessibilities` fragment and the `EOL` rule of `common.langium`.
//!
//! ```text
//! Radar:
//!   NEWLINE*
//!   ("radar-beta" | "radar-beta:" | "radar-beta" ":")
//!   NEWLINE*
//!   ( TitleAndAccessibilities
//!   | "axis" axes+=Axis ("," axes+=Axis)*
//!   | "curve" curves+=Curve ("," curves+=Curve)*
//!   | Option ("," Option)*
//!   | NEWLINE )*
//! Axis:
//!   name=ID ("[" label=STRING "]")?
//! Curve:
//!   name=ID ("[" label=STRING "]")? "{" Entries "}"
//! Entries:
//!   NEWLINE* NumberEntry ("," NEWLINE* NumberEntry)* NEWLINE*
//! | NEWLINE* DetailedEntry ("," NEWLINE* DetailedEntry)* NEWLINE*
//! NumberEntry:
//!   value=NUMBER
//! DetailedEntry:
//!   axis=[Axis:ID] ":"? value=NUMBER
//! Option:
//!   "showLegend" BOOLEAN | "ticks" NUMBER | "max" NUMBER | "min" NUMBER | "graticule" GRATICULE
//! EOL:
//!   NEWLINE+ | EOF
//! TitleAndAccessibilities:
//!   ((accDescr=ACC_DESCR | accTitle=ACC_TITLE | title=TITLE) EOL)+
//! ```
//!
//! Langium decides the two `Entries` alternatives by the first token after the newlines:
//! `NUMBER` starts a number entry, `ID` a detailed one, so one curve cannot mix them. The
//! statement loop needs no end-of-line: `axis A curve c{1}` on one line is valid, and the
//! grammar's `NEWLINE` alternative only skips blank space.
//!
//! Title, accessible title, and accessible description are single-valued assignments, so a
//! later line overwrites an earlier one, as Langium's AST builder does. The terminal values
//! go through `common/valueConverter.ts`: the default `STRING` strip-and-unescape for labels,
//! the default `NUMBER` conversion (`Number(input)`) for values, the default boolean
//! conversion, the identity for `GRATICULE` and `ID`, and the regex-driven trimming of
//! `TITLE`, `ACC_TITLE`, and `ACC_DESCR`. A cross-reference keeps only its `$refText`, the
//! axis id the entry names.

use super::ast::Graticule;
use super::lexer::{Lexer, Token, TokenKind};
use crate::common::text::{convert_string, trim_horizontal};
use crate::error::{ParseError, Span};

/// The `Radar` AST node after parsing, before the database is populated.
pub(crate) struct ParsedRadar {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_description: Option<String>,
    pub(crate) axes: Vec<ParsedAxis>,
    pub(crate) curves: Vec<ParsedCurve>,
    pub(crate) options: Vec<ParsedOption>,
}

impl ParsedRadar {
    fn new() -> Self {
        Self {
            title: None,
            acc_title: None,
            acc_description: None,
            axes: Vec::new(),
            curves: Vec::new(),
            options: Vec::new(),
        }
    }
}

/// One `Axis`.
pub(crate) struct ParsedAxis {
    pub(crate) name: String,
    /// `label`, unset when the axis has no quoted label.
    pub(crate) label: Option<String>,
}

/// One `Curve`.
pub(crate) struct ParsedCurve {
    pub(crate) name: String,
    /// `label`, unset when the curve has no quoted label.
    pub(crate) label: Option<String>,
    pub(crate) entries: ParsedEntries,
    /// The curve's name token, where this crate reports the two database errors upstream
    /// throws without a location.
    pub(crate) span: Span,
}

/// One `Entry` list. The grammar's two alternatives are disjoint, so a curve is either all
/// number entries or all detailed ones.
pub(crate) enum ParsedEntries {
    /// `NumberEntry`s, in source order.
    Numbers(Vec<f64>),
    /// `DetailedEntry`s, in source order.
    Detailed(Vec<ParsedDetailedEntry>),
}

/// One `DetailedEntry`: `axis=[Axis:ID] ':'? value=NUMBER`.
pub(crate) struct ParsedDetailedEntry {
    /// The `$refText` of the axis reference.
    pub(crate) axis: String,
    pub(crate) value: f64,
}

/// One `Option`.
pub(crate) enum ParsedOption {
    ShowLegend(bool),
    Ticks(f64),
    Max(f64),
    Min(f64),
    Graticule(Graticule),
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

    fn skip_newlines(&mut self) -> Result<(), ParseError> {
        while self.peek()? == TokenKind::Newline {
            self.bump()?;
        }
        Ok(())
    }

    fn parse_document(mut self) -> Result<ParsedRadar, ParseError> {
        self.skip_newlines()?;
        match self.peek()? {
            TokenKind::RadarBeta => {
                self.bump()?;
                // The grammar's third header form is `'radar-beta' ':'`.
                if self.peek()? == TokenKind::Colon {
                    self.bump()?;
                }
            }
            TokenKind::RadarBetaColon => {
                self.bump()?;
            }
            _ => return Err(self.unexpected("'radar-beta'")),
        }
        self.skip_newlines()?;

        let mut parsed = ParsedRadar::new();
        loop {
            match self.peek()? {
                TokenKind::Newline => {
                    self.bump()?;
                }
                TokenKind::Eof => break,
                TokenKind::AccDescr | TokenKind::AccTitle | TokenKind::Title => {
                    self.parse_title_and_accessibilities(&mut parsed)?;
                }
                TokenKind::Axis => self.parse_axes(&mut parsed)?,
                TokenKind::Curve => self.parse_curves(&mut parsed)?,
                TokenKind::ShowLegend
                | TokenKind::Ticks
                | TokenKind::Max
                | TokenKind::Min
                | TokenKind::Graticule => self.parse_options(&mut parsed)?,
                _ => {
                    return Err(self.unexpected(
                        "a title, an accessibility line, an axis, a curve, an option, or a newline",
                    ));
                }
            }
        }
        Ok(parsed)
    }

    /// `TitleAndAccessibilities: ((accDescr=ACC_DESCR | accTitle=ACC_TITLE | title=TITLE) EOL)+`.
    fn parse_title_and_accessibilities(
        &mut self,
        parsed: &mut ParsedRadar,
    ) -> Result<(), ParseError> {
        while matches!(
            self.peek()?,
            TokenKind::AccDescr | TokenKind::AccTitle | TokenKind::Title
        ) {
            let token = self.bump()?;
            match token.kind {
                TokenKind::AccDescr => {
                    parsed.acc_description = Some(acc_descr_value(&token.text));
                }
                TokenKind::AccTitle => {
                    parsed.acc_title = Some(acc_title_value(&token.text));
                }
                _ => {
                    parsed.title = Some(title_value(&token.text));
                }
            }
            self.expect_eol()?;
        }
        Ok(())
    }

    /// `'axis' axes+=Axis (',' axes+=Axis)*`.
    fn parse_axes(&mut self, parsed: &mut ParsedRadar) -> Result<(), ParseError> {
        self.expect(TokenKind::Axis)?;
        parsed.axes.push(self.parse_axis()?);
        while self.peek()? == TokenKind::Comma {
            self.bump()?;
            parsed.axes.push(self.parse_axis()?);
        }
        Ok(())
    }

    /// `Axis: name=ID (Label)?` with `Label: '[' label=STRING ']'`.
    fn parse_axis(&mut self) -> Result<ParsedAxis, ParseError> {
        let name = self.expect(TokenKind::Id)?.text;
        Ok(ParsedAxis {
            name,
            label: self.parse_label()?,
        })
    }

    /// `'curve' curves+=Curve (',' curves+=Curve)*`.
    fn parse_curves(&mut self, parsed: &mut ParsedRadar) -> Result<(), ParseError> {
        self.expect(TokenKind::Curve)?;
        parsed.curves.push(self.parse_curve()?);
        while self.peek()? == TokenKind::Comma {
            self.bump()?;
            parsed.curves.push(self.parse_curve()?);
        }
        Ok(())
    }

    /// `Curve: name=ID (Label)? '{' Entries '}'`.
    fn parse_curve(&mut self) -> Result<ParsedCurve, ParseError> {
        let token = self.expect(TokenKind::Id)?;
        let label = self.parse_label()?;
        self.expect(TokenKind::LBrace)?;
        let entries = self.parse_entries()?;
        self.expect(TokenKind::RBrace)?;
        Ok(ParsedCurve {
            name: token.text,
            label,
            entries,
            span: token.span,
        })
    }

    /// `Label: '[' label=STRING ']'`.
    fn parse_label(&mut self) -> Result<Option<String>, ParseError> {
        if self.peek()? != TokenKind::LBracket {
            return Ok(None);
        }
        self.bump()?;
        let label = self.expect(TokenKind::Str)?;
        self.expect(TokenKind::RBracket)?;
        Ok(Some(convert_string(&label.text)))
    }

    /// The two `Entries` alternatives, decided by the first token after the newlines.
    fn parse_entries(&mut self) -> Result<ParsedEntries, ParseError> {
        self.skip_newlines()?;
        let entries = match self.peek()? {
            TokenKind::Number => {
                let mut values = vec![self.parse_number_entry()?];
                while self.peek()? == TokenKind::Comma {
                    self.bump()?;
                    self.skip_newlines()?;
                    values.push(self.parse_number_entry()?);
                }
                ParsedEntries::Numbers(values)
            }
            TokenKind::Id => {
                let mut entries = vec![self.parse_detailed_entry()?];
                while self.peek()? == TokenKind::Comma {
                    self.bump()?;
                    self.skip_newlines()?;
                    entries.push(self.parse_detailed_entry()?);
                }
                ParsedEntries::Detailed(entries)
            }
            _ => return Err(self.unexpected("a number or an axis reference")),
        };
        self.skip_newlines()?;
        Ok(entries)
    }

    /// `NumberEntry: value=NUMBER`.
    fn parse_number_entry(&mut self) -> Result<f64, ParseError> {
        let token = self.expect(TokenKind::Number)?;
        parse_number(&token)
    }

    /// `DetailedEntry: axis=[Axis:ID] ':'? value=NUMBER`.
    fn parse_detailed_entry(&mut self) -> Result<ParsedDetailedEntry, ParseError> {
        let axis = self.expect(TokenKind::Id)?.text;
        if self.peek()? == TokenKind::Colon {
            self.bump()?;
        }
        let token = self.expect(TokenKind::Number)?;
        Ok(ParsedDetailedEntry {
            axis,
            value: parse_number(&token)?,
        })
    }

    /// `options+=Option (',' options+=Option)*`.
    fn parse_options(&mut self, parsed: &mut ParsedRadar) -> Result<(), ParseError> {
        parsed.options.push(self.parse_option()?);
        while self.peek()? == TokenKind::Comma {
            self.bump()?;
            parsed.options.push(self.parse_option()?);
        }
        Ok(())
    }

    fn parse_option(&mut self) -> Result<ParsedOption, ParseError> {
        let token = self.bump()?;
        match token.kind {
            TokenKind::ShowLegend => {
                let value = self.expect(TokenKind::Boolean)?;
                Ok(ParsedOption::ShowLegend(value.text == "true"))
            }
            TokenKind::Ticks => Ok(ParsedOption::Ticks(self.parse_option_number()?)),
            TokenKind::Max => Ok(ParsedOption::Max(self.parse_option_number()?)),
            TokenKind::Min => Ok(ParsedOption::Min(self.parse_option_number()?)),
            TokenKind::Graticule => {
                let value = self.expect(TokenKind::GraticuleValue)?;
                let graticule = match value.text.as_str() {
                    "circle" => Graticule::Circle,
                    _ => Graticule::Polygon,
                };
                Ok(ParsedOption::Graticule(graticule))
            }
            _ => Err(self.unexpected("an option name")),
        }
    }

    fn parse_option_number(&mut self) -> Result<f64, ParseError> {
        let token = self.expect(TokenKind::Number)?;
        parse_number(&token)
    }
}

/// Parses the document.
pub(crate) fn parse_document(body: &str) -> Result<ParsedRadar, ParseError> {
    Parser::new(body).parse_document()
}

fn parse_number(token: &Token) -> Result<f64, ParseError> {
    token
        .text
        .parse::<f64>()
        .map_err(|_| ParseError::at(format!("invalid number '{}'", token.text), token.span))
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
    let Some(value) = rest
        .trim_start_matches(crate::preprocess::is_js_whitespace)
        .strip_prefix('{')
    else {
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
    text.trim_matches(crate::preprocess::is_js_whitespace)
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
