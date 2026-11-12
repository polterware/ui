//! Conversion of the state database projection into the shared layout pipeline's
//! [`LayoutData`], the Rust side of `stateDb.ts` `getData()` and the fields
//! `stateRenderer-v3-unified.ts` `draw` reads from the configuration.
//!
//! `getData` returns the nodes and edges `dataFetcher.ts` derives, plus the diagram direction
//! and, for each node, the wrapping width and minimum label width of the state configuration.
//! `draw` adds the diagram type, the resolved layout algorithm, the markers the look chooses,
//! the diagram id, and the node and rank spacing (`state.nodeSpacing` / `state.rankSpacing`,
//! falling back to `50`). The viewport is set up with a constant `8` padding and
//! `state.useMaxWidth`.
//!
//! The arrow type the database recorded is the default look's (`neo`); `dataFetcher` picks it
//! from the run-time look, so [`layout_data`] rewrites it from `config.look` here, where the
//! configuration is known.

use crate::config::Config;
use crate::render::pipeline::{
    Edge as PipelineEdge, LayoutData, Node as PipelineNode, get_registered_layout_algorithm,
};

use super::ast::{Ast, Edge, Node};

/// The diagram id `render()` prefixes DOM ids with when the caller names none.
pub const DEFAULT_DIAGRAM_ID: &str = "mermaid";

/// The diagram type upstream registers the unified state renderer under
/// (`stateDetector-V2.ts`'s id, which both headers resolve to).
pub const DIAGRAM_TYPE: &str = "stateDiagram";

/// `stateRenderer-v3-unified.ts` `draw`'s constant viewport padding.
pub const DIAGRAM_PADDING: f64 = 8.0;

/// The node and rank spacing `draw` falls back to when the state configuration names none.
pub const DEFAULT_SPACING: f64 = 50.0;

/// The markers `draw` registers for each look.
pub fn markers(look: &str) -> Vec<String> {
    let marker = if look == "neo" { "barbNeo" } else { "barb" };
    vec![marker.to_string()]
}

/// Builds the layout data for a parsed state diagram.
///
/// This is `getData()` plus the fields `draw` sets from the configuration.
pub fn layout_data(ast: &Ast, config: &Config, diagram_id: &str) -> LayoutData {
    let state = &config.state;
    let nodes = ast
        .nodes
        .iter()
        .map(|node| layout_node(node, config))
        .collect();
    let edges = ast
        .edges
        .iter()
        .map(|edge| layout_edge(edge, config))
        .collect();

    LayoutData {
        nodes,
        edges,
        config: config.clone(),
        diagram_id: Some(diagram_id.to_string()),
        layout_algorithm: Some(get_registered_layout_algorithm(&config.layout, None)),
        direction: Some(ast.layout_direction.as_str().to_string()),
        r#type: Some(DIAGRAM_TYPE.to_string()),
        markers: markers(&config.look),
        use_max_width: Some(state.use_max_width),
        padding: Some(DIAGRAM_PADDING),
        node_spacing: Some(state.node_spacing.unwrap_or(DEFAULT_SPACING)),
        rank_spacing: Some(state.rank_spacing.unwrap_or(DEFAULT_SPACING)),
        // A state diagram has no root: only mindmap-like diagrams laid out by cose-bilkent
        // set one.
        root_node: None,
    }
}

fn layout_node(node: &Node, config: &Config) -> PipelineNode {
    let state = &config.state;
    PipelineNode {
        id: node.id.clone(),
        label: Some(node.label.clone()),
        description: Some(node.description.clone()),
        css_styles: Some(node.css_styles.clone()),
        css_compiled_styles: Some(node.css_compiled_styles.clone()),
        css_classes: Some(node.css_classes.clone()),
        label_style: node.label_style.clone(),
        label_type: node.label_type.clone(),
        dom_id: Some(node.dom_id.clone()),
        dir: node.dir.map(|dir| dir.as_str().to_string()),
        is_group: Some(node.is_group),
        shape: node.shape.clone(),
        look: Some(config.look.clone()),
        padding: node.padding,
        rx: node.rx,
        ry: node.ry,
        center_label: node.center_label,
        parent_id: node.parent_id.clone(),
        position: node.position.map(|position| position.as_str().to_string()),
        color_index: node.color_index.map(|index| index as usize),
        // `getData` fills both from the state configuration: every node wraps at
        // `wrappingWidth`, and a leaf also gets the minimum label width. A composite is
        // not widened.
        wrapping_width: Some(state.wrapping_width),
        min_width: (!node.is_group).then_some(state.min_node_width),
        ..PipelineNode::default()
    }
}

fn layout_edge(edge: &Edge, config: &Config) -> PipelineEdge {
    // `dataFetcher` chooses the arrow type from the run-time look.
    let arrow_type_end = if edge.arrow_type_end.is_empty() {
        None
    } else if config.look == "neo" {
        Some("arrow_barb_neo".to_string())
    } else {
        Some("arrow_barb".to_string())
    };
    PipelineEdge {
        id: edge.id.clone(),
        label: edge.label.clone(),
        classes: Some(edge.classes.clone()),
        style: Some(vec![edge.style.clone()]),
        arrowhead: Some(edge.arrowhead.clone()),
        arrowhead_style: Some(edge.arrowhead_style.clone()),
        arrow_type_end,
        label_type: Some(edge.label_type.clone()),
        labelpos: Some(edge.labelpos.clone()),
        thickness: Some(edge.thickness.clone()),
        pattern: edge.pattern.map(|pattern| pattern.as_str().to_string()),
        look: Some(config.look.clone()),
        start: Some(edge.start.clone()),
        end: Some(edge.end.clone()),
        ..PipelineEdge::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagrams::state::parse;

    fn ast(source: &str) -> Ast {
        parse(source).expect("parse state diagram")
    }

    #[test]
    fn maps_draw_settings_from_the_configuration() {
        let config = Config::default();
        let data = layout_data(&ast("stateDiagram-v2\n  [*] --> A\n"), &config, "d");

        assert_eq!(data.direction.as_deref(), Some("TB"));
        assert_eq!(data.r#type.as_deref(), Some(DIAGRAM_TYPE));
        assert_eq!(data.markers, ["barb"]);
        assert_eq!(data.diagram_id.as_deref(), Some("d"));
        assert_eq!(data.padding, Some(DIAGRAM_PADDING));
        assert_eq!(data.node_spacing, Some(DEFAULT_SPACING));
        assert_eq!(data.rank_spacing, Some(DEFAULT_SPACING));
        assert_eq!(data.use_max_width, Some(config.state.use_max_width));
    }

    #[test]
    fn maps_node_wrapping_and_minimum_width() {
        let config = Config::default();
        let data = layout_data(&ast("stateDiagram-v2\n  A\n"), &config, "d");
        let node = &data.nodes[0];
        assert_eq!(node.shape.as_deref(), Some("rect"));
        assert_eq!(node.padding, Some(8.0));
        assert_eq!(node.rx, Some(10.0));
        assert_eq!(node.wrapping_width, Some(config.state.wrapping_width));
        assert_eq!(node.min_width, Some(config.state.min_node_width));
        assert_eq!(node.label_type.as_deref(), Some("markdown"));
        assert_eq!(node.look.as_deref(), Some(config.look.as_str()));
    }

    #[test]
    fn maps_composite_without_a_minimum_width() {
        let config = Config::default();
        let data = layout_data(
            &ast("stateDiagram-v2\n  state One {\n    A --> B\n  }\n"),
            &config,
            "d",
        );
        let group = data
            .nodes
            .iter()
            .find(|node| node.id == "One")
            .expect("group node");
        assert_eq!(group.is_group, Some(true));
        assert_eq!(group.shape.as_deref(), Some("roundedWithTitle"));
        assert_eq!(group.min_width, None);
        assert_eq!(group.color_index, Some(0));
    }

    #[test]
    fn a_note_is_a_leaf_that_inherits_wrapping_and_minimum_width() {
        // `stateDb.getData` sets `wrappingWidth` on every node and `minWidth` on every
        // non-group node, so the note leaves (`note.ts` through `labelHelper`) are widened
        // like states. `dataFetcher` gives the note `labelType: 'markdown'`.
        let config = Config::default();
        let data = layout_data(
            &ast("stateDiagram-v2\n  [*] --> Idle\n  note right of Idle: Waiting for input\n"),
            &config,
            "d",
        );
        let note = data
            .nodes
            .iter()
            .find(|node| node.shape.as_deref() == Some("note"))
            .expect("note node");
        assert_eq!(note.is_group, Some(false));
        assert_eq!(note.padding, Some(15.0));
        assert_eq!(note.wrapping_width, Some(config.state.wrapping_width));
        assert_eq!(note.min_width, Some(config.state.min_node_width));
        assert_eq!(note.label_type.as_deref(), Some("markdown"));
    }

    #[test]
    fn maps_edges_and_the_look_s_arrow_type() {
        let classic = Config::default();
        let data = layout_data(
            &ast("stateDiagram-v2\n  A --> B: go\n  B --> C\n"),
            &classic,
            "d",
        );
        assert_eq!(data.edges[0].label.as_deref(), Some("go"));
        assert_eq!(data.edges[0].arrow_type_end.as_deref(), Some("arrow_barb"));
        assert_eq!(data.edges[0].classes.as_deref(), Some("transition"));

        let mut neo = Config::default();
        neo.look = "neo".to_string();
        let data = layout_data(&ast("stateDiagram-v2\n  A --> B\n"), &neo, "d");
        assert_eq!(data.markers, ["barbNeo"]);
        assert_eq!(
            data.edges[0].arrow_type_end.as_deref(),
            Some("arrow_barb_neo")
        );
    }

    #[test]
    fn maps_a_note_edge_without_an_arrow() {
        let config = Config::default();
        let data = layout_data(
            &ast("stateDiagram-v2\n  A\n  note right of A: hello\n"),
            &config,
            "d",
        );
        let note_edge = data
            .edges
            .iter()
            .find(|edge| edge.classes.as_deref() == Some("transition note-edge"))
            .expect("note edge");
        assert_eq!(note_edge.arrow_type_end, None);
        assert_eq!(note_edge.pattern.as_deref(), Some("dashed"));
        assert_eq!(note_edge.label, None);
    }
}
