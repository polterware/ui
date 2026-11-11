//! The requirement grammar: a hand-written recursive-descent port of
//! `parser/requirementDiagram.jison`.
//!
//! ```text
//! start
//!   : directive NEWLINE start
//!   | directive start
//!   | RD NEWLINE diagram EOF
//! directive
//!   : acc_title acc_title_value
//!   | acc_descr acc_descr_value
//!   | acc_descr_multiline_value
//! diagram
//!   : /* empty */
//!   | requirementDef diagram | elementDef diagram | relationshipDef diagram
//!   | directive diagram | direction diagram | styleStatement diagram
//!   | classDefStatement diagram | classStatement diagram | NEWLINE diagram
//! requirementDef
//!   : requirementType requirementName STRUCT_START NEWLINE requirementBody
//!   | requirementType requirementName STYLE_SEPARATOR idList STRUCT_START NEWLINE
//!     requirementBody
//! requirementBody
//!   : ID COLONSEP id NEWLINE requirementBody
//!   | TEXT COLONSEP text NEWLINE requirementBody
//!   | RISK COLONSEP riskLevel NEWLINE requirementBody
//!   | VERIFYMTHD COLONSEP verifyType NEWLINE requirementBody
//!   | NEWLINE requirementBody
//!   | STRUCT_STOP
//! elementDef
//!   : ELEMENT elementName STRUCT_START NEWLINE elementBody
//!   | ELEMENT elementName STYLE_SEPARATOR idList STRUCT_START NEWLINE elementBody
//! elementBody
//!   : TYPE COLONSEP type NEWLINE elementBody
//!   | DOCREF COLONSEP ref NEWLINE elementBody
//!   | NEWLINE elementBody
//!   | STRUCT_STOP
//! relationshipDef
//!   : id END_ARROW_L relationship LINE id
//!   | id LINE relationship END_ARROW_R id
//! classDefStatement: CLASSDEF idList stylesOpt
//! classStatement: CLASS idList idList | id STYLE_SEPARATOR idList
//! styleStatement: STYLE idList stylesOpt
//! idList: ALPHA | idList COMMA ALPHA | id | idList COMMA id
//! stylesOpt: style | stylesOpt COMMA style
//! style: styleComponent | style styleComponent
//! styleComponent: ALPHA | NUM | COLON | UNIT | SPACE | BRKT | PCT | MINUS | LABEL | SEMICOLON
//! ```
//!
//! The actions populate [`Db`](super::db::Db) exactly like upstream's: a requirement's body
//! fields are collected first and `addRequirement` copies them when the closing `}` is
//! reduced.
//!
//! `UNIT`, `SPACE`, `PCT`, and `LABEL` are referenced by `styleComponent` but the lexer
//! never returns them, so a style with a `%` or a quoted value is a parse error upstream and
//! here too.

use super::ast::{Direction, RelationshipType, RequirementType, RiskLevel, VerifyType};
use super::db::Db;
use super::lexer::{Lexer, Token, TokenKind};
use crate::error::ParseError;
use crate::preprocess::is_js_whitespace;

/// Parses the diagram body and populates the database.
pub(crate) fn parse(body: &str) -> Result<super::ast::Ast, ParseError> {
    Parser::new(body).parse_document()
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

    fn parse_document(mut self) -> Result<super::ast::Ast, ParseError> {
        let mut db = Db::new();

        // Upstream's `cleanupComments` deletes comment lines and then `trimStart`s the text;
        // this crate's preprocess keeps comments and line numbers, so comments and blank
        // lines before the header arrive as NEWLINE tokens. The `start` rule accepts
        // directives before the header, each optionally followed by one NEWLINE.
        loop {
            while self.peek()? == TokenKind::Newline {
                self.bump()?;
            }
            match self.peek()? {
                TokenKind::AccTitle | TokenKind::AccDescr | TokenKind::AccDescrMultilineValue => {
                    self.parse_directive(&mut db)?
                }
                _ => break,
            }
        }

        self.expect(TokenKind::RequirementDiagram)?;
        self.expect(TokenKind::Newline)?;
        self.parse_diagram(&mut db)?;
        self.expect(TokenKind::Eof)?;
        Ok(db.into_ast())
    }

    fn parse_diagram(&mut self, db: &mut Db) -> Result<(), ParseError> {
        loop {
            match self.peek()? {
                TokenKind::Eof => return Ok(()),
                TokenKind::Newline => {
                    self.bump()?;
                }
                TokenKind::AccTitle | TokenKind::AccDescr | TokenKind::AccDescrMultilineValue => {
                    self.parse_directive(db)?;
                }
                TokenKind::DirectionTb
                | TokenKind::DirectionBt
                | TokenKind::DirectionRl
                | TokenKind::DirectionLr => {
                    let kind = self.bump()?.kind;
                    db.set_direction(match kind {
                        TokenKind::DirectionTb => Direction::Tb,
                        TokenKind::DirectionBt => Direction::Bt,
                        TokenKind::DirectionRl => Direction::Rl,
                        TokenKind::DirectionLr => Direction::Lr,
                        _ => unreachable!("matched above"),
                    });
                }
                TokenKind::Requirement
                | TokenKind::FunctionalRequirement
                | TokenKind::InterfaceRequirement
                | TokenKind::PerformanceRequirement
                | TokenKind::PhysicalRequirement
                | TokenKind::DesignConstraint => {
                    self.parse_requirement(db)?;
                }
                TokenKind::Element => {
                    self.parse_element(db)?;
                }
                TokenKind::Style => {
                    self.parse_style_statement(db)?;
                }
                TokenKind::ClassDef => {
                    self.parse_class_def(db)?;
                }
                TokenKind::Class => {
                    self.parse_class_statement(db)?;
                }
                TokenKind::UnqString | TokenKind::QStr => {
                    self.parse_relationship_or_shorthand(db)?;
                }
                _ => return Err(self.unexpected("a diagram statement")),
            }
        }
    }

    fn parse_directive(&mut self, db: &mut Db) -> Result<(), ParseError> {
        let token = self.bump()?;
        match token.kind {
            TokenKind::AccTitle => {
                let value = self.expect(TokenKind::AccTitleValue)?;
                db.set_acc_title(trim(&value.text).to_string());
            }
            TokenKind::AccDescr => {
                let value = self.expect(TokenKind::AccDescrValue)?;
                db.set_acc_description(trim(&value.text).to_string());
            }
            TokenKind::AccDescrMultilineValue => {
                db.set_acc_description(trim(&token.text).to_string());
            }
            _ => unreachable!("called after matching a directive"),
        }
        Ok(())
    }

    fn parse_requirement(&mut self, db: &mut Db) -> Result<(), ParseError> {
        let requirement_type = requirement_type(self.bump()?.kind);
        let name = self.parse_id_value()?;
        let class_names = if self.peek()? == TokenKind::StyleSeparator {
            self.bump()?;
            Some(self.parse_id_list()?)
        } else {
            None
        };
        self.expect(TokenKind::StructStart)?;
        self.expect(TokenKind::Newline)?;

        loop {
            match self.peek()? {
                TokenKind::Id => {
                    self.bump()?;
                    self.expect(TokenKind::ColonSep)?;
                    db.set_new_req_id(self.parse_id_value()?);
                    self.expect(TokenKind::Newline)?;
                }
                TokenKind::Text => {
                    self.bump()?;
                    self.expect(TokenKind::ColonSep)?;
                    db.set_new_req_text(self.parse_id_value()?);
                    self.expect(TokenKind::Newline)?;
                }
                TokenKind::Risk => {
                    self.bump()?;
                    self.expect(TokenKind::ColonSep)?;
                    db.set_new_req_risk(self.parse_risk()?);
                    self.expect(TokenKind::Newline)?;
                }
                TokenKind::VerifyMethod => {
                    self.bump()?;
                    self.expect(TokenKind::ColonSep)?;
                    db.set_new_req_verify_method(self.parse_verify_type()?);
                    self.expect(TokenKind::Newline)?;
                }
                TokenKind::Newline => {
                    self.bump()?;
                }
                TokenKind::StructStop => {
                    self.bump()?;
                    break;
                }
                _ => return Err(self.unexpected("a requirement field or '}'")),
            }
        }

        db.add_requirement(&name, requirement_type);
        if let Some(class_names) = class_names {
            db.set_class(std::slice::from_ref(&name), &class_names);
        }
        Ok(())
    }

    fn parse_element(&mut self, db: &mut Db) -> Result<(), ParseError> {
        self.bump()?;
        let name = self.parse_id_value()?;
        let class_names = if self.peek()? == TokenKind::StyleSeparator {
            self.bump()?;
            Some(self.parse_id_list()?)
        } else {
            None
        };
        self.expect(TokenKind::StructStart)?;
        self.expect(TokenKind::Newline)?;

        loop {
            match self.peek()? {
                TokenKind::Type => {
                    self.bump()?;
                    self.expect(TokenKind::ColonSep)?;
                    db.set_new_element_type(self.parse_id_value()?);
                    self.expect(TokenKind::Newline)?;
                }
                TokenKind::DocRef => {
                    self.bump()?;
                    self.expect(TokenKind::ColonSep)?;
                    db.set_new_element_doc_ref(self.parse_id_value()?);
                    self.expect(TokenKind::Newline)?;
                }
                TokenKind::Newline => {
                    self.bump()?;
                }
                TokenKind::StructStop => {
                    self.bump()?;
                    break;
                }
                _ => return Err(self.unexpected("an element field or '}'")),
            }
        }

        db.add_element(&name);
        if let Some(class_names) = class_names {
            db.set_class(std::slice::from_ref(&name), &class_names);
        }
        Ok(())
    }

    fn parse_relationship_or_shorthand(&mut self, db: &mut Db) -> Result<(), ParseError> {
        let left = self.parse_id_value()?;
        match self.peek()? {
            TokenKind::EndArrowL => {
                self.bump()?;
                let relationship = self.parse_relationship()?;
                self.expect(TokenKind::Line)?;
                let right = self.parse_id_value()?;
                db.add_relationship(relationship, &right, &left);
                Ok(())
            }
            TokenKind::Line => {
                self.bump()?;
                let relationship = self.parse_relationship()?;
                self.expect(TokenKind::EndArrowR)?;
                let right = self.parse_id_value()?;
                db.add_relationship(relationship, &left, &right);
                Ok(())
            }
            TokenKind::StyleSeparator => {
                self.bump()?;
                let class_names = self.parse_id_list()?;
                db.set_class(std::slice::from_ref(&left), &class_names);
                Ok(())
            }
            _ => Err(self.unexpected("'<-', '-' or ':::'")),
        }
    }

    fn parse_style_statement(&mut self, db: &mut Db) -> Result<(), ParseError> {
        self.expect(TokenKind::Style)?;
        let ids = self.parse_id_list()?;
        let styles = self.parse_styles()?;
        db.set_css_style(&ids, &styles);
        Ok(())
    }

    fn parse_class_def(&mut self, db: &mut Db) -> Result<(), ParseError> {
        self.expect(TokenKind::ClassDef)?;
        let ids = self.parse_id_list()?;
        let styles = self.parse_styles()?;
        db.define_class(&ids, &styles);
        Ok(())
    }

    fn parse_class_statement(&mut self, db: &mut Db) -> Result<(), ParseError> {
        self.expect(TokenKind::Class)?;
        let ids = self.parse_id_list()?;
        let class_names = self.parse_id_list()?;
        db.set_class(&ids, &class_names);
        Ok(())
    }

    /// `idList`: comma-separated ids, where an id is `ALPHA`, `unqString`, or `qString`.
    fn parse_id_list(&mut self) -> Result<Vec<String>, ParseError> {
        let mut ids = vec![self.parse_id_element()?];
        while self.peek()? == TokenKind::Comma {
            self.bump()?;
            ids.push(self.parse_id_element()?);
        }
        Ok(ids)
    }

    fn parse_id_element(&mut self) -> Result<String, ParseError> {
        match self.peek()? {
            TokenKind::Alpha | TokenKind::UnqString | TokenKind::QStr => Ok(self.bump()?.text),
            _ => Err(self.unexpected("an id")),
        }
    }

    /// `id`: `unqString | qString`.
    fn parse_id_value(&mut self) -> Result<String, ParseError> {
        match self.peek()? {
            TokenKind::UnqString | TokenKind::QStr => Ok(self.bump()?.text),
            _ => Err(self.unexpected("an id")),
        }
    }

    /// `stylesOpt`: one style, then `,` and another style while they follow.
    fn parse_styles(&mut self) -> Result<Vec<String>, ParseError> {
        let mut styles = vec![self.parse_style()?];
        while self.peek()? == TokenKind::Comma {
            self.bump()?;
            styles.push(self.parse_style()?);
        }
        Ok(styles)
    }

    /// `style`: adjacent style components, concatenated. Upstream's `UNIT`, `SPACE`, `PCT`,
    /// and `LABEL` terminals never reach the parser, so only the tokens below contribute.
    fn parse_style(&mut self) -> Result<String, ParseError> {
        let mut style = String::new();
        loop {
            match self.peek()? {
                TokenKind::Alpha | TokenKind::Num => style.push_str(&self.bump()?.text),
                TokenKind::StyleColon => {
                    self.bump()?;
                    style.push(':');
                }
                TokenKind::Brkt => {
                    self.bump()?;
                    style.push('#');
                }
                TokenKind::Minus => {
                    self.bump()?;
                    style.push('-');
                }
                TokenKind::Semicolon => {
                    self.bump()?;
                    style.push(';');
                }
                _ => break,
            }
        }
        if style.is_empty() {
            return Err(self.unexpected("a style"));
        }
        Ok(style)
    }

    fn parse_relationship(&mut self) -> Result<RelationshipType, ParseError> {
        let kind = self.peek()?;
        let relationship = match kind {
            TokenKind::Contains => RelationshipType::Contains,
            TokenKind::Copies => RelationshipType::Copies,
            TokenKind::Derives => RelationshipType::Derives,
            TokenKind::Satisfies => RelationshipType::Satisfies,
            TokenKind::Verifies => RelationshipType::Verifies,
            TokenKind::Refines => RelationshipType::Refines,
            TokenKind::Traces => RelationshipType::Traces,
            _ => return Err(self.unexpected("a relationship type")),
        };
        self.bump()?;
        Ok(relationship)
    }

    fn parse_risk(&mut self) -> Result<RiskLevel, ParseError> {
        match self.peek()? {
            TokenKind::LowRisk => {
                self.bump()?;
                Ok(RiskLevel::Low)
            }
            TokenKind::MediumRisk => {
                self.bump()?;
                Ok(RiskLevel::Medium)
            }
            TokenKind::HighRisk => {
                self.bump()?;
                Ok(RiskLevel::High)
            }
            _ => Err(self.unexpected("a risk level")),
        }
    }

    fn parse_verify_type(&mut self) -> Result<VerifyType, ParseError> {
        match self.peek()? {
            TokenKind::VerifyAnalysis => {
                self.bump()?;
                Ok(VerifyType::Analysis)
            }
            TokenKind::VerifyDemonstration => {
                self.bump()?;
                Ok(VerifyType::Demonstration)
            }
            TokenKind::VerifyInspection => {
                self.bump()?;
                Ok(VerifyType::Inspection)
            }
            TokenKind::VerifyTest => {
                self.bump()?;
                Ok(VerifyType::Test)
            }
            _ => Err(self.unexpected("a verification method")),
        }
    }
}

fn requirement_type(kind: TokenKind) -> RequirementType {
    match kind {
        TokenKind::Requirement => RequirementType::Requirement,
        TokenKind::FunctionalRequirement => RequirementType::FunctionalRequirement,
        TokenKind::InterfaceRequirement => RequirementType::InterfaceRequirement,
        TokenKind::PerformanceRequirement => RequirementType::PerformanceRequirement,
        TokenKind::PhysicalRequirement => RequirementType::PhysicalRequirement,
        TokenKind::DesignConstraint => RequirementType::DesignConstraint,
        _ => unreachable!("called with a requirement type token"),
    }
}

fn trim(text: &str) -> &str {
    text.trim_matches(is_js_whitespace)
}
