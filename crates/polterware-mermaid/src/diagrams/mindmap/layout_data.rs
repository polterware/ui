//! Conversion of the mindmap database projection into the shared layout pipeline's
//! [`LayoutData`], the Rust side of `mindmapDb.ts` `getData()` and of the node sizing
//! `mindmapRenderer.ts` `draw` applies before it renders.
//!
//! `getData` hands over one node per mindmap entry (in tree preorder) and one edge per
//! parent-child pair, already carrying the section each node takes from `assignSections`.
//! [`layout_data_from`] adds what `draw` reads from the configuration after `diag.db.getData()`:
//! the diagram type, the resolved layout algorithm (cose-bilkent by default), the diagram
//! id, the viewport padding and `useMaxWidth`, and the shape-specific sizes and paddings
//! `draw` writes before the pipeline measures the nodes:
//!
//! * `rounded` nodes take a radius, a taper, no stroke, a zero width, and a padding of 15;
//! * `circle` nodes keep their maximum-width wrapping box and take a padding of 10;
//! * `rect` nodes get a zero width and a padding of 10;
//! * `hexagon` nodes get a zero width and height.
//!
//! A zero width is written as `None` here: upstream's `node.width || node.wrappingWidth ||`
//! fallback skips a zero, so the label wraps at `flowchart.wrappingWidth` instead of not
//! wrapping at all. `height` is `None` for the same reason (`draw` only writes it for the
//! hexagon, and the shape handlers test it for truthiness).

use crate::config::Config;
use crate::render::pipeline::{
    Edge as PipelineEdge, LayoutData, Node as PipelineNode, get_registered_layout_algorithm,
};

use super::ast::{ColorBy, Data, LayoutEdge, LayoutNode, NodeType};

/// The diagram id `render()` prefixes DOM ids with when the caller names none.
pub const DEFAULT_DIAGRAM_ID: &str = "mermaid";

/// The diagram type upstream registers the mindmap renderer under.
pub const DIAGRAM_TYPE: &str = "mindmap";

/// `getData`'s node spacing (`nodeSpacing: 50`).
pub const NODE_SPACING: f64 = 50.0;

/// `getData`'s rank spacing (`rankSpacing: 50`).
pub const RANK_SPACING: f64 = 50.0;

/// The layout `getData` forces when the configuration names none.
pub const FALLBACK_LAYOUT: &str = "cose-bilkent";

/// `assignSections`' mode, from `config.mindmap.colorBy`.
pub fn color_by(config: &Config) -> ColorBy {
    match config.mindmap.color_by.as_str() {
        "depth" => ColorBy::Depth,
        _ => ColorBy::Branch,
    }
}

/// Builds the layout data from an already projected [`Data`], so the renderer can keep the
/// sections the colors are painted from.
pub fn layout_data_from(data: &Data, config: &Config, diagram_id: &str) -> LayoutData {
    let nodes = data
        .nodes
        .iter()
        .map(|node| layout_node(node, config))
        .collect();
    let edges = data
        .edges
        .iter()
        .map(|edge| layout_edge(edge, config))
        .collect();

    // `getData` carries `rootNode: mindmapRoot`; the cose-bilkent layout validates that it is
    // present (it only checks for it), and the tree itself stays with the caller.
    let root_node = data.nodes.first().map(|root| {
        serde_json::json!({
            "id": root.id,
            "nodeId": root.node_id,
            "descr": root.label,
            "level": root.level,
        })
    });

    LayoutData {
        nodes,
        edges,
        config: config.clone(),
        diagram_id: Some(diagram_id.to_string()),
        layout_algorithm: Some(resolve_layout_algorithm(config)),
        direction: Some(data.direction.clone()),
        r#type: Some(DIAGRAM_TYPE.to_string()),
        markers: data.markers.clone(),
        use_max_width: Some(config.mindmap.use_max_width),
        padding: Some(config.mindmap.padding),
        node_spacing: Some(NODE_SPACING),
        rank_spacing: Some(RANK_SPACING),
        root_node,
        other: None,
    }
}

/// `getRegisteredLayoutAlgorithm(data4Layout.config.layout, { fallback: 'cose-bilkent' })`.
///
/// `getData` writes `cose-bilkent` when the user defined no layout, and upstream's default
/// `layout` (`elk`) is not a layout this crate implements; both end on the fallback.
fn resolve_layout_algorithm(config: &Config) -> String {
    let requested = if config.layout.is_empty() || config.layout == "elk" {
        FALLBACK_LAYOUT
    } else {
        config.layout.as_str()
    };
    get_registered_layout_algorithm(requested, Some(FALLBACK_LAYOUT))
}

fn layout_node(node: &LayoutNode, config: &Config) -> PipelineNode {
    let (width, height, padding) = draw_shape(node.node_type, config);
    PipelineNode {
        id: node.id.clone(),
        label: Some(node.label.clone()),
        label_type: Some(node.label_type.clone()),
        dom_id: Some(node.dom_id.clone()),
        is_group: Some(false),
        shape: Some(shape_for(node, config)),
        width,
        height,
        padding: Some(padding),
        css_classes: Some(node.css_classes.clone()),
        css_styles: Some(node.css_styles.clone()),
        center_label: Some(node.center_label),
        icon: node.icon.clone(),
        level: Some(f64::from(node.level)),
        node_id: Some(node.node_id.clone()),
        r#type: Some(f64::from(node.node_type.as_number())),
        look: Some(config.look.clone()),
        wrapping_width: Some(config.flowchart.wrapping_width),
        ..PipelineNode::default()
    }
}

fn layout_edge(edge: &LayoutEdge, config: &Config) -> PipelineEdge {
    PipelineEdge {
        id: edge.id.clone(),
        start: Some(edge.start.clone()),
        end: Some(edge.end.clone()),
        r#type: Some(edge.type_name.clone()),
        curve: Some(edge.curve.clone()),
        thickness: Some(edge.thickness.clone()),
        classes: Some(edge.classes.clone()),
        look: Some(config.look.clone()),
        depth: Some(f64::from(edge.depth)),
        ..PipelineEdge::default()
    }
}

/// `getShapeFromType`: the shape name, with the `redux` themes drawing an undelimited node as
/// a rounded rectangle.
fn shape_for(node: &LayoutNode, config: &Config) -> String {
    if node.shape == "defaultMindmapNode" && config.theme.to_lowercase().contains("redux") {
        "rounded".to_string()
    } else {
        node.shape.clone()
    }
}

/// The width, height, and padding `draw` leaves on a node of this type.
fn draw_shape(node_type: NodeType, config: &Config) -> (Option<f64>, Option<f64>, f64) {
    let max_width = config.mindmap.max_node_width;
    match node_type {
        NodeType::NoBorder => (Some(max_width), None, config.mindmap.padding),
        NodeType::RoundedRect => (None, None, 15.0),
        NodeType::Rect => (None, None, 10.0),
        NodeType::Circle => (Some(max_width), None, 10.0),
        NodeType::Cloud => (Some(max_width), None, config.mindmap.padding),
        NodeType::Bang => (Some(max_width), None, config.mindmap.padding),
        NodeType::Hexagon => (None, None, config.mindmap.padding * 2.0),
    }
}

#[cfg(test)]
mod tests {
    use super::super::ast::Ast;
    use super::*;
    use crate::diagrams::mindmap::parse;

    fn ast(source: &str) -> Ast {
        parse(source).expect("parse mindmap")
    }

    fn layout_data(source: &str, config: &Config, diagram_id: &str) -> LayoutData {
        layout_data_from(&ast(source).data_with(color_by(config)), config, diagram_id)
    }

    #[test]
    fn maps_draw_settings_from_the_configuration() {
        let config = Config::default();
        let data = layout_data("mindmap\n  root\n", &config, "d");

        assert_eq!(data.r#type.as_deref(), Some(DIAGRAM_TYPE));
        assert_eq!(data.layout_algorithm.as_deref(), Some(FALLBACK_LAYOUT));
        assert_eq!(data.diagram_id.as_deref(), Some("d"));
        assert_eq!(data.direction.as_deref(), Some("TB"));
        assert_eq!(data.markers, ["point"]);
        assert_eq!(data.padding, Some(config.mindmap.padding));
        assert_eq!(data.node_spacing, Some(NODE_SPACING));
        assert_eq!(data.rank_spacing, Some(RANK_SPACING));
        assert_eq!(data.use_max_width, Some(config.mindmap.use_max_width));
    }

    #[test]
    fn applies_the_draw_shape_sizes_and_paddings() {
        let config = Config::default();
        let data = layout_data(
            "mindmap\n  root((root))\n    plain\n    square[S]\n    rounded(R)\n    cloud)C(\n    bang))B((\n    hexagon{{H}}\n",
            &config,
            "d",
        );

        let node = |shape: &str| {
            data.nodes
                .iter()
                .find(|node| node.shape.as_deref() == Some(shape))
                .unwrap_or_else(|| panic!("node {shape}"))
        };

        let root = node("mindmapCircle");
        assert_eq!(root.width, Some(config.mindmap.max_node_width));
        assert_eq!(root.padding, Some(10.0));
        assert_eq!(root.level, Some(0.0));

        let plain = node("defaultMindmapNode");
        assert_eq!(plain.width, Some(config.mindmap.max_node_width));
        assert_eq!(plain.padding, Some(config.mindmap.padding));

        let square = node("rect");
        assert_eq!(square.width, None);
        assert_eq!(square.height, None);
        assert_eq!(square.padding, Some(10.0));

        let rounded = node("rounded");
        assert_eq!(rounded.width, None);
        assert_eq!(rounded.padding, Some(15.0));

        let cloud = node("cloud");
        assert_eq!(cloud.width, Some(config.mindmap.max_node_width));
        assert_eq!(cloud.padding, Some(config.mindmap.padding));

        let hexagon = node("hexagon");
        assert_eq!(hexagon.width, None);
        assert_eq!(hexagon.height, None);
        assert_eq!(hexagon.padding, Some(config.mindmap.padding * 2.0));
    }

    #[test]
    fn copies_the_section_classes_and_edge_ids() {
        let config = Config::default();
        let data = layout_data("mindmap\n  root\n    a\n    b\n", &config, "d");

        assert_eq!(
            data.nodes[0].css_classes.as_deref(),
            Some("mindmap-node section-root section--1")
        );
        assert_eq!(
            data.nodes[1].css_classes.as_deref(),
            Some("mindmap-node section-0")
        );
        assert_eq!(
            data.nodes[2].css_classes.as_deref(),
            Some("mindmap-node section-1")
        );
        assert_eq!(data.edges[0].id, "edge_0_1");
        assert_eq!(
            data.edges[0].classes.as_deref(),
            Some("edge section-edge-0 edge-depth-1")
        );
        assert_eq!(data.edges[0].curve.as_deref(), Some("basis"));
        assert_eq!(data.edges[0].thickness.as_deref(), Some("normal"));
        assert_eq!(data.edges[0].depth, Some(0.0));
    }

    #[test]
    fn honors_color_by_depth() {
        let mut config = Config::default();
        config.mindmap.color_by = "depth".to_string();
        let data = layout_data_from(
            &ast("mindmap\n  root\n    a\n      b\n").data_with(ColorBy::Depth),
            &config,
            "d",
        );
        let classes = |id: &str| {
            data.nodes
                .iter()
                .find(|node| node.id == id)
                .and_then(|node| node.css_classes.clone())
                .unwrap_or_default()
        };
        assert_eq!(classes("1"), "mindmap-node section-0");
        assert_eq!(classes("2"), "mindmap-node section-1");
    }

    #[test]
    fn uses_the_rounded_default_shape_for_redux_themes() {
        let mut config = Config::default();
        config.theme = "redux-color".to_string();
        let data = layout_data("mindmap\n  root\n    plain\n", &config, "d");
        assert_eq!(data.nodes[0].shape.as_deref(), Some("rounded"));
    }
}
