//! The kanban grammar: a hand-written recursive-descent port of the `%%` grammar in
//! `packages/mermaid/src/diagrams/kanban/parser/kanban.jison`.
//!
//! ```text
//! start:
//!   mindMap
//! | spaceLines mindMap
//! spaceLines:
//!   SPACELINE
//! | spaceLines SPACELINE
//! | spaceLines NL
//! mindMap:
//!   KANBAN document
//! | KANBAN NL document
//! stop:
//!   NL | EOF | SPACELINE
//! | stop NL | stop EOF
//! document:
//!   document statement stop
//! | statement stop
//! statement:
//!   SPACELIST node shapeData | SPACELIST node | SPACELIST ICON | SPACELIST CLASS
//! | SPACELINE
//! | node shapeData | node | ICON | CLASS | SPACELIST
//! node:
//!   nodeWithId | nodeWithoutId
//! nodeWithoutId:
//!   NODE_DSTART NODE_DESCR NODE_DEND
//! nodeWithId:
//!   NODE_ID
//! | NODE_ID NODE_DSTART NODE_DESCR NODE_DEND
//! shapeData:
//!   shapeData SHAPE_DATA
//! | SHAPE_DATA
//! ```
//!
//! The grammar is line oriented: `stop` is the separator after every statement, and it is the
//! reason a `SPACELINE` (a whitespace run that carries its line break) can be a statement on
//! its own but cannot be followed directly by another statement without an `NL`/`SPACELINE`
//! of its own. The port keeps that shape rule for rule instead of reading line by line, so
//! every input upstream accepts or rejects is accepted or rejected here.
//!
//! An indentation (`SPACELIST.length`) is the level `addNode` receives; a node written without
//! a shape has its id as its description, and `nodeWithoutId` uses the description for both.

use super::ast::NodeType;
use super::lexer::{Lexer, Token, TokenKind};
use crate::error::{ParseError, Span};

/// Parses the diagram body into the statements the database applies in order.
pub(crate) fn parse(body: &str) -> Result<ParsedKanban, ParseError> {
    Parser::new(body).parse_document()
}

/// The statements of a document, in grammar order.
pub(crate) struct ParsedKanban {
    pub(crate) statements: Vec<ParsedStatement>,
}

/// One statement the grammar reduced.
pub(crate) enum ParsedStatement {
    /// `SPACELIST? node shapeData?`.
    Node {
        level: usize,
        node: ParsedNode,
        shape_data: Option<ParsedShapeData>,
    },
    /// `SPACELIST? ICON`.
    Icon { icon: String, span: Span },
    /// `SPACELIST? CLASS`.
    Class { class: String, span: Span },
    /// `SPACELINE`, or a `SPACELIST` alone.
    Blank,
}

/// A `node` rule.
pub(crate) struct ParsedNode {
    pub(crate) id: String,
    pub(crate) description: String,
    pub(crate) node_type: NodeType,
    pub(crate) span: Span,
}

/// A `shapeData` rule: the concatenated `SHAPE_DATA` text of one `@{ ... }` block.
pub(crate) struct ParsedShapeData {
    pub(crate) text: String,
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

    /// `start: mindMap | spaceLines mindMap` and
    /// `mindMap: KANBAN document | KANBAN NL document`.
    fn parse_document(mut self) -> Result<ParsedKanban, ParseError> {
        while matches!(self.peek()?, TokenKind::SpaceLine | TokenKind::Newline) {
            self.bump()?;
        }
        self.expect(TokenKind::Kanban)?;
        if self.peek()? == TokenKind::Newline {
            self.bump()?;
        }

        let mut statements = Vec::new();
        loop {
            if !starts_statement(self.peek()?) {
                return Err(self.unexpected("a kanban item"));
            }
            statements.push(self.parse_statement()?);
            self.parse_stop()?;
            if self.peek()? == TokenKind::Eof {
                break;
            }
        }
        Ok(ParsedKanban { statements })
    }

    /// `stop: NL | EOF | SPACELINE | stop NL | stop EOF`: one of those tokens, then any run of
    /// `NL`. The end of input satisfies the rule without being consumed; the document loop
    /// ends on it.
    fn parse_stop(&mut self) -> Result<(), ParseError> {
        match self.peek()? {
            // The caller stops on the end of input, so `EOF` is already a satisfied stop.
            TokenKind::Eof => Ok(()),
            TokenKind::Newline | TokenKind::SpaceLine => {
                self.bump()?;
                while self.peek()? == TokenKind::Newline {
                    self.bump()?;
                }
                Ok(())
            }
            _ => Err(self.unexpected("a newline, a blank line, or end of input")),
        }
    }

    fn parse_statement(&mut self) -> Result<ParsedStatement, ParseError> {
        let level = if self.peek()? == TokenKind::SpaceList {
            let spaces = self.bump()?;
            Some(spaces.text.chars().count())
        } else {
            None
        };
        match self.peek()? {
            TokenKind::SpaceLine => {
                self.bump()?;
                Ok(ParsedStatement::Blank)
            }
            TokenKind::NodeId | TokenKind::NodeDStart => {
                let node = self.parse_node()?;
                let shape_data = if self.peek()? == TokenKind::ShapeData {
                    Some(self.parse_shape_data()?)
                } else {
                    None
                };
                Ok(ParsedStatement::Node {
                    level: level.unwrap_or(0),
                    node,
                    shape_data,
                })
            }
            TokenKind::Icon => {
                let token = self.bump()?;
                Ok(ParsedStatement::Icon {
                    icon: token.text,
                    span: token.span,
                })
            }
            TokenKind::Class => {
                let token = self.bump()?;
                Ok(ParsedStatement::Class {
                    class: token.text,
                    span: token.span,
                })
            }
            // `statement: SPACELIST` — the whitespace of a line with nothing else on it.
            TokenKind::Eof => Ok(ParsedStatement::Blank),
            TokenKind::Newline => Ok(ParsedStatement::Blank),
            _ => Err(self.unexpected("a kanban item")),
        }
    }

    /// `node: nodeWithId | nodeWithoutId`.
    fn parse_node(&mut self) -> Result<ParsedNode, ParseError> {
        if self.peek()? == TokenKind::NodeId {
            let id = self.bump()?;
            if self.peek()? != TokenKind::NodeDStart {
                return Ok(ParsedNode {
                    id: id.text.clone(),
                    description: id.text,
                    node_type: NodeType::NoBorder,
                    span: id.span,
                });
            }
            let (description, node_type) = self.parse_description()?;
            return Ok(ParsedNode {
                id: id.text,
                description,
                node_type,
                span: id.span,
            });
        }
        let start = self.expect(TokenKind::NodeDStart)?;
        let description = self.expect(TokenKind::NodeDDescr)?;
        let end = self.expect(TokenKind::NodeDEnd)?;
        Ok(ParsedNode {
            id: description.text.clone(),
            description: description.text,
            node_type: get_type(&start.text, &end.text),
            span: start.span,
        })
    }

    /// `NODE_DSTART NODE_DESCR NODE_DEND`, after a `NODE_ID`.
    fn parse_description(&mut self) -> Result<(String, NodeType), ParseError> {
        let start = self.expect(TokenKind::NodeDStart)?;
        let description = self.expect(TokenKind::NodeDDescr)?;
        let end = self.expect(TokenKind::NodeDEnd)?;
        Ok((description.text, get_type(&start.text, &end.text)))
    }

    /// `shapeData: shapeData SHAPE_DATA | SHAPE_DATA`.
    fn parse_shape_data(&mut self) -> Result<ParsedShapeData, ParseError> {
        let first = self.expect(TokenKind::ShapeData)?;
        let mut text = first.text;
        let mut end = first.span.end;
        while self.peek()? == TokenKind::ShapeData {
            let token = self.bump()?;
            text.push_str(&token.text);
            end = token.span.end;
        }
        Ok(ParsedShapeData {
            text,
            span: Span {
                line: first.span.line,
                column: first.span.column,
                start: first.span.start,
                end,
            },
        })
    }
}

fn starts_statement(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::SpaceList
            | TokenKind::NodeId
            | TokenKind::NodeDStart
            | TokenKind::Icon
            | TokenKind::Class
            | TokenKind::SpaceLine
    )
}

/// `getType(startStr, endStr)`: the node type the delimiters name.
fn get_type(start: &str, end: &str) -> NodeType {
    match start {
        "[" => NodeType::Rect,
        "(" => {
            if end == ")" {
                NodeType::RoundedRect
            } else {
                NodeType::Cloud
            }
        }
        "((" => NodeType::Circle,
        ")" => NodeType::Cloud,
        "))" => NodeType::Bang,
        "{{" => NodeType::Hexagon,
        _ => NodeType::NoBorder,
    }
}
