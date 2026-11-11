//! The usecase lexer: a hand-written reproduction of the Chevrotain token table in
//! `parser/usecase.tokens.ts`, including its three modes (`defaultMode`, `jsonBody`, and
//! `stereotype`), its longest-match precedence, and the category tokens (`LabelText`,
//! `Word`) the grammar tests for.
//!
//! Chevrotain tries the tokens of a mode in declaration order, and `longer_alt` lets a later
//! token take over when it matches more characters (`IDENTIFIER` versus `NUMBER`, and each
//! keyword versus `IDENTIFIER`). [`TokenKind`] carries the same names the parser spec uses.
//! A position no pattern matches is a [`ParseError`] carrying the character's [`Span`].

use crate::error::{ParseError, Span};

/// The kinds of `usecase.tokens.ts`, plus the synthetic end-of-input token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    AccDescrBlock,
    AccTitleLine,
    AccDescrLine,
    JsonDeclarationStart,
    JsonObjectLiteral,
    UnclosedJsonObjectLiteral,
    Usecase,
    Actor,
    SystemBoundary,
    End,
    Direction,
    Td,
    Tb,
    Bt,
    Lr,
    Rl,
    Note,
    For,
    Json,
    ClassDef,
    Class,
    Style,
    Include,
    Extend,
    True,
    False,
    Generalization,
    DependencyArrow,
    StereotypeStart,
    StereotypeEnd,
    StereotypeText,
    UnclosedStereotypeText,
    ClassSeparator,
    ForwardSolid,
    BackwardSolid,
    ForwardCircle,
    BackwardCircle,
    ForwardCross,
    BackwardCross,
    MarkerlessSolid,
    MetadataStart,
    At,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    LeftParen,
    RightParen,
    Comma,
    Colon,
    HashColor,
    PlainString,
    CssIdentifier,
    CssEscapedComma,
    Dash,
    Dot,
    Percent,
    CssPunctuation,
    LabelPunctuation,
    LabelSymbol,
    Identifier,
    NumberLiteral,
    MarkdownString,
    UnclosedMarkdownString,
    Comment,
    Newline,
    Eof,
}

impl TokenKind {
    /// The token names the parser spec uses.
    #[cfg(test)]
    pub(crate) fn name(self) -> &'static str {
        match self {
            TokenKind::AccDescrBlock => "ACC_DESCR_BLOCK",
            TokenKind::AccTitleLine => "ACC_TITLE_LINE",
            TokenKind::AccDescrLine => "ACC_DESCR_LINE",
            TokenKind::JsonDeclarationStart => "JSON_DECLARATION_START",
            TokenKind::JsonObjectLiteral => "JSON_OBJECT_LITERAL",
            TokenKind::UnclosedJsonObjectLiteral => "UNCLOSED_JSON_OBJECT_LITERAL",
            TokenKind::Usecase => "USECASE",
            TokenKind::Actor => "ACTOR",
            TokenKind::SystemBoundary => "SYSTEM_BOUNDARY",
            TokenKind::End => "END",
            TokenKind::Direction => "DIRECTION",
            TokenKind::Td => "TD",
            TokenKind::Tb => "TB",
            TokenKind::Bt => "BT",
            TokenKind::Lr => "LR",
            TokenKind::Rl => "RL",
            TokenKind::Note => "NOTE",
            TokenKind::For => "FOR",
            TokenKind::Json => "JSON",
            TokenKind::ClassDef => "CLASS_DEF",
            TokenKind::Class => "CLASS",
            TokenKind::Style => "STYLE",
            TokenKind::Include => "INCLUDE",
            TokenKind::Extend => "EXTEND",
            TokenKind::True => "TRUE",
            TokenKind::False => "FALSE",
            TokenKind::Generalization => "GENERALIZATION",
            TokenKind::DependencyArrow => "DEPENDENCY_ARROW",
            TokenKind::StereotypeStart => "STEREOTYPE_START",
            TokenKind::StereotypeEnd => "STEREOTYPE_END",
            TokenKind::StereotypeText => "STEREOTYPE_TEXT",
            TokenKind::UnclosedStereotypeText => "UNCLOSED_STEREOTYPE_TEXT",
            TokenKind::ClassSeparator => "CLASS_SEPARATOR",
            TokenKind::ForwardSolid => "FORWARD_SOLID",
            TokenKind::BackwardSolid => "BACKWARD_SOLID",
            TokenKind::ForwardCircle => "FORWARD_CIRCLE",
            TokenKind::BackwardCircle => "BACKWARD_CIRCLE",
            TokenKind::ForwardCross => "FORWARD_CROSS",
            TokenKind::BackwardCross => "BACKWARD_CROSS",
            TokenKind::MarkerlessSolid => "MARKERLESS_SOLID",
            TokenKind::MetadataStart => "METADATA_START",
            TokenKind::At => "AT",
            TokenKind::LeftBrace => "LBRACE",
            TokenKind::RightBrace => "RBRACE",
            TokenKind::LeftBracket => "LBRACKET",
            TokenKind::RightBracket => "RBRACKET",
            TokenKind::LeftParen => "LPAREN",
            TokenKind::RightParen => "RPAREN",
            TokenKind::Comma => "COMMA",
            TokenKind::Colon => "COLON",
            TokenKind::HashColor => "HASH_COLOR",
            TokenKind::PlainString => "PLAIN_STRING",
            TokenKind::CssIdentifier => "CSS_IDENTIFIER",
            TokenKind::CssEscapedComma => "CSS_ESCAPED_COMMA",
            TokenKind::Dash => "DASH",
            TokenKind::Dot => "DOT",
            TokenKind::Percent => "PERCENT",
            TokenKind::CssPunctuation => "CSS_PUNCTUATION",
            TokenKind::LabelPunctuation => "LABEL_PUNCTUATION",
            TokenKind::LabelSymbol => "LABEL_SYMBOL",
            TokenKind::Identifier => "IDENTIFIER",
            TokenKind::NumberLiteral => "NUMBER",
            TokenKind::MarkdownString => "MARKDOWN_STRING",
            TokenKind::UnclosedMarkdownString => "UNCLOSED_MARKDOWN_STRING",
            TokenKind::Comment => "COMMENT",
            TokenKind::Newline => "NEWLINE",
            TokenKind::Eof => "EOF",
        }
    }

    /// The `Word` category: every identifier-shaped token, keywords included.
    pub(crate) fn is_word(self) -> bool {
        matches!(
            self,
            TokenKind::Identifier
                | TokenKind::Usecase
                | TokenKind::Actor
                | TokenKind::SystemBoundary
                | TokenKind::End
                | TokenKind::Direction
                | TokenKind::Td
                | TokenKind::Tb
                | TokenKind::Bt
                | TokenKind::Lr
                | TokenKind::Rl
                | TokenKind::Note
                | TokenKind::For
                | TokenKind::Json
                | TokenKind::ClassDef
                | TokenKind::Class
                | TokenKind::Style
                | TokenKind::Include
                | TokenKind::Extend
                | TokenKind::True
                | TokenKind::False
        )
    }

    /// The `LabelText` category: what an unquoted label may contain.
    pub(crate) fn is_label_text(self) -> bool {
        self.is_word()
            || matches!(
                self,
                TokenKind::NumberLiteral
                    | TokenKind::At
                    | TokenKind::Comma
                    | TokenKind::Colon
                    | TokenKind::HashColor
                    | TokenKind::CssIdentifier
                    | TokenKind::CssEscapedComma
                    | TokenKind::Dash
                    | TokenKind::Dot
                    | TokenKind::Percent
                    | TokenKind::CssPunctuation
                    | TokenKind::LabelPunctuation
                    | TokenKind::LabelSymbol
            )
    }

    /// `isLabelToken`: `LabelText` plus the two string forms, which sit outside the category.
    pub(crate) fn starts_label(self) -> bool {
        self.is_label_text() || matches!(self, TokenKind::PlainString | TokenKind::MarkdownString)
    }

    /// A `styleComponent` alternative.
    pub(crate) fn is_style_component(self) -> bool {
        self.is_word()
            || matches!(
                self,
                TokenKind::PlainString
                    | TokenKind::NumberLiteral
                    | TokenKind::HashColor
                    | TokenKind::CssIdentifier
                    | TokenKind::CssEscapedComma
                    | TokenKind::Dash
                    | TokenKind::Dot
                    | TokenKind::Percent
                    | TokenKind::CssPunctuation
                    | TokenKind::Colon
                    | TokenKind::LeftParen
                    | TokenKind::RightParen
                    | TokenKind::LeftBracket
                    | TokenKind::RightBracket
                    | TokenKind::LeftBrace
                    | TokenKind::RightBrace
                    | TokenKind::At
                    | TokenKind::MarkerlessSolid
            )
    }
}

/// One lexed token: its kind, its image, and where it starts in the source.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Token<'a> {
    pub(crate) kind: TokenKind,
    pub(crate) image: &'a str,
    pub(crate) span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Default,
    JsonBody,
    Stereotype,
}

enum Matched {
    Skip(usize),
    Token(TokenKind, usize),
    Unmatched,
}

struct Lexer<'a> {
    text: &'a str,
    offset: usize,
    line: u32,
    column: u32,
    mode: Mode,
}

/// Lexes the whole body, ending with an [`TokenKind::Eof`] token.
pub(crate) fn tokenize(text: &str) -> Result<Vec<Token<'_>>, ParseError> {
    let mut lexer = Lexer {
        text,
        offset: 0,
        line: 1,
        column: 1,
        mode: Mode::Default,
    };
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next_token()? {
        tokens.push(token);
    }
    let span = Span {
        line: lexer.line,
        column: lexer.column,
        start: lexer.offset,
        end: lexer.offset,
    };
    tokens.push(Token {
        kind: TokenKind::Eof,
        image: "",
        span,
    });
    Ok(tokens)
}

impl<'a> Lexer<'a> {
    fn next_token(&mut self) -> Result<Option<Token<'a>>, ParseError> {
        loop {
            if self.offset >= self.text.len() {
                return Ok(None);
            }
            let matched = match self.mode {
                Mode::Default => self.match_default(),
                Mode::JsonBody => self.match_json_body(),
                Mode::Stereotype => self.match_stereotype(),
            };
            match matched {
                Matched::Skip(length) => self.advance(length),
                Matched::Token(kind, length) => {
                    let token = self.emit(kind, length);
                    match kind {
                        TokenKind::JsonDeclarationStart => self.mode = Mode::JsonBody,
                        TokenKind::JsonObjectLiteral
                        | TokenKind::UnclosedJsonObjectLiteral
                        | TokenKind::StereotypeEnd
                        | TokenKind::UnclosedStereotypeText => self.mode = Mode::Default,
                        TokenKind::StereotypeStart => self.mode = Mode::Stereotype,
                        _ => {}
                    }
                    return Ok(Some(token));
                }
                Matched::Unmatched => {
                    let rest = &self.text[self.offset..];
                    let ch = rest.chars().next().expect("offset is inside the text");
                    let span = Span {
                        line: self.line,
                        column: self.column,
                        start: self.offset,
                        end: self.offset + ch.len_utf8(),
                    };
                    return Err(ParseError::at(format!("unexpected character '{ch}'"), span));
                }
            }
        }
    }

    fn emit(&mut self, kind: TokenKind, length: usize) -> Token<'a> {
        let image = &self.text[self.offset..self.offset + length];
        let span = Span {
            line: self.line,
            column: self.column,
            start: self.offset,
            end: self.offset + length,
        };
        self.advance(length);
        Token { kind, image, span }
    }

    /// Advances over `length` bytes, keeping line and column in step. A CRLF pair counts as
    /// one line break, as Chevrotain's line counter does.
    fn advance(&mut self, length: usize) {
        let end = self.offset + length;
        let mut index = self.offset;
        while index < end {
            let ch = self.text[index..].chars().next().expect("inside the text");
            match ch {
                '\r' => {
                    self.line += 1;
                    self.column = 1;
                    index += 1;
                    if index < end && self.text[index..].starts_with('\n') {
                        index += 1;
                    }
                }
                '\n' => {
                    self.line += 1;
                    self.column = 1;
                    index += 1;
                }
                _ => {
                    self.column += 1;
                    index += ch.len_utf8();
                }
            }
        }
        self.offset = end;
    }

    fn match_default(&self) -> Matched {
        let rest = &self.text[self.offset..];

        // WhiteSpace, Lexer.SKIPPED.
        let hws = hspace_len(rest);
        if hws > 0 {
            return Matched::Skip(hws);
        }

        if let Some(length) = markdown_string_len(rest) {
            return Matched::Token(TokenKind::MarkdownString, length);
        }
        if rest.starts_with("\"`") {
            return Matched::Token(TokenKind::UnclosedMarkdownString, rest.len());
        }
        if rest.starts_with("%%") && self.at_indented_line_start() {
            let length = rest.find(['\n', '\r']).unwrap_or(rest.len());
            return Matched::Token(TokenKind::Comment, length);
        }
        if let Some(length) = newline_len(rest) {
            return Matched::Token(TokenKind::Newline, length);
        }
        if self.at_indented_line_start() {
            if let Some(length) = acc_descr_block_len(rest) {
                return Matched::Token(TokenKind::AccDescrBlock, length);
            }
            if let Some(length) = accessibility_line_len(rest, "accTitle") {
                return Matched::Token(TokenKind::AccTitleLine, length);
            }
            if let Some(length) = accessibility_line_len(rest, "accDescr") {
                return Matched::Token(TokenKind::AccDescrLine, length);
            }
        }
        if let Some(length) = json_declaration_len(rest) {
            return Matched::Token(TokenKind::JsonDeclarationStart, length);
        }

        for (word, kind) in KEYWORDS {
            if let Some(length) = keyword_len(
                rest,
                word,
                *kind == TokenKind::Include || *kind == TokenKind::Extend,
            ) {
                return Matched::Token(*kind, length);
            }
        }

        if rest.starts_with("--|>") {
            return Matched::Token(TokenKind::Generalization, 4);
        }
        if rest.starts_with("..>") {
            return Matched::Token(TokenKind::DependencyArrow, 3);
        }
        if rest.starts_with("<<") {
            return Matched::Token(TokenKind::StereotypeStart, 2);
        }
        if rest.starts_with(":::") {
            return Matched::Token(TokenKind::ClassSeparator, 3);
        }
        if let Some(length) = forward_solid_len(rest) {
            return Matched::Token(TokenKind::ForwardSolid, length);
        }
        if let Some(length) = backward_solid_len(rest) {
            return Matched::Token(TokenKind::BackwardSolid, length);
        }
        if rest.starts_with("--o") {
            return Matched::Token(TokenKind::ForwardCircle, 3);
        }
        if rest.starts_with("o--") {
            return Matched::Token(TokenKind::BackwardCircle, 3);
        }
        if rest.starts_with("--x") {
            return Matched::Token(TokenKind::ForwardCross, 3);
        }
        if rest.starts_with("x--") {
            return Matched::Token(TokenKind::BackwardCross, 3);
        }
        if let Some(length) = markerless_solid_len(rest) {
            return Matched::Token(TokenKind::MarkerlessSolid, length);
        }
        if rest.starts_with("@{") {
            return Matched::Token(TokenKind::MetadataStart, 2);
        }

        if let Some(kind) = single_char_token(rest) {
            return Matched::Token(kind, 1);
        }
        if let Some(length) = hash_color_len(rest) {
            return Matched::Token(TokenKind::HashColor, length);
        }
        if let Some(length) = plain_string_len(rest) {
            return Matched::Token(TokenKind::PlainString, length);
        }
        if let Some(length) = css_identifier_len(rest) {
            return Matched::Token(TokenKind::CssIdentifier, length);
        }
        let identifier = identifier_len(rest);
        if identifier > 0 {
            // IDENTIFIER's `longer_alt` hands a longer decimal to NUMBER.
            let number = number_len(rest);
            if number > identifier {
                return Matched::Token(TokenKind::NumberLiteral, number);
            }
            return Matched::Token(TokenKind::Identifier, identifier);
        }
        if let Some(length) = number_len_opt(rest) {
            return Matched::Token(TokenKind::NumberLiteral, length);
        }
        if rest.starts_with("\\,") {
            return Matched::Token(TokenKind::CssEscapedComma, 2);
        }
        if rest.starts_with('-') {
            return Matched::Token(TokenKind::Dash, 1);
        }
        if rest.starts_with('.') {
            return Matched::Token(TokenKind::Dot, 1);
        }
        if rest.starts_with('%') {
            return Matched::Token(TokenKind::Percent, 1);
        }
        if let Some(ch) = rest.chars().next()
            && "!#$&*+/=?^_|~".contains(ch)
        {
            return Matched::Token(TokenKind::CssPunctuation, 1);
        }
        if let Some(ch) = rest.chars().next()
            && ";<>\\`".contains(ch)
        {
            return Matched::Token(TokenKind::LabelPunctuation, 1);
        }
        if let Some(length) = label_symbol_len(rest) {
            return Matched::Token(TokenKind::LabelSymbol, length);
        }
        Matched::Unmatched
    }

    fn match_json_body(&self) -> Matched {
        let rest = &self.text[self.offset..];
        if !rest.starts_with('{') {
            return Matched::Unmatched;
        }
        match json_object_len(rest) {
            Some(length) => Matched::Token(TokenKind::JsonObjectLiteral, length),
            None => Matched::Token(TokenKind::UnclosedJsonObjectLiteral, rest.len()),
        }
    }

    fn match_stereotype(&self) -> Matched {
        let rest = &self.text[self.offset..];
        if rest.starts_with(">>") {
            return Matched::Token(TokenKind::StereotypeEnd, 2);
        }
        if let Some(length) = stereotype_text_len(rest) {
            return Matched::Token(TokenKind::StereotypeText, length);
        }
        let length = rest.find(['\n', '\r']).unwrap_or(rest.len());
        if length > 0 {
            return Matched::Token(TokenKind::UnclosedStereotypeText, length);
        }
        Matched::Unmatched
    }

    /// `isIndentedLineStart`: only spaces and tabs between the offset and a line break.
    fn at_indented_line_start(&self) -> bool {
        let bytes = self.text.as_bytes();
        let mut index = self.offset;
        while index > 0 {
            let byte = bytes[index - 1];
            if byte == b'\n' || byte == b'\r' {
                return true;
            }
            if byte != b' ' && byte != b'\t' {
                return false;
            }
            index -= 1;
        }
        true
    }
}

/// `keyword(name, pattern)`, in declaration order.
const KEYWORDS: &[(&str, TokenKind)] = &[
    ("usecase-beta", TokenKind::Usecase),
    ("actor", TokenKind::Actor),
    ("systemBoundary", TokenKind::SystemBoundary),
    ("end", TokenKind::End),
    ("direction", TokenKind::Direction),
    ("TD", TokenKind::Td),
    ("TB", TokenKind::Tb),
    ("BT", TokenKind::Bt),
    ("LR", TokenKind::Lr),
    ("RL", TokenKind::Rl),
    ("note", TokenKind::Note),
    ("for", TokenKind::For),
    ("json", TokenKind::Json),
    ("classDef", TokenKind::ClassDef),
    ("class", TokenKind::Class),
    ("style", TokenKind::Style),
    ("include", TokenKind::Include),
    ("extend", TokenKind::Extend),
    ("true", TokenKind::True),
    ("false", TokenKind::False),
];

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn hspace_len(rest: &str) -> usize {
    rest.as_bytes()
        .iter()
        .take_while(|byte| **byte == b' ' || **byte == b'\t')
        .count()
}

fn identifier_len(rest: &str) -> usize {
    rest.as_bytes()
        .iter()
        .take_while(|byte| is_word_byte(**byte))
        .count()
}

fn keyword_len(rest: &str, word: &str, ignore_case: bool) -> Option<usize> {
    let head = rest.get(..word.len())?;
    let matched = if ignore_case {
        head.eq_ignore_ascii_case(word)
    } else {
        head == word
    };
    if !matched {
        return None;
    }
    // `longer_alt: Identifier`: a longer identifier wins.
    if identifier_len(rest) > word.len() {
        return None;
    }
    Some(word.len())
}

/// `(?:\d+\.\d+|\d+|\.\d+)(?:[A-Za-z]+)?`, returning 0 when it does not match.
fn number_len(rest: &str) -> usize {
    number_len_opt(rest).unwrap_or(0)
}

fn number_len_opt(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    let mut index = 0;
    let mut matched = false;
    // \d+\.\d+
    let integer = digits_len(bytes, 0);
    if integer > 0 && bytes.get(integer) == Some(&b'.') {
        let fraction = digits_len(bytes, integer + 1);
        if fraction > 0 {
            index = integer + 1 + fraction;
            matched = true;
        }
    }
    if !matched {
        if integer > 0 {
            index = integer;
            matched = true;
        } else if bytes.first() == Some(&b'.') {
            let fraction = digits_len(bytes, 1);
            if fraction > 0 {
                index = 1 + fraction;
                matched = true;
            }
        }
    }
    if !matched {
        return None;
    }
    let letters = bytes[index..]
        .iter()
        .take_while(|byte| byte.is_ascii_alphabetic())
        .count();
    Some(index + letters)
}

fn digits_len(bytes: &[u8], start: usize) -> usize {
    bytes[start..]
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count()
}

fn newline_len(rest: &str) -> Option<usize> {
    if rest.starts_with("\r\n") {
        Some(2)
    } else if rest.starts_with('\n') || rest.starts_with('\r') {
        Some(1)
    } else {
        None
    }
}

fn markdown_string_len(rest: &str) -> Option<usize> {
    if !rest.starts_with("\"`") {
        return None;
    }
    let bytes = rest.as_bytes();
    let mut index = 2;
    while index + 1 < bytes.len() {
        if bytes[index] == b'`' && bytes[index + 1] == b'"' {
            return Some(index + 2);
        }
        index += 1;
    }
    None
}

fn accessibility_line_len(rest: &str, keyword: &str) -> Option<usize> {
    let rest = rest.strip_prefix(keyword)?;
    let bytes = rest.as_bytes();
    let mut index = 0;
    while bytes.get(index) == Some(&b' ') || bytes.get(index) == Some(&b'\t') {
        index += 1;
    }
    if bytes.get(index) != Some(&b':') {
        return None;
    }
    index += 1;
    while index < bytes.len() && bytes[index] != b'\n' && bytes[index] != b'\r' {
        index += 1;
    }
    Some(keyword.len() + index)
}

fn acc_descr_block_len(rest: &str) -> Option<usize> {
    let rest = rest.strip_prefix("accDescr")?;
    let bytes = rest.as_bytes();
    let mut index = 0;
    while bytes.get(index) == Some(&b' ') || bytes.get(index) == Some(&b'\t') {
        index += 1;
    }
    if bytes.get(index) != Some(&b'{') {
        return None;
    }
    let close = rest[index + 1..].find('}')?;
    Some("accDescr".len() + index + 1 + close + 1)
}

fn json_declaration_len(rest: &str) -> Option<usize> {
    let rest = rest.strip_prefix("json")?;
    let bytes = rest.as_bytes();
    let mut index = 0;
    while bytes.get(index) == Some(&b' ') || bytes.get(index) == Some(&b'\t') {
        index += 1;
    }
    if index == 0 {
        return None;
    }
    let id_start = index;
    while bytes.get(index).is_some_and(|byte| is_word_byte(*byte)) {
        index += 1;
    }
    if index == id_start {
        return None;
    }
    while bytes.get(index) == Some(&b' ') || bytes.get(index) == Some(&b'\t') {
        index += 1;
    }
    if bytes.get(index) != Some(&b'@') {
        return None;
    }
    index += 1;
    while bytes.get(index) == Some(&b' ') || bytes.get(index) == Some(&b'\t') {
        index += 1;
    }
    if bytes.get(index) != Some(&b'{') {
        return None;
    }
    Some("json".len() + index)
}

fn forward_solid_len(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    if bytes.first() != Some(&b'-') {
        return None;
    }
    let mut index = 0;
    while bytes.get(index) == Some(&b'-') {
        index += 1;
    }
    if index >= 2 && bytes.get(index) == Some(&b'>') {
        Some(index + 1)
    } else {
        None
    }
}

fn backward_solid_len(rest: &str) -> Option<usize> {
    let rest = rest.strip_prefix('<')?;
    let bytes = rest.as_bytes();
    let mut index = 0;
    while bytes.get(index) == Some(&b'-') {
        index += 1;
    }
    if index >= 2 { Some(index + 1) } else { None }
}

fn markerless_solid_len(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    let mut index = 0;
    while bytes.get(index) == Some(&b'-') {
        index += 1;
    }
    if index >= 2 { Some(index) } else { None }
}

fn single_char_token(rest: &str) -> Option<TokenKind> {
    let ch = rest.chars().next()?;
    Some(match ch {
        '@' => TokenKind::At,
        '{' => TokenKind::LeftBrace,
        '}' => TokenKind::RightBrace,
        '[' => TokenKind::LeftBracket,
        ']' => TokenKind::RightBracket,
        '(' => TokenKind::LeftParen,
        ')' => TokenKind::RightParen,
        ',' => TokenKind::Comma,
        ':' => TokenKind::Colon,
        _ => return None,
    })
}

fn hash_color_len(rest: &str) -> Option<usize> {
    let rest = rest.strip_prefix('#')?;
    let hex = rest
        .bytes()
        .take_while(|byte| byte.is_ascii_hexdigit())
        .count();
    if hex == 0 {
        return None;
    }
    Some(hex + 1)
}

fn plain_string_len(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    let quote = *bytes.first()?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    let mut index = 1;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == quote {
            return Some(index + 1);
        }
        if byte == b'\n' || byte == b'\r' {
            return None;
        }
        index += 1;
    }
    None
}

fn css_identifier_len(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    if bytes.starts_with(b"--")
        && bytes.len() > 2
        && (bytes[2].is_ascii_alphabetic() || bytes[2] == b'_')
    {
        let mut index = 3;
        while bytes
            .get(index)
            .is_some_and(|byte| is_word_byte(*byte) || *byte == b'-')
        {
            index += 1;
        }
        return Some(index);
    }
    let first = *bytes.first()?;
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return None;
    }
    let mut index = 1;
    while bytes.get(index).is_some_and(|byte| is_word_byte(*byte)) {
        index += 1;
    }
    let mut groups = 0;
    while bytes.get(index) == Some(&b'-') && bytes.get(index + 1).is_some_and(|b| is_word_byte(*b))
    {
        index += 1;
        while bytes.get(index).is_some_and(|byte| is_word_byte(*byte)) {
            index += 1;
        }
        groups += 1;
    }
    if groups == 0 {
        return None;
    }
    Some(index)
}

fn label_symbol_len(rest: &str) -> Option<usize> {
    let mut length = 0;
    for ch in rest.chars() {
        let is_plain_ascii = ch.is_ascii() && (0x21..=0x7e).contains(&(ch as u32));
        if ch == '\t' || ch == '\n' || ch == '\r' || ch == ' ' || is_plain_ascii {
            break;
        }
        length += ch.len_utf8();
    }
    if length == 0 { None } else { Some(length) }
}

fn json_object_len(rest: &str) -> Option<usize> {
    let mut depth = 0_i32;
    let mut quoted = false;
    let mut escaped = false;
    for (index, ch) in rest.char_indices() {
        if quoted {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = false;
            }
            continue;
        }
        match ch {
            '"' => quoted = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index + 1);
                }
            }
            _ => {}
        }
    }
    None
}

fn stereotype_text_len(rest: &str) -> Option<usize> {
    let end = rest.find(">>")?;
    let image = &rest[..end];
    if image.contains(['\n', '\r']) || image.trim().is_empty() {
        return None;
    }
    Some(end)
}
