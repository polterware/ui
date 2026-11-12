//! Port of Mermaid's fcose layout algorithm for the unified pipeline.
//!
//! Upstream registers the layout as `fcose` in `rendering-util/layout-algorithms/`, and
//! the architecture renderer (`diagrams/architecture/architectureRenderer.ts`) runs it
//! with `quality: 'proof'`, the configured `randomize`, `nodeSeparation`, `numIter`,
//! per-edge `idealEdgeLength` and `edgeElasticity`, and alignment and relative placement
//! constraints. This module exposes the layout to the pipeline; an architecture renderer
//! builds its [`FcoseOptions`] (including the constraint arrays) and calls
//! [`execute_fcose_layout`] or [`crate::layout::fcose::run_fcose_layout`] directly.
//!
//! Cose-bilkent's renderer paints a layout through the shared
//! [`render_layout_result`](super::cose_bilkent::render_layout_result), and so does this
//! one; they differ only in the layout that produces the positions and edges.

use crate::layout::fcose::{
    FcoseEdge, FcoseNode, FcoseOptions, FcosePosition, FcoseQuality, run_fcose_layout,
};
use crate::measure::TextMeasure;
use crate::render::layout_dagre::LayoutError;
use crate::scene::Scene;

use super::cose_bilkent::{LayoutResult, PositionedEdge, PositionedNode, render_layout_result};
use super::types::LayoutData;
use super::{RenderOptions, validate_layout_data};

/// The pipeline defaults, the options the architecture renderer starts from.
pub fn default_fcose_options() -> FcoseOptions {
    FcoseOptions {
        quality: FcoseQuality::Proof,
        animate: false,
        ..Default::default()
    }
}

/// The layout data as the fcose port's graph.
pub fn fcose_graph_nodes(data: &LayoutData) -> Vec<FcoseNode> {
    data.nodes
        .iter()
        .map(|node| FcoseNode {
            id: node.id.clone(),
            parent_id: node.parent_id.clone(),
            x: node.x.unwrap_or(0.0),
            y: node.y.unwrap_or(0.0),
            width: node.width.unwrap_or(40.0),
            height: node.height.unwrap_or(40.0),
            padding: node.padding.unwrap_or(0.0),
        })
        .collect()
}

/// The layout data's edges as the fcose port's graph, in layout-data order.
pub fn fcose_graph_edges(data: &LayoutData) -> Vec<FcoseEdge> {
    data.edges
        .iter()
        .filter_map(|edge| {
            Some(FcoseEdge {
                id: edge.id.clone(),
                source: edge.start.clone()?,
                target: edge.end.clone()?,
            })
        })
        .collect()
}

/// Runs fcose on the layout data, upstream's `layout.run()`.
pub fn execute_fcose_layout(
    data: &LayoutData,
    options: &FcoseOptions,
) -> Result<LayoutResult, LayoutError> {
    validate_layout_data(data)?;

    let positions = run_fcose_layout(&fcose_graph_nodes(data), &fcose_graph_edges(data), options)
        .map_err(|error| LayoutError::Dagre(error.to_string()))?;

    let positioned_nodes = positions
        .iter()
        .map(|FcosePosition { id, x, y }| PositionedNode {
            id: id.clone(),
            x: *x,
            y: *y,
        })
        .collect();

    Ok(LayoutResult {
        nodes: positioned_nodes,
        edges: straight_edges(data, &positions),
    })
}

/// The edges of the result, from center to center, cutting the endpoints short by the
/// radius the cose-bilkent renderer uses.
fn straight_edges(data: &LayoutData, positions: &[FcosePosition]) -> Vec<PositionedEdge> {
    let position_by_id: std::collections::HashMap<&str, (f64, f64)> = positions
        .iter()
        .map(|position| (position.id.as_str(), (position.x, position.y)))
        .collect();

    data.edges
        .iter()
        .filter_map(|edge| {
            let source = edge.start.as_deref()?;
            let target = edge.end.as_deref()?;
            let (source_x, source_y) = *position_by_id.get(source)?;
            let (target_x, target_y) = *position_by_id.get(target)?;

            let delta_x = target_x - source_x;
            let delta_y = target_y - source_y;
            let distance = delta_x.hypot(delta_y);
            let radius = 15.0;
            let (start_x, start_y, end_x, end_y) = if distance <= 2.0 * radius || distance == 0.0 {
                (source_x, source_y, target_x, target_y)
            } else {
                let unit_x = delta_x / distance;
                let unit_y = delta_y / distance;
                (
                    source_x + radius * unit_x,
                    source_y + radius * unit_y,
                    target_x - radius * unit_x,
                    target_y - radius * unit_y,
                )
            };

            Some(PositionedEdge {
                id: edge.id.clone(),
                source: source.to_string(),
                target: target.to_string(),
                start_x,
                start_y,
                mid_x: (start_x + end_x) / 2.0,
                mid_y: (start_y + end_y) / 2.0,
                end_x,
                end_y,
            })
        })
        .collect()
}

/// Render function for the fcose layout algorithm, upstream's architecture
/// `layout.run()` plus the pipeline's painting step.
pub fn fcose_render(
    data4_layout: &mut LayoutData,
    measure: &dyn TextMeasure,
) -> Result<Scene, LayoutError> {
    validate_layout_data(data4_layout)?;
    let options = default_fcose_options();

    render_layout_result(data4_layout, measure, |data| {
        execute_fcose_layout(data, &options)
    })
}

/// The renderer options no fcose layout uses, so the pipeline can read the algorithm's
/// name without special-casing it.
pub fn fcose_render_options() -> RenderOptions {
    RenderOptions {
        algorithm: Some("fcose".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure::FallbackMeasure;
    use crate::render::pipeline::render;
    use crate::render::pipeline::types::{Edge, Node};

    #[test]
    fn fcose_is_a_registered_layout_algorithm() {
        assert_eq!(
            super::super::get_registered_layout_algorithm("fcose", None),
            "fcose"
        );
        assert!(super::super::get_layout_loader_definition("fcose").is_ok());
    }

    #[test]
    fn the_pipeline_runs_the_fcose_algorithm() {
        let mut data = LayoutData {
            layout_algorithm: Some("fcose".to_string()),
            root_node: Some(serde_json::json!({"id": "root"})),
            nodes: vec![
                Node::new("a", "A", "rect"),
                Node::new("b", "B", "rect"),
                Node::new("c", "C", "rect"),
            ],
            edges: vec![Edge::new("ab", "a", "b"), Edge::new("bc", "b", "c")],
            ..Default::default()
        };

        let scene = render(&mut data, &FallbackMeasure).expect("render");

        assert!(scene.view_box.width > 0.0);
        for node in &data.nodes {
            assert!(node.x.is_some_and(f64::is_finite));
            assert!(node.y.is_some_and(f64::is_finite));
        }
    }
}
