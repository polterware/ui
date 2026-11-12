//! Port of cytoscape-fcose's `src/fcose/cose.js`: the CoSE post-processing that runs on
//! cose-base 2.2.0 after the spectral draft.
//!
//! The graph is transferred to a [`CoSELayout::new_v2`] the same way upstream does it:
//! top-most nodes first, each compound becoming a child graph, then the edges (one per
//! pair of nodes), then the options that upstream writes into `CoSEConstants`, and the
//! constraints. The constants fcose changes per run (the average ideal edge length and
//! the values derived from it) are instance fields of the layout here; the ones that
//! only choose a code path (`TILE`, `PURE_INCREMENTAL`, `APPLY_LAYOUT`, ...) are the
//! layout's flags.
//!
//! The result is upstream's `idToLNode` map: the nodes in creation order, each with its
//! rectangle after `runLayout`.

use crate::layout::cose_base::{CoSELayout, Constraints};
use crate::layout::layout_base::LayoutError;
use crate::layout::layout_base::util::dimension_d::DimensionD;
use crate::layout::layout_base::util::point::Point;

use super::FcoseEdgeInput;
use super::FcoseGraph;
use super::FcoseOptions;
use super::FcoseQuality;
use super::auxiliary;
use super::spectral::SpectralResult;

/// A node rectangle after the CoSE layout, upstream's `RectangleD` in `idToLNode`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CoseRect {
    /// Left edge.
    pub(crate) x: f64,
    /// Top edge.
    pub(crate) y: f64,
    /// Width.
    pub(crate) width: f64,
    /// Height.
    pub(crate) height: f64,
}

impl CoseRect {
    /// Center x, upstream's `getRect().getCenterX()`.
    pub(crate) fn get_center_x(&self) -> f64 {
        self.x + self.width / 2.0
    }

    /// Center y.
    pub(crate) fn get_center_y(&self) -> f64 {
        self.y + self.height / 2.0
    }
}

/// The CoSE result, upstream's `idToLNode` object in `Object.keys` order.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct CoseResult {
    /// Each node's id and rectangle.
    pub(crate) rects: Vec<(String, CoseRect)>,
}

/// Upstream `coseLayout`.
pub(crate) fn cose_layout(
    graph: &FcoseGraph,
    options: &FcoseOptions,
    spectral_result: Option<&SpectralResult>,
) -> Result<CoseResult, LayoutError> {
    let all_nodes: Vec<usize> = (0..graph.nodes.len()).collect();
    let top_most_nodes = auxiliary::get_top_most_nodes(graph, &all_nodes);
    let has_constraints = options.has_constraints();

    let mut layout: CoSELayout<()> = CoSELayout::new_v2();

    // The option fields fcose writes into the layout's constants.
    layout.quality = Some(if options.quality == FcoseQuality::Proof {
        2
    } else {
        0
    });
    layout.nesting_factor = Some(options.nesting_factor);
    layout.gravity = Some(options.gravity);
    layout.num_iter = Some(options.num_iter);
    layout.gravity_range = Some(options.gravity_range);
    layout.gravity_compound = Some(options.gravity_compound);
    layout.gravity_range_compound = Some(options.gravity_range_compound);
    layout.initial_energy_on_incremental = Some(options.initial_energy_on_incremental);

    // fcose always sets `DEFAULT_INCREMENTAL` to true and `PURE_INCREMENTAL` to the
    // inverse of `randomize`.
    layout.default_incremental = true;
    layout.pure_incremental = !options.randomize;
    layout.tile = options.tile && !has_constraints;
    layout.transform_on_constraint_handling = options.randomize;
    layout.enforce_constraints = true;
    layout.apply_layout = true;
    layout.tree_reduction_on_incremental = !has_constraints;
    layout.tiling_padding_vertical = options.tiling_padding_vertical;
    layout.tiling_padding_horizontal = options.tiling_padding_horizontal;

    layout.constraints = Constraints {
        fixed_node_constraint: options.fixed_node_constraint.clone(),
        alignment_constraint: options.alignment_constraint.clone(),
        relative_placement_constraint: options.relative_placement_constraint.clone(),
    };

    layout.new_graph_manager();
    let root = layout.fd.add_root();

    let mut id_to_lnode: Vec<(String, crate::layout::layout_base::NodeId)> = Vec::new();

    process_children_list(
        &mut layout,
        root,
        &top_most_nodes,
        graph,
        options,
        spectral_result,
        &mut id_to_lnode,
    )?;

    // Move the edge options and the average ideal edge length into the layout.
    process_edges(&mut layout, graph, options, &id_to_lnode)?;

    layout.run_layout()?;

    let rects = id_to_lnode
        .into_iter()
        .map(|(id, node)| {
            let rect = *layout.fd.layout.graph_manager.node(node).get_rect();
            (
                id,
                CoseRect {
                    x: rect.x,
                    y: rect.y,
                    width: rect.width,
                    height: rect.height,
                },
            )
        })
        .collect();

    Ok(CoseResult { rects })
}

/// Upstream `processChildrenList`: adds one node per child, recursing into compounds.
#[allow(clippy::too_many_arguments)]
fn process_children_list(
    layout: &mut CoSELayout<()>,
    parent_graph: crate::layout::layout_base::GraphId,
    children: &[usize],
    graph: &FcoseGraph,
    options: &FcoseOptions,
    spectral_result: Option<&SpectralResult>,
    id_to_lnode: &mut Vec<(String, crate::layout::layout_base::NodeId)>,
) -> Result<(), LayoutError> {
    for &child in children {
        let node = graph.node(child).clone();
        let dimensions = (node.width, node.height);

        let (left, top, width, height) = if options.randomize {
            let Some(spectral_result) = spectral_result else {
                return Err(LayoutError(
                    "A randomized fcose layout needs the spectral result",
                ));
            };
            if !graph.is_parent(child) {
                let index = spectral_result.index_of(&node.id);
                let left = index
                    .map(|index| spectral_result.x_coords[index] - dimensions.0 / 2.0)
                    .unwrap_or(f64::NAN);
                let top = index
                    .map(|index| spectral_result.y_coords[index] - dimensions.1 / 2.0)
                    .unwrap_or(f64::NAN);
                (left, top, dimensions.0, dimensions.1)
            } else {
                let (top_left_x, top_left_y, parent_width, parent_height) =
                    auxiliary::calc_bounding_box(
                        graph,
                        child,
                        &spectral_result.x_coords,
                        &spectral_result.y_coords,
                        &spectral_result.node_indexes,
                    );
                (top_left_x, top_left_y, parent_width, parent_height)
            }
        } else {
            (
                node.x - dimensions.0 / 2.0,
                node.y - dimensions.1 / 2.0,
                dimensions.0,
                dimensions.1,
            )
        };

        // Upstream replaces a NaN rectangle origin with zero.
        let left = if left.is_nan() { 0.0 } else { left };
        let top = if top.is_nan() { 0.0 } else { top };

        let the_node = layout.new_node(None);
        layout
            .fd
            .layout
            .graph_manager
            .graph_add_node(parent_graph, the_node)?;
        layout
            .fd
            .layout
            .graph_manager
            .node_mut(the_node)
            .set_rect(&Point::new(left, top), &DimensionD::new(width, height));
        layout.fd.node_mut(the_node).id = Some(node.id.clone());
        layout.fd.node_mut(the_node).node_repulsion = options.node_repulsion;

        // `parseInt(css('padding'))` truncates.
        let padding = node.padding.trunc();
        let lnode = layout.fd.layout.graph_manager.node_mut(the_node);
        lnode.padding_left = Some(padding);
        lnode.padding_top = Some(padding);
        lnode.padding_right = Some(padding);
        lnode.padding_bottom = Some(padding);

        id_to_lnode.push((node.id.clone(), the_node));

        if !graph.children(child).is_empty() {
            let new_graph_id = layout.new_graph(None);
            let new_graph = layout
                .fd
                .layout
                .graph_manager
                .add_graph(new_graph_id, the_node)?;
            process_children_list(
                layout,
                new_graph,
                graph.children(child),
                graph,
                options,
                spectral_result,
                id_to_lnode,
            )?;
        }
    }

    Ok(())
}

/// Upstream `processEdges`: adds one CoSE edge per pair of nodes and derives the default
/// edge length from the average ideal length.
fn process_edges(
    layout: &mut CoSELayout<()>,
    graph: &FcoseGraph,
    options: &FcoseOptions,
    id_to_lnode: &[(String, crate::layout::layout_base::NodeId)],
) -> Result<(), LayoutError> {
    let mut ideal_length_total = 0.0;
    let mut edge_count = 0;

    for edge in &graph.edges {
        let source_id = &graph.node(edge.source).id;
        let target_id = &graph.node(edge.target).id;
        let source_node = id_to_lnode
            .iter()
            .find(|(id, _)| id == source_id)
            .map(|(_, node)| *node);
        let target_node = id_to_lnode
            .iter()
            .find(|(id, _)| id == target_id)
            .map(|(_, node)| *node);

        let (Some(source_node), Some(target_node)) = (source_node, target_node) else {
            continue;
        };
        if source_node == target_node
            || !layout
                .fd
                .layout
                .graph_manager
                .node_edges_between(source_node, target_node)
                .is_empty()
        {
            continue;
        }

        let e1 = layout.new_edge(None);
        layout
            .fd
            .layout
            .graph_manager
            .add_edge(e1, source_node, target_node)?;

        let input = FcoseEdgeInput {
            id: edge.id.clone(),
            source: source_id.clone(),
            target: target_id.clone(),
            source_parent: graph
                .node(edge.source)
                .parent
                .map(|parent| graph.node(parent).id.clone()),
            target_parent: graph
                .node(edge.target)
                .parent
                .map(|parent| graph.node(parent).id.clone()),
        };
        let ideal_length = options.ideal_edge_length.resolve(&input);
        let edge_elasticity = options.edge_elasticity.resolve(&input);

        layout.fd.edge_mut(e1).ideal_length = ideal_length;
        layout.fd.edge_mut(e1).edge_elasticity = edge_elasticity;

        ideal_length_total += ideal_length;
        edge_count += 1;
    }

    // Upstream updates the shared defaults from the average ideal length.
    let default_edge_length = if edge_count > 0 {
        ideal_length_total / edge_count as f64
    } else {
        options.ideal_edge_length.constant().unwrap_or(50.0)
    };

    layout.fd.default_edge_length = default_edge_length;
    layout.fd.min_repulsion_dist = default_edge_length / 10.0;
    layout.default_radial_separation = default_edge_length;
    layout.fd.displacement_threshold_per_node = 3.0 * default_edge_length / 100.0;

    Ok(())
}

/// Upstream `auxiliary.relocateComponent` for a CoSE result: moves every rectangle so the
/// result keeps the center the graph had before the run.
pub(crate) fn relocate_component(
    result: &mut CoseResult,
    original_center: (f64, f64),
    has_constraints: bool,
) {
    if has_constraints {
        return;
    }

    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for (_, rect) in &result.rects {
        if rect.x < min_x {
            min_x = rect.x;
        }
        if rect.x + rect.width > max_x {
            max_x = rect.x + rect.width;
        }
        if rect.y < min_y {
            min_y = rect.y;
        }
        if rect.y + rect.height > max_y {
            max_y = rect.y + rect.height;
        }
    }

    let diff_on_x = original_center.0 - (max_x + min_x) / 2.0;
    let diff_on_y = original_center.1 - (max_y + min_y) / 2.0;

    for (_, rect) in &mut result.rects {
        rect.x += diff_on_x;
        rect.y += diff_on_y;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::fcose::FcoseEdge;
    use crate::layout::fcose::FcoseNode;

    #[test]
    fn a_proof_run_returns_one_rectangle_per_node() {
        let nodes = vec![
            FcoseNode::new("a", 0.0, 0.0, 40.0, 40.0),
            FcoseNode::new("b", 100.0, 0.0, 40.0, 40.0),
        ];
        let edges = vec![FcoseEdge::new("ab", "a", "b")];
        let graph = FcoseGraph::new(&nodes, &edges);
        let options = FcoseOptions {
            quality: FcoseQuality::Proof,
            randomize: false,
            num_iter: 50,
            ..Default::default()
        };

        let result = cose_layout(&graph, &options, None).unwrap();

        assert_eq!(result.rects.len(), 2);
        for (_, rect) in &result.rects {
            assert!(rect.get_center_x().is_finite());
            assert!(rect.get_center_y().is_finite());
        }
    }
}
