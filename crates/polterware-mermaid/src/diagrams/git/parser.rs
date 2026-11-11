//! The git graph grammar: a hand-written recursive-descent port of `gitGraph.langium`,
//! `reference.langium`, and the `TitleAndAccessibilities` fragment and `EOL` rule of
//! `common.langium`.
//!
//! ```text
//! GitGraph:
//!   NEWLINE*
//!   ('gitGraph' | 'gitGraph' ':' | 'gitGraph:' | 'gitGraph' Direction ':')
//!   ( NEWLINE | TitleAndAccessibilities | statements+=Statement )*
//! Statement: Commit | Branch | Merge | Checkout | CherryPicking;
//! Direction: ('LR' | 'TB' | 'BT');
//! Commit: 'commit'
//!   ('id:' id=STRING | 'msg:'? message=STRING | 'tag:' tags+=STRING
//!    | 'type:' type=('NORMAL' | 'REVERSE' | 'HIGHLIGHT'))*
//!   EOL;
//! Branch: 'branch' name=(REFERENCE|STRING) ('order:' order=INT)? EOL;
//! Merge: 'merge' branch=(REFERENCE|STRING)
//!   ('id:' id=STRING | 'tag:' tags+=STRING | 'type:' type=('NORMAL'|'REVERSE'|'HIGHLIGHT'))*
//!   EOL;
//! Checkout: ('checkout'|'switch') branch=(REFERENCE|STRING) EOL;
//! CherryPicking: 'cherry-pick'
//!   ('id:' id=STRING | 'tag:' tags+=STRING | 'parent:' parent=STRING)* EOL;
//! EOL: NEWLINE+ | EOF;
//! TitleAndAccessibilities:
//!   ((accDescr=ACC_DESCR | accTitle=ACC_TITLE | title=TITLE) EOL)+
//! ```
//!
//! Title, accessible title, and accessible description are single-valued assignments, so a
//! later line overwrites an earlier one, as Langium's AST builder does. Terminal values go
//! through `common/valueConverter.ts`: the default `STRING` strip-and-unescape, the default
//! `INT` conversion (`parseInt`), and the regex-driven trimming and space collapsing of
//! `TITLE`, `ACC_TITLE`, and `ACC_DESCR`. `REFERENCE` keeps its text.
//!
//! Langium recovers from parse errors and keeps going, and its own parser tests inspect that
//! recovered tree; the diagram pipeline (`parse.ts` in `@mermaid-js/parser`) throws when the
//! error lists are non-empty, and [`super::parse`] does the same. An unknown property such as
//! `oops:"ignored"` is therefore an error here, as it is for a rendered diagram.

use super::ast::{Ast, CommitType, Direction};
use super::db;
use super::lexer::{Lexer, Token, TokenKind};
use crate::error::{ParseError, Span};
use crate::preprocess::is_js_whitespace;

/// Parses the diagram body and populates the database.
pub(crate) fn parse(body: &str) -> Result<Ast, ParseError> {
    let parsed = Parser::new(body).parse_git_graph()?;
    db::build(parsed)
}

/// The Langium-style document tree, without populating the database. The grammar tests use it
/// to inspect statements the database would reject (a merge without commits, for instance).
#[cfg(test)]
pub(crate) fn parse_tree(body: &str) -> Result<ParsedGitGraph, ParseError> {
    Parser::new(body).parse_git_graph()
}

/// The `GitGraph` AST node after parsing, before the database is populated.
#[derive(Debug, Default)]
pub(crate) struct ParsedGitGraph {
    pub(crate) direction: Option<Direction>,
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_description: Option<String>,
    pub(crate) statements: Vec<Statement>,
}

/// One `Statement` (`Statement` in `gitGraphTypes.ts`).
#[derive(Debug)]
pub(crate) enum Statement {
    Commit(CommitStatement),
    Branch(BranchStatement),
    Merge(MergeStatement),
    Checkout(CheckoutStatement),
    CherryPick(CherryPickStatement),
}

/// A `Commit` statement (`CommitAst`).
#[derive(Debug)]
pub(crate) struct CommitStatement {
    pub(crate) id: Option<String>,
    pub(crate) message: Option<String>,
    pub(crate) tags: Vec<String>,
    pub(crate) kind: Option<CommitType>,
}

/// A `Branch` statement (`BranchAst`).
#[derive(Debug)]
pub(crate) struct BranchStatement {
    pub(crate) span: Span,
    pub(crate) name: String,
    pub(crate) order: Option<i64>,
}

/// A `Merge` statement (`MergeAst`).
#[derive(Debug)]
pub(crate) struct MergeStatement {
    pub(crate) span: Span,
    pub(crate) branch: String,
    pub(crate) id: Option<String>,
    pub(crate) tags: Vec<String>,
    pub(crate) kind: Option<CommitType>,
}

/// A `Checkout` statement (`CheckoutAst`).
#[derive(Debug)]
pub(crate) struct CheckoutStatement {
    pub(crate) span: Span,
    pub(crate) branch: String,
}

/// A `CherryPicking` statement (`CherryPickingAst`). `tags` is `None` when the statement has
/// no `tag:` at all, which is not the same as one empty tag.
#[derive(Debug)]
pub(crate) struct CherryPickStatement {
    pub(crate) span: Span,
    pub(crate) id: Option<String>,
    pub(crate) tags: Option<Vec<String>>,
    pub(crate) parent: Option<String>,
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

    /// `EOL: NEWLINE+ | EOF`.
    fn expect_eol(&mut self) -> Result<(), ParseError> {
        match self.peek()? {
            TokenKind::Eof => Ok(()),
            TokenKind::Newline => {
                while self.peek()? == TokenKind::Newline {
                    self.bump()?;
                }
                Ok(())
            }
            _ => Err(self.unexpected("a newline or end of input")),
        }
    }

    fn parse_git_graph(mut self) -> Result<ParsedGitGraph, ParseError> {
        while self.peek()? == TokenKind::Newline {
            self.bump()?;
        }
        let direction = self.parse_header()?;

        let mut parsed = ParsedGitGraph {
            direction,
            ..ParsedGitGraph::default()
        };
        loop {
            match self.peek()? {
                TokenKind::Newline => {
                    self.bump()?;
                }
                TokenKind::Eof => break,
                TokenKind::AccDescr => {
                    let token = self.bump()?;
                    parsed.acc_description = Some(acc_descr_value(&token.text));
                    self.expect_eol()?;
                }
                TokenKind::AccTitle => {
                    let token = self.bump()?;
                    parsed.acc_title = Some(acc_title_value(&token.text));
                    self.expect_eol()?;
                }
                TokenKind::Title => {
                    let token = self.bump()?;
                    parsed.title = Some(title_value(&token.text));
                    self.expect_eol()?;
                }
                TokenKind::Commit => parsed
                    .statements
                    .push(Statement::Commit(self.parse_commit()?)),
                TokenKind::Branch => parsed
                    .statements
                    .push(Statement::Branch(self.parse_branch()?)),
                TokenKind::Merge => parsed
                    .statements
                    .push(Statement::Merge(self.parse_merge()?)),
                TokenKind::Checkout | TokenKind::Switch => parsed
                    .statements
                    .push(Statement::Checkout(self.parse_checkout()?)),
                TokenKind::CherryPick => parsed
                    .statements
                    .push(Statement::CherryPick(self.parse_cherry_pick()?)),
                _ => {
                    return Err(self
                        .unexpected("a statement, a title, an accessibility line, or a newline"));
                }
            }
        }
        Ok(parsed)
    }

    /// The four header alternatives, which share the `gitGraph` keyword except for the single
    /// `gitGraph:` token. The direction needs its trailing colon.
    fn parse_header(&mut self) -> Result<Option<Direction>, ParseError> {
        match self.peek()? {
            TokenKind::GitGraph => {
                self.bump()?;
                if let Some(direction) = self.peek_direction()? {
                    self.bump()?;
                    self.expect(TokenKind::Colon)?;
                    Ok(Some(direction))
                } else if self.peek()? == TokenKind::Colon {
                    self.bump()?;
                    Ok(None)
                } else {
                    Ok(None)
                }
            }
            TokenKind::GitGraphColon => {
                self.bump()?;
                Ok(None)
            }
            _ => Err(self.unexpected("'gitGraph'")),
        }
    }

    fn peek_direction(&mut self) -> Result<Option<Direction>, ParseError> {
        Ok(match self.peek()? {
            TokenKind::Lr => Some(Direction::Lr),
            TokenKind::Tb => Some(Direction::Tb),
            TokenKind::Bt => Some(Direction::Bt),
            _ => None,
        })
    }

    fn parse_commit(&mut self) -> Result<CommitStatement, ParseError> {
        self.expect(TokenKind::Commit)?;
        let mut statement = CommitStatement {
            id: None,
            message: None,
            tags: Vec::new(),
            kind: None,
        };
        loop {
            match self.peek()? {
                TokenKind::IdColon => {
                    self.bump()?;
                    statement.id = Some(self.parse_string()?);
                }
                TokenKind::MsgColon => {
                    self.bump()?;
                    statement.message = Some(self.parse_string()?);
                }
                TokenKind::Str => statement.message = Some(self.parse_string()?),
                TokenKind::TagColon => {
                    self.bump()?;
                    statement.tags.push(self.parse_string()?);
                }
                TokenKind::TypeColon => {
                    self.bump()?;
                    statement.kind = Some(self.parse_commit_type()?);
                }
                _ => break,
            }
        }
        self.expect_eol()?;
        Ok(statement)
    }

    fn parse_branch(&mut self) -> Result<BranchStatement, ParseError> {
        let keyword = self.expect(TokenKind::Branch)?;
        let name = self.parse_name()?;
        let order = if self.peek()? == TokenKind::OrderColon {
            self.bump()?;
            let token = self.expect(TokenKind::Int)?;
            Some(token.text.parse::<i64>().map_err(|_| {
                ParseError::at(format!("invalid integer '{}'", token.text), token.span)
            })?)
        } else {
            None
        };
        self.expect_eol()?;
        Ok(BranchStatement {
            span: keyword.span,
            name,
            order,
        })
    }

    fn parse_merge(&mut self) -> Result<MergeStatement, ParseError> {
        let keyword = self.expect(TokenKind::Merge)?;
        let mut statement = MergeStatement {
            span: keyword.span,
            branch: self.parse_name()?,
            id: None,
            tags: Vec::new(),
            kind: None,
        };
        loop {
            match self.peek()? {
                TokenKind::IdColon => {
                    self.bump()?;
                    statement.id = Some(self.parse_string()?);
                }
                TokenKind::TagColon => {
                    self.bump()?;
                    statement.tags.push(self.parse_string()?);
                }
                TokenKind::TypeColon => {
                    self.bump()?;
                    statement.kind = Some(self.parse_commit_type()?);
                }
                _ => break,
            }
        }
        self.expect_eol()?;
        Ok(statement)
    }

    fn parse_checkout(&mut self) -> Result<CheckoutStatement, ParseError> {
        let keyword = self.bump()?;
        let branch = self.parse_name()?;
        self.expect_eol()?;
        Ok(CheckoutStatement {
            span: keyword.span,
            branch,
        })
    }

    fn parse_cherry_pick(&mut self) -> Result<CherryPickStatement, ParseError> {
        let keyword = self.expect(TokenKind::CherryPick)?;
        let mut statement = CherryPickStatement {
            span: keyword.span,
            id: None,
            tags: None,
            parent: None,
        };
        loop {
            match self.peek()? {
                TokenKind::IdColon => {
                    self.bump()?;
                    statement.id = Some(self.parse_string()?);
                }
                TokenKind::TagColon => {
                    self.bump()?;
                    statement
                        .tags
                        .get_or_insert_with(Vec::new)
                        .push(self.parse_string()?);
                }
                TokenKind::ParentColon => {
                    self.bump()?;
                    statement.parent = Some(self.parse_string()?);
                }
                _ => break,
            }
        }
        self.expect_eol()?;
        Ok(statement)
    }

    fn parse_commit_type(&mut self) -> Result<CommitType, ParseError> {
        let kind = match self.peek()? {
            TokenKind::Normal => CommitType::Normal,
            TokenKind::Reverse => CommitType::Reverse,
            TokenKind::Highlight => CommitType::Highlight,
            _ => return Err(self.unexpected("NORMAL, REVERSE, or HIGHLIGHT")),
        };
        self.bump()?;
        Ok(kind)
    }

    /// `name=(REFERENCE|STRING)`: the reference text, or the decoded string.
    fn parse_name(&mut self) -> Result<String, ParseError> {
        match self.peek()? {
            TokenKind::Reference => Ok(self.bump()?.text),
            TokenKind::Str => self.parse_string(),
            _ => Err(self.unexpected("REFERENCE or STRING")),
        }
    }

    fn parse_string(&mut self) -> Result<String, ParseError> {
        let token = self.expect(TokenKind::Str)?;
        Ok(convert_string(&token.text))
    }
}

/// Langium's default `STRING` conversion: drop the quotes and read `\` escapes.
fn convert_string(raw: &str) -> String {
    let inner = &raw[1..raw.len() - 1];
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('b') => out.push('\u{8}'),
            Some('f') => out.push('\u{c}'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('v') => out.push('\u{b}'),
            Some('0') => out.push('\0'),
            Some(other) => out.push(other),
            // The lexer only produces a string when every backslash has a character.
            None => {}
        }
    }
    out
}

/// `common/valueConverter.ts`'s `titleRegex`: `/title([\t ][^\n\r]*|)/`, capture trimmed and
/// with runs of two or more spaces or tabs collapsed to one.
fn title_value(raw: &str) -> String {
    let Some(start) = raw.find("title") else {
        return String::new();
    };
    let rest = &raw[start + "title".len()..];
    if rest.starts_with(' ') || rest.starts_with('\t') {
        collapse_spaces(trim(rest))
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
    collapse_spaces(trim(&value[..line_end(value)]))
}

/// `accessibilityDescrRegex`: `/accDescr(?:[\t ]*:([^\n\r]*)|\s*{([^}]*)})/`.
fn acc_descr_value(raw: &str) -> String {
    let Some(start) = raw.find("accDescr") else {
        return String::new();
    };
    let rest = &raw[start + "accDescr".len()..];
    let after_colon_space = trim_horizontal(rest);
    if let Some(value) = after_colon_space.strip_prefix(':') {
        return collapse_spaces(trim(&value[..line_end(value)]));
    }
    let Some(value) = rest.trim_start_matches(is_js_whitespace).strip_prefix('{') else {
        return String::new();
    };
    let capture = &value[..value.find('}').unwrap_or(value.len())];
    // `.replace(/^\s*/gm,'').replace(/\s+$/gm,'').replace(/[\t ]{2,}/gm,' ')
    //  .replace(/[\n\r]{2,}/gm,'\n')`
    let stripped = strip_line_starts(capture);
    let trimmed = trim_line_ends(&stripped);
    collapse_newlines(&collapse_spaces(&trimmed))
}

/// The first replacement of the multiline description: `\s*` at every line start. Because
/// JavaScript's `\s` includes line breaks, a blank line is swallowed with the indentation of
/// the next one.
fn strip_line_starts(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut line_start = true;
    for ch in text.chars() {
        if line_start && is_js_whitespace(ch) {
            continue;
        }
        out.push(ch);
        line_start = is_line_break(ch);
    }
    out
}

/// `/\s+$/gm`: trailing whitespace on each line and at the end of the text. A `\s+` run is
/// greedy and may span line breaks, so it trims up to the last position where `$` holds
/// (before a line terminator or at the end of the text), exactly like the global replacement.
fn trim_line_ends(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < chars.len() {
        if !is_js_whitespace(chars[index]) {
            out.push(chars[index]);
            index += 1;
            continue;
        }
        let mut run_end = index;
        while run_end < chars.len() && is_js_whitespace(chars[run_end]) {
            run_end += 1;
        }
        let match_end = (index + 1..=run_end)
            .rev()
            .find(|&end| end == chars.len() || is_line_break(chars[end]));
        match match_end {
            Some(end) => index = end,
            None => {
                out.extend(&chars[index..run_end]);
                index = run_end;
            }
        }
    }
    out
}

/// `String.prototype.trim`, close enough: JavaScript's `\s` also has U+FEFF.
fn trim(text: &str) -> &str {
    text.trim_matches(is_js_whitespace)
}

fn trim_horizontal(text: &str) -> &str {
    text.trim_start_matches([' ', '\t'])
}

/// `.replace(/[\t ]{2,}/gm, ' ')`: a run of two or more spaces or tabs becomes one space, and
/// a single one stays as it is.
fn collapse_spaces(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut run = 0;
    let mut run_char = ' ';
    for ch in text.chars() {
        if ch == ' ' || ch == '\t' {
            if run == 0 {
                run_char = ch;
            }
            run += 1;
            continue;
        }
        if run > 1 {
            out.push(' ');
        } else if run == 1 {
            out.push(run_char);
        }
        run = 0;
        out.push(ch);
    }
    if run > 1 {
        out.push(' ');
    } else if run == 1 {
        out.push(run_char);
    }
    out
}

/// `.replace(/[\n\r]{2,}/gm, '\n')`: a run of two or more line breaks becomes one newline, and
/// a single one stays as it is.
fn collapse_newlines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut run = 0;
    let mut run_char = '\n';
    for ch in text.chars() {
        if is_line_break(ch) {
            if run == 0 {
                run_char = ch;
            }
            run += 1;
            continue;
        }
        if run > 1 {
            out.push('\n');
        } else if run == 1 {
            out.push(run_char);
        }
        run = 0;
        out.push(ch);
    }
    if run > 1 {
        out.push('\n');
    } else if run == 1 {
        out.push(run_char);
    }
    out
}

fn is_line_break(ch: char) -> bool {
    ch == '\n' || ch == '\r'
}

fn line_end(text: &str) -> usize {
    text.find(['\r', '\n']).unwrap_or(text.len())
}
