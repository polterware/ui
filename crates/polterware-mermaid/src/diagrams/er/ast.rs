//! The ER database: the state upstream's `ErDB` holds once its parser has run, read through
//! the db's getters (`getEntities`, `getEntity`, `getRelationships`, `getClasses`,
//! `getSubGraphs`, `getDirection`, `getAccTitle`, `getAccDescription`, `getDiagramTitle`).
//!
//! Upstream stores entities and classes in `Map`s keyed by name; insertion order is the
//! source order, so they are `Vec`s here with lookup helpers. Renderer configuration
//! (`getData().config`, the `look` field) belongs to the caller and is not kept.

/// A cardinality, one end of a relationship (`ErDB.Cardinality`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cardinality {
    ZeroOrOne,
    ZeroOrMore,
    OneOrMore,
    OnlyOne,
    /// The parent end of a markdown-style relationship (`u--o{`).
    MdParent,
}

impl Cardinality {
    /// The upstream constant name, e.g. `ZERO_OR_ONE`.
    pub fn as_str(self) -> &'static str {
        match self {
            Cardinality::ZeroOrOne => "ZERO_OR_ONE",
            Cardinality::ZeroOrMore => "ZERO_OR_MORE",
            Cardinality::OneOrMore => "ONE_OR_MORE",
            Cardinality::OnlyOne => "ONLY_ONE",
            Cardinality::MdParent => "MD_PARENT",
        }
    }

    /// The name `getData` writes into `arrowTypeStart`/`arrowTypeEnd`.
    pub fn as_arrow_type(self) -> &'static str {
        match self {
            Cardinality::ZeroOrOne => "zero_or_one",
            Cardinality::ZeroOrMore => "zero_or_more",
            Cardinality::OneOrMore => "one_or_more",
            Cardinality::OnlyOne => "only_one",
            Cardinality::MdParent => "md_parent",
        }
    }
}

/// Whether a relationship is identifying (solid) or not (dashed)
/// (`ErDB.Identification`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Identification {
    NonIdentifying,
    Identifying,
}

impl Identification {
    /// The upstream constant name, e.g. `IDENTIFYING`.
    pub fn as_str(self) -> &'static str {
        match self {
            Identification::NonIdentifying => "NON_IDENTIFYING",
            Identification::Identifying => "IDENTIFYING",
        }
    }

    /// The `pattern` `getData` writes for the edge.
    pub fn arrow_pattern(self) -> &'static str {
        match self {
            Identification::Identifying => "solid",
            Identification::NonIdentifying => "dashed",
        }
    }
}

/// How a label's text is interpreted (`labelType`), as `sanitizeNodeLabelType` allows it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LabelType {
    Text,
    String,
    #[default]
    Markdown,
}

impl LabelType {
    pub fn as_str(self) -> &'static str {
        match self {
            LabelType::Text => "text",
            LabelType::String => "string",
            LabelType::Markdown => "markdown",
        }
    }
}

/// One attribute of an entity (`Attribute`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Attribute {
    /// The declared type (upstream `type`), including a `?` suffix and a generic
    /// `type~T~` part when present.
    pub type_name: String,
    pub name: String,
    /// The declared keys, as written (`PK`, `FK`, `UK`).
    pub keys: Vec<String>,
    /// The comment without its surrounding quotes; empty when there is none.
    pub comment: String,
}

/// One entity of the `entities` map (`EntityNode`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Entity {
    /// The generated id, `entity-<name>-<index>`.
    pub id: String,
    /// The map key and the drawn name.
    pub label: String,
    pub attributes: Vec<Attribute>,
    /// The alias from `name[alias]`; empty when none was given.
    pub alias: String,
    /// Always `erBox` (the layout shape).
    pub shape: String,
    /// The CSS classes, starting with `default`.
    pub css_classes: String,
    /// The styles a `style` statement applied.
    pub css_styles: Vec<String>,
    pub label_type: LabelType,
}

/// The cardinalities and type of a relationship (`RelSpec`).
///
/// The grammar assigns the cardinality written *second* to `card_a` and the one written
/// first to `card_b` (`relSpec: cardinality relType cardinality`), because the arrow heads
/// are read from the ends of the line inward.
#[derive(Clone, Debug, PartialEq)]
pub struct RelSpec {
    pub card_a: Cardinality,
    pub card_b: Cardinality,
    pub rel_type: Identification,
}

/// One relationship (`Relationship`), in source order.
#[derive(Clone, Debug, PartialEq)]
pub struct Relationship {
    /// The first entity's id, or the first subgraph's id.
    pub entity_a: String,
    /// The role label written after the colon.
    pub role_a: String,
    /// The second entity's id, or the second subgraph's id.
    pub entity_b: String,
    pub rel_spec: RelSpec,
}

/// One `classDef` entry (`EntityClass`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EntityClass {
    pub id: String,
    /// The styles, trimmed.
    pub styles: Vec<String>,
    /// The styles that mention `color`, with `fill` rewritten to `bgFill`.
    pub text_styles: Vec<String>,
}

/// One `subgraph ... end` block (`ErSubGraph`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SubGraph {
    pub id: String,
    /// The nodes the block listed, trimmed, unique, and without nodes an earlier
    /// subgraph already owns. A nested `subgraph` contributes its own id as a node.
    pub nodes: Vec<String>,
    pub title: String,
    pub classes: Vec<String>,
    pub css_styles: Vec<String>,
    /// The direction declared inside the block, when it had one.
    pub dir: Option<String>,
    pub label_type: LabelType,
}

/// The parsed diagram.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ast {
    /// The `entities` map, in insertion order.
    pub entities: Vec<Entity>,
    /// `getRelationships`.
    pub relationships: Vec<Relationship>,
    /// The `classes` map, in insertion order.
    pub classes: Vec<EntityClass>,
    /// `getSubGraphs`: inner blocks come before the blocks that contain them.
    pub sub_graphs: Vec<SubGraph>,
    /// `getDirection`: `TB` until a `direction` statement inside no subgraph changes it.
    pub direction: String,
    /// `getAccTitle`.
    pub acc_title: String,
    /// `getAccDescription`.
    pub acc_description: String,
    /// `getDiagramTitle`; the ER grammar has no `title` token, so it stays empty.
    pub diagram_title: String,
}

impl Ast {
    /// The entity with this name (`getEntity`).
    pub fn entity(&self, name: &str) -> Option<&Entity> {
        self.entities.iter().find(|entity| entity.label == name)
    }

    /// The class definition with this id (`getClasses().get`).
    pub fn class(&self, id: &str) -> Option<&EntityClass> {
        self.classes.iter().find(|class| class.id == id)
    }

    /// The subgraph with this id; the last declaration wins, as upstream's
    /// `subGraphLookup` map does (`getSubGraphs` keeps every declaration).
    pub fn sub_graph(&self, id: &str) -> Option<&SubGraph> {
        self.sub_graphs.iter().rev().find(|sub| sub.id == id)
    }

    /// The layout-facing projection `getData` returns, minus the configuration values
    /// (`config`, `look`) and the `other` bag, which are not part of the parse.
    pub fn data(&self) -> Data {
        let mut parent = Vec::new();
        for sub_graph in self.sub_graphs.iter().rev() {
            for node in &sub_graph.nodes {
                upsert(&mut parent, node.clone(), sub_graph.id.clone());
            }
        }
        let mut nodes = Vec::new();
        for sub_graph in self.sub_graphs.iter().rev() {
            nodes.push(Node {
                id: sub_graph.id.clone(),
                label: sub_graph.title.clone(),
                label_style: String::new(),
                label_type: sub_graph.label_type,
                parent_id: parent_of(&parent, &sub_graph.id),
                padding: 8.0,
                css_compiled_styles: self
                    .compiled_styles(sub_graph.classes.iter().map(String::as_str)),
                css_styles: sub_graph.css_styles.clone(),
                css_classes: sub_graph.classes.join(" "),
                shape: "rect".to_string(),
                dir: sub_graph.dir.clone(),
                is_group: true,
                ..Node::default()
            });
        }
        let mut color_index = 0;
        for entity in &self.entities {
            if self.sub_graphs.iter().any(|sub| sub.id == entity.label) {
                continue;
            }
            nodes.push(Node {
                id: entity.id.clone(),
                label: entity.label.clone(),
                label_type: entity.label_type,
                parent_id: parent_of(&parent, &entity.label),
                css_compiled_styles: self.compiled_styles(entity.css_classes.split(' ')),
                css_styles: entity.css_styles.clone(),
                css_classes: entity.css_classes.clone(),
                shape: entity.shape.clone(),
                attributes: entity.attributes.clone(),
                alias: entity.alias.clone(),
                color_index: Some(color_index),
                ..Node::default()
            });
            color_index += 1;
        }
        let edges = self
            .relationships
            .iter()
            .enumerate()
            .map(|(counter, relationship)| LayoutEdge {
                id: format!(
                    "id_{}_{}_{}",
                    relationship.entity_a, relationship.entity_b, counter
                ),
                type_name: "normal".to_string(),
                curve: "basis".to_string(),
                start: relationship.entity_a.clone(),
                end: relationship.entity_b.clone(),
                label: relationship.role_a.clone(),
                labelpos: "c".to_string(),
                thickness: "normal".to_string(),
                classes: "relationshipLine".to_string(),
                arrow_type_start: relationship.rel_spec.card_b,
                arrow_type_end: relationship.rel_spec.card_a,
                pattern: relationship.rel_spec.rel_type.arrow_pattern().to_string(),
                label_type: LabelType::Markdown,
            })
            .collect();
        Data {
            nodes,
            edges,
            direction: self.direction.clone(),
        }
    }

    /// `getCompiledStyles`: for each class name, its styles then its text styles, each
    /// trimmed.
    fn compiled_styles<'a>(&self, class_names: impl IntoIterator<Item = &'a str>) -> Vec<String> {
        let mut compiled = Vec::new();
        for name in class_names {
            if let Some(class) = self.class(name) {
                compiled.extend(
                    class
                        .styles
                        .iter()
                        .chain(class.text_styles.iter())
                        .map(|style| style.trim().to_string()),
                );
            }
        }
        compiled
    }
}

fn upsert(parents: &mut Vec<(String, String)>, node: String, parent: String) {
    match parents.iter_mut().find(|(key, _)| key == &node) {
        Some(entry) => entry.1 = parent,
        None => parents.push((node, parent)),
    }
}

fn parent_of(parents: &[(String, String)], node: &str) -> Option<String> {
    parents
        .iter()
        .find(|(key, _)| key == node)
        .map(|(_, parent)| parent.clone())
}

/// What `getData` hands the layout engine (`getData().nodes`).
///
/// A cluster node (a subgraph) carries `label_style`, `padding`, `shape = "rect"`, `dir`,
/// and `is_group = true`; an entity node carries `attributes`, `alias`, `shape = "erBox"`,
/// `color_index`, and `is_group = false`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Node {
    /// The subgraph id or the entity id.
    pub id: String,
    /// The subgraph title or the entity name.
    pub label: String,
    /// Cluster nodes: the empty `labelStyle` upstream writes; entity nodes have none.
    pub label_style: String,
    pub label_type: LabelType,
    /// The owning subgraph, when the node is listed by one.
    pub parent_id: Option<String>,
    /// Cluster nodes: `8`. Entity nodes carry no padding upstream.
    pub padding: f64,
    /// The styles of every class the node names, trimmed.
    pub css_compiled_styles: Vec<String>,
    pub css_styles: Vec<String>,
    pub css_classes: String,
    pub shape: String,
    /// Cluster nodes only.
    pub dir: Option<String>,
    pub is_group: bool,
    /// Entity nodes only.
    pub attributes: Vec<Attribute>,
    /// Entity nodes only.
    pub alias: String,
    /// Entity nodes only: the order the entity was emitted in.
    pub color_index: Option<usize>,
}

/// What `getData` hands the layout engine (`getData().edges`).
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutEdge {
    /// `id_<entityA>_<entityB>_<counter>`.
    pub id: String,
    /// Always `normal` (upstream's `type`).
    pub type_name: String,
    /// Always `basis`.
    pub curve: String,
    pub start: String,
    pub end: String,
    pub label: String,
    /// Always `c`.
    pub labelpos: String,
    /// Always `normal`.
    pub thickness: String,
    /// Always `relationshipLine`.
    pub classes: String,
    /// The first cardinality, lowercased.
    pub arrow_type_start: Cardinality,
    /// The second cardinality, lowercased.
    pub arrow_type_end: Cardinality,
    /// `solid` for identifying relationships, `dashed` otherwise.
    pub pattern: String,
    /// Always `markdown`.
    pub label_type: LabelType,
}

/// The `getData` result.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Data {
    pub nodes: Vec<Node>,
    pub edges: Vec<LayoutEdge>,
    pub direction: String,
}
