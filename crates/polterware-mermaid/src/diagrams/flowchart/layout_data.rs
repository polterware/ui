//! Conversion of the flowchart database projection into the shared layout pipeline's
//! [`LayoutData`], the Rust side of `flowDb.ts` `getData()` (see [`super::db::Db::get_data`]).
//!
//! The database already hands over the nodes and edges `getData` derives, including the
//! `getData`-specific shape names, compiled class styles and arrow ends. What it cannot know
//! without the diagram's configuration is the padding, minimum node width, wrapping width,
//! look, curve and spacing `getData` reads from `getConfig()`. [`layout_data`] fills those in
//! and assembles the data `render()` consumes, as `flowRenderer-v3-unified.ts` `draw` does
//! after `diag.db.getData()`.

use crate::config::Config;
use crate::render::pipeline::{
    Edge as PipelineEdge, LayoutData, Node as PipelineNode, get_registered_layout_algorithm,
};

use super::ast::{Ast, Node as FlowNode};

/// The diagram id `render()` prefixes DOM ids with when the caller names none.
pub const DEFAULT_DIAGRAM_ID: &str = "mermaid";

/// The diagram type upstream registers the unified flowchart renderer under.
pub const DIAGRAM_TYPE: &str = "flowchart-v2";

/// `flowRenderer-v3-unified.ts` `draw`'s `data4Layout.markers`.
pub const MARKERS: [&str; 3] = ["point", "circle", "cross"];

/// Builds the layout data for a parsed flowchart.
///
/// This is `getData()` plus the fields `draw` sets from the configuration: the diagram type,
/// the resolved layout algorithm, the direction, node and rank spacing, markers, diagram id,
/// and the padding and `useMaxWidth` the viewport setup reads from the layout data.
pub fn layout_data(ast: &Ast, config: &Config, diagram_id: &str) -> LayoutData {
    let flowchart = &config.flowchart;
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
                step: edge.step.map(f64::from),
                step_count: edge.step_count.map(f64::from),
                step_duration: edge.step_duration,
                show_step_number: flowchart.step_numbers.then_some(true),
                arrowhead: None,
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
                curve: Some(
                    edge.curve
                        .clone()
                        .unwrap_or_else(|| flowchart.curve.clone()),
                ),
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
        use_max_width: Some(flowchart.use_max_width),
        padding: Some(flowchart.diagram_padding),
        node_spacing: Some(flowchart.node_spacing),
        rank_spacing: Some(flowchart.rank_spacing),
    }
}

fn layout_node(node: &FlowNode, config: &Config) -> PipelineNode {
    let flowchart = &config.flowchart;
    let is_group = node.is_group;
    // A collapsed subgraph is drawn as a plain node and, like `getData`'s collapsed branch,
    // carries neither the diagram's node padding nor its minimum width.
    let collapsed = node.shape == "collapsedGroup";
    PipelineNode {
        id: node.id.clone(),
        label: Some(node.label.clone()),
        css_styles: Some(node.css_styles.clone()),
        css_compiled_styles: Some(node.css_compiled_styles.clone()),
        css_classes: Some(node.css_classes.clone()),
        label_style: Some(node.label_style.clone()),
        label_type: Some(node.label_type.as_str().to_string()),
        dom_id: node.dom_id.clone(),
        parent_id: node.parent_id.clone(),
        dir: node.dir.clone(),
        padding: Some(if is_group || collapsed {
            node.padding
        } else {
            flowchart.padding
        }),
        is_group: Some(is_group),
        shape: Some(node.shape.clone()),
        look: Some(config.look.clone()),
        link: node.link.clone(),
        link_target: node.link_target.clone(),
        tooltip: node.tooltip.clone(),
        icon: node.icon.clone(),
        pos: node.pos.clone(),
        img: node.img.clone(),
        asset_width: node.asset_width,
        asset_height: node.asset_height,
        min_width: if is_group || collapsed {
            None
        } else {
            Some(flowchart.min_node_width)
        },
        wrapping_width: Some(flowchart.wrapping_width),
        constraint: node.constraint.clone(),
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
    use crate::diagrams::flowchart::parse;

    fn ast(source: &str) -> Ast {
        parse(source).expect("parse flowchart")
    }

    #[test]
    fn maps_draw_settings_from_the_configuration() {
        let config = Config::default();
        let data = layout_data(&ast("flowchart TD\n  A --> B\n"), &config, "d");

        assert_eq!(data.direction.as_deref(), Some("TB"));
        assert_eq!(data.r#type.as_deref(), Some(DIAGRAM_TYPE));
        assert_eq!(data.markers, ["point", "circle", "cross"]);
        assert_eq!(data.diagram_id.as_deref(), Some("d"));
        assert_eq!(data.padding, Some(config.flowchart.diagram_padding));
        assert_eq!(data.node_spacing, Some(config.flowchart.node_spacing));
        assert_eq!(data.rank_spacing, Some(config.flowchart.rank_spacing));
        assert_eq!(data.use_max_width, Some(config.flowchart.use_max_width));
    }

    #[test]
    fn maps_node_padding_minimum_width_and_shape() {
        let config = Config::default();
        let data = layout_data(&ast("flowchart TD\n  A[Alpha]\n"), &config, "d");
        let node = &data.nodes[0];
        assert_eq!(node.shape.as_deref(), Some("squareRect"));
        assert_eq!(node.padding, Some(config.flowchart.padding));
        assert_eq!(node.min_width, Some(config.flowchart.min_node_width));
        assert_eq!(node.wrapping_width, Some(config.flowchart.wrapping_width));
        assert_eq!(node.look.as_deref(), Some(config.look.as_str()));
    }

    #[test]
    fn maps_subgraph_as_a_group_without_a_minimum_width() {
        let config = Config::default();
        let data = layout_data(
            &ast("flowchart TD\n  subgraph One\n    A\n  end\n"),
            &config,
            "d",
        );
        let group = data
            .nodes
            .iter()
            .find(|node| node.id == "One")
            .expect("group node");
        assert_eq!(group.is_group, Some(true));
        assert_eq!(group.shape.as_deref(), Some("rect"));
        assert_eq!(group.min_width, None);
    }

    #[test]
    fn maps_edge_labels_arrows_and_curve() {
        let config = Config::default();
        let data = layout_data(
            &ast("flowchart TD\n  A -->|go| B\n  B -.-> C\n"),
            &config,
            "d",
        );
        let first = &data.edges[0];
        assert_eq!(first.label.as_deref(), Some("go"));
        assert_eq!(first.arrow_type_start.as_deref(), Some("none"));
        assert_eq!(first.arrow_type_end.as_deref(), Some("arrow_point"));
        assert_eq!(
            first.curve.as_deref(),
            Some(config.flowchart.curve.as_str())
        );

        let dotted = &data.edges[1];
        assert_eq!(dotted.thickness.as_deref(), Some("dotted"));
        assert_eq!(dotted.pattern.as_deref(), Some("dotted"));
    }
}
