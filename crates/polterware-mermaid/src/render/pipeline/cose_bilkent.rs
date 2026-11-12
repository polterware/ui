//! Port of Cytoscape CoSE-Bilkent layout and Mermaid's cose-bilkent layout algorithm.
//!
//! Upstream sources ported line by line:
//! - `cytoscape-cose-bilkent/src/index.js`: options, defaults, graph construction, and layout execution.
//! - `rendering-util/layout-algorithms/cose-bilkent/types.ts`: [`PositionedNode`], [`PositionedEdge`], [`LayoutResult`].
//! - `rendering-util/layout-algorithms/cose-bilkent/cytoscape-setup.ts`: node/edge mapping and endpoint extraction.
//! - `rendering-util/layout-algorithms/cose-bilkent/layout.ts`: [`validate_layout_data`] and [`execute_cose_bilkent_layout`].
//! - `rendering-util/layout-algorithms/cose-bilkent/render.ts`: [`cose_bilkent_render`].

use std::collections::{HashMap, HashSet};

use crate::config::Config;
use crate::layout::cose_base::CoSELayout;
use crate::layout::layout_base::l_graph::GraphId;
use crate::layout::layout_base::l_node::NodeId;
use crate::layout::layout_base::util::dimension_d::DimensionD;
use crate::layout::layout_base::util::point::Point;
use crate::layout::layout_base::util::random_seed;
use crate::measure::TextMeasure;
use crate::render::clusters::{ClusterNode as RenderClusterNode, render_cluster_measured};
use crate::render::edges::{EdgeData, insert_edge_with_type};
use crate::render::layout_dagre::LayoutError;
use crate::render::markers::marker_definitions;
use crate::render::shapes::position_node;
use crate::scene::{PathShape, Rect, Scene};
use crate::theme::Theme;

use super::create_graph::create_graph_with_elements;
use super::setup_view_port_for_svg::setup_view_port_for_svg;
use super::types::{CreateGraphOptions, Edge, LayoutData, Node};

/// Positioned node after layout calculation.
/// Upstream: `PositionedNode` in `cose-bilkent/types.ts`.
#[derive(Clone, Debug, PartialEq)]
pub struct PositionedNode {
    pub id: String,
    pub x: f64,
    pub y: f64,
}

/// Positioned edge after layout calculation.
/// Upstream: `PositionedEdge` in `cose-bilkent/types.ts`.
#[derive(Clone, Debug, PartialEq)]
pub struct PositionedEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub start_x: f64,
    pub start_y: f64,
    pub mid_x: f64,
    pub mid_y: f64,
    pub end_x: f64,
    pub end_y: f64,
}

/// Result of layout algorithm execution.
/// Upstream: `LayoutResult` in `cose-bilkent/types.ts`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayoutResult {
    pub nodes: Vec<PositionedNode>,
    pub edges: Vec<PositionedEdge>,
}

/// Cytoscape layout configuration.
/// Upstream: `CytoscapeLayoutConfig` in `cose-bilkent/types.ts`.
#[derive(Clone, Debug, PartialEq)]
pub struct CytoscapeLayoutConfig {
    pub name: String,
    pub quality: String,
    pub style_enabled: bool,
    pub animate: bool,
}

impl Default for CytoscapeLayoutConfig {
    fn default() -> Self {
        Self {
            name: "cose-bilkent".to_string(),
            quality: "proof".to_string(),
            style_enabled: false,
            animate: false,
        }
    }
}

/// Options for CoSE-Bilkent layout.
/// Upstream: `defaults` in `cytoscape-cose-bilkent/src/index.js`.
#[derive(Clone, Debug)]
pub struct CoseLayoutOptions {
    pub quality: String,
    pub node_dimensions_include_labels: bool,
    pub refresh: i32,
    pub fit: bool,
    pub padding: f64,
    pub randomize: bool,
    pub node_repulsion: f64,
    pub ideal_edge_length: f64,
    pub edge_elasticity: f64,
    pub nesting_factor: f64,
    pub gravity: f64,
    pub num_iter: i32,
    pub tile: bool,
    pub animate: bool,
    pub animation_duration: f64,
    pub tiling_padding_vertical: f64,
    pub tiling_padding_horizontal: f64,
    pub gravity_range_compound: f64,
    pub gravity_compound: f64,
    pub gravity_range: f64,
    pub initial_energy_on_incremental: f64,
}

impl Default for CoseLayoutOptions {
    fn default() -> Self {
        Self {
            quality: "default".to_string(),
            node_dimensions_include_labels: false,
            refresh: 30,
            fit: true,
            padding: 10.0,
            randomize: true,
            node_repulsion: 4500.0,
            ideal_edge_length: 50.0,
            edge_elasticity: 0.45,
            nesting_factor: 0.1,
            gravity: 0.25,
            num_iter: 2500,
            tile: true,
            animate: false,
            animation_duration: 500.0,
            tiling_padding_vertical: 10.0,
            tiling_padding_horizontal: 10.0,
            gravity_range_compound: 1.5,
            gravity_compound: 1.0,
            gravity_range: 3.8,
            initial_energy_on_incremental: 0.5,
        }
    }
}

/// Recursively process node children hierarchy into CoSE layout graph manager.
/// Upstream: `_CoSELayout.prototype.processChildrenList` in `cytoscape-cose-bilkent/src/index.js`.
fn process_children_list(
    layout: &mut CoSELayout<()>,
    parent_graph: GraphId,
    children: &[&Node],
    children_map: &HashMap<String, Vec<&Node>>,
    id_to_lnode: &mut HashMap<String, NodeId>,
) -> Result<(), LayoutError> {
    for child in children {
        let the_node = layout.new_node(None);
        layout
            .fd
            .layout
            .graph_manager
            .graph_add_node(parent_graph, the_node)
            .map_err(|e| LayoutError::Dagre(format!("Failed to add node {}: {e}", child.id)))?;

        let w = child.width.unwrap_or(40.0);
        let h = child.height.unwrap_or(40.0);
        let x = child.x.unwrap_or(0.0);
        let y = child.y.unwrap_or(0.0);

        layout.fd.layout.graph_manager.node_mut(the_node).set_rect(
            &Point::new(x - w / 2.0, y - h / 2.0),
            &DimensionD::new(w, h),
        );
        layout.fd.node_mut(the_node).id = Some(child.id.clone());

        let padding = child.padding.unwrap_or(0.0);
        let lnode = layout.fd.layout.graph_manager.node_mut(the_node);
        lnode.padding_left = Some(padding);
        lnode.padding_top = Some(padding);
        lnode.padding_right = Some(padding);
        lnode.padding_bottom = Some(padding);

        id_to_lnode.insert(child.id.clone(), the_node);

        if let Some(sub_children) = children_map.get(&child.id)
            && !sub_children.is_empty()
        {
            let new_graph_id = layout.new_graph(None);
            let new_graph = layout
                .fd
                .layout
                .graph_manager
                .add_graph(new_graph_id, the_node)
                .map_err(|e| {
                    LayoutError::Dagre(format!("Failed to add graph for {}: {e}", child.id))
                })?;
            process_children_list(layout, new_graph, sub_children, children_map, id_to_lnode)?;
        }
    }
    Ok(())
}

/// Find top-most nodes in hierarchy.
/// Upstream: `_CoSELayout.prototype.getTopMostNodes` in `cytoscape-cose-bilkent/src/index.js`.
fn get_top_most_nodes(nodes: &[Node]) -> Vec<&Node> {
    let nodes_map: HashSet<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
    nodes
        .iter()
        .filter(|node| {
            if let Some(parent_id) = &node.parent_id {
                !nodes_map.contains(parent_id.as_str())
            } else {
                true
            }
        })
        .collect()
}

/// Execute the CoSE layout algorithm on a minimal graph representation.
/// Upstream: `_CoSELayout.prototype.run` in `cytoscape-cose-bilkent/src/index.js`.
pub fn run_cose_bilkent_layout(
    nodes: &[Node],
    edges: &[Edge],
    options: &CoseLayoutOptions,
) -> Result<LayoutResult, LayoutError> {
    random_seed::set_seed(1);

    let mut layout = CoSELayout::<()>::new();
    layout.quality = match options.quality.as_str() {
        "draft" => Some(0),
        "proof" => Some(2),
        _ => Some(1),
    };
    layout.node_repulsion = Some(options.node_repulsion);
    layout.ideal_edge_length = Some(options.ideal_edge_length);
    layout.edge_elasticity = Some(options.edge_elasticity);
    layout.nesting_factor = Some(options.nesting_factor);
    layout.gravity = Some(options.gravity);
    layout.num_iter = Some(options.num_iter as i64);
    layout.gravity_range = Some(options.gravity_range);
    layout.gravity_compound = Some(options.gravity_compound);
    layout.gravity_range_compound = Some(options.gravity_range_compound);
    layout.initial_energy_on_incremental = Some(options.initial_energy_on_incremental);
    layout.tiling_padding_vertical = options.tiling_padding_vertical;
    layout.tiling_padding_horizontal = options.tiling_padding_horizontal;

    layout.new_graph_manager();
    let root_graph = layout.fd.add_root();

    let mut children_map: HashMap<String, Vec<&Node>> = HashMap::new();
    for node in nodes {
        if let Some(parent_id) = &node.parent_id {
            children_map
                .entry(parent_id.clone())
                .or_default()
                .push(node);
        }
    }

    let top_nodes = get_top_most_nodes(nodes);
    let mut id_to_lnode = HashMap::new();
    process_children_list(
        &mut layout,
        root_graph,
        &top_nodes,
        &children_map,
        &mut id_to_lnode,
    )?;

    for edge in edges {
        if let (Some(source_id), Some(target_id)) = (&edge.start, &edge.end)
            && let (Some(&source_node), Some(&target_node)) =
                (id_to_lnode.get(source_id), id_to_lnode.get(target_id))
            && source_node != target_node
            && layout
                .fd
                .layout
                .graph_manager
                .node_edges_between(source_node, target_node)
                .is_empty()
        {
            let e1 = layout.new_edge(None);
            layout
                .fd
                .layout
                .graph_manager
                .add_edge(e1, source_node, target_node)
                .map_err(|e| LayoutError::Dagre(format!("Failed to add edge {}: {e}", edge.id)))?;
        }
    }

    layout
        .run_layout()
        .map_err(|e| LayoutError::Dagre(format!("CoSE layout run failed: {e}")))?;

    // Extract positioned nodes
    let mut positioned_nodes = Vec::new();
    for node in nodes {
        if let Some(&lnode) = id_to_lnode.get(&node.id) {
            let rect = layout.fd.layout.graph_manager.node(lnode).get_rect();
            positioned_nodes.push(PositionedNode {
                id: node.id.clone(),
                x: rect.get_center_x(),
                y: rect.get_center_y(),
            });
        }
    }

    // Extract positioned edges
    let node_pos_map: HashMap<&str, (f64, f64)> = positioned_nodes
        .iter()
        .map(|n| (n.id.as_str(), (n.x, n.y)))
        .collect();

    let mut positioned_edges = Vec::new();
    for edge in edges {
        if let (Some(source_id), Some(target_id)) = (&edge.start, &edge.end)
            && let (Some(&(sx, sy)), Some(&(tx, ty))) = (
                node_pos_map.get(source_id.as_str()),
                node_pos_map.get(target_id.as_str()),
            )
        {
            let dx = tx - sx;
            let dy = ty - sy;
            let dist = dx.hypot(dy);
            let r = 15.0;
            let (start_x, start_y, end_x, end_y, mid_x, mid_y) = if dist <= 2.0 * r || dist == 0.0 {
                (sx, sy, tx, ty, (sx + tx) / 2.0, (sy + ty) / 2.0)
            } else {
                let ux = dx / dist;
                let uy = dy / dist;
                let start_x = sx + r * ux;
                let start_y = sy + r * uy;
                let end_x = tx - r * ux;
                let end_y = ty - r * uy;
                (
                    start_x,
                    start_y,
                    end_x,
                    end_y,
                    (start_x + end_x) / 2.0,
                    (start_y + end_y) / 2.0,
                )
            };
            positioned_edges.push(PositionedEdge {
                id: edge.id.clone(),
                source: source_id.clone(),
                target: target_id.clone(),
                start_x,
                start_y,
                mid_x,
                mid_y,
                end_x,
                end_y,
            });
        }
    }

    Ok(LayoutResult {
        nodes: positioned_nodes,
        edges: positioned_edges,
    })
}

/// Validate layout data structure.
/// Upstream: `validateLayoutData` in `cose-bilkent/layout.ts`.
pub fn validate_layout_data(data: &LayoutData) -> Result<(), LayoutError> {
    if data.root_node.is_none() {
        return Err(LayoutError::Dagre("Root node is required".to_string()));
    }
    if data.nodes.is_empty() {
        return Err(LayoutError::Dagre(
            "No nodes found in layout data".to_string(),
        ));
    }
    Ok(())
}

/// Execute the cose-bilkent layout algorithm on generic layout data.
/// Upstream: `executeCoseBilkentLayout` in `cose-bilkent/layout.ts`.
pub fn execute_cose_bilkent_layout(
    data: &LayoutData,
    _config: &Config,
) -> Result<LayoutResult, LayoutError> {
    validate_layout_data(data)?;

    let options = CoseLayoutOptions {
        quality: "proof".to_string(),
        animate: false,
        ..Default::default()
    };

    run_cose_bilkent_layout(&data.nodes, &data.edges, &options)
}

/// Render function for the cose-bilkent layout algorithm.
/// Upstream: `render` in `cose-bilkent/render.ts`.
pub fn cose_bilkent_render(
    data4_layout: &mut LayoutData,
    measure: &dyn TextMeasure,
) -> Result<Scene, LayoutError> {
    validate_layout_data(data4_layout)?;

    let diagram_id = data4_layout
        .diagram_id
        .clone()
        .unwrap_or_else(|| "diag".to_string());
    let diagram_type = data4_layout
        .r#type
        .clone()
        .unwrap_or_else(|| "flowchart-v2".to_string());

    let mut scene = Scene::default();
    let markers_ref: Vec<&str> = data4_layout.markers.iter().map(|s| s.as_str()).collect();
    let defs = marker_definitions(&diagram_id, &diagram_type, &markers_ref);
    for def in defs {
        scene.define(def);
    }

    let mut overrides = serde_json::Map::new();
    overrides.insert("fontFamily".into(), serde_json::json!("monospace"));
    overrides.insert("fontSize".into(), serde_json::json!("16px"));
    let theme_vars = Theme::Default.variables(&overrides).unwrap_or_default();

    // Step 1: Measure nodes and clusters
    let mut created = create_graph_with_elements(
        data4_layout,
        &CreateGraphOptions::default(),
        measure,
        &theme_vars,
    )
    .map_err(|e| LayoutError::Dagre(format!("Measure failed: {e}")))?;

    // Step 2: Run layout algorithm
    let layout_result = execute_cose_bilkent_layout(data4_layout, &data4_layout.config)?;

    // Step 3: Position nodes and clusters
    for pos_node in &layout_result.nodes {
        if let Some(node) = data4_layout.nodes.iter_mut().find(|n| n.id == pos_node.id) {
            node.x = Some(pos_node.x);
            node.y = Some(pos_node.y);
        }
    }

    // Paint clusters
    for node in &data4_layout.nodes {
        if node.is_group == Some(true) {
            let mut cn = RenderClusterNode::new(
                &node.id,
                node.x.unwrap_or(0.0),
                node.y.unwrap_or(0.0),
                node.width.unwrap_or(0.0),
                node.height.unwrap_or(0.0),
            );
            cn.label = node.label.clone().unwrap_or_default();
            cn.padding = node.padding.unwrap_or(0.0);
            cn.rx = node.rx.unwrap_or(0.0);
            cn.ry = node.ry.unwrap_or(0.0);
            cn.label_width = node.label_b_box.map(|b| b.width).unwrap_or(0.0);
            cn.label_height = node.label_b_box.map(|b| b.height).unwrap_or(0.0);
            render_cluster_measured(&cn, &mut scene, "#ECECFF", "#9370DB", measure);
        }
    }

    // Paint regular nodes
    for node in &data4_layout.nodes {
        if node.is_group != Some(true)
            && let Some(mut group) = created.node_elements.remove(&node.id)
        {
            position_node(
                &mut group,
                &crate::render::shapes::Node::new(&node.id, "", ""),
                node.x.unwrap_or(0.0),
                node.y.unwrap_or(0.0),
                false,
                0.0,
            );
            scene.push(group);
        }
    }

    // Update edge points
    for pos_edge in &layout_result.edges {
        if let Some(edge) = data4_layout.edges.iter_mut().find(|e| e.id == pos_edge.id) {
            edge.points = Some(vec![
                crate::scene::Point::new(pos_edge.start_x, pos_edge.start_y),
                crate::scene::Point::new(pos_edge.mid_x, pos_edge.mid_y),
                crate::scene::Point::new(pos_edge.end_x, pos_edge.end_y),
            ]);
        }
    }

    // Step 4: Paint edges
    for edge in &data4_layout.edges {
        if edge.is_layout_only == Some(true) {
            continue;
        }
        let points = edge.points.clone().unwrap_or_default();
        let edge_data = EdgeData {
            id: edge.id.clone(),
            points,
            label: edge.label.clone(),
            label_width: edge.width.unwrap_or(0.0),
            label_height: edge.height.unwrap_or(0.0),
            label_x: edge.x,
            label_y: edge.y,
            curve: edge.curve.clone(),
            thickness: edge.thickness.clone().unwrap_or_default(),
            pattern: edge.pattern.clone().unwrap_or_default(),
            arrow_type_start: edge.arrow_type_start.clone(),
            arrow_type_end: edge.arrow_type_end.clone(),
            ..Default::default()
        };
        if let Ok(inserted) = insert_edge_with_type(&edge_data, &diagram_id, &diagram_type) {
            scene.push(PathShape {
                path: inserted.path,
                ..Default::default()
            });
        }
    }

    // Compute bounds and viewBox
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for node in &data4_layout.nodes {
        if let (Some(x), Some(y), Some(w), Some(h)) = (node.x, node.y, node.width, node.height) {
            min_x = min_x.min(x - w / 2.0);
            max_x = max_x.max(x + w / 2.0);
            min_y = min_y.min(y - h / 2.0);
            max_y = max_y.max(y + h / 2.0);
        }
        if node.is_group == Some(true)
            && let Some(bbox) = &node.label_b_box
            && let (Some(x), Some(y)) = (node.x, node.y)
            && bbox.width > 0.0
            && bbox.height > 0.0
        {
            min_x = min_x.min(x - bbox.width / 2.0);
            max_x = max_x.max(x + bbox.width / 2.0);
            min_y = min_y.min(y - node.height.unwrap_or(0.0) / 2.0);
            max_y = max_y.max(y - node.height.unwrap_or(0.0) / 2.0 + bbox.height);
        }
    }

    for edge in &data4_layout.edges {
        if let Some(pts) = &edge.points {
            for p in pts {
                min_x = min_x.min(p.x);
                max_x = max_x.max(p.x);
                min_y = min_y.min(p.y);
                max_y = max_y.max(p.y);
            }
        }
        if let (Some(lx), Some(ly), Some(lw), Some(lh)) = (edge.x, edge.y, edge.width, edge.height)
            && lw > 0.0
            && lh > 0.0
        {
            min_x = min_x.min(lx - lw / 2.0);
            max_x = max_x.max(lx + lw / 2.0);
            min_y = min_y.min(ly - lh / 2.0);
            max_y = max_y.max(ly + lh / 2.0);
        }
    }

    let bounds = if min_x <= max_x && min_y <= max_y {
        Rect::new(min_x, min_y, max_x - min_x, max_y - min_y)
    } else {
        Rect::ZERO
    };

    scene.bounds = bounds;
    let padding = data4_layout.padding.unwrap_or(8.0);
    let use_max_width = data4_layout.use_max_width.unwrap_or(false);
    setup_view_port_for_svg(&mut scene, padding, "flowchart", use_max_width);

    Ok(scene)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure::FallbackMeasure;

    #[test]
    fn test_cose_bilkent_basic() {
        let mut data = LayoutData {
            layout_algorithm: Some("cose-bilkent".to_string()),
            root_node: Some(serde_json::json!({"id": "root"})),
            nodes: vec![
                Node::new("root", "Root", "rect"),
                Node::new("child1", "Child 1", "rect"),
            ],
            edges: vec![Edge::new("e1", "root", "child1")],
            ..Default::default()
        };

        let scene = super::super::render(&mut data, &FallbackMeasure).expect("render");

        let root = data.nodes.iter().find(|n| n.id == "root").unwrap();
        let child1 = data.nodes.iter().find(|n| n.id == "child1").unwrap();

        let root_x = root.x.unwrap();
        let root_y = root.y.unwrap();
        let child1_x = child1.x.unwrap();
        let child1_y = child1.y.unwrap();

        assert!(
            (root_x - 154.89447644360843).abs() <= 0.01,
            "root_x: {root_x}"
        );
        assert!((root_y - 24.2).abs() <= 0.01, "root_y: {root_y}");
        assert!((child1_x - 48.6).abs() <= 0.01, "child1_x: {child1_x}");
        assert!((child1_y - 24.2).abs() <= 0.01, "child1_y: {child1_y}");

        assert!(
            (scene.view_box.x - 7.0).abs() <= 0.01,
            "vx: {}",
            scene.view_box.x
        );
        assert!(
            (scene.view_box.y - 7.0).abs() <= 0.01,
            "vy: {}",
            scene.view_box.y
        );
        assert!(
            (scene.view_box.width - 175.09447644360844).abs() <= 0.01,
            "vw: {}",
            scene.view_box.width
        );
        assert!(
            (scene.view_box.height - 34.4).abs() <= 0.01,
            "vh: {}",
            scene.view_box.height
        );
    }
}
