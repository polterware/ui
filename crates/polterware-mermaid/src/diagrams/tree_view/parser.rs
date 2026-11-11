//! The treeView grammar: a hand-written recursive-descent port of `treeView.langium`.
//!
//! ```text
//! TreeView:
//!   "treeView-beta"
//!   TitleAndAccessibilities?
//!   nodes+=TreeNode*
//! TitleAndAccessibilities:
//!   ((accDescr=ACC_DESCR | accTitle=ACC_TITLE | title=TITLE))+
//! TreeNode:
//!   indent=INDENTATION?
//!   (name=QUOTED_NAME | name=BARE_NAME)
//!   (classAnnotation=CLASS_ANNOTATION
//!    | iconAnnotation=ICON_ANNOTATION
//!    | descAnnotation=DESC_ANNOTATION)*
//! ```
//!
//! `NL` and `ML_COMMENT` are hidden terminals, so the whole body is one token stream: the
//! parser never looks at line breaks, and a title line is simply an `ACC_DESCR`, `ACC_TITLE`,
//! or `TITLE` token in the position before the first node.
//!
//! Every assignment in the rules is single-valued, so a later assignment overwrites an
//! earlier one, as Langium's AST builder does. The terminal values go through
//! `treeView/valueConverter.ts`: the `INDENTATION` length, the quote stripping of
//! `QUOTED_NAME`, the trailing-whitespace trim of `BARE_NAME`, and the `:::`/`icon()`/`##`
//! trimming of the annotations.
//!
//! Upstream's `parser.ts` first runs `preprocessBoxDrawing` and remaps the error lines of what
//! the grammar then reports; [`parse`] does the same and then populates the database.

use super::ast::Ast;
use super::box_drawing;
use super::db;
use super::lexer::{Lexer, Token, TokenKind};
use crate::common::text::trim_horizontal;
use crate::error::ParseError;

/// Parses the diagram body, converts the box-drawing format, and populates the database.
pub(crate) fn parse(body: &str) -> Result<Ast, ParseError> {
    let preprocessed = box_drawing::preprocess(body)?;
    let parsed = Parser::new(&preprocessed.text)
        .parse_tree_view()
        .map_err(|error| preprocessed.remap(error, body))?;
    Ok(db::build(parsed))
}

/// The `TreeView` AST node after parsing, before the database is populated.
#[derive(Debug, Default)]
pub(crate) struct ParsedTreeView {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_description: Option<String>,
    pub(crate) nodes: Vec<ParsedNode>,
}

/// One `TreeNode`.
#[derive(Debug)]
pub(crate) struct ParsedNode {
    /// `indent`: the length of the indentation token, `0` when it is absent.
    pub(crate) level: u32,
    pub(crate) name: String,
    pub(crate) class_annotation: Option<String>,
    /// `None` without an annotation and `Some("")` for an empty `icon()`, which the database
    /// turns into `none`.
    pub(crate) icon_annotation: Option<String>,
    /// The `##` text, possibly empty.
    pub(crate) description: Option<String>,
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

    fn parse_tree_view(mut self) -> Result<ParsedTreeView, ParseError> {
        self.expect(TokenKind::Keyword)?;

        let mut parsed = ParsedTreeView::default();
        while matches!(
            self.peek()?,
            TokenKind::AccDescr | TokenKind::AccTitle | TokenKind::Title
        ) {
            let token = self.bump()?;
            match token.kind {
                TokenKind::AccDescr => {
                    parsed.acc_description = Some(acc_descr_value(&token.text));
                }
                TokenKind::AccTitle => parsed.acc_title = Some(acc_title_value(&token.text)),
                _ => parsed.title = Some(title_value(&token.text)),
            }
        }

        while matches!(
            self.peek()?,
            TokenKind::Indentation | TokenKind::QuotedName | TokenKind::BareName
        ) {
            parsed.nodes.push(self.parse_node()?);
        }

        if self.peek()? != TokenKind::Eof {
            return Err(self.unexpected("end of input"));
        }
        Ok(parsed)
    }

    fn parse_node(&mut self) -> Result<ParsedNode, ParseError> {
        let level = if self.peek()? == TokenKind::Indentation {
            indentation_value(&self.bump()?.text)
        } else {
            0
        };
        let name = match self.peek()? {
            TokenKind::QuotedName => strip_quotes(&self.bump()?.text),
            TokenKind::BareName => trim_bare_name(&self.bump()?.text),
            _ => return Err(self.unexpected("a quoted or bare name")),
        };

        let mut node = ParsedNode {
            level,
            name,
            class_annotation: None,
            icon_annotation: None,
            description: None,
        };
        while matches!(
            self.peek()?,
            TokenKind::ClassAnnotation | TokenKind::IconAnnotation | TokenKind::DescAnnotation
        ) {
            let token = self.bump()?;
            match token.kind {
                TokenKind::ClassAnnotation => {
                    node.class_annotation = Some(class_value(&token.text));
                }
                TokenKind::IconAnnotation => {
                    node.icon_annotation = Some(icon_value(&token.text));
                }
                _ => node.description = Some(desc_value(&token.text)),
            }
        }
        Ok(node)
    }
}

/// `INDENTATION` → `input.length || 0`.
pub(super) fn indentation_value(raw: &str) -> u32 {
    raw.len() as u32
}

/// `QUOTED_NAME` → `input.substring(1, input.length - 1)`.
pub(super) fn strip_quotes(raw: &str) -> String {
    raw[1..raw.len() - 1].to_string()
}

/// `BARE_NAME` → `input.replace(/[\t ]+$/, '')`.
pub(super) fn trim_bare_name(raw: &str) -> String {
    raw.trim_end_matches([' ', '\t']).to_string()
}

/// `CLASS_ANNOTATION` → `input.trim().substring(3).trim()`.
pub(super) fn class_value(raw: &str) -> String {
    trim(&trim(raw)[":::".len()..]).to_string()
}

/// `ICON_ANNOTATION` → `input.trim().substring(5, input.trim().length - 1)`.
pub(super) fn icon_value(raw: &str) -> String {
    let trimmed = trim(raw);
    trimmed["icon(".len()..trimmed.len() - 1].to_string()
}

/// `DESC_ANNOTATION` → `input.trim().substring(2).trim()`.
pub(super) fn desc_value(raw: &str) -> String {
    trim(&trim(raw)["##".len()..]).to_string()
}

/// `common/valueConverter.ts`'s `titleRegex`: `/title([\t ][^\n\r]*|)/`, capture trimmed and
/// with runs of two or more spaces or tabs collapsed to one.
fn title_value(raw: &str) -> String {
    let Some(start) = raw.find("title") else {
        return String::new();
    };
    let rest = &raw[start + "title".len()..];
    if rest.starts_with(' ') || rest.starts_with('\t') {
        collapse(rest)
    } else {
        String::new()
    }
}

/// `accessibilityTitleRegex`: `/accTitle[\t ]*:([^\n\r]*)/`.
fn acc_title_value(raw: &str) -> String {
    let Some(start) = raw.find("accTitle") else {
        return String::new();
    };
    let rest = trim_horizontal(&raw[start + "accTitle".len()..]);
    let Some(value) = rest.strip_prefix(':') else {
        return String::new();
    };
    collapse(&value[..line_end(value)])
}

/// `accessibilityDescrRegex`: `/accDescr(?:[\t ]*:([^\n\r]*)|\s*{([^}]*)})/`.
fn acc_descr_value(raw: &str) -> String {
    let Some(start) = raw.find("accDescr") else {
        return String::new();
    };
    let rest = &raw[start + "accDescr".len()..];
    let after_colon_space = trim_horizontal(rest);
    if let Some(value) = after_colon_space.strip_prefix(':') {
        return collapse(&value[..line_end(value)]);
    }
    let Some(value) = rest.trim_start_matches(is_js_space).strip_prefix('{') else {
        return String::new();
    };
    let capture = &value[..value.find('}').unwrap_or(value.len())];
    normalize_multiline(capture)
}

/// The multiline branch of the accessible description conversion: strip whitespace at both
/// ends of every line, collapse runs of two or more spaces or tabs, collapse line breaks, and
/// collapse consecutive blank lines. For the regex sequence
/// `.replace(/^\s*/gm,'').replace(/\s+$/gm,'').replace(/[\t ]{2,}/gm,' ').replace(/[\n\r]{2,}/gm,'\n')`
/// this is the same result, including the blank lines the first replacement swallows.
fn normalize_multiline(capture: &str) -> String {
    let mut lines = Vec::new();
    for line in capture.split(['\n', '\r']) {
        let line = trim(line);
        if line.is_empty() {
            continue;
        }
        lines.push(collapse(line));
    }
    lines.join("\n")
}

/// `String.prototype.trim`, close enough: JavaScript's `\s` also has U+FEFF.
fn trim(text: &str) -> &str {
    text.trim_matches(is_js_space)
}

/// `.replace(/[\t ]{2,}/gm, ' ')` on an already trimmed string.
fn collapse(text: &str) -> String {
    let text = trim(text);
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b' ' || bytes[i] == b'\t' {
            let start = i;
            while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
                i += 1;
            }
            if i - start > 1 {
                out.push(' ');
            } else {
                out.push_str(&text[start..i]);
            }
        } else {
            let ch = text[i..].chars().next().expect("non-empty remainder");
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

fn line_end(text: &str) -> usize {
    text.find(['\r', '\n']).unwrap_or(text.len())
}

fn is_js_space(ch: char) -> bool {
    crate::preprocess::is_js_whitespace(ch)
}
