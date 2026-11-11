//! The ER grammar as a recursive-descent parser: upstream
//! `packages/mermaid/src/diagrams/er/parser/erDiagram.jison`.
//!
//! Every production of the jison file is here. The grammar is a sequence of statements
//! separated by optional newlines (`document: document line`, `line: statement | NEWLINE |
//! EOF`), so two statements may share a line and a statement ends where the next token
//! cannot continue it. As upstream's actions do, each statement updates the database while
//! the parse runs, which keeps entity and relationship order, and the inner-then-outer
//! order of nested subgraphs, identical to upstream.
//!
//! Quirks kept on purpose: a `classDef`, `class`, or `style` statement returns its keyword
//! text as the statement's value, and an `accTitle`/`accDescr` statement returns its text,
//! so a subgraph body counts those strings as nodes — exactly as upstream's `$$` values do.

use super::ast::{Ast, Attribute, Cardinality, Identification, RelSpec};
use super::db::{Db, Item};
use super::lexer::{Direction, Lexer, Token, TokenKind, js_trim};
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
    /// `subgraphDepth`: at zero a `direction` statement sets the diagram's direction;
    /// inside a subgraph it is kept by that subgraph instead.
    subgraph_depth: usize,
}

impl<'a> Parser<'a> {
    fn new(mut lexer: Lexer<'a>) -> Result<Self, ParseError> {
        let token = lexer.next_token()?;
        Ok(Self {
            lexer,
            token,
            db: Db::new(),
            subgraph_depth: 0,
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

    fn expect(&mut self, kind: TokenKind, expected: &str) -> Result<(), ParseError> {
        if !self.at(kind) {
            return Err(self.unexpected(expected));
        }
        self.bump()
    }

    /// `start: 'ER_DIAGRAM' document 'EOF'`.
    fn run(&mut self) -> Result<(), ParseError> {
        if !self.at(TokenKind::ErDiagram) {
            return Err(self.unexpected("'erDiagram'"));
        }
        self.bump()?;
        self.document(false)?;
        if !self.at(TokenKind::Eof) {
            return Err(self.unexpected("the end of the diagram"));
        }
        Ok(())
    }

    /// `document`: statements and newlines until the end of the input, or until `end`
    /// inside a subgraph.
    fn document(&mut self, in_subgraph: bool) -> Result<Vec<Item>, ParseError> {
        let mut items = Vec::new();
        loop {
            if self.at(TokenKind::Eof) || self.at(TokenKind::EndOfInput) {
                break;
            }
            if in_subgraph && self.at(TokenKind::End) {
                break;
            }
            if self.at(TokenKind::Newline) {
                self.bump()?;
                continue;
            }
            items.extend(self.statement()?);
        }
        Ok(items)
    }

    fn statement(&mut self) -> Result<Vec<Item>, ParseError> {
        match self.token.kind {
            TokenKind::AccTitle => {
                self.bump()?;
                if !self.at(TokenKind::AccTitleValue) {
                    return Err(self.unexpected("the accessibility title text"));
                }
                let title = js_trim(&self.token.text).to_string();
                self.bump()?;
                self.db.set_acc_title(&title);
                Ok(vec![Item::Node(title)])
            }
            TokenKind::AccDescr => {
                self.bump()?;
                if !self.at(TokenKind::AccDescrValue) {
                    return Err(self.unexpected("the accessibility description text"));
                }
                let description = js_trim(&self.token.text).to_string();
                self.bump()?;
                self.db.set_acc_description(&description);
                Ok(vec![Item::Node(description)])
            }
            TokenKind::AccDescrMultilineValue => {
                let description = js_trim(&self.token.text).to_string();
                self.bump()?;
                self.db.set_acc_description(&description);
                Ok(vec![Item::Node(description)])
            }
            TokenKind::Direction(direction) => {
                self.bump()?;
                if self.subgraph_depth == 0 {
                    self.db.set_direction(direction.value());
                    Ok(Vec::new())
                } else {
                    Ok(vec![Item::Dir(direction.value().to_string())])
                }
            }
            TokenKind::ClassDef => {
                let word = self.token.text.clone();
                self.bump()?;
                let ids = self.id_list()?;
                let styles = self.styles_opt()?;
                self.separator()?;
                self.db.add_class(&ids, &styles);
                Ok(vec![Item::Node(word)])
            }
            TokenKind::Class => {
                let word = self.token.text.clone();
                self.bump()?;
                let ids = self.id_list()?;
                let class_names = self.id_list()?;
                self.db.set_class(&ids, &class_names);
                Ok(vec![Item::Node(word)])
            }
            TokenKind::Style => {
                let word = self.token.text.clone();
                self.bump()?;
                let ids = self.id_list()?;
                let styles = self.styles_opt()?;
                self.separator()?;
                self.db.add_css_styles(&ids, &styles);
                Ok(vec![Item::Node(word)])
            }
            TokenKind::Subgraph => self.subgraph_statement(),
            kind if is_entity_start(kind) => self.entity_statement(),
            _ => Err(self.unexpected("a statement")),
        }
    }

    // -- entities and relationships ------------------------------------------------------

    fn entity_statement(&mut self) -> Result<Vec<Item>, ParseError> {
        let name = self.entity_name()?;
        match self.token.kind {
            TokenKind::Sqs => self.alias_statement(name),
            TokenKind::BlockStart => {
                self.bump()?;
                let attributes = self.attributes()?;
                self.expect(TokenKind::BlockStop, "'}'")?;
                self.db.add_entity(&name, "");
                if !attributes.is_empty() {
                    self.db.add_attributes(&name, attributes);
                }
                Ok(vec![Item::Node(name)])
            }
            TokenKind::StyleSeparator => {
                self.bump()?;
                let classes = self.id_list()?;
                if self.at(TokenKind::BlockStart) {
                    self.bump()?;
                    let attributes = self.attributes()?;
                    self.expect(TokenKind::BlockStop, "'}'")?;
                    self.db.add_entity(&name, "");
                    if !attributes.is_empty() {
                        self.db.add_attributes(&name, attributes);
                    }
                    self.db.set_class(std::slice::from_ref(&name), &classes);
                    Ok(vec![Item::Node(name)])
                } else if is_cardinality(self.token.kind) {
                    self.relationship(name, Some(classes))
                } else {
                    self.db.add_entity(&name, "");
                    self.db.set_class(std::slice::from_ref(&name), &classes);
                    Ok(vec![Item::Node(name)])
                }
            }
            kind if is_cardinality(kind) => self.relationship(name, None),
            _ => {
                self.db.add_entity(&name, "");
                Ok(vec![Item::Node(name)])
            }
        }
    }

    /// `entityName SQS entityName SQE` with its optional class list and attribute block.
    fn alias_statement(&mut self, name: String) -> Result<Vec<Item>, ParseError> {
        self.bump()?;
        let alias = self.entity_name()?;
        self.expect(TokenKind::Sqe, "']'")?;
        let classes = if self.at(TokenKind::StyleSeparator) {
            self.bump()?;
            Some(self.id_list()?)
        } else {
            None
        };
        if self.at(TokenKind::BlockStart) {
            self.bump()?;
            let attributes = self.attributes()?;
            self.expect(TokenKind::BlockStop, "'}'")?;
            self.db.add_entity(&name, &alias);
            if !attributes.is_empty() {
                self.db.add_attributes(&name, attributes);
            }
        } else {
            self.db.add_entity(&name, &alias);
        }
        if let Some(classes) = classes {
            self.db.set_class(std::slice::from_ref(&name), &classes);
        }
        Ok(vec![Item::Node(name)])
    }

    /// `entityName relSpec entityName COLON role`, with the optional class list the
    /// grammar allows on either entity name.
    fn relationship(
        &mut self,
        ent_a: String,
        class_a: Option<Vec<String>>,
    ) -> Result<Vec<Item>, ParseError> {
        let card_b = self.cardinality()?;
        let rel_type = self.identification()?;
        let card_a = self.cardinality()?;
        let rel_spec = RelSpec {
            card_a,
            card_b,
            rel_type,
        };
        let ent_b = self.entity_name()?;
        let class_b = if self.at(TokenKind::StyleSeparator) {
            self.bump()?;
            Some(self.id_list()?)
        } else {
            None
        };
        self.expect(TokenKind::Colon, "':'")?;
        let role = self.role()?;
        self.db.add_entity(&ent_a, "");
        self.db.add_entity(&ent_b, "");
        self.db.add_relationship(&ent_a, &role, &ent_b, rel_spec);
        if let Some(classes) = class_a {
            self.db.set_class(std::slice::from_ref(&ent_a), &classes);
        }
        if let Some(classes) = class_b {
            self.db.set_class(std::slice::from_ref(&ent_b), &classes);
        }
        Ok(vec![Item::Node(ent_a), Item::Node(ent_b)])
    }

    fn cardinality(&mut self) -> Result<Cardinality, ParseError> {
        let cardinality = match self.token.kind {
            TokenKind::ZeroOrOne => Cardinality::ZeroOrOne,
            TokenKind::ZeroOrMore => Cardinality::ZeroOrMore,
            TokenKind::OneOrMore => Cardinality::OneOrMore,
            TokenKind::OnlyOne => Cardinality::OnlyOne,
            TokenKind::MdParent => Cardinality::MdParent,
            _ => return Err(self.unexpected("a cardinality")),
        };
        self.bump()?;
        Ok(cardinality)
    }

    fn identification(&mut self) -> Result<Identification, ParseError> {
        let identification = match self.token.kind {
            TokenKind::NonIdentifying => Identification::NonIdentifying,
            TokenKind::Identifying => Identification::Identifying,
            _ => return Err(self.unexpected("a relationship type")),
        };
        self.bump()?;
        Ok(identification)
    }

    // -- attributes ---------------------------------------------------------------------

    /// `attributes: attribute | attribute attributes`, in source order.
    fn attributes(&mut self) -> Result<Vec<Attribute>, ParseError> {
        let mut attributes = Vec::new();
        while self.at(TokenKind::AttributeWord) {
            attributes.push(self.attribute()?);
        }
        Ok(attributes)
    }

    fn attribute(&mut self) -> Result<Attribute, ParseError> {
        let mut type_name = self.attribute_word("an attribute type")?;
        if self.at(TokenKind::Char('?')) {
            type_name.push('?');
            self.bump()?;
        }
        let name = self.attribute_word("an attribute name")?;
        let mut keys = Vec::new();
        loop {
            if self.at(TokenKind::AttributeKey) {
                keys.push(self.token.text.clone());
                self.bump()?;
                continue;
            }
            if self.at(TokenKind::Char(',')) {
                self.bump()?;
                if !self.at(TokenKind::AttributeKey) {
                    return Err(self.unexpected("an attribute key"));
                }
                keys.push(self.token.text.clone());
                self.bump()?;
                continue;
            }
            break;
        }
        let comment = if self.at(TokenKind::Comment) {
            let comment = strip_quotes(&self.token.text);
            self.bump()?;
            comment
        } else {
            String::new()
        };
        Ok(Attribute {
            type_name,
            name,
            keys,
            comment,
        })
    }

    fn attribute_word(&mut self, expected: &str) -> Result<String, ParseError> {
        if !self.at(TokenKind::AttributeWord) {
            return Err(self.unexpected(expected));
        }
        let text = self.token.text.clone();
        self.bump()?;
        Ok(text)
    }

    // -- names --------------------------------------------------------------------------

    /// `entityName`: a quoted name, or unquoted text, a number, a decimal, or `1`.
    fn entity_name(&mut self) -> Result<String, ParseError> {
        match self.token.kind {
            TokenKind::EntityName => {
                let name = strip_quotes(&self.token.text);
                self.bump()?;
                Ok(name)
            }
            TokenKind::UnicodeText
            | TokenKind::Num
            | TokenKind::DecimalNum
            | TokenKind::EntityOne => {
                let name = self.token.text.clone();
                self.bump()?;
                Ok(name)
            }
            _ => Err(self.unexpected("an entity name")),
        }
    }

    /// `role`: a quoted word or quoted entity name, or unquoted text.
    fn role(&mut self) -> Result<String, ParseError> {
        match self.token.kind {
            TokenKind::Word | TokenKind::EntityName => {
                let role = strip_quotes(&self.token.text);
                self.bump()?;
                Ok(role)
            }
            TokenKind::UnicodeText => {
                let role = self.token.text.clone();
                self.bump()?;
                Ok(role)
            }
            _ => Err(self.unexpected("a relationship label")),
        }
    }

    fn id_list(&mut self) -> Result<Vec<String>, ParseError> {
        let mut ids = vec![self.id_item()?];
        while self.at(TokenKind::Comma) {
            self.bump()?;
            ids.push(self.id_item()?);
        }
        Ok(ids)
    }

    fn id_item(&mut self) -> Result<String, ParseError> {
        match self.token.kind {
            TokenKind::UnicodeText | TokenKind::StyleText => {
                let id = self.token.text.clone();
                self.bump()?;
                Ok(id)
            }
            _ => Err(self.unexpected("an id")),
        }
    }

    // -- subgraphs ----------------------------------------------------------------------

    fn subgraph_statement(&mut self) -> Result<Vec<Item>, ParseError> {
        self.bump()?;
        let id = self.entity_name()?;
        let title = if self.at(TokenKind::Sqs) {
            self.bump()?;
            let title = self.subgraph_title()?;
            self.expect(TokenKind::Sqe, "']'")?;
            title
        } else {
            id.clone()
        };
        self.separator()?;
        self.subgraph_depth += 1;
        let body = self.document(true)?;
        if !self.at(TokenKind::End) {
            return Err(self.unexpected("'end'"));
        }
        self.bump()?;
        self.subgraph_depth -= 1;
        let id = self.db.add_sub_graph(&id, &title, &body, None);
        Ok(vec![Item::Node(id)])
    }

    /// `subgraphTitle: entityName | subgraphTitle entityName`, joined with spaces.
    fn subgraph_title(&mut self) -> Result<String, ParseError> {
        let mut parts = vec![self.entity_name()?];
        while is_entity_start(self.token.kind) {
            parts.push(self.entity_name()?);
        }
        Ok(parts.join(" "))
    }

    /// `separator: SEMI | NEWLINE | EOF`. `SEMI` only exists inside the style condition,
    /// which a subgraph header cannot be in; it is kept for fidelity.
    fn separator(&mut self) -> Result<(), ParseError> {
        match self.token.kind {
            TokenKind::Semi | TokenKind::Newline | TokenKind::Eof => self.bump(),
            _ => Err(self.unexpected("a line break")),
        }
    }

    // -- styles -------------------------------------------------------------------------

    /// `stylesOpt: style | stylesOpt COMMA style`.
    fn styles_opt(&mut self) -> Result<Vec<String>, ParseError> {
        let mut styles = vec![self.style()?];
        while self.at(TokenKind::Comma) {
            self.bump()?;
            styles.push(self.style()?);
        }
        Ok(styles)
    }

    /// `style: styleComponent | style styleComponent`.
    fn style(&mut self) -> Result<String, ParseError> {
        let mut style = String::new();
        while let TokenKind::StyleText | TokenKind::Num | TokenKind::Colon | TokenKind::Brkt =
            self.token.kind
        {
            style.push_str(&self.token.text);
            self.bump()?;
        }
        if style.is_empty() {
            return Err(self.unexpected("a style"));
        }
        Ok(style)
    }
}

fn is_entity_start(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::EntityName
            | TokenKind::UnicodeText
            | TokenKind::Num
            | TokenKind::DecimalNum
            | TokenKind::EntityOne
    )
}

fn is_cardinality(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::ZeroOrOne
            | TokenKind::ZeroOrMore
            | TokenKind::OneOrMore
            | TokenKind::OnlyOne
            | TokenKind::MdParent
    )
}

/// `$1.replace(/"/g, '')`.
fn strip_quotes(text: &str) -> String {
    text.replace('"', "")
}

fn describe(token: &Token) -> String {
    match token.kind {
        TokenKind::Eof => "the end of the input".to_string(),
        TokenKind::EndOfInput => {
            "the end of the input (no 'EOF' token in this lexer state)".to_string()
        }
        TokenKind::Newline => "a newline".to_string(),
        TokenKind::AccTitle => "'accTitle'".to_string(),
        TokenKind::AccDescr => "'accDescr'".to_string(),
        TokenKind::AccTitleValue
        | TokenKind::AccDescrValue
        | TokenKind::AccDescrMultilineValue
        | TokenKind::EntityName
        | TokenKind::Word
        | TokenKind::AttributeWord
        | TokenKind::Comment
        | TokenKind::Num
        | TokenKind::DecimalNum
        | TokenKind::UnicodeText
        | TokenKind::StyleText => format!("'{}'", token.text),
        TokenKind::ErDiagram => "'erDiagram'".to_string(),
        TokenKind::BlockStart => "'{'".to_string(),
        TokenKind::BlockStop => "'}'".to_string(),
        TokenKind::Brkt => "'#'".to_string(),
        TokenKind::Comma => "','".to_string(),
        TokenKind::StyleSeparator => "':::'".to_string(),
        TokenKind::Colon => "':'".to_string(),
        TokenKind::AttributeKey => format!("'{}'", token.text),
        TokenKind::Sqs => "'['".to_string(),
        TokenKind::Sqe => "']'".to_string(),
        TokenKind::Style => "'style'".to_string(),
        TokenKind::ClassDef => "'classDef'".to_string(),
        TokenKind::Class => "'class'".to_string(),
        TokenKind::Subgraph => "'subgraph'".to_string(),
        TokenKind::End => "'end'".to_string(),
        TokenKind::ZeroOrOne => "'zero or one'".to_string(),
        TokenKind::ZeroOrMore => "'zero or more'".to_string(),
        TokenKind::OneOrMore => "'one or more'".to_string(),
        TokenKind::OnlyOne => "'only one'".to_string(),
        TokenKind::MdParent => "'u'".to_string(),
        TokenKind::EntityOne => "'1'".to_string(),
        TokenKind::NonIdentifying => "'non-identifying'".to_string(),
        TokenKind::Identifying => "'identifying'".to_string(),
        TokenKind::Semi => "';'".to_string(),
        TokenKind::Char(ch) => format!("'{ch}'"),
        TokenKind::Direction(direction) => match direction {
            Direction::Tb => "'direction TB'".to_string(),
            Direction::Bt => "'direction BT'".to_string(),
            Direction::Rl => "'direction RL'".to_string(),
            Direction::Lr => "'direction LR'".to_string(),
        },
    }
}
