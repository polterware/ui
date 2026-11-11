//! The semantics of upstream `erDb.ts`: `addEntity`, `addAttributes`, `addRelationship`,
//! `addSubGraph`, `addClass`, `addCssStyles`, `setClass`, the subgraph depth flag, and the
//! accessibility setters, applied by the parser as it reduces each statement.
//!
//! `sanitizeText` (DOMPurify) is not applied; see the module docs in [`super`].

use super::ast::{Ast, Attribute, Entity, EntityClass, LabelType, RelSpec, Relationship, SubGraph};
use crate::common::sanitize::collapse_description_indent;

/// A value a `document` collected: what upstream's grammar concatenates into the list
/// `addSubGraph` receives.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Item {
    /// A statement's node name(s), or the string a `classDef`/`class`/`style`/accessibility
    /// statement returns (upstream assigns a token's text to `$$`).
    Node(String),
    /// A `direction` declared inside a subgraph, which its block keeps as `dir`.
    Dir(String),
}

pub(crate) struct Db {
    entities: Vec<Entity>,
    relationships: Vec<Relationship>,
    classes: Vec<EntityClass>,
    sub_graphs: Vec<SubGraph>,
    /// Upstream's `subCount`: only the never-taken id fallback reads it.
    sub_count: usize,
    direction: String,
    acc_title: String,
    acc_description: String,
    diagram_title: String,
}

impl Default for Db {
    fn default() -> Self {
        Self::new()
    }
}

impl Db {
    pub(crate) fn new() -> Self {
        Self {
            entities: Vec::new(),
            relationships: Vec::new(),
            classes: Vec::new(),
            sub_graphs: Vec::new(),
            sub_count: 0,
            direction: "TB".to_string(),
            acc_title: String::new(),
            acc_description: String::new(),
            diagram_title: String::new(),
        }
    }

    /// `addEntity`: creates the entity with the next generated id, or fills in an alias an
    /// existing entity does not have yet.
    pub(crate) fn add_entity(&mut self, name: &str, alias: &str) -> usize {
        if let Some(index) = self.entity_index(name) {
            if self.entities[index].alias.is_empty() && !alias.is_empty() {
                self.entities[index].alias = alias.to_string();
            }
            return index;
        }
        let entity = Entity {
            id: format!("entity-{name}-{}", self.entities.len()),
            label: name.to_string(),
            attributes: Vec::new(),
            alias: alias.to_string(),
            shape: "erBox".to_string(),
            css_classes: "default".to_string(),
            css_styles: Vec::new(),
            label_type: LabelType::Markdown,
        };
        self.entities.push(entity);
        self.entities.len() - 1
    }

    /// `addAttributes`: appends the attributes in source order. Upstream reverses the
    /// grammar's list because its `attributes` production is right-recursive; the parser
    /// already builds the list in source order, so the two reversals cancel out.
    pub(crate) fn add_attributes(&mut self, name: &str, attributes: Vec<Attribute>) {
        let index = self.add_entity(name, "");
        self.entities[index].attributes.extend(attributes);
    }

    /// `addRelationship`: a name that matches a subgraph stays the subgraph's id; every
    /// other name is (or becomes) an entity and contributes its generated id.
    pub(crate) fn add_relationship(
        &mut self,
        ent_a: &str,
        role_a: &str,
        ent_b: &str,
        rel_spec: RelSpec,
    ) {
        let entity_a = match self.subgraph_index(ent_a) {
            Some(_) => ent_a.to_string(),
            None => {
                let index = self.add_entity(ent_a, "");
                self.entities[index].id.clone()
            }
        };
        let entity_b = match self.subgraph_index(ent_b) {
            Some(_) => ent_b.to_string(),
            None => {
                let index = self.add_entity(ent_b, "");
                self.entities[index].id.clone()
            }
        };
        self.relationships.push(Relationship {
            entity_a,
            role_a: role_a.to_string(),
            entity_b,
            rel_spec,
        });
    }

    /// `addSubGraph`: builds the block from the names its body collected.
    ///
    /// `label_type` is the grammar's `_title.type`, which is always absent (the `subgraph`
    /// rule carries only `{ id, text }`), so it resolves to `markdown`.
    pub(crate) fn add_sub_graph(
        &mut self,
        id: &str,
        title: &str,
        items: &[Item],
        label_type: Option<&str>,
    ) -> String {
        let id = id.trim().to_string();
        let mut seen: Vec<String> = Vec::new();
        let mut nodes: Vec<String> = Vec::new();
        let mut dir = None;
        for item in items {
            match item {
                Item::Dir(value) => dir = Some(value.clone()),
                Item::Node(node) => {
                    let trimmed = node.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    if seen.iter().any(|seen| seen == trimmed) {
                        continue;
                    }
                    seen.push(trimmed.to_string());
                    nodes.push(node.clone());
                }
            }
        }
        let mut sub_graph = SubGraph {
            id: id.clone(),
            nodes,
            title: title.trim().to_string(),
            classes: Vec::new(),
            css_styles: Vec::new(),
            dir,
            label_type: sanitize_label_type(label_type),
        };
        // `makeUniq`: a node another subgraph already listed is dropped from this one.
        let owned: Vec<String> = self
            .sub_graphs
            .iter()
            .flat_map(|sub| sub.nodes.iter().cloned())
            .collect();
        sub_graph
            .nodes
            .retain(|node| !owned.iter().any(|owned| owned == node));
        self.sub_count += 1;
        self.sub_graphs.push(sub_graph);
        id
    }

    /// `addClass`: creates the class when needed and appends its styles.
    pub(crate) fn add_class(&mut self, ids: &[String], styles: &[String]) {
        for id in ids {
            let index = match self.classes.iter().position(|class| &class.id == id) {
                Some(index) => index,
                None => {
                    self.classes.push(EntityClass {
                        id: id.clone(),
                        styles: Vec::new(),
                        text_styles: Vec::new(),
                    });
                    self.classes.len() - 1
                }
            };
            for style in styles {
                if style.contains("color") {
                    self.classes[index]
                        .text_styles
                        .push(style.replacen("fill", "bgFill", 1));
                }
                self.classes[index].styles.push(style.clone());
            }
        }
    }

    /// `addCssStyles`: a `style` statement's styles, on the entities and subgraphs it names.
    pub(crate) fn add_css_styles(&mut self, ids: &[String], styles: &[String]) {
        for id in ids {
            if let Some(index) = self.entity_index(id) {
                self.entities[index]
                    .css_styles
                    .extend(styles.iter().cloned());
            }
            if let Some(index) = self.subgraph_index(id) {
                self.sub_graphs[index]
                    .css_styles
                    .extend(styles.iter().cloned());
            }
        }
    }

    /// `setClass`: appends the class names to the entities and subgraphs it names.
    pub(crate) fn set_class(&mut self, ids: &[String], class_names: &[String]) {
        for id in ids {
            if let Some(index) = self.entity_index(id) {
                for class_name in class_names {
                    self.entities[index].css_classes.push(' ');
                    self.entities[index].css_classes.push_str(class_name);
                }
            }
            if let Some(index) = self.subgraph_index(id) {
                self.sub_graphs[index]
                    .classes
                    .extend(class_names.iter().cloned());
            }
        }
    }

    pub(crate) fn set_direction(&mut self, direction: &str) {
        self.direction = direction.to_string();
    }

    /// `setAccTitle`: strips the leading whitespace the common db strips too.
    pub(crate) fn set_acc_title(&mut self, title: &str) {
        self.acc_title = title.trim_start().to_string();
    }

    /// `setAccDescription`: `sanitizeText(txt).replace(/\n\s+/g, '\n')`.
    pub(crate) fn set_acc_description(&mut self, description: &str) {
        self.acc_description = collapse_description_indent(description);
    }

    pub(crate) fn into_ast(self) -> Ast {
        Ast {
            entities: self.entities,
            relationships: self.relationships,
            classes: self.classes,
            sub_graphs: self.sub_graphs,
            direction: self.direction,
            acc_title: self.acc_title,
            acc_description: self.acc_description,
            diagram_title: self.diagram_title,
        }
    }

    fn entity_index(&self, name: &str) -> Option<usize> {
        self.entities.iter().position(|entity| entity.label == name)
    }

    /// `subGraphLookup.get`: the last declaration of an id wins.
    fn subgraph_index(&self, id: &str) -> Option<usize> {
        self.sub_graphs.iter().rposition(|sub| sub.id == id)
    }
}

fn sanitize_label_type(label_type: Option<&str>) -> LabelType {
    match label_type {
        Some("text") => LabelType::Text,
        Some("string") => LabelType::String,
        _ => LabelType::Markdown,
    }
}
