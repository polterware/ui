//! The quadrant chart grammar: a hand-written recursive-descent port of the `%%` block of
//! `parser/quadrant.jison`.
//!
//! ```text
//! start: eol start | SPACE start | QUADRANT document
//! document: | document line
//! line: statement eol
//! statement: | classDefStatement | SPACE statement | axisDetails | quadrantDetails
//!          | points | title title_value | acc_title acc_title_value
//!          | acc_descr acc_descr_value | acc_descr_multiline_value
//! classDefStatement: CLASSDEF SPACE idString SPACE stylesOpt
//! axisDetails: X-AXIS text AXIS-TEXT-DELIMITER text
//!            | X-AXIS text AXIS-TEXT-DELIMITER
//!            | X-AXIS text
//!            | Y-AXIS text AXIS-TEXT-DELIMITER text
//!            | Y-AXIS text AXIS-TEXT-DELIMITER
//!            | Y-AXIS text
//! quadrantDetails: QUADRANT_1 text | QUADRANT_2 text | QUADRANT_3 text | QUADRANT_4 text
//! points: text point_start point_x point_y
//!       | text class_name point_start point_x point_y
//!       | text point_start point_x point_y stylesOpt
//!       | text class_name point_start point_x point_y stylesOpt
//! stylesOpt: style | stylesOpt COMMA style
//! style: styleComponent | style styleComponent
//! idString: idStringToken | idString idStringToken
//! text: alphaNumToken | text textNoTagsToken | STR | MD_STR
//! eol: NEWLINE | SEMI | EOF
//! ```
//!
//! The `text` rule's `type` field (`'text'` or `'markdown'`, from `STR` and `MD_STR`) is not
//! kept: the database drops it, and only the string reaches [`super::ast::Ast`].
//!
//! Two upstream nonterminals are dead in this grammar and are not modelled: `section`, which
//! `statement` lists but no rule defines (jison treats it as a token the lexer never
//! produces), and the `NODE_STRING`, `DOWN`, `DEFAULT`, `UNIT`, `STYLE`, and `PCT` tokens that
//! `idStringToken` and `styleComponent` name but no lexer rule returns.
//!
//! The output is [`ParsedDiagram`] for the single-valued setters, plus the `addPoint` and
//! `addClass` calls the parser hands to its [`Sink`] as it parses. That is what lets the ported
//! parser spec record the same raw text and style strings upstream's mock database records.

use super::db::{Sink, StyleText, trim_js};
use super::lexer::{Lexer, Token, TokenKind};
use crate::error::ParseError;

/// What the grammar parsed, before the database trims, sanitizes, and validates it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ParsedDiagram {
    /// `setDiagramTitle`, already trimmed by the action.
    pub(crate) title: Option<String>,
    /// `setAccTitle`, already trimmed by the action.
    pub(crate) acc_title: Option<String>,
    /// `setAccDescription`, already trimmed by the action.
    pub(crate) acc_description: Option<String>,
    /// `setQuadrant1Text` through `setQuadrant4Text`, untrimmed.
    pub(crate) quadrant_texts: [Option<String>; 4],
    /// `setXAxisLeftText`, untrimmed.
    pub(crate) x_axis_left: Option<String>,
    /// `setXAxisRightText`, untrimmed.
    pub(crate) x_axis_right: Option<String>,
    /// `setYAxisBottomText`, untrimmed.
    pub(crate) y_axis_bottom: Option<String>,
    /// `setYAxisTopText`, untrimmed.
    pub(crate) y_axis_top: Option<String>,
}

/// The arguments of one `addPoint` call: the text object, the class name, the two token texts,
/// and the style strings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ParsedPoint {
    pub(crate) text: String,
    pub(crate) class_name: String,
    pub(crate) x: String,
    pub(crate) y: String,
    pub(crate) styles: Vec<StyleText>,
}

/// The arguments of one `addClass` call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ParsedClass {
    pub(crate) name: String,
    pub(crate) styles: Vec<StyleText>,
}

/// Parses the diagram body, handing each point and class to `sink` as upstream's actions do.
pub(crate) fn parse(body: &str, sink: &mut dyn Sink) -> Result<ParsedDiagram, ParseError> {
    Parser::new(body).parse_diagram(sink)
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
            Ok(token) if token.kind == TokenKind::End => {
                ParseError::at("unexpected end of input", token.span)
            }
            Ok(token) => ParseError::at(
                format!("expected {expected}, got '{}'", token.kind.name()),
                token.span,
            ),
            Err(error) => error,
        }
    }

    /// `eol: NEWLINE | SEMI | EOF`.
    fn expect_eol(&mut self) -> Result<(), ParseError> {
        match self.peek()? {
            TokenKind::Eof => Ok(()),
            TokenKind::Newline | TokenKind::Semi => {
                self.bump()?;
                Ok(())
            }
            _ => Err(self.unexpected("a newline or end of input")),
        }
    }

    /// `start` and `document`.
    fn parse_diagram(mut self, sink: &mut dyn Sink) -> Result<ParsedDiagram, ParseError> {
        // `start: eol start | SPACE start`, until the header.
        while matches!(
            self.peek()?,
            TokenKind::Space | TokenKind::Newline | TokenKind::Semi
        ) {
            self.bump()?;
        }
        self.expect(TokenKind::Quadrant)?;

        let mut parsed = ParsedDiagram::default();
        loop {
            // `statement: SPACE statement`.
            while self.peek()? == TokenKind::Space {
                self.bump()?;
            }
            match self.peek()? {
                TokenKind::Eof => break,
                TokenKind::Newline | TokenKind::Semi => {
                    self.bump()?;
                }
                _ => {
                    self.parse_statement(&mut parsed, sink)?;
                    self.expect_eol()?;
                }
            }
        }
        Ok(parsed)
    }

    /// `statement`, once the leading spaces are gone.
    fn parse_statement(
        &mut self,
        parsed: &mut ParsedDiagram,
        sink: &mut dyn Sink,
    ) -> Result<(), ParseError> {
        match self.peek()? {
            TokenKind::Title => {
                self.bump()?;
                let value = self.expect(TokenKind::TitleValue)?;
                parsed.title = Some(trim_js(&value.text).to_string());
            }
            TokenKind::AccTitle => {
                self.bump()?;
                let value = self.expect(TokenKind::AccTitleValue)?;
                parsed.acc_title = Some(trim_js(&value.text).to_string());
            }
            TokenKind::AccDescr => {
                self.bump()?;
                let value = self.expect(TokenKind::AccDescrValue)?;
                parsed.acc_description = Some(trim_js(&value.text).to_string());
            }
            TokenKind::AccDescrMultilineValue => {
                let value = self.bump()?;
                parsed.acc_description = Some(trim_js(&value.text).to_string());
            }
            TokenKind::ClassDef => sink.add_class(self.parse_class_def()?)?,
            TokenKind::XAxis => self.parse_axis(true, parsed)?,
            TokenKind::YAxis => self.parse_axis(false, parsed)?,
            TokenKind::Quadrant1
            | TokenKind::Quadrant2
            | TokenKind::Quadrant3
            | TokenKind::Quadrant4 => {
                let index = match self.bump()?.kind {
                    TokenKind::Quadrant1 => 0,
                    TokenKind::Quadrant2 => 1,
                    TokenKind::Quadrant3 => 2,
                    _ => 3,
                };
                parsed.quadrant_texts[index] = Some(self.parse_text()?);
            }
            _ => sink.add_point(self.parse_point()?)?,
        }
        Ok(())
    }

    /// `axisDetails`.
    fn parse_axis(&mut self, is_x: bool, parsed: &mut ParsedDiagram) -> Result<(), ParseError> {
        self.bump()?;
        let mut first = self.parse_text()?;
        let mut second = None;
        if self.peek()? == TokenKind::AxisTextDelimiter {
            self.bump()?;
            if is_eol(self.peek()?) {
                // `$2.text += " ⟶ "` in the action of the one-text rule.
                first.push_str(" ⟶ ");
            } else {
                second = Some(self.parse_text()?);
            }
        }
        if is_x {
            parsed.x_axis_left = Some(first);
            if let Some(second) = second {
                parsed.x_axis_right = Some(second);
            }
        } else {
            parsed.y_axis_bottom = Some(first);
            if let Some(second) = second {
                parsed.y_axis_top = Some(second);
            }
        }
        Ok(())
    }

    /// `points`.
    fn parse_point(&mut self) -> Result<ParsedPoint, ParseError> {
        let text = self.parse_text()?;
        let class_name = if self.peek()? == TokenKind::ClassName {
            self.bump()?.text
        } else {
            String::new()
        };
        self.expect(TokenKind::PointStart)?;
        let x = self.expect(TokenKind::PointX)?.text;
        let y = self.expect(TokenKind::PointY)?.text;
        let styles = self.parse_styles_opt()?;
        Ok(ParsedPoint {
            text,
            class_name,
            x,
            y,
            styles,
        })
    }

    /// `classDefStatement`.
    fn parse_class_def(&mut self) -> Result<ParsedClass, ParseError> {
        self.bump()?;
        self.expect(TokenKind::Space)?;
        if !is_id_string_token(self.peek()?) {
            return Err(self.unexpected("a class name"));
        }
        let mut name = String::new();
        while is_id_string_token(self.peek()?) {
            name.push_str(&self.bump()?.text);
        }
        self.expect(TokenKind::Space)?;
        let styles = self.parse_styles_required()?;
        Ok(ParsedClass { name, styles })
    }

    /// `stylesOpt` where the grammar makes it optional (the `points` rules without it).
    fn parse_styles_opt(&mut self) -> Result<Vec<StyleText>, ParseError> {
        if !is_style_component(self.peek()?) {
            return Ok(Vec::new());
        }
        let mut styles = vec![self.parse_style()?];
        while self.peek()? == TokenKind::Comma {
            self.bump()?;
            styles.push(self.parse_style()?);
        }
        Ok(styles)
    }

    /// `stylesOpt` where the grammar requires it (`classDefStatement`).
    fn parse_styles_required(&mut self) -> Result<Vec<StyleText>, ParseError> {
        let mut styles = vec![self.parse_style()?];
        while self.peek()? == TokenKind::Comma {
            self.bump()?;
            styles.push(self.parse_style()?);
        }
        Ok(styles)
    }

    /// `style: styleComponent | style styleComponent`, trimmed by the action.
    fn parse_style(&mut self) -> Result<StyleText, ParseError> {
        let first = self.bump_style_component()?;
        let mut text = first.text;
        let mut end = first.span.end;
        while is_style_component(self.peek()?) {
            let token = self.bump()?;
            text.push_str(&token.text);
            end = token.span.end;
        }
        Ok(StyleText {
            text: trim_js(&text).to_string(),
            span: crate::error::Span {
                line: first.span.line,
                column: first.span.column,
                start: first.span.start,
                end,
            },
        })
    }

    fn bump_style_component(&mut self) -> Result<Token, ParseError> {
        if !is_style_component(self.peek()?) {
            return Err(self.unexpected("a style"));
        }
        self.bump()
    }

    /// `text`, greedy over `textNoTagsToken`.
    fn parse_text(&mut self) -> Result<String, ParseError> {
        let first = match self.peek()? {
            TokenKind::Str | TokenKind::MdStr => self.bump()?,
            kind if is_alpha_num_token(kind) => self.bump()?,
            _ => return Err(self.unexpected("text")),
        };
        let mut text = first.text;
        while is_text_no_tags(self.peek()?) {
            text.push_str(&self.bump()?.text);
        }
        Ok(text)
    }
}

/// `eol`'s terminal set.
fn is_eol(kind: TokenKind) -> bool {
    matches!(kind, TokenKind::Newline | TokenKind::Semi | TokenKind::Eof)
}

/// `alphaNumToken`.
fn is_alpha_num_token(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Punctuation
            | TokenKind::Amp
            | TokenKind::Num
            | TokenKind::Alpha
            | TokenKind::Comma
            | TokenKind::Plus
            | TokenKind::Equals
            | TokenKind::Mult
            | TokenKind::Dot
            | TokenKind::Brkt
            | TokenKind::Underscore
            | TokenKind::UnicodeText
    )
}

/// `textNoTagsToken: alphaNumToken | SPACE | MINUS`.
fn is_text_no_tags(kind: TokenKind) -> bool {
    is_alpha_num_token(kind) || matches!(kind, TokenKind::Space | TokenKind::Minus)
}

/// `styleComponent`'s reachable tokens.
fn is_style_component(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Alpha
            | TokenKind::Num
            | TokenKind::Colon
            | TokenKind::Space
            | TokenKind::Brkt
            | TokenKind::Minus
    )
}

/// `idStringToken`'s reachable tokens.
fn is_id_string_token(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Alpha
            | TokenKind::Num
            | TokenKind::Minus
            | TokenKind::Comma
            | TokenKind::Colon
            | TokenKind::Amp
            | TokenKind::Brkt
            | TokenKind::Mult
            | TokenKind::UnicodeText
    )
}
