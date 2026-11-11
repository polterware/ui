//! The xychart grammar: a hand-written recursive-descent port of `parser/xychart.jison`.
//!
//! ```text
//! start      : eol start | XYCHART orientation? start | document
//! document   : statement*
//! statement  : title text
//!            | X_AXIS text? (bandData | NUMBER ">" NUMBER)?
//!            | Y_AXIS text? (NUMBER ">" NUMBER)?
//!            | (LINE | BAR) text? plotData
//!            | acc_title acc_title_value
//!            | acc_descr acc_descr_value
//!            | acc_descr_multiline_value
//!            | eol
//! bandData   : "[" text ("," text)* "]"
//! plotData   : "[" dataPoint ("," dataPoint)* "]"
//! dataPoint  : NUMBER STR?
//! text       : alphaNum | STR | MD_STR
//! eol        : NEWLINE | SEMI | EOF
//! ```
//!
//! Jison's LALR table decides statement boundaries with one token of lookahead, which the
//! grammar leaves unambiguous: `text` greedily takes adjacent `alphaNumToken`s (the
//! `alphaNum alphaNumToken` concatenation), a `NUMBER_WITH_DECIMAL` never starts a `text`, and
//! `[` after an axis always starts its data. Statements do not need a newline between them.
//!
//! The parser keeps the grammar actions' work only (`setDiagramTitle($text.text.trim())`, the
//! accessibility trims); everything else is the database's, applied in statement order by
//! [`super::db`].

use super::db;
use super::lexer::{Lexer, Token, TokenKind};
use crate::error::ParseError;
use crate::preprocess::is_js_whitespace;

/// Parses the diagram body and populates the database.
pub(crate) fn parse(body: &str) -> Result<super::Ast, ParseError> {
    let statements = Parser::new(body).parse()?;
    Ok(db::build(statements))
}

/// One grammar action, in source order.
pub(crate) enum Statement {
    /// `chartConfig: CHART_ORIENTATION`.
    Orientation(String),
    /// `title text`.
    Title(String),
    /// `acc_title acc_title_value`.
    AccTitle(String),
    /// `acc_descr acc_descr_value` and `acc_descr_multiline_value`.
    AccDescription(String),
    /// `parseXAxis`'s `setXAxisTitle`.
    XAxisTitle(String),
    /// `xAxisData`'s `setXAxisRangeData`.
    XAxisRange(f64, f64),
    /// `bandData`'s `setXAxisBand`.
    XAxisBand(Vec<String>),
    /// `parseYAxis`'s `setYAxisTitle`.
    YAxisTitle(String),
    /// `yAxisData`'s `setYAxisRangeData`.
    YAxisRange(f64, f64),
    /// `LINE` and its plot.
    Line {
        title: String,
        points: Vec<ParsedPoint>,
    },
    /// `BAR` and its plot.
    Bar {
        title: String,
        points: Vec<ParsedPoint>,
    },
}

/// A `dataPoint`: `{ value: Number($1), label: $2 }`.
pub(crate) struct ParsedPoint {
    pub(crate) value: f64,
    pub(crate) label: String,
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

    /// `start`: leading `eol`s and `XYCHART [CHART_ORIENTATION]` headers, then the document.
    fn parse(mut self) -> Result<Vec<Statement>, ParseError> {
        let mut statements = Vec::new();
        loop {
            match self.peek()? {
                TokenKind::Newline | TokenKind::Semi => {
                    self.bump()?;
                }
                TokenKind::Xychart => {
                    self.bump()?;
                    if self.peek()? == TokenKind::ChartOrientation {
                        let orientation = self.bump()?;
                        statements.push(Statement::Orientation(orientation.text));
                    }
                }
                _ => break,
            }
        }

        loop {
            match self.peek()? {
                TokenKind::Eof => break,
                TokenKind::Newline | TokenKind::Semi => {
                    self.bump()?;
                }
                TokenKind::Title => {
                    self.bump()?;
                    let text = self
                        .parse_text()?
                        .ok_or_else(|| self.unexpected("a title text"))?;
                    statements.push(Statement::Title(trim(&text).to_string()));
                }
                TokenKind::AccTitle => {
                    self.bump()?;
                    let value = self.expect(TokenKind::AccTitleValue)?;
                    statements.push(Statement::AccTitle(trim(&value.text).to_string()));
                }
                TokenKind::AccDescr => {
                    self.bump()?;
                    let value = self.expect(TokenKind::AccDescrValue)?;
                    statements.push(Statement::AccDescription(trim(&value.text).to_string()));
                }
                TokenKind::AccDescrMultilineValue => {
                    let value = self.bump()?;
                    statements.push(Statement::AccDescription(trim(&value.text).to_string()));
                }
                TokenKind::XAxis => {
                    self.bump()?;
                    self.parse_x_axis(&mut statements)?;
                }
                TokenKind::YAxis => {
                    self.bump()?;
                    self.parse_y_axis(&mut statements)?;
                }
                TokenKind::Line | TokenKind::Bar => {
                    let kind = self.bump()?.kind;
                    let title = self.parse_text()?.unwrap_or_default();
                    let points = self.parse_plot_data()?;
                    statements.push(match kind {
                        TokenKind::Line => Statement::Line { title, points },
                        _ => Statement::Bar { title, points },
                    });
                }
                _ => {
                    return Err(
                        self.unexpected("a title, an axis, a plot, or an accessibility line")
                    );
                }
            }
        }
        Ok(statements)
    }

    /// `parseXAxis`: `setXAxisTitle` for the text (empty when it is omitted) and then the data.
    fn parse_x_axis(&mut self, statements: &mut Vec<Statement>) -> Result<(), ParseError> {
        let title = self.parse_text()?;
        match self.peek()? {
            TokenKind::SquareBracesStart => {
                statements.push(Statement::XAxisTitle(title.unwrap_or_default()));
                self.bump()?;
                let categories = self.parse_band_data()?;
                statements.push(Statement::XAxisBand(categories));
            }
            TokenKind::Number => {
                statements.push(Statement::XAxisTitle(title.unwrap_or_default()));
                let (min, max) = self.parse_range()?;
                statements.push(Statement::XAxisRange(min, max));
            }
            _ => match title {
                Some(title) => statements.push(Statement::XAxisTitle(title)),
                None => return Err(self.unexpected("an axis title or axis data")),
            },
        }
        Ok(())
    }

    /// `parseYAxis`: the y axis has no band data, only `NUMBER ARROW_DELIMITER NUMBER`.
    fn parse_y_axis(&mut self, statements: &mut Vec<Statement>) -> Result<(), ParseError> {
        let title = self.parse_text()?;
        match self.peek()? {
            TokenKind::Number => {
                statements.push(Statement::YAxisTitle(title.unwrap_or_default()));
                let (min, max) = self.parse_range()?;
                statements.push(Statement::YAxisRange(min, max));
            }
            _ => match title {
                Some(title) => statements.push(Statement::YAxisTitle(title)),
                None => return Err(self.unexpected("an axis title or axis data")),
            },
        }
        Ok(())
    }

    /// `NUMBER_WITH_DECIMAL ARROW_DELIMITER NUMBER_WITH_DECIMAL`.
    fn parse_range(&mut self) -> Result<(f64, f64), ParseError> {
        let min = self.parse_number()?;
        self.expect(TokenKind::ArrowDelimiter)?;
        let max = self.parse_number()?;
        Ok((min, max))
    }

    /// `commaSeparatedTexts`, with the closing bracket expected after the last text.
    fn parse_band_data(&mut self) -> Result<Vec<String>, ParseError> {
        let mut categories = vec![
            self.parse_text()?
                .ok_or_else(|| self.unexpected("an axis category"))?,
        ];
        loop {
            match self.peek()? {
                TokenKind::Comma => {
                    self.bump()?;
                    let category = self
                        .parse_text()?
                        .ok_or_else(|| self.unexpected("an axis category"))?;
                    categories.push(category);
                }
                TokenKind::SquareBracesEnd => {
                    self.bump()?;
                    return Ok(categories);
                }
                _ => return Err(self.unexpected("a comma or the end of the axis categories")),
            }
        }
    }

    /// `plotData`: `SQUARE_BRACES_START dataPoints SQUARE_BRACES_END`, at least one point.
    fn parse_plot_data(&mut self) -> Result<Vec<ParsedPoint>, ParseError> {
        self.expect(TokenKind::SquareBracesStart)?;
        let mut points = Vec::new();
        loop {
            let value = self.parse_number()?;
            let label = if self.peek()? == TokenKind::Str {
                self.bump()?.text
            } else {
                String::new()
            };
            points.push(ParsedPoint { value, label });
            match self.peek()? {
                TokenKind::Comma => {
                    self.bump()?;
                }
                TokenKind::SquareBracesEnd => {
                    self.bump()?;
                    return Ok(points);
                }
                _ => return Err(self.unexpected("a comma or the end of the plot data")),
            }
        }
    }

    fn parse_number(&mut self) -> Result<f64, ParseError> {
        let token = self.expect(TokenKind::Number)?;
        token
            .text
            .parse::<f64>()
            .map_err(|_| ParseError::at(format!("invalid number '{}'", token.text), token.span))
    }

    /// `text`: one `alphaNum` run, or one `STR`/`MD_STR`. `None` when no text token starts.
    fn parse_text(&mut self) -> Result<Option<String>, ParseError> {
        let kind = self.peek()?;
        if matches!(kind, TokenKind::Str | TokenKind::MdStr) {
            return Ok(Some(self.bump()?.text));
        }
        if !is_alpha_num_token(kind) {
            return Ok(None);
        }
        let mut text = String::new();
        while is_alpha_num_token(self.peek()?) {
            text.push_str(&self.bump()?.text);
        }
        Ok(Some(text))
    }
}

/// `alphaNumToken: AMP | NUM | ALPHA | PLUS | EQUALS | MULT | DOT | BRKT | MINUS | UNDERSCORE`.
fn is_alpha_num_token(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Amp
            | TokenKind::Num
            | TokenKind::Alpha
            | TokenKind::Plus
            | TokenKind::Equals
            | TokenKind::Mult
            | TokenKind::Dot
            | TokenKind::Brkt
            | TokenKind::Minus
            | TokenKind::Underscore
    )
}

/// `String.prototype.trim`, close enough: JavaScript's `\s` also has U+FEFF.
fn trim(text: &str) -> &str {
    text.trim_matches(is_js_whitespace)
}
