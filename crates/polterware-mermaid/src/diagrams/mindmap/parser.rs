//! The mindmap grammar as a recursive-descent parser: upstream
//! `packages/mermaid/src/diagrams/mindmap/parser/mindmap.jison`.
//!
//! Every production of the jison file is here. A diagram is the `mindmap` header followed by
//! a `document` of statements, each of which must end with a `stop` — a line break, a comment
//! line (`SPACELINE`), or the end of the input. A statement is an indentation (`SPACELIST`)
//! and a node, an icon, or a class; a node, icon, or class alone; or a bare indentation or
//! comment line, which does nothing.
//!
//! Kept on purpose from the LR grammar:
//! * one newline token may follow the `mindmap` header (`mindMap: MINDMAP document |
//!   MINDMAP NL document`), and `[\s]+[\n]` makes a `SPACELINE` of every whitespace run
//!   that holds a line break with another whitespace character before it — so a blank (or
//!   whitespace-only) line between the header and the first node is a syntax error, while
//!   blank lines after a statement are its `stop`;
//! * the header must start the input: an indented `mindmap` is a `SPACELIST` first, which
//!   `start` does not accept, while blank or comment lines before it are `spaceLines`;
//! * two statements may not share a line, since every one of them ends with a `stop`;
//! * the indentation width is the statement's level, and the first node's width is the base
//!   the others are measured against (`Db::add_node`).

use super::ast::{Ast, NodeType};
use super::db::{Db, node_type as node_type_of};
use super::lexer::{Lexer, Token, TokenKind};
use crate::error::ParseError;

/// Parses the diagram body (see [`crate::preprocess`]).
pub(crate) fn parse(body: &str) -> Result<Ast, ParseError> {
    let mut parser = Parser::new(Lexer::new(body))?;
    parser.run()?;
    Ok(parser.db.into_ast())
}

struct Parser<'a> {
    lexer: Lexer<'a>,
    token: Token,
    db: Db,
}

impl<'a> Parser<'a> {
    fn new(mut lexer: Lexer<'a>) -> Result<Self, ParseError> {
        let token = lexer.next_token()?;
        Ok(Self {
            lexer,
            token,
            db: Db::new(),
        })
    }

    fn bump(&mut self) -> Result<(), ParseError> {
        self.token = self.lexer.next_token()?;
        Ok(())
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

    /// `start: mindMap | spaceLines mindMap`, then `document`.
    fn run(&mut self) -> Result<(), ParseError> {
        while matches!(self.token.kind, TokenKind::SpaceLine | TokenKind::Newline) {
            self.bump()?;
        }
        if !self.at(TokenKind::Mindmap) {
            return Err(self.unexpected("'mindmap'"));
        }
        self.bump()?;
        // `mindMap: MINDMAP NL document`: at most one NL token, which is a single line
        // break, may follow the header.
        if self.at(TokenKind::Newline) {
            self.bump()?;
        }
        self.statement()?;
        loop {
            let mut stopped = false;
            while matches!(self.token.kind, TokenKind::SpaceLine | TokenKind::Newline) {
                self.bump()?;
                stopped = true;
            }
            match self.token.kind {
                TokenKind::Eof => return Ok(()),
                TokenKind::EndOfInput => {
                    return Err(ParseError::at(
                        "Unexpected end of the input",
                        self.token.span,
                    ));
                }
                _ if !stopped => return Err(self.unexpected("a line break")),
                _ => self.statement()?,
            }
        }
    }

    /// `statement`: an optional indentation, then a node, an icon, or a class.
    fn statement(&mut self) -> Result<(), ParseError> {
        if self.at(TokenKind::SpaceList) {
            let level = self.token.text.chars().count() as i32;
            self.bump()?;
            match self.token.kind {
                TokenKind::NodeId | TokenKind::NodeStart => return self.node_statement(level),
                TokenKind::Icon => return self.icon_statement(),
                TokenKind::Class => return self.class_statement(),
                // `statement: SPACELIST` — an indented line with nothing after it.
                _ => return Ok(()),
            }
        }
        match self.token.kind {
            TokenKind::SpaceLine => {
                self.bump()?;
                Ok(())
            }
            TokenKind::NodeId | TokenKind::NodeStart => self.node_statement(0),
            TokenKind::Icon => self.icon_statement(),
            TokenKind::Class => self.class_statement(),
            _ => Err(self.unexpected("a node")),
        }
    }

    fn node_statement(&mut self, level: i32) -> Result<(), ParseError> {
        let span = self.token.span;
        let (id, descr, node_type) = self.node()?;
        self.db
            .add_node(level, &id, &descr, node_type)
            .map_err(|message| ParseError::at(message, span))
    }

    /// `node: nodeWithId | nodeWithoutId`. A node without an id takes its description as
    /// the id, and a node without a description takes its id as the description.
    fn node(&mut self) -> Result<(String, String, NodeType), ParseError> {
        if self.at(TokenKind::NodeId) {
            let id = self.token.text.clone();
            self.bump()?;
            if self.at(TokenKind::NodeStart) {
                let start = self.token.text.clone();
                self.bump()?;
                let descr = self.node_descr()?;
                let end = self.node_end()?;
                Ok((id, descr, node_type_of(&start, &end)))
            } else {
                Ok((id.clone(), id, NodeType::NoBorder))
            }
        } else if self.at(TokenKind::NodeStart) {
            let start = self.token.text.clone();
            self.bump()?;
            let descr = self.node_descr()?;
            let end = self.node_end()?;
            Ok((descr.clone(), descr, node_type_of(&start, &end)))
        } else {
            Err(self.unexpected("a node"))
        }
    }

    fn node_descr(&mut self) -> Result<String, ParseError> {
        if !self.at(TokenKind::NodeDescr) {
            return Err(self.unexpected("a node description"));
        }
        let descr = self.token.text.clone();
        self.bump()?;
        Ok(descr)
    }

    fn node_end(&mut self) -> Result<String, ParseError> {
        if !self.at(TokenKind::NodeEnd) {
            return Err(self.unexpected("the end of the node"));
        }
        let end = self.token.text.clone();
        self.bump()?;
        Ok(end)
    }

    fn icon_statement(&mut self) -> Result<(), ParseError> {
        let span = self.token.span;
        let icon = self.token.text.clone();
        self.bump()?;
        self.db
            .decorate_icon(&icon)
            .map_err(|message| ParseError::at(message, span))
    }

    fn class_statement(&mut self) -> Result<(), ParseError> {
        let span = self.token.span;
        let class = self.token.text.clone();
        self.bump()?;
        self.db
            .decorate_class(&class)
            .map_err(|message| ParseError::at(message, span))
    }
}

fn describe(token: &Token) -> String {
    match token.kind {
        TokenKind::Eof => "the end of the input".to_string(),
        TokenKind::EndOfInput => {
            "the end of the input (no 'EOF' token in this lexer state)".to_string()
        }
        TokenKind::Newline => "a line break".to_string(),
        TokenKind::SpaceLine => "a comment or blank line".to_string(),
        TokenKind::SpaceList => format!("the indentation '{}'", token.text),
        TokenKind::Mindmap => "'mindmap'".to_string(),
        TokenKind::NodeId => format!("'{}'", token.text),
        TokenKind::NodeStart => format!("'{}'", token.text),
        TokenKind::NodeDescr => format!("'{}'", token.text),
        TokenKind::NodeEnd => format!("'{}'", token.text),
        TokenKind::Icon => format!("the icon '{}'", token.text),
        TokenKind::Class => format!("the class '{}'", token.text),
    }
}
