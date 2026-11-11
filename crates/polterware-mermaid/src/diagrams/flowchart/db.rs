//! The diagram database: a port of upstream `flowDb.ts`.
//!
//! Every method mirrors the one upstream's grammar actions call (`addVertex`, `addLink`,
//! `addSubGraph`, `destructLink`, ...), and [`Db::get_data`] mirrors `getData`, which turns the
//! raw grammar state into the nodes and edges the layout engine consumes. The parts that need
//! the browser (tooltips as DOM nodes, click callbacks, configuration) stay out: tooltips are
//! kept as data, a click only marks the node clickable, and configuration defaults are the
//! published defaults (`maxEdges` 500, `flowchart.inheritDir` false, `stepDuration` 1).

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value};

use super::ast::{
    ArrowEnd, ArrowType, Ast, DroppedSubGraphMember, Edge, FlowClass, LabelType, LayoutEdge, Node,
    Stroke, SubGraph, Tooltip, Vertex,
};
use super::metadata;
use crate::common::sanitize::collapse_description_indent;
use crate::common::text::is_truthy;
use crate::error::{ParseError, Span};

const MERMAID_DOM_ID_PREFIX: &str = "flowchart-";
const DEFAULT_MAX_EDGES: usize = 500;
const EDGE_STEP_DURATION: f64 = 1.0;

/// A text and how it is interpreted, upstream's `FlowText`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlowText {
    pub text: String,
    pub label_type: LabelType,
}

/// The link the grammar built, upstream's link object.
#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub type_name: ArrowType,
    pub stroke: Stroke,
    pub length: Option<i64>,
    pub text: Option<FlowText>,
    pub id: Option<String>,
}

/// A line of a subgraph body, as the grammar's `document` collects it.
#[derive(Clone, Debug, PartialEq)]
pub enum StatementValue {
    /// A `vertexStatement`'s node ids, in the order the grammar produced them.
    Nodes(Vec<String>),
    /// A line that reduces to a string (`SEMI`, `NEWLINE`, `SPACE`, a subgraph id, a click id).
    Text(String),
    /// A `direction` statement; upstream filters it out of the member list and keeps its value.
    Direction(String),
}

/// One end of a link as `destructLink` reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkInfo {
    pub type_name: ArrowType,
    pub stroke: Stroke,
    pub length: Option<i64>,
}

/// The database the parser fills.
#[derive(Debug, Default)]
pub struct Db {
    vertex_counter: u32,
    vertices: Vec<Vertex>,
    edges: Vec<Edge>,
    default_interpolate: Option<String>,
    default_style: Option<Vec<String>>,
    classes: Vec<FlowClass>,
    sub_graphs: Vec<SubGraph>,
    dropped_sub_graph_members: Vec<DroppedSubGraphMember>,
    tooltips: Vec<Tooltip>,
    sub_count: u32,
    direction: Option<String>,
    acc_title: String,
    acc_description: String,
}

impl Db {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_acc_title(&mut self, title: &str) {
        self.acc_title = metadata::sanitize_text(title).trim_start().to_string();
    }

    pub fn set_acc_description(&mut self, description: &str) {
        self.acc_description = collapse_description_indent(&metadata::sanitize_text(description));
    }

    /// The class with this id, when one was declared (`getClasses().get(id)`).
    pub fn class(&self, id: &str) -> Option<&FlowClass> {
        self.classes.iter().find(|class| class.id == id)
    }

    /// `getDirection()`.
    #[allow(dead_code)] // used by the ported database specs
    pub fn direction(&self) -> Option<&str> {
        self.direction.as_deref()
    }

    pub fn set_direction(&mut self, raw: &str) {
        let mut direction = raw.trim().to_string();
        if direction.contains('<') {
            direction = "RL".to_string();
        }
        if direction.contains('^') {
            direction = "BT".to_string();
        }
        if direction.contains('>') {
            direction = "LR".to_string();
        }
        if direction.contains('v') {
            direction = "TB".to_string();
        }
        if direction == "TD" {
            direction = "TB".to_string();
        }
        self.direction = Some(direction);
    }

    /// `addVertex`: a node definition, an edge's `@{ }` metadata, or a subgraph's.
    #[allow(clippy::too_many_arguments)]
    pub fn add_vertex(
        &mut self,
        id: &str,
        text: Option<FlowText>,
        type_name: Option<&str>,
        style: Option<&[String]>,
        classes: Option<&[String]>,
        dir: Option<&str>,
        props: Option<Map<String, Value>>,
        metadata_text: Option<&str>,
        span: Span,
    ) -> Result<(), ParseError> {
        if id.trim().is_empty() {
            return Ok(());
        }
        let doc = match metadata_text {
            Some(text) => {
                let parsed = metadata::parse(text)
                    .map_err(|message| ParseError::at(format!("metadata: {message}"), span))?;
                Some(metadata::without_internal_keys(parsed))
            }
            None => None,
        };

        if doc.as_ref().is_some_and(is_truthy)
            && let Some(index) = self
                .sub_graphs
                .iter()
                .position(|sub_graph| sub_graph.id == id)
        {
            if let Some(Value::Object(map)) = &doc
                && !map.is_empty()
            {
                let sub_graph = &mut self.sub_graphs[index];
                let target = sub_graph.metadata.get_or_insert_with(Map::new);
                for (key, value) in map {
                    target.insert(key.clone(), value.clone());
                }
            }
            return Ok(());
        }

        if let Some(index) = self
            .edges
            .iter()
            .position(|edge| edge.id.as_deref() == Some(id))
        {
            if let Some(document) = doc.as_ref().and_then(metadata::as_map) {
                let edge = &mut self.edges[index];
                if let Some(Value::Bool(animate)) = document.get("animate") {
                    edge.animate = Some(*animate);
                }
                if let Some(Value::String(animation)) = document.get("animation") {
                    edge.animation = Some(animation.clone());
                }
                if let Some(Value::String(curve)) = document.get("curve") {
                    edge.interpolate = Some(curve.clone());
                }
                if let Some(step) = document.get("step").and_then(step_value) {
                    edge.step = Some(step);
                }
            }
            return Ok(());
        }

        let index = match self.vertices.iter().position(|vertex| vertex.id == id) {
            Some(index) => index,
            None => {
                self.vertices.push(Vertex {
                    id: id.to_string(),
                    text: None,
                    label_type: LabelType::Text,
                    dom_id: format!("{MERMAID_DOM_ID_PREFIX}{id}-{}", self.vertex_counter),
                    type_name: None,
                    styles: vec![],
                    classes: vec![],
                    dir: None,
                    props: Map::new(),
                    link: None,
                    link_target: None,
                    have_callback: false,
                    icon: None,
                    form: None,
                    pos: None,
                    img: None,
                    asset_width: None,
                    asset_height: None,
                    constraint: None,
                });
                self.vertices.len() - 1
            }
        };
        self.vertex_counter += 1;

        {
            let vertex = &mut self.vertices[index];
            if let Some(text) = text {
                let mut label = metadata::sanitize_text(text.text.trim());
                if label.starts_with('"') && label.ends_with('"') && label.len() >= 2 {
                    label = label[1..label.len() - 1].to_string();
                }
                vertex.text = Some(label);
                vertex.label_type = text.label_type;
            } else if vertex.text.is_none() {
                vertex.text = Some(id.to_string());
            }
            if let Some(type_name) = type_name {
                vertex.type_name = Some(type_name.to_string());
            }
            if let Some(style) = style {
                vertex.styles.extend(style.iter().cloned());
            }
            if let Some(classes) = classes {
                vertex.classes.extend(classes.iter().cloned());
            }
            if let Some(dir) = dir {
                vertex.dir = Some(dir.to_string());
            }
            if let Some(props) = props {
                for (key, value) in props {
                    vertex.props.insert(key, value);
                }
            }
        }

        if let Some(document) = doc.as_ref().and_then(metadata::as_map) {
            self.apply_vertex_metadata(index, document, span)?;
        }
        Ok(())
    }

    fn apply_vertex_metadata(
        &mut self,
        index: usize,
        document: &Map<String, Value>,
        span: Span,
    ) -> Result<(), ParseError> {
        if let Some(Value::String(shape)) = document.get("shape") {
            if shape != &shape.to_lowercase() || shape.contains('_') {
                return Err(ParseError::at(
                    format!("No such shape: {shape}. Shape names should be lowercase."),
                    span,
                ));
            }
            if !metadata::is_valid_shape(shape) {
                return Err(ParseError::at(format!("No such shape: {shape}."), span));
            }
            self.vertices[index].type_name = Some(shape.clone());
        }
        let vertex_id = self.vertices[index].id.clone();
        let label = document.get("label").filter(|value| is_truthy(value));
        if let Some(Value::String(label)) = label {
            self.vertices[index].text = Some(label.clone());
            self.vertices[index].label_type = sanitize_node_label_type(document.get("labelType"));
        }
        let empty_label = document
            .get("label")
            .and_then(Value::as_str)
            .is_none_or(|label| label.trim().is_empty());
        if let Some(Value::String(icon)) = document.get("icon") {
            let vertex = &mut self.vertices[index];
            let clears = empty_label && vertex.text.as_deref() == Some(vertex_id.as_str());
            vertex.icon = Some(icon.clone());
            if clears {
                vertex.text = Some(String::new());
            }
        }
        if let Some(Value::String(form)) = document.get("form") {
            self.vertices[index].form = Some(form.clone());
        }
        if let Some(Value::String(pos)) = document.get("pos") {
            self.vertices[index].pos = Some(pos.clone());
        }
        if let Some(Value::String(img)) = document.get("img") {
            let vertex = &mut self.vertices[index];
            let clears = empty_label && vertex.text.as_deref() == Some(vertex_id.as_str());
            vertex.img = Some(img.clone());
            if clears {
                vertex.text = Some(String::new());
            }
        }
        if let Some(Value::String(constraint)) = document.get("constraint") {
            self.vertices[index].constraint = Some(constraint.clone());
        }
        if let Some(width) = document
            .get("w")
            .filter(|value| is_truthy(value))
            .and_then(number)
        {
            self.vertices[index].asset_width = Some(width);
        }
        if let Some(height) = document
            .get("h")
            .filter(|value| is_truthy(value))
            .and_then(number)
        {
            self.vertices[index].asset_height = Some(height);
        }
        Ok(())
    }

    /// `addLink`: every start/end pair of a vertex group.
    pub fn add_link(
        &mut self,
        starts: &[String],
        ends: &[String],
        link: &Link,
        span: Span,
    ) -> Result<(), ParseError> {
        let id = link.id.as_deref().map(|id| id.replacen('@', "", 1));
        let last_start = starts.last().cloned();
        let first_end = ends.first().cloned();
        for start in starts {
            for end in ends {
                let keeps_id =
                    Some(start) == last_start.as_ref() && Some(end) == first_end.as_ref();
                self.add_single_link(
                    start,
                    end,
                    link,
                    if keeps_id { id.as_deref() } else { None },
                    span,
                )?;
            }
        }
        Ok(())
    }

    fn add_single_link(
        &mut self,
        start: &str,
        end: &str,
        link: &Link,
        id: Option<&str>,
        span: Span,
    ) -> Result<(), ParseError> {
        let mut edge = Edge {
            is_user_defined_id: false,
            start: start.to_string(),
            end: end.to_string(),
            interpolate: self.default_interpolate.clone(),
            type_name: Some(link.type_name),
            stroke: Some(link.stroke),
            style: None,
            length: link.length.map(|length| length.min(10)),
            text: String::new(),
            label_type: LabelType::Text,
            classes: vec![],
            id: None,
            animation: None,
            animate: None,
            step: None,
        };
        if let Some(text) = &link.text {
            let mut label = metadata::sanitize_text(text.text.trim());
            if label.starts_with('"') && label.ends_with('"') && label.len() >= 2 {
                label = label[1..label.len() - 1].to_string();
            }
            edge.text = label;
            edge.label_type = text.label_type;
        }
        let user_id = id.filter(|id| {
            !id.is_empty() && !self.edges.iter().any(|e| e.id.as_deref() == Some(*id))
        });
        if let Some(user_id) = user_id {
            edge.id = Some(user_id.to_string());
            edge.is_user_defined_id = true;
        } else {
            let counter = self
                .edges
                .iter()
                .filter(|e| e.start == start && e.end == end)
                .count();
            let counter = if counter == 0 { 0 } else { counter + 1 };
            edge.id = Some(format!("L_{start}_{end}_{counter}"));
        }
        if self.edges.len() < DEFAULT_MAX_EDGES {
            self.edges.push(edge);
            Ok(())
        } else {
            Err(ParseError::at(
                format!(
                    "Edge limit exceeded. {} edges found, but the limit is {DEFAULT_MAX_EDGES}.\n\nInitialize mermaid with maxEdges set to a higher number to allow more edges.\nYou cannot set this config via configuration inside the diagram as it is a secure config.\nYou have to call mermaid.initialize.",
                    self.edges.len()
                ),
                span,
            ))
        }
    }

    pub fn update_link_interpolate(
        &mut self,
        positions: &[LinkPosition],
        interpolate: &str,
        span: Span,
    ) -> Result<(), ParseError> {
        for position in positions {
            match position {
                LinkPosition::Default => {
                    self.default_interpolate = Some(interpolate.to_string());
                }
                LinkPosition::Index(index) => {
                    let count = self.edges.len();
                    let edge = self.edges.get_mut(*index).ok_or_else(|| {
                        ParseError::at(
                            format!(
                                "The index {index} for linkStyle is out of bounds. Valid indices for linkStyle are between 0 and {}. (Help: Ensure that the index is within the range of existing edges.)",
                                count.saturating_sub(1)
                            ),
                            span,
                        )
                    })?;
                    edge.interpolate = Some(interpolate.to_string());
                }
            }
        }
        Ok(())
    }

    pub fn update_link(
        &mut self,
        positions: &[LinkPosition],
        style: &[String],
        span: Span,
    ) -> Result<(), ParseError> {
        for position in positions {
            match position {
                LinkPosition::Default => {
                    self.default_style = Some(style.to_vec());
                }
                LinkPosition::Index(index) => {
                    if *index >= self.edges.len() {
                        return Err(ParseError::at(
                            format!(
                                "The index {index} for linkStyle is out of bounds. Valid indices for linkStyle are between 0 and {}. (Help: Ensure that the index is within the range of existing edges.)",
                                self.edges.len().saturating_sub(1)
                            ),
                            span,
                        ));
                    }
                    let mut styles = style.to_vec();
                    if !styles.is_empty() && !styles.iter().any(|style| style.starts_with("fill")) {
                        styles.push("fill:none".to_string());
                    }
                    self.edges[*index].style = Some(styles);
                }
            }
        }
        Ok(())
    }

    /// `addClass`: defines (or extends) classes for a comma separated list of ids.
    pub fn add_class(&mut self, ids: &str, style: &[String]) {
        let joined = style.join(",");
        let joined = joined.replace("\\,", "\u{a7}\u{a7}\u{a7}");
        let joined = joined.replace(',', ";");
        let joined = joined.replace("\u{a7}\u{a7}\u{a7}", ",");
        let parts: Vec<String> = joined.split(';').map(str::to_string).collect();
        for id in ids.split(',') {
            let index = match self.classes.iter().position(|class| class.id == id) {
                Some(index) => index,
                None => {
                    self.classes.push(FlowClass {
                        id: id.to_string(),
                        styles: vec![],
                        text_styles: vec![],
                    });
                    self.classes.len() - 1
                }
            };
            for part in &parts {
                let class = &mut self.classes[index];
                if part.contains("color") {
                    class.text_styles.push(part.replacen("fill", "bgFill", 1));
                }
                class.styles.push(part.clone());
            }
        }
    }

    /// `setClass`: attaches a class name to nodes, edges, and subgraphs with that id.
    pub fn set_class(&mut self, ids: &str, class_name: &str) {
        for id in ids.split(',') {
            if let Some(vertex) = self.vertices.iter_mut().find(|vertex| vertex.id == id) {
                vertex.classes.push(class_name.to_string());
            }
            if let Some(edge) = self
                .edges
                .iter_mut()
                .find(|edge| edge.id.as_deref() == Some(id))
            {
                edge.classes.push(class_name.to_string());
            }
            if let Some(sub_graph) = self
                .sub_graphs
                .iter_mut()
                .find(|sub_graph| sub_graph.id == id)
            {
                sub_graph.classes.push(class_name.to_string());
            }
        }
    }

    pub fn set_tooltip(&mut self, ids: &str, tooltip: &str) {
        let tooltip = metadata::sanitize_text(tooltip);
        for id in ids.split(',') {
            match self.tooltips.iter_mut().find(|entry| entry.id == id) {
                Some(entry) => entry.text = tooltip.clone(),
                None => self.tooltips.push(Tooltip {
                    id: id.to_string(),
                    text: tooltip.clone(),
                }),
            }
        }
    }

    /// `setLink`: stores the URL and target, and marks the nodes clickable.
    pub fn set_link(&mut self, ids: &str, link: &str, target: Option<&str>) {
        for id in ids.split(',') {
            if let Some(vertex) = self.vertices.iter_mut().find(|vertex| vertex.id == id) {
                vertex.link = format_url(link);
                vertex.link_target = target.map(str::to_string);
            }
        }
        self.set_class(ids, "clickable");
    }

    /// `setClickEvent`: registers a DOM handler upstream; in strict security it only marks the
    /// nodes clickable, which is the observable database change.
    pub fn set_click_event(&mut self, ids: &str) {
        self.set_class(ids, "clickable");
    }

    /// `addSubGraph`: dedupes the body's lines, filters members already owned elsewhere, and
    /// merges a repeated id into the first declaration.
    pub fn add_sub_graph(
        &mut self,
        id: Option<&FlowText>,
        title: Option<&FlowText>,
        same_text: bool,
        lines: &[StatementValue],
    ) -> String {
        let mut id = id.map(|text| text.text.trim().to_string());
        let title_text = title.map(|text| text.text.clone()).unwrap_or_default();
        let label_type = title.map_or(LabelType::Markdown, |text| text.label_type);
        if same_text && title_text.chars().any(char::is_whitespace) {
            id = None;
        }
        let (members, dir) = unique_members(lines);
        let id = id.unwrap_or_else(|| format!("subGraph{}", self.sub_count));
        self.sub_count += 1;
        let title = metadata::sanitize_text(&title_text).trim().to_string();

        let existed = self.sub_graphs.iter().any(|sub_graph| sub_graph.id == id);
        let unique = make_uniq(&members, &self.sub_graphs);
        self.record_dropped_members(&id, &members, &unique);
        let kept: Vec<String> = unique.into_iter().filter(|node| node != &id).collect();
        if existed {
            if let Some(existing) = self
                .sub_graphs
                .iter_mut()
                .find(|sub_graph| sub_graph.id == id)
            {
                existing.nodes.extend(kept);
            }
        } else {
            self.sub_graphs.push(SubGraph {
                id: id.clone(),
                title,
                nodes: kept,
                classes: vec![],
                dir,
                label_type,
                metadata: None,
            });
        }
        id
    }

    fn record_dropped_members(&mut self, id: &str, listed: &[String], unique: &[String]) {
        let kept: HashSet<&String> = unique.iter().collect();
        let mut dropped = vec![];
        for node in listed {
            if kept.contains(node) || node == id {
                continue;
            }
            if let Some(owner) = self
                .sub_graphs
                .iter()
                .find(|sub_graph| sub_graph.nodes.contains(node))
            {
                let owner_id = owner.id.clone();
                if owner_id != id && !self.is_nested_in(&owner_id, listed) {
                    dropped.push(DroppedSubGraphMember {
                        node: node.clone(),
                        kept_by: owner_id,
                        dropped_from: id.to_string(),
                    });
                }
            }
        }
        self.dropped_sub_graph_members.extend(dropped);
    }

    fn is_nested_in(&self, id: &str, ancestor_nodes: &[String]) -> bool {
        let mut visited = HashSet::new();
        let mut current = Some(id.to_string());
        while let Some(node) = current {
            if visited.contains(&node) {
                break;
            }
            if ancestor_nodes.contains(&node) {
                return true;
            }
            visited.insert(node.clone());
            current = self
                .sub_graphs
                .iter()
                .find(|sub_graph| sub_graph.nodes.contains(&node))
                .map(|sub_graph| sub_graph.id.clone());
        }
        false
    }

    fn sub_graph_lookup(&self, id: &str) -> Option<&SubGraph> {
        self.sub_graphs.iter().find(|sub_graph| sub_graph.id == id)
    }

    /// `destructLink`: reads both ends and combines them.
    pub fn destruct_link(&self, end: &str, start: Option<&str>) -> LinkInfo {
        let info = destruct_end_link(end);
        match start {
            Some(start) => {
                let mut start_info = destruct_start_link(start);
                if start_info.stroke != info.stroke {
                    return LinkInfo {
                        type_name: ArrowType::Invalid,
                        stroke: Stroke::Invalid,
                        length: None,
                    };
                }
                if start_info.type_name == ArrowType::Open {
                    start_info.type_name = info.type_name;
                } else if start_info.type_name != info.type_name {
                    return LinkInfo {
                        type_name: ArrowType::Invalid,
                        stroke: Stroke::Invalid,
                        length: None,
                    };
                } else {
                    start_info.type_name = match start_info.type_name {
                        ArrowType::Point => ArrowType::DoublePoint,
                        ArrowType::Cross => ArrowType::DoubleCross,
                        ArrowType::Circle => ArrowType::DoubleCircle,
                        other => other,
                    };
                }
                start_info.length = info.length;
                start_info
            }
            None => info,
        }
    }

    pub fn into_ast(self) -> Ast {
        let (nodes, layout_edges) = self.get_data();
        Ast {
            direction: self.direction,
            acc_title: self.acc_title,
            acc_description: self.acc_description,
            vertices: self.vertices,
            edges: self.edges,
            default_interpolate: self.default_interpolate,
            default_style: self.default_style,
            classes: self.classes,
            sub_graphs: self.sub_graphs,
            dropped_sub_graph_members: self.dropped_sub_graph_members,
            tooltips: self.tooltips,
            nodes,
            layout_edges,
        }
    }

    /// `getData`: the layout nodes and edges.
    pub fn get_data(&self) -> (Vec<Node>, Vec<LayoutEdge>) {
        let sub_graphs = &self.sub_graphs;
        let mut parent_db: HashMap<String, String> = HashMap::new();
        let mut sub_graph_db: HashSet<String> = HashSet::new();
        let mut sub_graph_parent: HashMap<String, String> = HashMap::new();
        for sub_graph in sub_graphs {
            for child in &sub_graph.nodes {
                if self.sub_graph_lookup(child).is_some() {
                    sub_graph_parent.insert(child.clone(), sub_graph.id.clone());
                }
            }
        }

        let mut declaration_index: HashMap<String, usize> = HashMap::new();
        let mut children_of: HashMap<String, Vec<String>> = HashMap::new();
        for sub_graph in sub_graphs {
            if let Some(parent) = sub_graph_parent.get(&sub_graph.id) {
                children_of
                    .entry(parent.clone())
                    .or_default()
                    .push(sub_graph.id.clone());
            }
        }
        let mut next_index = 0;
        for sub_graph in sub_graphs {
            if !sub_graph_parent.contains_key(&sub_graph.id) {
                walk_declarations(
                    &sub_graph.id,
                    &children_of,
                    &mut declaration_index,
                    &mut next_index,
                );
            }
        }

        let is_collapsed = |id: &str| {
            self.sub_graph_lookup(id)
                .and_then(|sub_graph| sub_graph.metadata.as_ref())
                .and_then(|metadata| metadata.get("view"))
                .and_then(Value::as_str)
                == Some("collapsed")
        };
        let outermost_collapsed = |id: &str| {
            let mut result = None;
            let mut seen = HashSet::new();
            let mut current = Some(id.to_string());
            while let Some(node) = current {
                if !seen.insert(node.clone()) {
                    break;
                }
                if is_collapsed(&node) {
                    result = Some(node.clone());
                }
                current = sub_graph_parent.get(&node).cloned();
            }
            result
        };

        let mut hidden_ids: HashSet<String> = HashSet::new();
        let mut collapsed_ancestor: HashMap<String, String> = HashMap::new();
        for sub_graph in sub_graphs {
            let Some(ancestor) = outermost_collapsed(&sub_graph.id) else {
                continue;
            };
            if sub_graph.id != ancestor {
                hidden_ids.insert(sub_graph.id.clone());
                collapsed_ancestor.insert(sub_graph.id.clone(), ancestor.clone());
            }
            for child in &sub_graph.nodes {
                if child == &ancestor {
                    continue;
                }
                hidden_ids.insert(child.clone());
                collapsed_ancestor.insert(child.clone(), ancestor.clone());
            }
        }

        for sub_graph in sub_graphs.iter().rev() {
            if hidden_ids.contains(&sub_graph.id) {
                continue;
            }
            if !sub_graph.nodes.is_empty() {
                sub_graph_db.insert(sub_graph.id.clone());
            }
            for id in &sub_graph.nodes {
                parent_db.insert(id.clone(), sub_graph.id.clone());
            }
        }

        let mut nodes = Vec::new();
        for sub_graph in sub_graphs.iter().rev() {
            if hidden_ids.contains(&sub_graph.id) {
                continue;
            }
            let collapsed = is_collapsed(&sub_graph.id);
            nodes.push(Node {
                id: sub_graph.id.clone(),
                label: sub_graph.title.clone(),
                label_style: String::new(),
                label_type: sub_graph.label_type,
                parent_id: parent_db.get(&sub_graph.id).cloned(),
                padding: 8.0,
                css_styles: vec![],
                css_compiled_styles: self
                    .get_compiled_styles(sub_graph.classes.iter().map(String::as_str)),
                css_classes: sub_graph.classes.join(" "),
                shape: if collapsed { "collapsedGroup" } else { "rect" }.to_string(),
                dir: sub_graph.dir.clone(),
                is_group: !collapsed,
                look: None,
                color_index: declaration_index.get(&sub_graph.id).copied(),
                dom_id: None,
                link: None,
                link_target: None,
                tooltip: None,
                icon: None,
                pos: None,
                img: None,
                asset_width: None,
                asset_height: None,
                min_width: None,
                constraint: None,
                metadata: if collapsed {
                    None
                } else {
                    sub_graph.metadata.clone()
                },
            });
        }

        for vertex in &self.vertices {
            if hidden_ids.contains(&vertex.id) {
                continue;
            }
            self.add_node_from_vertex(vertex, &mut nodes, &parent_db, &sub_graph_db);
        }

        let step_count = self
            .edges
            .iter()
            .filter_map(|edge| edge.step)
            .max()
            .unwrap_or(0);
        let mut layout_edges = Vec::new();
        for (index, raw) in self.edges.iter().enumerate() {
            let (arrow_start, arrow_end) = destruct_edge_type(raw.type_name);
            let mut styles = self.default_style.clone().unwrap_or_default();
            let start = collapsed_ancestor
                .get(&raw.start)
                .cloned()
                .unwrap_or_else(|| raw.start.clone());
            let end = collapsed_ancestor
                .get(&raw.end)
                .cloned()
                .unwrap_or_else(|| raw.end.clone());
            if start == end
                && (collapsed_ancestor.contains_key(&raw.start)
                    || collapsed_ancestor.contains_key(&raw.end))
            {
                continue;
            }
            if let Some(style) = &raw.style {
                styles.extend(style.iter().cloned());
            }
            let invisible = raw.stroke == Some(Stroke::Invisible);
            let open = raw.type_name == Some(ArrowType::Open);
            let id = raw
                .id
                .clone()
                .unwrap_or_else(|| format!("L_{start}_{end}_{index}"));
            let step = raw.step.filter(|step| *step > 0);
            layout_edges.push(LayoutEdge {
                id,
                is_user_defined_id: raw.is_user_defined_id,
                start,
                end,
                type_name: raw
                    .type_name
                    .map_or_else(|| "normal".to_string(), |kind| kind.as_str().to_string()),
                label: raw.text.clone(),
                label_type: raw.label_type,
                labelpos: "c".to_string(),
                thickness: raw.stroke,
                minlen: raw.length,
                classes: if invisible {
                    String::new()
                } else {
                    "edge-thickness-normal edge-pattern-solid flowchart-link".to_string()
                },
                arrow_type_start: if invisible || open {
                    ArrowEnd::None
                } else {
                    arrow_start
                },
                arrow_type_end: if invisible || open {
                    ArrowEnd::None
                } else {
                    arrow_end
                },
                arrowhead_style: "fill: #333".to_string(),
                css_compiled_styles: self
                    .get_compiled_styles(raw.classes.iter().map(String::as_str)),
                label_style: styles.clone(),
                style: styles,
                pattern: raw.stroke,
                look: None,
                animate: raw.animate,
                animation: raw.animation.clone(),
                step,
                step_count: step.map(|_| step_count),
                step_duration: step.map(|_| EDGE_STEP_DURATION),
                show_step_number: None,
                curve: raw
                    .interpolate
                    .clone()
                    .or_else(|| self.default_interpolate.clone()),
            });
        }

        (nodes, layout_edges)
    }

    fn add_node_from_vertex(
        &self,
        vertex: &Vertex,
        nodes: &mut Vec<Node>,
        parent_db: &HashMap<String, String>,
        sub_graph_db: &HashSet<String>,
    ) {
        let is_group = sub_graph_db.contains(&vertex.id);
        if let Some(node) = nodes.iter_mut().find(|node| node.id == vertex.id) {
            node.css_styles = vertex.styles.clone();
            node.css_compiled_styles =
                self.get_compiled_styles(vertex.classes.iter().map(String::as_str));
            node.css_classes = vertex.classes.join(" ");
            return;
        }
        let tooltip = self
            .tooltips
            .iter()
            .find(|tooltip| tooltip.id == vertex.id)
            .map(|tooltip| tooltip.text.clone());
        nodes.push(Node {
            id: vertex.id.clone(),
            label: vertex.text.clone().unwrap_or_default(),
            label_style: String::new(),
            label_type: vertex.label_type,
            parent_id: parent_db.get(&vertex.id).cloned(),
            padding: 8.0,
            css_styles: vertex.styles.clone(),
            css_compiled_styles: self.get_compiled_styles(
                ["default", "node"]
                    .into_iter()
                    .chain(vertex.classes.iter().map(String::as_str)),
            ),
            css_classes: format!("default {}", vertex.classes.join(" ")),
            shape: if is_group {
                "rect".to_string()
            } else {
                get_type_from_vertex(vertex).to_string()
            },
            dir: vertex.dir.clone(),
            is_group,
            look: None,
            color_index: None,
            dom_id: Some(vertex.dom_id.clone()),
            link: vertex.link.clone(),
            link_target: vertex.link_target.clone(),
            tooltip,
            icon: vertex.icon.clone(),
            pos: vertex.pos.clone(),
            img: vertex.img.clone(),
            asset_width: vertex.asset_width,
            asset_height: vertex.asset_height,
            min_width: None,
            constraint: vertex.constraint.clone(),
            metadata: None,
        });
    }

    fn get_compiled_styles<'a>(
        &self,
        class_defs: impl IntoIterator<Item = &'a str>,
    ) -> Vec<String> {
        let mut compiled: Vec<String> = vec![];
        for name in class_defs {
            if let Some(class) = self.class(name) {
                compiled.extend(class.styles.iter().map(|style| style.trim().to_string()));
                compiled.extend(
                    class
                        .text_styles
                        .iter()
                        .map(|style| style.trim().to_string()),
                );
            }
        }
        compiled
    }
}

/// Where a `linkStyle` statement points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkPosition {
    Default,
    Index(usize),
}

fn walk_declarations(
    id: &str,
    children_of: &HashMap<String, Vec<String>>,
    index: &mut HashMap<String, usize>,
    next: &mut usize,
) {
    index.insert(id.to_string(), *next);
    *next += 1;
    if let Some(children) = children_of.get(id) {
        for child in children {
            walk_declarations(child, children_of, index, next);
        }
    }
}

fn unique_members(lines: &[StatementValue]) -> (Vec<String>, Option<String>) {
    let mut members = Vec::new();
    let mut seen = HashSet::new();
    let mut dir = None;
    for line in lines {
        match line {
            StatementValue::Nodes(ids) => {
                for id in ids {
                    if id.trim().is_empty() || !seen.insert(id.clone()) {
                        continue;
                    }
                    members.push(id.clone());
                }
            }
            StatementValue::Text(text) => {
                if text.trim().is_empty() || !seen.insert(text.clone()) {
                    continue;
                }
                members.push(text.clone());
            }
            StatementValue::Direction(value) => dir = Some(value.clone()),
        }
    }
    (members, dir)
}

/// `makeUniq`: removes the ids another subgraph already lists.
pub fn make_uniq(nodes: &[String], all_subgraphs: &[SubGraph]) -> Vec<String> {
    nodes
        .iter()
        .filter(|node| !exists(all_subgraphs, node))
        .cloned()
        .collect()
}

/// `exists`: whether a subgraph lists `id`.
pub fn exists(all_subgraphs: &[SubGraph], id: &str) -> bool {
    all_subgraphs
        .iter()
        .any(|sub_graph| sub_graph.nodes.iter().any(|node| node == id))
}

fn sanitize_node_label_type(value: Option<&Value>) -> LabelType {
    match value.and_then(Value::as_str) {
        Some("text") => LabelType::Text,
        Some("string") => LabelType::String,
        Some("markdown") => LabelType::Markdown,
        _ => LabelType::Markdown,
    }
}

fn step_value(value: &Value) -> Option<u32> {
    match value {
        Value::Number(number) => {
            let number = number.as_f64()?;
            if number.fract() == 0.0 && number > 0.0 && number <= u32::MAX as f64 {
                Some(number as u32)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

fn format_url(link: &str) -> Option<String> {
    let url = link.trim();
    if url.is_empty() {
        return None;
    }
    let compact: String = url
        .chars()
        .filter(|ch| !ch.is_whitespace() && !ch.is_control())
        .collect::<String>()
        .to_ascii_lowercase();
    for scheme in ["javascript:", "vbscript:", "data:"] {
        if compact.starts_with(scheme) {
            return Some("about:blank".to_string());
        }
    }
    Some(url.to_string())
}

fn get_type_from_vertex(vertex: &Vertex) -> &str {
    if vertex.img.is_some() {
        return "imageSquare";
    }
    if vertex.icon.is_some() {
        return match vertex.form.as_deref() {
            Some("circle") => "iconCircle",
            Some("square") => "iconSquare",
            Some("rounded") => "iconRounded",
            _ => "icon",
        };
    }
    match vertex.type_name.as_deref() {
        Some("square") | None => "squareRect",
        Some("round") => "roundedRect",
        Some(other) => other,
    }
}

fn destruct_edge_type(type_name: Option<ArrowType>) -> (ArrowEnd, ArrowEnd) {
    let mut start = ArrowEnd::None;
    let mut end = ArrowEnd::Point;
    match type_name {
        Some(ArrowType::Point) => end = ArrowEnd::Point,
        Some(ArrowType::Circle) => end = ArrowEnd::Circle,
        Some(ArrowType::Cross) => end = ArrowEnd::Cross,
        Some(ArrowType::DoublePoint) => {
            start = ArrowEnd::Point;
            end = ArrowEnd::Point;
        }
        Some(ArrowType::DoubleCircle) => {
            start = ArrowEnd::Circle;
            end = ArrowEnd::Circle;
        }
        Some(ArrowType::DoubleCross) => {
            start = ArrowEnd::Cross;
            end = ArrowEnd::Cross;
        }
        Some(ArrowType::Open) | Some(ArrowType::Invalid) | None => {}
    }
    (start, end)
}

pub fn destruct_start_link(text: &str) -> LinkInfo {
    let trimmed = text.trim();
    let mut rest = trimmed;
    let mut type_name = ArrowType::Open;
    if let Some(first) = trimmed.chars().next() {
        let matched = match first {
            '<' => Some(ArrowType::Point),
            'x' => Some(ArrowType::Cross),
            'o' => Some(ArrowType::Circle),
            _ => None,
        };
        if let Some(matched) = matched {
            type_name = matched;
            rest = &trimmed[first.len_utf8()..];
        }
    }
    let mut stroke = Stroke::Normal;
    if rest.contains('=') {
        stroke = Stroke::Thick;
    }
    if rest.contains('.') {
        stroke = Stroke::Dotted;
    }
    LinkInfo {
        type_name,
        stroke,
        length: None,
    }
}

pub fn destruct_end_link(text: &str) -> LinkInfo {
    let trimmed = text.trim();
    let mut line = if trimmed.is_empty() {
        ""
    } else {
        &trimmed[..trimmed.len() - 1]
    };
    let mut type_name = ArrowType::Open;
    match trimmed.chars().last() {
        Some('x') => {
            type_name = ArrowType::Cross;
            if trimmed.starts_with('x') {
                type_name = ArrowType::DoubleCross;
                line = line.get(1..).unwrap_or("");
            }
        }
        Some('>') => {
            type_name = ArrowType::Point;
            if trimmed.starts_with('<') {
                type_name = ArrowType::DoublePoint;
                line = line.get(1..).unwrap_or("");
            }
        }
        Some('o') => {
            type_name = ArrowType::Circle;
            if trimmed.starts_with('o') {
                type_name = ArrowType::DoubleCircle;
                line = line.get(1..).unwrap_or("");
            }
        }
        _ => {}
    }
    let mut stroke = Stroke::Normal;
    let mut length = line.chars().count() as i64 - 1;
    if line.starts_with('=') {
        stroke = Stroke::Thick;
    }
    if line.starts_with('~') {
        stroke = Stroke::Invisible;
    }
    let dots = line.chars().filter(|ch| *ch == '.').count();
    if dots > 0 {
        stroke = Stroke::Dotted;
        length = dots as i64;
    }
    LinkInfo {
        type_name,
        stroke,
        length: Some(length),
    }
}
