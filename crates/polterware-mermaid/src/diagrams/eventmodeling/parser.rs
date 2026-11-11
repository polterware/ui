//! The event modeling grammar: a hand-written recursive-descent port of
//! `event-modeling.langium`.
//!
//! ```text
//! EventModel:
//!   "eventmodeling"
//!   ( accDescr=EM_ACC_DESCR | accTitle=EM_ACC_TITLE | title=EM_TITLE
//!   | modelEntities+=EmModelEntity | frames+=EmFrame | dataEntities+=EmDataEntity
//!   | noteEntities+=EmNoteEntity | gwtEntities+=EmGwt )*
//! EmTimeFrame:
//!   ("tf" | "timeframe") name=EM_FI modelEntityType=EmModelEntityType
//!   entityIdentifier=QualifiedName ("->>" sourceFrames+=[EmFrame:EM_FI])*
//!   ("[[" dataReference=[EmDataEntity:EM_EID] "]]")? EmDataInline?
//! EmResetFrame:
//!   ("rf" | "resetframe") ...same body...
//! EmModelEntity: "entity" name=QualifiedName
//! EmDataEntity: "data" name=EM_EID ("`" dataType=EmDataType "`")? dataBlockValue=EM_DATA_BLOCK
//! EmNoteEntity: "note" sourceFrame=[EmFrame:EM_FI] ("`" dataType ...)? dataBlockValue=EM_DATA_BLOCK
//! EmGwt:
//!   "gwt" sourceFrame=[EmFrame:EM_FI] "given" givenStatements+=EmGwtStatement+
//!   ("when" whenStatements+=EmGwtStatement+)? "then" thenStatements+=EmGwtStatement+
//! EmGwtStatement: EmModelEntityType entityIdentifier=[EmModelEntity:EM_EID]
//! ```
//!
//! Title, accessible title, and accessible description are single-valued assignments, so a
//! later match overwrites an earlier one, as Langium's AST builder does. Their terminals are
//! matched whole, and `AbstractMermaidValueConverter` keys on the rule names `ACC_TITLE`,
//! `ACC_DESCR`, and `TITLE`, not on `EM_ACC_TITLE`, `EM_ACC_DESCR`, and `EM_TITLE`, so the
//! values reach the database raw. `modelEntityType`, `entityIdentifier`, `dataType`, and the
//! reference texts are data type rules, whose value is the joined token text (hidden whitespace
//! is not part of it), exactly as the probe of upstream's parser shows.
//!
//! Cross-references are never resolved: mermaid parses with the bare `LangiumParser`, so a
//! reference keeps only `$refText`. This module keeps that text; [`super::db`] checks frame ids
//! the way `setAst` does.

use super::ast::{
    DataEntity, Frame, FrameKind, Gwt, GwtStatement, ModelEntity, ModelEntityType, NoteEntity,
};
use super::lexer::{Lexer, Token, TokenKind};
use crate::error::{ParseError, Span};

/// Parses the diagram body; the database's own writes happen in [`super::db`].
pub(crate) fn parse(body: &str) -> Result<ParsedEventModel, ParseError> {
    Parser::new(body).parse_event_model()
}

/// The `EventModel` AST after parsing, before the database is populated.
#[derive(Clone, Debug, Default)]
pub(crate) struct ParsedEventModel {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_description: Option<String>,
    pub(crate) frames: Vec<ParsedFrame>,
    pub(crate) data_entities: Vec<DataEntity>,
    pub(crate) model_entities: Vec<ModelEntity>,
    pub(crate) note_entities: Vec<NoteEntity>,
    pub(crate) gwt_entities: Vec<Gwt>,
}

/// One frame, with the `tf`/`rf` span `setAst` reports for a duplicate id.
#[derive(Clone, Debug)]
pub(crate) struct ParsedFrame {
    pub(crate) span: Span,
    pub(crate) frame: Frame,
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

    fn parse_event_model(mut self) -> Result<ParsedEventModel, ParseError> {
        self.expect(TokenKind::Eventmodeling)?;
        let mut parsed = ParsedEventModel::default();
        loop {
            match self.peek()? {
                TokenKind::AccDescr => {
                    parsed.acc_description = Some(self.bump()?.text);
                }
                TokenKind::AccTitle => {
                    parsed.acc_title = Some(self.bump()?.text);
                }
                TokenKind::Title => {
                    parsed.title = Some(self.bump()?.text);
                }
                TokenKind::Entity => parsed.model_entities.push(self.parse_model_entity()?),
                TokenKind::Tf | TokenKind::Timeframe => {
                    parsed.frames.push(self.parse_frame(FrameKind::Time)?);
                }
                TokenKind::Rf | TokenKind::Resetframe => {
                    parsed.frames.push(self.parse_frame(FrameKind::Reset)?);
                }
                TokenKind::Data => parsed.data_entities.push(self.parse_data_entity()?),
                TokenKind::Note => parsed.note_entities.push(self.parse_note()?),
                TokenKind::Gwt => parsed.gwt_entities.push(self.parse_gwt()?),
                TokenKind::Eof => break,
                _ => {
                    return Err(self.unexpected(
                        "a frame, a data entity, a model entity, a note, a given/when/then block, \
                         a title, or the end of input",
                    ));
                }
            }
        }
        Ok(parsed)
    }

    fn parse_frame(&mut self, kind: FrameKind) -> Result<ParsedFrame, ParseError> {
        let start = self.bump()?;
        let name = self.expect(TokenKind::Fid)?.text;
        let model_entity_type = self.parse_model_entity_type()?;
        let entity_identifier = self.parse_qualified_name()?;
        let mut source_frames = Vec::new();
        while self.peek()? == TokenKind::Arrow {
            self.bump()?;
            source_frames.push(self.expect(TokenKind::Fid)?.text);
        }
        let mut data_reference = None;
        if self.peek()? == TokenKind::OpenBrackets {
            self.bump()?;
            data_reference = Some(self.expect(TokenKind::Id)?.text);
            self.expect(TokenKind::CloseBrackets)?;
        }
        let (data_type, data_inline_value) = if self.peek()? == TokenKind::Backtick {
            let data_type = self.parse_data_type()?;
            let value = self.expect(TokenKind::DataInline)?.text;
            (Some(data_type), Some(value))
        } else if self.peek()? == TokenKind::DataInline {
            (None, Some(self.bump()?.text))
        } else {
            (None, None)
        };
        Ok(ParsedFrame {
            span: start.span,
            frame: Frame {
                kind,
                name,
                model_entity_type,
                entity_identifier,
                source_frames,
                data_reference,
                data_type,
                data_inline_value,
            },
        })
    }

    fn parse_qualified_name(&mut self) -> Result<String, ParseError> {
        let mut name = self.expect(TokenKind::Id)?.text;
        while self.peek()? == TokenKind::Dot {
            self.bump()?;
            name.push('.');
            name.push_str(&self.expect(TokenKind::Id)?.text);
        }
        Ok(name)
    }

    fn parse_model_entity_type(&mut self) -> Result<ModelEntityType, ParseError> {
        let kind = match self.peek()? {
            TokenKind::Rmo => ModelEntityType::Rmo,
            TokenKind::Readmodel => ModelEntityType::Readmodel,
            TokenKind::Ui => ModelEntityType::Ui,
            TokenKind::Cmd => ModelEntityType::Cmd,
            TokenKind::Command => ModelEntityType::Command,
            TokenKind::Evt => ModelEntityType::Evt,
            TokenKind::Event => ModelEntityType::Event,
            TokenKind::Pcr => ModelEntityType::Pcr,
            TokenKind::Processor => ModelEntityType::Processor,
            _ => return Err(self.unexpected("a model entity type")),
        };
        self.bump()?;
        Ok(kind)
    }

    fn parse_data_type(&mut self) -> Result<String, ParseError> {
        self.expect(TokenKind::Backtick)?;
        let kind = match self.peek()? {
            TokenKind::Json => "json",
            TokenKind::Jsobj => "jsobj",
            TokenKind::Figma => "figma",
            TokenKind::Salt => "salt",
            TokenKind::Uri => "uri",
            TokenKind::Md => "md",
            TokenKind::Html => "html",
            TokenKind::Text => "text",
            _ => return Err(self.unexpected("a data type")),
        };
        self.bump()?;
        self.expect(TokenKind::Backtick)?;
        Ok(kind.to_string())
    }

    /// The shared `EmDataBlock` fragment: an optional `` `type` `` and a block.
    fn parse_data_block(&mut self) -> Result<(Option<String>, String), ParseError> {
        let data_type = if self.peek()? == TokenKind::Backtick {
            Some(self.parse_data_type()?)
        } else {
            None
        };
        let value = self.expect(TokenKind::DataBlock)?.text;
        Ok((data_type, value))
    }

    fn parse_model_entity(&mut self) -> Result<ModelEntity, ParseError> {
        self.expect(TokenKind::Entity)?;
        Ok(ModelEntity {
            name: self.parse_qualified_name()?,
        })
    }

    fn parse_data_entity(&mut self) -> Result<DataEntity, ParseError> {
        self.expect(TokenKind::Data)?;
        let name = self.expect(TokenKind::Id)?.text;
        let (data_type, data_block_value) = self.parse_data_block()?;
        Ok(DataEntity {
            name,
            data_type,
            data_block_value,
        })
    }

    fn parse_note(&mut self) -> Result<NoteEntity, ParseError> {
        self.expect(TokenKind::Note)?;
        let source_frame = self.expect(TokenKind::Fid)?.text;
        let (data_type, data_block_value) = self.parse_data_block()?;
        Ok(NoteEntity {
            source_frame,
            data_type,
            data_block_value,
        })
    }

    fn parse_gwt(&mut self) -> Result<Gwt, ParseError> {
        self.expect(TokenKind::Gwt)?;
        let source_frame = self.expect(TokenKind::Fid)?.text;
        self.expect(TokenKind::Given)?;
        let given = self.parse_gwt_statements()?;
        let when = if self.peek()? == TokenKind::When {
            self.bump()?;
            self.parse_gwt_statements()?
        } else {
            Vec::new()
        };
        self.expect(TokenKind::Then)?;
        let then = self.parse_gwt_statements()?;
        Ok(Gwt {
            source_frame,
            given,
            when,
            then,
        })
    }

    fn parse_gwt_statements(&mut self) -> Result<Vec<GwtStatement>, ParseError> {
        let mut statements = Vec::new();
        while is_model_entity_type(self.peek()?) {
            statements.push(self.parse_gwt_statement()?);
        }
        if statements.is_empty() {
            return Err(self.unexpected("a model entity type"));
        }
        Ok(statements)
    }

    fn parse_gwt_statement(&mut self) -> Result<GwtStatement, ParseError> {
        self.parse_model_entity_type()?;
        let entity_identifier = self.expect(TokenKind::Id)?.text;
        Ok(GwtStatement { entity_identifier })
    }
}

fn is_model_entity_type(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Rmo
            | TokenKind::Readmodel
            | TokenKind::Ui
            | TokenKind::Cmd
            | TokenKind::Command
            | TokenKind::Evt
            | TokenKind::Event
            | TokenKind::Pcr
            | TokenKind::Processor
    )
}
