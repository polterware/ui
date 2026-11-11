//! The C4 database's writing side, ported from `c4Db.ts`.
//!
//! Every grammar action calls one of these methods in the order upstream calls it, so the
//! resulting lists match `getC4ShapeArray()`, `getBoundaries()`, and `getRels()` entry for
//! entry: an element with an alias that already exists is updated in place, not appended, and
//! a relationship between the same two endpoints is updated in place as well.
//!
//! A parameter may be a raw positional value or a single `{ key: value }` named override. A
//! named override writes its own key: `descr`, `techn`, `type`, and `label` are stored as
//! `{ text }`, everything else as a plain string. That is what lets `$tags="x"` land in a
//! text slot and still stay a string.
//!
//! # Where the port stops short of upstream
//!
//! * Upstream is JavaScript and stores whatever it is handed. A named attribute that lands
//!   in a slot that is not a bag field (an alias, a relationship endpoint) puts an object
//!   there; this port keeps the attribute's value as the string, which is what every later
//!   lookup expects.
//! * A key no C4 type declares is dropped instead of being added to the object.
//! * `parseInt`'s `NaN` is `None` here, so a non-numeric offset leaves the field unset
//!   instead of storing `NaN`; a named offset that lands on a colour key is dropped for the
//!   same reason.

use super::ast::{Ast, Boundary, Rel, Shape, Text, TextValue};
use super::sanitize;
use crate::preprocess::is_js_whitespace;

/// One parsed argument: `attribute` in the jison grammar.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Attr {
    /// `STR` or `ATTRIBUTE_EMPTY`, trimmed by the grammar.
    Text(String),
    /// `STR_KEY STR_VALUE`, the `{ key: value }` object upstream builds.
    Named { key: String, value: String },
}

fn text_from(attr: &Attr) -> Text {
    match attr {
        Attr::Text(value) => Text::plain(value),
        Attr::Named { key, value } => Text {
            text: TextValue::Named {
                key: key.clone(),
                value: value.clone(),
            },
        },
    }
}

/// A slot that upstream assigns straight to a string. A named value there is an object
/// upstream; this keeps the value (see the module docs).
fn slot_string(attr: &Attr) -> String {
    match attr {
        Attr::Text(value) => value.clone(),
        Attr::Named { value, .. } => value.clone(),
    }
}

/// The database `c4Db.ts` keeps between parses.
pub(crate) struct Db {
    shapes: Vec<Shape>,
    boundaries: Vec<Boundary>,
    rels: Vec<Rel>,
    boundary_stack: Vec<String>,
    current_boundary: String,
    parent_boundary: String,
    title: String,
    acc_description: String,
    c4_type: Option<String>,
    direction: Option<String>,
    shape_in_row: u32,
    boundary_in_row: u32,
}

impl Db {
    pub(crate) fn new() -> Self {
        Self {
            shapes: Vec::new(),
            boundaries: vec![global_boundary()],
            rels: Vec::new(),
            boundary_stack: vec![String::new()],
            current_boundary: "global".to_string(),
            parent_boundary: String::new(),
            title: String::new(),
            acc_description: String::new(),
            c4_type: None,
            direction: None,
            shape_in_row: 4,
            boundary_in_row: 2,
        }
    }

    /// `setC4Type`.
    pub(crate) fn set_c4_type(&mut self, c4_type: &str) {
        self.c4_type = Some(sanitize::sanitize_text(c4_type));
    }

    /// `setTitle`.
    pub(crate) fn set_title(&mut self, title: &str) {
        self.title = sanitize::sanitize_text(title);
    }

    /// `setAccDescription`.
    pub(crate) fn set_acc_description(&mut self, description: &str) {
        self.acc_description = sanitize::set_acc_description(description);
    }

    /// The `direction` start rule's `yy.setDirection`: upstream's C4 database has no such
    /// method and throws; this port keeps the parsed direction instead.
    pub(crate) fn set_direction(&mut self, direction: &str) {
        self.direction = Some(direction.to_string());
    }

    /// `addPersonOrSystem`: `type, alias, label, ?descr, ?sprite, ?tags, $link`.
    pub(crate) fn add_person_or_system(&mut self, type_c4_shape: &str, attrs: &[Attr]) {
        let alias = attrs.first().map(slot_string).unwrap_or_default();
        let current = self.current_boundary.clone();
        let index = self.shape_index(&alias);
        let shape = &mut self.shapes[index];

        shape.label = match attrs.get(1) {
            Some(attr) => text_from(attr),
            None => Text::plain(""),
        };
        match attrs.get(2) {
            None => {
                shape.descr.get_or_insert_with(|| Text::plain(""));
            }
            Some(Attr::Named { key, value }) => shape.set_named(key, value, true),
            Some(attr) => shape.descr = Some(text_from(attr)),
        }

        assign_shape_attributes(shape, attrs.get(3), attrs.get(4), attrs.get(5));
        shape.type_c4_shape = Text::plain(type_c4_shape);
        shape.parent_boundary = current;
        shape.wrap = false;
    }

    /// `addContainer`: `type, alias, label, ?techn, ?descr, ?sprite, ?tags, $link`.
    pub(crate) fn add_container(&mut self, type_c4_shape: &str, attrs: &[Attr]) {
        let alias = attrs.first().map(slot_string).unwrap_or_default();
        let current = self.current_boundary.clone();
        let index = self.shape_index(&alias);
        let shape = &mut self.shapes[index];

        shape.label = match attrs.get(1) {
            Some(attr) => text_from(attr),
            None => Text::plain(""),
        };
        match attrs.get(2) {
            None => {
                shape.techn.get_or_insert_with(|| Text::plain(""));
            }
            Some(Attr::Named { key, value }) => shape.set_named(key, value, true),
            Some(attr) => shape.techn = Some(text_from(attr)),
        }
        match attrs.get(3) {
            None => {
                shape.descr.get_or_insert_with(|| Text::plain(""));
            }
            Some(Attr::Named { key, value }) => shape.set_named(key, value, true),
            Some(attr) => shape.descr = Some(text_from(attr)),
        }

        assign_shape_attributes(shape, attrs.get(4), attrs.get(5), attrs.get(6));
        shape.wrap = false;
        shape.type_c4_shape = Text::plain(type_c4_shape);
        shape.parent_boundary = current;
    }

    /// `addComponent`: same parameters as [`Db::add_container`].
    pub(crate) fn add_component(&mut self, type_c4_shape: &str, attrs: &[Attr]) {
        self.add_container(type_c4_shape, attrs);
    }

    /// `addPersonOrSystemBoundary` and `addContainerBoundary`:
    /// `alias, label, ?type, ?tags, $link`.
    pub(crate) fn add_text_boundary(&mut self, default_type: &str, attrs: &[Attr]) {
        self.add_boundary(default_type, attrs, None);
    }

    /// `addDeploymentNode`:
    /// `nodeType, alias, label, ?type, ?descr, ?sprite, ?tags, $link`.
    pub(crate) fn add_deployment_node(&mut self, node_type: &str, attrs: &[Attr]) {
        self.add_boundary("node", attrs, Some(node_type));
    }

    /// `popBoundaryParseStack`.
    pub(crate) fn pop_boundary_parse_stack(&mut self) {
        self.current_boundary = self.parent_boundary.clone();
        self.boundary_stack.pop();
        self.parent_boundary = self.boundary_stack.pop().unwrap_or_default();
        self.boundary_stack.push(self.parent_boundary.clone());
    }

    /// `addRel`:
    /// `type, from, to, label, ?techn, ?descr, ?sprite, ?tags, $link`.
    pub(crate) fn add_rel(&mut self, type_name: &str, attrs: &[Attr]) {
        // "Don't allow label nulling": a missing label means no relationship at all.
        if attrs.len() < 3 {
            return;
        }
        let from = slot_string(&attrs[0]);
        let to = slot_string(&attrs[1]);
        let index = match self
            .rels
            .iter()
            .position(|rel| rel.from == from && rel.to == to)
        {
            Some(index) => index,
            None => {
                self.rels.push(Rel::default());
                self.rels.len() - 1
            }
        };
        let rel = &mut self.rels[index];

        rel.type_name = type_name.to_string();
        rel.from = from;
        rel.to = to;
        rel.label = text_from(&attrs[2]);
        match attrs.get(3) {
            None => {
                rel.techn.get_or_insert_with(|| Text::plain(""));
            }
            Some(Attr::Named { key, value }) => rel.set_named(key, value, true),
            Some(attr) => rel.techn = Some(text_from(attr)),
        }
        match attrs.get(4) {
            None => {
                rel.descr.get_or_insert_with(|| Text::plain(""));
            }
            Some(Attr::Named { key, value }) => rel.set_named(key, value, true),
            Some(attr) => rel.descr = Some(text_from(attr)),
        }

        assign_rel_attributes(rel, attrs.get(5), attrs.get(6), attrs.get(7));
        rel.wrap = false;
    }

    /// `updateElStyle`:
    /// `elementName, ?bgColor, ?fontColor, ?borderColor, ?shadowing, ?shape, ?sprite, ?techn,
    /// ?legendText, ?legendSprite`.
    pub(crate) fn update_el_style(&mut self, attrs: &[Attr]) {
        let Some(element_name) = attrs.first().map(slot_string) else {
            return;
        };
        let target = if let Some(index) = self
            .shapes
            .iter()
            .position(|shape| shape.alias == element_name)
        {
            Target::Shape(index)
        } else if let Some(index) = self
            .boundaries
            .iter()
            .position(|boundary| boundary.alias == element_name)
        {
            Target::Boundary(index)
        } else {
            return;
        };

        let parameters = [
            ("bgColor", attrs.get(1)),
            ("fontColor", attrs.get(2)),
            ("borderColor", attrs.get(3)),
            ("shadowing", attrs.get(4)),
            ("shape", attrs.get(5)),
            ("sprite", attrs.get(6)),
            ("techn", attrs.get(7)),
            ("legendText", attrs.get(8)),
            ("legendSprite", attrs.get(9)),
        ];
        for (field, attr) in parameters {
            let Some(attr) = attr else {
                continue;
            };
            match target {
                Target::Shape(index) => apply_shape_style(&mut self.shapes[index], field, attr),
                Target::Boundary(index) => {
                    apply_boundary_style(&mut self.boundaries[index], field, attr);
                }
            }
        }
    }

    /// `updateRelStyle`: `from, to, ?textColor, ?lineColor, ?offsetX, ?offsetY`.
    pub(crate) fn update_rel_style(&mut self, attrs: &[Attr]) {
        let (Some(from), Some(to)) = (attrs.first(), attrs.get(1)) else {
            return;
        };
        let from = slot_string(from);
        let to = slot_string(to);
        let Some(index) = self
            .rels
            .iter()
            .position(|rel| rel.from == from && rel.to == to)
        else {
            return;
        };
        let rel = &mut self.rels[index];

        for (field, attr) in [("textColor", attrs.get(2)), ("lineColor", attrs.get(3))] {
            let Some(attr) = attr else {
                continue;
            };
            match attr {
                Attr::Named { key, value } => rel.set_named(key, value, false),
                Attr::Text(value) => rel.set_style_positional(field, value),
            }
        }
        for (field, attr) in [("offsetX", attrs.get(4)), ("offsetY", attrs.get(5))] {
            let Some(attr) = attr else {
                continue;
            };
            match attr {
                Attr::Named { key, value } => {
                    let parsed = parse_int(value);
                    match key.as_str() {
                        "offsetX" => rel.offset_x = parsed,
                        "offsetY" => rel.offset_y = parsed,
                        _ => {}
                    }
                }
                Attr::Text(value) => {
                    rel.set_style_positional(field, value);
                }
            }
        }
    }

    /// `updateLayoutConfig`: `?c4ShapeInRow, ?c4BoundaryInRow`, each kept when `>= 1`.
    pub(crate) fn update_layout_config(&mut self, attrs: &[Attr]) {
        if let Some(value) = attrs.first().and_then(attr_int)
            && value >= 1
        {
            self.shape_in_row = value as u32;
        }
        if let Some(value) = attrs.get(1).and_then(attr_int)
            && value >= 1
        {
            self.boundary_in_row = value as u32;
        }
    }

    pub(crate) fn into_ast(self) -> Ast {
        Ast {
            c4_type: self.c4_type,
            title: self.title,
            direction: self.direction,
            shapes: self.shapes,
            boundaries: self.boundaries,
            rels: self.rels,
            shape_in_row: self.shape_in_row,
            boundary_in_row: self.boundary_in_row,
            acc_description: self.acc_description,
        }
    }

    /// The index of the shape with `alias`, appending an empty one when it is new.
    fn shape_index(&mut self, alias: &str) -> usize {
        match self.shapes.iter().position(|shape| shape.alias == alias) {
            Some(index) => index,
            None => {
                self.shapes.push(Shape {
                    alias: alias.to_string(),
                    ..Shape::default()
                });
                self.shapes.len() - 1
            }
        }
    }

    /// The shared body of the three boundary additions.
    fn add_boundary(&mut self, default_type: &str, attrs: &[Attr], node_type: Option<&str>) {
        let alias = attrs.first().map(slot_string).unwrap_or_default();
        let current = self.current_boundary.clone();
        let index = match self
            .boundaries
            .iter()
            .position(|boundary| boundary.alias == alias)
        {
            Some(index) => index,
            None => {
                self.boundaries.push(Boundary {
                    alias: alias.clone(),
                    ..Boundary::default()
                });
                self.boundaries.len() - 1
            }
        };
        let boundary = &mut self.boundaries[index];

        boundary.label = match attrs.get(1) {
            Some(attr) => text_from(attr),
            None => Text::plain(""),
        };
        match attrs.get(2) {
            None => boundary.type_name = Some(Text::plain(default_type)),
            Some(Attr::Named { key, value }) => boundary.set_named(key, value, true),
            Some(attr) => boundary.type_name = Some(text_from(attr)),
        }

        if let Some(node_type) = node_type {
            match attrs.get(3) {
                None => {
                    boundary.descr.get_or_insert_with(|| Text::plain(""));
                }
                Some(Attr::Named { key, value }) => boundary.set_named(key, value, true),
                Some(attr) => boundary.descr = Some(text_from(attr)),
            }
            // Upstream passes `{ tags, link }` here: the positional `sprite` argument of
            // `addDeploymentNode` is accepted and then never stored.
            assign_boundary_attributes(boundary, None, attrs.get(5), attrs.get(6));
            boundary.node_type = Some(node_type.to_string());
        } else {
            assign_boundary_attributes(boundary, None, attrs.get(3), attrs.get(4));
        }

        boundary.parent_boundary = current.clone();
        boundary.wrap = false;

        self.parent_boundary = current;
        self.current_boundary = alias;
        self.boundary_stack.push(self.parent_boundary.clone());
    }
}

enum Target {
    Shape(usize),
    Boundary(usize),
}

fn global_boundary() -> Boundary {
    Boundary {
        alias: "global".to_string(),
        label: Text::plain("global"),
        type_name: Some(Text::plain("global")),
        parent_boundary: String::new(),
        ..Boundary::default()
    }
}

fn assign_shape_attributes(
    shape: &mut Shape,
    sprite: Option<&Attr>,
    tags: Option<&Attr>,
    link: Option<&Attr>,
) {
    for (field, attr) in [("sprite", sprite), ("tags", tags), ("link", link)] {
        match attr {
            None => {}
            Some(Attr::Named { key, value }) => shape.set_named(key, value, true),
            Some(Attr::Text(value)) => match field {
                "sprite" => shape.sprite = Some(value.clone()),
                "tags" => shape.tags = Some(value.clone()),
                _ => shape.link = Some(value.clone()),
            },
        }
    }
}

fn assign_boundary_attributes(
    boundary: &mut Boundary,
    sprite: Option<&Attr>,
    tags: Option<&Attr>,
    link: Option<&Attr>,
) {
    for (field, attr) in [("sprite", sprite), ("tags", tags), ("link", link)] {
        match attr {
            None => {}
            Some(Attr::Named { key, value }) => boundary.set_named(key, value, true),
            Some(Attr::Text(value)) => match field {
                "sprite" => boundary.sprite = Some(value.clone()),
                "tags" => boundary.tags = Some(value.clone()),
                _ => boundary.link = Some(value.clone()),
            },
        }
    }
}

fn assign_rel_attributes(
    rel: &mut Rel,
    sprite: Option<&Attr>,
    tags: Option<&Attr>,
    link: Option<&Attr>,
) {
    for (field, attr) in [("sprite", sprite), ("tags", tags), ("link", link)] {
        match attr {
            None => {}
            Some(Attr::Named { key, value }) => rel.set_named(key, value, true),
            Some(Attr::Text(value)) => match field {
                "sprite" => rel.sprite = Some(value.clone()),
                "tags" => rel.tags = Some(value.clone()),
                _ => rel.link = Some(value.clone()),
            },
        }
    }
}

impl Shape {
    /// `assignAttributes`' object branch, or `UpdateElementStyle`'s (with `wrap` false).
    fn set_named(&mut self, key: &str, value: &str, wrap: bool) {
        let text = |value: &str| {
            if wrap {
                Text::plain(value)
            } else {
                Text::bare(value)
            }
        };
        match key {
            "label" => self.label = text(value),
            "descr" => self.descr = Some(text(value)),
            "techn" => self.techn = Some(text(value)),
            "type" => self.type_name = Some(text(value)),
            "sprite" => self.sprite = Some(value.to_string()),
            "tags" => self.tags = Some(value.to_string()),
            "link" => self.link = Some(value.to_string()),
            "bgColor" => self.bg_color = Some(value.to_string()),
            "fontColor" => self.font_color = Some(value.to_string()),
            "borderColor" => self.border_color = Some(value.to_string()),
            "shadowing" => self.shadowing = Some(value.to_string()),
            "shape" => self.shape = Some(value.to_string()),
            "legendText" => self.legend_text = Some(value.to_string()),
            "legendSprite" => self.legend_sprite = Some(value.to_string()),
            _ => {}
        }
    }

    /// A positional `UpdateElementStyle` value.
    fn set_style_positional(&mut self, field: &str, value: &str) {
        match field {
            "bgColor" => self.bg_color = Some(value.to_string()),
            "fontColor" => self.font_color = Some(value.to_string()),
            "borderColor" => self.border_color = Some(value.to_string()),
            "shadowing" => self.shadowing = Some(value.to_string()),
            "shape" => self.shape = Some(value.to_string()),
            "sprite" => self.sprite = Some(value.to_string()),
            "techn" => self.techn = Some(Text::bare(value)),
            "legendText" => self.legend_text = Some(value.to_string()),
            _ => self.legend_sprite = Some(value.to_string()),
        }
    }
}

impl Boundary {
    fn set_named(&mut self, key: &str, value: &str, wrap: bool) {
        let text = |value: &str| {
            if wrap {
                Text::plain(value)
            } else {
                Text::bare(value)
            }
        };
        match key {
            "label" => self.label = text(value),
            "descr" => self.descr = Some(text(value)),
            "techn" => self.techn = Some(text(value)),
            "type" => self.type_name = Some(text(value)),
            "sprite" => self.sprite = Some(value.to_string()),
            "tags" => self.tags = Some(value.to_string()),
            "link" => self.link = Some(value.to_string()),
            "nodeType" => self.node_type = Some(value.to_string()),
            "bgColor" => self.bg_color = Some(value.to_string()),
            "fontColor" => self.font_color = Some(value.to_string()),
            "borderColor" => self.border_color = Some(value.to_string()),
            "shadowing" => self.shadowing = Some(value.to_string()),
            "shape" => self.shape = Some(value.to_string()),
            "legendText" => self.legend_text = Some(value.to_string()),
            "legendSprite" => self.legend_sprite = Some(value.to_string()),
            _ => {}
        }
    }

    fn set_style_positional(&mut self, field: &str, value: &str) {
        match field {
            "bgColor" => self.bg_color = Some(value.to_string()),
            "fontColor" => self.font_color = Some(value.to_string()),
            "borderColor" => self.border_color = Some(value.to_string()),
            "shadowing" => self.shadowing = Some(value.to_string()),
            "shape" => self.shape = Some(value.to_string()),
            "sprite" => self.sprite = Some(value.to_string()),
            "techn" => self.techn = Some(Text::bare(value)),
            "legendText" => self.legend_text = Some(value.to_string()),
            _ => self.legend_sprite = Some(value.to_string()),
        }
    }
}

impl Rel {
    fn set_named(&mut self, key: &str, value: &str, wrap: bool) {
        let text = |value: &str| {
            if wrap {
                Text::plain(value)
            } else {
                Text::bare(value)
            }
        };
        match key {
            "label" => self.label = text(value),
            "techn" => self.techn = Some(text(value)),
            "descr" => self.descr = Some(text(value)),
            "sprite" => self.sprite = Some(value.to_string()),
            "tags" => self.tags = Some(value.to_string()),
            "link" => self.link = Some(value.to_string()),
            "textColor" => self.text_color = Some(value.to_string()),
            "lineColor" => self.line_color = Some(value.to_string()),
            "offsetX" => self.offset_x = parse_int(value),
            "offsetY" => self.offset_y = parse_int(value),
            _ => {}
        }
    }

    fn set_style_positional(&mut self, field: &str, value: &str) {
        match field {
            "textColor" => self.text_color = Some(value.to_string()),
            "lineColor" => self.line_color = Some(value.to_string()),
            "offsetX" => self.offset_x = parse_int(value),
            _ => self.offset_y = parse_int(value),
        }
    }
}

fn apply_shape_style(shape: &mut Shape, field: &str, attr: &Attr) {
    match attr {
        Attr::Named { key, value } => shape.set_named(key, value, false),
        Attr::Text(value) => shape.set_style_positional(field, value),
    }
}

fn apply_boundary_style(boundary: &mut Boundary, field: &str, attr: &Attr) {
    match attr {
        Attr::Named { key, value } => boundary.set_named(key, value, false),
        Attr::Text(value) => boundary.set_style_positional(field, value),
    }
}

fn attr_int(attr: &Attr) -> Option<i64> {
    match attr {
        Attr::Text(value) => parse_int(value),
        Attr::Named { value, .. } => parse_int(value),
    }
}

/// JavaScript's `parseInt(value)`: leading whitespace, an optional sign, an optional `0x`,
/// then as many ASCII digits as the radix accepts. `None` is `NaN`.
fn parse_int(text: &str) -> Option<i64> {
    let text = text.trim_start_matches(is_js_whitespace);
    let (negative, text) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let (radix, digits) = match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        Some(hex) => (16, hex),
        None => (10, text),
    };
    let length = digits.chars().take_while(|ch| ch.is_digit(radix)).count();
    if length == 0 {
        return None;
    }
    let value = i128::from_str_radix(&digits[..length], radix).ok()?;
    i64::try_from(if negative { -value } else { value }).ok()
}
