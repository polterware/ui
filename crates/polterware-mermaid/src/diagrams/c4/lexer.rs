//! The C4 lexer: a hand port of the `%lex` section of upstream
//! `diagrams/c4/parser/c4Diagram.jison`.
//!
//! jison's `jison-lex` compiles every rule to a regex anchored at the current position and
//! takes the first rule, in file order, that matches (no `%options flex` here). Rules without
//! a `<state>` prefix belong to `INITIAL` only, and every `%x` state is exclusive, so a state
//! sees only the rules tagged with it.
//!
//! `jison-lex` appends a `\b` to a rule that is a single quoted literal, so `"Rel"` does not
//! match the `Rel_Up` of `Rel_Up(a, b, "c")`; that is why the longer keywords still work
//! although `"Rel"` is declared first. Quoted literals elsewhere (`'('`, `'{'`) are escaped
//! and match literally.
//!
//! The ten element keywords and the seven start-condition families all behave the same while
//! an argument list is open, so they share one `Struct` state; the token itself says which
//! declaration it begins. `ATTRIBUTE`, `SPACE`, and `EOL` are declared upstream but the
//! lexer can never return them (earlier rules consume the same text), so they are not
//! produced here either.

use crate::error::{ParseError, Span};
use crate::preprocess::is_js_whitespace;

/// The direction a lone `direction` statement selects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Direction {
    Tb,
    Bt,
    Rl,
    Lr,
}

impl Direction {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Direction::Tb => "TB",
            Direction::Bt => "BT",
            Direction::Rl => "RL",
            Direction::Lr => "LR",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    Direction(Direction),
    Title,
    AccDescription,
    AccTitle,
    AccTitleValue,
    AccDescr,
    AccDescrValue,
    AccDescrMultilineValue,
    Newline,
    C4Context,
    C4Container,
    C4Component,
    C4Dynamic,
    C4Deployment,
    Person,
    PersonExt,
    System,
    SystemDb,
    SystemQueue,
    SystemExt,
    SystemExtDb,
    SystemExtQueue,
    Boundary,
    EnterpriseBoundary,
    SystemBoundary,
    ContainerBoundary,
    Container,
    ContainerDb,
    ContainerQueue,
    ContainerExt,
    ContainerExtDb,
    ContainerExtQueue,
    Component,
    ComponentDb,
    ComponentQueue,
    ComponentExt,
    ComponentExtDb,
    ComponentExtQueue,
    Node,
    NodeL,
    NodeR,
    Rel,
    BiRel,
    RelU,
    RelD,
    RelL,
    RelR,
    RelB,
    RelIndex,
    UpdateElStyle,
    UpdateRelStyle,
    UpdateLayoutConfig,
    Lbrace,
    Rbrace,
    Str,
    StrKey,
    StrValue,
    AttributeEmpty,
    Eof,
    EofInStruct,
}

impl TokenKind {
    /// The terminal name upstream's parser reports for this token.
    pub(crate) fn name(self) -> &'static str {
        match self {
            TokenKind::Direction(_) => "direction",
            TokenKind::Title => "title",
            TokenKind::AccDescription => "accDescription",
            TokenKind::AccTitle => "acc_title",
            TokenKind::AccTitleValue => "acc_title_value",
            TokenKind::AccDescr => "acc_descr",
            TokenKind::AccDescrValue => "acc_descr_value",
            TokenKind::AccDescrMultilineValue => "acc_descr_multiline_value",
            TokenKind::Newline => "NEWLINE",
            TokenKind::C4Context => "C4_CONTEXT",
            TokenKind::C4Container => "C4_CONTAINER",
            TokenKind::C4Component => "C4_COMPONENT",
            TokenKind::C4Dynamic => "C4_DYNAMIC",
            TokenKind::C4Deployment => "C4_DEPLOYMENT",
            TokenKind::Person => "PERSON",
            TokenKind::PersonExt => "PERSON_EXT",
            TokenKind::System => "SYSTEM",
            TokenKind::SystemDb => "SYSTEM_DB",
            TokenKind::SystemQueue => "SYSTEM_QUEUE",
            TokenKind::SystemExt => "SYSTEM_EXT",
            TokenKind::SystemExtDb => "SYSTEM_EXT_DB",
            TokenKind::SystemExtQueue => "SYSTEM_EXT_QUEUE",
            TokenKind::Boundary => "BOUNDARY",
            TokenKind::EnterpriseBoundary => "ENTERPRISE_BOUNDARY",
            TokenKind::SystemBoundary => "SYSTEM_BOUNDARY",
            TokenKind::ContainerBoundary => "CONTAINER_BOUNDARY",
            TokenKind::Container => "CONTAINER",
            TokenKind::ContainerDb => "CONTAINER_DB",
            TokenKind::ContainerQueue => "CONTAINER_QUEUE",
            TokenKind::ContainerExt => "CONTAINER_EXT",
            TokenKind::ContainerExtDb => "CONTAINER_EXT_DB",
            TokenKind::ContainerExtQueue => "CONTAINER_EXT_QUEUE",
            TokenKind::Component => "COMPONENT",
            TokenKind::ComponentDb => "COMPONENT_DB",
            TokenKind::ComponentQueue => "COMPONENT_QUEUE",
            TokenKind::ComponentExt => "COMPONENT_EXT",
            TokenKind::ComponentExtDb => "COMPONENT_EXT_DB",
            TokenKind::ComponentExtQueue => "COMPONENT_EXT_QUEUE",
            TokenKind::Node => "NODE",
            TokenKind::NodeL => "NODE_L",
            TokenKind::NodeR => "NODE_R",
            TokenKind::Rel => "REL",
            TokenKind::BiRel => "BIREL",
            TokenKind::RelU => "REL_U",
            TokenKind::RelD => "REL_D",
            TokenKind::RelL => "REL_L",
            TokenKind::RelR => "REL_R",
            TokenKind::RelB => "REL_B",
            TokenKind::RelIndex => "REL_INDEX",
            TokenKind::UpdateElStyle => "UPDATE_EL_STYLE",
            TokenKind::UpdateRelStyle => "UPDATE_REL_STYLE",
            TokenKind::UpdateLayoutConfig => "UPDATE_LAYOUT_CONFIG",
            TokenKind::Lbrace => "LBRACE",
            TokenKind::Rbrace => "RBRACE",
            TokenKind::Str => "STR",
            TokenKind::StrKey => "STR_KEY",
            TokenKind::StrValue => "STR_VALUE",
            TokenKind::AttributeEmpty => "ATTRIBUTE_EMPTY",
            TokenKind::Eof => "EOF",
            TokenKind::EofInStruct => "EOF_IN_STRUCT",
        }
    }

    /// Whether the keyword opens an argument list, i.e. whether its jison action calls
    /// `this.begin(...)`. The five diagram headers do not.
    pub(crate) fn opens_arguments(self) -> bool {
        !matches!(
            self,
            TokenKind::Direction(_)
                | TokenKind::Title
                | TokenKind::AccDescription
                | TokenKind::AccTitle
                | TokenKind::AccTitleValue
                | TokenKind::AccDescr
                | TokenKind::AccDescrValue
                | TokenKind::AccDescrMultilineValue
                | TokenKind::Newline
                | TokenKind::C4Context
                | TokenKind::C4Container
                | TokenKind::C4Component
                | TokenKind::C4Dynamic
                | TokenKind::C4Deployment
                | TokenKind::Lbrace
                | TokenKind::Rbrace
                | TokenKind::Str
                | TokenKind::StrKey
                | TokenKind::StrValue
                | TokenKind::AttributeEmpty
                | TokenKind::Eof
                | TokenKind::EofInStruct
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Token {
    pub(crate) kind: TokenKind,
    pub(crate) text: String,
    pub(crate) span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Initial,
    /// Any element keyword's start condition: an argument list is being lexed.
    Struct,
    Attribute,
    String,
    StringKv,
    StringKvKey,
    StringKvValue,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
}

pub(crate) struct Lexer<'a> {
    source: &'a str,
    pos: usize,
    states: Vec<State>,
    line_starts: Vec<usize>,
    done: bool,
}

impl<'a> Lexer<'a> {
    pub(crate) fn new(source: &'a str) -> Self {
        let mut line_starts = vec![0];
        for (index, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(index + 1);
            }
        }
        Self {
            source,
            pos: 0,
            states: vec![State::Initial],
            line_starts,
            done: false,
        }
    }

    fn state(&self) -> State {
        *self.states.last().expect("the state stack keeps INITIAL")
    }

    fn push_state(&mut self, state: State) {
        self.states.push(state);
    }

    fn pop_state(&mut self) {
        if self.states.len() > 1 {
            self.states.pop();
        }
    }

    fn rest(&self) -> &'a str {
        &self.source[self.pos..]
    }

    fn span(&self, start: usize, end: usize) -> Span {
        let line = self.line_starts.partition_point(|&offset| offset <= start) as u32;
        let line_start = self.line_starts[line.saturating_sub(1) as usize];
        Span {
            line,
            column: self.source[line_start..start].chars().count() as u32 + 1,
            start,
            end,
        }
    }

    pub(crate) fn next_token(&mut self) -> Result<Token, ParseError> {
        loop {
            if self.pos >= self.source.len() {
                return self.at_end();
            }
            let at = self.pos;
            let text = self.rest();
            match self.state() {
                State::Initial => {
                    if let Some((length, direction)) = match_direction(text) {
                        return Ok(self.token(TokenKind::Direction(direction), length));
                    }
                    if let Some(length) = match_word_and_rest(text, "title") {
                        return Ok(self.token(TokenKind::Title, length));
                    }
                    if let Some(length) = match_word_and_rest(text, "accDescription") {
                        return Ok(self.token(TokenKind::AccDescription, length));
                    }
                    if let Some(length) = match_prefixed_with_colon(text, "accTitle") {
                        self.push_state(State::AccTitle);
                        return Ok(self.token(TokenKind::AccTitle, length));
                    }
                    if let Some(length) = match_prefixed_with_colon(text, "accDescr") {
                        self.push_state(State::AccDescr);
                        return Ok(self.token(TokenKind::AccDescr, length));
                    }
                    if let Some(length) = match_prefixed_with_brace(text, "accDescr") {
                        self.push_state(State::AccDescrMultiline);
                        self.pos += length;
                        continue;
                    }
                    if text.starts_with("%%") {
                        self.pos = at + match_comment(text);
                        continue;
                    }
                    if let Some(length) = match_newline(text) {
                        return Ok(self.token(TokenKind::Newline, length));
                    }
                    if let Some(length) = match_whitespace(text) {
                        self.pos += length;
                        continue;
                    }
                    if let Some((kind, length)) = match_keyword(text) {
                        if kind.opens_arguments() {
                            self.push_state(State::Struct);
                        }
                        return Ok(self.token(kind, length));
                    }
                    if text.starts_with('{') {
                        return Ok(self.token(TokenKind::Lbrace, 1));
                    }
                    if text.starts_with('}') {
                        return Ok(self.token(TokenKind::Rbrace, 1));
                    }
                    return Err(self.lexical_error());
                }
                State::Struct => {
                    if let Some(length) = match_open_then_comma(text) {
                        self.push_state(State::Attribute);
                        return Ok(self.token(TokenKind::AttributeEmpty, length));
                    }
                    if text.starts_with('(') {
                        self.push_state(State::Attribute);
                        self.pos += 1;
                        continue;
                    }
                    if text.starts_with(')') {
                        self.pop_state();
                        self.pop_state();
                        self.pos += 1;
                        continue;
                    }
                    return Err(self.lexical_error());
                }
                State::Attribute => {
                    if text.starts_with(')') {
                        self.pop_state();
                        self.pop_state();
                        self.pos += 1;
                        continue;
                    }
                    if text.starts_with(",,") {
                        return Ok(self.token(TokenKind::AttributeEmpty, 2));
                    }
                    if text.starts_with(',') {
                        self.pos += 1;
                        continue;
                    }
                    if let Some(length) = match_spaces_quotes_quotes(text) {
                        return Ok(self.token(TokenKind::AttributeEmpty, length));
                    }
                    if let Some(length) = match_spaces_then(text, '"') {
                        self.push_state(State::String);
                        self.pos += length;
                        continue;
                    }
                    if let Some(length) = match_spaces_then(text, '$') {
                        self.push_state(State::StringKv);
                        self.pos += length;
                        continue;
                    }
                    if let Some(length) = match_not_comma(text) {
                        return Ok(self.token(TokenKind::Str, length));
                    }
                    return Err(self.lexical_error());
                }
                State::String => {
                    if text.starts_with('"') {
                        self.pop_state();
                        self.pos += 1;
                        continue;
                    }
                    let length = text.find('"').unwrap_or(text.len());
                    return Ok(self.token(TokenKind::Str, length));
                }
                State::StringKv => {
                    self.push_state(State::StringKvKey);
                    let length = text.find('=').unwrap_or(text.len());
                    return Ok(self.token(TokenKind::StrKey, length));
                }
                State::StringKvKey => {
                    if let Some(length) = match_equals_then_quote(text) {
                        self.pop_state();
                        self.push_state(State::StringKvValue);
                        self.pos += length;
                        continue;
                    }
                    return Err(self.lexical_error());
                }
                State::StringKvValue => {
                    if let Some(length) = match_not_quote(text) {
                        return Ok(self.token(TokenKind::StrValue, length));
                    }
                    if text.starts_with('"') {
                        self.pop_state();
                        self.pop_state();
                        self.pos += 1;
                        continue;
                    }
                    return Err(self.lexical_error());
                }
                State::AccTitle => {
                    self.pop_state();
                    let length = text.find('\n').unwrap_or(text.len());
                    return Ok(self.token(TokenKind::AccTitleValue, length));
                }
                State::AccDescr => {
                    self.pop_state();
                    let length = text.find('\n').unwrap_or(text.len());
                    return Ok(self.token(TokenKind::AccDescrValue, length));
                }
                State::AccDescrMultiline => {
                    if text.starts_with('}') {
                        self.pop_state();
                        self.pos += 1;
                        continue;
                    }
                    let length = text.find('}').unwrap_or(text.len());
                    return Ok(self.token(TokenKind::AccDescrMultilineValue, length));
                }
            }
        }
    }

    /// jison's end of input: the state's `<<EOF>>` rule if it has one, otherwise the bare
    /// `EOF` token, and the generic `EOF` once an empty rule already answered.
    fn at_end(&mut self) -> Result<Token, ParseError> {
        let kind = if self.done {
            TokenKind::Eof
        } else {
            self.done = true;
            match self.state() {
                State::Struct => TokenKind::EofInStruct,
                State::String => TokenKind::Str,
                State::StringKv => {
                    self.push_state(State::StringKvKey);
                    TokenKind::StrKey
                }
                State::AccTitle => {
                    self.pop_state();
                    TokenKind::AccTitleValue
                }
                State::AccDescr => {
                    self.pop_state();
                    TokenKind::AccDescrValue
                }
                State::AccDescrMultiline => TokenKind::AccDescrMultilineValue,
                State::Initial | State::Attribute | State::StringKvKey | State::StringKvValue => {
                    TokenKind::Eof
                }
            }
        };
        let span = self.span(self.pos, self.pos);
        Ok(Token {
            kind,
            text: String::new(),
            span,
        })
    }

    fn token(&mut self, kind: TokenKind, length: usize) -> Token {
        let start = self.pos;
        self.pos += length;
        Token {
            kind,
            text: self.source[start..self.pos].to_string(),
            span: self.span(start, self.pos),
        }
    }

    fn lexical_error(&self) -> ParseError {
        let span = self.span(self.pos, self.pos);
        match self.rest().chars().next() {
            Some(ch) => ParseError::at(format!("lexical error: unrecognized text '{ch}'"), span),
            None => ParseError::at("lexical error: unrecognized text", span),
        }
    }
}

/// The ten element keywords and the seven start-condition families, in the order the jison
/// lexer tests them.
const KEYWORDS: &[(&str, TokenKind)] = &[
    ("C4Context", TokenKind::C4Context),
    ("C4Container", TokenKind::C4Container),
    ("C4Component", TokenKind::C4Component),
    ("C4Dynamic", TokenKind::C4Dynamic),
    ("C4Deployment", TokenKind::C4Deployment),
    ("Person_Ext", TokenKind::PersonExt),
    ("Person", TokenKind::Person),
    ("SystemQueue_Ext", TokenKind::SystemExtQueue),
    ("SystemDb_Ext", TokenKind::SystemExtDb),
    ("System_Ext", TokenKind::SystemExt),
    ("SystemQueue", TokenKind::SystemQueue),
    ("SystemDb", TokenKind::SystemDb),
    ("System", TokenKind::System),
    ("Boundary", TokenKind::Boundary),
    ("Enterprise_Boundary", TokenKind::EnterpriseBoundary),
    ("System_Boundary", TokenKind::SystemBoundary),
    ("ContainerQueue_Ext", TokenKind::ContainerExtQueue),
    ("ContainerDb_Ext", TokenKind::ContainerExtDb),
    ("Container_Ext", TokenKind::ContainerExt),
    ("ContainerQueue", TokenKind::ContainerQueue),
    ("ContainerDb", TokenKind::ContainerDb),
    ("Container", TokenKind::Container),
    ("Container_Boundary", TokenKind::ContainerBoundary),
    ("ComponentQueue_Ext", TokenKind::ComponentExtQueue),
    ("ComponentDb_Ext", TokenKind::ComponentExtDb),
    ("Component_Ext", TokenKind::ComponentExt),
    ("ComponentQueue", TokenKind::ComponentQueue),
    ("ComponentDb", TokenKind::ComponentDb),
    ("Component", TokenKind::Component),
    ("Deployment_Node", TokenKind::Node),
    ("Node", TokenKind::Node),
    ("Node_L", TokenKind::NodeL),
    ("Node_R", TokenKind::NodeR),
    ("Rel", TokenKind::Rel),
    ("BiRel", TokenKind::BiRel),
    ("Rel_Up", TokenKind::RelU),
    ("Rel_U", TokenKind::RelU),
    ("Rel_Down", TokenKind::RelD),
    ("Rel_D", TokenKind::RelD),
    ("Rel_Left", TokenKind::RelL),
    ("Rel_L", TokenKind::RelL),
    ("Rel_Right", TokenKind::RelR),
    ("Rel_R", TokenKind::RelR),
    ("Rel_Back", TokenKind::RelB),
    ("RelIndex", TokenKind::RelIndex),
    ("UpdateElementStyle", TokenKind::UpdateElStyle),
    ("UpdateRelStyle", TokenKind::UpdateRelStyle),
    ("UpdateLayoutConfig", TokenKind::UpdateLayoutConfig),
];

fn match_keyword(text: &str) -> Option<(TokenKind, usize)> {
    KEYWORDS
        .iter()
        .find_map(|(literal, kind)| literal_prefix(text, literal).map(|length| (*kind, length)))
}

/// A quoted rule literal: `jison-lex` compiles it with a trailing `\b`, so the keyword must
/// not be followed by a word character.
fn literal_prefix(text: &str, literal: &str) -> Option<usize> {
    let rest = text.strip_prefix(literal)?;
    match rest.chars().next() {
        Some(ch) if ch.is_ascii_alphanumeric() || ch == '_' => None,
        _ => Some(literal.len()),
    }
}

/// `.*direction\s+TB[^\n]*` and its three siblings. `.*` is greedy and cannot cross a line
/// terminator, so the last `direction` on the line wins; the trailing `[^\n]*` runs to the
/// next line feed.
fn match_direction(text: &str) -> Option<(usize, Direction)> {
    let window = text
        .split(['\n', '\r', '\u{2028}', '\u{2029}'])
        .next()
        .unwrap_or("");
    for (word, direction) in [
        ("TB", Direction::Tb),
        ("BT", Direction::Bt),
        ("RL", Direction::Rl),
        ("LR", Direction::Lr),
    ] {
        if let Some(length) = match_direction_word(text, window, word) {
            return Some((length, direction));
        }
    }
    None
}

fn match_direction_word(text: &str, window: &str, word: &str) -> Option<usize> {
    let mut from = window.len();
    while let Some(offset) = window[..from].rfind("direction") {
        let after = &text[offset + "direction".len()..];
        let whitespace = match_whitespace(after).unwrap_or(0);
        if whitespace > 0
            && let Some(rest) = after[whitespace..].strip_prefix(word)
        {
            let end = offset + "direction".len() + whitespace + word.len();
            let tail = rest.find('\n').unwrap_or(rest.len());
            return Some(end + tail);
        }
        from = offset;
    }
    None
}

/// `"title"\s[^#\n;]+` and `"accDescription"\s[^#\n;]+`.
fn match_word_and_rest(text: &str, word: &str) -> Option<usize> {
    let rest = text.strip_prefix(word)?;
    let first = rest.chars().next()?;
    if !is_js_whitespace(first) {
        return None;
    }
    let body = &rest[first.len_utf8()..];
    let mut length = word.len() + first.len_utf8();
    for (offset, ch) in body.char_indices() {
        if matches!(ch, '#' | '\n' | ';') {
            break;
        }
        length = word.len() + first.len_utf8() + offset + ch.len_utf8();
    }
    (length > word.len() + first.len_utf8()).then_some(length)
}

/// `accTitle\s*":"\s*` and `accDescr\s*":"\s*`.
fn match_prefixed_with_colon(text: &str, word: &str) -> Option<usize> {
    let rest = text.strip_prefix(word)?;
    let whitespace = match_whitespace(rest).unwrap_or(0);
    let rest = rest[whitespace..].strip_prefix(':')?;
    Some(word.len() + whitespace + 1 + match_whitespace(rest).unwrap_or(0))
}

/// `accDescr\s*"{"\s*`.
fn match_prefixed_with_brace(text: &str, word: &str) -> Option<usize> {
    let rest = text.strip_prefix(word)?;
    let whitespace = match_whitespace(rest).unwrap_or(0);
    let rest = rest[whitespace..].strip_prefix('{')?;
    Some(word.len() + whitespace + 1 + match_whitespace(rest).unwrap_or(0))
}

/// `\%\%(?!\{)*[^\n]*(\r?\n?)+`: a comment runs to the end of its line and takes the line
/// break with it (the lookahead has no effect, exactly as in JavaScript).
fn match_comment(text: &str) -> usize {
    let mut end = text.find('\n').unwrap_or(text.len());
    if end < text.len() {
        end += 1;
        let bytes = text.as_bytes();
        while matches!(bytes.get(end), Some(b'\r' | b'\n')) {
            end += 1;
        }
    }
    end
}

/// `\s*(\r?\n)+`: the match ends after the last line feed of the whitespace run.
fn match_newline(text: &str) -> Option<usize> {
    let mut run_end = 0;
    for (offset, ch) in text.char_indices() {
        if is_js_whitespace(ch) {
            run_end = offset + ch.len_utf8();
        } else {
            break;
        }
    }
    let last_newline = text[..run_end].rfind('\n')?;
    Some(last_newline + 1)
}

/// `\s+`.
fn match_whitespace(text: &str) -> Option<usize> {
    let mut end = 0;
    for (offset, ch) in text.char_indices() {
        if is_js_whitespace(ch) {
            end = offset + ch.len_utf8();
        } else {
            break;
        }
    }
    (end > 0).then_some(end)
}

/// `[(][ ]*[,]`.
fn match_open_then_comma(text: &str) -> Option<usize> {
    let rest = text.strip_prefix('(')?;
    let spaces = rest.len() - rest.trim_start_matches(' ').len();
    rest[spaces..].strip_prefix(',')?;
    Some(1 + spaces + 1)
}

/// `[ ]*["]["]`.
fn match_spaces_quotes_quotes(text: &str) -> Option<usize> {
    let spaces = text.len() - text.trim_start_matches(' ').len();
    text[spaces..].strip_prefix("\"\"")?;
    Some(spaces + 2)
}

/// `[ ]*"` and `[ ]*\$`: the leading spaces and the one literal character.
fn match_spaces_then(text: &str, character: char) -> Option<usize> {
    let spaces = text.len() - text.trim_start_matches(' ').len();
    text[spaces..].strip_prefix(character)?;
    Some(spaces + character.len_utf8())
}

/// `[^,]+`.
fn match_not_comma(text: &str) -> Option<usize> {
    let length = text.find(',').unwrap_or(text.len());
    (length > 0).then_some(length)
}

/// `[=][ ]*["]`.
fn match_equals_then_quote(text: &str) -> Option<usize> {
    let rest = text.strip_prefix('=')?;
    let spaces = rest.len() - rest.trim_start_matches(' ').len();
    rest[spaces..].strip_prefix('"')?;
    Some(1 + spaces + 1)
}

/// `[^"]+`.
fn match_not_quote(text: &str) -> Option<usize> {
    let length = text.find('"').unwrap_or(text.len());
    (length > 0).then_some(length)
}
