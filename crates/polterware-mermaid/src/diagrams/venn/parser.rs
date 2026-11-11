//! The venn grammar: a hand-written recursive-descent port of upstream `parser/venn.jison`.
//!
//! ```text
//! start:            optNewlines VENN document EOF
//! optNewlines:      | optNewlines NEWLINE
//! document:         | document line
//! line:             NEWLINE | statement
//!
//! statement:        TITLE
//!                 | SET identifier labelOpt sizeOpt
//!                 | UNION identifierList labelOpt sizeOpt
//!                 | TEXT identifierList textValue labelOpt
//!                 | INDENT_TEXT indentedTextTail
//!                 | STYLE identifierList styleField (COMMA styleField)*
//!
//! labelOpt:         BRACKET_LABEL | /* empty */
//! sizeOpt:          COLON NUMERIC | /* empty */
//! indentedTextTail: textValue labelOpt
//! textValue:        IDENTIFIER | STRING | NUMERIC
//! styleField:       IDENTIFIER COLON styleValue
//! styleValue:       STRING | valueTokens
//! valueTokens:      valueToken | valueTokens valueToken
//! valueToken:       IDENTIFIER | NUMERIC | HEXCOLOR | RGBCOLOR | RGBACOLOR
//! identifierList:   identifier | identifierList COMMA identifier
//! identifier:       IDENTIFIER | STRING
//! ```
//!
//! The grammar has no statement terminator: `NEWLINE` is just another `line`, so two
//! statements may share a line. Every action is ported where the statement is read. `set` and
//! `union` also call `yy.setIndentMode(true)` (the lexer's indent mode) and record the sets an
//! indented `text` line attaches to; `union` first rejects fewer than two identifiers and then
//! rejects an identifier no earlier `set` declared, with upstream's message text.
//!
//! A `title` statement stores the token text from its seventh character on (`$1.substr(6)`),
//! sanitized by the database like `setDiagramTitle` does. A later title replaces an earlier
//! one, because the database field is single-valued.

use super::ast::Ast;
use super::db::Db;
use super::lexer::{Lexer, Token, TokenKind};
use crate::error::ParseError;

/// Parses the diagram body and populates the database.
pub(crate) fn parse(body: &str) -> Result<Ast, ParseError> {
    Parser::new(body).parse_start()
}

struct Parser<'a> {
    lexer: Lexer<'a>,
    lookahead: Option<Token>,
    db: Db,
}

impl<'a> Parser<'a> {
    fn new(body: &'a str) -> Self {
        Self {
            lexer: Lexer::new(body),
            lookahead: None,
            db: Db::default(),
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

    fn parse_start(mut self) -> Result<Ast, ParseError> {
        while self.peek()? == TokenKind::Newline {
            self.bump()?;
        }
        self.expect(TokenKind::Venn)?;
        self.parse_document()?;
        Ok(self.db.into_ast())
    }

    /// `document` and `line`: statements and line breaks, in any order, without terminators.
    fn parse_document(&mut self) -> Result<(), ParseError> {
        loop {
            match self.peek()? {
                TokenKind::Newline => {
                    self.bump()?;
                }
                TokenKind::Eof => return Ok(()),
                TokenKind::Title => {
                    let token = self.bump()?;
                    self.db.set_diagram_title(title_text(&token.text));
                }
                TokenKind::Set => self.parse_set()?,
                TokenKind::Union => self.parse_union()?,
                TokenKind::Text => self.parse_text()?,
                TokenKind::IndentText => self.parse_indented_text()?,
                TokenKind::Style => self.parse_style()?,
                _ => return Err(self.unexpected("a statement or a newline")),
            }
        }
    }

    fn parse_set(&mut self) -> Result<(), ParseError> {
        self.bump()?;
        let identifier = self.identifier()?;
        let label = self.optional_label()?;
        let size = self.optional_size()?;
        self.db.add_subset(&[identifier], label, size);
        self.lexer.set_indent_mode(true);
        Ok(())
    }

    fn parse_union(&mut self) -> Result<(), ParseError> {
        let keyword = self.bump()?;
        let identifiers = self.identifiers()?;
        let label = self.optional_label()?;
        let size = self.optional_size()?;
        if identifiers.len() < 2 {
            return Err(ParseError::at(
                "union requires multiple identifiers",
                keyword.span,
            ));
        }
        self.db
            .validate_union_identifiers(&identifiers, keyword.span)?;
        self.db.add_subset(&identifiers, label, size);
        self.lexer.set_indent_mode(true);
        Ok(())
    }

    fn parse_text(&mut self) -> Result<(), ParseError> {
        self.bump()?;
        let identifiers = self.identifiers()?;
        let id = self.text_value()?;
        let label = self.optional_label()?;
        self.db.add_text(&identifiers, id, label);
        Ok(())
    }

    fn parse_indented_text(&mut self) -> Result<(), ParseError> {
        let keyword = self.bump()?;
        let id = self.text_value()?;
        let label = self.optional_label()?;
        let Some(sets) = self.db.current_sets() else {
            return Err(ParseError::at("text requires set", keyword.span));
        };
        self.db.add_text(&sets, id, label);
        Ok(())
    }

    fn parse_style(&mut self) -> Result<(), ParseError> {
        self.bump()?;
        let identifiers = self.identifiers()?;
        let mut styles = vec![self.style_field()?];
        while self.peek()? == TokenKind::Comma {
            self.bump()?;
            styles.push(self.style_field()?);
        }
        self.db.add_style(&identifiers, styles);
        Ok(())
    }

    /// `identifier` and `identifierList`: one name, then comma-separated names.
    fn identifiers(&mut self) -> Result<Vec<String>, ParseError> {
        let mut identifiers = vec![self.identifier()?];
        while self.peek()? == TokenKind::Comma {
            self.bump()?;
            identifiers.push(self.identifier()?);
        }
        Ok(identifiers)
    }

    fn identifier(&mut self) -> Result<String, ParseError> {
        match self.peek()? {
            TokenKind::Identifier | TokenKind::Str => Ok(self.bump()?.text),
            _ => Err(self.unexpected("an identifier")),
        }
    }

    /// `IDENTIFIER | STRING | NUMERIC`, the id of a text node, kept as written.
    fn text_value(&mut self) -> Result<String, ParseError> {
        match self.peek()? {
            TokenKind::Identifier | TokenKind::Str | TokenKind::Numeric => Ok(self.bump()?.text),
            _ => Err(self.unexpected("a text value")),
        }
    }

    fn optional_label(&mut self) -> Result<Option<String>, ParseError> {
        if self.peek()? == TokenKind::BracketLabel {
            Ok(Some(self.bump()?.text))
        } else {
            Ok(None)
        }
    }

    fn optional_size(&mut self) -> Result<Option<f64>, ParseError> {
        if self.peek()? != TokenKind::Colon {
            return Ok(None);
        }
        self.bump()?;
        if self.peek()? != TokenKind::Numeric {
            return Err(self.unexpected("a number"));
        }
        let token = self.bump()?;
        let size = token
            .text
            .parse::<f64>()
            .map_err(|_| ParseError::at(format!("invalid number '{}'", token.text), token.span))?;
        Ok(Some(size))
    }

    fn style_field(&mut self) -> Result<(String, String), ParseError> {
        let key = match self.peek()? {
            TokenKind::Identifier => self.bump()?.text,
            _ => return Err(self.unexpected("a style property")),
        };
        self.expect(TokenKind::Colon)?;
        let value = self.style_value()?;
        Ok((key, value))
    }

    /// `styleValue`: a quoted string, or a run of value tokens joined by one space.
    fn style_value(&mut self) -> Result<String, ParseError> {
        if self.peek()? == TokenKind::Str {
            return Ok(self.bump()?.text);
        }
        let mut tokens = Vec::new();
        while matches!(
            self.peek()?,
            TokenKind::Identifier
                | TokenKind::Numeric
                | TokenKind::HexColor
                | TokenKind::RgbColor
                | TokenKind::RgbaColor
        ) {
            tokens.push(self.bump()?.text);
        }
        if tokens.is_empty() {
            return Err(self.unexpected("a style value"));
        }
        Ok(tokens.join(" "))
    }
}

/// `$1.substr(6)`: drop `title` and the one whitespace character the lexer required after it.
fn title_text(raw: &str) -> String {
    raw.chars().skip("title".len() + 1).collect()
}
