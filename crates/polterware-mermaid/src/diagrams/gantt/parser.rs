//! The parser: the grammar rules of upstream's `packages/mermaid/src/diagrams/gantt/parser/
//! gantt.jison`, written as recursive descent over the lexer's tokens, plus the calls the
//! rules make on the database.
//!
//! The grammar is `start: gantt document 'EOF'` with `document: document line` and
//! `line: NL | statement`. The lexer already drops comments and whitespace, so a line is one
//! statement and a newline is the only separator. Upstream's `line: SPACE statement` is dead
//! because no lexer rule answers `SPACE`, and its `line: EOF` only matters to the generated
//! parser's token stack, so both are left out here.
//!
//! Every statement action turns the token text into the value the rule passes to the
//! database, which is upstream's `$1.substr(n)`; the `click` statements are the answers of
//! `clickStatement`.

use super::ast::Ast;
use super::db::{Db, js_trim};
use super::lexer::{self, Kind, Token};
use crate::error::ParseError;

pub(super) fn parse(body: &str) -> Result<Ast, ParseError> {
    let tokens = lexer::lex(body)?;
    let mut parser = Parser {
        tokens,
        pos: 0,
        db: Db::new(),
    };
    parser.document()?;
    parser.db.into_ast()
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

    fn unexpected(&self, expecting: &str) -> ParseError {
        ParseError::at(format!("Expecting {expecting}"), self.peek().span)
    }

    fn expect(&mut self, kind: Kind, expecting: &str) -> Result<Token, ParseError> {
        if self.at(kind) {
            Ok(self.bump())
        } else {
            Err(self.unexpected(expecting))
        }
    }

    /// `start: gantt document 'EOF'`.
    ///
    /// Upstream's grammar wants the `gantt` token first; this crate's preprocess keeps the
    /// lines it removes (frontmatter, directives) as blank lines so spans keep their line
    /// numbers, so leading newlines are skipped here, as the sequence and state parsers do.
    fn document(&mut self) -> Result<(), ParseError> {
        while self.at(Kind::Newline) {
            self.bump();
        }
        self.expect(Kind::Gantt, "'gantt'")?;
        loop {
            match self.kind() {
                Kind::Eof => return Ok(()),
                Kind::Newline => {
                    self.bump();
                }
                Kind::DateFormat => {
                    let token = self.bump();
                    self.db.set_date_format(value(&token.text, 11));
                }
                Kind::InclusiveEndDates => {
                    self.bump();
                    self.db.enable_inclusive_end_dates();
                }
                Kind::TopAxis => {
                    self.bump();
                    self.db.enable_top_axis();
                }
                Kind::AxisFormat => {
                    let token = self.bump();
                    self.db.set_axis_format(value(&token.text, 11));
                }
                Kind::TickInterval => {
                    let token = self.bump();
                    self.db.set_tick_interval(value(&token.text, 13));
                }
                Kind::Excludes => {
                    let token = self.bump();
                    self.db.set_excludes(value(&token.text, 9));
                }
                Kind::Includes => {
                    let token = self.bump();
                    self.db.set_includes(value(&token.text, 9));
                }
                Kind::TodayMarker => {
                    let token = self.bump();
                    self.db.set_today_marker(value(&token.text, 12));
                }
                Kind::Weekday => {
                    let token = self.bump();
                    if let Some(day) = day(&token.text) {
                        self.db.set_weekday(&day);
                    }
                }
                Kind::Weekend => {
                    let token = self.bump();
                    if let Some(day) = day(&token.text) {
                        self.db.set_weekend(&day);
                    }
                }
                Kind::Title => {
                    let token = self.bump();
                    self.db.set_diagram_title(value(&token.text, 6));
                }
                Kind::AccTitle => {
                    self.bump();
                    let title = self.expect(Kind::AccTitleValue, "'acc_title_value'")?;
                    self.db.set_acc_title(js_trim(&title.text));
                }
                Kind::AccDescr => {
                    self.bump();
                    let description = self.expect(Kind::AccDescrValue, "'acc_descr_value'")?;
                    self.db.set_acc_description(js_trim(&description.text));
                }
                Kind::AccDescrMultilineValue => {
                    let token = self.bump();
                    self.db.set_acc_description(js_trim(&token.text));
                }
                Kind::Section => {
                    let token = self.bump();
                    self.db.add_section(value(&token.text, 8));
                }
                Kind::Click => self.click_statement()?,
                Kind::TaskTxt => {
                    let text = self.bump();
                    let data = self.expect(Kind::TaskData, "'taskData'")?;
                    self.db.add_task(&text.text, &data.text, text.span);
                }
                Kind::Invalid => {
                    return Err(ParseError::at("Unexpected character", self.peek().span));
                }
                _ => return Err(self.unexpected("a statement")),
            }
        }
    }

    /// `clickStatement`: any combination of `callbackname` and `href`, in either order.
    ///
    /// Upstream's actions call `setClickEvent` before `setLink` whenever both appear; the
    /// database only records the classes and links.
    fn click_statement(&mut self) -> Result<(), ParseError> {
        let ids = self.bump();
        match self.kind() {
            Kind::CallbackName => {
                self.bump();
                if self.at(Kind::CallbackArgs) {
                    self.bump();
                }
                self.db.set_click_event(&ids.text);
                if self.at(Kind::Href) {
                    let href = self.bump();
                    self.db.set_link(&ids.text, &href.text);
                }
            }
            Kind::Href => {
                let href = self.bump();
                if self.at(Kind::CallbackName) {
                    self.bump();
                    if self.at(Kind::CallbackArgs) {
                        self.bump();
                    }
                    self.db.set_click_event(&ids.text);
                }
                self.db.set_link(&ids.text, &href.text);
            }
            _ => return Err(self.unexpected("'callbackname' or 'href'")),
        }
        Ok(())
    }
}

/// Upstream's `$1.substr(n)`: the token text after the keyword and its whitespace character.
/// The offset counts characters, so a non-ASCII whitespace character keeps the value whole
/// (JavaScript counts UTF-16 units; every keyword here is ASCII and the character after it is
/// a single unit).
fn value(text: &str, skip: usize) -> &str {
    match text.char_indices().map(|(index, _)| index).nth(skip) {
        Some(index) => &text[index..],
        None => "",
    }
}

/// The day a `weekday`/`weekend` token names: its last word, lowercased.
fn day(text: &str) -> Option<String> {
    text.split_whitespace()
        .next_back()
        .map(str::to_ascii_lowercase)
}
