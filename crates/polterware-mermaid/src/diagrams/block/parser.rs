//! The block grammar: a hand-written recursive-descent port of upstream `parser/block.jison`.
//!
//! ```text
//! start:            BLOCK_DIAGRAM_KEY document EOF          (yy.setHierarchy)
//! document:         statement+                              (`statement document`, right recursive)
//! statement:        nodeStatement | columnsStatement | SPACE_BLOCK
//!                 | blockStatement | classDefStatement | cssClassStatement | styleStatement
//! nodeStatement:    nodeStatement link node | node SIZE | node
//! node:             NODE_ID | NODE_ID nodeShapeNLabel
//! nodeShapeNLabel:  NODE_DSTART STR NODE_DEND
//!                 | BLOCK_ARROW_START STR dirList BLOCK_ARROW_END
//! dirList:          DIR+
//! link:             LINK | START_LINK LINK_LABEL STR LINK
//! columnsStatement: COLUMNS
//! blockStatement:   id-block nodeStatement document end
//!                 | BLOCK_DIAGRAM_KEY document end
//! classDefStatement: classDef (CLASSDEF_ID | DEFAULT) CLASSDEF_STYLEOPTS
//! cssClassStatement: class CLASSENTITY_IDS STYLECLASS
//! styleStatement:   style STYLE_ENTITY_IDS STYLE_DEFINITION_DATA
//! ```
//!
//! Whitespace is not a token (see [`super::lexer`]), so the grammar's newline rules
//! (`spaceLines`, `separator`, `stop`) are unreachable, and so are the `NL`, `SPACE`,
//! `MD_STR`, `NODE_DESCR`, `LINKSTYLE`, `INTERPOLATE`, `DEFAULT`, and `acc_*` terminals:
//! no rule of the grammar accepts them. `accessibility` statements, `linkStyle`, and
//! `interpolate` are parse errors in a block diagram upstream, exactly as here.
//!
//! The rules build the same values the jison actions build, including where that value is
//! an array: `nodeStatement link node` produces `[start node, edge, end node]`, and the
//! left recursion means that a chain (`A --> B --> C`) hands the array itself to the next
//! link, where reading `.id`/`.label`/`.type` off an array yields `undefined`. Those
//! statements are kept as [`Parsed::Chain`] and flattened when the database is populated.
//!
//! `blockStatement` spreads its `nodeStatement` into the composite object, so a composite
//! written as `block:id [...]` keeps the id, label, directions, and width of that
//! statement; an anonymous `block` gets a generated id and an empty label.
//!
//! `generateId` is called where upstream's grammar actions call it: a `space` when its
//! statement is read, an anonymous `block` when its `end` is read (so a container is
//! numbered after everything inside it). Upstream's id carries a random middle,
//! `id-<random>-<n>`; here it is `id-<n>`.

use super::ast::{Ast, BlockType};
use super::db;
use super::lexer::{Lexer, Token, TokenKind};
use crate::error::{ParseError, Span};

/// The `node` rule's value: the id, the optional label and shape, and a block arrow's
/// directions. `typeStr` is the delimiter text (`"[]"`, `"()"`, `"<[]>"`, ...).
#[derive(Clone, Debug, Default)]
pub(crate) struct Node {
    pub(crate) id: Option<String>,
    pub(crate) label: Option<String>,
    pub(crate) type_str: Option<String>,
    pub(crate) directions: Vec<String>,
}

/// A `nodeStatement` that is one node, as the grammar's actions leave it.
#[derive(Clone, Debug, Default)]
pub(crate) struct NodeValue {
    pub(crate) id: Option<String>,
    pub(crate) label: Option<String>,
    pub(crate) type_name: Option<BlockType>,
    pub(crate) directions: Vec<String>,
    pub(crate) width_in_columns: Option<i64>,
}

/// The `link` rule's value.
#[derive(Clone, Debug)]
pub(crate) struct Link {
    pub(crate) edge_type_str: String,
    pub(crate) label: String,
}

/// An edge object as `nodeStatement link node` builds it, before the database renumbers it.
#[derive(Clone, Debug)]
pub(crate) struct EdgeValue {
    pub(crate) id: String,
    pub(crate) start: Option<String>,
    pub(crate) end: Option<String>,
    pub(crate) label: String,
    pub(crate) thickness: String,
    pub(crate) pattern: String,
    pub(crate) arrow_type_start: String,
    pub(crate) arrow_type_end: String,
    pub(crate) directions: Vec<String>,
}

/// One value the grammar produces for a `statement` rule.
#[derive(Clone, Debug)]
pub(crate) enum Statement {
    Node(NodeValue),
    Edge(EdgeValue),
    /// `{id, type: 'space', label: '', width, children: []}`, with the id `generateId`
    /// hands out when the statement is read.
    Space {
        id: String,
        width: i64,
    },
    /// `{...nodeStatement, type: 'composite', children}`.
    Composite {
        node: NodeValue,
        children: Vec<Parsed>,
    },
    ColumnSetting(i64),
    ClassDef {
        id: String,
        css: String,
    },
    ApplyClass {
        id: String,
        style_class: String,
    },
    ApplyStyles {
        id: String,
        styles_str: String,
        /// The statement's position, for the error upstream throws when the id is unknown.
        span: Span,
    },
}

/// One entry of a `document`: a statement, or the three values a chained edge produced.
/// `document: statement` turns an array into the array itself (`$$ = $1`), and the
/// database flattens it, so a chain contributes its elements in order.
#[derive(Clone, Debug)]
pub(crate) enum Parsed {
    Statement(Statement),
    Chain(Vec<Statement>),
}

impl Parsed {
    /// The fields a `nodeStatement link node` action reads off the left-hand value. On the
    /// array a chained statement produced, every field is `undefined` upstream.
    fn node_fields(&self) -> NodeValue {
        match self {
            Parsed::Statement(Statement::Node(node)) => NodeValue {
                id: node.id.clone(),
                label: node.label.clone(),
                type_name: node.type_name,
                directions: node.directions.clone(),
                width_in_columns: None,
            },
            _ => NodeValue::default(),
        }
    }
}

impl Statement {
    /// The label the database sanitizes, if the statement has one.
    pub(crate) fn label(&self) -> Option<&str> {
        match self {
            Statement::Node(node) => node.label.as_deref(),
            Statement::Edge(edge) => Some(&edge.label),
            Statement::Composite { node, .. } => node.label.as_deref(),
            _ => None,
        }
    }
}

/// Parses the diagram body and populates the database.
pub(crate) fn parse(body: &str) -> Result<Ast, ParseError> {
    let mut parser = Parser::new(body);
    // `start: BLOCK_DIAGRAM_KEY document EOF`.
    parser.expect(TokenKind::BlockDiagramKey)?;
    let statements = parser.require_document()?;
    parser.expect(TokenKind::Eof)?;
    db::build(&statements)
}

struct Parser {
    lexer: Lexer,
    lookahead: Option<Token>,
    /// `generateId`'s counter. Upstream keeps it in the database module and never resets it;
    /// a parse here numbers its own generated ids from 1.
    generated: usize,
}

impl Parser {
    fn new(body: &str) -> Self {
        Self {
            lexer: Lexer::new(body),
            lookahead: None,
            generated: 0,
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
            Err(error) => error.clone(),
        }
    }

    fn expect_statement(&mut self) -> ParseError {
        self.unexpected("a statement")
    }

    /// `document`: statements until `end` or the end of the input, at least one because the
    /// grammar has no empty `document`.
    fn require_document(&mut self) -> Result<Vec<Parsed>, ParseError> {
        let mut statements = Vec::new();
        loop {
            match self.peek()? {
                TokenKind::Eof | TokenKind::End => break,
                _ => statements.push(self.parse_statement()?),
            }
        }
        if statements.is_empty() {
            return Err(self.expect_statement());
        }
        Ok(statements)
    }

    fn parse_statement(&mut self) -> Result<Parsed, ParseError> {
        match self.peek()? {
            TokenKind::NodeId => self.parse_node_statement(),
            TokenKind::Columns => {
                let token = self.bump()?;
                Ok(Parsed::Statement(Statement::ColumnSetting(parse_int(
                    &token.text,
                ))))
            }
            TokenKind::SpaceBlock => {
                let token = self.bump()?;
                Ok(Parsed::Statement(Statement::Space {
                    id: self.generate_id(),
                    width: parse_int(&token.text),
                }))
            }
            TokenKind::IdBlock => self.parse_id_block(),
            TokenKind::BlockDiagramKey => self.parse_anonymous_block(),
            TokenKind::ClassDef => self.parse_class_def(),
            TokenKind::Class => self.parse_class_statement(),
            TokenKind::Style => self.parse_style_statement(),
            _ => Err(self.unexpected("a statement")),
        }
    }

    fn parse_node_statement(&mut self) -> Result<Parsed, ParseError> {
        let node = self.parse_node()?;
        let width_in_columns = if self.peek()? == TokenKind::Size {
            Some(parse_int(&self.bump()?.text))
        } else {
            Some(1)
        };
        let mut value = Parsed::Statement(Statement::Node(NodeValue {
            id: node.id.clone(),
            label: node.label.clone(),
            type_name: Some(db::type_str2_type(node.type_str.as_deref())),
            directions: node.directions.clone(),
            width_in_columns,
        }));
        while matches!(self.peek()?, TokenKind::Link | TokenKind::StartLink) {
            let link = self.parse_link()?;
            let target = self.parse_node()?;
            value = chained(&value, &link, &target);
        }
        Ok(value)
    }

    fn parse_node(&mut self) -> Result<Node, ParseError> {
        let id = self.expect(TokenKind::NodeId)?.text;
        let mut node = Node {
            id: Some(id),
            ..Node::default()
        };
        match self.peek()? {
            TokenKind::NodeDStart => {
                let start = self.bump()?.text;
                let label = self.expect(TokenKind::Str)?.text;
                let end = self.expect(TokenKind::NodeDEnd)?.text;
                node.label = Some(label);
                node.type_str = Some(format!("{start}{end}"));
            }
            TokenKind::BlockArrowStart => {
                let start = self.bump()?.text;
                let label = self.expect(TokenKind::Str)?.text;
                let mut directions = vec![self.expect(TokenKind::Dir)?.text];
                while self.peek()? == TokenKind::Dir {
                    directions.push(self.bump()?.text);
                }
                let end = self.expect(TokenKind::BlockArrowEnd)?.text;
                node.label = Some(label);
                node.directions = directions;
                node.type_str = Some(format!("{start}{end}"));
            }
            _ => {}
        }
        Ok(node)
    }

    fn parse_link(&mut self) -> Result<Link, ParseError> {
        match self.peek()? {
            TokenKind::Link => Ok(Link {
                edge_type_str: self.bump()?.text,
                label: String::new(),
            }),
            TokenKind::StartLink => {
                self.bump()?;
                self.expect(TokenKind::LinkLabel)?;
                let label = self.expect(TokenKind::Str)?.text;
                let edge_type_str = self.expect(TokenKind::Link)?.text;
                Ok(Link {
                    edge_type_str,
                    label,
                })
            }
            _ => Err(self.unexpected("LINK or START_LINK")),
        }
    }

    fn parse_id_block(&mut self) -> Result<Parsed, ParseError> {
        self.expect(TokenKind::IdBlock)?;
        let statement = self.parse_node_statement()?;
        let children = self.require_document()?;
        self.expect(TokenKind::End)?;
        Ok(Parsed::Statement(Statement::Composite {
            node: named_composite(&statement),
            children,
        }))
    }

    fn parse_anonymous_block(&mut self) -> Result<Parsed, ParseError> {
        self.expect(TokenKind::BlockDiagramKey)?;
        let children = self.require_document()?;
        self.expect(TokenKind::End)?;
        Ok(Parsed::Statement(Statement::Composite {
            node: NodeValue {
                // `{ id: yy.generateId(), type: 'composite', label: '', children: $2 }`,
                // run when the `end` is reduced, after the children were read.
                id: Some(self.generate_id()),
                label: Some(String::new()),
                type_name: Some(BlockType::Composite),
                directions: Vec::new(),
                width_in_columns: None,
            },
            children,
        }))
    }

    fn parse_class_def(&mut self) -> Result<Parsed, ParseError> {
        self.expect(TokenKind::ClassDef)?;
        let id = match self.peek()? {
            // The CLASSDEF state always reads `DEFAULT` as DEFAULT_CLASSDEF_ID, so the
            // grammar's `classDef DEFAULT` branch cannot be reached; it is kept here.
            TokenKind::ClassDefId | TokenKind::Default => self.bump()?.text,
            _ => return Err(self.unexpected("CLASSDEF_ID")),
        };
        let css = self.expect(TokenKind::ClassDefStyleOpts)?.text;
        Ok(Parsed::Statement(Statement::ClassDef {
            id: id.trim().to_string(),
            css: css.trim().to_string(),
        }))
    }

    fn parse_class_statement(&mut self) -> Result<Parsed, ParseError> {
        self.expect(TokenKind::Class)?;
        let id = self.expect(TokenKind::ClassEntityIds)?.text;
        let style_class = self.expect(TokenKind::StyleClass)?.text;
        Ok(Parsed::Statement(Statement::ApplyClass {
            id: id.trim().to_string(),
            style_class: style_class.trim().to_string(),
        }))
    }

    /// `yy.generateId` (`id-<random>-<n>` upstream; the random middle is not reproducible).
    fn generate_id(&mut self) -> String {
        self.generated += 1;
        format!("id-{}", self.generated)
    }

    fn parse_style_statement(&mut self) -> Result<Parsed, ParseError> {
        let start = self.expect(TokenKind::Style)?.span;
        let id = self.expect(TokenKind::StyleEntityIds)?.text;
        let styles = self.expect(TokenKind::StyleDefinitionData)?;
        Ok(Parsed::Statement(Statement::ApplyStyles {
            id: id.trim().to_string(),
            styles_str: styles.text.trim().to_string(),
            span: Span {
                end: styles.span.end,
                ..start
            },
        }))
    }
}

/// `nodeStatement link node`: `[start node, edge, end node]`. Reading the fields off an
/// array (a chained statement) leaves them empty, as upstream's `undefined`.
fn chained(left: &Parsed, link: &Link, target: &Node) -> Parsed {
    let start = left.node_fields();
    let end = NodeValue {
        id: target.id.clone(),
        label: target.label.clone(),
        type_name: Some(db::type_str2_type(target.type_str.as_deref())),
        directions: target.directions.clone(),
        width_in_columns: None,
    };
    let edge = EdgeValue {
        id: format!(
            "{}-{}",
            js_id(start.id.as_deref()),
            js_id(target.id.as_deref())
        ),
        start: start.id.clone(),
        end: target.id.clone(),
        label: link.label.clone(),
        thickness: db::edge_str_to_thickness(&link.edge_type_str).to_string(),
        pattern: db::edge_str_to_pattern(&link.edge_type_str).to_string(),
        arrow_type_start: db::edge_str_to_edge_start_data(&link.edge_type_str).to_string(),
        arrow_type_end: db::edge_str_to_edge_data(&link.edge_type_str).to_string(),
        directions: target.directions.clone(),
    };
    Parsed::Chain(vec![
        Statement::Node(start),
        Statement::Edge(edge),
        Statement::Node(end),
    ])
}

/// `{...nodeStatement, type: 'composite'}` for the `id-block` form: the spread keeps every
/// field of the statement, `widthInColumns` included.
fn named_composite(statement: &Parsed) -> NodeValue {
    match statement {
        Parsed::Statement(Statement::Node(node)) => NodeValue {
            type_name: Some(BlockType::Composite),
            ..node.clone()
        },
        _ => NodeValue {
            type_name: Some(BlockType::Composite),
            ..NodeValue::default()
        },
    }
}

/// `` `${value}` `` for the ids the edge id concatenates: an absent id prints `undefined`.
fn js_id(id: Option<&str>) -> &str {
    id.unwrap_or("undefined")
}

/// `parseInt` over the digits the lexer matched. A value that does not fit saturates, so a
/// hostile width cannot wrap around.
fn parse_int(text: &str) -> i64 {
    text.parse::<i64>().unwrap_or_else(|_| {
        if text.starts_with('-') {
            i64::MIN
        } else {
            i64::MAX
        }
    })
}
