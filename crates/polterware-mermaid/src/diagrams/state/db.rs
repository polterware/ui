//! The database semantics of `stateDb.ts` and `dataFetcher.ts`, applied while the parser's
//! document is reduced, exactly in upstream's order: `setRootDoc` translates the document
//! (`docTranslator`), `extract` walks the scaled statements into states, relations, classes
//! and links, and `dataFetcher` derives the layout nodes and edges that `getData` returns.
//!
//! `getData` reads the run-time `config` for `look`, `state.wrappingWidth` and
//! `state.minNodeWidth`; this crate owns no config, so those fields are absent from
//! [`Node`]/[`Edge`] and the edge arrow type uses the default look (`neo`), `arrow_barb_neo`.
//! The note group's and the note's padding use the documented defaults of the flowchart and
//! state configs (`15` and `16`). `docTranslator`'s trailing concurrency region gets an
//! `id-<n>` id instead of upstream's random `generateId()` id, so parses stay reproducible;
//! nothing keys off that id.

use std::collections::HashMap;

use super::ast::{
    Ast, Direction, Edge, EdgePattern, Link, Node, Note, NotePosition, Relation, RelationStatement,
    State, StateKind, StateStatement, Statement, StyleClass,
};
use crate::common::sanitize;
use crate::error::ParseError;

const SHAPE_STATE: &str = "rect";
const SHAPE_STATE_WITH_DESC: &str = "rectWithTitle";
const SHAPE_START: &str = "stateStart";
const SHAPE_END: &str = "stateEnd";
const SHAPE_DIVIDER: &str = "divider";
const SHAPE_GROUP: &str = "roundedWithTitle";
const SHAPE_NOTE: &str = "note";
const SHAPE_NOTEGROUP: &str = "noteGroup";

const CSS_DIAGRAM_STATE: &str = "statediagram-state";
const CSS_DIAGRAM_CLUSTER: &str = "statediagram-cluster";
const CSS_DIAGRAM_CLUSTER_ALT: &str = "statediagram-cluster-alt";
const CSS_DIAGRAM_NOTE: &str = "statediagram-note";
const CSS_EDGE: &str = "transition";
const CSS_EDGE_NOTE_EDGE: &str = "transition note-edge";

const G_EDGE_STYLE: &str = "fill:none";
const G_EDGE_ARROWHEADSTYLE: &str = "fill: #333";
const G_EDGE_LABELPOS: &str = "c";
const G_EDGE_LABELTYPE: &str = "markdown";
const G_EDGE_THICKNESS: &str = "normal";

/// The node's literal `8`, `flowchart.padding` for a note, and the note group's literal
/// `16`.
const NODE_PADDING: f64 = 8.0;
const NOTE_PADDING: f64 = 15.0;
const NOTE_GROUP_PADDING: f64 = 16.0;
const NODE_RX: f64 = 10.0;

const START_NODE: &str = "[*]";
const DIAGRAM_DIRECTION_DEFAULT: Direction = Direction::Tb;

/// A label as upstream holds it before `extract`'s last pass: a string or an array.
#[derive(Clone, Debug)]
enum NodeLabel {
    Text(String),
    Lines(Vec<String>),
}

impl NodeLabel {
    /// The JavaScript `.length`: characters for a string, entries for an array.
    fn len(&self) -> usize {
        match self {
            NodeLabel::Text(text) => text.chars().count(),
            NodeLabel::Lines(lines) => lines.len(),
        }
    }

    fn sanitized(self) -> NodeLabel {
        match self {
            NodeLabel::Text(text) => NodeLabel::Text(sanitize::sanitize_text(&text)),
            NodeLabel::Lines(lines) => NodeLabel::Lines(
                lines
                    .iter()
                    .map(|line| sanitize::sanitize_text(line))
                    .collect(),
            ),
        }
    }
}

/// Upstream's `NodeData` while the fetcher still builds it.
#[derive(Clone, Debug)]
struct NodeData {
    id: String,
    shape: Option<String>,
    label: NodeLabel,
    css_classes: String,
    css_compiled_styles: Vec<String>,
    css_styles: Vec<String>,
    dir: Option<Direction>,
    dom_id: String,
    node_type: Option<String>,
    is_group: bool,
    color_index: Option<u32>,
    padding: Option<f64>,
    rx: Option<f64>,
    ry: Option<f64>,
    center_label: Option<bool>,
    parent_id: Option<String>,
    position: Option<NotePosition>,
    label_style: Option<String>,
    label_type: Option<String>,
    description: Vec<String>,
}

/// The record the fetcher keeps per state id (`nodeDb`).
#[derive(Clone, Debug)]
struct PendingNode {
    shape: Option<String>,
    description: NodeLabel,
    css_classes: String,
    css_styles: Vec<String>,
    node_type: Option<String>,
    dir: Option<Direction>,
    color_index: Option<u32>,
}

pub(crate) struct Db {
    nodes: Vec<NodeData>,
    edges: Vec<Edge>,
    states: Vec<State>,
    relations: Vec<Relation>,
    classes: Vec<StyleClass>,
    links: Vec<Link>,
    start_end_count: u32,
    acc_title: String,
    acc_description: String,
    diagram_title: String,
    direction: Direction,
    layout_direction: Direction,
    node_db: HashMap<String, PendingNode>,
    graph_item_count: usize,
    next_color_index: u32,
    container_color_index: HashMap<String, Option<u32>>,
}

/// Translates the parser's document (`setRootDoc` for a version 2 database) and applies the
/// database and data-fetcher semantics to it.
pub(crate) fn build(
    mut root_doc: Vec<Statement>,
    acc_title: String,
    acc_description: String,
) -> Result<Ast, ParseError> {
    translate_root_doc(&mut root_doc);
    let mut db = Db::new();
    db.acc_title = acc_title;
    db.acc_description = acc_description;
    db.direction = first_direction(&root_doc);
    db.layout_direction = last_direction(&root_doc);
    db.extract(&mut root_doc)?;
    db.postprocess_nodes()?;
    Ok(db.into_ast())
}

impl Db {
    pub(crate) fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            states: Vec::new(),
            relations: Vec::new(),
            classes: Vec::new(),
            links: Vec::new(),
            start_end_count: 0,
            acc_title: String::new(),
            acc_description: String::new(),
            diagram_title: String::new(),
            direction: DIAGRAM_DIRECTION_DEFAULT,
            layout_direction: DIAGRAM_DIRECTION_DEFAULT,
            node_db: HashMap::new(),
            graph_item_count: 0,
            next_color_index: 0,
            container_color_index: HashMap::new(),
        }
    }

    /// `clear(saveCommon)`.
    fn clear(&mut self, save_common: bool) {
        self.nodes.clear();
        self.edges.clear();
        self.states.clear();
        self.relations.clear();
        self.classes.clear();
        self.start_end_count = 0;
        if !save_common {
            self.links.clear();
            self.acc_title.clear();
            self.acc_description.clear();
            self.diagram_title.clear();
        }
    }

    /// `extract`: apply the statements, then derive the layout data from them.
    pub(crate) fn extract(&mut self, statements: &mut [Statement]) -> Result<(), ParseError> {
        self.clear(true);
        for item in statements.iter_mut() {
            match item {
                Statement::State(state) => {
                    if let Some(note) = &mut state.note {
                        note.text = sanitize::sanitize_text(&note.text);
                    }
                    self.add_state(
                        state.id.trim(),
                        state.state_type.unwrap_or_default(),
                        state.doc.clone(),
                        state_descriptions(state),
                        state.note.clone(),
                        &[],
                        &[],
                        &[],
                    );
                }
                Statement::Relation(relation) => self.add_relation_objs(relation),
                Statement::ClassDef { id, classes } => {
                    self.add_style_class(id.trim(), classes);
                }
                Statement::Style { id, style_class } => {
                    self.handle_style_def(id, style_class);
                }
                Statement::ApplyClass { id, style_class } => {
                    self.set_css_class(id.trim(), style_class);
                }
                Statement::Click { id, url, tooltip } => self.add_link(id, url, tooltip),
                Statement::Direction(_) | Statement::Other(_) => {}
            }
        }
        self.reset_fetcher();
        self.setup_doc(None, statements, true)
    }

    /// `addState`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn add_state(
        &mut self,
        id: &str,
        state_type: StateKind,
        doc: Option<Vec<Statement>>,
        descriptions: &[String],
        note: Option<Note>,
        classes: &[String],
        styles: &[String],
        text_styles: &[String],
    ) {
        let trimmed = id.trim();
        if self.state(trimmed).is_none() {
            self.states.push(State {
                id: trimmed.to_string(),
                descriptions: Vec::new(),
                state_type,
                doc,
                note: None,
                classes: Vec::new(),
                styles: Vec::new(),
                text_styles: Vec::new(),
            });
        } else if let Some(state) = self.state_mut(trimmed)
            && state.doc.is_none()
        {
            state.doc = doc;
        }
        for description in descriptions {
            self.add_description(trimmed, description.trim());
        }
        if let Some(note) = note {
            let text = sanitize::sanitize_text(&note.text);
            if let Some(state) = self.state_mut(trimmed) {
                state.note = Some(Note {
                    position: note.position,
                    text,
                });
            }
        }
        for class in classes {
            self.set_css_class(trimmed, class.trim());
        }
        for style in styles {
            self.set_style(trimmed, style.trim());
        }
        for text_style in text_styles {
            self.set_text_style(trimmed, text_style.trim());
        }
    }

    /// `addDescription`.
    pub(crate) fn add_description(&mut self, id: &str, description: &str) {
        let text = match description.strip_prefix(':') {
            Some(rest) => rest.trim(),
            None => description,
        };
        let text = sanitize::sanitize_text(text);
        if let Some(state) = self.state_mut(id) {
            state.descriptions.push(text);
        }
    }

    /// `addStyleClass`.
    pub(crate) fn add_style_class(&mut self, id: &str, attributes: &str) {
        if self.classes.iter().all(|class| class.id != id) {
            self.classes.push(StyleClass {
                id: id.to_string(),
                styles: Vec::new(),
                text_styles: Vec::new(),
            });
        }
        if attributes.is_empty() {
            return;
        }
        let Some(index) = self.classes.iter().position(|class| class.id == id) else {
            return;
        };
        for attrib in attributes.split(',') {
            let fixed = strip_first_semicolon(attrib).trim().to_string();
            if attrib.contains("color") {
                let text_style = fixed.replacen("fill", "bgFill", 1);
                let text_style = text_style.replacen("color", "fill", 1);
                self.classes[index].text_styles.push(text_style);
            }
            self.classes[index].styles.push(fixed);
        }
    }

    /// `handleStyleDef`.
    fn handle_style_def(&mut self, id: &str, style_class: &str) {
        let ids: Vec<String> = id.trim().split(',').map(str::to_string).collect();
        let styles: Vec<String> = style_class
            .split(',')
            .map(|style| style.replace(';', "").trim().to_string())
            .collect();
        for id in ids {
            if self.state(&id).is_none() {
                self.add_state(
                    id.trim(),
                    StateKind::Default,
                    None,
                    &[],
                    None,
                    &[],
                    &[],
                    &[],
                );
            }
            if let Some(state) = self.state_mut(id.trim()) {
                state.styles = styles.clone();
            }
        }
    }

    /// `setCssClass`.
    fn set_css_class(&mut self, ids: &str, class: &str) {
        for id in ids.split(',') {
            if self.state(id).is_none() {
                self.add_state(
                    id.trim(),
                    StateKind::Default,
                    None,
                    &[],
                    None,
                    &[],
                    &[],
                    &[],
                );
            }
            if let Some(state) = self.state_mut(id.trim()) {
                state.classes.push(class.to_string());
            }
        }
    }

    fn set_style(&mut self, id: &str, style: &str) {
        if let Some(state) = self.state_mut(id) {
            state.styles.push(style.to_string());
        }
    }

    fn set_text_style(&mut self, id: &str, style: &str) {
        if let Some(state) = self.state_mut(id) {
            state.text_styles.push(style.to_string());
        }
    }

    fn add_link(&mut self, id: &str, url: &str, tooltip: &str) {
        self.links.push(Link {
            id: id.to_string(),
            url: url.to_string(),
            tooltip: tooltip.to_string(),
        });
    }

    /// `addRelationObjs`.
    fn add_relation_objs(&mut self, relation: &RelationStatement) {
        let item1 = &relation.state1;
        let item2 = &relation.state2;
        let id1 = self.start_id_if_needed(item1.id.trim());
        let type1 = start_type_if_needed(item1.id.trim(), item1.state_type);
        let id2 = self.start_id_if_needed(item2.id.trim());
        let type2 = start_type_if_needed(item2.id.trim(), item2.state_type);
        self.add_state(
            &id1,
            type1,
            item1.doc.clone(),
            state_descriptions(item1),
            item1.note.clone(),
            &item1.classes,
            &[],
            &[],
        );
        self.add_state(
            &id2,
            type2,
            item2.doc.clone(),
            state_descriptions(item2),
            item2.note.clone(),
            &item2.classes,
            &[],
            &[],
        );
        self.relations.push(Relation {
            id1,
            id2,
            relation_title: sanitize::sanitize_text(relation.description.as_deref().unwrap_or("")),
        });
    }

    fn start_id_if_needed(&mut self, id: &str) -> String {
        if id == START_NODE {
            self.start_end_count += 1;
            format!("start{}", self.start_end_count)
        } else {
            id.to_string()
        }
    }

    fn reset_fetcher(&mut self) {
        self.node_db.clear();
        self.graph_item_count = 0;
        self.next_color_index = 0;
        self.container_color_index.clear();
    }

    /// `setupDoc`.
    fn setup_doc(
        &mut self,
        parent: Option<&StateStatement>,
        doc: &[Statement],
        alt_flag: bool,
    ) -> Result<(), ParseError> {
        for item in doc {
            match item {
                Statement::State(state) => self.data_fetcher(parent, state, alt_flag)?,
                Statement::Relation(relation) => {
                    self.data_fetcher(parent, &relation.state1, alt_flag)?;
                    self.data_fetcher(parent, &relation.state2, alt_flag)?;
                    self.edges.push(Edge {
                        id: format!("edge{}", self.graph_item_count),
                        start: relation.state1.id.clone(),
                        end: relation.state2.id.clone(),
                        arrowhead: "normal".to_string(),
                        arrow_type_end: "arrow_barb_neo".to_string(),
                        style: G_EDGE_STYLE.to_string(),
                        label_style: String::new(),
                        label: Some(sanitize::sanitize_text(
                            relation.description.as_deref().unwrap_or(""),
                        )),
                        arrowhead_style: G_EDGE_ARROWHEADSTYLE.to_string(),
                        labelpos: G_EDGE_LABELPOS.to_string(),
                        label_type: G_EDGE_LABELTYPE.to_string(),
                        thickness: G_EDGE_THICKNESS.to_string(),
                        classes: CSS_EDGE.to_string(),
                        pattern: None,
                    });
                    self.graph_item_count += 1;
                }
                Statement::Direction(_)
                | Statement::Other(_)
                | Statement::ClassDef { .. }
                | Statement::Style { .. }
                | Statement::ApplyClass { .. }
                | Statement::Click { .. } => {}
            }
        }
        Ok(())
    }

    /// `dataFetcher`.
    fn data_fetcher(
        &mut self,
        parent: Option<&StateStatement>,
        item: &StateStatement,
        alt_flag: bool,
    ) -> Result<(), ParseError> {
        let item_id = item.id.clone();
        let (class_str, styles) = match self.state(&item_id) {
            Some(state) => (state.classes.join(" "), state.styles.clone()),
            None => (String::new(), Vec::new()),
        };
        let user_styled = !class_str.trim().is_empty() || !styles.is_empty();

        if !self.node_db.contains_key(&item_id) {
            let mut shape = Some(SHAPE_STATE.to_string());
            if item.start == Some(true) {
                shape = Some(SHAPE_START.to_string());
            } else if item.start == Some(false) {
                shape = Some(SHAPE_END.to_string());
            }
            if item.state_type != Some(StateKind::Default) {
                shape = item.state_type.map(|kind| kind.as_str().to_string());
            }
            self.node_db.insert(
                item_id.clone(),
                PendingNode {
                    shape,
                    description: NodeLabel::Text(sanitize::sanitize_text(&item_id)),
                    css_classes: format!("{class_str} {CSS_DIAGRAM_STATE}"),
                    css_styles: styles,
                    node_type: None,
                    dir: None,
                    color_index: None,
                },
            );
        }

        let mut new_node = self
            .node_db
            .remove(&item_id)
            .expect("the node was just inserted");
        if item.description_is_set() {
            match &mut new_node.description {
                NodeLabel::Lines(lines) => {
                    new_node.shape = Some(SHAPE_STATE_WITH_DESC.to_string());
                    lines.extend(item.description.iter().cloned());
                }
                NodeLabel::Text(old) => {
                    let old = old.clone();
                    if !old.is_empty() {
                        new_node.shape = Some(SHAPE_STATE_WITH_DESC.to_string());
                        if old == item_id {
                            new_node.description = NodeLabel::Lines(item.description.clone());
                        } else {
                            let mut lines = vec![old];
                            lines.extend(item.description.iter().cloned());
                            new_node.description = NodeLabel::Lines(lines);
                        }
                    } else {
                        new_node.shape = Some(SHAPE_STATE.to_string());
                        new_node.description = NodeLabel::Lines(item.description.clone());
                    }
                }
            }
            new_node.description = new_node.description.clone().sanitized();
        }

        if new_node.description.len() == 1
            && new_node.shape.as_deref() == Some(SHAPE_STATE_WITH_DESC)
        {
            new_node.shape = Some(
                if new_node.node_type.as_deref() == Some("group") {
                    SHAPE_GROUP
                } else {
                    SHAPE_STATE
                }
                .to_string(),
            );
        }

        if new_node.node_type.is_none() && item.doc.is_some() {
            new_node.node_type = Some("group".to_string());
            new_node.dir = Some(get_dir(item));
            new_node.shape = Some(
                if item.state_type == Some(StateKind::Divider) {
                    SHAPE_DIVIDER
                } else {
                    SHAPE_GROUP
                }
                .to_string(),
            );
            new_node.color_index =
                self.color_slot_for(new_node.shape.as_deref(), &item_id, parent, user_styled);
            new_node.css_classes = format!(
                "{} {} {}",
                new_node.css_classes,
                CSS_DIAGRAM_CLUSTER,
                if alt_flag {
                    CSS_DIAGRAM_CLUSTER_ALT
                } else {
                    ""
                }
            );
        }
        self.node_db.insert(item_id.clone(), new_node.clone());

        let label = if new_node.shape.as_deref() == Some(SHAPE_DIVIDER) {
            NodeLabel::Text(String::new())
        } else {
            new_node.description.clone()
        };
        let node_data = NodeData {
            id: item_id.clone(),
            shape: new_node.shape.clone(),
            label,
            css_classes: new_node.css_classes.clone(),
            css_compiled_styles: Vec::new(),
            css_styles: new_node.css_styles.clone(),
            dir: new_node.dir,
            dom_id: state_dom_id(&item_id, self.graph_item_count, None),
            node_type: new_node.node_type.clone(),
            is_group: new_node.node_type.as_deref() == Some("group"),
            color_index: new_node.color_index,
            padding: Some(NODE_PADDING),
            rx: Some(NODE_RX),
            ry: Some(NODE_RX),
            center_label: Some(true),
            parent_id: parent
                .filter(|parent| parent.id != "root")
                .map(|parent| parent.id.clone()),
            position: None,
            label_style: Some(String::new()),
            label_type: Some("markdown".to_string()),
            description: Vec::new(),
        };

        if let Some(note) = &item.note {
            let note_id = format!("{item_id}----note-{}", self.graph_item_count);
            let parent_id = format!("{item_id}----parent");
            let note_data = NodeData {
                id: note_id.clone(),
                shape: Some(SHAPE_NOTE.to_string()),
                label: NodeLabel::Text(note.text.clone()),
                css_classes: CSS_DIAGRAM_NOTE.to_string(),
                css_compiled_styles: Vec::new(),
                css_styles: Vec::new(),
                dir: None,
                dom_id: state_dom_id(&item_id, self.graph_item_count, Some("note")),
                node_type: Some("node".to_string()),
                is_group: false,
                color_index: None,
                padding: Some(NOTE_PADDING),
                rx: None,
                ry: None,
                center_label: None,
                parent_id: Some(parent_id.clone()),
                position: note.position,
                label_style: Some(String::new()),
                // `noteData` leaves `labelType` unset, so `note.ts` measures the note as
                // plain text even though the annotated state is markdown.
                label_type: None,
                description: Vec::new(),
            };
            let group_data = NodeData {
                id: parent_id.clone(),
                shape: Some(SHAPE_NOTEGROUP.to_string()),
                label: NodeLabel::Text(note.text.clone()),
                css_classes: new_node.css_classes.clone(),
                css_compiled_styles: Vec::new(),
                css_styles: Vec::new(),
                dir: None,
                dom_id: state_dom_id(&item_id, self.graph_item_count, Some("parent")),
                node_type: Some("group".to_string()),
                is_group: true,
                color_index: None,
                padding: Some(NOTE_GROUP_PADDING),
                rx: None,
                ry: None,
                center_label: None,
                parent_id: None,
                position: note.position,
                label_style: Some(String::new()),
                label_type: None,
                description: Vec::new(),
            };
            self.graph_item_count += 1;
            self.insert_or_update(group_data);
            self.insert_or_update(note_data.clone());
            self.insert_or_update(node_data);

            let (from, to) = if note.position == Some(NotePosition::LeftOf) {
                (note_data.id, item_id.clone())
            } else {
                (item_id.clone(), note_data.id)
            };
            self.edges.push(Edge {
                id: format!("{from}-{to}"),
                start: from,
                end: to,
                arrowhead: "none".to_string(),
                arrow_type_end: String::new(),
                style: G_EDGE_STYLE.to_string(),
                label_style: String::new(),
                label: None,
                arrowhead_style: G_EDGE_ARROWHEADSTYLE.to_string(),
                labelpos: G_EDGE_LABELPOS.to_string(),
                label_type: G_EDGE_LABELTYPE.to_string(),
                thickness: G_EDGE_THICKNESS.to_string(),
                classes: CSS_EDGE_NOTE_EDGE.to_string(),
                pattern: Some(EdgePattern::Dashed),
            });
        } else {
            self.insert_or_update(node_data);
        }

        if let Some(doc) = &item.doc {
            self.setup_doc(Some(item), doc, !alt_flag)?;
        }
        Ok(())
    }

    /// `dataFetcher.ts` `colorSlotFor`.
    fn color_slot_for(
        &mut self,
        shape: Option<&str>,
        item_id: &str,
        parent: Option<&StateStatement>,
        user_styled: bool,
    ) -> Option<u32> {
        if shape == Some(SHAPE_DIVIDER)
            && let Some(parent) = parent
            && let Some(inherited) = self.container_color_index.get(&parent.id)
        {
            let inherited = *inherited;
            self.container_color_index
                .insert(item_id.to_string(), inherited);
            return inherited;
        }
        let slot = self.next_color_index;
        self.next_color_index += 1;
        let effective = if user_styled { None } else { Some(slot) };
        self.container_color_index
            .insert(item_id.to_string(), effective);
        effective
    }

    /// `insertOrUpdateNode`.
    fn insert_or_update(&mut self, mut node: NodeData) {
        if node.id.is_empty() || node.id == "</join></fork>" || node.id == "</choice>" {
            return;
        }
        if !node.css_classes.is_empty() {
            let mut compiled = node.css_compiled_styles.clone();
            for class in node.css_classes.split(' ') {
                if let Some(class_def) = self.classes.iter().find(|def| def.id == class) {
                    compiled.extend(class_def.styles.iter().cloned());
                }
            }
            node.css_compiled_styles = compiled;
        }
        if let Some(existing) = self
            .nodes
            .iter_mut()
            .find(|existing| existing.id == node.id)
        {
            *existing = node;
        } else {
            self.nodes.push(node);
        }
    }

    /// `extract`'s last pass: split a label array into label plus description.
    fn postprocess_nodes(&mut self) -> Result<(), ParseError> {
        for node in &mut self.nodes {
            let label = std::mem::replace(&mut node.label, NodeLabel::Text(String::new()));
            let NodeLabel::Lines(lines) = label else {
                node.label = label;
                continue;
            };
            let description = lines.get(1..).map(<[String]>::to_vec).unwrap_or_default();
            if node.is_group && !description.is_empty() {
                return Err(ParseError::new(
                    format!(
                        "Group nodes can only have label. Remove the additional description for node [{}]",
                        node.id
                    ),
                    None,
                ));
            }
            node.description = description;
            node.label = NodeLabel::Text(lines.first().cloned().unwrap_or_default());
        }
        Ok(())
    }

    fn into_ast(self) -> Ast {
        Ast {
            states: self.states,
            relations: self.relations,
            classes: self.classes,
            links: self.links,
            nodes: self.nodes.into_iter().map(finalize_node).collect(),
            edges: self.edges,
            direction: self.direction,
            layout_direction: self.layout_direction,
            acc_title: self.acc_title,
            acc_description: self.acc_description,
            diagram_title: self.diagram_title,
        }
    }

    pub(crate) fn state(&self, id: &str) -> Option<&State> {
        self.states.iter().find(|state| state.id == id)
    }

    #[cfg(test)]
    pub(crate) fn classes(&self) -> &[StyleClass] {
        &self.classes
    }

    #[cfg(test)]
    pub(crate) fn links(&self) -> &[Link] {
        &self.links
    }

    fn state_mut(&mut self, id: &str) -> Option<&mut State> {
        self.states.iter_mut().find(|state| state.id == id)
    }
}

fn finalize_node(node: NodeData) -> Node {
    let label = match node.label {
        NodeLabel::Text(text) => text,
        NodeLabel::Lines(lines) => lines.first().cloned().unwrap_or_default(),
    };
    Node {
        id: node.id,
        shape: node.shape,
        label,
        description: node.description,
        css_classes: node.css_classes,
        css_compiled_styles: node.css_compiled_styles,
        css_styles: node.css_styles,
        dir: node.dir,
        dom_id: node.dom_id,
        node_type: node.node_type,
        is_group: node.is_group,
        color_index: node.color_index,
        padding: node.padding,
        rx: node.rx,
        ry: node.ry,
        center_label: node.center_label,
        parent_id: node.parent_id,
        position: node.position,
        label_style: node.label_style,
        label_type: node.label_type,
    }
}

/// `stateDomId`: `state-<id>[----<type>]-<counter>`.
fn state_dom_id(item_id: &str, counter: usize, node_type: Option<&str>) -> String {
    let type_str = node_type
        .filter(|kind| !kind.is_empty())
        .map(|kind| format!("----{kind}"))
        .unwrap_or_default();
    format!("state-{item_id}{type_str}-{counter}")
}

/// `dataFetcher.ts` `getDir`: the last `direction` statement in a document, else `TB`.
fn get_dir(item: &StateStatement) -> Direction {
    let mut dir = Direction::Tb;
    if let Some(doc) = &item.doc {
        for statement in doc {
            if let Statement::Direction(value) = statement {
                dir = *value;
            }
        }
    }
    dir
}

fn first_direction(doc: &[Statement]) -> Direction {
    doc.iter()
        .find_map(|statement| match statement {
            Statement::Direction(value) => Some(*value),
            _ => None,
        })
        .unwrap_or(DIAGRAM_DIRECTION_DEFAULT)
}

fn last_direction(doc: &[Statement]) -> Direction {
    doc.iter()
        .rev()
        .find_map(|statement| match statement {
            Statement::Direction(value) => Some(*value),
            _ => None,
        })
        .unwrap_or(DIAGRAM_DIRECTION_DEFAULT)
}

/// The descriptions upstream's `if (descr)` sees, or an empty slice when it is falsy.
fn state_descriptions(state: &StateStatement) -> &[String] {
    if state.description_is_set() {
        &state.description
    } else {
        &[]
    }
}

fn start_type_if_needed(id: &str, state_type: Option<StateKind>) -> StateKind {
    if id == START_NODE {
        StateKind::Start
    } else {
        state_type.unwrap_or(StateKind::Default)
    }
}

/// JavaScript's `attrib.replace(/([^;]*);/, '$1')`: drop the first semicolon.
fn strip_first_semicolon(text: &str) -> String {
    match text.find(';') {
        Some(index) => {
            let mut owned = text.to_string();
            owned.remove(index);
            owned
        }
        None => text.to_string(),
    }
}

fn is_divider(statement: &Statement) -> bool {
    matches!(statement, Statement::State(state) if state.state_type == Some(StateKind::Divider))
}

/// `setRootDoc` for version 2: `docTranslator` over the root document.
fn translate_root_doc(doc: &mut [Statement]) {
    let mut generated = 0;
    translate_doc("root", doc, &mut generated);
}

fn translate_doc(parent_id: &str, doc: &mut [Statement], generated: &mut u32) {
    for statement in doc.iter_mut() {
        match statement {
            Statement::Relation(relation) => {
                translate_state(parent_id, &mut relation.state1, true, generated);
                translate_state(parent_id, &mut relation.state2, false, generated);
            }
            Statement::State(state) => translate_state(parent_id, state, true, generated),
            Statement::ClassDef { .. }
            | Statement::Style { .. }
            | Statement::ApplyClass { .. }
            | Statement::Direction(_)
            | Statement::Click { .. }
            | Statement::Other(_) => {}
        }
    }
}

/// `docTranslator` for one state or relation end: rename `[*]`, then split its document on
/// `--` dividers.
fn translate_state(parent_id: &str, node: &mut StateStatement, first: bool, generated: &mut u32) {
    if node.id == START_NODE {
        node.id = format!("{parent_id}{}", if first { "_start" } else { "_end" });
        node.start = Some(first);
    } else {
        node.id = node.id.trim().to_string();
    }

    let Some(original) = node.doc.take() else {
        return;
    };
    let mut doc = Vec::new();
    let mut current = Vec::new();
    let mut saw_divider = false;
    for statement in original {
        if is_divider(&statement) {
            saw_divider = true;
            let mut region = statement;
            if let Statement::State(state) = &mut region {
                state.doc = Some(std::mem::take(&mut current));
            }
            doc.push(region);
        } else {
            current.push(statement);
        }
    }
    if saw_divider {
        if !current.is_empty() {
            *generated += 1;
            doc.push(Statement::State(StateStatement {
                id: format!("id-{generated}"),
                state_type: Some(StateKind::Divider),
                description: Vec::new(),
                doc: Some(std::mem::take(&mut current)),
                note: None,
                classes: Vec::new(),
                start: None,
            }));
        }
        node.doc = Some(doc);
    } else {
        node.doc = Some(current);
    }

    let id = node.id.clone();
    if let Some(doc) = &mut node.doc {
        translate_doc(&id, doc, generated);
    }
}
