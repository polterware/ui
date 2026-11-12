//! Port of `index.ts`: the swimlanes arm of `createCommonLayoutRenderer`.
//!
//! Upstream builds it with `createCommonLayoutRenderer({ prepareLayout,
//! runLayoutCore, afterPaint })`, so a swimlanes render is one pass of: prepare the layout
//! data, measure its elements, run [`super::layout_core::run_swimlane_layout_core`], paint
//! the scene, and let `afterPaint` rewrite the routed paths with their line hops. This
//! module keeps that order. The paint step mirrors `common/index.ts` `paintLayoutData`:
//! lanes are clusters, every other node is positioned from the layout, and every edge is
//! painted from its routed polyline. `afterPaint` is upstream's `applySwimlaneLineJumps`,
//! which rewrites the painted `d` once the geometry is final; here the rewrite happens on
//! the scene paths this pass produces, because the layout data is the only geometry source.
//!
//! The engine differences are the ones the shared scene documents: HTML labels are plain text
//! runs, and the line hops are applied to the scene paths this pass builds rather than to DOM
//! attributes.

use crate::measure::{Font, TextMeasure, TextOptions, shape_text};
use crate::render::clusters::{ClusterNode as RenderClusterNode, render_cluster_measured};
use crate::render::edges::{EdgeData, insert_edge_with_type};
use crate::render::markers::marker_definitions;
use crate::render::pipeline::create_graph::create_graph_with_elements;
use crate::render::pipeline::dagre::svg_path_bounds;
use crate::render::pipeline::setup_view_port_for_svg::setup_view_port_for_svg;
use crate::render::pipeline::types::{CreateGraphOptions, LayoutData, Node};
use crate::render::shapes::position_node;
use crate::scene::{Path, PathShape, Rect, Scene};
use crate::theme::{Theme, ThemeVariables};

use super::SwimlaneError;
use super::adjust_layout::apply_swimlane_line_jumps;
use super::edge_label_nodes::create_edge_label_nodes;
use super::helpers::prepare_layout_for_swimlanes;
use super::layout_core::run_swimlane_layout_core;

/// `prepareSwimlaneLayout`: mark the lanes and turn every labelled edge into a label node
/// before the Sugiyama phases read the graph.
pub fn prepare_swimlane_layout(data: &mut LayoutData) {
    prepare_layout_for_swimlanes(data);

    let transformed = create_edge_label_nodes(data);
    data.nodes = transformed.nodes;
    data.edges = transformed.edges;
}

/// `createCommonLayoutRenderer`'s swimlanes arm: prepare, measure, lay out, paint, then
/// apply the line hops.
pub fn render(data: &mut LayoutData, measure: &dyn TextMeasure) -> Result<Scene, SwimlaneError> {
    prepare_swimlane_layout(data);

    let theme_vars = theme_variables();
    let mut created =
        create_graph_with_elements(data, &CreateGraphOptions::default(), measure, &theme_vars)
            .map_err(|error| SwimlaneError::Layout(error.to_string()))?;
    measure_edge_label_nodes(data, measure);

    run_swimlane_layout_core(data)?;

    let jump_paths: std::collections::HashMap<String, Path> =
        apply_swimlane_line_jumps(data)?.into_iter().collect();

    let diagram_id = data
        .diagram_id
        .clone()
        .unwrap_or_else(|| "diag".to_string());
    let diagram_type = data
        .r#type
        .clone()
        .unwrap_or_else(|| "swimlane".to_string());
    let mut scene = Scene::default();

    let markers: Vec<&str> = data.markers.iter().map(String::as_str).collect();
    for def in marker_definitions(&diagram_id, &diagram_type, &markers) {
        scene.define(def);
    }

    // Lanes are clusters; they carry the title band the swimlane shape paints.
    for node in &data.nodes {
        if node.is_group != Some(true) {
            continue;
        }
        let mut cluster = RenderClusterNode::new(
            &node.id,
            node.x.unwrap_or(0.0),
            node.y.unwrap_or(0.0),
            node.width.unwrap_or(0.0),
            node.height.unwrap_or(0.0),
        );
        cluster.label = node.label.clone().unwrap_or_default();
        cluster.padding = node.padding.unwrap_or(0.0);
        cluster.rx = node.rx.unwrap_or(0.0);
        cluster.ry = node.ry.unwrap_or(0.0);
        cluster.label_width = node.label_b_box.map(|bbox| bbox.width).unwrap_or(0.0);
        cluster.label_height = node.label_b_box.map(|bbox| bbox.height).unwrap_or(0.0);
        cluster.shape = node.shape.clone().unwrap_or_else(|| "rect".to_string());
        cluster.classes = node.css_classes.clone().unwrap_or_default();
        cluster.look = node.look.clone().unwrap_or_else(|| "classic".to_string());
        cluster.color_index = node.color_index;
        cluster.direction = node.dir.clone().unwrap_or_else(|| "TB".to_string());
        cluster.boundary_type = node
            .boundary_type
            .clone()
            .unwrap_or_else(|| "rect".to_string());
        cluster.content_top = metadata_number(node, "swimlaneContentTop");
        render_cluster_measured(&cluster, &mut scene, "#ECECFF", "#9370DB", measure);
    }

    for node in &data.nodes {
        if node.is_group == Some(true) {
            continue;
        }
        let Some(mut group) = created.node_elements.remove(&node.id) else {
            continue;
        };
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

    let mut path_bounds: Option<Rect> = None;
    for edge in &data.edges {
        if edge.is_layout_only.unwrap_or(false) {
            continue;
        }
        let Some(points) = edge.points.clone().filter(|points| points.len() >= 2) else {
            continue;
        };
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
            layout: Some("swimlane".to_string()),
            ..Default::default()
        };
        let Ok(inserted) = insert_edge_with_type(&edge_data, &diagram_id, &diagram_type) else {
            continue;
        };
        let path = jump_paths
            .get(&edge.id)
            .cloned()
            .unwrap_or(inserted.path.clone());
        if let Some(bounds) = svg_path_bounds(&path) {
            path_bounds = Some(path_bounds.map_or(bounds, |acc| acc.union(bounds)));
        }
        scene.push(PathShape {
            path,
            ..Default::default()
        });
    }

    let bounds = layout_bounds(data, path_bounds);
    scene.bounds = bounds;
    let padding = data.padding.unwrap_or(8.0);
    let use_max_width = data.use_max_width.unwrap_or(false);
    setup_view_port_for_svg(&mut scene, padding, "swimlane", use_max_width);

    Ok(scene)
}

fn theme_variables() -> ThemeVariables {
    let mut overrides = serde_json::Map::new();
    overrides.insert("fontFamily".into(), serde_json::json!("monospace"));
    overrides.insert("fontSize".into(), serde_json::json!("16px"));
    Theme::Default.variables(&overrides).unwrap_or_default()
}

/// `insertMeasuredNode` for the edge-label nodes: upstream measures the node group's
/// `getBBox()` after painting the `labelRect` shape, whose only other geometry is a 0.1 × 0.1
/// rect, so the box is the label's text block. A label node starts at width and height `0`
/// because that is upstream's "not measured yet" marker, so the generic measure step keeps it
/// as the shape bounds and the label would collapse to that rect.
fn measure_edge_label_nodes(data: &mut LayoutData, measure: &dyn TextMeasure) {
    let wrapping_width = data.config.flowchart.wrapping_width;
    let font = Font::new("monospace", 16.0);
    for node in data
        .nodes
        .iter_mut()
        .filter(|node| node.is_edge_label == Some(true))
    {
        let Some(label) = node.label.clone() else {
            continue;
        };
        let Ok(block) = shape_text(
            &label,
            &TextOptions {
                width: node.wrapping_width.unwrap_or(wrapping_width),
                markdown: node.label_type.as_deref() == Some("markdown"),
                min_width: 0.0,
            },
            &font,
            measure,
        ) else {
            continue;
        };
        node.width = Some(block.size.width);
        node.height = Some(block.size.height);
    }
}

/// `metadata.swimlaneContentTop`, the top of a lane's content band, which the title band ends
/// at.
fn metadata_number(node: &Node, key: &str) -> Option<f64> {
    node.metadata
        .as_ref()
        .and_then(|metadata| metadata.get(key))
        .and_then(serde_json::Value::as_f64)
}

/// The box `getBBox()` reports for everything the layout painted: the node boxes (lane
/// clusters included), the label nodes, and the edge paths. The fitted viewBox grows it by
/// the diagram padding.
fn layout_bounds(data: &LayoutData, path_bounds: Option<Rect>) -> Rect {
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for node in &data.nodes {
        let (Some(x), Some(y), Some(width), Some(height)) =
            (node.x, node.y, node.width, node.height)
        else {
            continue;
        };
        min_x = min_x.min(x - width / 2.0);
        max_x = max_x.max(x + width / 2.0);
        min_y = min_y.min(y - height / 2.0);
        max_y = max_y.max(y + height / 2.0);
    }

    if let Some(bounds) = path_bounds {
        min_x = min_x.min(bounds.x);
        max_x = max_x.max(bounds.x + bounds.width);
        min_y = min_y.min(bounds.y);
        max_y = max_y.max(bounds.y + bounds.height);
    }

    if min_x <= max_x && min_y <= max_y {
        Rect::new(min_x, min_y, max_x - min_x, max_y - min_y)
    } else {
        Rect::ZERO
    }
}
