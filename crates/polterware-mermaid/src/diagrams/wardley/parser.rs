//! The wardley grammar: a hand-written recursive-descent port of `wardley.langium` with the
//! `TitleAndAccessibilities` fragment and the `EOL` rule of `common.langium`.
//!
//! ```text
//! Wardley:
//!   NEWLINE* KW_WARDLEY ( NEWLINE | TitleAndAccessibilities | Statement )*
//! Statement:
//!   Size | Evolution | Anchor | Component | Link | Evolve | Pipeline | Note
//!   | Annotations | Annotation | Accelerator | Deaccelerator
//! Size:
//!   "size" "[" width=INT "," height=INT "]" EOL
//! Evolution:
//!   "evolution" stages+=EvolutionStage (ARROW stages+=EvolutionStage)+ EOL
//! EvolutionStage:
//!   name=(ID | STRING | NAME_WITH_SPACES) ("@" boundary=WARDLEY_NUMBER)?
//!   ("/" secondName=(ID | STRING | NAME_WITH_SPACES))?
//! Anchor:
//!   "anchor" name="[" visibility=WARDLEY_NUMBER "," evolution=WARDLEY_NUMBER "]" EOL
//! Component:
//!   "component" name "[" visibility=WARDLEY_NUMBER "," evolution=WARDLEY_NUMBER "]"
//!   label=Label? decorator=Decorator? ("inertia" | "(" "inertia" ")")? EOL
//! Label:
//!   "label" "[" negX?='-'? offsetX=INT "," negY?='-'? offsetY=INT "]"
//! Decorator:
//!   "(" strategy=STRATEGY ")"
//! Link:
//!   from=(STRING | ID | NAME_WITH_SPACES) fromPort=LINK_PORT?
//!   arrow=(LINK_ARROW | ARROW)? to=(STRING | ID | NAME_WITH_SPACES) toPort=LINK_PORT?
//!   linkLabel=LINK_LABEL? EOL
//! Evolve:
//!   "evolve" component=(STRING | ID | NAME_WITH_SPACES) target=WARDLEY_NUMBER EOL
//! Pipeline:
//!   "pipeline" parent=(STRING | ID | NAME_WITH_SPACES) "{" NEWLINE+
//!   components+=PipelineComponent+ "}" EOL
//! PipelineComponent:
//!   "component" name=(STRING | ID | NAME_WITH_SPACES) "[" evolution=WARDLEY_NUMBER "]"
//!   label=Label? EOL
//! Note:
//!   "note" text=STRING "[" visibility=WARDLEY_NUMBER "," evolution=WARDLEY_NUMBER "]" EOL
//! Annotations:
//!   "annotations" "[" x=CoordinateValue "," y=CoordinateValue "]" EOL
//! Annotation:
//!   "annotation" number=INT "," "[" x=CoordinateValue "," y=CoordinateValue "]"
//!   text=STRING EOL
//! CoordinateValue:
//!   WARDLEY_NUMBER | INT
//! Accelerator:
//!   "accelerator" name="[" x=WARDLEY_NUMBER "," y=WARDLEY_NUMBER "]" EOL
//! Deaccelerator:
//!   "deaccelerator" name="[" x=WARDLEY_NUMBER "," y=WARDLEY_NUMBER "]" EOL
//! EOL:
//!   NEWLINE+ | EOF
//! TitleAndAccessibilities:
//!   ((accDescr=ACC_DESCR | accTitle=ACC_TITLE | title=TITLE) EOL)+
//! ```
//!
//! `size` and `evolution` are single-valued assignments, so a later line overwrites an earlier
//! one; every other statement appends to a list. The terminal values go through the
//! `common/valueConverter.ts` conversions: `STRING` unquotes and reads escapes, `INT` and
//! `WARDLEY_NUMBER` become `Number(input)`, `TITLE`, `ACC_TITLE`, and `ACC_DESCR` are trimmed
//! by the common regexes, and `LINK_LABEL` loses its leading `;` and is trimmed
//! (`wardley/valueConverter.ts`).
//!
//! Two decisions need a second token of lookahead: after a component's coordinates, `(` can
//! open a `Decorator` (`(build)`) or the `inertia` group (`(inertia)`), and the token after
//! the parenthesis tells them apart, as Langium's k-lookahead does.

use super::ast::SourceStrategy;
use super::lexer::{Lexer, Token, TokenKind};
use crate::error::{ParseError, Span};
use crate::preprocess::is_js_whitespace;

/// Parses the diagram body into the AST upstream's parser produces.
pub(crate) fn parse(body: &str) -> Result<ParsedWardley, ParseError> {
    Parser::new(body).parse_wardley()
}

/// A `Number` terminal or rule: its value and the token it came from.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Number {
    pub(crate) value: f64,
    pub(crate) span: Span,
}

#[derive(Clone, Debug)]
pub(crate) struct ParsedStage {
    pub(crate) name: String,
    pub(crate) boundary: Option<f64>,
    pub(crate) second_name: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct ParsedEvolution {
    pub(crate) stages: Vec<ParsedStage>,
}

#[derive(Clone, Debug)]
pub(crate) struct ParsedLabel {
    pub(crate) neg_x: bool,
    pub(crate) offset_x: f64,
    pub(crate) neg_y: bool,
    pub(crate) offset_y: f64,
}

#[derive(Clone, Debug)]
pub(crate) struct ParsedAnchor {
    pub(crate) name: String,
    pub(crate) visibility: Number,
    pub(crate) evolution: Number,
}

#[derive(Clone, Debug)]
pub(crate) struct ParsedComponent {
    pub(crate) name: String,
    pub(crate) visibility: Number,
    pub(crate) evolution: Number,
    pub(crate) label: Option<ParsedLabel>,
    pub(crate) strategy: Option<SourceStrategy>,
    pub(crate) inertia: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct ParsedLink {
    pub(crate) from: String,
    pub(crate) from_port: Option<String>,
    pub(crate) arrow: Option<String>,
    pub(crate) to: String,
    pub(crate) to_port: Option<String>,
    pub(crate) link_label: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct ParsedEvolve {
    pub(crate) component: String,
    pub(crate) target: Number,
}

#[derive(Clone, Debug)]
pub(crate) struct ParsedPipelineComponent {
    pub(crate) name: String,
    pub(crate) evolution: Number,
    pub(crate) label: Option<ParsedLabel>,
}

#[derive(Clone, Debug)]
pub(crate) struct ParsedPipeline {
    pub(crate) parent: String,
    pub(crate) parent_span: Span,
    pub(crate) components: Vec<ParsedPipelineComponent>,
}

#[derive(Clone, Debug)]
pub(crate) struct ParsedNote {
    pub(crate) text: String,
    pub(crate) visibility: Number,
    pub(crate) evolution: Number,
}

#[derive(Clone, Debug)]
pub(crate) struct ParsedAnnotationsBox {
    pub(crate) x: Number,
    pub(crate) y: Number,
}

#[derive(Clone, Debug)]
pub(crate) struct ParsedAnnotation {
    pub(crate) number: f64,
    pub(crate) x: Number,
    pub(crate) y: Number,
    pub(crate) text: String,
}

#[derive(Clone, Debug)]
pub(crate) struct ParsedAccelerator {
    pub(crate) name: String,
    pub(crate) x: Number,
    pub(crate) y: Number,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ParsedSize {
    pub(crate) width: f64,
    pub(crate) height: f64,
}

/// The document as the grammar reads it, before `populateDb` converts coordinates and
/// writes the database.
#[derive(Clone, Debug, Default)]
pub(crate) struct ParsedWardley {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_description: Option<String>,
    pub(crate) size: Option<ParsedSize>,
    pub(crate) evolution: Option<ParsedEvolution>,
    pub(crate) anchors: Vec<ParsedAnchor>,
    pub(crate) components: Vec<ParsedComponent>,
    pub(crate) links: Vec<ParsedLink>,
    pub(crate) evolves: Vec<ParsedEvolve>,
    pub(crate) pipelines: Vec<ParsedPipeline>,
    pub(crate) notes: Vec<ParsedNote>,
    pub(crate) annotations_boxes: Vec<ParsedAnnotationsBox>,
    pub(crate) annotations: Vec<ParsedAnnotation>,
    pub(crate) accelerators: Vec<ParsedAccelerator>,
    pub(crate) deaccelerators: Vec<ParsedAccelerator>,
}

struct Parser<'a> {
    lexer: Lexer<'a>,
    lookahead: Vec<Token>,
}

impl<'a> Parser<'a> {
    fn new(body: &'a str) -> Self {
        Self {
            lexer: Lexer::new(body),
            lookahead: Vec::new(),
        }
    }

    fn fill(&mut self, ahead: usize) -> Result<TokenKind, ParseError> {
        while self.lookahead.len() <= ahead {
            let token = self.lexer.next_token()?;
            self.lookahead.push(token);
        }
        Ok(self.lookahead[ahead].kind)
    }

    fn peek(&mut self) -> Result<TokenKind, ParseError> {
        self.fill(0)
    }

    fn peek_ahead(&mut self, ahead: usize) -> Result<TokenKind, ParseError> {
        self.fill(ahead)
    }

    fn bump(&mut self) -> Result<Token, ParseError> {
        self.fill(0)?;
        Ok(self.lookahead.remove(0))
    }

    fn expect(&mut self, expected: TokenKind) -> Result<Token, ParseError> {
        if self.peek()? != expected {
            return Err(self.unexpected(expected.name()));
        }
        self.bump()
    }

    fn unexpected(&mut self, expected: &str) -> ParseError {
        match self.fill(0) {
            Ok(token) => ParseError::at(
                format!("expected {expected}, got '{}'", token.name()),
                self.lookahead[0].span,
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

    fn parse_wardley(mut self) -> Result<ParsedWardley, ParseError> {
        while self.peek()? == TokenKind::Newline {
            self.bump()?;
        }
        self.expect(TokenKind::KwWardley)?;
        let mut parsed = ParsedWardley::default();
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
                TokenKind::KwSize => parsed.size = Some(self.parse_size()?),
                TokenKind::KwEvolution => parsed.evolution = Some(self.parse_evolution()?),
                TokenKind::KwAnchor => parsed.anchors.push(self.parse_anchor()?),
                TokenKind::KwComponent => parsed.components.push(self.parse_component()?),
                TokenKind::KwEvolve => parsed.evolves.push(self.parse_evolve()?),
                TokenKind::KwPipeline => parsed.pipelines.push(self.parse_pipeline()?),
                TokenKind::KwNote => parsed.notes.push(self.parse_note()?),
                TokenKind::KwAnnotations => {
                    parsed.annotations_boxes.push(self.parse_annotations_box()?);
                }
                TokenKind::KwAnnotation => {
                    parsed.annotations.push(self.parse_annotation()?);
                }
                TokenKind::KwAccelerator => parsed
                    .accelerators
                    .push(self.parse_accelerator(TokenKind::KwAccelerator)?),
                TokenKind::KwDeaccelerator => parsed
                    .deaccelerators
                    .push(self.parse_accelerator(TokenKind::KwDeaccelerator)?),
                TokenKind::Str | TokenKind::Id | TokenKind::NameWithSpaces => {
                    parsed.links.push(self.parse_link()?);
                }
                _ => return Err(self.unexpected("a wardley statement")),
            }
        }
        Ok(parsed)
    }

    fn parse_size(&mut self) -> Result<ParsedSize, ParseError> {
        self.expect(TokenKind::KwSize)?;
        self.expect(TokenKind::LBracket)?;
        let width = self.parse_int()?.value;
        self.expect(TokenKind::Comma)?;
        let height = self.parse_int()?.value;
        self.expect(TokenKind::RBracket)?;
        self.expect_eol()?;
        Ok(ParsedSize { width, height })
    }

    fn parse_evolution(&mut self) -> Result<ParsedEvolution, ParseError> {
        self.expect(TokenKind::KwEvolution)?;
        let mut stages = vec![self.parse_stage()?];
        if self.peek()? != TokenKind::Arrow {
            return Err(self.unexpected(TokenKind::Arrow.name()));
        }
        while self.peek()? == TokenKind::Arrow {
            self.bump()?;
            stages.push(self.parse_stage()?);
        }
        self.expect_eol()?;
        Ok(ParsedEvolution { stages })
    }

    fn parse_stage(&mut self) -> Result<ParsedStage, ParseError> {
        let name = self.parse_name()?;
        let boundary = if self.peek()? == TokenKind::At {
            self.bump()?;
            Some(self.parse_wardley_number()?.value)
        } else {
            None
        };
        let second_name = if self.peek()? == TokenKind::Slash {
            self.bump()?;
            Some(self.parse_name()?)
        } else {
            None
        };
        Ok(ParsedStage {
            name,
            boundary,
            second_name,
        })
    }

    fn parse_anchor(&mut self) -> Result<ParsedAnchor, ParseError> {
        self.expect(TokenKind::KwAnchor)?;
        let name = self.parse_name()?;
        self.expect(TokenKind::LBracket)?;
        let visibility = self.parse_wardley_number()?;
        self.expect(TokenKind::Comma)?;
        let evolution = self.parse_wardley_number()?;
        self.expect(TokenKind::RBracket)?;
        self.expect_eol()?;
        Ok(ParsedAnchor {
            name,
            visibility,
            evolution,
        })
    }

    fn parse_component(&mut self) -> Result<ParsedComponent, ParseError> {
        self.expect(TokenKind::KwComponent)?;
        let name = self.parse_name()?;
        self.expect(TokenKind::LBracket)?;
        let visibility = self.parse_wardley_number()?;
        self.expect(TokenKind::Comma)?;
        let evolution = self.parse_wardley_number()?;
        self.expect(TokenKind::RBracket)?;
        let label = if self.peek()? == TokenKind::KwLabel {
            Some(self.parse_label()?)
        } else {
            None
        };
        // `(` starts a decorator unless the token after it is `inertia`, in which case the
        // optional group around the parenthesis belongs to the inertia rule.
        let decorator =
            if self.peek()? == TokenKind::LParen && self.peek_ahead(1)? != TokenKind::KwInertia {
                Some(self.parse_decorator()?)
            } else {
                None
            };
        let inertia = match self.peek()? {
            TokenKind::KwInertia => {
                self.bump()?;
                true
            }
            TokenKind::LParen => {
                self.bump()?;
                self.expect(TokenKind::KwInertia)?;
                self.expect(TokenKind::RParen)?;
                true
            }
            _ => false,
        };
        self.expect_eol()?;
        Ok(ParsedComponent {
            name,
            visibility,
            evolution,
            label,
            strategy: decorator,
            inertia,
        })
    }

    fn parse_label(&mut self) -> Result<ParsedLabel, ParseError> {
        self.expect(TokenKind::KwLabel)?;
        self.expect(TokenKind::LBracket)?;
        let neg_x = self.peek()? == TokenKind::Minus;
        if neg_x {
            self.bump()?;
        }
        let offset_x = self.parse_int()?.value;
        self.expect(TokenKind::Comma)?;
        let neg_y = self.peek()? == TokenKind::Minus;
        if neg_y {
            self.bump()?;
        }
        let offset_y = self.parse_int()?.value;
        self.expect(TokenKind::RBracket)?;
        Ok(ParsedLabel {
            neg_x,
            offset_x,
            neg_y,
            offset_y,
        })
    }

    fn parse_decorator(&mut self) -> Result<SourceStrategy, ParseError> {
        self.expect(TokenKind::LParen)?;
        let token = self.expect(TokenKind::Strategy)?;
        let strategy = match token.text.as_str() {
            "build" => SourceStrategy::Build,
            "buy" => SourceStrategy::Buy,
            "outsource" => SourceStrategy::Outsource,
            "market" => SourceStrategy::Market,
            other => {
                return Err(ParseError::at(
                    format!("unknown strategy '{other}'"),
                    token.span,
                ));
            }
        };
        self.expect(TokenKind::RParen)?;
        Ok(strategy)
    }

    fn parse_link(&mut self) -> Result<ParsedLink, ParseError> {
        let from = self.parse_name()?;
        let from_port = if self.peek()? == TokenKind::LinkPort {
            Some(self.bump()?.text)
        } else {
            None
        };
        let arrow = match self.peek()? {
            TokenKind::LinkArrow | TokenKind::Arrow => Some(self.bump()?.text),
            _ => None,
        };
        let to = self.parse_name()?;
        let to_port = if self.peek()? == TokenKind::LinkPort {
            Some(self.bump()?.text)
        } else {
            None
        };
        let link_label = if self.peek()? == TokenKind::LinkLabel {
            Some(link_label_value(&self.bump()?.text))
        } else {
            None
        };
        self.expect_eol()?;
        Ok(ParsedLink {
            from,
            from_port,
            arrow,
            to,
            to_port,
            link_label,
        })
    }

    fn parse_evolve(&mut self) -> Result<ParsedEvolve, ParseError> {
        self.expect(TokenKind::KwEvolve)?;
        let component = self.parse_name()?;
        let target = self.parse_wardley_number()?;
        self.expect_eol()?;
        Ok(ParsedEvolve { component, target })
    }

    fn parse_pipeline(&mut self) -> Result<ParsedPipeline, ParseError> {
        self.expect(TokenKind::KwPipeline)?;
        let (parent, parent_span) = self.parse_name_token()?;
        self.expect(TokenKind::LBrace)?;
        if self.peek()? != TokenKind::Newline {
            return Err(self.unexpected(TokenKind::Newline.name()));
        }
        while self.peek()? == TokenKind::Newline {
            self.bump()?;
        }
        let mut components = Vec::new();
        while self.peek()? == TokenKind::KwComponent {
            components.push(self.parse_pipeline_component()?);
        }
        if components.is_empty() {
            return Err(self.unexpected(TokenKind::KwComponent.name()));
        }
        self.expect(TokenKind::RBrace)?;
        self.expect_eol()?;
        Ok(ParsedPipeline {
            parent,
            parent_span,
            components,
        })
    }

    fn parse_pipeline_component(&mut self) -> Result<ParsedPipelineComponent, ParseError> {
        self.expect(TokenKind::KwComponent)?;
        let name = self.parse_name()?;
        self.expect(TokenKind::LBracket)?;
        let evolution = self.parse_wardley_number()?;
        self.expect(TokenKind::RBracket)?;
        let label = if self.peek()? == TokenKind::KwLabel {
            Some(self.parse_label()?)
        } else {
            None
        };
        self.expect_eol()?;
        Ok(ParsedPipelineComponent {
            name,
            evolution,
            label,
        })
    }

    fn parse_note(&mut self) -> Result<ParsedNote, ParseError> {
        self.expect(TokenKind::KwNote)?;
        let text = self.parse_string()?;
        self.expect(TokenKind::LBracket)?;
        let visibility = self.parse_wardley_number()?;
        self.expect(TokenKind::Comma)?;
        let evolution = self.parse_wardley_number()?;
        self.expect(TokenKind::RBracket)?;
        self.expect_eol()?;
        Ok(ParsedNote {
            text,
            visibility,
            evolution,
        })
    }

    fn parse_annotations_box(&mut self) -> Result<ParsedAnnotationsBox, ParseError> {
        self.expect(TokenKind::KwAnnotations)?;
        self.expect(TokenKind::LBracket)?;
        let x = self.parse_coordinate()?;
        self.expect(TokenKind::Comma)?;
        let y = self.parse_coordinate()?;
        self.expect(TokenKind::RBracket)?;
        self.expect_eol()?;
        Ok(ParsedAnnotationsBox { x, y })
    }

    fn parse_annotation(&mut self) -> Result<ParsedAnnotation, ParseError> {
        self.expect(TokenKind::KwAnnotation)?;
        let number = self.parse_int()?.value;
        self.expect(TokenKind::Comma)?;
        self.expect(TokenKind::LBracket)?;
        let x = self.parse_coordinate()?;
        self.expect(TokenKind::Comma)?;
        let y = self.parse_coordinate()?;
        self.expect(TokenKind::RBracket)?;
        let text = self.parse_string()?;
        self.expect_eol()?;
        Ok(ParsedAnnotation { number, x, y, text })
    }

    fn parse_accelerator(&mut self, keyword: TokenKind) -> Result<ParsedAccelerator, ParseError> {
        self.expect(keyword)?;
        let name = self.parse_name()?;
        self.expect(TokenKind::LBracket)?;
        let x = self.parse_wardley_number()?;
        self.expect(TokenKind::Comma)?;
        let y = self.parse_wardley_number()?;
        self.expect(TokenKind::RBracket)?;
        self.expect_eol()?;
        Ok(ParsedAccelerator { name, x, y })
    }

    fn parse_name(&mut self) -> Result<String, ParseError> {
        Ok(self.parse_name_token()?.0)
    }

    fn parse_name_token(&mut self) -> Result<(String, Span), ParseError> {
        let token = match self.peek()? {
            TokenKind::Str => {
                let token = self.bump()?;
                return Ok((convert_string(&token.text), token.span));
            }
            TokenKind::Id | TokenKind::NameWithSpaces => self.bump()?,
            _ => return Err(self.unexpected("a name")),
        };
        Ok((token.text, token.span))
    }

    fn parse_string(&mut self) -> Result<String, ParseError> {
        let token = self.expect(TokenKind::Str)?;
        Ok(convert_string(&token.text))
    }

    fn parse_wardley_number(&mut self) -> Result<Number, ParseError> {
        let token = self.expect(TokenKind::WardleyNumber)?;
        Ok(Number {
            value: token.text.parse().unwrap_or(f64::NAN),
            span: token.span,
        })
    }

    fn parse_int(&mut self) -> Result<Number, ParseError> {
        let token = self.expect(TokenKind::Int)?;
        Ok(Number {
            value: token.text.parse().unwrap_or(f64::NAN),
            span: token.span,
        })
    }

    fn parse_coordinate(&mut self) -> Result<Number, ParseError> {
        match self.peek()? {
            TokenKind::WardleyNumber => self.parse_wardley_number(),
            TokenKind::Int => self.parse_int(),
            _ => Err(self.unexpected("a coordinate")),
        }
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

/// `wardley/valueConverter.ts`'s `LINK_LABEL`: drop the leading `;` and trim.
fn link_label_value(raw: &str) -> String {
    trim(&raw[1..]).to_string()
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
    let Some(value) = rest.trim_start_matches(is_js_whitespace).strip_prefix('{') else {
        return String::new();
    };
    let capture = &value[..value.find('}').unwrap_or(value.len())];
    normalize_multiline(capture)
}

/// The multiline branch of the accessible description conversion: strip whitespace at both
/// ends of every line, collapse runs of two or more spaces or tabs, collapse line breaks, and
/// collapse consecutive blank lines.
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
    text.trim_matches(is_js_whitespace)
}

fn trim_horizontal(text: &str) -> &str {
    text.trim_start_matches([' ', '\t'])
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
