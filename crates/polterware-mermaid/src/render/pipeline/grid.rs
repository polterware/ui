//! Grid layout in the unified render pipeline.
//!
//! Upstream registers a layout as a name plus a lazy loader in
//! `rendering-util/render.ts`'s `registerDefaultLayoutLoaders`; the guide that ships with the
//! grid design (`docs/community/layout-makers-guide.md`) uses exactly `{ name: 'grid', loader }
//! with `createCommonLayoutRenderer({ prepareLayout: prepareGridLayout, runLayoutCore:
//! runGridLayoutCore })`. This module is that registration for the Rust pipeline: flowcharts with
//! `config: layout: grid` resolve to [`grid_render`], whose order is `prepareGridLayout` before
//! measurement and `runGridLayoutCore` after it.
//!
//! The source is Mermaid 12.3.0 (MIT, Copyright (c) 2014 - 2022 Knut Sveidqvist).

use std::collections::HashMap;

use crate::layout::grid::edge_labels::prepare_grid_layout;
use crate::layout::grid::layout_core::{GridLayoutCoreInternalOptions, run_grid_layout_core};
use crate::layout::grid::types::{
    Edge as GridEdge, GridConfig, GridCurve, GroupTitleRect as GridGroupTitleRect,
    LayoutData as GridLayoutData, Node as GridNode, PortClipping, RouterPoint,
};
use crate::measure::{Font, TextMeasure};
use crate::render::clusters::{ClusterNode as RenderClusterNode, render_cluster_measured};
use crate::render::edges::{EdgeData, insert_edge_with_type};
use crate::render::layout_dagre::LayoutError;
use crate::render::markers::marker_definitions;
use crate::render::pipeline::create_graph::{create_graph_with_elements, to_shape_node};
use crate::render::pipeline::setup_view_port_for_svg::setup_view_port_for_svg;
use crate::render::pipeline::types::{CreateGraphOptions, LayoutData, Node};
use crate::render::shapes::{ShapeError, position_node, render_node};
use crate::scene::{PathShape, Rect, Scene};
use crate::theme::Theme;

/// The mapping from the pipeline's graph to the grid port's graph.
pub fn grid_layout_data(data: &LayoutData) -> GridLayoutData {
    let nodes = data
        .nodes
        .iter()
        .map(|node| GridNode {
            id: node.id.clone(),
            label: node.label.clone(),
            parent_id: node.parent_id.clone(),
            is_group: node.is_group == Some(true),
            is_edge_label: node.is_edge_label == Some(true),
            width: node.width,
            height: node.height,
            x: node.x,
            y: node.y,
            label_bbox: node
                .label_b_box
                .map(|bbox| crate::layout::grid::types::LabelSize {
                    width: bbox.width,
                    height: bbox.height,
                }),
            group_title_rect: node.group_title_rect.map(|rect| GridGroupTitleRect {
                left: rect.left,
                right: rect.right,
                top: rect.top,
                bottom: rect.bottom,
            }),
            metadata: node.metadata.clone(),
            placement_id: None,
            padding: node.padding,
            shape: node.shape.clone(),
            is_dummy: node.is_dummy == Some(true),
            label_style: node.label_style.clone(),
            edge_start: node.edge_start.clone(),
            edge_end: node.edge_end.clone(),
            dir: node.dir.clone(),
        })
        .collect();

    let edges = data
        .edges
        .iter()
        .map(|edge| GridEdge {
            id: edge.id.clone(),
            label: edge.label.clone(),
            start: edge.start.clone(),
            end: edge.end.clone(),
            label_node_id: edge.label_node_id.clone(),
            points: edge
                .points
                .as_ref()
                .map(|points| {
                    points
                        .iter()
                        .map(|point| RouterPoint::new(point.x, point.y))
                        .collect()
                })
                .unwrap_or_default(),
            curve: edge.curve.as_deref().and_then(GridCurve::parse),
            corner_radius: edge.corner_radius,
            port_clipping: edge.port_clipping.as_deref().and_then(|value| {
                (value == "outline-orthogonal").then_some(PortClipping::OutlineOrthogonal)
            }),
            skip_corner_fix: edge.skip_corner_fix == Some(true),
            arrow_type_start: edge.arrow_type_start.clone(),
            arrow_type_end: edge.arrow_type_end.clone(),
            edge_type: edge.r#type.clone(),
            label_style: edge.label_style.clone(),
        })
        .collect();

    GridLayoutData {
        nodes,
        edges,
        config: GridConfig {
            grid: data.config.grid.clone(),
        },
    }
}

/// The measured label box `createGraphWithElements` assigns a `labelRect` helper: the shaped
/// text with no extra padding, which is what the grid fixtures show.
fn measured_edge_label(
    edge: &crate::render::pipeline::types::Edge,
    measure: &dyn TextMeasure,
    font: &Font,
) -> Option<(f64, f64)> {
    let label = edge.label.as_ref().filter(|label| !label.is_empty())?;
    let block = crate::measure::shape_text(
        label,
        &crate::measure::TextOptions {
            width: f64::INFINITY,
            markdown: edge.label_type.as_deref() == Some("markdown"),
            min_width: 0.0,
        },
        font,
        measure,
    )
    .ok()?;
    Some((block.size.width, block.size.height))
}

/// Runs the grid algorithm on the pipeline graph and paints the shared scene.
pub fn grid_render(
    data4_layout: &mut LayoutData,
    measure: &dyn TextMeasure,
) -> Result<Scene, LayoutError> {
    let mut overrides = serde_json::Map::new();
    overrides.insert("fontFamily".into(), serde_json::json!("monospace"));
    overrides.insert("fontSize".into(), serde_json::json!("16px"));
    let theme_vars = Theme::Default.variables(&overrides).unwrap_or_default();
    let font = Font::new("monospace", 16.0);

    // `measureLayout` runs before the core: it sizes real nodes and makes the edge text, which
    // `prepareGridLayout` turns into helper nodes.
    let mut created = create_graph_with_elements(
        data4_layout,
        &CreateGraphOptions::default(),
        measure,
        &theme_vars,
    )
    .map_err(|error| LayoutError::Dagre(format!("Measure failed: {error}")))?;

    let measured_labels: Vec<Option<(f64, f64)>> = data4_layout
        .edges
        .iter()
        .map(|edge| measured_edge_label(edge, measure, &font))
        .collect();

    let mut grid = grid_layout_data(data4_layout);
    prepare_grid_layout(&mut grid);
    for (edge_index, edge) in grid.edges.iter().enumerate() {
        let Some(label_node_id) = edge.label_node_id.as_deref() else {
            continue;
        };
        let Some((width, height)) = measured_labels[edge_index] else {
            continue;
        };
        if let Some(node) = grid.nodes.iter_mut().find(|node| node.id == label_node_id) {
            node.width = Some(width);
            node.height = Some(height);
        }
    }

    let result = run_grid_layout_core(&mut grid, GridLayoutCoreInternalOptions::default())
        .map_err(|error| LayoutError::Dagre(error.to_string()))?;
    let _ = result;

    let grid_node_by_id: HashMap<&str, &GridNode> = grid
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let grid_edge_by_id: HashMap<&str, &GridEdge> = grid
        .edges
        .iter()
        .map(|edge| (edge.id.as_str(), edge))
        .collect();

    // Real nodes keep the helper's measured box; only the geometry the core owns moves.
    for node in &mut data4_layout.nodes {
        if let Some(grid_node) = grid_node_by_id.get(node.id.as_str()) {
            node.x = grid_node.x;
            node.y = grid_node.y;
            node.width = grid_node.width;
            node.height = grid_node.height;
            if node.is_group == Some(true) {
                node.rx.get_or_insert(0.0);
            }
        }
    }

    // Label helpers are nodes in the render model, exactly as upstream's `prepareGridLayout`
    // leaves them.
    let label_type_by_helper: HashMap<&str, &str> = data4_layout
        .edges
        .iter()
        .filter_map(|edge| {
            Some((
                edge.label_node_id.as_deref()?,
                edge.label_type.as_deref().unwrap_or(""),
            ))
        })
        .collect();
    let mut helper_nodes: Vec<Node> = Vec::new();
    for grid_node in grid.nodes.iter().filter(|node| node.is_edge_label) {
        helper_nodes.push(Node {
            id: grid_node.id.clone(),
            label: grid_node.label.clone(),
            label_type: label_type_by_helper
                .get(grid_node.id.as_str())
                .filter(|label_type| !label_type.is_empty())
                .map(|label_type| label_type.to_string()),
            shape: grid_node
                .shape
                .clone()
                .or_else(|| Some("labelRect".to_string())),
            width: grid_node.width,
            height: grid_node.height,
            x: grid_node.x,
            y: grid_node.y,
            is_edge_label: Some(true),
            is_dummy: Some(true),
            is_group: Some(false),
            edge_start: grid_node.edge_start.clone(),
            edge_end: grid_node.edge_end.clone(),
            label_style: grid_node.label_style.clone(),
            dir: grid_node.dir.clone(),
            ..Node::default()
        });
    }

    for edge in &mut data4_layout.edges {
        if let Some(grid_edge) = grid_edge_by_id.get(edge.id.as_str()) {
            if !grid_edge.points.is_empty() {
                edge.points = Some(
                    grid_edge
                        .points
                        .iter()
                        .map(|point| crate::scene::Point::new(point.x, point.y))
                        .collect(),
                );
            }
            edge.label_node_id = grid_edge.label_node_id.clone();
            edge.curve = grid_edge
                .curve
                .map(|curve| curve.as_str().to_string())
                .or(edge.curve.take());
            edge.corner_radius = grid_edge.corner_radius.or(edge.corner_radius);
            edge.port_clipping = grid_edge
                .port_clipping
                .map(|_| "outline-orthogonal".to_string())
                .or(edge.port_clipping.take());
            edge.skip_corner_fix = Some(grid_edge.skip_corner_fix);
            // The helper node draws the text now.
            edge.label = None;
        }
        let Some(label_node_id) = edge.label_node_id.as_deref() else {
            continue;
        };
        let Some(helper) = helper_nodes.iter().find(|node| node.id == label_node_id) else {
            continue;
        };
        edge.x = helper.x;
        edge.y = helper.y;
        edge.width = helper.width;
        edge.height = helper.height;
    }

    for helper in helper_nodes {
        // `insertMeasuredNode` would replace the helper's label box with the `labelRect` path's
        // 0.1 × 0.1 bounds; the grid layout needs the text box the edge label was measured at.
        let rendered = render_node(to_shape_node(&helper), measure, &theme_vars)
            .map_err(|error: ShapeError| LayoutError::Dagre(error.to_string()))?;
        created
            .node_elements
            .insert(helper.id.clone(), rendered.group);
        data4_layout.nodes.push(helper);
    }

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

    // Paint clusters.
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
            cn.shape = node.shape.clone().unwrap_or_else(|| "rect".to_string());
            cn.classes = node.css_classes.clone().unwrap_or_default();
            cn.look = node.look.clone().unwrap_or_else(|| "classic".to_string());
            cn.color_index = node.color_index;
            cn.direction = node.dir.clone().unwrap_or_else(|| "TB".to_string());
            cn.boundary_type = node
                .boundary_type
                .clone()
                .unwrap_or_else(|| "rect".to_string());
            render_cluster_measured(&cn, &mut scene, "#ECECFF", "#9370DB", measure);
        }
    }

    // Paint nodes, label helpers included.
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

    // Paint edges from the routes the label transaction committed.
    let mut path_bounds: Option<Rect> = None;
    for edge in &data4_layout.edges {
        if edge.is_layout_only == Some(true) {
            continue;
        }
        let edge_data = EdgeData {
            id: edge.id.clone(),
            points: edge.points.clone().unwrap_or_default(),
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
            if let Some(points) = edge.points.as_ref()
                && let (Some(first), Some(last)) = (points.first(), points.last())
            {
                // The jsdom SVG bbox is built from the path commands, so a rounded corner's
                // control points, not the curve extrema, are the box the viewport fits.
                let mut bounds = Rect::new(first.x.min(last.x), first.y.min(last.y), 0.0, 0.0);
                for point in points {
                    let left = bounds.x.min(point.x);
                    let top = bounds.y.min(point.y);
                    let right = (bounds.x + bounds.width).max(point.x);
                    let bottom = (bounds.y + bounds.height).max(point.y);
                    bounds = Rect::new(left, top, right - left, bottom - top);
                }
                path_bounds =
                    Some(path_bounds.map_or(bounds, |accumulated| accumulated.union(bounds)));
            }
            scene.push(PathShape {
                path: inserted.path,
                ..Default::default()
            });
        }
    }

    // Bounds and viewBox: node boxes, group titles, helper label boxes and edge label boxes.
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

    if let Some(bounds) = path_bounds {
        min_x = min_x.min(bounds.x);
        max_x = max_x.max(bounds.x + bounds.width);
        min_y = min_y.min(bounds.y);
        max_y = max_y.max(bounds.y + bounds.height);
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

    #[test]
    fn grid_is_a_registered_layout_algorithm() {
        assert_eq!(
            crate::render::pipeline::get_registered_layout_algorithm("grid", None),
            "grid"
        );
        assert!(
            crate::render::pipeline::get_layout_loader_definition("grid").is_ok(),
            "grid loader is registered"
        );
    }

    #[test]
    fn maps_pipeline_config_grid_into_the_grid_config() {
        let mut data = LayoutData::default();
        data.config.grid = Some(serde_json::json!({ "columns": 3 }));
        let grid = grid_layout_data(&data);
        assert_eq!(grid.config.grid, Some(serde_json::json!({ "columns": 3 })));
    }
}

#[cfg(test)]
mod render_tests {
    use crate::config::Config;
    use crate::measure::FallbackMeasure;

    #[test]
    fn matches_the_upstream_grid_render_fixtures() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/layout/grid/testdata/render");
        let mut count = 0;
        for entry in std::fs::read_dir(&dir).expect("grid render fixtures") {
            let path = entry.expect("fixture entry").path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let fixture: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).expect("fixture file"))
                    .expect("fixture json");
            let body = fixture["body"].as_str().expect("fixture body");
            let ast = crate::diagrams::flowchart::parse(body).expect("parse fixture body");
            let mut config = Config::default();
            config.layout = "grid".to_string();
            config.flowchart.use_max_width = false;
            let (data, scene) = crate::diagrams::flowchart::render::draw_layout_ast(
                &ast,
                &config,
                &FallbackMeasure,
            )
            .expect("grid fixture renders");
            let name = fixture["name"].as_str().unwrap_or("fixture");

            let compare = |kind: &str,
                           id: &str,
                           actual: (Option<f64>, Option<f64>, Option<f64>, Option<f64>),
                           expected: &serde_json::Value| {
                for (label, actual_value, key) in [
                    ("x", actual.0, "x"),
                    ("y", actual.1, "y"),
                    ("width", actual.2, "width"),
                    ("height", actual.3, "height"),
                ] {
                    let expected_value = expected[key].as_f64();
                    match (actual_value, expected_value) {
                        (Some(actual_value), Some(expected_value)) => assert!(
                            (actual_value - expected_value).abs() <= 0.01,
                            "{name}: {kind} {id} {label}: actual {actual_value} vs expected {expected_value}"
                        ),
                        (None, None) => {}
                        _ => panic!("{name}: {kind} {id} {label} presence mismatch"),
                    }
                }
            };

            for expected_node in fixture["nodes"].as_array().expect("fixture nodes") {
                let id = expected_node["id"].as_str().expect("node id");
                let node = data
                    .nodes
                    .iter()
                    .find(|node| {
                        node.id == id
                            && node.is_group != Some(true)
                            && node.is_edge_label != Some(true)
                    })
                    .unwrap_or_else(|| panic!("{name}: node {id}"));
                compare(
                    "node",
                    id,
                    (node.x, node.y, node.width, node.height),
                    expected_node,
                );
            }
            for expected_cluster in fixture["clusters"].as_array().expect("fixture clusters") {
                let id = expected_cluster["id"].as_str().expect("cluster id");
                let node = data
                    .nodes
                    .iter()
                    .find(|node| node.id == id && node.is_group == Some(true))
                    .unwrap_or_else(|| panic!("{name}: cluster {id}"));
                compare(
                    "cluster",
                    id,
                    (node.x, node.y, node.width, node.height),
                    expected_cluster,
                );
            }
            for expected_label in fixture["labels"].as_array().expect("fixture labels") {
                let id = expected_label["id"].as_str().expect("label id");
                let node = data
                    .nodes
                    .iter()
                    .find(|node| node.id == id && node.is_edge_label == Some(true))
                    .unwrap_or_else(|| panic!("{name}: label {id}"));
                compare(
                    "label",
                    id,
                    (node.x, node.y, node.width, node.height),
                    expected_label,
                );
            }
            for expected_edge in fixture["edges"].as_array().expect("fixture edges") {
                let id = expected_edge["id"].as_str().expect("edge id");
                let edge = data
                    .edges
                    .iter()
                    .find(|edge| edge.id == id)
                    .unwrap_or_else(|| panic!("{name}: edge {id}"));
                let actual_points = edge.points.as_ref().expect("edge points");
                let expected_points = expected_edge["points"].as_array().expect("points");
                assert_eq!(
                    actual_points.len(),
                    expected_points.len(),
                    "{name}: edge {id} point count"
                );
                for (index, (actual, expected)) in
                    actual_points.iter().zip(expected_points.iter()).enumerate()
                {
                    let expected_x = expected["x"].as_f64().expect("point x");
                    let expected_y = expected["y"].as_f64().expect("point y");
                    assert!(
                        (actual.x - expected_x).abs() <= 0.01,
                        "{name}: edge {id} point {index} x: actual {} vs expected {expected_x}",
                        actual.x
                    );
                    assert!(
                        (actual.y - expected_y).abs() <= 0.01,
                        "{name}: edge {id} point {index} y: actual {} vs expected {expected_y}",
                        actual.y
                    );
                }
            }

            let view_box = &fixture["viewBox"];
            for (label, actual, key) in [
                ("x", scene.view_box.x, "x"),
                ("y", scene.view_box.y, "y"),
                ("width", scene.view_box.width, "width"),
                ("height", scene.view_box.height, "height"),
            ] {
                let expected = view_box[key].as_f64().expect("viewBox value");
                assert!(
                    (actual - expected).abs() <= 0.01,
                    "{name}: viewBox {label}: actual {actual} vs expected {expected}"
                );
            }

            count += 1;
        }
        assert!(
            count >= 8,
            "expected at least 8 grid fixtures, found {count}"
        );
    }

    #[test]
    fn renders_a_flowchart_through_the_grid_layout() {
        let mut config = Config::default();
        config.layout = "grid".to_string();
        let scene = crate::diagrams::flowchart::render::draw(
            "flowchart TD\n  A[Start] --> B[End]\n  B --> C[Third]\n",
            &config,
            &FallbackMeasure,
        )
        .expect("grid flowchart renders");
        assert!(scene.view_box.width > 0.0);
        assert!(scene.view_box.height > 0.0);
        assert!(!scene.items.is_empty());
    }
}
