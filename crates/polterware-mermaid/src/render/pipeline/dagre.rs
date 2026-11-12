//! Port of upstream `rendering-util/layout-algorithms/dagre/index.js`.
//! Reuses `crate::render::layout_dagre` rather than duplicating it.

use std::collections::HashMap;

use crate::layout::dagre::{EdgeLabel, LayeredGraph, NodeLabel, Point};
use crate::layout::graphlib::{Edge as GraphEdge, GraphOptions};
use crate::measure::TextMeasure;
use crate::render::clusters::{ClusterNode as RenderClusterNode, render_cluster_measured};
use crate::render::edges::{EdgeData, insert_edge_with_type};
use crate::render::layout_dagre::{
    ClusterIndex, DagreConfig, LayoutError, SelfLoopSegment, adjust_clusters_and_edges,
    configure_graph, run_prepared_layout, sort_nodes_by_hierarchy,
};
use crate::render::markers::marker_definitions;
use crate::render::shapes::{ShapeError, position_node};
use crate::scene::{PathShape, Rect, Scene};
use crate::theme::{Theme, ThemeVariables};

use super::create_graph::{CreatedGraph, create_graph_with_elements};
use super::setup_view_port_for_svg::setup_view_port_for_svg;
use super::types::{CreateGraphOptions, LayoutData, Node};

pub fn get_default_self_loop_side(rankdir: Option<&str>) -> &'static str {
    match rankdir.unwrap_or("TB") {
        "BT" => "bottom",
        "LR" => "right",
        "RL" => "left",
        _ => "top",
    }
}

pub fn should_merge_self_loop_segments(diagram_type: Option<&str>) -> bool {
    matches!(
        diagram_type.unwrap_or("flowchart"),
        "flowchart" | "flowchart-v2" | "stateDiagram" | "er" | "classDiagram"
    )
}

pub fn get_self_loop_side(
    _graph: &LayeredGraph,
    _node: &Node,
    _rankdir: Option<&str>,
) -> &'static str {
    get_default_self_loop_side(_rankdir)
}

pub fn get_self_loop_points(
    node: &Node,
    side: &str,
    y_offset: f64,
    label_width: f64,
) -> Vec<Point> {
    let x = node.x.unwrap_or(0.0);
    let y = node.y.unwrap_or(0.0) - y_offset;
    let width = node.width.unwrap_or(0.0);
    let height = node.height.unwrap_or(0.0);
    let half_width = width / 2.0;
    let half_height = height / 2.0;
    let max_span = (width * 0.8).clamp(36.0, 100.0);
    let span = (label_width.max(width * 0.35)).clamp(36.0, max_span);
    let depth = (width.min(height) * 0.45).clamp(24.0, 48.0);

    match side {
        "bottom" => {
            let bottom = y + half_height;
            vec![
                Point::new(x - span / 2.0, bottom),
                Point::new(x - span / 2.0, bottom + depth),
                Point::new(x + span / 2.0, bottom + depth),
                Point::new(x + span / 2.0, bottom),
            ]
        }
        "right" => {
            let right = x + half_width;
            vec![
                Point::new(right, y - span / 2.0),
                Point::new(right + depth, y - span / 2.0),
                Point::new(right + depth, y + span / 2.0),
                Point::new(right, y + span / 2.0),
            ]
        }
        "left" => {
            let left = x - half_width;
            vec![
                Point::new(left, y - span / 2.0),
                Point::new(left - depth, y - span / 2.0),
                Point::new(left - depth, y + span / 2.0),
                Point::new(left, y + span / 2.0),
            ]
        }
        _ => {
            let top = y - half_height;
            vec![
                Point::new(x - span / 2.0, top),
                Point::new(x - span / 2.0, top - depth),
                Point::new(x + span / 2.0, top - depth),
                Point::new(x + span / 2.0, top),
            ]
        }
    }
}

pub fn get_self_loop_label_position(
    node: &Node,
    points: &[Point],
    side: &str,
    y_offset: f64,
    label_width: f64,
    label_height: f64,
) -> Point {
    let gap = 4.0;
    let x = node.x.unwrap_or(0.0);
    let y = node.y.unwrap_or(0.0) - y_offset;
    match side {
        "bottom" => {
            let max_y = points.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
            Point::new(x, max_y + label_height / 2.0 + gap)
        }
        "right" => {
            let max_x = points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
            Point::new(max_x + label_width / 2.0 + gap, y)
        }
        "left" => {
            let min_x = points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
            Point::new(min_x - label_width / 2.0 - gap, y)
        }
        _ => {
            let min_y = points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
            Point::new(x, min_y - label_height / 2.0 - gap)
        }
    }
}

pub struct DagrePreparedLayout {
    pub graph: LayeredGraph,
    pub cluster_index: ClusterIndex,
    pub self_loops: Vec<SelfLoopSegment>,
}

pub fn prepare_layout_for_dagre(data4_layout: &LayoutData) -> DagrePreparedLayout {
    let mut graph = LayeredGraph::with_options(GraphOptions {
        multigraph: true,
        compound: true,
        ..GraphOptions::default()
    });

    let config = DagreConfig {
        node_spacing: data4_layout
            .node_spacing
            .or(Some(data4_layout.config.flowchart.node_spacing)),
        rank_spacing: data4_layout
            .rank_spacing
            .or(Some(data4_layout.config.flowchart.rank_spacing)),
        margin_x: 8.0,
        margin_y: 8.0,
        ..Default::default()
    };

    let dir = data4_layout.direction.as_deref().unwrap_or("TB");
    configure_graph(&mut graph, dir, &config);

    for node in &data4_layout.nodes {
        let mut nl = NodeLabel::default();
        nl.width = node.width.unwrap_or(0.0);
        nl.height = node.height.unwrap_or(0.0);
        graph.set_node_with(&node.id, nl);
        if let Some(parent_id) = &node.parent_id {
            let _ = graph.set_parent(&node.id, Some(parent_id.as_str()));
        }
    }

    let mut self_loops = Vec::new();
    for edge in &data4_layout.edges {
        let Some(start) = &edge.start else { continue };
        let Some(end) = &edge.end else { continue };
        if start == end {
            let label = EdgeLabel {
                width: edge.width,
                height: edge.height,
                ..Default::default()
            };
            let segments =
                crate::render::layout_dagre::add_self_loop(&mut graph, start, &edge.id, &label);
            self_loops.extend(segments);
        } else {
            let label = EdgeLabel {
                width: edge.width,
                height: edge.height,
                ..Default::default()
            };
            let _ = graph.set_edge_full(start, end, Some(label), Some(&edge.id));
        }
    }

    let cluster_index = adjust_clusters_and_edges(&mut graph);

    DagrePreparedLayout {
        graph,
        cluster_index,
        self_loops,
    }
}

pub struct DagreMeasuredLayout {
    pub graph: LayeredGraph,
    pub cluster_index: ClusterIndex,
    pub self_loops: Vec<SelfLoopSegment>,
    pub created: CreatedGraph,
    pub sub_graph_title_total_margin: f64,
    pub merge_self_loops: bool,
}

pub fn measure_dagre_graph(
    data4_layout: &mut LayoutData,
    mut prepared: DagrePreparedLayout,
    measure: &dyn TextMeasure,
    theme: &ThemeVariables,
) -> Result<DagreMeasuredLayout, ShapeError> {
    let created =
        create_graph_with_elements(data4_layout, &CreateGraphOptions::default(), measure, theme)?;

    // Sync measured node sizes back into the prepared dagre graph
    for node in &data4_layout.nodes {
        if let Some(n) = prepared.graph.node_mut(&node.id) {
            n.width = node.width.unwrap_or(0.0);
            n.height = node.height.unwrap_or(0.0);
        }
        for sub in prepared.cluster_index.extracted.values_mut() {
            if let Some(n) = sub.node_mut(&node.id) {
                n.width = node.width.unwrap_or(0.0);
                n.height = node.height.unwrap_or(0.0);
            }
        }
    }

    // Sync edge label sizes back into the prepared dagre graph
    for edge in &data4_layout.edges {
        if let (Some(start), Some(end)) = (&edge.start, &edge.end)
            && let Some(e) = prepared
                .graph
                .edge_object_mut(&GraphEdge::named(start, end, &edge.id))
        {
            e.width = edge.width;
            e.height = edge.height;
        }
    }

    let top = data4_layout.config.flowchart.sub_graph_title_margin.top;
    let bottom = data4_layout.config.flowchart.sub_graph_title_margin.bottom;
    let sub_graph_title_total_margin = top + bottom;
    let merge_self_loops = should_merge_self_loop_segments(data4_layout.r#type.as_deref());

    Ok(DagreMeasuredLayout {
        graph: prepared.graph,
        cluster_index: prepared.cluster_index,
        self_loops: prepared.self_loops,
        created,
        sub_graph_title_total_margin,
        merge_self_loops,
    })
}

pub fn run_dagre_graph_layout(
    graph: &mut LayeredGraph,
    cluster_index: &mut ClusterIndex,
) -> Result<(), LayoutError> {
    run_prepared_layout(graph, cluster_index)
}

pub fn normalize_dagre_node(
    graph: &LayeredGraph,
    node_id: &str,
    sub_graph_title_total_margin: f64,
) -> Option<NodeLabel> {
    let node = graph.node(node_id)?;
    let mut normalized = node.clone();
    if !graph.children(node_id).is_empty() {
        normalized.height += sub_graph_title_total_margin;
    } else if let Some(y) = normalized.y {
        normalized.y = Some(y + sub_graph_title_total_margin / 2.0);
    }
    Some(normalized)
}

pub fn apply_dagre_node_layout(target: &mut Node, dagre_node: &NodeLabel) {
    if let Some(x) = dagre_node.x {
        target.x = Some(x);
    }
    if let Some(y) = dagre_node.y {
        target.y = Some(y);
    }
    target.width = Some(dagre_node.width);
    target.height = Some(dagre_node.height);
}

pub fn apply_dagre_layout_result(
    data4_layout: &mut LayoutData,
    measured: &mut DagreMeasuredLayout,
) {
    let sorted_ids = sort_nodes_by_hierarchy(&measured.graph);
    let node_map: HashMap<String, usize> = data4_layout
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.clone(), i))
        .collect();

    for node_id in sorted_ids {
        if let Some(dagre_node) = normalize_dagre_node(
            &measured.graph,
            &node_id,
            measured.sub_graph_title_total_margin,
        ) && let Some(&idx) = node_map.get(&node_id)
        {
            apply_dagre_node_layout(&mut data4_layout.nodes[idx], &dagre_node);
        }
    }

    // Also check extracted clusters for positions
    for (cid, subgraph) in &measured.cluster_index.extracted {
        for nid in subgraph.nodes() {
            if let Some(n) = subgraph.node(&nid)
                && let Some(&idx) = node_map.get(&nid)
                && data4_layout.nodes[idx].x.is_none()
            {
                data4_layout.nodes[idx].x = n.x;
                data4_layout.nodes[idx].y = n.y;
                data4_layout.nodes[idx].width = Some(n.width);
                data4_layout.nodes[idx].height = Some(n.height);
            }
        }
        if let Some(&idx) = node_map.get(cid)
            && let Some(cn) = measured.graph.node(cid)
        {
            data4_layout.nodes[idx].x = cn.x;
            data4_layout.nodes[idx].y = cn.y;
            data4_layout.nodes[idx].width = Some(cn.width);
            data4_layout.nodes[idx].height = Some(cn.height);
        }
    }

    // Render edges
    let edge_offset_y = measured.sub_graph_title_total_margin / 2.0;
    let render_edges = crate::render::layout_dagre::get_render_edges(
        &measured.graph,
        &measured.cluster_index,
        &measured.self_loops,
        edge_offset_y,
        measured.merge_self_loops,
    );

    let edge_by_id: HashMap<String, usize> = data4_layout
        .edges
        .iter()
        .enumerate()
        .map(|(i, e)| (e.id.clone(), i))
        .collect();

    for re in render_edges {
        let edge_id = re
            .edge
            .name
            .clone()
            .unwrap_or_else(|| format!("{}-{}", re.edge.v, re.edge.w));
        if let Some(&idx) = edge_by_id.get(&edge_id) {
            let target = &mut data4_layout.edges[idx];
            if let Some(ref pts) = re.label.points {
                target.points = Some(
                    pts.iter()
                        .map(|p| crate::scene::Point::new(p.x, p.y))
                        .collect(),
                );
            }
            target.x = re.label.x;
            target.y = re.label.y;
            target.width = Some(re.label.width());
            target.height = Some(re.label.height());
        }
    }
}

pub fn run_dagre_layout_core(
    data4_layout: &mut LayoutData,
    measured: &mut DagreMeasuredLayout,
) -> Result<(), LayoutError> {
    run_dagre_graph_layout(&mut measured.graph, &mut measured.cluster_index)?;
    apply_dagre_layout_result(data4_layout, measured);
    Ok(())
}

pub fn dagre_render(
    data4_layout: &mut LayoutData,
    measure: &dyn TextMeasure,
) -> Result<Scene, LayoutError> {
    let mut overrides = serde_json::Map::new();
    overrides.insert("fontFamily".into(), serde_json::json!("monospace"));
    overrides.insert("fontSize".into(), serde_json::json!("16px"));
    let theme_vars = Theme::Default.variables(&overrides).unwrap_or_default();

    let prepared = prepare_layout_for_dagre(data4_layout);
    let mut measured = measure_dagre_graph(data4_layout, prepared, measure, &theme_vars)
        .map_err(|e: ShapeError| LayoutError::Dagre(e.to_string()))?;

    run_dagre_layout_core(data4_layout, &mut measured)?;

    // Paint into Scene
    let mut scene = Scene::default();
    let diagram_id = data4_layout
        .diagram_id
        .clone()
        .unwrap_or_else(|| "diag".to_string());
    let diagram_type = data4_layout
        .r#type
        .clone()
        .unwrap_or_else(|| "flowchart-v2".to_string());

    // Register marker defs
    let markers_ref: Vec<&str> = data4_layout.markers.iter().map(|s| s.as_str()).collect();
    let defs = marker_definitions(&diagram_id, &diagram_type, &markers_ref);
    for def in defs {
        scene.define(def);
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

    // Paint nodes
    for node in &data4_layout.nodes {
        if node.is_group != Some(true)
            && let Some(mut group) = measured.created.node_elements.remove(&node.id)
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

    // Paint edges
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
