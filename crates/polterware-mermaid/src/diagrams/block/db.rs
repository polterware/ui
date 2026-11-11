//! The block database's writing side, ported from `blockDB.ts`: `setHierarchy`,
//! `populateBlockDatabase`, `addStyleClass`, `addStyle2Node`, `setCssClass`, and the
//! `typeStr2Type`/`edgeStrTo*` helpers the grammar calls.
//!
//! The database is a `Map` whose insertion order `getBlocksFlat` exposes, so the arena here
//! keeps the keys in insertion order and `set` replaces a value without moving its key, as
//! JavaScript's `Map.set` does. Blocks are arena entries referred to by index because
//! `populateBlockDatabase` mutates a block after its children were placed (a `style`
//! statement can name a block defined earlier, and a duplicate id merges into the first
//! object).
//!
//! Two upstream footguns are reproduced on purpose:
//!
//! * An anonymous `block` and every `space` take their id from `generateId` (see
//!   [`super::parser`]); this module just uses the id the grammar handed it.
//! * A `style` statement naming a block the database does not hold yet throws a `TypeError`
//!   upstream (`addStyle2Node` writes through `undefined`); here it is a [`ParseError`] at
//!   the statement.

use std::collections::HashMap;

use super::ast::{Ast, Block, BlockType, ClassDef, Edge};
use super::parser::{EdgeValue, NodeValue, Parsed, Statement};
use super::sanitize;
use crate::error::{ParseError, Span};

/// Populates a fresh database from the parsed document (`setHierarchy`).
pub(crate) fn build(statements: &[Parsed]) -> Result<Ast, ParseError> {
    let mut db = Db::new();
    let root = db.root;
    db.populate(statements, root)?;
    Ok(db.into_ast())
}

/// `typeStr2Type`: the shape a `NODE_DSTART` + `NODE_DEND` pair names.
pub(crate) fn type_str2_type(type_str: Option<&str>) -> BlockType {
    match type_str {
        Some("[]") => BlockType::Square,
        Some("()") => BlockType::Round,
        Some("(())") => BlockType::Circle,
        Some(">]") => BlockType::RectLeftInvArrow,
        Some("{}") => BlockType::Diamond,
        Some("{{}}") => BlockType::Hexagon,
        Some("([])") => BlockType::Stadium,
        Some("[[]]") => BlockType::Subroutine,
        Some("[()]") => BlockType::Cylinder,
        Some("((()))") => BlockType::DoubleCircle,
        Some("[//]") => BlockType::LeanRight,
        Some("[\\\\]") => BlockType::LeanLeft,
        Some("[/\\]") => BlockType::Trapezoid,
        Some("[\\/]") => BlockType::InvTrapezoid,
        Some("<[]>") => BlockType::BlockArrow,
        _ => BlockType::Na,
    }
}

/// `edgeStrToEdgeData`: the arrow head at the end of a link, decided by its last character.
pub(crate) fn edge_str_to_edge_data(edge_type_str: &str) -> &'static str {
    match edge_type_str.trim().chars().next_back() {
        Some('x') => "arrow_cross",
        Some('o') => "arrow_circle",
        Some('>') => "arrow_point",
        _ => "",
    }
}

/// `edgeStrToEdgeStartData`: the arrow head at the start of a link.
pub(crate) fn edge_str_to_edge_start_data(edge_type_str: &str) -> &'static str {
    match edge_type_str.trim().chars().next() {
        Some('x') => "arrow_cross",
        Some('o') => "arrow_circle",
        Some('<') => "arrow_point",
        _ => "arrow_open",
    }
}

/// `edgeStrToThickness`.
pub(crate) fn edge_str_to_thickness(edge_type_str: &str) -> &'static str {
    if edge_type_str.contains("==") {
        "thick"
    } else {
        "normal"
    }
}

/// `edgeStrToPattern`.
pub(crate) fn edge_str_to_pattern(edge_type_str: &str) -> &'static str {
    if edge_type_str.contains(".-") {
        "dotted"
    } else {
        "solid"
    }
}

/// One block object of the database (`Block` in `blockTypes.ts`), minus the edge fields,
/// which live in [`Edge`].
#[derive(Clone, Debug, Default)]
struct Object {
    id: String,
    label: Option<String>,
    type_name: BlockType,
    columns: Option<i64>,
    width_in_columns: Option<i64>,
    width: Option<i64>,
    classes: Vec<String>,
    directions: Vec<String>,
    styles: Vec<String>,
    color_index: Option<usize>,
    children: Option<Vec<usize>>,
}

struct Db {
    objects: Vec<Object>,
    /// The map's keys in insertion order (`getBlocksFlat`).
    keys: Vec<String>,
    index: HashMap<String, usize>,
    root: usize,
    edges: Vec<Edge>,
    edge_count: HashMap<String, usize>,
    classes: Vec<ClassDef>,
    class_index: HashMap<String, usize>,
    next_color_index: usize,
    warnings: Vec<String>,
}

impl Db {
    fn new() -> Self {
        let root = Object {
            id: "root".to_string(),
            type_name: BlockType::Composite,
            columns: Some(-1),
            ..Object::default()
        };
        Self {
            objects: vec![root],
            keys: vec!["root".to_string()],
            index: HashMap::from([("root".to_string(), 0)]),
            root: 0,
            edges: Vec::new(),
            edge_count: HashMap::new(),
            classes: Vec::new(),
            class_index: HashMap::new(),
            next_color_index: 0,
            warnings: Vec::new(),
        }
    }

    /// A `Map.set`: a new key appends to the insertion order, an existing key keeps its
    /// position and only takes the new value.
    fn set_key(&mut self, key: String, index: usize) {
        if self.index.insert(key.clone(), index).is_none() {
            self.keys.push(key);
        }
    }

    fn push_object(&mut self, object: Object) -> usize {
        self.objects.push(object);
        self.objects.len() - 1
    }

    /// `populateBlockDatabase`: places a list of statements under `parent`, flattening the
    /// arrays a chained edge produced.
    fn populate(&mut self, statements: &[Parsed], parent: usize) -> Result<(), ParseError> {
        let mut list: Vec<&Statement> = Vec::new();
        for parsed in statements {
            match parsed {
                Parsed::Statement(statement) => list.push(statement),
                Parsed::Chain(elements) => list.extend(elements.iter()),
            }
        }
        let column = list
            .iter()
            .find_map(|statement| match statement {
                Statement::ColumnSetting(columns) => Some(*columns),
                _ => None,
            })
            .unwrap_or(-1);
        let mut children = Vec::new();
        for statement in list {
            if column > 0
                && let Some(width) = statement_width(statement)
                && width > column
            {
                self.warnings.push(format!(
                    "Block {} width {width} exceeds configured column width {column}",
                    statement_id(statement).unwrap_or("undefined")
                ));
            }
            let label = statement_label(statement);
            match statement {
                Statement::ClassDef { id, css } => self.add_style_class(id, css),
                Statement::ApplyClass { id, style_class } => self.set_css_class(id, style_class),
                Statement::ApplyStyles {
                    id,
                    styles_str,
                    span,
                } => {
                    if !styles_str.is_empty() {
                        self.add_style2_node(id, styles_str, *span)?;
                    }
                }
                Statement::ColumnSetting(columns) => self.objects[parent].columns = Some(*columns),
                Statement::Edge(edge) => self.add_edge(edge, label.unwrap_or_default()),
                Statement::Node(node) => {
                    self.place(block_object(node, label), None, &mut children)?;
                }
                Statement::Composite {
                    node,
                    children: nested,
                } => {
                    let object = Object {
                        children: Some(Vec::new()),
                        ..block_object(node, label)
                    };
                    self.place(object, Some(nested), &mut children)?;
                }
                Statement::Space { id, width } => {
                    let object = Object {
                        id: id.clone(),
                        label: Some(String::new()),
                        type_name: BlockType::Space,
                        width: Some(*width),
                        children: Some(Vec::new()),
                        ..Object::default()
                    };
                    self.place(object, None, &mut children)?;
                }
            }
        }
        self.objects[parent].children = Some(children);
        Ok(())
    }

    /// The tail of `populateBlockDatabase`'s loop: the label fallback, the first-object
    /// merge, the recursion into a composite's children, and the `space` copies.
    fn place(
        &mut self,
        mut object: Object,
        nested: Option<&[Parsed]>,
        children: &mut Vec<usize>,
    ) -> Result<(), ParseError> {
        let id = object.id.clone();
        if object.label.as_deref().is_none_or(str::is_empty) {
            object.label = Some(if object.type_name == BlockType::Composite {
                String::new()
            } else {
                id.clone()
            });
        }
        let existing = self.index.get(&id).copied();
        let index = match existing {
            None => {
                if object.type_name == BlockType::Composite {
                    object.color_index = Some(self.next_color_index);
                    self.next_color_index += 1;
                }
                let index = self.push_object(object);
                self.set_key(id.clone(), index);
                index
            }
            Some(existing) => {
                if object.type_name != BlockType::Na {
                    self.objects[existing].type_name = object.type_name;
                }
                if object.label.as_deref() != Some(id.as_str()) {
                    self.objects[existing].label = object.label.clone();
                }
                // Upstream still recurses into the new object, which is not in the map, so
                // its children are added to the database without a parent that reaches them.
                self.push_object(object)
            }
        };
        if let Some(nested) = nested {
            self.populate(nested, index)?;
        }
        if self.objects[index].type_name == BlockType::Space {
            let width = self.objects[index].width.unwrap_or(1);
            for copy in 0..width {
                let mut clone = self.objects[index].clone();
                let clone_id = format!("{id}-{copy}");
                clone.id = clone_id.clone();
                let clone_index = self.push_object(clone);
                self.set_key(clone_id, clone_index);
                children.push(clone_index);
            }
        } else if existing.is_none() {
            children.push(index);
        }
        Ok(())
    }

    /// `addStyleClass`: styles split on `,`, the first `;` dropped, and the attributes that
    /// mention `color` renamed for text styling too.
    fn add_style_class(&mut self, id: &str, attributes: &str) {
        let index = match self.class_index.get(id).copied() {
            Some(index) => index,
            None => {
                self.classes.push(ClassDef {
                    id: id.to_string(),
                    ..ClassDef::default()
                });
                let index = self.classes.len() - 1;
                self.class_index.insert(id.to_string(), index);
                index
            }
        };
        for attribute in attributes.split(',') {
            let fixed = attribute.replacen(';', "", 1).trim().to_string();
            if attribute.contains("color") {
                let text_style = fixed
                    .replacen("fill", "bgFill", 1)
                    .replacen("color", "fill", 1);
                self.classes[index].text_styles.push(text_style);
            }
            self.classes[index].styles.push(fixed);
        }
    }

    /// `addStyle2Node`: an unknown id is an error, because upstream writes through
    /// `undefined` and throws.
    fn add_style2_node(&mut self, id: &str, styles: &str, span: Span) -> Result<(), ParseError> {
        let Some(index) = self.index.get(id).copied() else {
            return Err(ParseError::at(
                format!("cannot set styles on unknown block '{id}'"),
                span,
            ));
        };
        self.objects[index].styles = styles.split(',').map(str::to_string).collect();
        Ok(())
    }

    /// `setCssClass`: ids split on `,`, a lookup by the raw segment but a placeholder stored
    /// under its trimmed id.
    fn set_css_class(&mut self, item_ids: &str, style_class: &str) {
        for id in item_ids.split(',') {
            let index = match self.index.get(id).copied() {
                Some(index) => index,
                None => {
                    let trimmed = id.trim().to_string();
                    let index = self.push_object(Object {
                        id: trimmed.clone(),
                        type_name: BlockType::Na,
                        children: Some(Vec::new()),
                        ..Object::default()
                    });
                    self.set_key(trimmed, index);
                    index
                }
            };
            self.objects[index].classes.push(style_class.to_string());
        }
    }

    /// The `edge` branch: number the edge per start/end pair and move it out of the blocks.
    fn add_edge(&mut self, edge: &EdgeValue, label: String) {
        let count = self.edge_count.get(&edge.id).copied().unwrap_or(0) + 1;
        self.edge_count.insert(edge.id.clone(), count);
        self.edges.push(Edge {
            id: format!("{count}-{}", edge.id),
            start: edge.start.clone().unwrap_or_default(),
            end: edge.end.clone().unwrap_or_default(),
            label,
            thickness: edge.thickness.clone(),
            pattern: edge.pattern.clone(),
            arrow_type_start: edge.arrow_type_start.clone(),
            arrow_type_end: edge.arrow_type_end.clone(),
            directions: edge.directions.clone(),
        });
    }

    fn materialize(&self, index: usize) -> Block {
        let object = &self.objects[index];
        Block {
            id: object.id.clone(),
            label: object.label.clone().unwrap_or_default(),
            type_name: object.type_name,
            columns: object.columns,
            width_in_columns: object.width_in_columns,
            width: object.width,
            classes: object.classes.clone(),
            directions: object.directions.clone(),
            styles: object.styles.clone(),
            color_index: object.color_index,
            children: object.children.as_ref().map(|children| {
                children
                    .iter()
                    .map(|&child| self.materialize(child))
                    .collect()
            }),
        }
    }

    fn into_ast(self) -> Ast {
        let blocks = self.objects[self.root]
            .children
            .as_ref()
            .map(|children| {
                children
                    .iter()
                    .map(|&child| self.materialize(child))
                    .collect()
            })
            .unwrap_or_default();
        let flat = self
            .keys
            .iter()
            .map(|key| {
                let index = *self.index.get(key).expect("every key maps to an object");
                self.materialize(index)
            })
            .collect();
        Ast {
            blocks,
            flat,
            edges: self.edges,
            classes: self.classes,
            warnings: self.warnings,
        }
    }
}

/// The object a `nodeStatement` names, before the database fills in what it inherits.
fn block_object(node: &NodeValue, label: Option<String>) -> Object {
    Object {
        id: node.id.clone().unwrap_or_default(),
        label,
        type_name: node.type_name.unwrap_or(BlockType::Na),
        directions: node.directions.clone(),
        width_in_columns: node.width_in_columns,
        ..Object::default()
    }
}

fn statement_width(statement: &Statement) -> Option<i64> {
    match statement {
        Statement::Node(node) => node.width_in_columns,
        Statement::Composite { node, .. } => node.width_in_columns,
        _ => None,
    }
}

fn statement_id(statement: &Statement) -> Option<&str> {
    match statement {
        Statement::Node(node) => node.id.as_deref(),
        Statement::Composite { node, .. } => node.id.as_deref(),
        _ => None,
    }
}

/// `if (block.label) block.label = sanitizeText(block.label)`.
fn statement_label(statement: &Statement) -> Option<String> {
    statement.label().map(|label| {
        if label.is_empty() {
            String::new()
        } else {
            sanitize::sanitize_text(label)
        }
    })
}
