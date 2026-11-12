//! Conversion of the class database projection into the shared layout pipeline's
//! [`LayoutData`], the Rust side of `classDb.ts` `getData()` and the fields
//! `classRenderer-v3-unified.ts` `draw` sets on top of it.
//!
//! `getData()` turns namespaces into groups, classes into `classBox` nodes, notes into `note`
//! nodes, lollipop relations into hidden interface nodes, and every relation and note into an
//! edge. [`layout_data`] does the same from the parsed [`Ast`], then fills the fields `draw`
//! reads from the configuration: the diagram type, the resolved layout algorithm, the markers,
//! the diagram id, and the spacing, padding and `useMaxWidth` the viewport setup uses.

use crate::config::Config;
use crate::render::pipeline::{
    ClassMemberData, Edge as PipelineEdge, LayoutData, Node as PipelineNode,
    get_registered_layout_algorithm,
};

use super::ast::{Ast, ClassMember, ClassNode, ClassNote, ClassRelation, Endpoint, NamespaceNode};

/// The diagram id `render()` prefixes DOM ids with when the caller names none.
pub const DEFAULT_DIAGRAM_ID: &str = "mermaid";

/// The diagram type upstream registers the unified class renderer under.
pub const DIAGRAM_TYPE: &str = "classDiagram";

/// `classRenderer-v3-unified.ts` `draw`'s `data4Layout.markers`.
pub const MARKERS: [&str; 5] = [
    "aggregation",
    "extension",
    "composition",
    "dependency",
    "lollipop",
];

/// Builds the layout data for a parsed class diagram.
///
/// This is `getData()` plus the fields `draw` sets from the configuration.
pub fn layout_data(ast: &Ast, config: &Config, diagram_id: &str) -> LayoutData {
    let hierarchical = config.class.hierarchical_namespaces;
    let padding = config.class.padding;

    let mut nodes: Vec<PipelineNode> = Vec::new();
    for namespace in ast.namespaces.values() {
        if !hierarchical && !namespace.explicit {
            continue;
        }
        nodes.push(namespace_node(namespace, hierarchical, padding, config));
    }

    // Only classes consume a colour slot -- namespaces are containers and notes have their
    // own fixed note colour, so neither should shift the cycle.
    for (class_color_index, class_node) in ast.classes.values().enumerate() {
        let parent_id = if hierarchical {
            class_node.parent.clone()
        } else {
            resolve_explicit_ancestor(ast, class_node.parent.as_deref())
        };
        nodes.push(class_node_data(
            class_node,
            parent_id,
            class_color_index,
            padding,
            config,
        ));
    }

    let mut edges: Vec<PipelineEdge> = Vec::new();
    for note in ast.notes.values() {
        let note_parent_id = if hierarchical {
            note.parent.clone()
        } else {
            resolve_explicit_ancestor(ast, note.parent.as_deref())
        };
        nodes.push(note_node(note, note_parent_id, padding, config));

        let note_class_id = note
            .class
            .as_deref()
            .and_then(|class| ast.classes.get(class))
            .map(|class| class.id.clone());
        if let Some(note_class_id) = note_class_id {
            edges.push(PipelineEdge {
                id: format!("edgeNote{}", note.index),
                start: Some(note.id.clone()),
                end: Some(note_class_id),
                r#type: Some("normal".to_string()),
                thickness: Some("normal".to_string()),
                classes: Some("relation".to_string()),
                arrow_type_start: Some("none".to_string()),
                arrow_type_end: Some("none".to_string()),
                arrowhead_style: Some(String::new()),
                label_style: Some(vec![String::new()]),
                style: Some(vec!["fill: none".to_string()]),
                pattern: Some("dotted".to_string()),
                look: Some(config.look.clone()),
                ..PipelineEdge::default()
            });
        }
    }

    for interface in &ast.interfaces {
        nodes.push(PipelineNode {
            id: interface.id.clone(),
            label: Some(interface.label.clone()),
            is_group: Some(false),
            shape: Some("rect".to_string()),
            css_styles: Some(vec!["opacity: 0;".to_string()]),
            look: Some(config.look.clone()),
            ..PipelineNode::default()
        });
    }

    let mut counter = 0usize;
    for relation in &ast.relations {
        counter += 1;
        edges.push(relation_edge(relation, counter, config));
    }

    LayoutData {
        nodes,
        edges,
        config: config.clone(),
        diagram_id: Some(diagram_id.to_string()),
        layout_algorithm: Some(get_registered_layout_algorithm(&config.layout, None)),
        direction: Some(ast.direction.as_str().to_string()),
        r#type: Some(DIAGRAM_TYPE.to_string()),
        markers: MARKERS.iter().map(|marker| marker.to_string()).collect(),
        // `draw` passes `conf?.useMaxWidth ?? true` with `conf` being the state section.
        use_max_width: Some(config.state.use_max_width),
        padding: Some(8.0),
        // `draw` passes `conf?.nodeSpacing || 50` and `conf?.rankSpacing || 50` with `conf`
        // being the state section, so a configured zero falls back to the default.
        node_spacing: Some(
            config
                .state
                .node_spacing
                .filter(|spacing| *spacing != 0.0)
                .unwrap_or(50.0),
        ),
        rank_spacing: Some(
            config
                .state
                .rank_spacing
                .filter(|spacing| *spacing != 0.0)
                .unwrap_or(50.0),
        ),
        root_node: None,
    }
}

fn namespace_node(
    namespace: &NamespaceNode,
    hierarchical: bool,
    padding: Option<f64>,
    config: &Config,
) -> PipelineNode {
    PipelineNode {
        id: namespace.id.clone(),
        label: Some(if hierarchical {
            namespace.label.clone()
        } else {
            namespace.id.clone()
        }),
        is_group: Some(true),
        padding: Some(padding.unwrap_or(16.0)),
        shape: Some("rect".to_string()),
        css_styles: Some(Vec::new()),
        look: Some(config.look.clone()),
        parent_id: if hierarchical {
            namespace.parent.clone()
        } else {
            None
        },
        ..PipelineNode::default()
    }
}

fn class_node_data(
    class_node: &ClassNode,
    parent_id: Option<String>,
    color_index: usize,
    padding: Option<f64>,
    config: &Config,
) -> PipelineNode {
    PipelineNode {
        id: class_node.id.clone(),
        label: Some(class_node.label.clone()),
        text: Some(class_node.text.clone()),
        shape: Some("classBox".to_string()),
        css_classes: Some(class_node.css_classes.clone()),
        members: class_node.members.iter().map(member_data).collect(),
        methods: class_node.methods.iter().map(member_data).collect(),
        annotations: class_node.annotations.clone(),
        dom_id: Some(class_node.dom_id.clone()),
        styles: class_node.styles.clone(),
        class_padding: padding,
        hide_empty_members_box: Some(config.class.hide_empty_members_box),
        is_group: Some(false),
        parent_id,
        look: Some(config.look.clone()),
        color_index: Some(color_index),
        link: class_node.link.clone(),
        link_target: class_node.link_target.clone(),
        have_callback: Some(class_node.have_callback),
        tooltip: class_node.tooltip.clone(),
        ..PipelineNode::default()
    }
}

fn member_data(member: &ClassMember) -> ClassMemberData {
    ClassMemberData {
        text: member.text.clone(),
        css_style: member.css_style().to_string(),
    }
}

fn note_node(
    note: &ClassNote,
    parent_id: Option<String>,
    padding: Option<f64>,
    config: &Config,
) -> PipelineNode {
    let theme = &config.theme_variables;
    let note_background = theme
        .get("noteBkgColor")
        .and_then(|value| value.as_str())
        .unwrap_or("#fff5ad");
    let note_border = theme
        .get("noteBorderColor")
        .and_then(|value| value.as_str())
        .unwrap_or("#9370DB");
    PipelineNode {
        id: note.id.clone(),
        label: Some(note.text.clone()),
        is_group: Some(false),
        shape: Some("note".to_string()),
        padding: Some(padding.unwrap_or(6.0)),
        css_styles: Some(vec![
            "text-align: left".to_string(),
            "white-space: nowrap".to_string(),
            format!("fill: {note_background}"),
            format!("stroke: {note_border}"),
        ]),
        look: Some(config.look.clone()),
        parent_id,
        label_type: Some("markdown".to_string()),
        // `labelHelper` falls back to `flowchart.wrappingWidth`; the shape context has no
        // configuration, so the layout data carries the same width on the node.
        wrapping_width: Some(config.flowchart.wrapping_width),
        ..PipelineNode::default()
    }
}

fn relation_edge(relation: &ClassRelation, counter: usize, config: &Config) -> PipelineEdge {
    PipelineEdge {
        id: format!("id_{}_{}_{counter}", relation.id1, relation.id2),
        start: Some(relation.id1.clone()),
        end: Some(relation.id2.clone()),
        r#type: Some("normal".to_string()),
        label: relation.title.clone(),
        labelpos: Some("c".to_string()),
        thickness: Some("normal".to_string()),
        classes: Some("relation".to_string()),
        arrow_type_start: Some(arrow_marker(relation.relation.type1).to_string()),
        arrow_type_end: Some(arrow_marker(relation.relation.type2).to_string()),
        start_label_right: Some(if relation.relation_title1 == "none" {
            String::new()
        } else {
            relation.relation_title1.clone()
        }),
        end_label_left: Some(if relation.relation_title2 == "none" {
            String::new()
        } else {
            relation.relation_title2.clone()
        }),
        arrowhead_style: Some(String::new()),
        label_style: Some(vec!["display: inline-block".to_string()]),
        style: Some(relation.style.clone()),
        pattern: Some(
            if relation.relation.line_type == super::ast::LineType::DottedLine {
                "dashed"
            } else {
                "solid"
            }
            .to_string(),
        ),
        look: Some(config.look.clone()),
        label_type: Some("markdown".to_string()),
        ..PipelineEdge::default()
    }
}

/// `getArrowMarker`: the marker name for a relation endpoint.
fn arrow_marker(endpoint: Endpoint) -> &'static str {
    match endpoint {
        Endpoint::None => "none",
        Endpoint::Aggregation => "aggregation",
        Endpoint::Extension => "extension",
        Endpoint::Composition => "composition",
        Endpoint::Dependency => "dependency",
        Endpoint::Lollipop => "lollipop",
    }
}

/// `resolveExplicitAncestor`: the nearest declared namespace, used by compact rendering.
fn resolve_explicit_ancestor(ast: &Ast, id: Option<&str>) -> Option<String> {
    let mut current = id;
    while let Some(namespace_id) = current {
        let namespace = ast.namespaces.get(namespace_id)?;
        if namespace.explicit {
            return Some(namespace_id.to_string());
        }
        current = namespace.parent.as_deref();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagrams::class::parse;

    fn ast(source: &str) -> Ast {
        parse(source).expect("parse class diagram")
    }

    #[test]
    fn maps_draw_settings_from_the_configuration() {
        let config = Config::default();
        let data = layout_data(&ast("classDiagram\n  A <|-- B\n"), &config, "d");

        assert_eq!(data.direction.as_deref(), Some("TB"));
        assert_eq!(data.r#type.as_deref(), Some(DIAGRAM_TYPE));
        assert_eq!(
            data.markers,
            [
                "aggregation",
                "extension",
                "composition",
                "dependency",
                "lollipop"
            ]
        );
        assert_eq!(data.diagram_id.as_deref(), Some("d"));
        assert_eq!(data.padding, Some(8.0));
        assert_eq!(data.node_spacing, Some(50.0));
        assert_eq!(data.rank_spacing, Some(50.0));
        assert_eq!(data.use_max_width, Some(config.state.use_max_width));
    }

    #[test]
    fn maps_classes_notes_and_namespaces() {
        let config = Config::default();
        let data = layout_data(
            &ast(
                "classDiagram\n  namespace Zoo {\n    class Animal\n  }\n  note for Animal \"hi\"\n",
            ),
            &config,
            "d",
        );
        let animal = data
            .nodes
            .iter()
            .find(|node| node.id == "Animal")
            .expect("class node");
        assert_eq!(animal.shape.as_deref(), Some("classBox"));
        assert_eq!(animal.text.as_deref(), Some("Animal"));
        assert_eq!(animal.color_index, Some(0));
        assert_eq!(animal.parent_id.as_deref(), Some("Zoo"));

        let namespace = data
            .nodes
            .iter()
            .find(|node| node.id == "Zoo")
            .expect("namespace node");
        assert_eq!(namespace.is_group, Some(true));
        assert_eq!(namespace.shape.as_deref(), Some("rect"));
        assert_eq!(namespace.padding, Some(16.0));

        let note = data
            .nodes
            .iter()
            .find(|node| node.id == "note0")
            .expect("note node");
        assert_eq!(note.shape.as_deref(), Some("note"));
        assert_eq!(note.padding, Some(6.0));
        assert_eq!(note.label_type.as_deref(), Some("markdown"));
        assert!(data.edges.iter().any(|edge| edge.id == "edgeNote0"));
    }

    #[test]
    fn maps_relation_markers_and_labels() {
        let config = Config::default();
        let data = layout_data(
            &ast("classDiagram\n  A \"1\" <|-- \"many\" B : owns\n  C ..> D\n"),
            &config,
            "d",
        );
        let first = &data.edges[0];
        assert_eq!(first.id, "id_A_B_1");
        assert_eq!(first.arrow_type_start.as_deref(), Some("extension"));
        assert_eq!(first.arrow_type_end.as_deref(), Some("none"));
        assert_eq!(first.start_label_right.as_deref(), Some("1"));
        assert_eq!(first.end_label_left.as_deref(), Some("many"));
        assert_eq!(first.label.as_deref(), Some("owns"));

        let dependency = &data.edges[1];
        assert_eq!(dependency.pattern.as_deref(), Some("dashed"));
        assert_eq!(dependency.arrow_type_end.as_deref(), Some("dependency"));
    }
}
