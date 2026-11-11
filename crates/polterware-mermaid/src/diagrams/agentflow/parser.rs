//! The agentflow grammar: a hand-written recursive-descent port of upstream
//! `parser/agentflow.jison`.
//!
//! The grammar is flowchart's with agentflow's vocabulary: `agentflow-beta` opens the diagram,
//! `flow ... end` and `global ... end` are the containers, `connector` declares a connector,
//! and the three edge operators are `-->`, `-.-`, and `--x`. Whitespace is significant; a
//! `&` groups vertices; a link can carry a label before, between, or after its halves; and a
//! subgraph body is parsed as its own `document` until `end`. Actions call the database methods
//! in the same order the jison rules do, which keeps ids, counters, and element mappings
//! identical.
//!
//! Upstream's parser wrapper strips horizontal whitespace between a closing `}` and the line's
//! newline so the LALR grammar's `node shapeData separator` rule reduces. This parser reads
//! separators explicitly instead, so it never needs that fold and positions stay in source
//! space.

use super::ast::LabelType;
use super::db::{Db, EndPosition, FlowText, Link, LinkPosition, StatementValue};
use super::lexer::{Lexer, Token, TokenKind, TokenKind::*};
use crate::error::{ParseError, Span};

/// Parses the diagram body. The body starts with the header line; frontmatter and `%%{init}%%`
/// directives are already removed (see [`crate::preprocess`]).
pub fn parse(body: &str) -> Result<super::ast::Ast, ParseError> {
    parse_with_offset(body, 0)
}

/// Parses the diagram body with a line offset for the frontmatter the preprocessor removed, so
/// element-mapping positions land in original-source space (`setFrontmatterLineOffset`).
pub fn parse_with_offset(body: &str, offset: u32) -> Result<super::ast::Ast, ParseError> {
    let mut parser = Parser::new(body, offset);
    parser.parse_diagram()?;
    Ok(parser.db.into_ast())
}

struct NodeGroup {
    ids: Vec<String>,
    start: Span,
    /// The last symbol's (line, exclusive 0-based column).
    end: EndPosition,
}

struct ShapeSpan {
    text: String,
    start: Span,
    end: EndPosition,
}

struct Parser<'a> {
    lexer: Lexer<'a>,
    lookahead: Option<Token>,
    last_span: Span,
    last_end: EndPosition,
    db: Db,
}

impl<'a> Parser<'a> {
    fn new(body: &'a str, offset: u32) -> Self {
        let mut db = Db::new();
        db.set_frontmatter_line_offset(offset);
        Self {
            lexer: Lexer::new(body),
            lookahead: None,
            last_span: Span::default(),
            last_end: (1, 0, 0),
            db,
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
        self.last_end = (token.end_line, token.end_column, token.span.end);
        Ok(token)
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

    fn skip_spaces(&mut self) -> Result<(), ParseError> {
        while self.peek()? == Space {
            self.bump()?;
        }
        Ok(())
    }

    fn parse_diagram(&mut self) -> Result<(), ParseError> {
        while matches!(self.peek()?, Space | Newline | Comment) {
            self.bump()?;
        }
        self.expect(Graph)?;
        match self.peek()? {
            Nodir => {
                self.bump()?;
                self.db.set_direction("TB");
            }
            Dir => {
                let token = self.bump()?;
                self.db.set_direction(&token.text);
                self.expect_first_statement_separator()?;
            }
            _ => return Err(self.unexpected("NODIR or DIR")),
        }
        self.parse_document(false)?;
        Ok(())
    }

    fn expect_first_statement_separator(&mut self) -> Result<(), ParseError> {
        match self.peek()? {
            Semi | Newline => {
                self.bump()?;
                Ok(())
            }
            Comment => {
                self.bump()?;
                Ok(())
            }
            Space => {
                self.skip_spaces()?;
                self.expect(Newline)?;
                Ok(())
            }
            Eof => Ok(()),
            _ => Err(self.unexpected("SEMI, NEWLINE, or spaces and a NEWLINE")),
        }
    }

    /// A `document`: statements and blank lines, ending at `end` (inside a block) or at EOF.
    fn parse_document(&mut self, in_block: bool) -> Result<Vec<StatementValue>, ParseError> {
        let mut lines = Vec::new();
        loop {
            match self.peek()? {
                Eof => {
                    self.bump()?;
                    break;
                }
                End if in_block => break,
                End => return Err(self.unexpected("NEWLINE, SEMI, or EOF")),
                Semi | Newline | Space => {
                    let token = self.bump()?;
                    lines.push(StatementValue::Text(token.text));
                }
                Comment => {
                    self.bump()?;
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
            AccTitle => {
                self.bump()?;
                let value = self.expect(AccTitleValue)?.text.trim().to_string();
                self.db.set_acc_title(&value);
                Ok(Some(StatementValue::Text(value)))
            }
            AccDescr => {
                self.bump()?;
                let value = self.expect(AccDescrValue)?.text.trim().to_string();
                self.db.set_acc_description(&value);
                Ok(Some(StatementValue::Text(value)))
            }
            AccDescrMultilineValue => {
                let value = self.bump()?.text.trim().to_string();
                self.db.set_acc_description(&value);
                Ok(Some(StatementValue::Text(value)))
            }
            Flow => {
                let id = self.parse_flow()?;
                Ok(Some(StatementValue::Text(id)))
            }
            Global => {
                self.parse_global()?;
                Ok(None)
            }
            Connector => {
                let id = self.parse_connector()?;
                Ok(Some(StatementValue::Text(id)))
            }
            Style => {
                self.parse_style_statement()?;
                self.expect_separator()?;
                Ok(None)
            }
            LinkStyle => {
                self.parse_link_style_statement()?;
                self.expect_separator()?;
                Ok(None)
            }
            ClassDef => {
                self.parse_class_def_statement()?;
                self.expect_separator()?;
                Ok(None)
            }
            Class => {
                self.parse_class_statement()?;
                self.expect_separator()?;
                Ok(None)
            }
            Click => {
                let id = self.parse_click_statement()?;
                self.expect_separator()?;
                Ok(Some(StatementValue::Text(id)))
            }
            DirectionTb | DirectionBt | DirectionRl | DirectionLr | DirectionTd => {
                let token = self.bump()?;
                let value = match token.kind {
                    DirectionTb => "TB",
                    DirectionBt => "BT",
                    DirectionRl => "RL",
                    DirectionLr => "LR",
                    _ => "TD",
                };
                Ok(Some(StatementValue::Direction(value.to_string())))
            }
            _ => {
                let statement = self.parse_vertex_statement()?;
                self.expect_separator()?;
                Ok(Some(StatementValue::Nodes(statement)))
            }
        }
    }

    fn parse_flow(&mut self) -> Result<String, ParseError> {
        let start = self.expect(Flow)?.span;
        let mut id = None;
        if self.peek()? == Space {
            self.bump()?;
            id = Some(self.parse_text_no_tags()?);
        }
        let mut title = None;
        if self.peek()? == Sqs {
            self.bump()?;
            title = Some(self.parse_text(Sqe)?);
            self.expect(Sqe)?;
        } else if id.is_some() {
            title = Some(FlowText {
                text: String::new(),
                label_type: LabelType::Text,
            });
        }
        let shape = if self.peek()? == ShapeData {
            Some(self.parse_shape_data()?)
        } else {
            None
        };
        self.expect_separator()?;
        let body = self.parse_document(true)?;
        let end_token = self.expect(End)?;
        let end = (end_token.end_line, end_token.end_column, end_token.span.end);

        let sub_graph_id = self
            .db
            .add_sub_graph(id.as_ref(), title.as_ref(), Some("flow"), &body);
        if let Some(shape) = shape {
            self.db.add_vertex(
                &sub_graph_id,
                None,
                None,
                None,
                None,
                None,
                None,
                Some(&shape.text),
                shape.start,
            )?;
        }
        let mapping_id = id
            .as_ref()
            .map(|text| text.text.trim().to_string())
            .unwrap_or_default();
        self.db.add_subgraph_mapping(&mapping_id, start, end);
        Ok(sub_graph_id)
    }

    fn parse_global(&mut self) -> Result<(), ParseError> {
        self.expect(Global)?;
        self.expect_separator()?;
        let body = self.parse_document(true)?;
        self.expect(End)?;
        self.db.add_global(&body);
        Ok(())
    }

    fn parse_connector(&mut self) -> Result<String, ParseError> {
        let start = self.expect(Connector)?.span;
        self.expect(Space)?;
        let id = self.parse_text_no_tags()?;
        let title = if self.peek()? == Sqs {
            self.bump()?;
            let title = self.parse_text(Sqe)?;
            self.expect(Sqe)?;
            Some(title)
        } else {
            Some(FlowText {
                text: String::new(),
                label_type: LabelType::Text,
            })
        };
        let shape = if self.peek()? == ShapeData {
            Some(self.parse_shape_data()?)
        } else {
            None
        };
        let (_, separator_end) = self.expect_separator()?;
        let connector_id = self.db.add_connector(&id, title.as_ref());
        if let Some(shape) = shape {
            self.db.add_vertex(
                &connector_id,
                None,
                None,
                None,
                None,
                None,
                None,
                Some(&shape.text),
                shape.start,
            )?;
        }
        self.db
            .add_connector_mapping(&connector_id, start, separator_end);
        Ok(connector_id)
    }

    fn parse_vertex_statement(&mut self) -> Result<Vec<String>, ParseError> {
        let first = self.parse_node()?;
        let mut stmt_ids = first.ids.clone();
        let mut nodes = first.ids.clone();
        let statement_start = first.start;
        loop {
            if !matches!(self.peek()?, Link | LinkId | StartLink) {
                break;
            }
            let link = self.parse_link()?;
            let next = self.parse_node()?;
            self.db.add_link(&stmt_ids, &next.ids, &link, next.start)?;
            let position = self.db.position(statement_start, Some(next.end));
            self.db.add_edge_mapping(&next.ids, position);
            stmt_ids = next.ids.clone();
            let mut combined = next.ids;
            combined.extend(nodes);
            nodes = combined;
        }
        Ok(nodes)
    }

    /// A node group: one styled vertex, extended by `&`-separated vertices, each able to carry
    /// trailing `@{ }` metadata for the vertex before it.
    fn parse_node(&mut self) -> Result<NodeGroup, ParseError> {
        let (first_id, start, mut end) = self.parse_styled_vertex()?;
        let mut ids = vec![first_id];
        loop {
            let shape = if self.peek()? == ShapeData {
                Some(self.parse_shape_data()?)
            } else {
                None
            };
            self.skip_spaces()?;
            if self.peek()? != Amp {
                if let Some(shape) = shape {
                    let id = ids.last().cloned().expect("a node group has a vertex");
                    self.db.add_vertex(
                        &id,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        Some(&shape.text),
                        shape.start,
                    )?;
                    let position = self.db.position(shape.start, Some(shape.end));
                    self.db.extend_vertex_mapping(&id, position);
                    end = shape.end;
                }
                break;
            }
            self.bump()?;
            self.skip_spaces()?;
            let (vertex_id, _, vertex_end) = self.parse_styled_vertex()?;
            if let Some(shape) = shape {
                let id = ids.last().cloned().expect("a node group has a vertex");
                self.db.add_vertex(
                    &id,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(&shape.text),
                    shape.start,
                )?;
                let position = self.db.position(shape.start, Some(shape.end));
                self.db.extend_vertex_mapping(&id, position);
            }
            end = vertex_end;
            ids.push(vertex_id);
        }
        Ok(NodeGroup { ids, start, end })
    }

    fn parse_styled_vertex(&mut self) -> Result<(String, Span, EndPosition), ParseError> {
        let (id, start, mut end) = self.parse_vertex()?;
        if self.peek()? == StyleSeparator {
            self.bump()?;
            let class = self.parse_id_string()?;
            self.db.set_class(&id, &class);
            end = self.last_end;
        }
        Ok((id, start, end))
    }

    fn parse_vertex(&mut self) -> Result<(String, Span, EndPosition), ParseError> {
        let start = self.peek_span()?;
        let id = self.parse_id_string()?;
        let mut end = self.last_end;
        let mut type_name = None;
        let mut text = None;
        let mut props = None;
        match self.peek()? {
            Sqs => {
                self.bump()?;
                text = Some(self.parse_text(Sqe)?);
                self.expect(Sqe)?;
                end = self.last_end;
                type_name = Some("square");
            }
            DoubleCircleStart => {
                self.bump()?;
                text = Some(self.parse_text(DoubleCircleEnd)?);
                self.expect(DoubleCircleEnd)?;
                end = self.last_end;
                type_name = Some("doublecircle");
            }
            Ps => {
                self.bump()?;
                if self.peek()? == Ps {
                    self.bump()?;
                    text = Some(self.parse_text(Pe)?);
                    self.expect(Pe)?;
                    self.expect(Pe)?;
                    type_name = Some("circle");
                } else {
                    text = Some(self.parse_text(Pe)?);
                    self.expect(Pe)?;
                    type_name = Some("round");
                }
                end = self.last_end;
            }
            EllipseStart => {
                self.bump()?;
                text = Some(self.parse_text(EllipseEnd)?);
                self.expect(EllipseEnd)?;
                end = self.last_end;
                type_name = Some("ellipse");
            }
            StadiumStart => {
                self.bump()?;
                text = Some(self.parse_text(StadiumEnd)?);
                self.expect(StadiumEnd)?;
                end = self.last_end;
                type_name = Some("stadium");
            }
            SubroutineStart => {
                self.bump()?;
                text = Some(self.parse_text(SubroutineEnd)?);
                self.expect(SubroutineEnd)?;
                end = self.last_end;
                type_name = Some("subroutine");
            }
            VertexWithPropsStart => {
                self.bump()?;
                let field = self.expect(NodeString)?.text;
                self.expect(Colon)?;
                let value = self.expect(NodeString)?.text;
                self.expect(Pipe)?;
                text = Some(self.parse_text(Sqe)?);
                self.expect(Sqe)?;
                end = self.last_end;
                let mut map = serde_json::Map::new();
                map.insert(field, serde_json::Value::String(value));
                props = Some(map);
                type_name = Some("rect");
            }
            CylinderStart => {
                self.bump()?;
                text = Some(self.parse_text(CylinderEnd)?);
                self.expect(CylinderEnd)?;
                end = self.last_end;
                type_name = Some("cylinder");
            }
            DiamondStart => {
                self.bump()?;
                if self.peek()? == DiamondStart {
                    self.bump()?;
                    text = Some(self.parse_text(DiamondStop)?);
                    self.expect(DiamondStop)?;
                    self.expect(DiamondStop)?;
                    type_name = Some("hexagon");
                } else {
                    text = Some(self.parse_text(DiamondStop)?);
                    self.expect(DiamondStop)?;
                    type_name = Some("diamond");
                }
                end = self.last_end;
            }
            TagEnd => {
                self.bump()?;
                text = Some(self.parse_text(Sqe)?);
                self.expect(Sqe)?;
                end = self.last_end;
                type_name = Some("odd");
            }
            TrapStart => {
                self.bump()?;
                text = Some(self.parse_text_any(&[TrapEnd, InvTrapEnd])?);
                let closer = self.bump()?.kind;
                end = self.last_end;
                type_name = Some(if closer == TrapEnd {
                    "trapezoid"
                } else {
                    "lean_right"
                });
            }
            InvTrapStart => {
                self.bump()?;
                text = Some(self.parse_text_any(&[TrapEnd, InvTrapEnd])?);
                let closer = self.bump()?.kind;
                end = self.last_end;
                type_name = Some(if closer == TrapEnd {
                    "lean_left"
                } else {
                    "inv_trapezoid"
                });
            }
            _ => {}
        }
        self.db
            .add_vertex(&id, text, type_name, None, None, None, props, None, start)?;
        let position = self.db.position(start, Some(end));
        self.db
            .add_vertex_mapping(&id, type_name.is_some(), position);
        Ok((id, start, end))
    }

    fn parse_link(&mut self) -> Result<Link, ParseError> {
        let mut id = None;
        if self.peek()? == LinkId {
            id = Some(self.bump()?.text);
        }
        if self.peek()? == StartLink {
            let start = self.bump()?.text;
            let text = self.parse_edge_text()?;
            let end = self.expect(Link)?.text;
            let info = self.db.destruct_link(&end, Some(&start));
            return Ok(Link {
                type_name: info.type_name,
                stroke: info.stroke,
                length: info.length,
                text: Some(text),
                id: None,
            });
        }
        let end = self.expect(Link)?.text;
        let info = self.db.destruct_link(&end, None);
        let mut link = Link {
            type_name: info.type_name,
            stroke: info.stroke,
            length: info.length,
            text: None,
            id,
        };
        if self.peek()? == Pipe {
            self.bump()?;
            link.text = Some(self.parse_text(Pipe)?);
            self.expect(Pipe)?;
            if self.peek()? == Space {
                self.bump()?;
            }
        } else if self.peek()? == Space {
            self.bump()?;
        }
        Ok(link)
    }

    fn parse_edge_text(&mut self) -> Result<FlowText, ParseError> {
        match self.peek()? {
            Str => {
                let token = self.bump()?;
                Ok(FlowText {
                    text: token.text,
                    label_type: LabelType::String,
                })
            }
            MdStr => {
                let token = self.bump()?;
                Ok(FlowText {
                    text: token.text,
                    label_type: LabelType::Markdown,
                })
            }
            _ => {
                let mut text = String::new();
                while matches!(self.peek()?, EdgeText | UnicodeText) {
                    text.push_str(&self.bump()?.text);
                }
                if text.is_empty() {
                    return Err(self.unexpected("edge text"));
                }
                Ok(FlowText {
                    text,
                    label_type: LabelType::Text,
                })
            }
        }
    }

    fn parse_text(&mut self, terminator: TokenKind) -> Result<FlowText, ParseError> {
        self.parse_text_any(&[terminator])
    }

    /// The `text` production: one string token, or a run of text tokens.
    fn parse_text_any(&mut self, terminators: &[TokenKind]) -> Result<FlowText, ParseError> {
        match self.peek()? {
            Str => {
                let token = self.bump()?;
                return Ok(FlowText {
                    text: token.text,
                    label_type: LabelType::String,
                });
            }
            MdStr => {
                let token = self.bump()?;
                return Ok(FlowText {
                    text: token.text,
                    label_type: LabelType::Markdown,
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
            label_type: LabelType::Text,
        })
    }

    /// `textNoTags`: `STR` is a plain label, not a string, exactly as upstream's action says.
    fn parse_text_no_tags(&mut self) -> Result<FlowText, ParseError> {
        match self.peek()? {
            Str => {
                let token = self.bump()?;
                return Ok(FlowText {
                    text: token.text,
                    label_type: LabelType::Text,
                });
            }
            MdStr => {
                let token = self.bump()?;
                return Ok(FlowText {
                    text: token.text,
                    label_type: LabelType::Markdown,
                });
            }
            _ => {}
        }
        let mut text = String::new();
        while is_text_no_tags_token(self.peek()?) {
            text.push_str(&self.bump()?.text);
        }
        if text.is_empty() {
            return Err(self.unexpected("container id"));
        }
        Ok(FlowText {
            text,
            label_type: LabelType::Text,
        })
    }

    fn parse_shape_data(&mut self) -> Result<ShapeSpan, ParseError> {
        let start = self.peek_span()?;
        let mut end = (start.line, start.column.saturating_sub(1), start.end);
        let mut text = String::new();
        while self.peek()? == ShapeData {
            let token = self.bump()?;
            end = (token.end_line, token.end_column, token.span.end);
            text.push_str(&token.text);
        }
        Ok(ShapeSpan { text, start, end })
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
            let number = self.expect(Num)?;
            let index = number
                .text
                .parse::<usize>()
                .map_err(|_| ParseError::at("invalid link index", number.span))?;
            positions.push(LinkPosition::Index(index));
            if self.peek()? != Comma {
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
            if self.peek()? != Comma {
                break;
            }
            self.bump()?;
        }
        Ok(styles)
    }

    fn parse_style_statement(&mut self) -> Result<(), ParseError> {
        self.expect(Style)?;
        self.expect(Space)?;
        let id = self.parse_id_string()?;
        self.expect(Space)?;
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
        self.expect(ClassDef)?;
        self.expect(Space)?;
        let ids = self.parse_id_string()?;
        self.expect(Space)?;
        let styles = self.parse_styles()?;
        self.db.add_class(&ids, &styles);
        Ok(())
    }

    fn parse_class_statement(&mut self) -> Result<(), ParseError> {
        self.expect(Class)?;
        self.expect(Space)?;
        let ids = self.parse_id_string()?;
        self.expect(Space)?;
        let class = self.parse_id_string()?;
        self.db.set_class(&ids, &class);
        Ok(())
    }

    fn parse_link_style_statement(&mut self) -> Result<(), ParseError> {
        let span = self.expect(LinkStyle)?.span;
        self.expect(Space)?;
        let positions = if self.peek()? == Default {
            self.bump()?;
            vec![LinkPosition::Default]
        } else {
            self.parse_num_list()?
        };
        self.expect(Space)?;
        if self.peek()? == Interpolate {
            self.bump()?;
            self.expect(Space)?;
            let interpolate = self.parse_alpha_num()?;
            if self.peek()? == Space {
                self.bump()?;
                let styles = self.parse_styles()?;
                self.db
                    .update_link_interpolate(&positions, &interpolate, span)?;
                self.db.update_link(&positions, &styles, span)?;
            } else {
                self.db
                    .update_link_interpolate(&positions, &interpolate, span)?;
            }
        } else {
            let styles = self.parse_styles()?;
            self.db.update_link(&positions, &styles, span)?;
        }
        Ok(())
    }

    fn parse_click_statement(&mut self) -> Result<String, ParseError> {
        let click = self.expect(Click)?;
        let id = click.text;
        match self.peek()? {
            CallbackName => {
                self.bump()?;
                self.db.set_click_event(&id);
                if self.peek()? == CallbackArgs {
                    self.bump()?;
                }
                if self.peek()? == Space {
                    self.bump()?;
                    let _tooltip = self.expect(Str)?;
                }
            }
            Href => {
                self.bump()?;
                let link = self.expect(Str)?;
                self.parse_click_link(&id, &link.text)?;
            }
            Str => {
                let link = self.bump()?;
                self.parse_click_link(&id, &link.text)?;
            }
            _ => {
                let _name = self.parse_alpha_num()?;
                self.db.set_click_event(&id);
                if self.peek()? == Space {
                    self.bump()?;
                    let _tooltip = self.expect(Str)?;
                }
            }
        }
        Ok(id)
    }

    fn parse_click_link(&mut self, id: &str, link: &str) -> Result<(), ParseError> {
        let mut target = None;
        if self.peek()? == Space {
            self.bump()?;
            match self.peek()? {
                Str => {
                    self.bump()?;
                    if self.peek()? == Space {
                        self.bump()?;
                        target = Some(self.expect(LinkTarget)?.text);
                    }
                }
                LinkTarget => {
                    target = Some(self.bump()?.text);
                }
                _ => return Err(self.unexpected("STR or LINK_TARGET")),
            }
        }
        self.db.set_link(id, link, target.as_deref());
        Ok(())
    }

    /// A `separator`: NEWLINE, SEMI, COMMENT, or EOF. Returns its span and end position.
    fn expect_separator(&mut self) -> Result<(Span, EndPosition), ParseError> {
        match self.peek()? {
            Newline | Semi | Comment => {
                let token = self.bump()?;
                Ok((
                    token.span,
                    (token.end_line, token.end_column, token.span.end),
                ))
            }
            Eof => Ok((self.peek_span()?, self.last_end)),
            _ => Err(self.unexpected("NEWLINE, SEMI, or EOF")),
        }
    }
}

fn is_text_token(kind: TokenKind) -> bool {
    matches!(kind, Text | TagStart | TagEnd | UnicodeText)
}

fn is_text_no_tags_token(kind: TokenKind) -> bool {
    matches!(
        kind,
        Num | NodeString
            | Space
            | Minus
            | Amp
            | UnicodeText
            | Colon
            | Mult
            | Brkt
            | Style
            | LinkStyle
            | ClassDef
            | Class
            | Click
            | Graph
            | Dir
            | Flow
            | Connector
            | Global
            | End
            | Down
            | Up
            | StartLink
    )
}

fn is_id_string_token(kind: TokenKind) -> bool {
    matches!(
        kind,
        Num | NodeString | Down | Minus | Default | Comma | Colon | Amp | Brkt | Mult | UnicodeText
    )
}

fn is_alpha_num_token(kind: TokenKind) -> bool {
    matches!(
        kind,
        Num | UnicodeText | NodeString | Dir | Down | Minus | Comma | Colon | Amp | Brkt | Mult
    )
}

fn is_style_component(kind: TokenKind) -> bool {
    matches!(kind, Num | NodeString | Colon | Space | Brkt | Style)
}
