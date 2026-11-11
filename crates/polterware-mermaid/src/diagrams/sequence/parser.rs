//! The sequence grammar as a recursive-descent parser: upstream
//! `packages/mermaid/src/diagrams/sequence/parser/sequenceDiagram.jison`.
//!
//! The grammar is line oriented and builds a flat list of statements which `SequenceDB.apply`
//! runs after the parse (so parse errors are reported before apply errors, as upstream does).
//! Every production of the jison file is here; `document` is the shared line list, and the
//! block statements (`loop`, `rect`, `opt`, `alt`, `par`, `par_over`, `critical`, `break`,
//! `box`) recurse into it until their `end` (and `else`/`and`/`option` for their sections).

use super::ast::{Ast, Label, LineType, Placement};
use super::db::{Action, ActionKind, Db, parse_box_data};
use super::lexer::{Arrow, Lexer, Token, TokenKind, js_trim};
use crate::error::{ParseError, Span};

/// Parses the diagram body (see [`crate::preprocess`]).
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    let mut parser = Parser::new(Lexer::new(body))?;
    let actions = parser.start()?;
    let mut db = Db::new();
    db.apply_all(actions)?;
    Ok(db.into_ast())
}

struct Parser<'a> {
    lexer: Lexer<'a>,
    token: Token,
    ahead: Option<Token>,
}

impl<'a> Parser<'a> {
    fn new(mut lexer: Lexer<'a>) -> Result<Self, ParseError> {
        let token = lexer.next_token()?;
        Ok(Self {
            lexer,
            token,
            ahead: None,
        })
    }

    fn next_token(&mut self) -> Result<Token, ParseError> {
        match self.ahead.take() {
            Some(token) => Ok(token),
            None => self.lexer.next_token(),
        }
    }

    fn bump(&mut self) -> Result<(), ParseError> {
        self.token = self.next_token()?;
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

    fn start(&mut self) -> Result<Vec<Action>, ParseError> {
        while self.at(TokenKind::Newline) {
            self.bump()?;
        }
        if !self.at(TokenKind::Sd) {
            return Err(self.unexpected("'sequenceDiagram'"));
        }
        self.bump()?;
        let actions = self.document(&[])?;
        if !self.at(TokenKind::Eof) {
            return Err(self.unexpected("the end of the diagram"));
        }
        Ok(actions)
    }

    fn document(&mut self, terminators: &[TokenKind]) -> Result<Vec<Action>, ParseError> {
        let mut actions = Vec::new();
        loop {
            if self.at(TokenKind::Eof) || terminators.contains(&self.token.kind) {
                break;
            }
            actions.extend(self.line()?);
        }
        Ok(actions)
    }

    fn line(&mut self) -> Result<Vec<Action>, ParseError> {
        if self.at(TokenKind::Newline) || self.at(TokenKind::Invalid) {
            self.bump()?;
            return Ok(Vec::new());
        }
        self.statement()
    }

    fn statement(&mut self) -> Result<Vec<Action>, ParseError> {
        let span = self.token.span;
        match self.token.kind {
            TokenKind::Participant => {
                self.bump()?;
                self.participant_statement(span, "participant", false)
            }
            TokenKind::ParticipantActor => {
                self.bump()?;
                self.participant_statement(span, "actor", false)
            }
            TokenKind::Create => {
                self.bump()?;
                match self.token.kind {
                    TokenKind::Participant => {
                        self.bump()?;
                        self.participant_statement(span, "participant", true)
                    }
                    TokenKind::ParticipantActor => {
                        self.bump()?;
                        self.participant_statement(span, "actor", true)
                    }
                    TokenKind::Destroy => {
                        self.bump()?;
                        let mut actions = self.destroy_statement(span)?;
                        for action in &mut actions {
                            if let ActionKind::DestroyParticipant { actor } = &action.kind {
                                action.kind = ActionKind::CreateParticipant {
                                    actor: actor.clone(),
                                    draw: None,
                                    description: None,
                                    config: None,
                                };
                            }
                        }
                        Ok(actions)
                    }
                    _ => Err(self.unexpected("'participant' or 'actor'")),
                }
            }
            TokenKind::Destroy => {
                self.bump()?;
                self.destroy_statement(span)
            }
            TokenKind::Box => self.box_statement(span),
            TokenKind::Activate => {
                self.bump()?;
                let actor = self.actor_id()?;
                self.line_end()?;
                Ok(vec![action(span, ActionKind::ActiveStart { actor })])
            }
            TokenKind::Deactivate => {
                self.bump()?;
                let actor = self.actor_id()?;
                self.line_end()?;
                Ok(vec![action(span, ActionKind::ActiveEnd { actor })])
            }
            TokenKind::Note => self.note_statement(span),
            TokenKind::Links => {
                self.bump()?;
                let actor = self.actor_id()?;
                let text = self.text2()?;
                self.line_end()?;
                Ok(vec![
                    participant_action(span, actor.clone()),
                    action(span, ActionKind::AddLinks { actor, text }),
                ])
            }
            TokenKind::Link => {
                if matches!(self.lexer_peek()?, TokenKind::Actor | TokenKind::Link) {
                    self.bump()?;
                    let actor = self.actor_id()?;
                    let text = self.text2()?;
                    self.line_end()?;
                    Ok(vec![
                        participant_action(span, actor.clone()),
                        action(span, ActionKind::AddALink { actor, text }),
                    ])
                } else {
                    self.signal(span)
                }
            }
            TokenKind::Properties => {
                self.bump()?;
                let actor = self.actor_id()?;
                let text = self.text2()?;
                self.line_end()?;
                Ok(vec![
                    participant_action(span, actor.clone()),
                    action(span, ActionKind::AddProperties { actor, text }),
                ])
            }
            TokenKind::Details => {
                self.bump()?;
                let actor = self.actor_id()?;
                let _text = self.text2()?;
                self.line_end()?;
                Ok(vec![
                    participant_action(span, actor),
                    action(span, ActionKind::AddDetails),
                ])
            }
            TokenKind::Title => {
                let title = self.token.text.chars().skip(6).collect::<String>();
                self.bump()?;
                Ok(vec![action(span, ActionKind::SetDiagramTitle(title))])
            }
            TokenKind::LegacyTitle => {
                let title = self.token.text.chars().skip(7).collect::<String>();
                self.bump()?;
                Ok(vec![action(span, ActionKind::SetDiagramTitle(title))])
            }
            TokenKind::AccTitle => {
                self.bump()?;
                if !self.at(TokenKind::AccTitleValue) {
                    return Err(self.unexpected("the accessibility title text"));
                }
                let text = js_trim(&self.token.text).to_string();
                self.bump()?;
                Ok(vec![action(span, ActionKind::SetAccTitle(text))])
            }
            TokenKind::AccDescr => {
                self.bump()?;
                if !self.at(TokenKind::AccDescrValue) {
                    return Err(self.unexpected("the accessibility description text"));
                }
                let text = js_trim(&self.token.text).to_string();
                self.bump()?;
                Ok(vec![action(span, ActionKind::SetAccDescription(text))])
            }
            TokenKind::AccDescrMultilineValue => {
                let text = js_trim(&self.token.text).to_string();
                self.bump()?;
                Ok(vec![action(span, ActionKind::SetAccDescription(text))])
            }
            TokenKind::Autonumber => self.autonumber(span),
            TokenKind::Loop => self.simple_block(span, Block::Loop),
            TokenKind::Rect => self.simple_block(span, Block::Rect),
            TokenKind::Opt => self.simple_block(span, Block::Opt),
            TokenKind::Break => self.simple_block(span, Block::Break),
            TokenKind::Alt => self.alt_statement(span),
            TokenKind::Par => self.par_statement(span, LineType::ParStart),
            TokenKind::ParOver => self.par_statement(span, LineType::ParOverStart),
            TokenKind::Critical => self.critical_statement(span),
            TokenKind::Actor => self.signal(span),
            _ => Err(self.unexpected("a statement")),
        }
    }

    /// The next token without consuming it: a `link` token is a statement keyword when an
    /// actor follows it, and an actor id when a signal follows it.
    fn lexer_peek(&mut self) -> Result<TokenKind, ParseError> {
        if self.ahead.is_none() {
            self.ahead = Some(self.lexer.next_token()?);
        }
        Ok(self
            .ahead
            .as_ref()
            .map_or(TokenKind::Eof, |token| token.kind))
    }

    fn participant_statement(
        &mut self,
        span: Span,
        draw: &'static str,
        create: bool,
    ) -> Result<Vec<Action>, ParseError> {
        let (actor, config) = self.actor_with_config()?;
        let description = if self.at(TokenKind::As) {
            self.bump()?;
            Some(self.rest_of_line_label()?)
        } else {
            None
        };
        self.line_end()?;
        let kind = if create {
            ActionKind::CreateParticipant {
                actor,
                draw: Some(draw),
                description,
                config,
            }
        } else {
            ActionKind::AddParticipant {
                actor,
                draw: Some(draw),
                description,
                config,
            }
        };
        Ok(vec![action(span, kind)])
    }

    fn destroy_statement(&mut self, span: Span) -> Result<Vec<Action>, ParseError> {
        let actor = self.actor_id()?;
        self.line_end()?;
        Ok(vec![action(span, ActionKind::DestroyParticipant { actor })])
    }

    fn note_statement(&mut self, span: Span) -> Result<Vec<Action>, ParseError> {
        self.bump()?;
        match self.token.kind {
            TokenKind::LeftOf | TokenKind::RightOf => {
                let placement = if self.at(TokenKind::LeftOf) {
                    Placement::LeftOf
                } else {
                    Placement::RightOf
                };
                self.bump()?;
                let actor = self.actor_id()?;
                let text = self.text2()?;
                self.line_end()?;
                Ok(vec![
                    participant_action(span, actor.clone()),
                    action(
                        span,
                        ActionKind::AddNote {
                            actors: vec![actor],
                            placement,
                            text,
                        },
                    ),
                ])
            }
            TokenKind::Over => {
                self.bump()?;
                let mut actors = vec![self.actor_id()?];
                if self.at(TokenKind::Comma) {
                    self.bump()?;
                    actors.push(self.actor_id()?);
                }
                let text = self.text2()?;
                self.line_end()?;
                let mut actions = actors
                    .iter()
                    .map(|actor| participant_action(span, actor.clone()))
                    .collect::<Vec<_>>();
                actions.push(action(
                    span,
                    ActionKind::AddNote {
                        actors,
                        placement: Placement::Over,
                        text,
                    },
                ));
                Ok(actions)
            }
            _ => Err(self.unexpected("a note placement ('left of', 'right of', or 'over')")),
        }
    }

    fn box_statement(&mut self, span: Span) -> Result<Vec<Action>, ParseError> {
        self.bump()?;
        if !self.at(TokenKind::RestOfLine) {
            return Err(self.unexpected("the box description"));
        }
        let data = parse_box_data(&self.token.text);
        self.bump()?;
        let mut actions = vec![action(span, ActionKind::BoxStart { data })];
        loop {
            match self.token.kind {
                TokenKind::Newline => self.bump()?,
                TokenKind::Participant => {
                    let line_span = self.token.span;
                    self.bump()?;
                    actions.extend(self.participant_statement(line_span, "participant", false)?);
                }
                TokenKind::ParticipantActor => {
                    let line_span = self.token.span;
                    self.bump()?;
                    actions.extend(self.participant_statement(line_span, "actor", false)?);
                }
                TokenKind::Destroy => {
                    let line_span = self.token.span;
                    self.bump()?;
                    actions.extend(self.destroy_statement(line_span)?);
                }
                TokenKind::End => {
                    let end_span = self.token.span;
                    self.bump()?;
                    actions.push(action(end_span, ActionKind::BoxEnd));
                    return Ok(actions);
                }
                TokenKind::Eof => return Err(self.unexpected("'end'")),
                _ => return Err(self.unexpected("a participant statement or 'end'")),
            }
        }
    }

    fn autonumber(&mut self, span: Span) -> Result<Vec<Action>, ParseError> {
        self.bump()?;
        let kind = match self.token.kind {
            TokenKind::Num => {
                let index = parse_number(&self.token.text);
                self.bump()?;
                let step = if self.at(TokenKind::Num) {
                    let step = parse_number(&self.token.text);
                    self.bump()?;
                    Some(step)
                } else {
                    Some(1.0)
                };
                self.line_end()?;
                ActionKind::SequenceIndex {
                    index: Some(index),
                    step,
                    visible: true,
                }
            }
            TokenKind::Off => {
                self.bump()?;
                self.line_end()?;
                ActionKind::SequenceIndex {
                    index: None,
                    step: None,
                    visible: false,
                }
            }
            TokenKind::Newline | TokenKind::Eof => {
                self.line_end()?;
                ActionKind::SequenceIndex {
                    index: None,
                    step: None,
                    visible: true,
                }
            }
            _ => return Err(self.unexpected("a number, 'off', or a newline")),
        };
        Ok(vec![action(span, kind)])
    }

    fn simple_block(&mut self, span: Span, block: Block) -> Result<Vec<Action>, ParseError> {
        self.bump()?;
        let text = self.rest_of_line_label()?;
        let body = self.document(&[TokenKind::End])?;
        if !self.at(TokenKind::End) {
            return Err(self.unexpected("'end'"));
        }
        let end_span = self.token.span;
        self.bump()?;
        let (start, end) = match block {
            Block::Loop => (ActionKind::LoopStart { text }, ActionKind::LoopEnd),
            Block::Rect => (ActionKind::RectStart { text }, ActionKind::RectEnd),
            Block::Opt => (ActionKind::OptStart { text }, ActionKind::OptEnd),
            Block::Break => (ActionKind::BreakStart { text }, ActionKind::BreakEnd),
        };
        let mut actions = vec![action(span, start)];
        actions.extend(body);
        actions.push(action(end_span, end));
        Ok(actions)
    }

    fn alt_statement(&mut self, span: Span) -> Result<Vec<Action>, ParseError> {
        self.bump()?;
        let text = self.rest_of_line_label()?;
        let mut actions = vec![action(span, ActionKind::AltStart { text })];
        let mut sections = self.document(&[TokenKind::Else, TokenKind::End])?;
        actions.append(&mut sections);
        while self.at(TokenKind::Else) {
            let else_span = self.token.span;
            self.bump()?;
            let text = self.rest_of_line_label()?;
            actions.push(action(else_span, ActionKind::AltElse { text }));
            let mut section = self.document(&[TokenKind::Else, TokenKind::End])?;
            actions.append(&mut section);
        }
        if !self.at(TokenKind::End) {
            return Err(self.unexpected("'end'"));
        }
        let end_span = self.token.span;
        self.bump()?;
        actions.push(action(end_span, ActionKind::AltEnd));
        Ok(actions)
    }

    fn par_statement(
        &mut self,
        span: Span,
        start_type: LineType,
    ) -> Result<Vec<Action>, ParseError> {
        self.bump()?;
        let text = self.rest_of_line_label()?;
        let mut actions = vec![action(
            span,
            ActionKind::ParStart {
                text,
                signal_type: start_type,
            },
        )];
        let mut sections = self.document(&[TokenKind::And, TokenKind::End])?;
        actions.append(&mut sections);
        while self.at(TokenKind::And) {
            let and_span = self.token.span;
            self.bump()?;
            let text = self.rest_of_line_label()?;
            actions.push(action(and_span, ActionKind::ParAnd { text }));
            let mut section = self.document(&[TokenKind::And, TokenKind::End])?;
            actions.append(&mut section);
        }
        if !self.at(TokenKind::End) {
            return Err(self.unexpected("'end'"));
        }
        let end_span = self.token.span;
        self.bump()?;
        actions.push(action(end_span, ActionKind::ParEnd));
        Ok(actions)
    }

    fn critical_statement(&mut self, span: Span) -> Result<Vec<Action>, ParseError> {
        self.bump()?;
        let text = self.rest_of_line_label()?;
        let mut actions = vec![action(span, ActionKind::CriticalStart { text })];
        let mut sections = self.document(&[TokenKind::Option, TokenKind::End])?;
        actions.append(&mut sections);
        while self.at(TokenKind::Option) {
            let option_span = self.token.span;
            self.bump()?;
            let text = self.rest_of_line_label()?;
            actions.push(action(option_span, ActionKind::CriticalOption { text }));
            let mut section = self.document(&[TokenKind::Option, TokenKind::End])?;
            actions.append(&mut section);
        }
        if !self.at(TokenKind::End) {
            return Err(self.unexpected("'end'"));
        }
        let end_span = self.token.span;
        self.bump()?;
        actions.push(action(end_span, ActionKind::CriticalEnd));
        Ok(actions)
    }

    fn signal(&mut self, span: Span) -> Result<Vec<Action>, ParseError> {
        let from = self.actor_id()?;
        let mut actions = vec![participant_action(span, from.clone())];
        if self.at(TokenKind::Parens) {
            self.bump()?;
            let signal_type = self.signal_type()?;
            let dual = if self.at(TokenKind::Parens) {
                self.bump()?;
                true
            } else {
                false
            };
            let to = self.actor_id()?;
            actions.push(participant_action(span, to.clone()));
            let msg = self.text2()?;
            self.line_end()?;
            if dual {
                actions.push(action(
                    span,
                    ActionKind::AddMessage {
                        from: from.clone(),
                        to: to.clone(),
                        msg,
                        signal_type,
                        activate: true,
                        central_connection: Some(LineType::CentralConnectionDual as u8),
                    },
                ));
                actions.push(action(span, ActionKind::CentralConnection { actor: to }));
                actions.push(action(
                    span,
                    ActionKind::CentralConnectionReverse { actor: from },
                ));
            } else {
                actions.push(action(
                    span,
                    ActionKind::AddMessage {
                        from: from.clone(),
                        to,
                        msg,
                        signal_type,
                        activate: false,
                        central_connection: Some(LineType::CentralConnectionReverse as u8),
                    },
                ));
                actions.push(action(
                    span,
                    ActionKind::CentralConnectionReverse { actor: from },
                ));
            }
            return Ok(actions);
        }
        let signal_type = self.signal_type()?;
        let (activate, central_connection, activation, central) = match self.token.kind {
            TokenKind::Plus => {
                self.bump()?;
                (true, None, Some(Activation::Start), None)
            }
            TokenKind::Minus => {
                self.bump()?;
                (false, None, Some(Activation::End), None)
            }
            TokenKind::Parens => {
                self.bump()?;
                (
                    true,
                    Some(LineType::CentralConnection as u8),
                    None,
                    Some(true),
                )
            }
            _ => (false, None, None, None),
        };
        let to = self.actor_id()?;
        actions.push(participant_action(span, to.clone()));
        let msg = self.text2()?;
        self.line_end()?;
        actions.push(action(
            span,
            ActionKind::AddMessage {
                from: from.clone(),
                to: to.clone(),
                msg,
                signal_type,
                activate,
                central_connection,
            },
        ));
        match activation {
            Some(Activation::Start) => {
                actions.push(action(span, ActionKind::ActiveStart { actor: to.clone() }))
            }
            Some(Activation::End) => {
                actions.push(action(span, ActionKind::ActiveEnd { actor: from }))
            }
            None => {}
        }
        if central.is_some() {
            actions.push(action(span, ActionKind::CentralConnection { actor: to }));
        }
        Ok(actions)
    }

    fn signal_type(&mut self) -> Result<LineType, ParseError> {
        let kind = match self.token.kind {
            TokenKind::Arrow(arrow) => arrow_line_type(arrow),
            _ => return Err(self.unexpected("an arrow")),
        };
        self.bump()?;
        Ok(kind)
    }

    fn actor_id(&mut self) -> Result<String, ParseError> {
        match self.token.kind {
            TokenKind::Actor | TokenKind::Link => {
                let id = self.token.text.clone();
                self.bump()?;
                Ok(id)
            }
            _ => Err(self.unexpected("an actor id")),
        }
    }

    fn actor_with_config(&mut self) -> Result<(String, Option<String>), ParseError> {
        let id = self.actor_id()?;
        if !self.at(TokenKind::ConfigStart) {
            return Ok((id, None));
        }
        self.bump()?;
        if !self.at(TokenKind::ConfigContent) {
            return Err(self.unexpected("the participant config content"));
        }
        let config = self.token.text.clone();
        self.bump()?;
        if !self.at(TokenKind::ConfigEnd) {
            return Err(self.unexpected("the end of the participant config"));
        }
        self.bump()?;
        Ok((id, Some(config)))
    }

    fn text2(&mut self) -> Result<Label, ParseError> {
        if !self.at(TokenKind::Txt) {
            return Err(self.unexpected("a message"));
        }
        let raw = self.token.text.clone();
        self.bump()?;
        let trimmed = js_trim(&raw);
        let without_colon = trimmed.strip_prefix(':').unwrap_or(trimmed);
        Ok(parse_message(without_colon))
    }

    fn rest_of_line_label(&mut self) -> Result<Label, ParseError> {
        if !self.at(TokenKind::RestOfLine) {
            return Err(self.unexpected("the rest of the line"));
        }
        let text = self.token.text.clone();
        self.bump()?;
        Ok(parse_message(&text))
    }

    fn line_end(&mut self) -> Result<(), ParseError> {
        match self.token.kind {
            TokenKind::Newline => self.bump(),
            TokenKind::Eof => Ok(()),
            _ => Err(self.unexpected("a newline")),
        }
    }
}

enum Block {
    Loop,
    Rect,
    Opt,
    Break,
}

enum Activation {
    Start,
    End,
}

fn action(span: Span, kind: ActionKind) -> Action {
    Action { span, kind }
}

fn participant_action(span: Span, actor: String) -> Action {
    action(
        span,
        ActionKind::AddParticipant {
            actor,
            draw: None,
            description: None,
            config: None,
        },
    )
}

/// `parseMessage`: pull out a leading `:?nowrap:`/`:?wrap:` marker and trim.
fn parse_message(text: &str) -> Label {
    let (text, wrap) = super::db::extract_wrap(Some(text));
    Label {
        text: text.unwrap_or_default(),
        wrap,
    }
}

fn parse_number(text: &str) -> f64 {
    text.parse::<f64>().unwrap_or(0.0)
}

fn arrow_line_type(arrow: Arrow) -> LineType {
    match arrow {
        Arrow::Solid => LineType::Solid,
        Arrow::BidirectionalSolid => LineType::BidirectionalSolid,
        Arrow::Dotted => LineType::Dotted,
        Arrow::BidirectionalDotted => LineType::BidirectionalDotted,
        Arrow::SolidOpen => LineType::SolidOpen,
        Arrow::DottedOpen => LineType::DottedOpen,
        Arrow::SolidCross => LineType::SolidCross,
        Arrow::DottedCross => LineType::DottedCross,
        Arrow::SolidPoint => LineType::SolidPoint,
        Arrow::DottedPoint => LineType::DottedPoint,
        Arrow::SolidTopDotted => LineType::SolidTopDotted,
        Arrow::SolidBottomDotted => LineType::SolidBottomDotted,
        Arrow::StickTopDotted => LineType::StickTopDotted,
        Arrow::StickBottomDotted => LineType::StickBottomDotted,
        Arrow::SolidTopReverseDotted => LineType::SolidTopReverseDotted,
        Arrow::SolidBottomReverseDotted => LineType::SolidBottomReverseDotted,
        Arrow::StickTopReverseDotted => LineType::StickTopReverseDotted,
        Arrow::StickBottomReverseDotted => LineType::StickBottomReverseDotted,
        Arrow::SolidTop => LineType::SolidTop,
        Arrow::SolidBottom => LineType::SolidBottom,
        Arrow::StickTop => LineType::StickTop,
        Arrow::StickBottom => LineType::StickBottom,
        Arrow::SolidTopReverse => LineType::SolidTopReverse,
        Arrow::SolidBottomReverse => LineType::SolidBottomReverse,
        Arrow::StickTopReverse => LineType::StickTopReverse,
        Arrow::StickBottomReverse => LineType::StickBottomReverse,
    }
}

fn describe(token: &Token) -> String {
    let name = match token.kind {
        TokenKind::Eof => return "the end of the input".to_string(),
        TokenKind::Newline => "a newline",
        TokenKind::Num => return format!("'{}'", token.text),
        TokenKind::ConfigStart => "'@{'",
        TokenKind::ConfigContent => return format!("'{}'", token.text),
        TokenKind::ConfigEnd => "'}'",
        TokenKind::Actor => return format!("'{}'", token.text),
        TokenKind::Invalid => return format!("invalid text '{}'", token.text),
        TokenKind::Box => "'box'",
        TokenKind::Participant => "'participant'",
        TokenKind::ParticipantActor => "'actor'",
        TokenKind::Create => "'create'",
        TokenKind::Destroy => "'destroy'",
        TokenKind::As => "'as'",
        TokenKind::Loop => "'loop'",
        TokenKind::Rect => "'rect'",
        TokenKind::Opt => "'opt'",
        TokenKind::Alt => "'alt'",
        TokenKind::Else => "'else'",
        TokenKind::Par => "'par'",
        TokenKind::ParOver => "'par_over'",
        TokenKind::And => "'and'",
        TokenKind::Critical => "'critical'",
        TokenKind::Option => "'option'",
        TokenKind::Break => "'break'",
        TokenKind::RestOfLine => return format!("'{}'", token.text),
        TokenKind::End => "'end'",
        TokenKind::LeftOf => "'left of'",
        TokenKind::RightOf => "'right of'",
        TokenKind::Links => "'links'",
        TokenKind::Link => "'link'",
        TokenKind::Properties => "'properties'",
        TokenKind::Details => "'details'",
        TokenKind::Over => "'over'",
        TokenKind::Note => "'note'",
        TokenKind::Activate => "'activate'",
        TokenKind::Deactivate => "'deactivate'",
        TokenKind::Title => return format!("'{}'", token.text),
        TokenKind::LegacyTitle => return format!("'{}'", token.text),
        TokenKind::AccTitle => "'accTitle'",
        TokenKind::AccTitleValue => return format!("'{}'", token.text),
        TokenKind::AccDescr => "'accDescr'",
        TokenKind::AccDescrValue => return format!("'{}'", token.text),
        TokenKind::AccDescrMultilineValue => return format!("'{}'", token.text),
        TokenKind::Sd => "'sequenceDiagram'",
        TokenKind::Autonumber => "'autonumber'",
        TokenKind::Off => "'off'",
        TokenKind::Comma => "','",
        TokenKind::Arrow(_) => "an arrow",
        TokenKind::Txt => return format!("'{}'", token.text),
        TokenKind::Plus => "'+'",
        TokenKind::Minus => "'-'",
        TokenKind::Parens => "'()'",
    };
    name.to_string()
}
