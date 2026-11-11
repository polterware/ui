//! The C4 grammar: a hand-written recursive-descent port of the `%%` section of upstream
//! `diagrams/c4/parser/c4Diagram.jison`.
//!
//! ```text
//! start: mermaidDoc | direction
//! mermaidDoc: (C4Context|C4Container|C4Component|C4Dynamic|C4Deployment) NEWLINE statements EOF
//! statements: otherStatements | diagramStatements | otherStatements diagramStatements
//! otherStatement: title | accDescription | acc_title acc_title_value
//!               | acc_descr acc_descr_value | acc_descr_multiline_value
//! diagramStatement: PERSON attributes | ... | boundaryStatement | REL attributes | ...
//! boundaryStatement: boundaryStart (LBRACE NEWLINE | NEWLINE LBRACE | NEWLINE LBRACE NEWLINE)
//!                    diagramStatements RBRACE
//! attributes: attribute | attribute attributes
//! attribute: STR | STR_KEY STR_VALUE | ATTRIBUTE_EMPTY
//! ```
//!
//! jison is an LALR(1) generator, so the left recursion of `otherStatements` and
//! `diagramStatements` becomes a loop here: a statement is followed by a `NEWLINE` (or the
//! end of the input, or the `}` that closes a boundary) and then another statement. A
//! comment consumes its own line break, so a statement and the next one are joined without a
//! `NEWLINE` token exactly as upstream's parser would see them, and are a parse error.
//!
//! `title`/`accTitle`/`accDescr` are only accepted at the top level: a boundary body is
//! `diagramStatements`, which has no `otherStatement`.

use super::ast::Ast;
use super::db::{Attr, Db};
use super::lexer::{Lexer, Token, TokenKind};
use crate::error::ParseError;

/// Parses the diagram body and populates the database.
pub(crate) fn parse(body: &str) -> Result<Ast, ParseError> {
    Parser::new(body).parse_document().map(Db::into_ast)
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
            db: Db::new(),
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

    fn parse_document(mut self) -> Result<Db, ParseError> {
        if let TokenKind::Direction(direction) = self.peek()? {
            self.bump()?;
            self.db.set_direction(direction.as_str());
            self.expect(TokenKind::Eof)?;
            return Ok(self.db);
        }

        let header = self.bump()?;
        let c4_type = match header.kind {
            TokenKind::C4Context => "C4Context",
            TokenKind::C4Container => "C4Container",
            TokenKind::C4Component => "C4Component",
            TokenKind::C4Dynamic => "C4Dynamic",
            TokenKind::C4Deployment => "C4Deployment",
            _ => {
                return Err(ParseError::at(
                    format!(
                        "expected C4Context, C4Container, C4Component, C4Dynamic, or C4Deployment, got '{}'",
                        header.kind.name()
                    ),
                    header.span,
                ));
            }
        };
        self.db.set_c4_type(c4_type);
        self.expect(TokenKind::Newline)?;
        self.parse_statements(false)?;
        self.expect(TokenKind::Eof)?;
        Ok(self.db)
    }

    /// `statements`, as a loop over `diagramStatement`/`otherStatement` separated by a
    /// `NEWLINE`; the last one may end the input or the enclosing boundary directly.
    fn parse_statements(&mut self, in_boundary: bool) -> Result<(), ParseError> {
        loop {
            self.parse_statement(in_boundary)?;
            match self.peek()? {
                TokenKind::Newline => {
                    self.bump()?;
                }
                TokenKind::Eof => return Ok(()),
                TokenKind::Rbrace if in_boundary => return Ok(()),
                _ => return Err(self.unexpected("NEWLINE")),
            }
            match self.peek()? {
                TokenKind::Eof => return Ok(()),
                TokenKind::Rbrace if in_boundary => return Ok(()),
                _ => {}
            }
        }
    }

    fn parse_statement(&mut self, in_boundary: bool) -> Result<(), ParseError> {
        if !in_boundary {
            match self.peek()? {
                TokenKind::Title => {
                    let token = self.bump()?;
                    self.db.set_title(token.text.get(6..).unwrap_or(""));
                    return Ok(());
                }
                TokenKind::AccDescription => {
                    let token = self.bump()?;
                    self.db
                        .set_acc_description(token.text.get(15..).unwrap_or(""));
                    return Ok(());
                }
                TokenKind::AccTitle => {
                    self.bump()?;
                    let value = self.expect(TokenKind::AccTitleValue)?;
                    self.db.set_title(value.text.trim());
                    return Ok(());
                }
                TokenKind::AccDescr => {
                    self.bump()?;
                    let value = self.expect(TokenKind::AccDescrValue)?;
                    self.db.set_acc_description(value.text.trim());
                    return Ok(());
                }
                TokenKind::AccDescrMultilineValue => {
                    let value = self.bump()?;
                    self.db.set_acc_description(value.text.trim());
                    return Ok(());
                }
                _ => {}
            }
        }

        match self.peek()? {
            TokenKind::Person => self.element("person", Element::Person),
            TokenKind::PersonExt => self.element("external_person", Element::Person),
            TokenKind::System => self.element("system", Element::Person),
            TokenKind::SystemDb => self.element("system_db", Element::Person),
            TokenKind::SystemQueue => self.element("system_queue", Element::Person),
            TokenKind::SystemExt => self.element("external_system", Element::Person),
            TokenKind::SystemExtDb => self.element("external_system_db", Element::Person),
            TokenKind::SystemExtQueue => self.element("external_system_queue", Element::Person),
            TokenKind::Container => self.element("container", Element::Container),
            TokenKind::ContainerDb => self.element("container_db", Element::Container),
            TokenKind::ContainerQueue => self.element("container_queue", Element::Container),
            TokenKind::ContainerExt => self.element("external_container", Element::Container),
            TokenKind::ContainerExtDb => self.element("external_container_db", Element::Container),
            TokenKind::ContainerExtQueue => {
                self.element("external_container_queue", Element::Container)
            }
            TokenKind::Component => self.element("component", Element::Component),
            TokenKind::ComponentDb => self.element("component_db", Element::Component),
            TokenKind::ComponentQueue => self.element("component_queue", Element::Component),
            TokenKind::ComponentExt => self.element("external_component", Element::Component),
            TokenKind::ComponentExtDb => self.element("external_component_db", Element::Component),
            TokenKind::ComponentExtQueue => {
                self.element("external_component_queue", Element::Component)
            }
            TokenKind::EnterpriseBoundary => {
                self.boundary("ENTERPRISE", "system", BoundaryKind::Text)
            }
            TokenKind::SystemBoundary => self.boundary("SYSTEM", "system", BoundaryKind::Text),
            TokenKind::Boundary => self.boundary("", "system", BoundaryKind::Text),
            TokenKind::ContainerBoundary => {
                self.boundary("CONTAINER", "container", BoundaryKind::Text)
            }
            TokenKind::Node => self.boundary("", "node", BoundaryKind::Node("node")),
            TokenKind::NodeL => self.boundary("", "node", BoundaryKind::Node("nodeL")),
            TokenKind::NodeR => self.boundary("", "node", BoundaryKind::Node("nodeR")),
            TokenKind::Rel => self.rel("rel"),
            TokenKind::BiRel => self.rel("birel"),
            TokenKind::RelU => self.rel("rel_u"),
            TokenKind::RelD => self.rel("rel_d"),
            TokenKind::RelL => self.rel("rel_l"),
            TokenKind::RelR => self.rel("rel_r"),
            TokenKind::RelB => self.rel("rel_b"),
            TokenKind::RelIndex => {
                self.bump()?;
                let mut attrs = self.parse_attributes()?;
                attrs.remove(0);
                self.db.add_rel("rel", &attrs);
                Ok(())
            }
            TokenKind::UpdateElStyle => {
                self.bump()?;
                let attrs = self.parse_attributes()?;
                self.db.update_el_style(&attrs);
                Ok(())
            }
            TokenKind::UpdateRelStyle => {
                self.bump()?;
                let attrs = self.parse_attributes()?;
                self.db.update_rel_style(&attrs);
                Ok(())
            }
            TokenKind::UpdateLayoutConfig => {
                self.bump()?;
                let attrs = self.parse_attributes()?;
                self.db.update_layout_config(&attrs);
                Ok(())
            }
            _ => Err(self.unexpected("a statement")),
        }
    }

    /// A person/system/container/component declaration.
    fn element(&mut self, type_c4_shape: &str, element: Element) -> Result<(), ParseError> {
        self.bump()?;
        let attrs = self.parse_attributes()?;
        match element {
            Element::Person => self.db.add_person_or_system(type_c4_shape, &attrs),
            Element::Container => self.db.add_container(type_c4_shape, &attrs),
            Element::Component => self.db.add_component(type_c4_shape, &attrs),
        }
        Ok(())
    }

    /// A boundary or deployment node declaration and its body.
    fn boundary(
        &mut self,
        splice: &str,
        default_type: &str,
        kind: BoundaryKind,
    ) -> Result<(), ParseError> {
        self.bump()?;
        let mut attrs = self.parse_attributes()?;
        if !splice.is_empty() {
            attrs.insert(attrs.len().min(2), Attr::Text(splice.to_string()));
        }
        match kind {
            BoundaryKind::Text => self.db.add_text_boundary(default_type, &attrs),
            BoundaryKind::Node(node_type) => self.db.add_deployment_node(node_type, &attrs),
        }

        match self.peek()? {
            TokenKind::Lbrace => {
                self.bump()?;
                self.expect(TokenKind::Newline)?;
            }
            TokenKind::Newline => {
                self.bump()?;
                self.expect(TokenKind::Lbrace)?;
                if self.peek()? == TokenKind::Newline {
                    self.bump()?;
                }
            }
            _ => return Err(self.unexpected("LBRACE or NEWLINE")),
        }
        self.parse_statements(true)?;
        self.expect(TokenKind::Rbrace)?;
        self.db.pop_boundary_parse_stack();
        Ok(())
    }

    fn rel(&mut self, type_name: &str) -> Result<(), ParseError> {
        self.bump()?;
        let attrs = self.parse_attributes()?;
        self.db.add_rel(type_name, &attrs);
        Ok(())
    }

    fn parse_attributes(&mut self) -> Result<Vec<Attr>, ParseError> {
        let mut attrs = Vec::new();
        loop {
            match self.peek()? {
                TokenKind::AttributeEmpty => {
                    self.bump()?;
                    attrs.push(Attr::Text(String::new()));
                }
                TokenKind::Str => {
                    let token = self.bump()?;
                    attrs.push(Attr::Text(token.text.trim().to_string()));
                }
                TokenKind::StrKey => {
                    let key = self.bump()?;
                    let value = self.expect(TokenKind::StrValue)?;
                    attrs.push(Attr::Named {
                        key: key.text.trim().to_string(),
                        value: value.text.trim().to_string(),
                    });
                }
                _ => break,
            }
        }
        if attrs.is_empty() {
            return Err(self.unexpected("an attribute"));
        }
        Ok(attrs)
    }
}

#[derive(Clone, Copy)]
enum Element {
    Person,
    Container,
    Component,
}

#[derive(Clone, Copy)]
enum BoundaryKind {
    Text,
    Node(&'static str),
}
