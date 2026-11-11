//! The flowchart grammar: a hand-written recursive-descent port of upstream `parser/flow.jison`.
//!
//! The grammar is small but stateful: whitespace is significant, `&` groups vertices, a link
//! can carry a label before or between its two halves, and a subgraph body is parsed as its own
//! `document` until `end`. Actions call the database methods in the same order the jison rules
//! do, which keeps ids, the vertex counter, and the edge counter identical.

use super::ast::Ast;
use super::db::{Db, FlowText, Link, LinkPosition, StatementValue};
use super::lexer::{Direction, Lexer, Token, TokenKind};
use crate::error::{ParseError, Span};

/// Parses the diagram body. The body starts with the header line; frontmatter and `%%{init}%%`
/// directives are already removed (see [`crate::preprocess`]).
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    let mut parser = Parser::new(body);
    parser.parse_diagram()?;
    Ok(parser.db.into_ast())
}

struct VertexStatement {
    nodes: Vec<String>,
}

struct Parser<'a> {
    lexer: Lexer<'a>,
    lookahead: Option<Token>,
    last_span: Span,
    db: Db,
}

impl<'a> Parser<'a> {
    fn new(body: &'a str) -> Self {
        Self {
            lexer: Lexer::new(body),
            lookahead: None,
            last_span: Span::default(),
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

    fn peek_span(&mut self) -> Result<Span, ParseError> {
        Ok(self.fill()?.span)
    }

    fn bump(&mut self) -> Result<Token, ParseError> {
        self.fill()?;
        let token = self.lookahead.take().expect("filled above");
        self.last_span = token.span;
        Ok(token)
    }

    fn expect(&mut self, expected: TokenKind) -> Result<Token, ParseError> {
        let kind = self.peek()?;
        if kind != expected {
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

    fn skip_spaces(&mut self) -> Result<usize, ParseError> {
        let mut count = 0;
        while self.peek()? == TokenKind::Space {
            self.bump()?;
            count += 1;
        }
        Ok(count)
    }

    fn expect_separator(&mut self) -> Result<(), ParseError> {
        match self.peek()? {
            TokenKind::Newline | TokenKind::Semi => {
                self.bump()?;
                Ok(())
            }
            TokenKind::Eof => Ok(()),
            _ => Err(self.unexpected("NEWLINE, SEMI, or EOF")),
        }
    }

    fn expect_first_statement_separator(&mut self) -> Result<(), ParseError> {
        match self.peek()? {
            TokenKind::Semi | TokenKind::Newline => {
                self.bump()?;
                Ok(())
            }
            TokenKind::Space => {
                self.skip_spaces()?;
                self.expect(TokenKind::Newline)?;
                Ok(())
            }
            _ => Err(self.unexpected("SEMI, NEWLINE, or spaces and a NEWLINE")),
        }
    }

    fn parse_diagram(&mut self) -> Result<(), ParseError> {
        while matches!(self.peek()?, TokenKind::Space | TokenKind::Newline) {
            self.bump()?;
        }
        self.expect(TokenKind::Graph)?;
        match self.peek()? {
            TokenKind::Nodir => {
                self.bump()?;
                self.db.set_direction("TB");
            }
            TokenKind::Dir => {
                let token = self.bump()?;
                self.db.set_direction(&token.text);
                self.expect_first_statement_separator()?;
            }
            _ => return Err(self.unexpected("NODIR or DIR")),
        }
        self.parse_document(false)?;
        Ok(())
    }

    /// A `document`: statements and blank lines, ending at `end` (when `in_subgraph`) or at EOF.
    fn parse_document(&mut self, in_subgraph: bool) -> Result<Vec<StatementValue>, ParseError> {
        let mut lines = Vec::new();
        loop {
            match self.peek()? {
                TokenKind::Eof => {
                    self.bump()?;
                    break;
                }
                TokenKind::End if in_subgraph => break,
                TokenKind::End => return Err(self.unexpected("NEWLINE, SEMI, or EOF")),
                TokenKind::Semi | TokenKind::Newline | TokenKind::Space => {
                    let token = self.bump()?;
                    lines.push(StatementValue::Text(token.text));
                }
                _ => {
                    if let Some(line) = self.parse_statement()? {
                        lines.push(line);
                    }
                }
            }
        }
        Ok(lines)
    }

    fn parse_statement(&mut self) -> Result<Option<StatementValue>, ParseError> {
        match self.peek()? {
            TokenKind::AccTitle => {
                self.bump()?;
                let value = self.expect(TokenKind::AccTitleValue)?;
                let value = value.text.trim().to_string();
                self.db.set_acc_title(&value);
                Ok(Some(StatementValue::Text(value)))
            }
            TokenKind::AccDescr => {
                self.bump()?;
                let value = self.expect(TokenKind::AccDescrValue)?;
                let value = value.text.trim().to_string();
                self.db.set_acc_description(&value);
                Ok(Some(StatementValue::Text(value)))
            }
            TokenKind::AccDescrMultilineValue => {
                let value = self.bump()?.text;
                let value = value.trim().to_string();
                self.db.set_acc_description(&value);
                Ok(Some(StatementValue::Text(value)))
            }
            TokenKind::Subgraph => {
                let id = self.parse_subgraph()?;
                Ok(Some(StatementValue::Text(id)))
            }
            TokenKind::Style => {
                self.parse_style_statement()?;
                self.expect_separator()?;
                Ok(None)
            }
            TokenKind::LinkStyle => {
                self.parse_link_style_statement()?;
                self.expect_separator()?;
                Ok(None)
            }
            TokenKind::ClassDef => {
                self.parse_class_def_statement()?;
                self.expect_separator()?;
                Ok(None)
            }
            TokenKind::Class => {
                self.parse_class_statement()?;
                self.expect_separator()?;
                Ok(None)
            }
            TokenKind::Click => {
                let id = self.parse_click_statement()?;
                self.expect_separator()?;
                Ok(Some(StatementValue::Text(id)))
            }
            TokenKind::Direction(direction) => {
                self.bump()?;
                let value = match direction {
                    Direction::Tb => "TB",
                    Direction::Bt => "BT",
                    Direction::Rl => "RL",
                    Direction::Lr => "LR",
                    Direction::Td => "TD",
                };
                Ok(Some(StatementValue::Direction(value.to_string())))
            }
            _ => {
                let statement = self.parse_vertex_statement()?;
                self.expect_separator()?;
                Ok(Some(StatementValue::Nodes(statement.nodes)))
            }
        }
    }

    fn parse_vertex_statement(&mut self) -> Result<VertexStatement, ParseError> {
        let first = self.parse_node()?;
        let mut stmt = first.clone();
        let mut nodes = first;
        loop {
            if !matches!(
                self.peek()?,
                TokenKind::Link | TokenKind::LinkId | TokenKind::StartLink
            ) {
                break;
            }
            let link = self.parse_link()?;
            let span = self.peek_span()?;
            let next = self.parse_node()?;
            self.db.add_link(&stmt, &next, &link, span)?;
            stmt = next.clone();
            let mut combined = next;
            combined.extend(nodes);
            nodes = combined;
        }
        Ok(VertexStatement { nodes })
    }

    /// A node group: one styled vertex, extended by `&`-separated vertices, each able to carry
    /// trailing `@{ }` metadata for the vertex before it.
    fn parse_node(&mut self) -> Result<Vec<String>, ParseError> {
        let mut ids = vec![self.parse_styled_vertex()?];
        loop {
            let shape = if self.peek()? == TokenKind::ShapeData {
                Some(self.parse_shape_data()?)
            } else {
                None
            };
            self.skip_spaces()?;
            if self.peek()? != TokenKind::Amp {
                if let Some((shape, span)) = shape {
                    let id = ids.last().cloned().expect("a node group has a vertex");
                    self.db.add_vertex(
                        &id,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        Some(&shape),
                        span,
                    )?;
                }
                break;
            }
            self.bump()?;
            self.skip_spaces()?;
            let vertex = self.parse_styled_vertex()?;
            if let Some((shape, span)) = shape {
                let id = ids.last().cloned().expect("a node group has a vertex");
                self.db
                    .add_vertex(&id, None, None, None, None, None, None, Some(&shape), span)?;
            }
            ids.push(vertex);
        }
        Ok(ids)
    }

    fn parse_styled_vertex(&mut self) -> Result<String, ParseError> {
        let id = self.parse_vertex()?;
        if self.peek()? == TokenKind::StyleSeparator {
            self.bump()?;
            let class = self.parse_id_string()?;
            self.db.set_class(&id, &class);
        }
        Ok(id)
    }

    fn parse_vertex(&mut self) -> Result<String, ParseError> {
        let id = self.parse_id_string()?;
        let span = self.last_span;
        let (type_name, text, props) = match self.peek()? {
            TokenKind::Sqs => {
                self.bump()?;
                let text = self.parse_text(TokenKind::Sqe)?;
                self.expect(TokenKind::Sqe)?;
                (Some("square"), Some(text), None)
            }
            TokenKind::DoubleCircleStart => {
                self.bump()?;
                let text = self.parse_text(TokenKind::DoubleCircleEnd)?;
                self.expect(TokenKind::DoubleCircleEnd)?;
                (Some("doublecircle"), Some(text), None)
            }
            TokenKind::Ps => {
                self.bump()?;
                if self.peek()? == TokenKind::Ps {
                    self.bump()?;
                    let text = self.parse_text(TokenKind::Pe)?;
                    self.expect(TokenKind::Pe)?;
                    self.expect(TokenKind::Pe)?;
                    (Some("circle"), Some(text), None)
                } else {
                    let text = self.parse_text(TokenKind::Pe)?;
                    self.expect(TokenKind::Pe)?;
                    (Some("round"), Some(text), None)
                }
            }
            TokenKind::EllipseStart => {
                self.bump()?;
                let text = self.parse_text(TokenKind::EllipseEnd)?;
                self.expect(TokenKind::EllipseEnd)?;
                (Some("ellipse"), Some(text), None)
            }
            TokenKind::StadiumStart => {
                self.bump()?;
                let text = self.parse_text(TokenKind::StadiumEnd)?;
                self.expect(TokenKind::StadiumEnd)?;
                (Some("stadium"), Some(text), None)
            }
            TokenKind::SubroutineStart => {
                self.bump()?;
                let text = self.parse_text(TokenKind::SubroutineEnd)?;
                self.expect(TokenKind::SubroutineEnd)?;
                (Some("subroutine"), Some(text), None)
            }
            TokenKind::VertexWithPropsStart => {
                self.bump()?;
                let field = self.expect(TokenKind::NodeString)?.text;
                self.expect(TokenKind::Colon)?;
                let value = self.expect(TokenKind::NodeString)?.text;
                self.expect(TokenKind::Pipe)?;
                let text = self.parse_text(TokenKind::Sqe)?;
                self.expect(TokenKind::Sqe)?;
                let mut props = serde_json::Map::new();
                props.insert(field, serde_json::Value::String(value));
                (Some("rect"), Some(text), Some(props))
            }
            TokenKind::CylinderStart => {
                self.bump()?;
                let text = self.parse_text(TokenKind::CylinderEnd)?;
                self.expect(TokenKind::CylinderEnd)?;
                (Some("cylinder"), Some(text), None)
            }
            TokenKind::DiamondStart => {
                self.bump()?;
                if self.peek()? == TokenKind::DiamondStart {
                    self.bump()?;
                    let text = self.parse_text(TokenKind::DiamondStop)?;
                    self.expect(TokenKind::DiamondStop)?;
                    self.expect(TokenKind::DiamondStop)?;
                    (Some("hexagon"), Some(text), None)
                } else {
                    let text = self.parse_text(TokenKind::DiamondStop)?;
                    self.expect(TokenKind::DiamondStop)?;
                    (Some("diamond"), Some(text), None)
                }
            }
            TokenKind::TagEnd => {
                self.bump()?;
                let text = self.parse_text(TokenKind::Sqe)?;
                self.expect(TokenKind::Sqe)?;
                (Some("odd"), Some(text), None)
            }
            TokenKind::TrapStart => {
                self.bump()?;
                let text = self.parse_text_any(&[TokenKind::TrapEnd, TokenKind::InvTrapEnd])?;
                let closer = self.bump()?.kind;
                let type_name = if closer == TokenKind::TrapEnd {
                    "trapezoid"
                } else {
                    "lean_right"
                };
                (Some(type_name), Some(text), None)
            }
            TokenKind::InvTrapStart => {
                self.bump()?;
                let text = self.parse_text_any(&[TokenKind::TrapEnd, TokenKind::InvTrapEnd])?;
                let closer = self.bump()?.kind;
                let type_name = if closer == TokenKind::TrapEnd {
                    "lean_left"
                } else {
                    "inv_trapezoid"
                };
                (Some(type_name), Some(text), None)
            }
            _ => (None, None, None),
        };
        self.db
            .add_vertex(&id, text, type_name, None, None, None, props, None, span)?;
        Ok(id)
    }

    fn parse_link(&mut self) -> Result<Link, ParseError> {
        // `linkStatement START_LINK edgeText LINK` and `LINK_ID START_LINK edgeText LINK`
        // carry their label between the two halves; `LINK` and `LINK_ID LINK` take an optional
        // `|text|` label after the statement instead.
        let mut id = None;
        if self.peek()? == TokenKind::LinkId {
            id = Some(self.bump()?.text);
        }
        if self.peek()? == TokenKind::StartLink {
            let start = self.bump()?.text;
            let text = self.parse_edge_text()?;
            let end = self.expect(TokenKind::Link)?.text;
            let info = self.db.destruct_link(&end, Some(&start));
            return Ok(Link {
                type_name: info.type_name,
                stroke: info.stroke,
                length: info.length,
                text: Some(text),
                id,
            });
        }
        let end = self.expect(TokenKind::Link)?.text;
        let info = self.db.destruct_link(&end, None);
        let mut link = Link {
            type_name: info.type_name,
            stroke: info.stroke,
            length: info.length,
            text: None,
            id,
        };
        if self.peek()? == TokenKind::Pipe {
            self.bump()?;
            let text = self.parse_text(TokenKind::Pipe)?;
            self.expect(TokenKind::Pipe)?;
            link.text = Some(text);
            if self.peek()? == TokenKind::Space {
                self.bump()?;
            }
        } else if self.peek()? == TokenKind::Space {
            self.bump()?;
        }
        Ok(link)
    }

    fn parse_edge_text(&mut self) -> Result<FlowText, ParseError> {
        match self.peek()? {
            TokenKind::Str => {
                let token = self.bump()?;
                Ok(FlowText {
                    text: token.text,
                    label_type: super::ast::LabelType::String,
                })
            }
            TokenKind::MdStr => {
                let token = self.bump()?;
                Ok(FlowText {
                    text: token.text,
                    label_type: super::ast::LabelType::Markdown,
                })
            }
            _ => {
                let mut text = String::new();
                while matches!(self.peek()?, TokenKind::EdgeText | TokenKind::UnicodeText) {
                    text.push_str(&self.bump()?.text);
                }
                if text.is_empty() {
                    return Err(self.unexpected("edge text"));
                }
                Ok(FlowText {
                    text,
                    label_type: super::ast::LabelType::Text,
                })
            }
        }
    }

    fn parse_text(&mut self, terminator: TokenKind) -> Result<FlowText, ParseError> {
        self.parse_text_any(&[terminator])
    }

    /// The `text` / `textNoTags` / `edgeText` productions: either one string token or a run of
    /// text tokens. `terminators` names the tokens that end the run, as a safety check.
    fn parse_text_any(&mut self, terminators: &[TokenKind]) -> Result<FlowText, ParseError> {
        match self.peek()? {
            TokenKind::Str => {
                let token = self.bump()?;
                return Ok(FlowText {
                    text: token.text,
                    label_type: super::ast::LabelType::String,
                });
            }
            TokenKind::MdStr => {
                let token = self.bump()?;
                return Ok(FlowText {
                    text: token.text,
                    label_type: super::ast::LabelType::Markdown,
                });
            }
            _ => {}
        }
        let mut text = String::new();
        loop {
            let kind = self.peek()?;
            if terminators.contains(&kind) || !is_text_token(kind) {
                break;
            }
            text.push_str(&self.bump()?.text);
        }
        if text.is_empty() {
            return Err(self.unexpected("text"));
        }
        Ok(FlowText {
            text,
            label_type: super::ast::LabelType::Text,
        })
    }

    fn parse_text_no_tags(&mut self) -> Result<FlowText, ParseError> {
        match self.peek()? {
            TokenKind::Str => {
                let token = self.bump()?;
                return Ok(FlowText {
                    text: token.text,
                    label_type: super::ast::LabelType::Text,
                });
            }
            TokenKind::MdStr => {
                let token = self.bump()?;
                return Ok(FlowText {
                    text: token.text,
                    label_type: super::ast::LabelType::Markdown,
                });
            }
            _ => {}
        }
        let mut text = String::new();
        while is_text_no_tags_token(self.peek()?) {
            text.push_str(&self.bump()?.text);
        }
        if text.is_empty() {
            return Err(self.unexpected("subgraph title"));
        }
        Ok(FlowText {
            text,
            label_type: super::ast::LabelType::Text,
        })
    }

    fn parse_shape_data(&mut self) -> Result<(String, Span), ParseError> {
        let mut text = String::new();
        let mut span = self.peek_span()?;
        let mut first = true;
        while self.peek()? == TokenKind::ShapeData {
            if first {
                span = self.peek_span()?;
                first = false;
            }
            text.push_str(&self.bump()?.text);
        }
        Ok((text, span))
    }

    fn parse_id_string(&mut self) -> Result<String, ParseError> {
        let mut text = String::new();
        while is_id_string_token(self.peek()?) {
            text.push_str(&self.bump()?.text);
        }
        if text.is_empty() {
            return Err(self.unexpected("node id"));
        }
        Ok(text)
    }

    fn parse_alpha_num(&mut self) -> Result<String, ParseError> {
        let mut text = String::new();
        while is_alpha_num_token(self.peek()?) {
            text.push_str(&self.bump()?.text);
        }
        if text.is_empty() {
            return Err(self.unexpected("number or name"));
        }
        Ok(text)
    }

    fn parse_num_list(&mut self) -> Result<Vec<LinkPosition>, ParseError> {
        let mut positions = Vec::new();
        loop {
            let number = self.expect(TokenKind::Num)?;
            let index = number
                .text
                .parse::<usize>()
                .map_err(|_| ParseError::at("invalid link index", number.span))?;
            positions.push(LinkPosition::Index(index));
            if self.peek()? != TokenKind::Comma {
                break;
            }
            self.bump()?;
        }
        Ok(positions)
    }

    fn parse_styles(&mut self) -> Result<Vec<String>, ParseError> {
        let mut styles = Vec::new();
        loop {
            let mut style = String::new();
            while is_style_component(self.peek()?) {
                style.push_str(&self.bump()?.text);
            }
            if style.is_empty() {
                return Err(self.unexpected("style"));
            }
            styles.push(style);
            if self.peek()? != TokenKind::Comma {
                break;
            }
            self.bump()?;
        }
        Ok(styles)
    }

    fn parse_style_statement(&mut self) -> Result<(), ParseError> {
        self.expect(TokenKind::Style)?;
        self.expect(TokenKind::Space)?;
        let id = self.parse_id_string()?;
        self.expect(TokenKind::Space)?;
        let styles = self.parse_styles()?;
        self.db.add_vertex(
            &id,
            None,
            None,
            Some(&styles),
            None,
            None,
            None,
            None,
            self.last_span,
        )
    }

    fn parse_class_def_statement(&mut self) -> Result<(), ParseError> {
        self.expect(TokenKind::ClassDef)?;
        self.expect(TokenKind::Space)?;
        let ids = self.parse_id_string()?;
        self.expect(TokenKind::Space)?;
        let styles = self.parse_styles()?;
        self.db.add_class(&ids, &styles);
        Ok(())
    }

    fn parse_class_statement(&mut self) -> Result<(), ParseError> {
        self.expect(TokenKind::Class)?;
        self.expect(TokenKind::Space)?;
        let ids = self.parse_id_string()?;
        self.expect(TokenKind::Space)?;
        let class = self.parse_id_string()?;
        self.db.set_class(&ids, &class);
        Ok(())
    }

    fn parse_link_style_statement(&mut self) -> Result<(), ParseError> {
        self.expect(TokenKind::LinkStyle)?;
        self.expect(TokenKind::Space)?;
        let positions = if self.peek()? == TokenKind::Default {
            self.bump()?;
            vec![LinkPosition::Default]
        } else {
            self.parse_num_list()?
        };
        self.expect(TokenKind::Space)?;
        if self.peek()? == TokenKind::Interpolate {
            self.bump()?;
            self.expect(TokenKind::Space)?;
            let interpolate = self.parse_alpha_num()?;
            if self.peek()? == TokenKind::Space {
                self.bump()?;
                let styles = self.parse_styles()?;
                self.db
                    .update_link_interpolate(&positions, &interpolate, self.last_span)?;
                self.db.update_link(&positions, &styles, self.last_span)?;
            } else {
                self.db
                    .update_link_interpolate(&positions, &interpolate, self.last_span)?;
            }
        } else {
            let styles = self.parse_styles()?;
            self.db.update_link(&positions, &styles, self.last_span)?;
        }
        Ok(())
    }

    fn parse_click_statement(&mut self) -> Result<String, ParseError> {
        let click = self.expect(TokenKind::Click)?;
        let id = click.text;
        match self.peek()? {
            TokenKind::CallbackName => {
                self.bump()?;
                self.db.set_click_event(&id);
                if self.peek()? == TokenKind::CallbackArgs {
                    self.bump()?;
                }
                if self.peek()? == TokenKind::Space {
                    self.bump()?;
                    let tooltip = self.expect(TokenKind::Str)?;
                    self.db.set_tooltip(&id, &tooltip.text);
                }
            }
            TokenKind::Href => {
                self.bump()?;
                let link = self.expect(TokenKind::Str)?;
                self.parse_click_link(&id, &link.text)?;
            }
            TokenKind::Str => {
                let link = self.bump()?;
                self.parse_click_link(&id, &link.text)?;
            }
            _ => {
                let _name = self.parse_alpha_num()?;
                self.db.set_click_event(&id);
                if self.peek()? == TokenKind::Space {
                    self.bump()?;
                    let tooltip = self.expect(TokenKind::Str)?;
                    self.db.set_tooltip(&id, &tooltip.text);
                }
            }
        }
        Ok(id)
    }

    fn parse_click_link(&mut self, id: &str, link: &str) -> Result<(), ParseError> {
        let mut tooltip = None;
        let mut target = None;
        if self.peek()? == TokenKind::Space {
            self.bump()?;
            match self.peek()? {
                TokenKind::Str => {
                    tooltip = Some(self.bump()?.text);
                    if self.peek()? == TokenKind::Space {
                        self.bump()?;
                        let token = self.expect(TokenKind::LinkTarget)?;
                        target = Some(token.text);
                    }
                }
                TokenKind::LinkTarget => {
                    let token = self.bump()?;
                    target = Some(token.text);
                }
                _ => return Err(self.unexpected("STR or LINK_TARGET")),
            }
        }
        self.db.set_link(id, link, target.as_deref());
        if let Some(tooltip) = tooltip {
            self.db.set_tooltip(id, &tooltip);
        }
        Ok(())
    }

    fn parse_subgraph(&mut self) -> Result<String, ParseError> {
        self.expect(TokenKind::Subgraph)?;
        if self.peek()? == TokenKind::Space {
            self.bump()?;
            let name = self.parse_text_no_tags()?;
            if self.peek()? == TokenKind::Sqs {
                self.bump()?;
                let title = self.parse_text(TokenKind::Sqe)?;
                self.expect(TokenKind::Sqe)?;
                self.expect_separator_required()?;
                let body = self.parse_document(true)?;
                self.expect(TokenKind::End)?;
                Ok(self
                    .db
                    .add_sub_graph(Some(&name), Some(&title), false, &body))
            } else {
                self.expect_separator_required()?;
                let body = self.parse_document(true)?;
                self.expect(TokenKind::End)?;
                Ok(self.db.add_sub_graph(Some(&name), Some(&name), true, &body))
            }
        } else {
            self.expect_separator_required()?;
            let body = self.parse_document(true)?;
            self.expect(TokenKind::End)?;
            Ok(self.db.add_sub_graph(None, None, false, &body))
        }
    }

    fn expect_separator_required(&mut self) -> Result<(), ParseError> {
        match self.peek()? {
            TokenKind::Newline | TokenKind::Semi => {
                self.bump()?;
                Ok(())
            }
            _ => Err(self.unexpected("NEWLINE or SEMI")),
        }
    }
}

fn is_text_token(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Text | TokenKind::TagStart | TokenKind::TagEnd | TokenKind::UnicodeText
    )
}

fn is_text_no_tags_token(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Num
            | TokenKind::NodeString
            | TokenKind::Space
            | TokenKind::Minus
            | TokenKind::Amp
            | TokenKind::UnicodeText
            | TokenKind::Colon
            | TokenKind::Mult
            | TokenKind::Brkt
            | TokenKind::StartLink
            | TokenKind::Style
            | TokenKind::LinkStyle
            | TokenKind::ClassDef
            | TokenKind::Class
            | TokenKind::Click
            | TokenKind::Graph
            | TokenKind::Dir
            | TokenKind::Subgraph
            | TokenKind::End
            | TokenKind::Down
            | TokenKind::Up
    )
}

fn is_id_string_token(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Num
            | TokenKind::NodeString
            | TokenKind::Down
            | TokenKind::Minus
            | TokenKind::Default
            | TokenKind::Comma
            | TokenKind::Colon
            | TokenKind::Amp
            | TokenKind::Brkt
            | TokenKind::Mult
            | TokenKind::UnicodeText
    )
}

fn is_alpha_num_token(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Num
            | TokenKind::UnicodeText
            | TokenKind::NodeString
            | TokenKind::Dir
            | TokenKind::Down
            | TokenKind::Minus
            | TokenKind::Comma
            | TokenKind::Colon
            | TokenKind::Amp
            | TokenKind::Brkt
            | TokenKind::Mult
    )
}

fn is_style_component(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Num
            | TokenKind::NodeString
            | TokenKind::Colon
            | TokenKind::Space
            | TokenKind::Brkt
            | TokenKind::Style
    )
}
