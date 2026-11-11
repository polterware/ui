//! The parser: the grammar rules of upstream's
//! `packages/mermaid/src/diagrams/class/parser/classDiagram.jison`, written as recursive
//! descent over the lexer's tokens, plus the database semantics of `classDb.ts` as the
//! methods the rules call on [`Db`].
//!
//! The grammar's one non-obvious shape is the alternating `statement NEWLINE statement`
//! lists: a blank line collapses into a single `NEWLINE` token because the lexer rule is
//! `\s*(\r?\n)+`, and a comment swallows the newlines that follow it, so the lists never
//! need to skip empty statements.

use super::ast::*;
use super::lexer::{self, Kind, Token};
use crate::common::sanitize::collapse_description_indent;
use crate::error::ParseError;

pub(super) fn parse(body: &str) -> Result<Ast, ParseError> {
    let tokens = lexer::lex(body)?;
    let mut parser = Parser {
        tokens,
        pos: 0,
        db: Db::default(),
    };
    parser.document()?;
    Ok(parser.db.ast)
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

    /// `graphConfig: CLASS_DIAGRAM NEWLINE statements EOF`, and `start: statements` for a
    /// body that arrives without its header line.
    fn document(&mut self) -> Result<(), ParseError> {
        if self.at(Kind::ClassDiagram) {
            self.bump();
            self.expect(Kind::Newline, "'NEWLINE'")?;
        }
        if self.at(Kind::Eof) || self.at(Kind::EofSignal) {
            return Err(self.unexpected("a statement"));
        }
        while !self.at(Kind::Eof) {
            if self.at(Kind::EofSignal) {
                return Err(self.unexpected("'EOF'"));
            }
            self.statement()?;
            if self.at(Kind::Newline) {
                self.bump();
            } else if self.at(Kind::Eof) {
                break;
            } else {
                return Err(self.unexpected("'NEWLINE'"));
            }
        }
        Ok(())
    }

    fn statement(&mut self) -> Result<(), ParseError> {
        match self.kind() {
            Kind::Class => {
                self.class_statement()?;
            }
            Kind::Namespace => self.namespace_statement()?,
            Kind::Note | Kind::NoteFor => {
                self.note_statement()?;
            }
            Kind::CssClass => self.css_class_statement()?,
            Kind::ClassDef => self.class_def_statement()?,
            Kind::Style => self.style_statement()?,
            Kind::Click | Kind::Callback | Kind::Link => self.click_statement()?,
            Kind::AnnotationStart => self.annotation_statement()?,
            Kind::DirectionTb | Kind::DirectionBt | Kind::DirectionRl | Kind::DirectionLr => {
                self.db.ast.direction = match self.bump().kind {
                    Kind::DirectionBt => Direction::Bt,
                    Kind::DirectionRl => Direction::Rl,
                    Kind::DirectionLr => Direction::Lr,
                    _ => Direction::Tb,
                };
            }
            Kind::AccTitle => {
                self.bump();
                let value = self.expect(Kind::AccTitleValue, "'acc_title_value'")?;
                self.db.ast.acc_title = Some(value.text.trim().to_string());
            }
            Kind::AccDescr => {
                self.bump();
                let value = self.expect(Kind::AccDescrValue, "'acc_descr_value'")?;
                self.set_acc_description(&value.text);
            }
            Kind::AccDescrMultilineValue => {
                let value = self.bump();
                self.set_acc_description(&value.text);
            }
            Kind::Alpha | Kind::UnicodeText | Kind::BqString | Kind::Minus => {
                self.member_statement()?;
            }
            _ => return Err(self.unexpected("a statement")),
        }
        Ok(())
    }

    /// `memberStatement` and `relationStatement`: both start with a class name.
    fn member_statement(&mut self) -> Result<(), ParseError> {
        let id = self.class_name()?;
        match self.kind() {
            Kind::Str
            | Kind::Extension
            | Kind::Dependency
            | Kind::Composition
            | Kind::Aggregation
            | Kind::Lollipop
            | Kind::Line
            | Kind::DottedLine => self.relation_statement(id),
            Kind::Label => {
                let label = self.bump().text;
                self.db.add_member(&id, &cleanup_label(&label));
                Ok(())
            }
            Kind::Newline | Kind::Eof => Ok(()),
            _ => Err(self.unexpected("'NEWLINE'")),
        }
    }

    /// `relationStatement`: `className relation className`, with a cardinality `STR` on
    /// either side and an optional trailing `LABEL`.
    fn relation_statement(&mut self, id1: String) -> Result<(), ParseError> {
        let relation_title1 = if self.at(Kind::Str) {
            self.bump().text
        } else {
            "none".to_string()
        };
        let relation = self.relation()?;
        let relation_title2 = if self.at(Kind::Str) {
            self.bump().text
        } else {
            "none".to_string()
        };
        let id2 = self.class_name()?;
        let title = if self.at(Kind::Label) {
            Some(cleanup_label(&self.bump().text))
        } else {
            None
        };
        self.db.add_relation(ClassRelation {
            id1,
            id2,
            relation_title1,
            relation_title2,
            title,
            style: Vec::new(),
            relation,
        });
        Ok(())
    }

    /// `relation: relationType lineType relationType | lineType relationType |
    /// relationType lineType | lineType`.
    fn relation(&mut self) -> Result<Relation, ParseError> {
        if let Some(line_type) = line_type_of(self.kind()) {
            self.bump();
            let type2 = self.take_endpoint();
            return Ok(Relation {
                type1: Endpoint::None,
                type2,
                line_type,
            });
        }

        let type1 = endpoint_of(self.kind()).ok_or_else(|| self.unexpected("a relation"))?;
        self.bump();
        let line_type = line_type_of(self.kind()).ok_or_else(|| self.unexpected("a line type"))?;
        self.bump();
        let type2 = self.take_endpoint();
        Ok(Relation {
            type1,
            type2,
            line_type,
        })
    }

    fn take_endpoint(&mut self) -> Endpoint {
        match endpoint_of(self.kind()) {
            Some(endpoint) => {
                self.bump();
                endpoint
            }
            None => Endpoint::None,
        }
    }

    /// `classStatement`: a class definition, its label, annotations, css class, and body.
    fn class_statement(&mut self) -> Result<String, ParseError> {
        self.expect(Kind::Class, "'CLASS'")?;
        let id = self.class_name()?;
        self.db.add_class(&id);

        if self.at(Kind::Sqs) {
            self.bump();
            let label = self.expect(Kind::Str, "'STR'")?.text;
            self.expect(Kind::Sqe, "'SQE'")?;
            self.db.set_class_label(&id, &label);
        }

        if self.at(Kind::AnnotationStart) {
            self.bump();
            let annotation = self.alpha_num_token()?;
            self.expect(Kind::AnnotationEnd, "'ANNOTATION_END'")?;
            self.db
                .add_annotation(&id, &annotation)
                .map_err(|message| ParseError::at(message, self.peek().span))?;
            if self.at(Kind::StructStart) {
                self.bump();
                self.class_body(&id, false)?;
            }
        } else if self.at(Kind::StyleSeparator) {
            self.bump();
            let css_class = self.alpha_num_token()?;
            self.db.set_css_class(&id, &css_class);
            if self.at(Kind::StructStart) {
                self.bump();
                self.class_body(&id, true)?;
            }
        } else if self.at(Kind::StructStart) {
            self.bump();
            self.class_body(&id, false)?;
        }

        Ok(id)
    }

    /// `members: MEMBER | MEMBER members`, closed by `STRUCT_STOP`.
    ///
    /// `required` marks the `classIdentifier STYLE_SEPARATOR alphaNumToken STRUCT_START
    /// members STRUCT_STOP` form: unlike a plain class body or an annotated one, the
    /// grammar has no empty variant for it.
    fn class_body(&mut self, id: &str, required: bool) -> Result<(), ParseError> {
        let mut members = Vec::new();
        let stop_span;
        loop {
            match self.kind() {
                Kind::StructStop => {
                    stop_span = self.peek().span;
                    self.bump();
                    break;
                }
                Kind::Member => members.push(self.bump().text),
                Kind::EofInStruct => return Err(self.unexpected("'STRUCT_STOP'")),
                _ => return Err(self.unexpected("'MEMBER'")),
            }
        }
        if members.is_empty() {
            if required {
                return Err(ParseError::at("Expecting 'MEMBER'", stop_span));
            }
            return Ok(());
        }
        self.db.add_members(id, members);
        Ok(())
    }

    /// `namespaceStatement`: a namespace name, an optional label, and the class statements
    /// it holds.
    fn namespace_statement(&mut self) -> Result<(), ParseError> {
        self.expect(Kind::Namespace, "'NAMESPACE'")?;
        let name = self.namespace_name()?;
        let label = if self.at(Kind::Sqs) {
            self.bump();
            let label = self.expect(Kind::Str, "'STR'")?.text;
            self.expect(Kind::Sqe, "'SQE'")?;
            Some(label)
        } else {
            None
        };
        let id = self.db.add_namespace(&name, label.as_deref());

        self.expect(Kind::StructStart, "'STRUCT_START'")?;
        if self.at(Kind::Newline) {
            self.bump();
        }
        let (classes, notes) = self.class_statements()?;
        self.expect(Kind::StructStop, "'STRUCT_STOP'")?;
        self.db.add_classes_to_namespace(&id, &classes, &notes);
        self.db.pop_namespace();
        Ok(())
    }

    /// `classStatements`: class statements, notes, and nested namespaces, answering the
    /// class ids and note ids to add to the namespace.
    fn class_statements(&mut self) -> Result<(Vec<String>, Vec<String>), ParseError> {
        let mut classes = Vec::new();
        let mut notes = Vec::new();
        loop {
            match self.kind() {
                Kind::Class => classes.push(self.class_statement()?),
                Kind::Note | Kind::NoteFor => notes.push(self.note_statement()?),
                Kind::Namespace => self.namespace_statement()?,
                Kind::StructStop => break,
                Kind::EofInStruct => return Err(self.unexpected("'STRUCT_STOP'")),
                _ => return Err(self.unexpected("a class statement")),
            }
            if self.at(Kind::Newline) {
                self.bump();
            }
        }
        Ok((classes, notes))
    }

    /// `noteStatement`: `note for <class> "text"` or `note "text"`.
    fn note_statement(&mut self) -> Result<String, ParseError> {
        if self.at(Kind::NoteFor) {
            self.bump();
            let class = self.class_name()?;
            let text = self.expect(Kind::Str, "'STR'")?.text;
            Ok(self.db.add_note(&text, Some(&class)))
        } else {
            self.expect(Kind::Note, "'NOTE'")?;
            let text = self.expect(Kind::Str, "'STR'")?.text;
            Ok(self.db.add_note(&text, None))
        }
    }

    /// `cssClassStatement: CSSCLASS STR ALPHA`.
    fn css_class_statement(&mut self) -> Result<(), ParseError> {
        self.expect(Kind::CssClass, "'CSSCLASS'")?;
        let ids = self.expect(Kind::Str, "'STR'")?.text;
        let css_class = self.expect(Kind::Alpha, "'ALPHA'")?.text;
        self.db.set_css_class(&ids, &css_class);
        Ok(())
    }

    /// `classDefStatement: CLASSDEF classList stylesOpt`.
    fn class_def_statement(&mut self) -> Result<(), ParseError> {
        self.expect(Kind::ClassDef, "'CLASSDEF'")?;
        let mut ids = vec![self.expect(Kind::Alpha, "'ALPHA'")?.text];
        while self.at(Kind::Comma) {
            self.bump();
            ids.push(self.expect(Kind::Alpha, "'ALPHA'")?.text);
        }
        let styles = self.styles_opt()?;
        self.db.define_class(&ids, &styles);
        Ok(())
    }

    /// `styleStatement: STYLE ALPHA stylesOpt`.
    fn style_statement(&mut self) -> Result<(), ParseError> {
        self.expect(Kind::Style, "'STYLE'")?;
        let id = self.expect(Kind::Alpha, "'ALPHA'")?.text;
        let styles = self.styles_opt()?;
        self.db.set_css_style(&id, &styles);
        Ok(())
    }

    /// `stylesOpt: style | stylesOpt COMMA style`.
    fn styles_opt(&mut self) -> Result<Vec<String>, ParseError> {
        let mut styles = vec![self.style()?];
        while self.at(Kind::Comma) {
            self.bump();
            styles.push(self.style()?);
        }
        Ok(styles)
    }

    /// `style: styleComponent | style styleComponent`, where a `styleComponent` is
    /// `ALPHA | NUM | COLON | UNIT | SPACE | BRKT | STYLE | PCT | LABEL`.
    fn style(&mut self) -> Result<String, ParseError> {
        let mut text = String::new();
        while matches!(
            self.kind(),
            Kind::Alpha | Kind::Colon | Kind::Brkt | Kind::Style | Kind::Pct | Kind::Label
        ) {
            text.push_str(&self.bump().text);
        }
        if text.is_empty() {
            return Err(self.unexpected("a style"));
        }
        Ok(text)
    }

    /// `clickStatement`: `callback`, `link`, and `click` with a `call` or `href` form.
    fn click_statement(&mut self) -> Result<(), ParseError> {
        match self.kind() {
            Kind::Callback => {
                self.bump();
                let id = self.class_name()?;
                self.expect(Kind::Str, "'STR'")?;
                self.db.set_click_event(&id);
                if self.at(Kind::Str) {
                    let tooltip = self.bump().text;
                    self.db.set_tooltip(&id, &tooltip);
                }
            }
            Kind::Link => {
                self.bump();
                let id = self.class_name()?;
                let link = self.expect(Kind::Str, "'STR'")?.text;
                let mut tooltip = None;
                if self.at(Kind::Str) {
                    tooltip = Some(self.bump().text);
                }
                let target = if self.at(Kind::LinkTarget) {
                    Some(self.bump().text)
                } else {
                    None
                };
                self.db.set_link(&id, &link, target.as_deref());
                if let Some(tooltip) = tooltip {
                    self.db.set_tooltip(&id, &tooltip);
                }
            }
            Kind::Click => {
                self.bump();
                let id = self.class_name()?;
                match self.kind() {
                    Kind::CallbackName => {
                        self.bump();
                        if self.at(Kind::CallbackArgs) {
                            self.bump();
                        }
                        self.db.set_click_event(&id);
                        if self.at(Kind::Str) {
                            let tooltip = self.bump().text;
                            self.db.set_tooltip(&id, &tooltip);
                        }
                    }
                    Kind::Href => {
                        self.bump();
                        let link = self.expect(Kind::Str, "'STR'")?.text;
                        let mut tooltip = None;
                        if self.at(Kind::Str) {
                            tooltip = Some(self.bump().text);
                        }
                        let target = if self.at(Kind::LinkTarget) {
                            Some(self.bump().text)
                        } else {
                            None
                        };
                        self.db.set_link(&id, &link, target.as_deref());
                        if let Some(tooltip) = tooltip {
                            self.db.set_tooltip(&id, &tooltip);
                        }
                    }
                    _ => return Err(self.unexpected("'CALLBACK_NAME' or 'HREF'")),
                }
            }
            _ => unreachable!(),
        }
        Ok(())
    }

    /// `annotationStatement: ANNOTATION_START alphaNumToken ANNOTATION_END className`.
    fn annotation_statement(&mut self) -> Result<(), ParseError> {
        self.expect(Kind::AnnotationStart, "'ANNOTATION_START'")?;
        let annotation = self.alpha_num_token()?;
        self.expect(Kind::AnnotationEnd, "'ANNOTATION_END'")?;
        let span = self.peek().span;
        let class = self.class_name()?;
        self.db
            .add_annotation(&class, &annotation)
            .map_err(|message| ParseError::at(message, span))
    }

    /// `className`: `alphaNumToken`, dotted names, backticked names, and one
    /// `GENERICTYPE` suffix. The rule's recursive `alphaNumToken className` alternative is
    /// a plain concatenation, so whitespace between the tokens disappears.
    fn class_name(&mut self) -> Result<String, ParseError> {
        let mut name = self.name_piece()?;
        loop {
            match self.kind() {
                Kind::Dot => {
                    self.bump();
                    name.push('.');
                    name.push_str(&self.name_piece()?);
                }
                Kind::Alpha | Kind::UnicodeText | Kind::Minus | Kind::BqString => {
                    name.push_str(&self.bump().text);
                }
                Kind::GenericType => {
                    let generic = self.bump().text;
                    name = format!("{name}~{generic}~");
                    break;
                }
                _ => break,
            }
        }
        Ok(name)
    }

    /// `namespaceName`: a class name without a generic type.
    fn namespace_name(&mut self) -> Result<String, ParseError> {
        let mut name = self.name_piece()?;
        loop {
            match self.kind() {
                Kind::Dot => {
                    self.bump();
                    name.push('.');
                    name.push_str(&self.name_piece()?);
                }
                Kind::Alpha | Kind::UnicodeText | Kind::Minus | Kind::BqString => {
                    name.push_str(&self.bump().text);
                }
                _ => break,
            }
        }
        Ok(name)
    }

    fn name_piece(&mut self) -> Result<String, ParseError> {
        match self.kind() {
            Kind::Alpha | Kind::UnicodeText | Kind::Minus | Kind::BqString => Ok(self.bump().text),
            _ => Err(self.unexpected("a class name")),
        }
    }

    /// `commonDb.setAccDescription` after the grammar's `trim`: the indentation that
    /// follows a newline is dropped.
    fn set_acc_description(&mut self, text: &str) {
        self.db.ast.acc_description =
            Some(collapse_description_indent(&sanitize_text(text.trim())));
    }

    /// `alphaNumToken: UNICODE_TEXT | NUM | ALPHA | MINUS`.
    fn alpha_num_token(&mut self) -> Result<String, ParseError> {
        match self.kind() {
            Kind::Alpha | Kind::UnicodeText | Kind::Minus => Ok(self.bump().text),
            _ => Err(self.unexpected("a name")),
        }
    }
}

fn line_type_of(kind: Kind) -> Option<LineType> {
    match kind {
        Kind::Line => Some(LineType::Line),
        Kind::DottedLine => Some(LineType::DottedLine),
        _ => None,
    }
}

fn endpoint_of(kind: Kind) -> Option<Endpoint> {
    match kind {
        Kind::Aggregation => Some(Endpoint::Aggregation),
        Kind::Extension => Some(Endpoint::Extension),
        Kind::Composition => Some(Endpoint::Composition),
        Kind::Dependency => Some(Endpoint::Dependency),
        Kind::Lollipop => Some(Endpoint::Lollipop),
        _ => None,
    }
}

/// `cleanupLabel`: drop the leading colon and trim.
fn cleanup_label(label: &str) -> String {
    let label = label.strip_prefix(':').unwrap_or(label);
    sanitize_text(label.trim())
}

// The database half: `classDb.ts`. The parser above calls these while it builds the
// diagram; the public getters live on `Ast` in `ast.rs`.

/// Upstream's `ClassDB`: the [`Ast`] being filled, plus the counters and namespace stack
/// the parser keeps while it works (and discards when it answers).
#[derive(Default)]
struct Db {
    ast: Ast,
    class_counter: usize,
    namespace_counter: usize,
    namespace_stack: Vec<String>,
}

impl Db {
    /// `addClass`: note the generic type is split out of the id and shown escaped in the
    /// class's `text`, while `setClassLabel` writes it unescaped.
    fn add_class(&mut self, id: &str) {
        let id = sanitize_text(id);
        let (class_name, type_name) = split_class_name_and_type(&id);
        if self.ast.classes.contains(&class_name) {
            return;
        }
        let name = sanitize_text(&class_name);
        let text = if type_name.is_empty() {
            name.clone()
        } else {
            format!("{name}&lt;{type_name}&gt;")
        };
        let dom_id = format!("classId-{name}-{}", self.class_counter);
        self.class_counter += 1;
        let node = ClassNode {
            id: name.clone(),
            type_name,
            label: name.clone(),
            text,
            css_classes: "default".to_string(),
            methods: Vec::new(),
            members: Vec::new(),
            annotations: Vec::new(),
            dom_id,
            styles: Vec::new(),
            parent: None,
            link: None,
            link_target: None,
            have_callback: false,
            tooltip: None,
        };
        self.ast.classes.insert(name, node);
    }

    /// `setClassLabel`: the label replaces the id, and the generic type stays in the text,
    /// unescaped here where `addClass` writes it escaped.
    fn set_class_label(&mut self, id: &str, label: &str) {
        let id = sanitize_text(id);
        let (class_name, _) = split_class_name_and_type(&id);
        let label = sanitize_text(label);
        if let Some(node) = self.ast.classes.get_mut(&class_name) {
            let suffix = if node.type_name.is_empty() {
                String::new()
            } else {
                format!("<{}>", node.type_name)
            };
            node.label = label;
            node.text = format!("{}{}", node.label, suffix);
        }
    }

    /// `addAnnotation`: the class has to exist, as it does upstream, where the `!` on the
    /// map value throws a `TypeError`; the message names the missing class instead.
    fn add_annotation(&mut self, class_name: &str, annotation: &str) -> Result<(), String> {
        let (class_name, _) = split_class_name_and_type(class_name);
        match self.ast.classes.get_mut(&class_name) {
            Some(node) => {
                node.annotations.push(annotation.to_string());
                Ok(())
            }
            None => Err(format!("Class not found: {class_name}")),
        }
    }

    /// `addMember`: `<<annotation>>`, then a member with a `)` is a method, then any other
    /// non-empty text is an attribute.
    fn add_member(&mut self, class_name: &str, member: &str) {
        self.add_class(class_name);

        let (validated_class_name, _) = split_class_name_and_type(&sanitize_text(class_name));
        let Some(node) = self.ast.classes.get_mut(&validated_class_name) else {
            return;
        };

        let member = member.trim();
        if member.starts_with("<<") && member.ends_with(">>") {
            node.annotations
                .push(sanitize_text(&member[2..member.len() - 2]));
        } else if member.find(')').is_some_and(|index| index > 0) {
            node.methods
                .push(ClassMember::new(member, MemberType::Method));
        } else if !member.is_empty() {
            node.members
                .push(ClassMember::new(member, MemberType::Attribute));
        }
    }

    /// `addMembers`. Upstream's list arrives reversed and it reverses it again; here the
    /// members are collected in source order, so they are added in source order.
    fn add_members(&mut self, class_name: &str, members: Vec<String>) {
        for member in &members {
            self.add_member(class_name, member);
        }
    }

    /// `addNote`.
    fn add_note(&mut self, text: &str, class: Option<&str>) -> String {
        let index = self.ast.notes.len();
        let id = format!("note{index}");
        self.ast.notes.insert(
            id.clone(),
            ClassNote {
                id: id.clone(),
                class: class.map(str::to_string),
                text: text.to_string(),
                index,
                parent: None,
            },
        );
        id
    }

    /// `setCssClass`: a comma separated id list, with the `classId-` prefix a numeric id
    /// starts with.
    fn set_css_class(&mut self, ids: &str, css_class: &str) {
        for id in ids.split(',') {
            let mut id = id.to_string();
            if id.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                id = format!("classId-{id}");
            }
            let (class_name, _) = split_class_name_and_type(&id);
            if let Some(node) = self.ast.classes.get_mut(&class_name) {
                node.css_classes.push(' ');
                node.css_classes.push_str(css_class);
            }
        }
    }

    /// `defineClass`: a `classDef` style set, its text-only styles (`fill` becomes
    /// `bgFill`), and the styles each already-assigned class receives.
    fn define_class(&mut self, ids: &[String], styles: &[String]) {
        for id in ids {
            if !self.ast.style_classes.contains(id) {
                self.ast.style_classes.insert(
                    id.clone(),
                    StyleClass {
                        id: id.clone(),
                        styles: Vec::new(),
                        text_styles: Vec::new(),
                    },
                );
            }

            for style in styles {
                if style.contains("color") {
                    let text_style = style.replacen("fill", "bgFill", 1);
                    if let Some(style_class) = self.ast.style_classes.get_mut(id) {
                        style_class.text_styles.push(text_style);
                    }
                }
                if let Some(style_class) = self.ast.style_classes.get_mut(id) {
                    style_class.styles.push(style.clone());
                }
            }

            for node in self.ast.classes.values_mut() {
                if node.css_classes.contains(id.as_str()) {
                    for style in styles {
                        for part in style.split(',') {
                            node.styles.push(part.to_string());
                        }
                    }
                }
            }
        }
    }

    /// `setTooltip`.
    fn set_tooltip(&mut self, ids: &str, tooltip: &str) {
        for id in ids.split(',') {
            let (class_name, _) = split_class_name_and_type(id);
            if let Some(node) = self.ast.classes.get_mut(&class_name) {
                node.tooltip = Some(sanitize_text(tooltip));
            }
        }
    }

    /// `setLink`, including the `clickable` css class it adds afterwards.
    fn set_link(&mut self, ids: &str, link: &str, target: Option<&str>) {
        for id in ids.split(',') {
            let mut id = id.to_string();
            if id.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                id = format!("classId-{id}");
            }
            let (class_name, _) = split_class_name_and_type(&id);
            if let Some(node) = self.ast.classes.get_mut(&class_name) {
                node.link = format_url(link);
                node.link_target = Some(sanitize_text(target.unwrap_or("_blank")));
            }
        }
        self.set_css_class(ids, "clickable");
    }

    /// `setClickEvent`. Upstream's `setClickFunc` only registers a DOM handler when the
    /// security level is `loose`; the state it always keeps is `haveCallback` and the
    /// `clickable` css class, which is all the database holds.
    fn set_click_event(&mut self, ids: &str) {
        for id in ids.split(',') {
            let (class_name, _) = split_class_name_and_type(id);
            if let Some(node) = self.ast.classes.get_mut(&class_name) {
                node.have_callback = true;
            }
        }
        self.set_css_class(ids, "clickable");
    }

    /// `setCssStyle`: a `style` line's declarations for one class; a value with commas is
    /// split into one style per part.
    fn set_css_style(&mut self, id: &str, styles: &[String]) {
        if styles.is_empty() {
            return;
        }
        let Some(node) = self.ast.classes.get_mut(id) else {
            return;
        };
        for style in styles {
            if style.contains(',') {
                for part in style.split(',') {
                    node.styles.push(part.to_string());
                }
            } else {
                node.styles.push(style.clone());
            }
        }
    }

    /// `addRelation`: a lollipop's dangling class becomes a hidden `interface` node, and
    /// the ids of a relation are always the class names without their generic types.
    fn add_relation(&mut self, mut relation: ClassRelation) {
        let is_marker = |endpoint: Endpoint| {
            matches!(
                endpoint,
                Endpoint::Lollipop
                    | Endpoint::Aggregation
                    | Endpoint::Composition
                    | Endpoint::Dependency
                    | Endpoint::Extension
            )
        };

        if relation.relation.type1 == Endpoint::Lollipop && !is_marker(relation.relation.type2) {
            self.add_class(&relation.id2);
            self.add_interface(&relation.id1, &relation.id2);
            relation.id1 = format!("interface{}", self.ast.interfaces.len() - 1);
        } else if relation.relation.type2 == Endpoint::Lollipop
            && !is_marker(relation.relation.type1)
        {
            self.add_class(&relation.id1);
            self.add_interface(&relation.id2, &relation.id1);
            relation.id2 = format!("interface{}", self.ast.interfaces.len() - 1);
        } else {
            self.add_class(&relation.id1);
            self.add_class(&relation.id2);
        }

        relation.id1 = split_class_name_and_type(&relation.id1).0;
        relation.id2 = split_class_name_and_type(&relation.id2).0;
        relation.relation_title1 = sanitize_text(relation.relation_title1.trim());
        relation.relation_title2 = sanitize_text(relation.relation_title2.trim());
        self.ast.relations.push(relation);
    }

    fn add_interface(&mut self, label: &str, class_id: &str) {
        let id = format!("interface{}", self.ast.interfaces.len());
        self.ast.interfaces.push(Interface {
            id,
            label: label.to_string(),
            class_id: class_id.to_string(),
        });
    }

    /// `addNamespace`: a dotted name creates its intermediate ancestors, a re-declaration
    /// promotes an ancestor to explicit, and the qualified id is pushed on the stack that
    /// qualifies the namespaces declared inside it.
    fn add_namespace(&mut self, id: &str, label: Option<&str>) -> String {
        let qualified_id = match self.namespace_stack.last() {
            Some(prefix) => format!("{prefix}.{id}"),
            None => id.to_string(),
        };
        self.namespace_stack.push(qualified_id.clone());

        if let Some(existing) = self.ast.namespaces.get_mut(&qualified_id) {
            existing.explicit = true;
            if let Some(label) = label {
                existing.label = label.to_string();
            }
            return qualified_id;
        }

        let parts: Vec<String> = qualified_id.split('.').map(str::to_string).collect();
        let ancestor_ids = get_ancestor_ids(&qualified_id);
        for (index, current_id) in ancestor_ids.iter().enumerate() {
            let parent_id = if index > 0 {
                Some(ancestor_ids[index - 1].clone())
            } else {
                None
            };
            let is_leaf = index + 1 == ancestor_ids.len();
            let node_label = if is_leaf {
                label
                    .map(str::to_string)
                    .unwrap_or_else(|| parts[index].clone())
            } else {
                parts[index].clone()
            };

            if !self.ast.namespaces.contains(current_id) {
                let node =
                    self.create_namespace_node(current_id, &node_label, parent_id.clone(), is_leaf);
                self.ast.namespaces.insert(current_id.clone(), node);
            } else if is_leaf && let Some(node) = self.ast.namespaces.get_mut(current_id) {
                node.explicit = true;
            }

            if let Some(parent_id) = parent_id {
                self.link_parent_child(&parent_id, current_id);
            }
        }

        qualified_id
    }

    fn create_namespace_node(
        &mut self,
        id: &str,
        label: &str,
        parent: Option<String>,
        explicit: bool,
    ) -> NamespaceNode {
        let dom_id = format!("classId-{id}-{}", self.namespace_counter);
        self.namespace_counter += 1;
        NamespaceNode {
            id: id.to_string(),
            label: label.to_string(),
            dom_id,
            classes: Vec::new(),
            notes: Vec::new(),
            children: Vec::new(),
            parent,
            explicit,
        }
    }

    fn link_parent_child(&mut self, parent_id: &str, child_id: &str) {
        let child_has_no_parent = self
            .ast
            .namespaces
            .get(child_id)
            .is_some_and(|child| child.parent.is_none());
        if child_has_no_parent && let Some(child) = self.ast.namespaces.get_mut(child_id) {
            child.parent = Some(parent_id.to_string());
        }
        if let Some(parent) = self.ast.namespaces.get_mut(parent_id)
            && !parent.children.iter().any(|child| child == child_id)
        {
            parent.children.push(child_id.to_string());
        }
    }

    fn pop_namespace(&mut self) {
        self.namespace_stack.pop();
    }

    /// `addClassesToNamespace`.
    fn add_classes_to_namespace(
        &mut self,
        id: &str,
        class_names: &[String],
        note_names: &[String],
    ) {
        if !self.ast.namespaces.contains(id) {
            return;
        }
        for name in class_names {
            let (class_name, _) = split_class_name_and_type(name);
            if let Some(node) = self.ast.classes.get_mut(&class_name) {
                node.parent = Some(id.to_string());
            }
            if let Some(namespace) = self.ast.namespaces.get_mut(id)
                && !namespace.classes.contains(&class_name)
            {
                namespace.classes.push(class_name);
            }
        }
        for note_name in note_names {
            if let Some(note) = self.ast.notes.get_mut(note_name) {
                note.parent = Some(id.to_string());
            }
            if let Some(namespace) = self.ast.namespaces.get_mut(id)
                && !namespace.notes.contains(note_name)
            {
                namespace.notes.push(note_name.clone());
            }
        }
    }
}

/// `getAncestorIds`: every prefix of a dotted id, `A`, `A.B`, `A.B.C`.
fn get_ancestor_ids(qualified_id: &str) -> Vec<String> {
    let parts: Vec<&str> = qualified_id.split('.').collect();
    let mut ids: Vec<String> = Vec::with_capacity(parts.len());
    ids.push(parts[0].to_string());
    for part in &parts[1..] {
        let previous = ids.last().expect("one id per part");
        ids.push(format!("{previous}.{part}"));
    }
    ids
}

/// `utils.formatUrl` with the default `strict` security level: trim, and leave the text to
/// [`sanitize_url`].
fn format_url(link: &str) -> Option<String> {
    let link = link.trim();
    if link.is_empty() {
        return None;
    }
    Some(sanitize_url(link))
}

/// A stand-in for `@braintree/sanitize-url`: strip control characters, answer
/// `about:blank` for a dangerous scheme, and normalise a URL that has a scheme and an
/// authority with no path the way `new URL(...).href` does (it appends the `/`).
fn sanitize_url(url: &str) -> String {
    const BLANK: &str = "about:blank";
    let cleaned: String = url.chars().filter(|c| !c.is_control()).collect::<String>();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        return BLANK.to_string();
    }

    if let Some(colon) = cleaned.find(':') {
        let scheme = cleaned[..colon].trim().to_ascii_lowercase();
        let valid_scheme = !scheme.is_empty()
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.');
        if valid_scheme {
            if matches!(scheme.as_str(), "javascript" | "data" | "vbscript") {
                return BLANK.to_string();
            }
            if let Some(rest) = cleaned[colon + 1..].strip_prefix("//")
                && rest.find(['/', '?', '#']).is_none()
            {
                return format!("{}//{}/", &cleaned[..colon + 1], rest);
            }
        }
    }

    cleaned.to_string()
}
