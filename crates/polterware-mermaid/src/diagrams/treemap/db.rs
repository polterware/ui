//! The treemap database's writing side, ported from `db.ts`, `utils.ts`'s `buildHierarchy`,
//! the `populate` function of `parser.ts`, and `diagrams/common/populateCommonDb.ts`.
//!
//! `parser.ts` parses the whole document first and then populates the database. The order of
//! the writes is the one `populate` uses: the accessibility fields and the title, then every
//! `classDef` (the first loop, so a class is known before any node references it), then every
//! node built from the rows, in row order. Finally `addNodesRecursively` walks the hierarchy
//! depth first from level 0, which is the order of `getNodes()` and `levels`.

use super::ast::{Ast, StyleClass, TreemapNode};
use super::parser::{Item, ItemKind, ParsedRow, ParsedTreemap};
use crate::common::sanitize;

/// `populate`: applies the parsed document to a fresh database.
pub(crate) fn build(parsed: ParsedTreemap) -> Ast {
    let mut db = Db::new();
    if let Some(value) = parsed.acc_description.filter(|value| !value.is_empty()) {
        db.set_acc_description(&value);
    }
    if let Some(value) = parsed.acc_title.filter(|value| !value.is_empty()) {
        db.set_acc_title(&value);
    }
    if let Some(value) = parsed.title.filter(|value| !value.is_empty()) {
        db.set_diagram_title(&value);
    }
    for row in &parsed.rows {
        if let ParsedRow::ClassDef {
            class_name,
            style_text,
        } = row
        {
            db.add_class(class_name, style_text);
        }
    }
    let mut items = Vec::new();
    for row in parsed.rows {
        if let ParsedRow::Item { indent, item } = row {
            let level = indent.unwrap_or(0);
            items.push(db.hierarchy_item(level, item));
        }
    }
    let roots = build_hierarchy(items);
    let mut nodes = Vec::new();
    let mut levels = Vec::new();
    collect(&roots, 0, &mut nodes, &mut levels);
    Ast {
        nodes,
        levels,
        root: TreemapNode {
            name: String::new(),
            children: Some(roots),
            ..TreemapNode::default()
        },
        classes: db.classes,
        diagram_title: db.diagram_title,
        acc_title: db.acc_title,
        acc_description: db.acc_description,
    }
}

/// One row's data as `populate` hands it to `buildHierarchy`.
pub(crate) struct HierarchyItem {
    pub(crate) level: usize,
    pub(crate) name: String,
    pub(crate) kind: ItemKind,
    pub(crate) value: Option<f64>,
    pub(crate) class_selector: Option<String>,
    pub(crate) css_compiled_styles: Option<Vec<String>>,
}

/// `buildHierarchy`: folds the flat rows into trees by comparing their levels.
///
/// The stack holds the sections that can still take children. A row pops every section whose
/// level is at least its own, then joins the new top of the stack as a child, or the roots when
/// the stack is empty. Only sections join the stack, so a leaf never takes children.
pub(crate) fn build_hierarchy(items: Vec<HierarchyItem>) -> Vec<TreemapNode> {
    let mut arena: Vec<Option<TreemapNode>> = items
        .iter()
        .map(|item| {
            let leaf = item.kind == ItemKind::Leaf;
            Some(TreemapNode {
                name: item.name.clone(),
                children: if leaf { None } else { Some(Vec::new()) },
                value: if leaf { item.value } else { None },
                class_selector: item.class_selector.clone(),
                css_compiled_styles: item.css_compiled_styles.clone(),
            })
        })
        .collect();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); items.len()];
    let mut roots = Vec::new();
    let mut stack: Vec<(usize, usize)> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        while let Some(&(_, level)) = stack.last() {
            if level >= item.level {
                stack.pop();
            } else {
                break;
            }
        }
        if let Some(&(parent, _)) = stack.last() {
            children[parent].push(index);
        } else {
            roots.push(index);
        }
        if item.kind != ItemKind::Leaf {
            stack.push((index, item.level));
        }
    }
    roots
        .into_iter()
        .map(|index| take_node(index, &mut arena, &children))
        .collect()
}

/// Materializes one arena entry, pulling its children in first.
fn take_node(
    index: usize,
    arena: &mut [Option<TreemapNode>],
    children: &[Vec<usize>],
) -> TreemapNode {
    let mut node = arena[index].take().expect("each node is materialized once");
    if node.children.is_some() {
        let taken = children[index]
            .iter()
            .map(|&child| take_node(child, arena, children))
            .collect();
        node.children = Some(taken);
    }
    node
}

/// `addNodesRecursively`: the depth-first `addNode` order.
fn collect(
    nodes: &[TreemapNode],
    level: usize,
    flat: &mut Vec<TreemapNode>,
    levels: &mut Vec<usize>,
) {
    for node in nodes {
        flat.push(node.clone());
        levels.push(level);
        if let Some(children) = &node.children {
            collect(children, level + 1, flat, levels);
        }
    }
}

/// The module state `db.ts` keeps for the fields this port exposes.
struct Db {
    classes: Vec<StyleClass>,
    diagram_title: String,
    acc_title: String,
    acc_description: String,
}

impl Db {
    fn new() -> Self {
        Self {
            classes: Vec::new(),
            diagram_title: String::new(),
            acc_title: String::new(),
            acc_description: String::new(),
        }
    }

    /// `setDiagramTitle`: `sanitizeText(txt)`.
    fn set_diagram_title(&mut self, title: &str) {
        self.diagram_title = sanitize::sanitize_text(title);
    }

    /// `setAccTitle`: `sanitizeText(txt).replace(/^\s+/g, '')`.
    fn set_acc_title(&mut self, title: &str) {
        self.acc_title = sanitize::set_acc_title(title);
    }

    /// `setAccDescription`: `sanitizeText(txt).replace(/\n\s+/g, '\n')`.
    fn set_acc_description(&mut self, description: &str) {
        self.acc_description = sanitize::set_acc_description(description);
    }

    /// `addClass`: appends each style to the class, creating it on first use.
    fn add_class(&mut self, id: &str, style: &str) {
        let index = match self.classes.iter().position(|class| class.id == id) {
            Some(index) => index,
            None => {
                self.classes.push(StyleClass {
                    id: id.to_string(),
                    styles: Vec::new(),
                    text_styles: Vec::new(),
                });
                self.classes.len() - 1
            }
        };
        for style in split_styles(style) {
            if is_label_style(&style) {
                self.classes[index].text_styles.push(style.clone());
            }
            self.classes[index].styles.push(style);
        }
    }

    /// `getStylesForClass`.
    fn styles_for_class(&self, class_selector: &str) -> &[String] {
        self.classes
            .iter()
            .find(|class| class.id == class_selector)
            .map_or(&[], |class| class.styles.as_slice())
    }

    /// The fields `populate` copies out of one row.
    fn hierarchy_item(&self, level: usize, item: Item) -> HierarchyItem {
        let css_compiled_styles = item
            .class_selector
            .as_deref()
            .map(|selector| self.styles_for_class(selector).to_vec())
            .filter(|styles| !styles.is_empty());
        HierarchyItem {
            level,
            name: item.name,
            kind: item.kind,
            value: item.value,
            class_selector: item.class_selector,
            css_compiled_styles,
        }
    }
}

/// `_style.replace(/\\,/g, '§§§').replace(/,/g, ';').replace(/§§§/g, ',').split(';')`.
///
/// The escaped commas come back as commas inside one style, and the rest separate styles.
/// `split` always yields at least one entry, so an empty style becomes `[""]`.
fn split_styles(style: &str) -> Vec<String> {
    const PLACEHOLDER: &str = "\u{a7}\u{a7}\u{a7}";
    style
        .replace("\\,", PLACEHOLDER)
        .replace(',', ";")
        .replace(PLACEHOLDER, ",")
        .split(';')
        .map(str::to_string)
        .collect()
}

/// `isLabelStyle` from `rendering-elements/shapes/handDrawnShapeStyles.ts`, which compares the
/// whole style string against these keys.
fn is_label_style(style: &str) -> bool {
    matches!(
        style,
        "color"
            | "font-size"
            | "font-family"
            | "font-weight"
            | "font-style"
            | "text-decoration"
            | "text-align"
            | "text-transform"
            | "line-height"
            | "letter-spacing"
            | "word-spacing"
            | "text-shadow"
            | "text-overflow"
            | "white-space"
            | "word-wrap"
            | "word-break"
            | "overflow-wrap"
            | "hyphens"
    )
}
