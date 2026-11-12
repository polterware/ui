//! Conversion of the agentflow database projection into the shared layout pipeline's
//! [`LayoutData`], the Rust side of `agentflowDb.ts` `getData()` (see [`Ast`]'s `nodes` and
//! `layout_edges`) and the fields `agentflow/renderer.ts` `draw` sets on top of it.
//!
//! `getData` hands over the container nodes (expanded ones as `flowGroup` groups, collapsed
//! ones as plain `collapsedGroup` nodes), the vertex nodes with their normalised shapes, and
//! the layout edges. [`layout_data`] fills in what `getData` reads from the configuration and
//! what `draw` adds afterwards: the diagram type, the resolved layout algorithm, the
//! direction, the `agentflow` block's node and rank spacing, the four markers `draw`
//! registers, the diagram id, and the viewport padding and `useMaxWidth`.
//!
//! The node padding and the label wrapping width are the ones `getData` reads: `padding`
//! comes from the `flowchart` block (upstream reads `config.flowchart?.padding || 8`, not
//! the `agentflow` one), and only a vertex node — not a container — carries the
//! `agentflow` block's `wrappingWidth` and `minNodeWidth`.
//!
//! Colour-slot assignment (`colorSlots.ts`) stays out, as the database module documents: this
//! crate paints the scene from the Polterware theme rather than from a Mermaid palette.

use crate::config::Config;
use crate::render::pipeline::{
    Edge as PipelineEdge, LayoutData, Node as PipelineNode, get_registered_layout_algorithm,
};

use super::ast::{Ast, Node as AgentflowNode};

/// The `flowchart.padding` fallback `getData`'s `config.flowchart?.padding || 8` applies.
pub const DEFAULT_NODE_PADDING: f64 = 8.0;

/// The diagram id `render()` prefixes DOM ids with when the caller names none.
pub const DEFAULT_DIAGRAM_ID: &str = "mermaid";

/// The diagram type upstream registers the agentflow renderer under.
pub const DIAGRAM_TYPE: &str = "agentflow";

/// `agentflow/renderer.ts` `draw`'s `data4Layout.markers`.
pub const MARKERS: [&str; 4] = ["point", "circle", "cross", "hierarchy"];

/// Builds the layout data for a parsed agentflow diagram.
pub fn layout_data(ast: &Ast, config: &Config, diagram_id: &str) -> LayoutData {
    let agentflow = &config.agentflow;
    let nodes = ast
        .nodes
        .iter()
        .map(|node| layout_node(node, config))
        .collect();
    let edges = ast
        .layout_edges
        .iter()
        .map(|edge| {
            let label = (!edge.label.is_empty()).then(|| edge.label.clone());
            PipelineEdge {
                id: edge.id.clone(),
                label,
                classes: Some(edge.classes.clone()),
                style: Some(edge.style.clone()),
                animate: edge.animate,
                animation: edge.animation.clone(),
                arrowhead_style: Some(edge.arrowhead_style.clone()),
                arrow_type_end: Some(edge.arrow_type_end.as_str().to_string()),
                arrow_type_start: Some(edge.arrow_type_start.as_str().to_string()),
                css_compiled_styles: Some(edge.css_compiled_styles.clone()),
                start: Some(edge.start.clone()),
                end: Some(edge.end.clone()),
                r#type: Some(edge.type_name.clone()),
                label_type: Some(edge.label_type.as_str().to_string()),
                labelpos: Some(edge.labelpos.clone()),
                thickness: edge.thickness.map(|stroke| stroke.as_str().to_string()),
                minlen: edge.minlen.map(|length| length as f64),
                label_style: Some(edge.label_style.clone()),
                pattern: edge.pattern.map(|stroke| stroke.as_str().to_string()),
                look: Some(config.look.clone()),
                is_user_defined_id: Some(edge.is_user_defined_id),
                // `getData` ends the chain on `config.flowchart?.curve`; the `agentflow`
                // block has no curve of its own.
                curve: Some(
                    edge.curve
                        .clone()
                        .unwrap_or_else(|| config.flowchart.curve.clone()),
                ),
                metadata: edge
                    .metadata
                    .as_ref()
                    .map(|metadata| serde_json::Value::Object(metadata.clone())),
                ..PipelineEdge::default()
            }
        })
        .collect();

    LayoutData {
        nodes,
        edges,
        config: config.clone(),
        diagram_id: Some(diagram_id.to_string()),
        layout_algorithm: Some(get_registered_layout_algorithm(&config.layout, None)),
        direction: ast.direction.clone(),
        r#type: Some(DIAGRAM_TYPE.to_string()),
        markers: MARKERS.iter().map(|marker| marker.to_string()).collect(),
        use_max_width: Some(agentflow.use_max_width),
        padding: Some(agentflow.diagram_padding),
        node_spacing: Some(agentflow.node_spacing),
        rank_spacing: Some(agentflow.rank_spacing),
        root_node: None,
        other: None,
    }
}

fn layout_node(node: &AgentflowNode, config: &Config) -> PipelineNode {
    let agentflow = &config.agentflow;
    let is_group = node.is_group;
    // A collapsed container is drawn as a plain node and carries `getData`'s literal
    // padding of 8, like an expanded one; only a vertex node takes the flowchart padding.
    let collapsed = node.shape == "collapsedGroup";
    PipelineNode {
        id: node.id.clone(),
        label: Some(node.label.clone()),
        css_styles: Some(node.css_styles.clone()),
        css_compiled_styles: Some(node.css_compiled_styles.clone()),
        css_classes: Some(node.css_classes.clone()),
        label_type: Some(node.label_type.as_str().to_string()),
        dom_id: node.dom_id.clone(),
        parent_id: node.parent_id.clone(),
        dir: node.dir.clone(),
        padding: Some(if is_group || collapsed {
            node.padding
        } else if config.flowchart.padding == 0.0 {
            // `getData`'s `config.flowchart?.padding || 8` treats a zero as absent.
            DEFAULT_NODE_PADDING
        } else {
            config.flowchart.padding
        }),
        is_group: Some(is_group),
        shape: Some(node.shape.clone()),
        look: Some(config.look.clone()),
        link: node.link.clone(),
        link_target: node.link_target.clone(),
        min_width: if is_group || collapsed {
            None
        } else {
            Some(agentflow.min_node_width)
        },
        wrapping_width: if is_group || collapsed {
            None
        } else {
            Some(agentflow.wrapping_width)
        },
        metadata: node
            .metadata
            .as_ref()
            .map(|metadata| serde_json::Value::Object(metadata.clone())),
        ..PipelineNode::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagrams::agentflow::parse;

    fn ast(source: &str) -> Ast {
        parse(source).expect("parse agentflow diagram")
    }

    #[test]
    fn maps_draw_settings_from_the_agentflow_block() {
        let config = Config::default();
        let data = layout_data(&ast("agentflow-beta TB\n  a --> b\n"), &config, "d");

        assert_eq!(data.direction.as_deref(), Some("TB"));
        assert_eq!(data.r#type.as_deref(), Some(DIAGRAM_TYPE));
        assert_eq!(data.markers, ["point", "circle", "cross", "hierarchy"]);
        assert_eq!(data.diagram_id.as_deref(), Some("d"));
        assert_eq!(data.padding, Some(config.agentflow.diagram_padding));
        assert_eq!(data.node_spacing, Some(config.agentflow.node_spacing));
        assert_eq!(data.rank_spacing, Some(config.agentflow.rank_spacing));
        assert_eq!(data.use_max_width, Some(config.agentflow.use_max_width));
    }

    #[test]
    fn uses_the_flowchart_padding_for_vertex_nodes() {
        let config = Config::default();
        let data = layout_data(&ast("agentflow-beta TB\n  a[Alpha]\n"), &config, "d");
        let node = &data.nodes[0];
        assert_eq!(node.shape.as_deref(), Some("roundedRect"));
        assert_eq!(node.padding, Some(config.flowchart.padding));
        assert_eq!(node.min_width, Some(config.agentflow.min_node_width));
        assert_eq!(node.wrapping_width, Some(config.agentflow.wrapping_width));
        assert_eq!(node.look.as_deref(), Some(config.look.as_str()));
    }

    #[test]
    fn maps_a_flow_container_as_a_group_without_label_constraints() {
        let config = Config::default();
        let data = layout_data(
            &ast("agentflow-beta TB\n  flow one[One]\n    a\n  end\n"),
            &config,
            "d",
        );
        let group = data
            .nodes
            .iter()
            .find(|node| node.id == "one")
            .expect("group node");
        assert_eq!(group.is_group, Some(true));
        assert_eq!(group.shape.as_deref(), Some("flowGroup"));
        assert_eq!(group.padding, Some(8.0));
        assert_eq!(group.min_width, None);
        assert_eq!(group.wrapping_width, None);
    }

    #[test]
    fn maps_a_collapsed_container_as_a_plain_node() {
        let config = Config::default();
        let data = layout_data(
            &ast(
                "agentflow-beta TB\n  flow one[One]\n    a\n  end\n  one@{ view: \"collapsed\" }\n",
            ),
            &config,
            "d",
        );
        let collapsed = data
            .nodes
            .iter()
            .find(|node| node.id == "one")
            .expect("collapsed node");
        assert_eq!(collapsed.is_group, Some(false));
        assert_eq!(collapsed.shape.as_deref(), Some("collapsedGroup"));
        assert_eq!(
            collapsed
                .metadata
                .as_ref()
                .and_then(|m| m["containerType"].as_str()),
            Some("flow")
        );
    }

    #[test]
    fn maps_edge_semantics_and_the_flowchart_curve() {
        let config = Config::default();
        let data = layout_data(
            &ast("agentflow-beta TB\n  a --> b\n  b -.- a\n  b --x a\n"),
            &config,
            "d",
        );
        assert_eq!(data.edges.len(), 3);
        let sequence = &data.edges[0];
        assert_eq!(sequence.arrow_type_start.as_deref(), Some("none"));
        assert_eq!(sequence.arrow_type_end.as_deref(), Some("arrow_point"));
        assert_eq!(
            sequence.curve.as_deref(),
            Some(config.flowchart.curve.as_str())
        );
        assert_eq!(data.edges[1].thickness.as_deref(), Some("dotted"));
        assert_eq!(data.edges[2].arrow_type_end.as_deref(), Some("arrow_cross"));
    }
}
