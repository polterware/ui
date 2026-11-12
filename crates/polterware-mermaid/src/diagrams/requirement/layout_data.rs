//! Conversion of the requirement database projection into the shared layout pipeline's
//! [`LayoutData`], the Rust side of `requirementDb.ts` `getData()` (see [`Ast::data`]) and
//! the fields `requirementRenderer.ts` `draw` sets on top of it.
//!
//! `getData` hands over the requirement and element nodes and the relationship edges.
//! [`layout_data`] adds what `draw` reads from the configuration: the diagram type, the
//! resolved layout algorithm, the direction, the `state` node and rank spacing, the look's
//! requirement markers, the diagram id, and the viewport padding and `useMaxWidth`.
//!
//! The relationship label `getData` returns is HTML-escaped (`&lt;&lt;contains&gt;&gt;`);
//! `createText` tokenizes that escaped string and decodes each word only when it writes the
//! tspan, so the layout edge keeps the escaped label that decides the box and the layout.

use crate::config::Config;
use crate::render::pipeline::{
    Edge as PipelineEdge, ElementNodeData, LayoutData, Node as PipelineNode, RequirementNodeData,
    get_registered_layout_algorithm,
};

use super::ast::{Ast, Node as RequirementNode};

/// The diagram id `render()` prefixes DOM ids with when the caller names none.
pub const DEFAULT_DIAGRAM_ID: &str = "mermaid";

/// The diagram type upstream registers the unified requirement renderer under.
pub const DIAGRAM_TYPE: &str = "requirement";

/// `requirementRenderer.ts` `draw`'s markers for the default look.
pub const MARKERS: [&str; 2] = ["requirement_contains", "requirement_arrow"];

/// `requirementRenderer.ts` `draw`'s markers for the `neo` look.
pub const MARKERS_NEO: [&str; 2] = ["requirement_contains_neo", "requirement_arrow_neo"];

/// `requirementRenderer.ts` `draw`'s `padding` passed to `setupViewPortForSVG`.
pub const VIEWPORT_PADDING: f64 = 8.0;

/// `draw`'s `conf?.nodeSpacing ?? 50`.
pub const DEFAULT_NODE_SPACING: f64 = 50.0;

/// `draw`'s `conf?.rankSpacing ?? 50`.
pub const DEFAULT_RANK_SPACING: f64 = 50.0;

/// Builds the layout data for a parsed requirement diagram.
pub fn layout_data(ast: &Ast, config: &Config, diagram_id: &str) -> LayoutData {
    let data = ast.data();
    let markers = if config.look == "neo" {
        MARKERS_NEO
    } else {
        MARKERS
    };
    let nodes = data
        .nodes
        .iter()
        .map(|node| layout_node(node, config))
        .collect();
    let edges = data
        .edges
        .iter()
        .map(|edge| PipelineEdge {
            id: edge.id.clone(),
            // `createText` tokenizes the escaped label and decodes each word only when it
            // writes the tspan, so `&lt;&lt;contains&gt;&gt;` measures as one word.
            label: Some(edge.label.clone()),
            classes: Some(edge.classes.clone()),
            start: edge.start.clone(),
            end: edge.end.clone(),
            r#type: Some(edge.type_name.clone()),
            labelpos: Some(edge.labelpos.clone()),
            thickness: Some(edge.thickness.clone()),
            pattern: Some(edge.pattern.clone()),
            style: Some(edge.style.clone()),
            label_type: Some("markdown".to_string()),
            arrow_type_start: Some(edge.arrow_type_start.clone()),
            arrow_type_end: Some(edge.arrow_type_end.clone()),
            look: Some(config.look.clone()),
            ..PipelineEdge::default()
        })
        .collect();

    LayoutData {
        nodes,
        edges,
        config: config.clone(),
        diagram_id: Some(diagram_id.to_string()),
        layout_algorithm: Some(get_registered_layout_algorithm(&config.layout, None)),
        direction: Some(data.direction.as_str().to_string()),
        r#type: Some(DIAGRAM_TYPE.to_string()),
        markers: markers.iter().map(|marker| marker.to_string()).collect(),
        // `draw` reads `conf` from the `state` configuration section.
        use_max_width: Some(config.state.use_max_width),
        padding: Some(VIEWPORT_PADDING),
        node_spacing: Some(config.state.node_spacing.unwrap_or(DEFAULT_NODE_SPACING)),
        rank_spacing: Some(config.state.rank_spacing.unwrap_or(DEFAULT_RANK_SPACING)),
        root_node: None,
        other: None,
    }
}

fn layout_node(node: &RequirementNode, config: &Config) -> PipelineNode {
    PipelineNode {
        id: node.id.clone(),
        label: Some(node.label.clone()),
        requirement: node
            .requirement
            .as_ref()
            .map(|requirement| RequirementNodeData {
                type_name: requirement.type_name.clone(),
                requirement_id: requirement.requirement_id.clone(),
                text: requirement.text.clone(),
                risk: requirement
                    .risk
                    .map(|risk| risk.as_str().to_string())
                    .unwrap_or_default(),
                verify_method: requirement
                    .verify_method
                    .map(|verify_method| verify_method.as_str().to_string())
                    .unwrap_or_default(),
            }),
        element: node.element.as_ref().map(|element| ElementNodeData {
            type_name: element.type_name.clone(),
            doc_ref: element.doc_ref.clone(),
        }),
        css_styles: Some(node.css_styles.clone()),
        css_classes: Some(node.css_classes.clone()),
        shape: Some("requirementBox".to_string()),
        look: Some(config.look.clone()),
        color_index: Some(node.color_index),
        ..PipelineNode::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagrams::requirement::parse;

    fn ast(source: &str) -> Ast {
        parse(source).expect("parse requirement diagram")
    }

    #[test]
    fn maps_draw_settings_from_the_configuration() {
        let config = Config::default();
        let data = layout_data(
            &ast("requirementDiagram\n  requirement r {\n  }\n"),
            &config,
            "d",
        );

        assert_eq!(data.direction.as_deref(), Some("TB"));
        assert_eq!(data.r#type.as_deref(), Some(DIAGRAM_TYPE));
        assert_eq!(data.markers, MARKERS);
        assert_eq!(data.diagram_id.as_deref(), Some("d"));
        assert_eq!(data.padding, Some(VIEWPORT_PADDING));
        assert_eq!(data.node_spacing, Some(DEFAULT_NODE_SPACING));
        assert_eq!(data.rank_spacing, Some(DEFAULT_RANK_SPACING));
        assert_eq!(data.use_max_width, Some(config.state.use_max_width));
    }

    #[test]
    fn uses_the_neo_markers_for_the_neo_look() {
        let mut config = Config::default();
        config.look = "neo".to_string();
        let data = layout_data(
            &ast("requirementDiagram\n  requirement r {\n  }\n"),
            &config,
            "d",
        );
        assert_eq!(data.markers, MARKERS_NEO);
    }

    #[test]
    fn maps_requirement_fields_and_color_index() {
        let config = Config::default();
        let data = layout_data(
            &ast(
                "requirementDiagram\n  requirement r {\n    id: x\n    text: the text\n    risk: high\n    verifymethod: test\n  }\n",
            ),
            &config,
            "d",
        );
        let node = &data.nodes[0];
        assert_eq!(node.id, "r");
        assert_eq!(node.label.as_deref(), Some("r"));
        assert_eq!(node.shape.as_deref(), Some("requirementBox"));
        assert_eq!(node.color_index, Some(0));
        assert_eq!(node.css_classes.as_deref(), Some("default"));
        let requirement = node.requirement.as_ref().expect("requirement payload");
        assert_eq!(requirement.type_name, "Requirement");
        assert_eq!(requirement.requirement_id, "x");
        assert_eq!(requirement.text, "the text");
        assert_eq!(requirement.risk, "High");
        assert_eq!(requirement.verify_method, "Test");
        assert!(node.element.is_none());
    }

    #[test]
    fn maps_element_fields() {
        let config = Config::default();
        let data = layout_data(
            &ast("requirementDiagram\n  element e {\n    type: simulation\n    docref: r1\n  }\n"),
            &config,
            "d",
        );
        let node = &data.nodes[0];
        assert_eq!(node.color_index, Some(0));
        let element = node.element.as_ref().expect("element payload");
        assert_eq!(element.type_name, "simulation");
        assert_eq!(element.doc_ref, "r1");
        assert!(node.requirement.is_none());
    }

    #[test]
    fn decodes_relationship_labels_and_maps_markers() {
        let config = Config::default();
        let data = layout_data(
            &ast(
                "requirementDiagram\n  requirement a {\n  }\n  requirement b {\n  }\n  a - contains -> b\n  a - traces -> b\n",
            ),
            &config,
            "d",
        );
        let contains = &data.edges[0];
        assert_eq!(contains.label.as_deref(), Some("&lt;&lt;contains&gt;&gt;"));
        assert_eq!(contains.pattern.as_deref(), Some("normal"));
        assert_eq!(
            contains.arrow_type_start.as_deref(),
            Some("requirement_contains")
        );
        assert_eq!(contains.arrow_type_end.as_deref(), Some(""));

        let traces = &data.edges[1];
        assert_eq!(traces.label.as_deref(), Some("&lt;&lt;traces&gt;&gt;"));
        assert_eq!(traces.pattern.as_deref(), Some("dashed"));
        assert_eq!(traces.arrow_type_start.as_deref(), Some(""));
        assert_eq!(traces.arrow_type_end.as_deref(), Some("requirement_arrow"));
    }

    #[test]
    fn keeps_relationships_to_missing_nodes_without_endpoints() {
        let config = Config::default();
        let data = layout_data(
            &ast("requirementDiagram\n  a - copies -> b\n"),
            &config,
            "d",
        );
        assert!(data.edges[0].start.is_none());
        assert!(data.edges[0].end.is_none());
    }
}
