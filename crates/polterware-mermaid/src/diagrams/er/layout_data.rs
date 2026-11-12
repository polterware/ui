//! Conversion of the ER database projection into the shared layout pipeline's
//! [`LayoutData`], the Rust side of `erDb.ts` `getData()` (see [`super::ast::Ast::data`]).
//!
//! `getData` already hands over the entity and subgraph nodes and the relationship edges.
//! [`layout_data`] adds what `erRenderer-unified.ts` `draw` reads from the configuration
//! after `diag.db.getData()`: the diagram type, the resolved layout algorithm, the direction,
//! the ER node and rank spacing, the cardinality markers for the look, the diagram id, and
//! the viewport padding and `useMaxWidth`.
//!
//! Entity nodes carry `wrapping_width` from `flowchart.wrappingWidth`: upstream leaves the
//! field unset and `labelHelper` falls back to the same configuration value, which is what
//! wraps an alias too long for one line.

use crate::config::Config;
use crate::render::pipeline::{
    Edge as PipelineEdge, LayoutData, Node as PipelineNode, NodeAttribute,
    get_registered_layout_algorithm,
};

use super::ast::{Ast, Node as ErNode};

/// The diagram id `render()` prefixes DOM ids with when the caller names none.
pub const DEFAULT_DIAGRAM_ID: &str = "mermaid";

/// The diagram type upstream registers the unified ER renderer under.
pub const DIAGRAM_TYPE: &str = "er";

/// `erRenderer-unified.ts` `draw`'s markers for the default look.
pub const MARKERS: [&str; 4] = ["only_one", "zero_or_one", "one_or_more", "zero_or_more"];

/// `erRenderer-unified.ts` `draw`'s markers for the `neo` look.
pub const MARKERS_NEO: [&str; 4] = [
    "only_one_neo",
    "zero_or_one_neo",
    "one_or_more_neo",
    "zero_or_more_neo",
];

/// `erRenderer-unified.ts` `draw`'s `padding` passed to `setupViewPortForSVG`.
pub const VIEWPORT_PADDING: f64 = 8.0;

/// Builds the layout data for a parsed ER diagram.
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
            label: (!edge.label.is_empty()).then(|| edge.label.clone()),
            classes: Some(edge.classes.clone()),
            start: Some(edge.start.clone()),
            end: Some(edge.end.clone()),
            r#type: Some(edge.type_name.clone()),
            curve: Some(edge.curve.clone()),
            labelpos: Some(edge.labelpos.clone()),
            thickness: Some(edge.thickness.clone()),
            pattern: Some(edge.pattern.clone()),
            label_type: Some(edge.label_type.as_str().to_string()),
            arrow_type_start: Some(edge.arrow_type_start.as_arrow_type().to_string()),
            arrow_type_end: Some(edge.arrow_type_end.as_arrow_type().to_string()),
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
        direction: Some(data.direction),
        r#type: Some(DIAGRAM_TYPE.to_string()),
        markers: markers.iter().map(|marker| marker.to_string()).collect(),
        use_max_width: Some(config.er.use_max_width),
        padding: Some(VIEWPORT_PADDING),
        node_spacing: Some(config.er.node_spacing),
        rank_spacing: Some(config.er.rank_spacing),
        root_node: None,
        other: None,
    }
}

fn layout_node(node: &ErNode, config: &Config) -> PipelineNode {
    let is_group = node.is_group;
    PipelineNode {
        id: node.id.clone(),
        label: Some(node.label.clone()),
        label_style: (!node.label_style.is_empty()).then(|| node.label_style.clone()),
        label_type: Some(node.label_type.as_str().to_string()),
        parent_id: node.parent_id.clone(),
        css_styles: Some(node.css_styles.clone()),
        css_compiled_styles: Some(node.css_compiled_styles.clone()),
        css_classes: Some(node.css_classes.clone()),
        padding: is_group.then_some(node.padding),
        is_group: Some(is_group),
        shape: Some(node.shape.clone()),
        dir: node.dir.clone(),
        look: Some(config.look.clone()),
        color_index: node.color_index,
        attributes: node
            .attributes
            .iter()
            .map(|attribute| NodeAttribute {
                type_name: attribute.type_name.clone(),
                name: attribute.name.clone(),
                keys: attribute.keys.clone(),
                comment: attribute.comment.clone(),
            })
            .collect(),
        alias: (!node.alias.is_empty()).then(|| node.alias.clone()),
        wrapping_width: Some(config.flowchart.wrapping_width),
        ..PipelineNode::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagrams::er::parse;

    fn ast(source: &str) -> Ast {
        parse(source).expect("parse er diagram")
    }

    #[test]
    fn maps_draw_settings_from_the_configuration() {
        let config = Config::default();
        let data = layout_data(&ast("erDiagram\n  A ||--o{ B : has\n"), &config, "d");

        assert_eq!(data.direction.as_deref(), Some("TB"));
        assert_eq!(data.r#type.as_deref(), Some(DIAGRAM_TYPE));
        assert_eq!(data.markers, MARKERS);
        assert_eq!(data.diagram_id.as_deref(), Some("d"));
        assert_eq!(data.padding, Some(VIEWPORT_PADDING));
        assert_eq!(data.node_spacing, Some(config.er.node_spacing));
        assert_eq!(data.rank_spacing, Some(config.er.rank_spacing));
        assert_eq!(data.use_max_width, Some(config.er.use_max_width));
    }

    #[test]
    fn uses_the_neo_markers_for_the_neo_look() {
        let mut config = Config::default();
        config.look = "neo".to_string();
        let data = layout_data(&ast("erDiagram\n  A ||--o{ B : has\n"), &config, "d");
        assert_eq!(data.markers, MARKERS_NEO);
    }

    #[test]
    fn maps_entity_attributes_alias_and_color_index() {
        let config = Config::default();
        let data = layout_data(
            &ast("erDiagram\n  CUSTOMER[\"Buyer\"] {\n    string id PK \"key\"\n  }\n"),
            &config,
            "d",
        );
        let entity = data
            .nodes
            .iter()
            .find(|node| node.id == "entity-CUSTOMER-0")
            .expect("entity node");
        assert_eq!(entity.shape.as_deref(), Some("erBox"));
        assert_eq!(entity.label.as_deref(), Some("CUSTOMER"));
        assert_eq!(entity.alias.as_deref(), Some("Buyer"));
        assert_eq!(entity.color_index, Some(0));
        assert_eq!(entity.wrapping_width, Some(config.flowchart.wrapping_width));
        assert_eq!(entity.attributes.len(), 1);
        assert_eq!(entity.attributes[0].type_name, "string");
        assert_eq!(entity.attributes[0].keys, ["PK"]);
        assert_eq!(entity.attributes[0].comment, "key");
    }

    #[test]
    fn maps_subgraphs_as_groups_without_a_shape() {
        let config = Config::default();
        let data = layout_data(
            &ast("erDiagram\n  subgraph SALES\n    A\n  end\n"),
            &config,
            "d",
        );
        let group = data
            .nodes
            .iter()
            .find(|node| node.id == "SALES")
            .expect("group node");
        assert_eq!(group.is_group, Some(true));
        assert_eq!(group.shape.as_deref(), Some("rect"));
        assert_eq!(group.padding, Some(8.0));
    }

    #[test]
    fn maps_relationship_cardinalities_pattern_and_curve() {
        let config = Config::default();
        let data = layout_data(&ast("erDiagram\n  A |o..|| B : relates\n"), &config, "d");
        let edge = &data.edges[0];
        assert_eq!(edge.label.as_deref(), Some("relates"));
        assert_eq!(edge.arrow_type_start.as_deref(), Some("zero_or_one"));
        assert_eq!(edge.arrow_type_end.as_deref(), Some("only_one"));
        assert_eq!(edge.pattern.as_deref(), Some("dashed"));
        assert_eq!(edge.curve.as_deref(), Some("basis"));
        assert_eq!(edge.thickness.as_deref(), Some("normal"));
        assert_eq!(edge.classes.as_deref(), Some("relationshipLine"));
    }
}
