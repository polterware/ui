//! The state grammar (`parser/stateDiagram.jison`) as a recursive-descent parser over
//! [`Lexer`].
//!
//! The jison grammar is left-recursive through `document: document line`; here a loop over
//! `parse_statement` builds the same document, and a nested `{ ... }` parses its own until
//! `STRUCT_STOP`. Statement values mirror the grammar actions: the objects the database's
//! `extract` reads, and the bare token values its default actions leave in the document.

use super::ast::{
    Direction, Note, NotePosition, RelationStatement, StateKind, StateStatement, Statement,
};
use super::lexer::{Lexer, Token, TokenKind};
use super::sanitize;
use crate::error::ParseError;

/// What the parser leaves behind: the root document and the accessibility fields, which
/// the grammar sets through `setAccTitle` / `setAccDescription` while parsing.
pub(crate) struct Parsed {
    pub root_doc: Vec<Statement>,
    pub acc_title: String,
    pub acc_description: String,
}

pub(crate) fn parse_source(body: &str) -> Result<Parsed, ParseError> {
    Parser::new(body).run()
}

struct Parser<'a> {
    lexer: Lexer<'a>,
    lookahead: Option<Token>,
    acc_title: String,
    acc_description: String,
    divider_count: u32,
}

impl<'a> Parser<'a> {
    fn new(body: &'a str) -> Self {
        Self {
            lexer: Lexer::new(body),
            lookahead: None,
            acc_title: String::new(),
            acc_description: String::new(),
            divider_count: 0,
        }
    }

    fn run(mut self) -> Result<Parsed, ParseError> {
        loop {
            match self.peek()?.kind {
                TokenKind::Newline => {
                    self.bump()?;
                }
                TokenKind::Sd => {
                    self.bump()?;
                    break;
                }
                _ => return Err(unexpected(self.peek()?)),
            }
        }
        let root_doc = self.parse_document(false)?;
        let token = self.bump()?;
        if token.kind != TokenKind::Eof {
            return Err(unexpected(&token));
        }
        Ok(Parsed {
            root_doc,
            acc_title: self.acc_title,
            acc_description: self.acc_description,
        })
    }

    fn fill(&mut self) -> Result<(), ParseError> {
        if self.lookahead.is_none() {
            self.lookahead = Some(self.lexer.next_token()?);
        }
        Ok(())
    }

    fn peek(&mut self) -> Result<&Token, ParseError> {
        self.fill()?;
        Ok(self.lookahead.as_ref().expect("filled"))
    }

    fn peek_kind(&mut self) -> Result<TokenKind, ParseError> {
        Ok(self.peek()?.kind)
    }

    fn bump(&mut self) -> Result<Token, ParseError> {
        self.fill()?;
        Ok(self.lookahead.take().expect("filled"))
    }

    fn expect(&mut self, kind: TokenKind) -> Result<Token, ParseError> {
        let token = self.bump()?;
        if token.kind == kind {
            Ok(token)
        } else {
            Err(unexpected(&token))
        }
    }

    fn parse_document(&mut self, in_struct: bool) -> Result<Vec<Statement>, ParseError> {
        let mut doc = Vec::new();
        loop {
            match self.peek_kind()? {
                TokenKind::Eof => break,
                TokenKind::Newline => {
                    self.bump()?;
                }
                TokenKind::StructStop if in_struct => break,
                _ => {
                    let statement = self.parse_statement()?;
                    doc.push(statement);
                }
            }
        }
        Ok(doc)
    }

    fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        match self.peek_kind()? {
            TokenKind::ClassDef => self.parse_class_def(),
            TokenKind::Style => self.parse_style(),
            TokenKind::Class => self.parse_apply_class(),
            TokenKind::HideEmpty => Ok(Statement::Other(self.bump()?.text)),
            TokenKind::Scale => {
                let scale = self.bump()?;
                self.expect(TokenKind::Width)?;
                Ok(Statement::Other(scale.text))
            }
            TokenKind::Note => self.parse_note(),
            TokenKind::Fork => self.parse_shape(StateKind::Fork),
            TokenKind::Join => self.parse_shape(StateKind::Join),
            TokenKind::Choice => self.parse_shape(StateKind::Choice),
            TokenKind::Concurrent => {
                self.bump()?;
                self.divider_count += 1;
                Ok(Statement::State(StateStatement {
                    id: format!("divider-id-{}", self.divider_count),
                    state_type: Some(StateKind::Divider),
                    description: Vec::new(),
                    doc: None,
                    note: None,
                    classes: Vec::new(),
                    start: None,
                }))
            }
            TokenKind::AccTitle => {
                self.bump()?;
                let value = self.expect(TokenKind::AccTitleValue)?;
                let value = value.text.trim().to_string();
                self.acc_title = sanitize::set_acc_title(&value);
                Ok(Statement::Other(value))
            }
            TokenKind::AccDescr => {
                self.bump()?;
                let value = self.expect(TokenKind::AccDescrValue)?;
                let value = value.text.trim().to_string();
                self.acc_description = sanitize::set_acc_description(&value);
                Ok(Statement::Other(value))
            }
            TokenKind::AccDescrMultilineValue => {
                let value = self.bump()?.text.trim().to_string();
                self.acc_description = sanitize::set_acc_description(&value);
                Ok(Statement::Other(value))
            }
            TokenKind::Click => self.parse_click(),
            TokenKind::Id | TokenKind::EdgeState => self.parse_id_statement_line(),
            TokenKind::CompositState => self.parse_composit_state(),
            TokenKind::StateDescr => self.parse_state_descr(),
            TokenKind::DirectionTb
            | TokenKind::DirectionBt
            | TokenKind::DirectionRl
            | TokenKind::DirectionLr => self.parse_direction(),
            _ => Err(unexpected(self.peek()?)),
        }
    }

    fn parse_shape(&mut self, kind: StateKind) -> Result<Statement, ParseError> {
        let token = self.bump()?;
        Ok(Statement::State(StateStatement {
            id: token.text,
            state_type: Some(kind),
            description: Vec::new(),
            doc: None,
            note: None,
            classes: Vec::new(),
            start: None,
        }))
    }

    fn parse_note(&mut self) -> Result<Statement, ParseError> {
        let note_token = self.bump()?;
        match self.peek_kind()? {
            TokenKind::LeftOf | TokenKind::RightOf => {
                let position = if self.bump()?.kind == TokenKind::LeftOf {
                    NotePosition::LeftOf
                } else {
                    NotePosition::RightOf
                };
                let id = self.expect(TokenKind::Id)?;
                let text = self.expect(TokenKind::NoteText)?;
                Ok(Statement::State(StateStatement {
                    id: id.text.trim().to_string(),
                    state_type: None,
                    description: Vec::new(),
                    doc: None,
                    note: Some(Note {
                        position: Some(position),
                        text: text.text.trim().to_string(),
                    }),
                    classes: Vec::new(),
                    start: None,
                }))
            }
            TokenKind::NoteText => {
                self.bump()?;
                self.expect(TokenKind::As)?;
                self.expect(TokenKind::Id)?;
                Ok(Statement::Other(note_token.text))
            }
            _ => Err(unexpected(self.peek()?)),
        }
    }

    fn parse_click(&mut self) -> Result<Statement, ParseError> {
        self.bump()?;
        let id = self.parse_id_statement()?;
        if self.peek_kind()? == TokenKind::String {
            let url = self.bump()?;
            let tooltip = self.expect(TokenKind::String)?;
            self.expect(TokenKind::Newline)?;
            Ok(Statement::Click {
                id: id.id,
                url: url.text,
                tooltip: tooltip.text,
            })
        } else {
            self.expect(TokenKind::Href)?;
            let url = self.expect(TokenKind::String)?;
            self.expect(TokenKind::Newline)?;
            Ok(Statement::Click {
                id: id.id,
                url: url.text,
                tooltip: String::new(),
            })
        }
    }

    fn parse_class_def(&mut self) -> Result<Statement, ParseError> {
        self.bump()?;
        let id = match self.peek_kind()? {
            TokenKind::DefaultClassDefId => self.bump()?.text,
            TokenKind::ClassDefId => self.bump()?.text,
            _ => return Err(unexpected(self.peek()?)),
        };
        let classes = self.expect(TokenKind::ClassDefStyleOptions)?.text;
        Ok(Statement::ClassDef {
            id: id.trim().to_string(),
            classes: classes.trim().to_string(),
        })
    }

    fn parse_style(&mut self) -> Result<Statement, ParseError> {
        self.bump()?;
        let id = self.expect(TokenKind::StyleIds)?.text;
        let style_class = self.expect(TokenKind::StyleDefStyleOptions)?.text;
        Ok(Statement::Style {
            id: id.trim().to_string(),
            style_class: style_class.trim().to_string(),
        })
    }

    fn parse_apply_class(&mut self) -> Result<Statement, ParseError> {
        self.bump()?;
        let id = self.expect(TokenKind::ClassEntityIds)?.text;
        let style_class = self.expect(TokenKind::StyleClass)?.text;
        Ok(Statement::ApplyClass {
            id: id.trim().to_string(),
            style_class: style_class.trim().to_string(),
        })
    }

    fn parse_id_statement_line(&mut self) -> Result<Statement, ParseError> {
        let first = self.parse_id_statement()?;
        if self.peek_kind()? == TokenKind::Arrow {
            self.bump()?;
            let second = self.parse_id_statement()?;
            let description = if self.peek_kind()? == TokenKind::Descr {
                Some(trim_colon(&self.bump()?.text))
            } else {
                None
            };
            Ok(Statement::Relation(RelationStatement {
                state1: first,
                state2: second,
                description,
            }))
        } else if self.peek_kind()? == TokenKind::Descr {
            let mut state = first;
            state.description = vec![trim_colon(&self.bump()?.text)];
            Ok(Statement::State(state))
        } else {
            Ok(Statement::State(first))
        }
    }

    fn parse_id_statement(&mut self) -> Result<StateStatement, ParseError> {
        let token = self.bump()?;
        let id = token.text.trim().to_string();
        let mut classes = Vec::new();
        if self.peek_kind()? == TokenKind::StyleSeparator {
            self.bump()?;
            classes.push(self.expect(TokenKind::Id)?.text.trim().to_string());
        }
        Ok(StateStatement {
            id,
            state_type: Some(StateKind::Default),
            description: Vec::new(),
            doc: None,
            note: None,
            classes,
            start: None,
        })
    }

    fn parse_composit_state(&mut self) -> Result<Statement, ParseError> {
        let token = self.bump()?;
        if self.peek_kind()? == TokenKind::StructStart {
            self.bump()?;
            let doc = self.parse_document(true)?;
            self.expect(TokenKind::StructStop)?;
            Ok(Statement::State(StateStatement {
                id: token.text,
                state_type: Some(StateKind::Default),
                description: Vec::new(),
                doc: Some(doc),
                note: None,
                classes: Vec::new(),
                start: None,
            }))
        } else {
            Ok(Statement::Other(token.text))
        }
    }

    fn parse_state_descr(&mut self) -> Result<Statement, ParseError> {
        let descr = self.bump()?.text;
        self.expect(TokenKind::As)?;
        let id = self.expect(TokenKind::Id)?.text;
        if self.peek_kind()? == TokenKind::StructStart {
            self.bump()?;
            let doc = self.parse_document(true)?;
            self.expect(TokenKind::StructStop)?;
            Ok(Statement::State(StateStatement {
                id,
                state_type: Some(StateKind::Default),
                description: vec![descr],
                doc: Some(doc),
                note: None,
                classes: Vec::new(),
                start: None,
            }))
        } else {
            let description = descr.trim().to_string();
            if let Some((id, extra)) = id.split_once(':') {
                Ok(Statement::State(StateStatement {
                    id: id.to_string(),
                    state_type: Some(StateKind::Default),
                    description: vec![description, extra.to_string()],
                    doc: None,
                    note: None,
                    classes: Vec::new(),
                    start: None,
                }))
            } else {
                Ok(Statement::State(StateStatement {
                    id,
                    state_type: Some(StateKind::Default),
                    description: vec![description],
                    doc: None,
                    note: None,
                    classes: Vec::new(),
                    start: None,
                }))
            }
        }
    }

    fn parse_direction(&mut self) -> Result<Statement, ParseError> {
        let direction = match self.bump()?.kind {
            TokenKind::DirectionTb => Direction::Tb,
            TokenKind::DirectionBt => Direction::Bt,
            TokenKind::DirectionRl => Direction::Rl,
            TokenKind::DirectionLr => Direction::Lr,
            _ => unreachable!("checked by the caller"),
        };
        Ok(Statement::Direction(direction))
    }
}

/// `trimColon`: strip one leading colon, then trim.
fn trim_colon(text: &str) -> String {
    match text.strip_prefix(':') {
        Some(rest) => rest.trim().to_string(),
        None => text.trim().to_string(),
    }
}

fn unexpected(token: &Token) -> ParseError {
    let detail = if token.kind == TokenKind::Eof {
        "Unexpected end of input".to_string()
    } else {
        format!("Unexpected '{}'", token.kind.name())
    };
    ParseError::at(
        format!("Parse error on line {}: {}", token.span.line, detail),
        token.span,
    )
}
