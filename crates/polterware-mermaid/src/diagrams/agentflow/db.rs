//! The agentflow database: a port of upstream `agentflowDb.ts`.
//!
//! Every method mirrors the one upstream's grammar actions call (`addVertex`, `addLink`,
//! `addSubGraph`, `addGlobal`, `addConnector`, `destructLink`, ...), and [`Db::build_data`]
//! mirrors `getData`, which turns the raw grammar state into the nodes and edges the renderer
//! consumes and runs the shape and containment diagnostics. DOM wiring, click callbacks,
//! configuration defaults, and colour-slot assignment stay out; nothing here lays out or draws.

use std::collections::HashSet;

use serde_json::{Map, Value};

use super::ast::{
    ArrowEnd, ArrowType, Ast, CONTAINMENT_VIOLATION, Diagnostic, Edge, ElementMapping,
    ElementPosition, FlowClass, LabelType, LayoutEdge, MappingType, Metadata, Node, SHAPE_REMOVED,
    SHAPE_UNSUPPORTED, Severity, Stroke, SubGraph, Vertex,
};
use super::metadata;
use crate::error::{ParseError, Span};

const MERMAID_DOM_ID_PREFIX: &str = "agentflow-";
const DEFAULT_MAX_EDGES: usize = 500;

/// A symbol's end: the 1-based line, the 0-based exclusive column, and the byte offset.
pub type EndPosition = (u32, u32, usize);

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

/// One end of a link as `destructLink` reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkInfo {
    pub type_name: ArrowType,
    pub stroke: Stroke,
    pub length: Option<i64>,
    pub edge_semantic: Option<super::ast::EdgeSemantic>,
}

/// A line of a document, as the grammar's `document` collects it.
#[derive(Clone, Debug, PartialEq)]
pub enum StatementValue {
    /// A `vertexStatement`'s node ids, in the order the grammar produced them.
    Nodes(Vec<String>),
    /// A line that reduces to a string (`SEMI`, `NEWLINE`, `SPACE`, a subgraph id, a click id).
    Text(String),
    /// A `direction` statement; upstream filters it out of the member list and keeps its value.
    Direction(String),
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
    connectors: Vec<String>,
    global_nodes: Vec<String>,
    sub_count: u32,
    direction: Option<String>,
    acc_title: String,
    acc_description: String,
    frontmatter_line_offset: u32,
    element_mappings: Vec<ElementMapping>,
    bare_mappings: HashSet<usize>,
    diagnostics: Vec<Diagnostic>,
}

impl Db {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_frontmatter_line_offset(&mut self, offset: u32) {
        self.frontmatter_line_offset = offset;
    }

    pub fn set_acc_title(&mut self, title: &str) {
        self.acc_title = metadata::sanitize_text(title).trim().to_string();
    }

    pub fn set_acc_description(&mut self, description: &str) {
        self.acc_description = description.trim().to_string();
    }

    /// `setDirection`.
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
        metadata_span: Span,
    ) -> Result<(), ParseError> {
        if id.trim().is_empty() {
            return Ok(());
        }
        let mut doc = match metadata_text {
            Some(text) => Some(parse_metadata(
                text,
                metadata_span,
                self.frontmatter_line_offset,
            )?),
            None => None,
        };

        // Resolve a shape alias before any other shape handling, keeping the authored name for
        // the lowercase check.
        let mut authored_shape = None;
        if let Some(Value::Object(map)) = &mut doc
            && let Some(Value::String(shape)) = map.get("shape")
        {
            let authored = shape.clone();
            authored_shape = Some(authored.clone());
            if let Some(resolved) = metadata::resolve_shape_alias(Some(&authored))
                && resolved != authored
            {
                map.insert("shape".to_string(), Value::String(resolved.to_string()));
            }
        }

        // A subgraph id with metadata attaches it to the container.
        if doc.is_some()
            && let Some(index) = self.sub_graphs.iter().position(|sg| sg.id == id)
        {
            if let Some(Value::Object(map)) = &doc {
                merge_metadata(&mut self.sub_graphs[index].metadata, map);
            }
            return Ok(());
        }

        // A connector declared with the `connector` keyword.
        if doc.is_some()
            && self.connectors.iter().any(|connector| connector == id)
            && let Some(index) = self.vertices.iter().position(|vertex| vertex.id == id)
        {
            if let Some(Value::Object(map)) = &doc {
                merge_metadata(&mut self.vertices[index].metadata, map);
            }
            return Ok(());
        }

        // `connectors` is reserved as a declaration group id.
        if id == "connectors" {
            return Ok(());
        }

        // Edge metadata routes to the edge.
        if let Some(index) = self
            .edges
            .iter()
            .position(|edge| edge.id.as_deref() == Some(id))
        {
            if let Some(Value::Object(map)) = &doc {
                let edge = &mut self.edges[index];
                if let Some(Value::Bool(animate)) = map.get("animate") {
                    edge.animate = Some(*animate);
                }
                if let Some(Value::String(animation)) = map.get("animation") {
                    edge.animation = Some(animation.clone());
                }
                if let Some(Value::String(curve)) = map.get("curve") {
                    edge.interpolate = Some(curve.clone());
                }
                merge_metadata(&mut edge.metadata, map);
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
                    metadata: None,
                    is_connector: false,
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

        if let Some(Value::Object(map)) = &doc {
            self.apply_vertex_metadata(index, map, authored_shape, metadata_span)?;
        }
        Ok(())
    }

    fn apply_vertex_metadata(
        &mut self,
        index: usize,
        document: &Map<String, Value>,
        authored_shape: Option<String>,
        span: Span,
    ) -> Result<(), ParseError> {
        let map = document.clone();
        merge_metadata(&mut self.vertices[index].metadata, &map);
        if let Some(shape) = map.get("shape").filter(|value| metadata::is_truthy(value)) {
            let authored = authored_shape
                .or_else(|| shape.as_str().map(str::to_string))
                .unwrap_or_else(|| shape.to_string());
            if !shape.is_string() {
                return Err(ParseError::at(
                    format!("No such shape: {}.", json_text(shape)),
                    span,
                ));
            }
            if authored != authored.to_lowercase() || authored.contains('_') {
                return Err(ParseError::at(
                    format!("No such shape: {authored}. Shape names should be lowercase."),
                    span,
                ));
            }
            let Some(shape_name) = shape.as_str() else {
                return Err(ParseError::at(
                    format!("No such shape: {}.", json_text(shape)),
                    span,
                ));
            };
            if !metadata::is_valid_shape(shape_name) {
                return Err(ParseError::at(
                    format!("No such shape: {shape_name}."),
                    span,
                ));
            }
            self.vertices[index].type_name = Some(shape_name.to_string());
        }
        if let Some(label) = map.get("label").filter(|value| metadata::is_truthy(value))
            && let Some(label) = label.as_str()
        {
            self.vertices[index].text = Some(metadata::sanitize_text(label));
            self.vertices[index].label_type = sanitize_node_label_type(map.get("labelType"));
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
        let last_start = starts.last();
        let first_end = ends.first();
        for start in starts {
            for end in ends {
                let keeps_id = Some(start) == last_start && Some(end) == first_end;
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
            edge_semantic: link.semantic(),
            style: None,
            length: link.length.map(|length| length.min(10)),
            text: String::new(),
            label_type: LabelType::Text,
            classes: vec![],
            id: None,
            animation: None,
            animate: None,
            metadata: None,
        };
        if let Some(text) = &link.text {
            let mut label = metadata::sanitize_text(text.text.trim());
            if label.starts_with('"') && label.ends_with('"') && label.len() >= 2 {
                label = label[1..label.len() - 1].to_string();
            }
            edge.text = label;
            edge.label_type = sanitize_node_label_type(Some(&Value::String(
                text.label_type.as_str().to_string(),
            )));
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
                    let edge = self
                        .edges
                        .get_mut(*index)
                        .ok_or_else(|| link_style_bounds_error(*index, count, span))?;
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
                        return Err(link_style_bounds_error(*index, self.edges.len(), span));
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
            if let Some(sub_graph) = self.sub_graphs.iter_mut().find(|sg| sg.id == id) {
                sub_graph.classes.push(class_name.to_string());
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

    /// `setClickEvent`: registers a DOM handler upstream; it only marks nodes clickable here.
    pub fn set_click_event(&mut self, ids: &str) {
        self.set_class(ids, "clickable");
    }

    /// `addSubGraph`: dedupes the body's lines, filters members already owned elsewhere, and
    /// merges a repeated id into the first declaration.
    pub fn add_sub_graph(
        &mut self,
        id: Option<&FlowText>,
        title: Option<&FlowText>,
        type_name: Option<&str>,
        lines: &[StatementValue],
    ) -> String {
        let mut id = id.map(|text| text.text.trim().to_string());
        let title_text = title.map(|text| text.text.clone()).unwrap_or_default();
        let label_type = title.map_or(LabelType::Markdown, |text| text.label_type);
        if id.as_deref() == Some("") {
            id = None;
        }
        let (members, dir) = unique_members(lines);
        let id = id.unwrap_or_else(|| format!("subGraph{}", self.sub_count));
        self.sub_count += 1;
        let title = metadata::sanitize_text(&title_text).trim().to_string();

        let existed = self.sub_graphs.iter().any(|sg| sg.id == id);
        let unique = make_uniq(&members, &self.sub_graphs);
        let kept: Vec<String> = unique
            .into_iter()
            .filter(|node| node != &id && !self.global_nodes.contains(node))
            .collect();
        if existed {
            let existing = self
                .sub_graphs
                .iter_mut()
                .find(|sg| sg.id == id)
                .expect("existed checked above");
            for node in kept {
                if !existing.nodes.contains(&node) {
                    existing.nodes.push(node);
                }
            }
            if !title.is_empty() {
                existing.title = title;
            }
            if dir.is_some() {
                existing.dir = dir;
            }
            if let Some(type_name) = type_name {
                existing.type_name = Some(type_name.to_string());
            }
        } else {
            self.sub_graphs.push(SubGraph {
                id: id.clone(),
                title,
                nodes: kept,
                classes: vec![],
                dir,
                label_type,
                type_name: type_name.map(str::to_string),
                metadata: None,
            });
        }
        id
    }

    /// `addGlobal`: ids inside a `global ... end` block keep no parent, whatever order the
    /// flow blocks that name them appear in.
    pub fn add_global(&mut self, lines: &[StatementValue]) {
        for line in lines {
            match line {
                StatementValue::Nodes(ids) => {
                    for id in ids {
                        self.push_global(id);
                    }
                }
                StatementValue::Text(text) => self.push_global(text),
                StatementValue::Direction(_) => {}
            }
        }
        for sub_graph in &mut self.sub_graphs {
            sub_graph
                .nodes
                .retain(|node| !self.global_nodes.contains(node));
        }
    }

    fn push_global(&mut self, raw: &str) {
        let id = raw.trim();
        if !id.is_empty() && !self.global_nodes.iter().any(|node| node == id) {
            self.global_nodes.push(id.to_string());
        }
    }

    /// `addConnector`: stores the connector as a vertex with `is_connector`.
    pub fn add_connector(&mut self, text: &FlowText, title: Option<&FlowText>) -> String {
        let id = text.text.trim().to_string();
        if id.is_empty() {
            return String::new();
        }
        let title = title.map(|text| text.text.trim()).unwrap_or("");
        let resolved_title = if title.is_empty() {
            id.clone()
        } else {
            metadata::sanitize_text(title)
        };
        if let Some(index) = self
            .connectors
            .iter()
            .position(|connector| *connector == id)
        {
            if !title.is_empty()
                && let Some(vertex) = self.vertices.iter_mut().find(|vertex| vertex.id == id)
            {
                vertex.text = Some(resolved_title);
                let _ = index;
            }
        } else {
            let vertex = Vertex {
                id: id.clone(),
                text: Some(resolved_title),
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
                metadata: None,
                is_connector: true,
            };
            self.vertices.push(vertex);
            self.connectors.push(id.clone());
            self.vertex_counter += 1;
        }
        id
    }

    /// `destructLink`: reads both ends and combines them.
    pub fn destruct_link(&self, end: &str, start: Option<&str>) -> LinkInfo {
        let info = destruct_end_link(end);
        match start {
            Some(start) => {
                let start_info = destruct_start_link(start);
                if start_info.stroke != info.stroke {
                    return LinkInfo {
                        type_name: ArrowType::Invalid,
                        stroke: Stroke::Invalid,
                        length: None,
                        edge_semantic: None,
                    };
                }
                LinkInfo {
                    type_name: info.type_name,
                    stroke: info.stroke,
                    length: info.length,
                    edge_semantic: compute_edge_semantic(info.type_name, info.stroke),
                }
            }
            None => info,
        }
    }

    pub fn add_vertex_mapping(&mut self, id: &str, has_shape: bool, position: ElementPosition) {
        if id.is_empty() {
            return;
        }
        self.element_mappings.push(ElementMapping {
            id: id.to_string(),
            type_name: MappingType::Vertex,
            position,
        });
        if !has_shape {
            self.bare_mappings.insert(self.element_mappings.len() - 1);
        }
    }

    /// `extendVertexMapping`: widen a declaration to cover a trailing `@{ ... }` block, and
    /// retype an annotation on a bare reference as an attachment.
    pub fn extend_vertex_mapping(&mut self, id: &str, position: ElementPosition) {
        if id.is_empty() {
            return;
        }
        for index in (0..self.element_mappings.len()).rev() {
            let mapping = &self.element_mappings[index];
            if mapping.type_name != MappingType::Vertex || mapping.id != id {
                continue;
            }
            if self.bare_mappings.remove(&index) {
                self.element_mappings[index].type_name = MappingType::Attachment;
            }
            let target = &mut self.element_mappings[index].position;
            let extends_past_end = position.end_line > target.end_line
                || (position.end_line == target.end_line
                    && position.end_column > target.end_column);
            if extends_past_end {
                target.end_line = position.end_line;
                target.end_column = position.end_column;
                target.end_index = position.end_index;
            }
            return;
        }
        self.element_mappings.push(ElementMapping {
            id: id.to_string(),
            type_name: MappingType::Attachment,
            position,
        });
    }

    pub fn add_edge_mapping(&mut self, to_nodes: &[String], position: ElementPosition) {
        let id = if to_nodes.is_empty() {
            "edge".to_string()
        } else {
            to_nodes.join(">")
        };
        self.element_mappings.push(ElementMapping {
            id,
            type_name: MappingType::Edge,
            position,
        });
    }

    pub fn add_subgraph_mapping(&mut self, id: &str, start: Span, end: (u32, u32, usize)) {
        if id.is_empty() {
            return;
        }
        let position = position_from(start, self.frontmatter_line_offset, Some(end));
        self.element_mappings.push(ElementMapping {
            id: id.to_string(),
            type_name: MappingType::SubGraph,
            position,
        });
    }

    pub fn add_connector_mapping(&mut self, id: &str, start: Span, end: (u32, u32, usize)) {
        if id.is_empty() {
            return;
        }
        let position = position_from(start, self.frontmatter_line_offset, Some(end));
        self.element_mappings.push(ElementMapping {
            id: id.to_string(),
            type_name: MappingType::Connector,
            position,
        });
    }

    /// The position a mapping records: `start` is the first symbol, `end` the last symbol's
    /// (line, exclusive 0-based column).
    pub fn position(&self, start: Span, end: Option<EndPosition>) -> ElementPosition {
        position_from(start, self.frontmatter_line_offset, end)
    }

    pub fn into_ast(mut self) -> Ast {
        let (nodes, layout_edges) = self.build_data();
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
            connectors: self.connectors,
            global_nodes: self.global_nodes,
            element_mappings: self.element_mappings,
            diagnostics: self.diagnostics,
            nodes,
            layout_edges,
        }
    }

    /// `getData`: the nodes and edges the renderer consumes.
    fn build_data(&mut self) -> (Vec<Node>, Vec<LayoutEdge>) {
        let sub_graphs = self.sub_graphs.clone();
        let mut parent_db: Vec<(String, String)> = Vec::new();
        let mut sub_graph_db: Vec<String> = Vec::new();

        let mut hidden_ids: Vec<String> = Vec::new();
        let mut collapsed_ancestor: Vec<(String, String)> = Vec::new();
        for sub_graph in &sub_graphs {
            if sub_graph.metadata.as_ref().and_then(|m| m.get("view"))
                == Some(&Value::String("collapsed".to_string()))
                && !hidden_ids.contains(&sub_graph.id)
            {
                let mut seen = HashSet::new();
                self.collect_descendants(
                    &sub_graphs,
                    &sub_graph.id,
                    &sub_graph.id,
                    &mut seen,
                    &mut hidden_ids,
                    &mut collapsed_ancestor,
                );
            }
        }

        for sub_graph in sub_graphs.iter().rev() {
            if hidden_ids.contains(&sub_graph.id) {
                continue;
            }
            if !sub_graph.nodes.is_empty() && !sub_graph_db.contains(&sub_graph.id) {
                sub_graph_db.push(sub_graph.id.clone());
            }
            for id in &sub_graph.nodes {
                if would_close_containment_cycle(id, &sub_graph.id, &parent_db) {
                    let message = format!(
                        "Container \"{}\" cannot contain \"{}\" because \"{}\" already contains it. The nesting that would close the loop is dropped.",
                        sub_graph.id, id, id
                    );
                    self.emit_warning(CONTAINMENT_VIOLATION, &message, Some(id), None);
                    continue;
                }
                parent_db.retain(|(node, _)| node != id);
                parent_db.push((id.clone(), sub_graph.id.clone()));
            }
        }

        let mut nodes = Vec::new();
        for sub_graph in sub_graphs.iter().rev() {
            if hidden_ids.contains(&sub_graph.id) {
                continue;
            }
            let collapsed = sub_graph.metadata.as_ref().and_then(|m| m.get("view"))
                == Some(&Value::String("collapsed".to_string()));
            let metadata = if collapsed {
                let mut metadata = sub_graph.metadata.clone().unwrap_or_default();
                metadata.insert(
                    "containerType".to_string(),
                    sub_graph
                        .type_name
                        .clone()
                        .map_or(Value::Null, Value::String),
                );
                Some(metadata)
            } else {
                sub_graph.metadata.clone()
            };
            nodes.push(Node {
                id: sub_graph.id.clone(),
                label: sub_graph.title.clone(),
                label_type: sub_graph.label_type,
                parent_id: parent_of(&parent_db, &sub_graph.id),
                padding: 8.0,
                css_styles: vec![],
                css_compiled_styles: self.get_compiled_styles(&sub_graph.classes),
                css_classes: sub_graph.classes.join(" "),
                shape: if collapsed {
                    "collapsedGroup"
                } else {
                    "flowGroup"
                }
                .to_string(),
                dir: sub_graph.dir.clone(),
                is_group: !collapsed,
                dom_id: None,
                link: None,
                link_target: None,
                metadata,
            });
        }

        let vertices = self.vertices.clone();
        for vertex in &vertices {
            if hidden_ids.contains(&vertex.id) {
                continue;
            }
            self.add_node_from_vertex(vertex, &mut nodes, &parent_db, &sub_graph_db);
        }

        let mut layout_edges = Vec::new();
        for (index, raw) in self.edges.iter().enumerate() {
            let start = collapsed_ancestor
                .iter()
                .find(|(id, _)| id == &raw.start)
                .map_or(raw.start.clone(), |(_, ancestor)| ancestor.clone());
            let end = collapsed_ancestor
                .iter()
                .find(|(id, _)| id == &raw.end)
                .map_or(raw.end.clone(), |(_, ancestor)| ancestor.clone());
            if start == end && raw.start != raw.end {
                continue;
            }
            let (arrow_start, arrow_end) = destruct_edge_type(raw.type_name);
            let mut styles = self.default_style.clone().unwrap_or_default();
            if let Some(style) = &raw.style {
                styles.extend(style.iter().cloned());
            }
            let invisible = raw.stroke == Some(Stroke::Invisible);
            let open = raw.type_name == Some(ArrowType::Open);
            layout_edges.push(LayoutEdge {
                id: raw
                    .id
                    .clone()
                    .unwrap_or_else(|| format!("L_{start}_{end}_{index}")),
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
                css_compiled_styles: self.get_compiled_styles(&raw.classes),
                label_style: styles.clone(),
                style: styles,
                pattern: raw.stroke,
                animate: raw.animate,
                animation: raw.animation.clone(),
                curve: raw
                    .interpolate
                    .clone()
                    .or_else(|| self.default_interpolate.clone()),
                metadata: raw.metadata.clone(),
            });
        }

        self.normalise_node_shapes(&mut nodes);
        (nodes, layout_edges)
    }

    fn collect_descendants(
        &self,
        sub_graphs: &[SubGraph],
        sg_id: &str,
        ancestor: &str,
        seen: &mut HashSet<String>,
        hidden: &mut Vec<String>,
        collapsed: &mut Vec<(String, String)>,
    ) {
        if !seen.insert(sg_id.to_string()) {
            return;
        }
        let Some(sub_graph) = sub_graphs.iter().find(|sg| sg.id == sg_id) else {
            return;
        };
        for child in &sub_graph.nodes {
            if child == ancestor || seen.contains(child) {
                continue;
            }
            if !hidden.contains(child) {
                hidden.push(child.clone());
            }
            collapsed.retain(|(id, _)| id != child);
            collapsed.push((child.clone(), ancestor.to_string()));
            self.collect_descendants(sub_graphs, child, ancestor, seen, hidden, collapsed);
        }
    }

    fn add_node_from_vertex(
        &self,
        vertex: &Vertex,
        nodes: &mut Vec<Node>,
        parent_db: &[(String, String)],
        sub_graph_db: &[String],
    ) {
        let is_group = sub_graph_db.contains(&vertex.id);
        if let Some(node) = nodes.iter_mut().find(|node| node.id == vertex.id) {
            node.css_styles = vertex.styles.clone();
            node.css_compiled_styles = self.get_compiled_styles(&vertex.classes);
            node.css_classes = vertex.classes.join(" ");
            return;
        }
        let mut metadata = vertex.metadata.clone();
        if is_group {
            // A group node keeps the container shape set above.
            if let Some(metadata) = &mut metadata {
                metadata
                    .entry("containerType".to_string())
                    .or_insert_with(|| Value::String("flow".to_string()));
            }
        }
        nodes.push(Node {
            id: vertex.id.clone(),
            label: vertex.text.clone().unwrap_or_default(),
            label_type: vertex.label_type,
            parent_id: parent_of(parent_db, &vertex.id),
            padding: 8.0,
            css_styles: vertex.styles.clone(),
            css_compiled_styles: if is_group {
                self.get_compiled_styles(&vertex.classes)
            } else {
                let mut classes = vec!["default".to_string(), "node".to_string()];
                classes.extend(vertex.classes.iter().cloned());
                self.get_compiled_styles(&classes)
            },
            css_classes: if is_group {
                vertex.classes.join(" ")
            } else {
                format!("default {}", vertex.classes.join(" "))
            },
            shape: if is_group {
                "rect".to_string()
            } else {
                get_type_from_vertex(vertex).to_string()
            },
            dir: vertex.dir.clone(),
            is_group,
            dom_id: Some(vertex.dom_id.clone()),
            link: vertex.link.clone(),
            link_target: vertex.link_target.clone(),
            metadata,
        });
    }

    fn get_compiled_styles(&self, class_defs: &[String]) -> Vec<String> {
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

    fn class(&self, id: &str) -> Option<&FlowClass> {
        self.classes.iter().find(|class| class.id == id)
    }

    /// `normaliseNodeShapes`: group nodes keep their cluster shape; every other node ends on
    /// a supported canonical shape, reporting removed shapes as errors and the rest as warnings.
    fn normalise_node_shapes(&mut self, nodes: &mut [Node]) {
        for node in nodes {
            if node.is_group {
                continue;
            }
            if node.shape.is_empty() || node.shape == "squareRect" || node.shape == "rect" {
                node.shape = metadata::DEFAULT_SHAPE.to_string();
            }
            if metadata::REMOVED_SHAPES.contains(&node.shape.as_str()) {
                let message = format!(
                    "shape \"{}\" was removed in v0.8.1, using \"{}\"",
                    node.shape,
                    metadata::DEFAULT_SHAPE
                );
                self.emit_diagnostic(
                    SHAPE_REMOVED,
                    Severity::Error,
                    &message,
                    Some(&node.id),
                    None,
                );
                node.shape = metadata::DEFAULT_SHAPE.to_string();
                continue;
            }
            if !metadata::ALLOWED_SHAPES.contains(&node.shape.as_str()) {
                let message = format!(
                    "shape \"{}\" is not supported, using \"{}\"",
                    node.shape,
                    metadata::DEFAULT_SHAPE
                );
                self.emit_diagnostic(
                    SHAPE_UNSUPPORTED,
                    Severity::Warning,
                    &message,
                    Some(&node.id),
                    None,
                );
                node.shape = metadata::DEFAULT_SHAPE.to_string();
            }
        }
    }

    fn emit_warning(
        &mut self,
        id: &str,
        message: &str,
        node_id: Option<&str>,
        edge_id: Option<&str>,
    ) {
        self.emit_diagnostic(id, Severity::Warning, message, node_id, edge_id);
    }

    fn emit_diagnostic(
        &mut self,
        id: &str,
        severity: Severity,
        message: &str,
        node_id: Option<&str>,
        edge_id: Option<&str>,
    ) {
        let anchor = node_id.or(edge_id);
        let position = anchor.and_then(|anchor| {
            self.element_mappings
                .iter()
                .find(|mapping| mapping.id == anchor)
                .map(|mapping| mapping.position)
        });
        let diagnostic = Diagnostic {
            id: id.to_string(),
            severity,
            message: message.to_string(),
            node_id: node_id.map(str::to_string),
            edge_id: if node_id.is_none() {
                edge_id.map(str::to_string)
            } else {
                None
            },
            position,
        };
        let duplicate = self.diagnostics.iter().any(|existing| {
            existing.id == diagnostic.id
                && existing.severity == diagnostic.severity
                && existing.message == diagnostic.message
                && existing.node_id == diagnostic.node_id
                && existing.edge_id == diagnostic.edge_id
        });
        if !duplicate {
            self.diagnostics.push(diagnostic);
        }
    }
}

/// Where a `linkStyle` statement points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkPosition {
    Default,
    Index(usize),
}

impl Link {
    fn semantic(&self) -> Option<super::ast::EdgeSemantic> {
        compute_edge_semantic(self.type_name, self.stroke)
    }
}

/// The §5.1 operator table.
fn compute_edge_semantic(type_name: ArrowType, stroke: Stroke) -> Option<super::ast::EdgeSemantic> {
    use super::ast::EdgeSemantic;
    match (type_name, stroke) {
        (ArrowType::Point, Stroke::Normal) => Some(EdgeSemantic::Sequence),
        (ArrowType::Cross, Stroke::Normal) => Some(EdgeSemantic::Failure),
        (ArrowType::Open, Stroke::Dotted) => Some(EdgeSemantic::Reference),
        _ => None,
    }
}

pub fn destruct_start_link(text: &str) -> LinkInfo {
    let trimmed = text.trim();
    let type_name = ArrowType::Open;
    let stroke = if trimmed.contains('.') {
        Stroke::Dotted
    } else {
        Stroke::Normal
    };
    LinkInfo {
        type_name,
        stroke,
        length: None,
        edge_semantic: None,
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
        Some('x') => type_name = ArrowType::Cross,
        Some('>') => type_name = ArrowType::Point,
        Some('-') | Some('.') => {
            type_name = ArrowType::Open;
            line = trimmed;
        }
        _ => {}
    }
    let mut stroke = Stroke::Normal;
    let mut length = line.chars().count() as i64 - 1;
    let dots = line.chars().filter(|ch| *ch == '.').count();
    if dots > 0 {
        stroke = Stroke::Dotted;
        length = dots as i64;
    }
    LinkInfo {
        type_name,
        stroke,
        length: Some(length),
        edge_semantic: compute_edge_semantic(type_name, stroke),
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

fn get_type_from_vertex(vertex: &Vertex) -> &str {
    match metadata::resolve_shape_alias(vertex.type_name.as_deref()) {
        Some("square") => "squareRect",
        Some("round") => "roundedRect",
        Some("ellipse") => "ellipse",
        Some(other) => other,
        None => "squareRect",
    }
}

fn parse_metadata(text: &str, span: Span, offset: u32) -> Result<Value, ParseError> {
    let mut result = metadata::parse(text);
    if result.is_err() && text.contains('\n') {
        let stripped = metadata::strip_line_trailing_commas(text);
        if stripped != text {
            result = metadata::parse(&stripped);
        }
    }
    match result {
        Ok(value) => {
            let value = metadata::strip_prototype_keys(coalesce_null(value));
            Ok(value)
        }
        Err(error) => {
            let inline = !text.contains('\n');
            let line = span.line + offset + if inline { 0 } else { error.line as u32 };
            let column = if error.line == 0 {
                span.column + 2 + error.column as u32
            } else {
                error.column as u32 + 1
            };
            let error_span = Span {
                line,
                column,
                start: span.start,
                end: span.end,
            };
            Err(ParseError::at(
                format!("{} ({line}:{column})", error.message),
                error_span,
            ))
        }
    }
}

fn coalesce_null(value: Value) -> Value {
    match value {
        Value::Null => Value::Object(Map::new()),
        other => other,
    }
}

fn merge_metadata(target: &mut Option<Metadata>, source: &Map<String, Value>) {
    let target = target.get_or_insert_with(Map::new);
    for (key, value) in source {
        target.insert(key.clone(), value.clone());
    }
}

fn sanitize_node_label_type(value: Option<&Value>) -> LabelType {
    match value.and_then(Value::as_str) {
        Some("text") => LabelType::Text,
        Some("string") => LabelType::String,
        Some("markdown") => LabelType::Markdown,
        _ => LabelType::Markdown,
    }
}

fn json_text(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
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
fn make_uniq(nodes: &[String], all_subgraphs: &[SubGraph]) -> Vec<String> {
    nodes
        .iter()
        .filter(|node| {
            !all_subgraphs
                .iter()
                .any(|sub_graph| sub_graph.nodes.iter().any(|member| member == *node))
        })
        .cloned()
        .collect()
}

fn would_close_containment_cycle(
    child_id: &str,
    parent_id: &str,
    parent_db: &[(String, String)],
) -> bool {
    let mut seen = HashSet::new();
    let mut current = Some(parent_id.to_string());
    while let Some(node) = current {
        if node == child_id {
            return true;
        }
        if !seen.insert(node.clone()) {
            return false;
        }
        current = parent_of(parent_db, &node);
    }
    false
}

fn parent_of(parent_db: &[(String, String)], id: &str) -> Option<String> {
    parent_db
        .iter()
        .find(|(node, _)| node == id)
        .map(|(_, parent)| parent.clone())
}

fn link_style_bounds_error(index: usize, count: usize, span: Span) -> ParseError {
    ParseError::at(
        format!(
            "The index {index} for linkStyle is out of bounds. Valid indices for linkStyle are between 0 and {}. (Help: Ensure that the index is within the range of existing edges.)",
            count.saturating_sub(1)
        ),
        span,
    )
}

fn position_from(start: Span, offset: u32, end: Option<EndPosition>) -> ElementPosition {
    let (end_line, end_column, end_index) =
        end.unwrap_or((start.line, start.column.saturating_sub(1), start.end));
    ElementPosition {
        start_line: start.line + offset,
        start_column: start.column.saturating_sub(1),
        end_line: end_line + offset,
        end_column,
        start_index: start.start,
        end_index,
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
