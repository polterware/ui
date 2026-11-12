//! Conversion of the use case database projection into the shared layout pipeline's
//! [`LayoutData`], the Rust side of `usecaseDb.ts` `getData()` and the preparation
//! `usecaseRenderer.ts` `draw` applies before `render`.
//!
//! `getData` numbers actors and use cases from one colour cycle, boundaries from a second,
//! and hands over the relationship and note edges. `draw` then copies the configuration it
//! reads (type, layout algorithm, direction, spacing, fonts, markers, padding, `useMaxWidth`)
//! and `prepareUsecaseLayoutData` escapes plain labels, folds an ellipse/rect stereotype into
//! a markdown first line, and gives every node its DOM id.
//!
//! The fonts are the use case stylesheet's: an actor label is measured at
//! `actorFontSize` (it matches `.actor-label`) and a business use case label at
//! `usecaseFontSize` (`.usecase-label`). Every other label — an ellipse or rectangle label,
//! a stereotype, a note, a JSON table, a boundary title, and an edge label, which upstream
//! measures before it joins the `.edgeLabel` group — matches no rule with a size of its own
//! and keeps the theme's `fontSize`.

use serde_json::{Map, Value};

use crate::config::Config;
use crate::render::pipeline::{
    Edge as PipelineEdge, JsonRowData, LayoutData, Node as PipelineNode,
    get_registered_layout_algorithm,
};

use super::ast::{
    Actor, ActorType, ArrowType, Ast, LabelType, Relationship, RelationshipType, UseCase,
    UseCaseShape,
};

/// The diagram id `render()` prefixes DOM ids with when the caller names none.
pub const DEFAULT_DIAGRAM_ID: &str = "mermaid";

/// The diagram type upstream registers the unified use case renderer under.
pub const DIAGRAM_TYPE: &str = "usecase";

/// `USECASE_MARKERS`.
pub const MARKERS: [&str; 4] = ["point", "circle", "cross", "extension"];

/// Builds the layout data for a parsed use case diagram, prepared as `draw` prepares it.
pub fn layout_data(ast: &Ast, config: &Config, diagram_id: &str) -> LayoutData {
    let usecase = &config.usecase;
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut color_index = 0usize;
    let mut boundary_color_index = 0usize;

    for actor in &ast.actors {
        let slot = count_then_increment(&mut color_index);
        nodes.push(actor_node(actor, config, slot));
    }
    for use_case in &ast.use_cases {
        let slot = count_then_increment(&mut color_index);
        nodes.push(use_case_node(use_case, config, slot));
    }
    for note in &ast.notes {
        nodes.push(note_node(note, config));
    }
    for json in &ast.json_nodes {
        nodes.push(json_node(json, config));
    }
    for boundary in &ast.system_boundaries {
        let slot = count_then_increment(&mut boundary_color_index);
        nodes.push(boundary_node(boundary, config, slot));
    }

    for relationship in &ast.relationships {
        edges.push(relationship_edge(relationship, config));
    }
    for note in &ast.notes {
        edges.push(note_edge(note, config));
    }

    let mut data = LayoutData {
        nodes,
        edges,
        config: config.clone(),
        diagram_id: Some(diagram_id.to_string()),
        layout_algorithm: Some(get_registered_layout_algorithm(&config.layout, None)),
        direction: Some(ast.direction.as_str().to_string()),
        r#type: Some(DIAGRAM_TYPE.to_string()),
        markers: MARKERS.iter().map(|marker| marker.to_string()).collect(),
        use_max_width: Some(usecase.use_max_width),
        padding: Some(usecase.diagram_padding),
        node_spacing: Some(usecase.node_spacing),
        rank_spacing: Some(usecase.rank_spacing),
        root_node: None,
    };
    prepare_layout_data(&mut data, ast);
    data
}

/// The cycle index a node takes; the counter is incremented after the value is read, as
/// `colorIndex++` does.
fn count_then_increment(counter: &mut usize) -> usize {
    let value = *counter;
    *counter += 1;
    value
}

fn compiled_styles(ast: &Ast, classes: &[String]) -> Vec<String> {
    let mut compiled: Vec<String> = Vec::new();
    let mut index_of = std::collections::HashMap::new();
    for class_name in std::iter::once("default").chain(classes.iter().map(String::as_str)) {
        let Some(definition) = ast.class_def(class_name) else {
            continue;
        };
        for raw_style in &definition.styles {
            let style = raw_style.trim();
            let separator = style.find(':');
            let property = match separator {
                Some(index) => style[..index].trim(),
                None => style,
            };
            if property.is_empty() {
                continue;
            }
            match index_of.get(property) {
                Some(&existing) => compiled[existing] = style.to_string(),
                None => {
                    index_of.insert(property.to_string(), compiled.len());
                    compiled.push(style.to_string());
                }
            }
        }
    }
    compiled
}

fn actor_shape(actor_type: ActorType) -> &'static str {
    match actor_type {
        ActorType::Normal => "usecaseActor",
        ActorType::Hollow => "usecaseActorHollow",
        ActorType::Awesome => "usecaseActorAwesome",
        ActorType::Icon => "usecaseActorIcon",
    }
}

fn use_case_shape(use_case: &UseCase) -> String {
    if use_case.shape == UseCaseShape::Ellipse {
        if use_case.business {
            "usecaseBusiness".to_string()
        } else {
            "usecaseEllipse".to_string()
        }
    } else {
        use_case.shape.as_str().to_string()
    }
}

fn actor_node(actor: &Actor, config: &Config, slot: usize) -> PipelineNode {
    let usecase = &config.usecase;
    let mut classes = vec![
        "default".to_string(),
        "usecase-actor".to_string(),
        format!("usecase-actor-{}", actor.type_.as_str()),
    ];
    if actor.business {
        classes.push("usecase-business".to_string());
    }
    classes.extend(actor.classes.iter().cloned());
    PipelineNode {
        id: actor.id.clone(),
        label: Some(actor.label.clone()),
        label_type: Some(actor.label_type.as_str().to_string()),
        shape: Some(actor_shape(actor.type_).to_string()),
        is_group: Some(false),
        padding: Some(10.0),
        look: Some(config.look.clone()),
        color_index: Some(slot),
        css_classes: Some(classes.join(" ")),
        css_styles: Some(actor.styles.clone()),
        business: Some(actor.business),
        icon: actor.icon.clone(),
        stereotype: actor.stereotype.clone(),
        parent_id: actor.parent_id.clone(),
        wrapping_width: Some(usecase.wrapping_width),
        label_font_size: Some(usecase.actor_font_size),
        ..PipelineNode::default()
    }
}

fn use_case_node(use_case: &UseCase, config: &Config, slot: usize) -> PipelineNode {
    let usecase = &config.usecase;
    let shape = use_case_shape(use_case);
    let mut classes = vec![
        "default".to_string(),
        "usecase-element".to_string(),
        format!("usecase-{}", use_case.shape.as_str()),
    ];
    if use_case.business {
        classes.push("usecase-business".to_string());
    }
    classes.extend(use_case.classes.iter().cloned());
    PipelineNode {
        id: use_case.id.clone(),
        label: Some(use_case.label.clone()),
        label_type: Some(use_case.label_type.as_str().to_string()),
        shape: Some(shape.clone()),
        is_group: Some(false),
        padding: Some(if use_case.shape == UseCaseShape::Ellipse {
            20.0
        } else {
            10.0
        }),
        look: Some(config.look.clone()),
        color_index: Some(slot),
        css_classes: Some(classes.join(" ")),
        css_styles: Some(use_case.styles.clone()),
        business: Some(use_case.business),
        stereotype: use_case.stereotype.clone(),
        parent_id: use_case.parent_id.clone(),
        wrapping_width: Some(usecase.wrapping_width),
        min_width: Some(usecase.min_node_width),
        spread_ports: (shape == "usecaseEllipse" || shape == "usecaseBusiness").then_some(true),
        // The business shape's label carries `.usecase-label`; the plain ellipse and
        // rectangle labels match no sized rule and keep the theme's font size.
        label_font_size: (shape == "usecaseBusiness").then_some(usecase.usecase_font_size),
        ..PipelineNode::default()
    }
}

fn note_node(note: &super::ast::UsecaseNote, config: &Config) -> PipelineNode {
    PipelineNode {
        id: note.id.clone(),
        label: Some(note.label.clone()),
        label_type: Some(note.label_type.as_str().to_string()),
        shape: Some("note".to_string()),
        is_group: Some(false),
        padding: Some(10.0),
        look: Some(config.look.clone()),
        css_classes: Some("default usecase-note".to_string()),
        css_styles: Some(Vec::new()),
        css_compiled_styles: Some(Vec::new()),
        wrapping_width: Some(config.usecase.wrapping_width),
        min_width: Some(config.usecase.min_node_width),
        ..PipelineNode::default()
    }
}

fn json_node(json: &super::ast::UsecaseJsonNode, config: &Config) -> PipelineNode {
    let mut classes = vec!["default".to_string(), "usecase-json-table".to_string()];
    classes.extend(json.classes.iter().cloned());
    PipelineNode {
        id: json.id.clone(),
        label: Some(json.id.clone()),
        label_type: Some(LabelType::Text.as_str().to_string()),
        shape: Some("usecaseJsonTable".to_string()),
        is_group: Some(false),
        padding: Some(10.0),
        look: Some(config.look.clone()),
        css_classes: Some(classes.join(" ")),
        css_styles: Some(json.styles.clone()),
        json_rows: flatten_json_rows(&json.value, &json.property_order),
        wrapping_width: Some(config.usecase.wrapping_width),
        min_width: Some(config.usecase.min_node_width),
        ..PipelineNode::default()
    }
}

fn boundary_node(
    boundary: &super::ast::SystemBoundary,
    config: &Config,
    slot: usize,
) -> PipelineNode {
    let mut classes = vec![
        "default".to_string(),
        "system-boundary".to_string(),
        format!("system-boundary-{}", boundary.type_.as_str()),
    ];
    classes.extend(boundary.classes.iter().cloned());
    PipelineNode {
        id: boundary.id.clone(),
        label: Some(boundary.label.clone()),
        label_type: Some(boundary.label_type.as_str().to_string()),
        shape: Some("usecaseSystemBoundary".to_string()),
        is_group: Some(true),
        padding: Some(20.0),
        look: Some(config.look.clone()),
        color_index: Some(slot),
        css_classes: Some(classes.join(" ")),
        css_styles: Some(boundary.styles.clone()),
        boundary_type: Some(boundary.type_.as_str().to_string()),
        wrapping_width: Some(config.usecase.wrapping_width),
        ..PipelineNode::default()
    }
}

fn relationship_visuals(relationship: &Relationship) -> (Option<String>, String, Option<String>) {
    // (label, pattern, labelType)
    match relationship.type_ {
        RelationshipType::Include => (
            Some(format!("«{}»", relationship.type_.as_str())),
            "dotted".to_string(),
            Some(LabelType::Text.as_str().to_string()),
        ),
        RelationshipType::Extend => (
            Some(format!("«{}»", relationship.type_.as_str())),
            "dotted".to_string(),
            Some(LabelType::Text.as_str().to_string()),
        ),
        RelationshipType::Generalization => (None, "solid".to_string(), None),
        RelationshipType::Association => (
            relationship.label.clone(),
            "solid".to_string(),
            relationship
                .label_type
                .map(|label_type| label_type.as_str().to_string()),
        ),
    }
}

fn association_markers(arrow_type: ArrowType) -> (&'static str, &'static str) {
    match arrow_type {
        ArrowType::SolidArrow => ("none", "arrow_point"),
        ArrowType::BackArrow => ("arrow_point", "none"),
        ArrowType::CircleArrow => ("none", "arrow_circle"),
        ArrowType::CrossArrow => ("none", "arrow_cross"),
        ArrowType::CircleArrowReversed => ("arrow_circle", "none"),
        ArrowType::CrossArrowReversed => ("arrow_cross", "none"),
        ArrowType::LineSolid => ("none", "none"),
    }
}

fn relationship_edge(relationship: &Relationship, config: &Config) -> PipelineEdge {
    let (label, pattern, label_type) = relationship_visuals(relationship);
    let (arrow_type_start, arrow_type_end) = match relationship.type_ {
        RelationshipType::Include | RelationshipType::Extend => ("none", "arrow_point"),
        RelationshipType::Generalization => ("none", "extension"),
        RelationshipType::Association => association_markers(relationship.arrow_type),
    };
    let mut classes = vec![
        "default".to_string(),
        "relationship".to_string(),
        format!("relationship-{}", relationship.type_.as_str()),
    ];
    classes.extend(relationship.classes.iter().cloned());
    if relationship.animate || relationship.animation.is_some() {
        classes.push(format!(
            "edge-animation-{}",
            relationship
                .animation
                .map(|animation| animation.as_str())
                .unwrap_or("fast")
        ));
    }
    PipelineEdge {
        id: relationship.id.clone(),
        label,
        label_type,
        classes: Some(classes.join(" ")),
        style: Some(relationship.styles.clone()),
        animate: Some(relationship.animate),
        animation: relationship
            .animation
            .map(|animation| animation.as_str().to_string()),
        start: Some(relationship.source.clone()),
        end: Some(relationship.target.clone()),
        source: Some(relationship.source.clone()),
        target: Some(relationship.target.clone()),
        r#type: Some("edge".to_string()),
        labelpos: Some("c".to_string()),
        thickness: Some("normal".to_string()),
        pattern: Some(pattern),
        arrow_type_start: Some(arrow_type_start.to_string()),
        arrow_type_end: Some(arrow_type_end.to_string()),
        look: Some(config.look.clone()),
        minlen: Some(relationship.minlen as f64),
        is_user_defined_id: Some(relationship.explicit_id),
        css_compiled_styles: Some(Vec::new()),
        ..PipelineEdge::default()
    }
}

fn note_edge(note: &super::ast::UsecaseNote, config: &Config) -> PipelineEdge {
    PipelineEdge {
        id: format!("{}-edge", note.id),
        start: Some(note.id.clone()),
        end: Some(note.target.clone()),
        source: Some(note.id.clone()),
        target: Some(note.target.clone()),
        r#type: Some("edge".to_string()),
        pattern: Some("dotted".to_string()),
        arrow_type_start: Some("none".to_string()),
        arrow_type_end: Some("none".to_string()),
        labelpos: Some("c".to_string()),
        classes: Some("default relationship relationship-note".to_string()),
        style: Some(Vec::new()),
        animate: Some(false),
        look: Some(config.look.clone()),
        thickness: Some("normal".to_string()),
        minlen: Some(1.0),
        is_user_defined_id: Some(false),
        ..PipelineEdge::default()
    }
}

/// `usecaseNodeDomId`: the id a node's group carries, reduced to word characters and dashes.
fn node_dom_id(id: &str) -> String {
    let sanitized: String = id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = sanitized.trim_matches('_');
    if trimmed.is_empty() {
        "usecase-element".to_string()
    } else {
        format!("usecase-{trimmed}")
    }
}

/// `escapePlainLabel`.
fn escape_plain_label(label: &str) -> String {
    label
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// `escapeMarkdownMarkers`.
fn escape_markdown_markers(label: &str) -> String {
    let mut escaped = String::with_capacity(label.len());
    for character in label.chars() {
        if matches!(character, '*' | '[' | '\\' | ']' | '_' | '`') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

/// `prepareUsecaseLayoutData`: the escaping and stereotype folding `draw` performs before
/// the layout runs.
fn prepare_layout_data(data: &mut LayoutData, ast: &Ast) {
    for node in &mut data.nodes {
        node.dom_id = Some(node_dom_id(&node.id));
        if node.label_type.as_deref() == Some(LabelType::Text.as_str())
            && let Some(label) = &node.label
        {
            node.label = Some(escape_plain_label(label));
        }
        if let Some(stereotype) = &node.stereotype {
            node.stereotype = Some(escape_plain_label(stereotype));
        }
        if node.is_group != Some(true) && node.shape.as_deref() == Some("usecaseJsonTable") {
            for row in &mut node.json_rows {
                row.key = escape_plain_label(&row.key);
                row.value = escape_plain_label(&row.value);
            }
        }
        let fold = node.is_group != Some(true)
            && matches!(node.shape.as_deref(), Some("usecaseEllipse" | "rect"))
            && node.stereotype.is_some();
        if fold {
            let stereotype = node.stereotype.take().unwrap_or_default();
            let label = node
                .label
                .clone()
                .unwrap_or_else(|| escape_plain_label(&node.id));
            let label = if node.label_type.as_deref() == Some(LabelType::Text.as_str()) {
                escape_markdown_markers(&label)
            } else {
                label
            };
            node.label = Some(format!(
                "«{}»<br/>{}",
                escape_markdown_markers(&stereotype),
                label
            ));
            node.label_type = Some(LabelType::Markdown.as_str().to_string());
        }
    }
    for edge in &mut data.edges {
        if edge.label_type.as_deref() == Some(LabelType::Text.as_str())
            && let Some(label) = &edge.label
        {
            edge.label = Some(escape_plain_label(label));
        }
    }
    // `cssCompiledStyles` is built from the AST's class definitions, which the node
    // builders above cannot reach once the class list is flattened.
    for node in &mut data.nodes {
        let classes: Vec<String> = node
            .css_classes
            .as_deref()
            .unwrap_or_default()
            .split_whitespace()
            .map(|class_name| class_name.to_string())
            .collect();
        node.css_compiled_styles = Some(compiled_styles(ast, &classes));
    }
}

/// `flattenJsonRows`: walk the parsed object once into the table's key/value rows.
fn flatten_json_rows(value: &Value, property_order: &Map<String, Value>) -> Vec<JsonRowData> {
    let mut rows = Vec::new();
    visit_json(value, "", "", property_order, &mut rows);
    rows
}

fn property_order_keys(property_order: &Map<String, Value>, pointer: &str) -> Option<Vec<String>> {
    property_order.get(pointer).and_then(|value| {
        value.as_array().map(|keys| {
            keys.iter()
                .filter_map(|key| key.as_str().map(|key| key.to_string()))
                .collect()
        })
    })
}

fn visit_json(
    current: &Value,
    path: &str,
    pointer: &str,
    property_order: &Map<String, Value>,
    rows: &mut Vec<JsonRowData>,
) {
    if let Value::Array(items) = current {
        if items.is_empty() {
            push_row(rows, path, path, "[]");
            return;
        }
        let scalar_array = items
            .iter()
            .all(|item| !item.is_object() && !item.is_array());
        if scalar_array {
            for (index, item) in items.iter().enumerate() {
                push_row(
                    rows,
                    if index == 0 { path } else { "" },
                    path,
                    &display_json_scalar(item),
                );
            }
            return;
        }
        for (index, item) in items.iter().enumerate() {
            visit_json(
                item,
                &format!("{path}[{index}]"),
                &format!("{pointer}/{index}"),
                property_order,
                rows,
            );
        }
        return;
    }

    if let Value::Object(object) = current {
        let keys = property_order_keys(property_order, pointer)
            .unwrap_or_else(|| object.keys().cloned().collect());
        if keys.is_empty() {
            push_row(rows, path, path, "{}");
            return;
        }
        for key in keys {
            let child_path = if path.is_empty() {
                key.clone()
            } else {
                format!("{path}.{key}")
            };
            visit_json(
                object.get(&key).unwrap_or(&Value::Null),
                &child_path,
                &format!("{pointer}/{}", escape_json_pointer_part(&key)),
                property_order,
                rows,
            );
        }
        return;
    }

    push_row(rows, path, path, &display_json_scalar(current));
}

fn push_row(rows: &mut Vec<JsonRowData>, key: &str, accessible_key: &str, value: &str) {
    rows.push(JsonRowData {
        key: key.to_string(),
        accessible_key: accessible_key.to_string(),
        value: value.to_string(),
    });
}

fn escape_json_pointer_part(part: &str) -> String {
    part.replace('~', "~0").replace('/', "~1")
}

fn display_json_scalar(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => "null".to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::Array(_) | Value::Object(_) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagrams::usecase::parse;

    fn ast(source: &str) -> Ast {
        parse(source).expect("parse use case diagram")
    }

    #[test]
    fn maps_the_draw_settings_from_the_configuration() {
        let config = Config::default();
        let data = layout_data(
            &ast("usecase-beta\nactor User\nLogin\nUser --> Login\n"),
            &config,
            "d",
        );

        assert_eq!(data.r#type.as_deref(), Some(DIAGRAM_TYPE));
        assert_eq!(data.direction.as_deref(), Some("LR"));
        assert_eq!(
            data.layout_algorithm.as_deref(),
            Some(get_registered_layout_algorithm(&config.layout, None).as_str())
        );
        assert_eq!(data.markers, MARKERS);
        assert_eq!(data.diagram_id.as_deref(), Some("d"));
        assert_eq!(data.padding, Some(config.usecase.diagram_padding));
        assert_eq!(data.node_spacing, Some(config.usecase.node_spacing));
        assert_eq!(data.rank_spacing, Some(config.usecase.rank_spacing));
        assert_eq!(data.use_max_width, Some(config.usecase.use_max_width));
    }

    #[test]
    fn maps_actor_and_use_case_nodes() {
        let config = Config::default();
        let data = layout_data(
            &ast("usecase-beta\nactor User\nLogin\nUser --> Login\n"),
            &config,
            "d",
        );

        let actor = &data.nodes[0];
        assert_eq!(actor.id, "User");
        assert_eq!(actor.shape.as_deref(), Some("usecaseActor"));
        assert_eq!(actor.padding, Some(10.0));
        assert_eq!(actor.color_index, Some(0));
        assert_eq!(actor.label_font_size, Some(config.usecase.actor_font_size));
        assert!(actor.min_width.is_none());
        assert_eq!(actor.dom_id.as_deref(), Some("usecase-User"));
        assert_eq!(
            actor.css_classes.as_deref(),
            Some("default usecase-actor usecase-actor-normal")
        );

        let use_case = &data.nodes[1];
        assert_eq!(use_case.shape.as_deref(), Some("usecaseEllipse"));
        assert_eq!(use_case.padding, Some(20.0));
        assert_eq!(use_case.color_index, Some(1));
        assert_eq!(use_case.min_width, Some(config.usecase.min_node_width));
        assert_eq!(use_case.spread_ports, Some(true));
        assert_eq!(use_case.label_font_size, None);
    }

    #[test]
    fn folds_an_ellipse_stereotype_into_a_markdown_label() {
        let config = Config::default();
        let data = layout_data(&ast("usecase-beta\nLogin <<Primary>>\n"), &config, "d");
        let node = &data.nodes[0];
        assert_eq!(node.stereotype, None);
        assert_eq!(node.label.as_deref(), Some("«Primary»<br/>Login"));
        assert_eq!(node.label_type.as_deref(), Some("markdown"));
    }

    #[test]
    fn maps_relationship_edges_and_markers() {
        let config = Config::default();
        let data = layout_data(
            &ast("usecase-beta\nA --> B\nA ..> : include B\nA --|> B\n"),
            &config,
            "d",
        );

        let association = &data.edges[0];
        assert_eq!(association.arrow_type_end.as_deref(), Some("arrow_point"));
        assert_eq!(association.pattern.as_deref(), Some("solid"));

        let include = &data.edges[1];
        assert_eq!(include.label.as_deref(), Some("«include»"));
        assert_eq!(include.pattern.as_deref(), Some("dotted"));
        assert_eq!(include.arrow_type_end.as_deref(), Some("arrow_point"));

        let generalization = &data.edges[2];
        assert_eq!(generalization.arrow_type_end.as_deref(), Some("extension"));
        assert!(generalization.label.is_none());
    }

    #[test]
    fn flattens_json_tables() {
        let config = Config::default();
        let data = layout_data(
            &ast("usecase-beta\njson Payload@{\n  \"fruit\": \"Apple\",\n  \"items\": [1, 2]\n}\n"),
            &config,
            "d",
        );
        let rows = &data.nodes[0].json_rows;
        assert_eq!(
            rows.iter()
                .map(|row| (row.key.as_str(), row.value.as_str()))
                .collect::<Vec<_>>(),
            vec![("fruit", "Apple"), ("items", "1"), ("", "2")]
        );
        assert_eq!(rows[1].accessible_key, "items");
    }
}
