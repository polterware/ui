//! The parser: the grammar rules of upstream's `packages/mermaid/src/diagrams/user-journey/
//! parser/journey.jison`, written as recursive descent over the lexer's tokens, plus the
//! calls the rules make on the database.
//!
//! The grammar is:
//!
//! ```text
//! start    : journey document 'EOF'
//! document : /* empty */ | document line
//! line     : SPACE statement | statement | NEWLINE | EOF
//! statement
//!          : title
//!          | acc_title acc_title_value
//!          | acc_descr acc_descr_value
//!          | acc_descr_multiline_value
//!          | section
//!          | taskName taskData
//! ```
//!
//! `line: SPACE statement` is dead because no lexer rule answers `SPACE`, and `line: EOF`
//! only matters to the generated parser's stack, so both are left out. Newlines are the only
//! separators a statement needs, and the lexer can swallow one (a whitespace run that starts
//! with indentation, a `%%` comment, or the `\s` of a `title`), so a document is simply the
//! statements in token order.
//!
//! The action of each statement is upstream's: `title` and `section` drop their keyword and
//! the one whitespace character after it (`$1.substr(6)` and `$1.substr(8)`), the two
//! accessibility values are trimmed, and a task passes its two tokens to `addTask`.

use super::ast::Ast;
use super::db::{Db, js_trim};
use super::lexer::{self, Kind, Token};
use crate::error::ParseError;

/// Parses the diagram body and populates the database.
pub(super) fn parse(body: &str) -> Result<Ast, ParseError> {
    let tokens = lexer::lex(body);
    let mut parser = Parser {
        tokens,
        pos: 0,
        db: Db::new(),
    };
    parser.document()
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    db: Db,
}

impl Parser {
    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn kind(&self) -> Kind {
        self.peek().kind
    }

    fn at(&self, kind: Kind) -> bool {
        self.kind() == kind
    }

    fn bump(&mut self) -> Token {
        let token = self.tokens[self.pos].clone();
        self.pos += 1;
        token
    }

    fn unexpected(&self, expected: &str) -> ParseError {
        let token = self.peek();
        ParseError::at(
            format!("expected {expected}, got '{}'", token.kind.name()),
            token.span,
        )
    }

    fn expect(&mut self, kind: Kind, expected: &str) -> Result<Token, ParseError> {
        if self.at(kind) {
            Ok(self.bump())
        } else {
            Err(self.unexpected(expected))
        }
    }

    /// `start: journey document 'EOF'`.
    ///
    /// The body this crate's [`crate::preprocess`] hands a parser can start with the blank
    /// lines it left where it removed frontmatter or directives, so leading newlines are
    /// skipped; upstream's parser receives text that starts with the header.
    fn document(&mut self) -> Result<Ast, ParseError> {
        while self.at(Kind::Newline) {
            self.bump();
        }
        self.expect(Kind::Journey, "the 'journey' header")?;
        loop {
            match self.kind() {
                Kind::Newline => {
                    self.bump();
                }
                Kind::Eof => return Ok(std::mem::take(&mut self.db).into_ast()),
                Kind::End => {
                    return Err(ParseError::at("unexpected end of input", self.peek().span));
                }
                Kind::Title => {
                    let token = self.bump();
                    self.db
                        .set_diagram_title(after_keyword(&token.text, "title"));
                }
                Kind::AccTitle => {
                    self.bump();
                    let value = self.expect(Kind::AccTitleValue, "the accessible title text")?;
                    self.db.set_acc_title(js_trim(&value.text).to_string());
                }
                Kind::AccDescr => {
                    self.bump();
                    let value = self.expect(Kind::AccDescrValue, "the accessible description")?;
                    self.db
                        .set_acc_description(js_trim(&value.text).to_string());
                }
                Kind::AccDescrMultilineValue => {
                    let token = self.bump();
                    self.db
                        .set_acc_description(js_trim(&token.text).to_string());
                }
                Kind::Section => {
                    let token = self.bump();
                    self.db.add_section(after_keyword(&token.text, "section"));
                }
                Kind::TaskName => {
                    let name = self.bump();
                    let data = self.expect(Kind::TaskData, "the task data after ':'")?;
                    self.db.add_task(&name.text, &data.text);
                }
                _ => {
                    return Err(self.unexpected(
                        "a statement: a title, an accessibility line, a section, or a task",
                    ));
                }
            }
        }
    }
}

/// `$1.substr(n)` after a keyword: drop the keyword and the one whitespace character the
/// lexer required after it. Upstream's `substr` counts UTF-16 code units and the keyword is
/// ASCII, so skipping one character is the same slice.
fn after_keyword(text: &str, keyword: &str) -> String {
    text[keyword.len()..].chars().skip(1).collect()
}
