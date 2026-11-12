//! The usecase grammar: a hand-written recursive-descent port of the Chevrotain grammar in
//! `parser/usecase.parser.ts`, with the token order and modes of `parser/usecase.tokens.ts`.
//!
//! The grammar is line oriented: the header, then a sequence of lines, each a blank line, a
//! whole-line comment, or one statement. Statements build detached drafts in the same order
//! the upstream visitor adds them; [`super::db::build`] resolves those drafts only once the
//! whole document has parsed, so a failure publishes nothing.
//!
//! ```text
//! start:  USECASE lineEnd (blankLine | commentLine | statement)*
//! statement:
//!   accTitle | accDescr | direction | actorStatement | systemBoundaryStatement
//!   noteStatement | jsonStatement | classDefStatement | classStatement | styleStatement
//!   metadataAssignmentStatement | entityStatement
//! ```
//!
//! Chevrotain's optional gates are reproduced by lookahead: an `IDENTIFIER AT` starts a
//! relation tail, a label followed by a markerless operator starts a labeled arrow, and a
//! declaration target followed by `@{` is a metadata assignment.

use super::ast::{
    ArrowType, GraphStatement, LabelType, MetadataOccurrence, RelationshipType, StatementKind,
    UseCaseShape,
};
use super::db::{
    DraftBoundary, DraftClassAssignment, DraftClassDefinition, DraftElement, DraftEndpoint,
    DraftJson, DraftLabel, DraftLocation, DraftMetadata, DraftMetadataAssignment,
    DraftMetadataProperty, DraftNote, DraftRelationship, DraftStyleAssignment, ElementKind,
    MetadataValue, ParsedDocument,
};
use super::lexer::{self, Token, TokenKind};
use crate::error::{ParseError, Span};
use serde_json::{Map, Value};

/// Parses the body and resolves it into the database.
pub(crate) fn parse(body: &str) -> Result<super::ast::Ast, ParseError> {
    let tokens = lexer::tokenize(body)?;
    let document = Parser::new(body, tokens).parse_document()?;
    super::db::build(document, body)
}

/// Lexes and parses without resolving drafts, like upstream's grammar-only spec helper.
#[cfg(test)]
pub(crate) fn parse_grammar(body: &str) -> Result<ParsedDocument, ParseError> {
    let tokens = lexer::tokenize(body)?;
    Parser::new(body, tokens).parse_document()
}

/// A parsed actor item.
struct ParsedActorItem {
    id: String,
    label: DraftLabel,
    location: DraftLocation,
    generated: bool,
    node_span: Span,
    metadata: Option<DraftMetadata>,
    stereotype: Option<ParsedStereotype>,
    classes: Vec<String>,
    class_spans: Vec<Span>,
}

/// A parsed entity (use case) name, with its optional suffixes.
struct ParsedEntity {
    id: String,
    label: DraftLabel,
    location: DraftLocation,
    generated: bool,
    node_span: Span,
    shape: Option<UseCaseShape>,
    metadata: Option<DraftMetadata>,
    stereotype: Option<ParsedStereotype>,
    classes: Vec<String>,
    class_spans: Vec<Span>,
    explicit_declaration: bool,
}

struct ParsedStereotype {
    value: String,
    span: Span,
}

struct ParsedClassSuffix {
    classes: Vec<String>,
    spans: Vec<Span>,
}

struct ParsedArrow {
    type_: RelationshipType,
    arrow_type: ArrowType,
    label: Option<DraftLabel>,
    minlen: u32,
}

struct ParsedRelationTail {
    explicit_id: Option<String>,
    explicit_id_location: Option<DraftLocation>,
    arrow: ParsedArrow,
    target: ParsedEntity,
    node_span: Span,
    location: DraftLocation,
}

struct Parser<'a> {
    source: &'a str,
    tokens: Vec<Token<'a>>,
    position: usize,
    document: ParsedDocument,
    parent_boundary: Option<(String, DraftLocation)>,
    anonymous_edge: usize,
    anonymous_note: usize,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str, tokens: Vec<Token<'a>>) -> Self {
        Self {
            source,
            tokens,
            position: 0,
            document: ParsedDocument::default(),
            parent_boundary: None,
            anonymous_edge: 0,
            anonymous_note: 0,
        }
    }

    fn parse_document(mut self) -> Result<ParsedDocument, ParseError> {
        // `preprocess` removes frontmatter and `%%` comment lines and keeps a directive's
        // line breaks; upstream hands its parser the cleaned text. Leading blank and comment
        // lines are therefore skipped before the header.
        loop {
            match self.peek() {
                TokenKind::Newline => {
                    self.bump();
                }
                TokenKind::Comment => {
                    self.bump();
                    self.line_end()?;
                }
                _ => break,
            }
        }
        let header = self.expect(TokenKind::Usecase, "the usecase header")?;
        self.document.header_span = header.span;
        self.line_end()?;
        while self.peek() != TokenKind::Eof {
            let statement = self.parse_line()?;
            self.document.add_root(statement);
        }
        Ok(self.document)
    }

    // ----- token helpers -------------------------------------------------------------

    fn peek(&self) -> TokenKind {
        self.tokens[self.position].kind
    }

    fn peek_at(&self, offset: usize) -> TokenKind {
        self.tokens
            .get(self.position + offset)
            .map(|token| token.kind)
            .unwrap_or(TokenKind::Eof)
    }

    fn token(&self) -> &Token<'a> {
        &self.tokens[self.position]
    }

    fn bump(&mut self) -> Token<'a> {
        let token = self.tokens[self.position].clone();
        self.position += 1;
        token
    }

    fn expect(&mut self, kind: TokenKind, what: &str) -> Result<Token<'a>, ParseError> {
        if self.peek() != kind {
            return Err(self.expected(what));
        }
        Ok(self.bump())
    }

    fn expected(&self, what: &str) -> ParseError {
        let token = self.token();
        ParseError::at(
            format!("expected {what}, but found: '{}'", token.image),
            token.span,
        )
    }

    fn line_end(&mut self) -> Result<(), ParseError> {
        match self.peek() {
            TokenKind::Newline => {
                self.bump();
                Ok(())
            }
            TokenKind::Eof => Ok(()),
            _ => Err(self.expected("a newline or end of input")),
        }
    }

    /// The span from the token at `start` through the last token consumed, excluding the
    /// trailing newline and the synthetic end-of-input token.
    fn span_from(&self, start: usize) -> Span {
        let first = &self.tokens[start];
        let mut span = first.span;
        for token in &self.tokens[start..self.position] {
            if token.kind != TokenKind::Newline && token.kind != TokenKind::Eof {
                span.end = token.span.end;
            }
        }
        span
    }

    fn location_from(&self, start: usize) -> DraftLocation {
        let first = &self.tokens[start];
        DraftLocation {
            span: self.span_from(start),
            line: first.span.line,
            column: first.span.column,
        }
    }

    fn token_span(&self, token: &Token<'_>, trim: usize) -> Span {
        let mut span = token.span;
        span.start += trim;
        span.end = span.end.saturating_sub(trim);
        span.column += trim as u32;
        span
    }

    fn token_location(&self, token: &Token<'_>) -> DraftLocation {
        let trim = match token.kind {
            TokenKind::PlainString => 1,
            TokenKind::MarkdownString => 2,
            _ => 0,
        };
        DraftLocation {
            span: self.token_span(token, trim),
            line: token.span.line,
            column: token.span.column + trim as u32,
        }
    }

    fn token_label(&self, token: &Token<'_>) -> DraftLabel {
        let type_ = if token.kind == TokenKind::MarkdownString {
            LabelType::Markdown
        } else {
            LabelType::Text
        };
        let trim = match type_ {
            LabelType::Markdown => 2,
            LabelType::Text if token.kind == TokenKind::PlainString => 1,
            LabelType::Text => 0,
        };
        let span = self.token_span(token, trim);
        DraftLabel {
            text: self.source[span.start..span.end].to_string(),
            type_,
            span,
        }
    }

    fn decode_plain(&self, token: &Token<'_>) -> String {
        if token.kind == TokenKind::PlainString {
            token.image[1..token.image.len() - 1].to_string()
        } else {
            token.image.to_string()
        }
    }

    fn is_arrow_start(&self) -> bool {
        matches!(
            self.peek(),
            TokenKind::DependencyArrow
                | TokenKind::Generalization
                | TokenKind::ForwardSolid
                | TokenKind::BackwardSolid
                | TokenKind::MarkerlessSolid
                | TokenKind::ForwardCircle
                | TokenKind::BackwardCircle
                | TokenKind::ForwardCross
                | TokenKind::BackwardCross
        )
    }

    fn starts_relation_tail(&self) -> bool {
        self.is_arrow_start()
            || (self.peek() == TokenKind::Identifier && self.peek_at(1) == TokenKind::At)
    }

    fn is_metadata_assignment(&self) -> bool {
        matches!(
            self.peek(),
            TokenKind::Identifier | TokenKind::PlainString | TokenKind::MarkdownString
        ) && self.peek_at(1) == TokenKind::MetadataStart
    }

    fn is_forbidden_plantuml_statement(&self) -> bool {
        if self.peek() != TokenKind::Identifier {
            return false;
        }
        matches!(
            self.token().image.to_ascii_lowercase().as_str(),
            "allowmixing" | "newpage" | "package" | "rectangle" | "skinparam"
        )
    }

    /// `hasLabeledRight`: a label token, then more label tokens, then one of `allowed`.
    fn has_labeled_right(&self, allowed: &[TokenKind]) -> bool {
        if !self.peek().starts_label() {
            return false;
        }
        let mut offset = 1;
        loop {
            let kind = self.peek_at(offset);
            if allowed.contains(&kind) {
                return true;
            }
            if !kind.starts_label() {
                return false;
            }
            offset += 1;
        }
    }

    // ----- lines and statements ------------------------------------------------------

    fn parse_line(&mut self) -> Result<usize, ParseError> {
        match self.peek() {
            TokenKind::Newline => {
                let token = self.bump();
                let statement = GraphStatement::new(StatementKind::Blank, token.span);
                Ok(self.document.add_statement(statement, Vec::new()))
            }
            TokenKind::Comment => {
                let token = self.bump();
                self.line_end()?;
                let statement = GraphStatement::new(StatementKind::Comment, token.span);
                Ok(self.document.add_statement(statement, Vec::new()))
            }
            _ => self.parse_statement(),
        }
    }

    fn parse_statement(&mut self) -> Result<usize, ParseError> {
        match self.peek() {
            TokenKind::AccTitleLine => self.parse_acc_title(),
            TokenKind::AccDescrLine | TokenKind::AccDescrBlock => self.parse_acc_descr(),
            TokenKind::Direction => self.parse_direction(),
            TokenKind::Actor => self.parse_actor_statement(),
            TokenKind::SystemBoundary => self.parse_system_boundary(),
            TokenKind::Note => self.parse_note_statement(),
            TokenKind::JsonDeclarationStart => self.parse_json_statement(),
            TokenKind::ClassDef => self.parse_class_def_statement(),
            TokenKind::Class => self.parse_class_statement(),
            TokenKind::Style => self.parse_style_statement(),
            _ if self.is_metadata_assignment() => self.parse_metadata_assignment(),
            _ if self.is_forbidden_plantuml_statement() => {
                Err(self.expected("a usecase statement (this PlantUML statement is not supported)"))
            }
            _ => self.parse_entity_statement(),
        }
    }

    fn parse_acc_title(&mut self) -> Result<usize, ParseError> {
        let token = self.expect(TokenKind::AccTitleLine, "an accessibility title")?;
        self.document.acc_title = token
            .image
            .split_once(':')
            .map(|(_, value)| value.trim().to_string())
            .unwrap_or_default();
        self.line_end()?;
        let statement = GraphStatement::new(StatementKind::AccTitle, token.span);
        Ok(self.document.add_statement(statement, Vec::new()))
    }

    fn parse_acc_descr(&mut self) -> Result<usize, ParseError> {
        let token = match self.peek() {
            TokenKind::AccDescrLine => self.bump(),
            TokenKind::AccDescrBlock => self.bump(),
            _ => return Err(self.expected("an accessibility description")),
        };
        let description = if token.kind == TokenKind::AccDescrLine {
            token
                .image
                .split_once(':')
                .map(|(_, value)| value.trim().to_string())
                .unwrap_or_default()
        } else {
            let start = token.image.find('{').map(|index| index + 1).unwrap_or(0);
            let end = token.image.rfind('}').unwrap_or(token.image.len());
            token.image[start.min(end)..end].trim().to_string()
        };
        self.document.acc_description = description;
        self.line_end()?;
        let statement = GraphStatement::new(StatementKind::AccDescr, token.span);
        Ok(self.document.add_statement(statement, Vec::new()))
    }

    fn parse_actor_statement(&mut self) -> Result<usize, ParseError> {
        let start = self.position;
        self.expect(TokenKind::Actor, "the actor keyword")?;
        let mut items = vec![self.parse_actor_item()?];
        let mut relation = None;
        if self.peek() == TokenKind::Comma {
            while self.peek() == TokenKind::Comma {
                self.bump();
                items.push(self.parse_actor_item()?);
            }
        } else if self.starts_relation_tail() {
            relation = Some(self.parse_relation_tail()?);
        }
        self.line_end()?;
        let span = self.span_from(start);

        let occurrences: Vec<_> = items
            .iter()
            .map(|item| self.actor_occurrence(item, true))
            .collect();

        let mut statement = GraphStatement::new(StatementKind::Node, span);
        statement.nodes = occurrences;

        for item in &items {
            let draft = self.actor_draft(item);
            self.document.elements.push(draft);
        }

        if let Some(relation) = relation {
            if relation.target.explicit_declaration {
                let draft = self.entity_draft(&relation.target);
                self.document.elements.push(draft);
            }
            let source = &items[0];
            let draft = DraftRelationship {
                source: self.actor_endpoint(source),
                target: self.entity_endpoint(&relation.target),
                location: DraftLocation {
                    span: relation.node_span,
                    line: relation.location.line,
                    column: relation.location.column,
                },
                explicit_id: relation.explicit_id.clone(),
                explicit_id_location: relation.explicit_id_location,
                type_: relation.arrow.type_,
                arrow_type: relation.arrow.arrow_type,
                label: relation.arrow.label.clone(),
                minlen: relation.arrow.minlen,
            };
            let id = draft
                .explicit_id
                .clone()
                .unwrap_or_else(|| format!("edge-{}", self.anonymous_edge));
            if draft.explicit_id.is_none() {
                self.anonymous_edge += 1;
            }
            self.document.relationship_drafts.push(draft.clone());
            let mut edge_occurrence = super::ast::EdgeOccurrence {
                id,
                span,
                label_span: draft.label.as_ref().map(|label| label.span),
                id_span: draft.explicit_id_location.map(|location| location.span),
                ..Default::default()
            };
            edge_occurrence.span = span;
            statement.kind = StatementKind::Edge;
            statement.nodes.push(
                self.entity_occurrence(&relation.target, relation.target.explicit_declaration),
            );
            statement.edges.push(edge_occurrence);
            Ok(self.document.add_statement(statement, Vec::new()))
        } else {
            Ok(self.document.add_statement(statement, Vec::new()))
        }
    }

    fn parse_actor_item(&mut self) -> Result<ParsedActorItem, ParseError> {
        let start = self.position;
        let mut item = self.parse_actor_name()?;
        if self.peek() == TokenKind::MetadataStart {
            item.metadata = Some(self.parse_metadata()?);
        }
        if self.peek() == TokenKind::StereotypeStart {
            item.stereotype = Some(self.parse_stereotype()?);
        }
        if self.peek() == TokenKind::ClassSeparator {
            let suffix = self.parse_class_suffix()?;
            item.classes = suffix.classes;
            item.class_spans = suffix.spans;
        }
        item.node_span = self.span_from(start);
        Ok(item)
    }

    fn parse_actor_name(&mut self) -> Result<ParsedActorItem, ParseError> {
        match self.peek() {
            TokenKind::Identifier => {
                let token = self.bump();
                let label = if self.peek() == TokenKind::LeftParen {
                    self.bump();
                    let label = self.parse_node_label()?;
                    self.expect(TokenKind::RightParen, "a closing parenthesis")?;
                    label
                } else {
                    self.token_label(&token)
                };
                Ok(ParsedActorItem {
                    id: token.image.to_string(),
                    label,
                    location: self.token_location(&token),
                    generated: false,
                    node_span: token.span,
                    metadata: None,
                    stereotype: None,
                    classes: Vec::new(),
                    class_spans: Vec::new(),
                })
            }
            TokenKind::PlainString | TokenKind::MarkdownString => {
                let token = self.bump();
                let label = self.token_label(&token);
                Ok(ParsedActorItem {
                    id: generate_id(&label.text),
                    label,
                    location: self.token_location(&token),
                    generated: true,
                    node_span: token.span,
                    metadata: None,
                    stereotype: None,
                    classes: Vec::new(),
                    class_spans: Vec::new(),
                })
            }
            _ => Err(self.expected("an actor name")),
        }
    }

    fn parse_actor_declaration_only(&mut self) -> Result<usize, ParseError> {
        let start = self.position;
        self.expect(TokenKind::Actor, "the actor keyword")?;
        let mut items = vec![self.parse_actor_item()?];
        while self.peek() == TokenKind::Comma {
            self.bump();
            items.push(self.parse_actor_item()?);
        }
        self.line_end()?;
        let span = self.span_from(start);
        let mut statement = GraphStatement::new(StatementKind::Node, span);
        statement.nodes = items
            .iter()
            .map(|item| self.actor_occurrence(item, true))
            .collect();
        for item in &items {
            let draft = self.actor_draft(item);
            self.document.elements.push(draft);
        }
        Ok(self.document.add_statement(statement, Vec::new()))
    }

    fn parse_entity_statement(&mut self) -> Result<usize, ParseError> {
        let start = self.position;
        let mut source = self.parse_entity_name()?;
        if self.starts_relation_tail() {
            let relation = self.parse_relation_tail()?;
            self.line_end()?;
            let span = self.span_from(start);
            let location = relation.location;

            if source.explicit_declaration {
                let draft = self.entity_draft(&source);
                self.document.elements.push(draft);
            }
            if relation.target.explicit_declaration {
                let draft = self.entity_draft(&relation.target);
                self.document.elements.push(draft);
            }
            let draft = DraftRelationship {
                source: self.entity_endpoint(&source),
                target: self.entity_endpoint(&relation.target),
                location: DraftLocation {
                    span: relation.node_span,
                    line: location.line,
                    column: location.column,
                },
                explicit_id: relation.explicit_id.clone(),
                explicit_id_location: relation.explicit_id_location,
                type_: relation.arrow.type_,
                arrow_type: relation.arrow.arrow_type,
                label: relation.arrow.label.clone(),
                minlen: relation.arrow.minlen,
            };
            let id = draft
                .explicit_id
                .clone()
                .unwrap_or_else(|| format!("edge-{}", self.anonymous_edge));
            if draft.explicit_id.is_none() {
                self.anonymous_edge += 1;
            }
            self.document.relationship_drafts.push(draft.clone());
            let mut statement = GraphStatement::new(StatementKind::Edge, span);
            statement.nodes = vec![
                self.entity_occurrence(&source, source.explicit_declaration),
                self.entity_occurrence(&relation.target, relation.target.explicit_declaration),
            ];
            statement.edges.push(super::ast::EdgeOccurrence {
                id,
                span,
                label_span: draft.label.as_ref().map(|label| label.span),
                id_span: draft.explicit_id_location.map(|location| location.span),
                ..Default::default()
            });
            Ok(self.document.add_statement(statement, Vec::new()))
        } else {
            self.line_end()?;
            let span = self.span_from(start);
            source.explicit_declaration = true;
            if source.shape.is_none() {
                source.shape = Some(UseCaseShape::Ellipse);
            }
            let draft = self.entity_draft(&source);
            self.document.elements.push(draft);
            let mut statement = GraphStatement::new(StatementKind::Node, span);
            statement.nodes = vec![self.entity_occurrence(&source, true)];
            Ok(self.document.add_statement(statement, Vec::new()))
        }
    }

    fn parse_entity_name(&mut self) -> Result<ParsedEntity, ParseError> {
        let start = self.position;
        let (id, label, location, generated, shape) = match self.peek() {
            TokenKind::Identifier => {
                let token = self.bump();
                let mut shape = None;
                let label = if self.peek() == TokenKind::LeftParen {
                    self.bump();
                    let label = self.parse_node_label()?;
                    self.expect(TokenKind::RightParen, "a closing parenthesis")?;
                    shape = Some(UseCaseShape::Ellipse);
                    label
                } else if self.peek() == TokenKind::LeftBracket {
                    self.bump();
                    let label = self.parse_node_label()?;
                    self.expect(TokenKind::RightBracket, "a closing bracket")?;
                    shape = Some(UseCaseShape::Rect);
                    label
                } else {
                    self.token_label(&token)
                };
                (
                    token.image.to_string(),
                    label,
                    self.token_location(&token),
                    false,
                    shape,
                )
            }
            TokenKind::PlainString | TokenKind::MarkdownString => {
                let token = self.bump();
                let label = self.token_label(&token);
                (
                    generate_id(&label.text),
                    label,
                    self.token_location(&token),
                    true,
                    None,
                )
            }
            _ => return Err(self.expected("a use case or actor name")),
        };

        let mut metadata = None;
        if self.peek() == TokenKind::MetadataStart {
            metadata = Some(self.parse_metadata()?);
        }
        let mut stereotype = None;
        if self.peek() == TokenKind::StereotypeStart {
            stereotype = Some(self.parse_stereotype()?);
        }
        let mut classes = Vec::new();
        let mut class_spans = Vec::new();
        if self.peek() == TokenKind::ClassSeparator {
            let suffix = self.parse_class_suffix()?;
            classes = suffix.classes;
            class_spans = suffix.spans;
        }
        let explicit_declaration = shape.is_some() || metadata.is_some() || stereotype.is_some();
        Ok(ParsedEntity {
            id,
            label,
            location,
            generated,
            node_span: self.span_from(start),
            shape,
            metadata,
            stereotype,
            classes,
            class_spans,
            explicit_declaration,
        })
    }

    fn parse_relation_tail(&mut self) -> Result<ParsedRelationTail, ParseError> {
        let start = self.position;
        let mut explicit_id = None;
        let mut explicit_id_location = None;
        if self.peek() == TokenKind::Identifier && self.peek_at(1) == TokenKind::At {
            let token = self.bump();
            self.bump();
            explicit_id = Some(token.image.to_string());
            explicit_id_location = Some(self.token_location(&token));
        }
        let arrow = self.parse_arrow()?;
        let target = self.parse_entity_name()?;
        Ok(ParsedRelationTail {
            explicit_id,
            explicit_id_location,
            arrow,
            target,
            node_span: self.span_from(start),
            location: self.location_from(start),
        })
    }

    fn parse_arrow(&mut self) -> Result<ParsedArrow, ParseError> {
        match self.peek() {
            TokenKind::DependencyArrow | TokenKind::Generalization => {
                self.parse_semantic_relation()
            }
            TokenKind::ForwardSolid => {
                let token = self.bump();
                Ok(ParsedArrow {
                    type_: RelationshipType::Association,
                    arrow_type: ArrowType::SolidArrow,
                    label: None,
                    minlen: solid_minlen(token.image),
                })
            }
            TokenKind::BackwardSolid => {
                let token = self.bump();
                let mut label = None;
                let mut length = token.clone();
                if token.image == "<--" && self.has_labeled_right(&[TokenKind::MarkerlessSolid]) {
                    label = Some(self.parse_edge_label()?);
                    length = self.expect(TokenKind::MarkerlessSolid, "a markerless arrow")?;
                }
                Ok(ParsedArrow {
                    type_: RelationshipType::Association,
                    arrow_type: ArrowType::BackArrow,
                    label,
                    minlen: solid_minlen(length.image),
                })
            }
            TokenKind::MarkerlessSolid => {
                let token = self.bump();
                let mut label = None;
                let mut arrow_type = ArrowType::LineSolid;
                let mut minlen = solid_minlen(token.image);
                if token.image == "--"
                    && self.has_labeled_right(&[
                        TokenKind::ForwardSolid,
                        TokenKind::MarkerlessSolid,
                        TokenKind::ForwardCircle,
                        TokenKind::ForwardCross,
                    ])
                {
                    label = Some(self.parse_edge_label()?);
                    let last = match self.peek() {
                        TokenKind::ForwardSolid
                        | TokenKind::MarkerlessSolid
                        | TokenKind::ForwardCircle
                        | TokenKind::ForwardCross => self.bump(),
                        _ => return Err(self.expected("a relationship arrow")),
                    };
                    arrow_type = match last.kind {
                        TokenKind::ForwardSolid => ArrowType::SolidArrow,
                        TokenKind::ForwardCircle => ArrowType::CircleArrow,
                        TokenKind::ForwardCross => ArrowType::CrossArrow,
                        _ => ArrowType::LineSolid,
                    };
                    minlen = match arrow_type {
                        ArrowType::SolidArrow | ArrowType::LineSolid => solid_minlen(last.image),
                        _ => 1,
                    };
                }
                Ok(ParsedArrow {
                    type_: RelationshipType::Association,
                    arrow_type,
                    label,
                    minlen,
                })
            }
            TokenKind::ForwardCircle => {
                self.bump();
                Ok(ParsedArrow {
                    type_: RelationshipType::Association,
                    arrow_type: ArrowType::CircleArrow,
                    label: None,
                    minlen: 1,
                })
            }
            TokenKind::BackwardCircle => {
                self.bump();
                let mut label = None;
                if self.has_labeled_right(&[TokenKind::MarkerlessSolid]) {
                    label = Some(self.parse_edge_label()?);
                    self.expect(TokenKind::MarkerlessSolid, "a markerless arrow")?;
                }
                Ok(ParsedArrow {
                    type_: RelationshipType::Association,
                    arrow_type: ArrowType::CircleArrowReversed,
                    label,
                    minlen: 1,
                })
            }
            TokenKind::ForwardCross => {
                self.bump();
                Ok(ParsedArrow {
                    type_: RelationshipType::Association,
                    arrow_type: ArrowType::CrossArrow,
                    label: None,
                    minlen: 1,
                })
            }
            TokenKind::BackwardCross => {
                self.bump();
                let mut label = None;
                if self.has_labeled_right(&[TokenKind::MarkerlessSolid]) {
                    label = Some(self.parse_edge_label()?);
                    self.expect(TokenKind::MarkerlessSolid, "a markerless arrow")?;
                }
                Ok(ParsedArrow {
                    type_: RelationshipType::Association,
                    arrow_type: ArrowType::CrossArrowReversed,
                    label,
                    minlen: 1,
                })
            }
            _ => Err(self.expected("a relationship arrow")),
        }
    }

    fn parse_semantic_relation(&mut self) -> Result<ParsedArrow, ParseError> {
        if self.peek() == TokenKind::Generalization {
            self.bump();
            return Ok(ParsedArrow {
                type_: RelationshipType::Generalization,
                arrow_type: ArrowType::SolidArrow,
                label: None,
                minlen: 1,
            });
        }
        self.expect(TokenKind::DependencyArrow, "a dependency arrow")?;
        self.expect(TokenKind::Colon, "a colon")?;
        let token = match self.peek() {
            TokenKind::Include | TokenKind::Extend => self.bump(),
            _ => return Err(self.expected("include or extend")),
        };
        let (type_, text) = if token.kind == TokenKind::Include {
            (RelationshipType::Include, "include")
        } else {
            (RelationshipType::Extend, "extend")
        };
        Ok(ParsedArrow {
            type_,
            arrow_type: ArrowType::SolidArrow,
            label: Some(DraftLabel {
                text: text.to_string(),
                type_: LabelType::Text,
                span: token.span,
            }),
            minlen: 1,
        })
    }

    fn parse_edge_label(&mut self) -> Result<DraftLabel, ParseError> {
        self.parse_node_label()
    }

    fn parse_node_label(&mut self) -> Result<DraftLabel, ParseError> {
        match self.peek() {
            TokenKind::PlainString | TokenKind::MarkdownString => {
                let token = self.bump();
                Ok(self.token_label(&token))
            }
            kind if kind.is_label_text() => {
                let start = self.position;
                while self.peek().is_label_text() {
                    self.bump();
                }
                let span = self.span_from(start);
                Ok(DraftLabel {
                    text: self.source[span.start..span.end].to_string(),
                    type_: LabelType::Text,
                    span,
                })
            }
            _ => Err(self.expected("a label")),
        }
    }

    fn parse_metadata(&mut self) -> Result<DraftMetadata, ParseError> {
        let start = self.position;
        self.expect(TokenKind::MetadataStart, "an opening @{")?;
        while self.peek() == TokenKind::Newline {
            self.bump();
        }
        let mut properties = Vec::new();
        if self.peek() != TokenKind::RightBrace {
            properties.push(self.parse_metadata_property()?);
            loop {
                if self.peek() == TokenKind::Comma {
                    let mut offset = 1;
                    while self.peek_at(offset) == TokenKind::Newline {
                        offset += 1;
                    }
                    if !is_property_start(self.peek_at(offset)) {
                        break;
                    }
                    self.bump();
                    while self.peek() == TokenKind::Newline {
                        self.bump();
                    }
                    properties.push(self.parse_metadata_property()?);
                } else if self.peek() == TokenKind::Newline {
                    let mut offset = 0;
                    while self.peek_at(offset) == TokenKind::Newline {
                        offset += 1;
                    }
                    if self.peek_at(offset) == TokenKind::Comma {
                        offset += 1;
                        while self.peek_at(offset) == TokenKind::Newline {
                            offset += 1;
                        }
                    }
                    if !is_property_start(self.peek_at(offset)) {
                        break;
                    }
                    while self.peek() == TokenKind::Newline {
                        self.bump();
                    }
                    if self.peek() == TokenKind::Comma {
                        self.bump();
                        while self.peek() == TokenKind::Newline {
                            self.bump();
                        }
                    }
                    properties.push(self.parse_metadata_property()?);
                } else {
                    break;
                }
            }
            if self.peek() == TokenKind::Comma {
                self.bump();
            }
            while self.peek() == TokenKind::Newline {
                self.bump();
            }
        }
        self.expect(TokenKind::RightBrace, "a closing }")?;
        Ok(DraftMetadata {
            properties,
            location: self.location_from(start),
        })
    }

    fn parse_metadata_property(&mut self) -> Result<DraftMetadataProperty, ParseError> {
        let key_token = match self.peek() {
            TokenKind::Identifier | TokenKind::PlainString => self.bump(),
            _ => return Err(self.expected("a metadata property name")),
        };
        let key = self.decode_plain(&key_token);
        self.expect(TokenKind::Colon, "a colon")?;
        let value_token = match self.peek() {
            TokenKind::Identifier | TokenKind::PlainString | TokenKind::True | TokenKind::False => {
                self.bump()
            }
            _ => return Err(self.expected("a metadata value")),
        };
        let value = match value_token.kind {
            TokenKind::True => MetadataValue::Bool(true),
            TokenKind::False => MetadataValue::Bool(false),
            _ => MetadataValue::String(self.decode_plain(&value_token)),
        };
        let span = Span {
            line: key_token.span.line,
            column: key_token.span.column,
            start: key_token.span.start,
            end: value_token.span.end,
        };
        Ok(DraftMetadataProperty {
            key,
            value,
            span,
            key_span: self.token_span(&key_token, string_trim(&key_token)),
            value_span: self.token_span(&value_token, string_trim(&value_token)),
            location: self.token_location(&key_token),
        })
    }

    fn parse_stereotype(&mut self) -> Result<ParsedStereotype, ParseError> {
        self.expect(TokenKind::StereotypeStart, "an opening <<")?;
        let token = self.expect(TokenKind::StereotypeText, "a stereotype")?;
        self.expect(TokenKind::StereotypeEnd, "a closing >>")?;
        Ok(ParsedStereotype {
            value: token.image.trim().to_string(),
            span: token.span,
        })
    }

    fn parse_class_suffix(&mut self) -> Result<ParsedClassSuffix, ParseError> {
        self.expect(TokenKind::ClassSeparator, ":::")?;
        let mut classes = Vec::new();
        let mut spans = Vec::new();
        let token = self.expect(TokenKind::Identifier, "a class name")?;
        classes.push(token.image.to_string());
        spans.push(token.span);
        while self.peek() == TokenKind::Comma {
            self.bump();
            let token = self.expect(TokenKind::Identifier, "a class name")?;
            classes.push(token.image.to_string());
            spans.push(token.span);
        }
        Ok(ParsedClassSuffix { classes, spans })
    }

    fn parse_system_boundary(&mut self) -> Result<usize, ParseError> {
        let start = self.position;
        self.expect(TokenKind::SystemBoundary, "the systemBoundary keyword")?;
        let mut boundary = self.parse_system_boundary_name()?;
        let metadata = if self.peek() == TokenKind::MetadataStart {
            Some(self.parse_metadata()?)
        } else {
            None
        };
        let mut class_spans = Vec::new();
        if self.peek() == TokenKind::ClassSeparator {
            let suffix = self.parse_class_suffix()?;
            boundary.classes = suffix.classes;
            class_spans = suffix.spans;
        }
        self.line_end()?;
        self.document.boundaries.push(boundary.clone());

        let previous = self.parent_boundary.clone();
        self.parent_boundary = Some((boundary.id.clone(), boundary.location));
        let children = self.parse_boundary_content();
        self.parent_boundary = previous;
        let children = children?;

        let end = self.expect(TokenKind::End, "the end keyword")?;
        self.line_end()?;
        let span = self.span_from(start);
        let mut statement = GraphStatement::new(StatementKind::Group, span);
        statement.group = Some(boundary.id.clone());
        statement.id_span = Some(boundary.location.span);
        statement.title_span = Some(boundary.label.span);
        statement.end_span = Some(end.span);
        statement.class_spans = class_spans;
        let assignment = metadata.clone();
        let index = self.document.add_statement(statement, children);
        if let Some(metadata) = assignment {
            self.document
                .metadata_assignments
                .push(DraftMetadataAssignment {
                    target: boundary.id.clone(),
                    target_location: boundary.location,
                    metadata: metadata.clone(),
                    statement: index,
                });
        }
        if let Some(metadata) = metadata {
            let occurrences = metadata_occurrences(&metadata);
            self.document.arena[index].statement.metadata = occurrences;
        }
        Ok(index)
    }

    fn parse_system_boundary_name(&mut self) -> Result<DraftBoundary, ParseError> {
        let (id, label, location, generated) = match self.peek() {
            TokenKind::Identifier => {
                let token = self.bump();
                let label = if self.peek() == TokenKind::LeftParen {
                    self.bump();
                    let label = self.parse_node_label()?;
                    self.expect(TokenKind::RightParen, "a closing parenthesis")?;
                    label
                } else if self.peek() == TokenKind::LeftBracket {
                    self.bump();
                    let label = self.parse_node_label()?;
                    self.expect(TokenKind::RightBracket, "a closing bracket")?;
                    label
                } else {
                    self.token_label(&token)
                };
                (
                    token.image.to_string(),
                    label,
                    self.token_location(&token),
                    false,
                )
            }
            TokenKind::PlainString | TokenKind::MarkdownString => {
                let token = self.bump();
                let label = self.token_label(&token);
                (
                    generate_id(&label.text),
                    label,
                    self.token_location(&token),
                    true,
                )
            }
            _ => return Err(self.expected("a system boundary name")),
        };
        Ok(DraftBoundary {
            id,
            label,
            location,
            generated,
            classes: Vec::new(),
        })
    }

    fn parse_boundary_content(&mut self) -> Result<Vec<usize>, ParseError> {
        let mut children = Vec::new();
        loop {
            match self.peek() {
                TokenKind::Newline => {
                    let token = self.bump();
                    let statement = GraphStatement::new(StatementKind::Blank, token.span);
                    children.push(self.document.add_statement(statement, Vec::new()));
                }
                TokenKind::Comment => {
                    let token = self.bump();
                    self.line_end()?;
                    let statement = GraphStatement::new(StatementKind::Comment, token.span);
                    children.push(self.document.add_statement(statement, Vec::new()));
                }
                TokenKind::End | TokenKind::Eof => break,
                _ => children.push(self.parse_boundary_element()?),
            }
        }
        Ok(children)
    }

    fn parse_boundary_element(&mut self) -> Result<usize, ParseError> {
        if self.peek() == TokenKind::Actor {
            return self.parse_actor_declaration_only();
        }
        let start = self.position;
        let mut entity = self.parse_entity_name()?;
        self.line_end()?;
        let span = self.span_from(start);
        entity.explicit_declaration = true;
        if entity.shape.is_none() {
            entity.shape = Some(UseCaseShape::Ellipse);
        }
        let draft = self.entity_draft(&entity);
        self.document.elements.push(draft);
        let mut statement = GraphStatement::new(StatementKind::Node, span);
        statement.nodes = vec![self.entity_occurrence(&entity, true)];
        Ok(self.document.add_statement(statement, Vec::new()))
    }

    fn parse_metadata_assignment(&mut self) -> Result<usize, ParseError> {
        let start = self.position;
        let token = self.bump();
        let id = if token.kind == TokenKind::Identifier {
            token.image.to_string()
        } else {
            generate_id(&self.token_label(&token).text)
        };
        let target_location = self.token_location(&token);
        let metadata = self.parse_metadata()?;
        self.line_end()?;
        let span = self.span_from(start);
        let mut statement = GraphStatement::new(StatementKind::Metadata, span);
        statement.nodes = vec![super::ast::NodeOccurrence {
            id: id.clone(),
            span: target_location.span,
            id_span: target_location.span,
            ..Default::default()
        }];
        statement.metadata = metadata_occurrences(&metadata);
        let index = self.document.add_statement(statement, Vec::new());
        self.document
            .metadata_assignments
            .push(DraftMetadataAssignment {
                target: id,
                target_location,
                metadata,
                statement: index,
            });
        Ok(index)
    }

    fn parse_note_statement(&mut self) -> Result<usize, ParseError> {
        let start = self.position;
        self.expect(TokenKind::Note, "the note keyword")?;
        self.expect(TokenKind::For, "the for keyword")?;
        let target = self.expect(TokenKind::Identifier, "a note target")?;
        let label = self.parse_node_label()?;
        self.line_end()?;
        let span = self.span_from(start);
        self.document.note_drafts.push(DraftNote {
            target: target.image.to_string(),
            target_location: self.token_location(&target),
            label: label.clone(),
            location: self.location_from(start),
        });
        let id = format!("note-{}", self.anonymous_note);
        self.anonymous_note += 1;
        let mut statement = GraphStatement::new(StatementKind::Note, span);
        statement.reference = Some(id);
        statement.reference_span = Some(label.span);
        statement.nodes = vec![super::ast::NodeOccurrence {
            id: target.image.to_string(),
            span: target.span,
            id_span: target.span,
            ..Default::default()
        }];
        Ok(self.document.add_statement(statement, Vec::new()))
    }

    fn parse_json_statement(&mut self) -> Result<usize, ParseError> {
        let start = self.position;
        let start_token = self.expect(TokenKind::JsonDeclarationStart, "a JSON declaration")?;
        let (id, relative) = json_id(start_token.image)
            .ok_or_else(|| ParseError::at("invalid JSON declaration", start_token.span))?;
        let literal = self.expect(TokenKind::JsonObjectLiteral, "a JSON object")?;
        let (value, property_order) = parse_ordered_json_object(
            literal.image,
            literal.span.line,
            literal.span.column,
            literal.span.start,
        )?;
        let mut classes = Vec::new();
        let mut class_spans = Vec::new();
        if self.peek() == TokenKind::ClassSeparator {
            let suffix = self.parse_class_suffix()?;
            classes = suffix.classes;
            class_spans = suffix.spans;
        }
        self.line_end()?;
        let span = self.span_from(start);
        let id_location = DraftLocation {
            span: Span {
                line: start_token.span.line,
                column: start_token.span.column + relative as u32,
                start: start_token.span.start + relative,
                end: start_token.span.start + relative + id.len(),
            },
            line: start_token.span.line,
            column: start_token.span.column + relative as u32,
        };
        self.document.json_drafts.push(DraftJson {
            id: id.clone(),
            value,
            property_order,
            location: id_location,
            classes: classes.clone(),
        });
        let mut statement = GraphStatement::new(StatementKind::Json, span);
        statement.nodes = vec![super::ast::NodeOccurrence {
            id,
            span: id_location.span,
            id_span: id_location.span,
            defines: true,
            class_spans: class_spans.clone(),
            ..Default::default()
        }];
        statement.class_spans = class_spans;
        Ok(self.document.add_statement(statement, Vec::new()))
    }

    fn parse_direction(&mut self) -> Result<usize, ParseError> {
        let start = self.position;
        self.expect(TokenKind::Direction, "the direction keyword")?;
        let token = match self.peek() {
            TokenKind::Td | TokenKind::Tb | TokenKind::Bt | TokenKind::Lr | TokenKind::Rl => {
                self.bump()
            }
            _ => return Err(self.expected("TD, TB, BT, LR, or RL")),
        };
        if let Some(direction) = super::ast::Direction::from_token(token.image) {
            self.document.directions.push(direction);
        }
        self.line_end()?;
        let statement = GraphStatement::new(StatementKind::Direction, self.span_from(start));
        Ok(self.document.add_statement(statement, Vec::new()))
    }

    fn parse_class_def_statement(&mut self) -> Result<usize, ParseError> {
        let start = self.position;
        self.expect(TokenKind::ClassDef, "the classDef keyword")?;
        let mut ids = Vec::new();
        let first = self.expect(TokenKind::Identifier, "a class name")?;
        ids.push((first.image.to_string(), first.span));
        while self.peek() == TokenKind::Comma {
            self.bump();
            let token = self.expect(TokenKind::Identifier, "a class name")?;
            ids.push((token.image.to_string(), token.span));
        }
        let styles = self.parse_styles()?;
        self.line_end()?;
        let span = self.span_from(start);
        self.document.class_definitions.push(DraftClassDefinition {
            ids: ids.iter().map(|(id, _)| id.clone()).collect(),
            styles,
        });
        let mut statement = GraphStatement::new(StatementKind::ClassDef, span);
        statement.reference = Some(ids[0].0.clone());
        statement.reference_span = Some(ids[0].1);
        Ok(self.document.add_statement(statement, Vec::new()))
    }

    fn parse_class_statement(&mut self) -> Result<usize, ParseError> {
        let start = self.position;
        self.expect(TokenKind::Class, "the class keyword")?;
        let first = self.expect(TokenKind::Identifier, "a class target")?;
        let mut ids = vec![(first.image.to_string(), first.span)];
        while self.peek() == TokenKind::Comma {
            self.bump();
            let token = self.expect(TokenKind::Identifier, "a class target")?;
            ids.push((token.image.to_string(), token.span));
        }
        let token = self.expect(TokenKind::Identifier, "a class name")?;
        ids.push((token.image.to_string(), token.span));
        while self.peek() == TokenKind::Comma {
            self.bump();
            let token = self.expect(TokenKind::Identifier, "a class name")?;
            ids.push((token.image.to_string(), token.span));
        }
        self.line_end()?;
        let span = self.span_from(start);

        let mut split = 1;
        while split < ids.len() {
            let between = &self.source[ids[split - 1].1.end..ids[split].1.start];
            if !between.contains(',') {
                break;
            }
            split += 1;
        }
        let targets: Vec<(String, DraftLocation)> = ids[..split]
            .iter()
            .map(|(id, span)| {
                (
                    id.clone(),
                    DraftLocation {
                        span: *span,
                        line: span.line,
                        column: span.column,
                    },
                )
            })
            .collect();
        let classes: Vec<String> = ids[split..].iter().map(|(id, _)| id.clone()).collect();
        self.document.class_assignments.push(DraftClassAssignment {
            targets: targets.clone(),
            classes: classes.clone(),
        });
        let mut statement = GraphStatement::new(StatementKind::ClassAssign, span);
        statement.reference = classes.first().cloned();
        statement.reference_span = ids.get(split).map(|(_, span)| *span);
        statement.nodes = targets
            .iter()
            .map(|(id, location)| super::ast::NodeOccurrence {
                id: id.clone(),
                span: location.span,
                id_span: location.span,
                ..Default::default()
            })
            .collect();
        Ok(self.document.add_statement(statement, Vec::new()))
    }

    fn parse_style_statement(&mut self) -> Result<usize, ParseError> {
        let start = self.position;
        self.expect(TokenKind::Style, "the style keyword")?;
        let target = self.expect(TokenKind::Identifier, "a style target")?;
        let styles = self.parse_styles()?;
        self.line_end()?;
        let span = self.span_from(start);
        self.document.style_assignments.push(DraftStyleAssignment {
            target: target.image.to_string(),
            location: self.token_location(&target),
            styles,
        });
        let mut statement = GraphStatement::new(StatementKind::Style, span);
        statement.nodes = vec![super::ast::NodeOccurrence {
            id: target.image.to_string(),
            span: target.span,
            id_span: target.span,
            ..Default::default()
        }];
        Ok(self.document.add_statement(statement, Vec::new()))
    }

    fn parse_styles(&mut self) -> Result<Vec<String>, ParseError> {
        let mut styles = vec![self.parse_style_value()?];
        while self.peek() == TokenKind::Comma {
            self.bump();
            styles.push(self.parse_style_value()?);
        }
        Ok(styles)
    }

    fn parse_style_value(&mut self) -> Result<String, ParseError> {
        let start = self.position;
        if self.peek().is_word() || self.peek() == TokenKind::CssIdentifier {
            self.bump();
        } else if self.peek() == TokenKind::MarkerlessSolid && self.peek_at(1).is_word() {
            self.bump();
            self.bump();
        } else {
            return Err(self.expected("a style property"));
        }
        self.expect(TokenKind::Colon, "a colon")?;
        if !self.peek().is_style_component() {
            return Err(self.expected("a style value"));
        }
        while self.peek().is_style_component() {
            self.bump();
        }
        let span = self.span_from(start);
        Ok(self.source[span.start..span.end].replace("\\,", ","))
    }

    // ----- drafts --------------------------------------------------------------------

    fn actor_draft(&self, item: &ParsedActorItem) -> DraftElement {
        DraftElement {
            id: item.id.clone(),
            kind: ElementKind::Actor,
            label: item.label.clone(),
            location: item.location,
            generated: item.generated,
            parent_id: self.parent_boundary.as_ref().map(|(id, _)| id.clone()),
            parent_location: self.parent_boundary.as_ref().map(|(_, location)| *location),
            shape: None,
            metadata: item.metadata.clone(),
            stereotype: item
                .stereotype
                .as_ref()
                .map(|stereotype| stereotype.value.clone()),
            classes: item.classes.clone(),
        }
    }

    fn entity_draft(&self, entity: &ParsedEntity) -> DraftElement {
        DraftElement {
            id: entity.id.clone(),
            kind: ElementKind::Usecase,
            label: entity.label.clone(),
            location: entity.location,
            generated: entity.generated,
            parent_id: self.parent_boundary.as_ref().map(|(id, _)| id.clone()),
            parent_location: self.parent_boundary.as_ref().map(|(_, location)| *location),
            shape: entity.shape,
            metadata: entity.metadata.clone(),
            stereotype: entity
                .stereotype
                .as_ref()
                .map(|stereotype| stereotype.value.clone()),
            classes: entity.classes.clone(),
        }
    }

    fn actor_endpoint(&self, item: &ParsedActorItem) -> DraftEndpoint {
        DraftEndpoint {
            id: item.id.clone(),
            label: item.label.clone(),
            location: item.location,
            generated: item.generated,
            declaration: true,
            classes_on_reference: !item.classes.is_empty(),
        }
    }

    fn entity_endpoint(&self, entity: &ParsedEntity) -> DraftEndpoint {
        DraftEndpoint {
            id: entity.id.clone(),
            label: entity.label.clone(),
            location: entity.location,
            generated: entity.generated,
            declaration: entity.explicit_declaration,
            classes_on_reference: !entity.classes.is_empty(),
        }
    }

    fn actor_occurrence(
        &self,
        item: &ParsedActorItem,
        defines: bool,
    ) -> super::ast::NodeOccurrence {
        super::ast::NodeOccurrence {
            id: item.id.clone(),
            span: item.node_span,
            id_span: item.location.span,
            label_span: Some(item.label.span),
            defines,
            stereotype_span: item.stereotype.as_ref().map(|stereotype| stereotype.span),
            metadata: item
                .metadata
                .as_ref()
                .map(metadata_occurrences)
                .unwrap_or_default(),
            class_spans: item.class_spans.clone(),
        }
    }

    fn entity_occurrence(
        &self,
        entity: &ParsedEntity,
        defines: bool,
    ) -> super::ast::NodeOccurrence {
        super::ast::NodeOccurrence {
            id: entity.id.clone(),
            span: entity.node_span,
            id_span: entity.location.span,
            label_span: Some(entity.label.span),
            defines,
            stereotype_span: entity.stereotype.as_ref().map(|stereotype| stereotype.span),
            metadata: entity
                .metadata
                .as_ref()
                .map(metadata_occurrences)
                .unwrap_or_default(),
            class_spans: entity.class_spans.clone(),
        }
    }
}

fn is_property_start(kind: TokenKind) -> bool {
    matches!(kind, TokenKind::Identifier | TokenKind::PlainString)
}

fn string_trim(token: &Token<'_>) -> usize {
    match token.kind {
        TokenKind::PlainString => 1,
        TokenKind::MarkdownString => 2,
        _ => 0,
    }
}

fn metadata_occurrences(metadata: &DraftMetadata) -> Vec<MetadataOccurrence> {
    metadata
        .properties
        .iter()
        .map(|property| MetadataOccurrence {
            key: property.key.clone(),
            span: property.span,
            key_span: property.key_span,
            value_span: property.value_span,
        })
        .collect()
}

/// `generateId`: `label.replace(/\W/g, '_')`, one replacement per UTF-16 code unit, as
/// JavaScript's non-Unicode `\W` sees it.
fn generate_id(label: &str) -> String {
    let mut id = String::with_capacity(label.len());
    for ch in label.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            id.push(ch);
        } else {
            for _ in 0..ch.len_utf16() {
                id.push('_');
            }
        }
    }
    id
}

fn solid_minlen(image: &str) -> u32 {
    let dashes = image.bytes().filter(|byte| *byte == b'-').count() as u32;
    dashes.saturating_sub(1).max(1)
}

/// `^json[\t ]+(\w+)` and the matched prefix length, so the id's offset is exact.
fn json_id(image: &str) -> Option<(String, usize)> {
    let rest = image.strip_prefix("json")?;
    let bytes = rest.as_bytes();
    let mut index = 0;
    while bytes.get(index) == Some(&b' ') || bytes.get(index) == Some(&b'\t') {
        index += 1;
    }
    if index == 0 {
        return None;
    }
    let start = index;
    while bytes
        .get(index)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    {
        index += 1;
    }
    if index == start {
        return None;
    }
    let id = rest[start..index].to_string();
    let relative = "json".len() + start;
    Some((id, relative))
}

/// `parseOrderedJsonObject`: validates the object root and records each object's property
/// order under its escaped JSON Pointer.
pub(crate) fn parse_ordered_json_object(
    text: &str,
    start_line: u32,
    start_column: u32,
    absolute_start: usize,
) -> Result<(Value, Map<String, Value>), ParseError> {
    let mut parser = JsonParser {
        text,
        offset: 0,
        property_order: Map::new(),
    };
    parser.skip_whitespace();
    if parser.peek() != Some('{') {
        let location = json_location(text, parser.offset, start_line, start_column);
        return Err(json_error(
            "JSON value must have an object root",
            absolute_start + parser.offset,
            location,
        ));
    }
    let value = parser.parse_value("").map_err(|offset| {
        let location = json_location(text, offset, start_line, start_column);
        json_error("Invalid JSON", absolute_start + offset, location)
    })?;
    parser.skip_whitespace();
    if parser.offset != text.len() {
        let location = json_location(text, parser.offset, start_line, start_column);
        return Err(json_error(
            "Invalid JSON: unexpected trailing characters",
            absolute_start + parser.offset,
            location,
        ));
    }
    Ok((value, parser.property_order))
}

fn json_error(message: &str, offset: usize, location: (u32, u32)) -> ParseError {
    ParseError::at(
        format!("{message} (line {}, column {})", location.0, location.1),
        Span {
            line: location.0,
            column: location.1,
            start: offset,
            end: offset,
        },
    )
}

fn json_location(text: &str, offset: usize, start_line: u32, start_column: u32) -> (u32, u32) {
    let end = offset.min(text.len());
    let mut line = start_line;
    let mut column = start_column;
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < end {
        let byte = bytes[index];
        if byte == b'\r' {
            line += 1;
            column = 1;
            index += 1;
            if index < end && bytes[index] == b'\n' {
                index += 1;
            }
        } else if byte == b'\n' {
            line += 1;
            column = 1;
            index += 1;
        } else {
            column += 1;
            index += 1;
        }
    }
    (line, column)
}

struct JsonParser<'a> {
    text: &'a str,
    offset: usize,
    property_order: Map<String, Value>,
}

impl JsonParser<'_> {
    fn peek(&self) -> Option<char> {
        self.text[self.offset..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.offset += ch.len_utf8();
        Some(ch)
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t' | '\n' | '\r')) {
            self.bump();
        }
    }

    fn parse_value(&mut self, pointer: &str) -> Result<Value, usize> {
        self.skip_whitespace();
        match self.peek() {
            Some('{') => self.parse_object(pointer),
            Some('[') => self.parse_array(pointer),
            Some('"') => self.parse_string().map(Value::String),
            Some('t') => self.parse_literal("true", Value::Bool(true)),
            Some('f') => self.parse_literal("false", Value::Bool(false)),
            Some('n') => self.parse_literal("null", Value::Null),
            Some('-' | '0'..='9') => self.parse_number(),
            _ => Err(self.offset),
        }
    }

    fn parse_object(&mut self, pointer: &str) -> Result<Value, usize> {
        self.bump();
        let mut order: Vec<String> = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        self.property_order
            .insert(pointer.to_string(), Value::Array(Vec::new()));
        let mut object = Map::new();
        self.skip_whitespace();
        if self.peek() == Some('}') {
            self.bump();
            return Ok(Value::Object(object));
        }
        loop {
            if self.peek() != Some('"') {
                return Err(self.offset);
            }
            let property = self.parse_string()?;
            let escaped = property.replace('~', "~0").replace('/', "~1");
            let property_pointer = format!("{pointer}/{escaped}");
            if seen.contains(&property) {
                delete_pointer_subtree(&mut self.property_order, &property_pointer);
            } else {
                seen.push(property.clone());
                order.push(property.clone());
            }
            self.skip_whitespace();
            if self.peek() != Some(':') {
                return Err(self.offset);
            }
            self.bump();
            let value = self.parse_value(&property_pointer)?;
            object.insert(property, value);
            self.skip_whitespace();
            match self.peek() {
                Some('}') => {
                    self.bump();
                    self.property_order.insert(
                        pointer.to_string(),
                        Value::Array(order.into_iter().map(Value::String).collect()),
                    );
                    return Ok(Value::Object(object));
                }
                Some(',') => {
                    self.bump();
                    self.skip_whitespace();
                }
                _ => return Err(self.offset),
            }
        }
    }

    fn parse_array(&mut self, pointer: &str) -> Result<Value, usize> {
        self.bump();
        self.skip_whitespace();
        let mut values = Vec::new();
        if self.peek() == Some(']') {
            self.bump();
            return Ok(Value::Array(values));
        }
        let mut index = 0;
        loop {
            let value = self.parse_value(&format!("{pointer}/{index}"))?;
            values.push(value);
            index += 1;
            self.skip_whitespace();
            match self.peek() {
                Some(']') => {
                    self.bump();
                    return Ok(Value::Array(values));
                }
                Some(',') => {
                    self.bump();
                    self.skip_whitespace();
                }
                _ => return Err(self.offset),
            }
        }
    }

    fn parse_string(&mut self) -> Result<String, usize> {
        self.bump();
        let mut value = String::new();
        loop {
            let character_offset = self.offset;
            let Some(ch) = self.bump() else {
                return Err(self.offset);
            };
            if ch == '"' {
                return Ok(value);
            }
            if (ch as u32) < 0x20 {
                return Err(character_offset);
            }
            if ch != '\\' {
                value.push(ch);
                continue;
            }
            let escape_offset = self.offset;
            let Some(escape) = self.bump() else {
                return Err(self.offset);
            };
            match escape {
                '"' | '\\' | '/' => value.push(escape),
                'b' => value.push('\u{8}'),
                'f' => value.push('\u{c}'),
                'n' => value.push('\n'),
                'r' => value.push('\r'),
                't' => value.push('\t'),
                'u' => {
                    let code = self
                        .text
                        .get(self.offset..self.offset + 4)
                        .filter(|code| code.chars().all(|ch| ch.is_ascii_hexdigit()))
                        .ok_or(self.offset)?;
                    let code = u32::from_str_radix(code, 16).map_err(|_| self.offset)?;
                    self.offset += 4;
                    // JSON.parse reads UTF-16 code units, so a surrogate pair is one
                    // character. A lone surrogate has no Rust `char`; U+FFFD stands in.
                    let scalar = if (0xD800..=0xDBFF).contains(&code) {
                        let low = self
                            .text
                            .get(self.offset..self.offset + 6)
                            .filter(|low| low.starts_with("\\u"))
                            .and_then(|low| u32::from_str_radix(&low[2..], 16).ok())
                            .filter(|low| (0xDC00..=0xDFFF).contains(low));
                        if let Some(low) = low {
                            self.offset += 6;
                            0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00)
                        } else {
                            0xFFFD
                        }
                    } else if (0xDC00..=0xDFFF).contains(&code) {
                        0xFFFD
                    } else {
                        code
                    };
                    value.push(char::from_u32(scalar).unwrap_or('\u{FFFD}'));
                }
                _ => return Err(escape_offset),
            }
        }
    }

    fn parse_literal(&mut self, literal: &str, value: Value) -> Result<Value, usize> {
        if self.text[self.offset..].starts_with(literal) {
            self.offset += literal.len();
            Ok(value)
        } else {
            Err(self.offset)
        }
    }

    fn parse_number(&mut self) -> Result<Value, usize> {
        let start = self.offset;
        if self.peek() == Some('-') {
            self.bump();
        }
        match self.peek() {
            Some('0') => {
                self.bump();
            }
            Some('1'..='9') => {
                while matches!(self.peek(), Some('0'..='9')) {
                    self.bump();
                }
            }
            _ => return Err(self.offset),
        }
        if self.peek() == Some('.') {
            self.bump();
            if !matches!(self.peek(), Some('0'..='9')) {
                return Err(self.offset);
            }
            while matches!(self.peek(), Some('0'..='9')) {
                self.bump();
            }
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            self.bump();
            if matches!(self.peek(), Some('+' | '-')) {
                self.bump();
            }
            if !matches!(self.peek(), Some('0'..='9')) {
                return Err(self.offset);
            }
            while matches!(self.peek(), Some('0'..='9')) {
                self.bump();
            }
        }
        let raw = &self.text[start..self.offset];
        if raw.contains(['.', 'e', 'E']) {
            return raw
                .parse::<f64>()
                .ok()
                .and_then(serde_json::Number::from_f64)
                .map(Value::Number)
                .ok_or(start);
        }
        if let Ok(value) = raw.parse::<i64>() {
            return Ok(Value::Number(value.into()));
        }
        if let Ok(value) = raw.parse::<u64>() {
            return Ok(Value::Number(value.into()));
        }
        raw.parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .ok_or(start)
    }
}

fn delete_pointer_subtree(order: &mut Map<String, Value>, pointer: &str) {
    let descendant_prefix = format!("{pointer}/");
    order.retain(|existing, _| existing != pointer && !existing.starts_with(&descendant_prefix));
}
