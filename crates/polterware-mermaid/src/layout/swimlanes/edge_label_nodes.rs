//! Port of `edgeLabelNodes.ts`: a labelled edge gets a label node that takes part in the
//! Sugiyama layout, while the labelled edge itself stays and is stamped with `labelNodeId`.
//!
//! Two `isLayoutOnly` virtual edges (`A -> label`, `label -> B`) are appended so layering and
//! ordering put the label between its ends. They are never routed or rendered.

use crate::render::pipeline::types::{Edge, LayoutData, Node};

use super::helpers::{node_is_dummy, node_is_edge_label};

pub fn create_edge_label_nodes(data: &mut LayoutData) -> LayoutData {
    let mut nodes_to_add: Vec<Node> = Vec::new();
    let mut layout_only_edges: Vec<Edge> = Vec::new();

    let node_by_id: std::collections::HashMap<String, Node> = data
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node.clone()))
        .collect();

    for edge in &mut data.edges {
        let Some(label) = edge.label.clone().filter(|label| !label.is_empty()) else {
            continue;
        };
        if edge.is_layout_only.unwrap_or(false) {
            continue;
        }
        if edge.label_node_id.is_some() {
            continue;
        }

        let source = edge.start.as_deref().and_then(|id| node_by_id.get(id));
        let target = edge.end.as_deref().and_then(|id| node_by_id.get(id));
        let (Some(source), Some(target)) = (source, target) else {
            continue;
        };

        let label_node_id = format!(
            "edge-label-{}-{}-{}",
            edge.start.as_deref().unwrap_or(""),
            edge.end.as_deref().unwrap_or(""),
            edge.id
        );

        // For cross-lane edges the label goes to the target lane, which keeps it closer to
        // where the edge is heading.
        let is_cross_lane = source.parent_id != target.parent_id;
        let label_lane = if is_cross_lane {
            target.parent_id.clone()
        } else {
            source.parent_id.clone()
        };

        let label_node = Node {
            id: label_node_id.clone(),
            label: Some(label),
            edge_start: edge.start.clone(),
            edge_end: edge.end.clone(),
            shape: Some("labelRect".to_string()),
            width: Some(0.0),
            height: Some(0.0),
            is_edge_label: Some(true),
            is_dummy: Some(true),
            parent_id: label_lane,
            is_group: Some(false),
            label_style: edge
                .label_style
                .as_ref()
                .and_then(|styles| styles.first())
                .cloned(),
            dir: source.dir.clone(),
            ..Default::default()
        };
        nodes_to_add.push(label_node);

        edge.label_node_id = Some(label_node_id.clone());
        edge.label = None;
        edge.text = None;

        layout_only_edges.push(Edge {
            id: format!("{}-to-label", edge.id),
            start: edge.start.clone(),
            end: Some(label_node_id.clone()),
            r#type: Some("normal".to_string()),
            is_layout_only: Some(true),
            ..Default::default()
        });
        layout_only_edges.push(Edge {
            id: format!("{}-from-label", edge.id),
            start: Some(label_node_id),
            end: edge.end.clone(),
            r#type: Some("normal".to_string()),
            is_layout_only: Some(true),
            ..Default::default()
        });
    }

    let mut result = data.clone();
    result.nodes.extend(nodes_to_add);
    result.edges.extend(layout_only_edges);
    // Keep the helpers referenced so the module knows the flags it stamps.
    debug_assert!(
        result
            .nodes
            .iter()
            .all(|node| !node_is_edge_label(node) || node_is_dummy(node))
    );
    result
}
