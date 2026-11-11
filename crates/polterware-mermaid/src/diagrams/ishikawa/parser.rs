//! The ishikawa grammar as a recursive-descent parser: upstream
//! `packages/mermaid/src/diagrams/ishikawa/parser/ishikawa.jison`, actions included.
//!
//! ```text
//! start      : ishikawa | spaceLines ishikawa
//! spaceLines : SPACELINE | spaceLines SPACELINE | spaceLines NL
//! ishikawa   : ISHIKAWA document | ISHIKAWA NL document
//! document   : statement stop | document statement stop
//! stop       : NL | EOF | SPACELINE | stop NL | stop EOF
//! statement  : SPACELIST TEXT | TEXT | SPACELINE | SPACELIST
//! ```
//!
//! A diagram is the header, an optional single `NL`, then one or more statements, each
//! followed by a `stop`. The header must start the input: blank and comment lines may
//! precede it (`spaceLines`) but an indentation may not. A statement adds a node (an
//! indentation and a line of text, or a line of text alone), or does nothing (a `%%` comment
//! line, a whitespace-only line with no line break before its end, or a bare indentation).
//! The indentation width, in characters, is the node's level.
//!
//! Kept on purpose from the LR grammar:
//!
//! * `ishikawa\n` is an error (one `NL`, then the end of the input where a statement should
//!   start), while `ishikawa\n\n` and `ishikawa\n%% comment\n` parse with no root: two line
//!   breaks are one `SPACELINE` statement;
//! * an empty line between the header and the first statement is a `SPACELINE`, so
//!   `ishikawa\n\n a` fails at ` a`: the `SPACELINE` statement leaves no root, and the
//!   following indentation cannot be a `stop`;
//! * two statements cannot share a line, and a `TEXT` that is not the last line must be
//!   followed by a line break, a comment or a blank line, or the end of the input;
//! * a header followed by text on the same line (`ishikawa Blurry Photo`) makes that text the
//!   first node, at level `0`.
//!
//! Errors carry the [`Span`](crate::error::Span) of the token that could not continue the
//! document; upstream's jison text names its line and token instead.

use super::ast::Ast;
use super::db::Db;
use super::lexer::{Lexer, Token, TokenKind};
use crate::error::ParseError;

/// Parses the diagram body and populates the database.
pub(crate) fn parse(body: &str) -> Result<Ast, ParseError> {
    let mut parser = Parser::new(Lexer::new(body));
    parser.run()?;
    Ok(parser.db.into_ast())
}

struct Parser<'a> {
    lexer: Lexer<'a>,
    token: Token,
    db: Db,
}

impl<'a> Parser<'a> {
    fn new(mut lexer: Lexer<'a>) -> Self {
        let token = lexer.next_token();
        Self {
            lexer,
            token,
            db: Db::new(),
        }
    }

    fn bump(&mut self) {
        self.token = self.lexer.next_token();
    }

    fn at(&self, kind: TokenKind) -> bool {
        self.token.kind == kind
    }

    fn unexpected(&self, expected: &str) -> ParseError {
        ParseError::at(
            format!("Expected {expected}, found {}", describe(&self.token)),
            self.token.span,
        )
    }

    /// `start: ishikawa | spaceLines ishikawa`, then the body.
    fn run(&mut self) -> Result<(), ParseError> {
        while matches!(self.token.kind, TokenKind::SpaceLine | TokenKind::Newline) {
            self.bump();
        }
        if !self.at(TokenKind::Ishikawa) {
            return Err(self.unexpected("'ishikawa'"));
        }
        self.bump();
        // `ishikawa: ISHIKAWA document | ISHIKAWA NL document`: at most one line break token
        // may follow the header, and the document still needs a statement.
        if self.at(TokenKind::Newline) {
            self.bump();
        }
        self.document()
    }

    /// `document: statement stop | document statement stop`.
    fn document(&mut self) -> Result<(), ParseError> {
        loop {
            self.statement()?;
            self.stop()?;
            if self.at(TokenKind::Eof) {
                return Ok(());
            }
        }
    }

    /// `statement`: an optional indentation, then a line of text; or nothing at all.
    fn statement(&mut self) -> Result<(), ParseError> {
        match self.token.kind {
            TokenKind::SpaceList => {
                // `$1.length`: JavaScript counts the UTF-16 units of the indentation, and
                // every whitespace character JavaScript's `\s` matches is in the BMP, so
                // counting `char`s is the same number.
                let level = self.token.text.chars().count();
                self.bump();
                if self.at(TokenKind::Text) {
                    let text = self.take_text();
                    self.db.add_node(level, &text);
                }
                Ok(())
            }
            TokenKind::Text => {
                let text = self.take_text();
                self.db.add_node(0, &text);
                Ok(())
            }
            TokenKind::SpaceLine => {
                self.bump();
                Ok(())
            }
            _ => Err(self.unexpected("a line of text")),
        }
    }

    /// `stop: NL | EOF | SPACELINE | stop NL | stop EOF`: the line break, comment, or blank
    /// line that ends a statement, then any number of line breaks, or the end of the input.
    fn stop(&mut self) -> Result<(), ParseError> {
        match self.token.kind {
            TokenKind::Newline => {
                while self.at(TokenKind::Newline) {
                    self.bump();
                }
                Ok(())
            }
            TokenKind::SpaceLine => {
                self.bump();
                while self.at(TokenKind::Newline) {
                    self.bump();
                }
                Ok(())
            }
            TokenKind::Eof => Ok(()),
            _ => Err(self.unexpected("a line break or the end of the diagram")),
        }
    }

    /// `$2.trim()`: the text token without the whitespace JavaScript's `String.prototype.trim`
    /// removes.
    fn take_text(&mut self) -> String {
        let text = self
            .token
            .text
            .trim_matches(crate::preprocess::is_js_whitespace)
            .to_string();
        self.bump();
        text
    }
}

fn describe(token: &Token) -> String {
    match token.kind {
        TokenKind::Ishikawa => "'ishikawa'".to_string(),
        TokenKind::SpaceLine => "a comment or blank line".to_string(),
        TokenKind::Newline => "a line break".to_string(),
        TokenKind::SpaceList => format!("the indentation '{}'", token.text),
        TokenKind::Text => format!("'{}'", token.text),
        TokenKind::Eof => "the end of the input".to_string(),
    }
}
