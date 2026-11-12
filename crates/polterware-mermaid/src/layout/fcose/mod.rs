//! Port of cytoscape-fcose 2.2.0, the layout Mermaid's architecture diagram runs.
//!
//! The sources are `src/fcose/index.js` ([`self`]), `cose.js` ([`cose`]),
//! `spectral.js` ([`spectral`]), and `auxiliary.js` ([`auxiliary`]) under
//! `research/mermaid-port/upstream/mermaid/node_modules/.pnpm/cytoscape-fcose@2.2.0_cytoscape@3.34.0`.
//! The CoSE post-processing runs cose-base 2.2.0, which is already ported in
//! [`crate::layout::cose_base`]; this module only builds the layout from a graph and
//! reads its rectangles back.
//!
//! Mermaid calls the layout from `diagrams/architecture/architectureRenderer.ts` with
//! `quality: 'proof'`, the configured `randomize` and `nodeSeparation`, `numIter`,
//! `nodeDimensionsIncludeLabels: false`, an `idealEdgeLength` and an `edgeElasticity`
//! function of the edge's parent, and the alignment and relative placement constraints
//! derived from the diagram. [`FcoseOptions`] exposes those options; [`FcoseEdgeValue`]
//! is the function form of an edge option.
//!
//! Randomness is Mermaid's: `architectureSeed.ts` replaces `Math.random` with a
//! mulberry32 generator seeded by `config.architecture.seed` (default 1) around each
//! `layout.run()`, and every random draw of fcose and of cose-base goes through it.
//! [`run_fcose_layout`] selects that generator through
//! [`set_math_random_seed`](crate::layout::layout_base::util::random_seed::set_math_random_seed),
//! so a run repeats for the same [`FcoseOptions::seed`].
//!
//! Where fcose calls cytoscape's collection API (`nodes()`, `children()`,
//! `descendants()`, `neighborhood()`, `connectedEdges()`, `boundingBox()`,
//! `layoutDimensions()`), this port works on [`FcoseGraph`], the layout's own graph:
//! node indices in the order the input lists them, direct children in input order, and
//! bounding boxes from each node's center and size. Packing is not ported: Mermaid does
//! not register cytoscape-layout-utilities, so fcose's `packComponents` branch never
//! runs and the whole graph is laid out as one component.
//!
//! `nodeDimensionsIncludeLabels` is accepted but the caller supplies the dimensions: a
//! [`FcoseNode`]'s width and height are the values cytoscape's `layoutDimensions` would
//! return, labels included when the option asks for them. The port has no renderer to
//! measure a label with.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use crate::layout::cose_base::{
    AlignmentConstraint, FixedNodeConstraint, RelativePlacementConstraint,
};
use crate::layout::layout_base::LayoutError;
use crate::layout::layout_base::util::random_seed;

pub mod auxiliary;
pub mod cose;
pub mod spectral;

/// The initial-position strategy, upstream's `quality`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FcoseQuality {
    /// Spectral layout plus CoSE with fast cooling.
    #[default]
    Default,
    /// Spectral layout only.
    Draft,
    /// Spectral layout plus CoSE with slow cooling.
    Proof,
}

/// A node of the layout, as fcose reads it from cytoscape.
#[derive(Clone, Debug, PartialEq)]
pub struct FcoseNode {
    /// The node's id.
    pub id: String,
    /// The id of the compound node that contains it, upstream's `parent`.
    pub parent_id: Option<String>,
    /// The center x, upstream's `position('x')`.
    pub x: f64,
    /// The center y.
    pub y: f64,
    /// The width, upstream's `outerWidth()` or `layoutDimensions().w`.
    pub width: f64,
    /// The height.
    pub height: f64,
    /// The compound padding, upstream's `parseInt(css('padding'))`; 0 for a simple node.
    pub padding: f64,
}

impl FcoseNode {
    /// A simple node.
    pub fn new(id: impl Into<String>, x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            id: id.into(),
            parent_id: None,
            x,
            y,
            width,
            height,
            padding: 0.0,
        }
    }
}

/// An edge of the layout, as fcose reads it from cytoscape.
#[derive(Clone, Debug, PartialEq)]
pub struct FcoseEdge {
    /// The edge's id.
    pub id: String,
    /// The id of the source node.
    pub source: String,
    /// The id of the target node.
    pub target: String,
}

impl FcoseEdge {
    /// An edge.
    pub fn new(
        id: impl Into<String>,
        source: impl Into<String>,
        target: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            source: source.into(),
            target: target.into(),
        }
    }
}

/// What an edge-valued option sees, upstream's `edge` argument of
/// `idealEdgeLength(edge)` and `edgeElasticity(edge)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FcoseEdgeInput {
    /// The edge's id.
    pub id: String,
    /// The source node's id.
    pub source: String,
    /// The target node's id.
    pub target: String,
    /// The source node's parent, when it has one.
    pub source_parent: Option<String>,
    /// The target node's parent, when it has one.
    pub target_parent: Option<String>,
}

/// An option that fcose reads for each edge, upstream's number or function.
#[derive(Clone)]
pub enum FcoseEdgeValue {
    /// The same value for every edge.
    Constant(f64),
    /// A value computed from the edge.
    Function(Arc<dyn Fn(&FcoseEdgeInput) -> f64 + Send + Sync>),
}

impl FcoseEdgeValue {
    /// The value for one edge.
    pub fn resolve(&self, edge: &FcoseEdgeInput) -> f64 {
        match self {
            FcoseEdgeValue::Constant(value) => *value,
            FcoseEdgeValue::Function(function) => function(edge),
        }
    }

    /// The constant value, `None` for a function.
    pub fn constant(&self) -> Option<f64> {
        match self {
            FcoseEdgeValue::Constant(value) => Some(*value),
            FcoseEdgeValue::Function(_) => None,
        }
    }
}

impl fmt::Debug for FcoseEdgeValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FcoseEdgeValue::Constant(value) => f.debug_tuple("Constant").field(value).finish(),
            FcoseEdgeValue::Function(_) => f.write_str("Function(..)"),
        }
    }
}

impl Default for FcoseEdgeValue {
    fn default() -> Self {
        FcoseEdgeValue::Constant(50.0)
    }
}

/// The options of a run, upstream's `defaults` in `fcose/index.js`.
#[derive(Clone, Debug)]
pub struct FcoseOptions {
    /// The initial-position strategy.
    pub quality: FcoseQuality,
    /// Whether the spectral layout places the nodes first.
    pub randomize: bool,
    /// Whether the layout animates; the port does not animate.
    pub animate: bool,
    /// Whether disconnected components are packed; the port does not pack.
    pub pack_components: bool,
    /// Whether the node dimensions the caller supplies include the labels. The port does
    /// not measure labels; see the module docs.
    pub node_dimensions_include_labels: bool,
    /// Whether the spectral columns are sampled greedily, else randomly.
    pub sampling_type: bool,
    /// How many columns the spectral layout samples.
    pub sample_size: usize,
    /// How far apart the spectral layout spaces the nodes.
    pub node_separation: f64,
    /// The power iteration tolerance of the spectral layout.
    pub pi_tol: f64,
    /// Node repulsion multiplier, upstream's `nodeRepulsion`.
    pub node_repulsion: f64,
    /// Ideal edge length, upstream's `idealEdgeLength`.
    pub ideal_edge_length: FcoseEdgeValue,
    /// Spring constant, upstream's `edgeElasticity`.
    pub edge_elasticity: FcoseEdgeValue,
    /// How much nesting adds to an edge's ideal length.
    pub nesting_factor: f64,
    /// Gravity force.
    pub gravity: f64,
    /// Maximum number of CoSE iterations.
    pub num_iter: i64,
    /// Whether zero-degree members and compounds are tiled.
    pub tile: bool,
    /// Vertical space between two tiled rows.
    pub tiling_padding_vertical: f64,
    /// Horizontal space between two tiled nodes.
    pub tiling_padding_horizontal: f64,
    /// Gravity range of a compound.
    pub gravity_range_compound: f64,
    /// Gravity force inside a compound.
    pub gravity_compound: f64,
    /// Gravity range.
    pub gravity_range: f64,
    /// Initial cooling factor of an incremental run.
    pub initial_energy_on_incremental: f64,
    /// Nodes that must keep a position.
    pub fixed_node_constraint: Option<Vec<FixedNodeConstraint>>,
    /// Groups that must be aligned.
    pub alignment_constraint: Option<AlignmentConstraint>,
    /// Required relative placements.
    pub relative_placement_constraint: Option<Vec<RelativePlacementConstraint>>,
    /// The seed of Mermaid's `Math.random`. Not upstream; it makes a run repeat.
    pub seed: u32,
}

impl Default for FcoseOptions {
    fn default() -> Self {
        Self {
            quality: FcoseQuality::Default,
            randomize: true,
            animate: true,
            pack_components: true,
            node_dimensions_include_labels: false,
            sampling_type: true,
            sample_size: 25,
            node_separation: 75.0,
            pi_tol: 0.0000001,
            node_repulsion: 4500.0,
            ideal_edge_length: FcoseEdgeValue::Constant(50.0),
            edge_elasticity: FcoseEdgeValue::Constant(0.45),
            nesting_factor: 0.1,
            gravity: 0.25,
            num_iter: 2500,
            tile: true,
            tiling_padding_vertical: 10.0,
            tiling_padding_horizontal: 10.0,
            gravity_range_compound: 1.5,
            gravity_compound: 1.0,
            gravity_range: 3.8,
            initial_energy_on_incremental: 0.3,
            fixed_node_constraint: None,
            alignment_constraint: None,
            relative_placement_constraint: None,
            seed: 1,
        }
    }
}

impl FcoseOptions {
    /// Whether the options carry any constraint, upstream's `constraintExist`.
    pub fn has_constraints(&self) -> bool {
        self.fixed_node_constraint.is_some()
            || self.alignment_constraint.is_some()
            || self.relative_placement_constraint.is_some()
    }

    /// Drops empty constraint arrays, upstream's validity check at the start of `run`.
    pub fn sanitized(mut self) -> Self {
        if self
            .fixed_node_constraint
            .as_ref()
            .is_some_and(|constraint| constraint.is_empty())
        {
            self.fixed_node_constraint = None;
        }

        if let Some(alignment) = &mut self.alignment_constraint {
            if alignment
                .vertical
                .as_ref()
                .is_some_and(|groups| groups.is_empty())
            {
                alignment.vertical = None;
            }
            if alignment
                .horizontal
                .as_ref()
                .is_some_and(|groups| groups.is_empty())
            {
                alignment.horizontal = None;
            }
        }

        if self
            .relative_placement_constraint
            .as_ref()
            .is_some_and(|constraint| constraint.is_empty())
        {
            self.relative_placement_constraint = None;
        }

        self
    }
}

/// The position of a node after a run.
#[derive(Clone, Debug, PartialEq)]
pub struct FcosePosition {
    /// The node's id.
    pub id: String,
    /// The center x.
    pub x: f64,
    /// The center y.
    pub y: f64,
}

/// A node of the layout's own graph.
#[derive(Clone, Debug)]
pub(crate) struct GraphNode {
    pub(crate) id: String,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) padding: f64,
    pub(crate) parent: Option<usize>,
    pub(crate) children: Vec<usize>,
}

/// An edge of the layout's own graph.
#[derive(Clone, Debug)]
pub(crate) struct GraphEdge {
    pub(crate) id: String,
    pub(crate) source: usize,
    pub(crate) target: usize,
}

/// The layout's own graph, the part of cytoscape's collection API fcose uses.
pub(crate) struct FcoseGraph {
    pub(crate) nodes: Vec<GraphNode>,
    pub(crate) edges: Vec<GraphEdge>,
    pub(crate) index: HashMap<String, usize>,
}

impl FcoseGraph {
    fn new(input_nodes: &[FcoseNode], input_edges: &[FcoseEdge]) -> Self {
        let index: HashMap<String, usize> = input_nodes
            .iter()
            .enumerate()
            .map(|(i, node)| (node.id.clone(), i))
            .collect();

        let mut nodes: Vec<GraphNode> = input_nodes
            .iter()
            .map(|node| GraphNode {
                id: node.id.clone(),
                x: node.x,
                y: node.y,
                width: node.width,
                height: node.height,
                padding: node.padding,
                parent: node
                    .parent_id
                    .as_ref()
                    .and_then(|parent| index.get(parent))
                    .copied(),
                children: Vec::new(),
            })
            .collect();

        for i in 0..nodes.len() {
            if let Some(parent) = nodes[i].parent {
                nodes[parent].children.push(i);
            }
        }

        let edges = input_edges
            .iter()
            .filter_map(|edge| {
                let source = *index.get(&edge.source)?;
                let target = *index.get(&edge.target)?;
                Some(GraphEdge {
                    id: edge.id.clone(),
                    source,
                    target,
                })
            })
            .collect();

        Self {
            nodes,
            edges,
            index,
        }
    }

    pub(crate) fn node(&self, node: usize) -> &GraphNode {
        &self.nodes[node]
    }

    pub(crate) fn edge(&self, edge: usize) -> &GraphEdge {
        &self.edges[edge]
    }

    /// Upstream `node.isParent()`: the node has at least one child.
    pub(crate) fn is_parent(&self, node: usize) -> bool {
        !self.nodes[node].children.is_empty()
    }

    /// Upstream `node.children()`.
    pub(crate) fn children(&self, node: usize) -> &[usize] {
        &self.nodes[node].children
    }

    /// Upstream `node.descendants()`: depth-first, children in input order.
    pub(crate) fn descendants(&self, node: usize) -> Vec<usize> {
        fn add(graph: &FcoseGraph, nodes: &[usize], out: &mut Vec<usize>) {
            for &node in nodes {
                out.push(node);
                let children = graph.children(node);
                if !children.is_empty() {
                    let children = children.to_vec();
                    add(graph, &children, out);
                }
            }
        }

        let mut out = Vec::new();
        let children = self.children(node).to_vec();
        add(self, &children, &mut out);
        out
    }

    /// Upstream `node.ancestors()`: parent first.
    pub(crate) fn ancestors(&self, node: usize) -> Vec<usize> {
        let mut out = Vec::new();
        let mut current = self.nodes[node].parent;
        while let Some(parent) = current {
            out.push(parent);
            current = self.nodes[parent].parent;
        }
        out
    }

    /// The depth of a node in the compound tree, 0 at the top.
    pub(crate) fn depth(&self, node: usize) -> usize {
        self.ancestors(node).len()
    }

    /// Upstream `node.neighborhood().nodes()`: the other ends of the node's edges, in
    /// edge order, once each.
    pub(crate) fn neighbors(&self, node: usize) -> Vec<usize> {
        let mut out = Vec::new();
        for edge in &self.edges {
            let other = if edge.source == node {
                edge.target
            } else if edge.target == node {
                edge.source
            } else {
                continue;
            };
            if !out.contains(&other) {
                out.push(other);
            }
        }
        out
    }

    /// Upstream `node.connectedEdges()`.
    pub(crate) fn connected_edges(&self, node: usize) -> Vec<usize> {
        self.edges
            .iter()
            .enumerate()
            .filter(|(_, edge)| edge.source == node || edge.target == node)
            .map(|(i, _)| i)
            .collect()
    }

    /// Upstream `node.edgesWith(other)`: the edges between two nodes.
    pub(crate) fn edges_with(&self, node: usize, other: usize) -> Vec<usize> {
        self.edges
            .iter()
            .enumerate()
            .filter(|(_, edge)| {
                (edge.source == node && edge.target == other)
                    || (edge.source == other && edge.target == node)
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Upstream `nodes.boundingBox()` over nodes only: the extent of their rectangles.
    pub(crate) fn bounding_box(&self, nodes: &[usize]) -> BoundingBox {
        let mut bounds = BoundingBox {
            x1: f64::INFINITY,
            y1: f64::INFINITY,
            x2: f64::NEG_INFINITY,
            y2: f64::NEG_INFINITY,
        };

        for &node in nodes {
            let node = self.node(node);
            bounds.x1 = bounds.x1.min(node.x - node.width / 2.0);
            bounds.x2 = bounds.x2.max(node.x + node.width / 2.0);
            bounds.y1 = bounds.y1.min(node.y - node.height / 2.0);
            bounds.y2 = bounds.y2.max(node.y + node.height / 2.0);
        }

        if bounds.x1 > bounds.x2 || bounds.y1 > bounds.y2 {
            BoundingBox {
                x1: 0.0,
                y1: 0.0,
                x2: 0.0,
                y2: 0.0,
            }
        } else {
            bounds
        }
    }

    /// The center of a bounding box, upstream's `{x: x1 + w / 2, y: y1 + h / 2}`.
    pub(crate) fn bounding_box_center(&self, nodes: &[usize]) -> (f64, f64) {
        let bounds = self.bounding_box(nodes);
        ((bounds.x1 + bounds.x2) / 2.0, (bounds.y1 + bounds.y2) / 2.0)
    }
}

/// A bounding box, upstream's `{x1, y1, x2, y2, w, h}`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BoundingBox {
    pub(crate) x1: f64,
    pub(crate) y1: f64,
    pub(crate) x2: f64,
    pub(crate) y2: f64,
}

/// Runs fcose on a graph, upstream `Layout.prototype.run` in `fcose/index.js`.
///
/// Returns every node's center, simple nodes first in input order and then the
/// compounds, whose bounds follow their children.
pub fn run_fcose_layout(
    nodes: &[FcoseNode],
    edges: &[FcoseEdge],
    options: &FcoseOptions,
) -> Result<Vec<FcosePosition>, LayoutError> {
    let options = options.clone().sanitized();
    if nodes.is_empty() {
        return Ok(Vec::new());
    }

    let graph = FcoseGraph::new(nodes, edges);
    let all_nodes: Vec<usize> = (0..graph.nodes.len()).collect();
    let original_center = graph.bounding_box_center(&all_nodes);

    // Only a fixed node pins the layout: alignment and relative placement still recenter.
    let has_fixed_nodes = options.fixed_node_constraint.is_some();

    // Mermaid replaces `Math.random` for exactly one `layout.run()`.
    random_seed::set_math_random_seed(options.seed);

    let mut positions = if options.quality == FcoseQuality::Draft {
        if !options.randomize {
            // Upstream logs that draft needs randomize and leaves the positions alone.
            return Ok(input_positions(&graph));
        }

        let mut spectral = spectral::spectral_layout(&graph, &options);
        spectral::relocate_component(&mut spectral, original_center, has_fixed_nodes, &graph);
        spectral_positions(&graph, &spectral)
    } else {
        let spectral_result = if options.randomize {
            Some(spectral::spectral_layout(&graph, &options))
        } else {
            None
        };

        let mut result = cose::cose_layout(&graph, &options, spectral_result.as_ref())?;
        cose::relocate_component(&mut result, original_center, has_fixed_nodes);
        let by_id: HashMap<&str, cose::CoseRect> = result
            .rects
            .iter()
            .map(|(id, rect)| (id.as_str(), *rect))
            .collect();
        graph
            .nodes
            .iter()
            .filter_map(|node| {
                by_id.get(node.id.as_str()).map(|rect| FcosePosition {
                    id: node.id.clone(),
                    x: rect.get_center_x(),
                    y: rect.get_center_y(),
                })
            })
            .collect()
    };

    // Cytoscape positions the simple nodes and derives each compound's bounds from its
    // children; fcose itself leaves compound positions to that step.
    compound_positions(&graph, &mut positions);

    Ok(positions)
}

/// The input positions, upstream's transfer when no layout step runs.
fn input_positions(graph: &FcoseGraph) -> Vec<FcosePosition> {
    graph
        .nodes
        .iter()
        .map(|node| FcosePosition {
            id: node.id.clone(),
            x: node.x,
            y: node.y,
        })
        .collect()
}

/// The positions of the spectral result: simple nodes from its coordinate arrays and
/// compounds from their children.
fn spectral_positions(graph: &FcoseGraph, result: &spectral::SpectralResult) -> Vec<FcosePosition> {
    let mut positions = Vec::with_capacity(graph.nodes.len());
    for node in &graph.nodes {
        let position = result
            .index_of(&node.id)
            .map(|index| (result.x_coords[index], result.y_coords[index]))
            .unwrap_or((node.x, node.y));
        positions.push(FcosePosition {
            id: node.id.clone(),
            x: position.0,
            y: position.1,
        });
    }

    positions
}

/// Places every compound at the center of its children, each side padded, innermost
/// compounds first. Cytoscape's `updateCompoundBounds` (`bounds.mjs`) does this after a
/// layout positions the simple nodes.
fn compound_positions(graph: &FcoseGraph, positions: &mut [FcosePosition]) {
    let mut order: Vec<usize> = (0..graph.nodes.len())
        .filter(|&node| graph.is_parent(node))
        .collect();
    order.sort_by_key(|&node| std::cmp::Reverse(graph.depth(node)));

    let index_of: HashMap<String, usize> = positions
        .iter()
        .enumerate()
        .map(|(i, position)| (position.id.clone(), i))
        .collect();

    // Cytoscape's `boundingBox` expands every node by one point per side to cover
    // antialiasing (`bounds.mjs`: `expandBoundingBox(bounds, 1)`), and a compound's
    // `autoWidth` is its children's bounding box, so that extra point accumulates one
    // level per compound and moves the center when the extremes come from different
    // children.
    let mut bounding_sizes: Vec<(f64, f64)> = graph
        .nodes
        .iter()
        .map(|node| (node.width + 2.0, node.height + 2.0))
        .collect();

    for node in order {
        let mut x1 = f64::INFINITY;
        let mut y1 = f64::INFINITY;
        let mut x2 = f64::NEG_INFINITY;
        let mut y2 = f64::NEG_INFINITY;

        for &child in graph.children(node) {
            let Some(&child_position) = index_of.get(graph.node(child).id.as_str()) else {
                continue;
            };
            let (width, height) = bounding_sizes[child];
            let position = &positions[child_position];
            x1 = x1.min(position.x - width / 2.0);
            x2 = x2.max(position.x + width / 2.0);
            y1 = y1.min(position.y - height / 2.0);
            y2 = y2.max(position.y + height / 2.0);
        }

        if x1 > x2 || y1 > y2 {
            continue;
        }

        let padding = graph.node(node).padding;
        let outer_width = x2 - x1 + 2.0 * padding;
        let outer_height = y2 - y1 + 2.0 * padding;
        bounding_sizes[node] = (outer_width + 2.0, outer_height + 2.0);

        let Some(&position_index) = index_of.get(graph.node(node).id.as_str()) else {
            continue;
        };
        positions[position_index].x = (x1 + x2) / 2.0;
        positions[position_index].y = (y1 + y2) / 2.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph() -> (Vec<FcoseNode>, Vec<FcoseEdge>) {
        let nodes = vec![
            FcoseNode::new("a", 0.0, 0.0, 40.0, 40.0),
            FcoseNode::new("b", 100.0, 0.0, 40.0, 40.0),
            FcoseNode::new("c", 200.0, 0.0, 40.0, 40.0),
        ];
        let edges = vec![
            FcoseEdge::new("ab", "a", "b"),
            FcoseEdge::new("bc", "b", "c"),
        ];

        (nodes, edges)
    }

    #[test]
    fn a_draft_without_randomize_keeps_the_input_positions() {
        let (nodes, edges) = graph();
        let options = FcoseOptions {
            quality: FcoseQuality::Draft,
            randomize: false,
            ..Default::default()
        };

        let positions = run_fcose_layout(&nodes, &edges, &options).unwrap();

        assert_eq!(positions.len(), 3);
        for (position, node) in positions.iter().zip(&nodes) {
            assert_eq!(position.id, node.id);
            assert_eq!((position.x, position.y), (node.x, node.y));
        }
    }

    #[test]
    fn a_run_repeats_for_one_seed_and_differs_for_another() {
        let (nodes, edges) = graph();
        let options = FcoseOptions {
            num_iter: 100,
            ..Default::default()
        };

        let first = run_fcose_layout(&nodes, &edges, &options).unwrap();
        let second = run_fcose_layout(&nodes, &edges, &options).unwrap();
        assert_eq!(first, second);

        let other = run_fcose_layout(&nodes, &edges, &FcoseOptions { seed: 7, ..options }).unwrap();
        assert_ne!(first, other);
    }

    #[test]
    fn compounds_follow_their_children() {
        let mut parent = FcoseNode::new("group", 0.0, 0.0, 0.0, 0.0);
        parent.padding = 10.0;
        let mut a = FcoseNode::new("a", 0.0, 0.0, 40.0, 40.0);
        a.parent_id = Some("group".into());
        let mut b = FcoseNode::new("b", 100.0, 0.0, 40.0, 40.0);
        b.parent_id = Some("group".into());

        let positions = run_fcose_layout(
            &[parent, a, b],
            &[FcoseEdge::new("ab", "a", "b")],
            &FcoseOptions {
                quality: FcoseQuality::Proof,
                randomize: false,
                ..Default::default()
            },
        )
        .unwrap();

        let group = positions.iter().find(|p| p.id == "group").unwrap();
        let a = positions.iter().find(|p| p.id == "a").unwrap();
        let b = positions.iter().find(|p| p.id == "b").unwrap();
        assert!((group.x - (a.x + b.x) / 2.0).abs() < 1e-9);
        assert!((group.y - 0.0).abs() < 1e-9);
    }

    fn edge_value(value: &serde_json::Value) -> FcoseEdgeValue {
        if let Some(constant) = value.as_f64() {
            return FcoseEdgeValue::Constant(constant);
        }

        let same = value["same"].as_f64().expect("same");
        let cross = value["cross"].as_f64().expect("cross");
        FcoseEdgeValue::Function(Arc::new(move |edge: &FcoseEdgeInput| {
            if edge.source_parent == edge.target_parent {
                same
            } else {
                cross
            }
        }))
    }

    fn constraints(value: &serde_json::Value) -> FcoseOptions {
        let parse_fixed = |value: &serde_json::Value| {
            value.as_array().map(|entries| {
                entries
                    .iter()
                    .map(|entry| FixedNodeConstraint {
                        node_id: entry["nodeId"].as_str().expect("nodeId").to_string(),
                        position: crate::layout::layout_base::util::point_d::PointD::new(
                            entry["position"]["x"].as_f64().expect("x"),
                            entry["position"]["y"].as_f64().expect("y"),
                        ),
                    })
                    .collect()
            })
        };

        let parse_alignment = |value: &serde_json::Value| {
            let groups = |value: &serde_json::Value| {
                value.as_array().map(|groups| {
                    groups
                        .iter()
                        .map(|group| {
                            group
                                .as_array()
                                .expect("group")
                                .iter()
                                .map(|id| id.as_str().expect("id").to_string())
                                .collect()
                        })
                        .collect()
                })
            };
            value.as_object().map(|_| AlignmentConstraint {
                vertical: groups(&value["vertical"]),
                horizontal: groups(&value["horizontal"]),
            })
        };

        let parse_relative = |value: &serde_json::Value| {
            value.as_array().map(|entries| {
                entries
                    .iter()
                    .map(|entry| RelativePlacementConstraint {
                        left: entry["left"].as_str().map(str::to_string),
                        right: entry["right"].as_str().map(str::to_string),
                        top: entry["top"].as_str().map(str::to_string),
                        bottom: entry["bottom"].as_str().map(str::to_string),
                        gap: entry["gap"].as_f64(),
                    })
                    .collect()
            })
        };

        FcoseOptions {
            fixed_node_constraint: parse_fixed(&value["fixedNodeConstraint"]),
            alignment_constraint: parse_alignment(&value["alignmentConstraint"]),
            relative_placement_constraint: parse_relative(&value["relativePlacementConstraint"]),
            ..Default::default()
        }
    }

    #[test]
    fn every_fixture_matches_cytoscape_fcose() {
        let testdata_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/layout/fcose/testdata");
        let entries = std::fs::read_dir(&testdata_dir).expect("read fcose testdata");

        let mut count = 0;
        for entry in entries {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }

            let fixture: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).expect("read fixture"))
                    .expect("parse fixture");
            let input = &fixture["input"];
            let expected = &fixture["expected"];

            let nodes: Vec<FcoseNode> = input["nodes"]
                .as_array()
                .expect("nodes")
                .iter()
                .map(|node| FcoseNode {
                    id: node["id"].as_str().expect("id").to_string(),
                    parent_id: node["parentId"].as_str().map(str::to_string),
                    x: node["x"].as_f64().expect("x"),
                    y: node["y"].as_f64().expect("y"),
                    width: node["width"].as_f64().expect("width"),
                    height: node["height"].as_f64().expect("height"),
                    padding: node["padding"].as_f64().unwrap_or(0.0),
                })
                .collect();

            let edges: Vec<FcoseEdge> = input["edges"]
                .as_array()
                .expect("edges")
                .iter()
                .map(|edge| FcoseEdge {
                    id: edge["id"].as_str().expect("id").to_string(),
                    source: edge["source"].as_str().expect("source").to_string(),
                    target: edge["target"].as_str().expect("target").to_string(),
                })
                .collect();

            let options = input["options"].clone();
            let mut layout_options = constraints(&options);
            layout_options.quality = match options["quality"].as_str().expect("quality") {
                "draft" => FcoseQuality::Draft,
                "proof" => FcoseQuality::Proof,
                _ => FcoseQuality::Default,
            };
            layout_options.randomize = options["randomize"].as_bool().expect("randomize");
            layout_options.seed = options["seed"].as_u64().expect("seed") as u32;
            layout_options.node_separation =
                options["nodeSeparation"].as_f64().expect("nodeSeparation");
            layout_options.sample_size =
                options["sampleSize"].as_u64().expect("sampleSize") as usize;
            layout_options.num_iter = options["numIter"].as_i64().expect("numIter");
            layout_options.pi_tol = options["piTol"].as_f64().expect("piTol");
            layout_options.sampling_type = options["samplingType"].as_bool().expect("samplingType");
            layout_options.ideal_edge_length = edge_value(&options["idealEdgeLength"]);
            layout_options.edge_elasticity = edge_value(&options["edgeElasticity"]);

            let positions =
                run_fcose_layout(&nodes, &edges, &layout_options).expect("fcose layout");

            for expected_node in expected["nodes"].as_array().expect("expected nodes") {
                let id = expected_node["id"].as_str().expect("id");
                let position = positions
                    .iter()
                    .find(|position| position.id == id)
                    .unwrap_or_else(|| panic!("{}: node {id} in result", path.display()));
                let expected_x = expected_node["x"].as_f64().expect("x");
                let expected_y = expected_node["y"].as_f64().expect("y");
                assert!(
                    (position.x - expected_x).abs() <= 1e-6,
                    "{}: node {id} x: {} != {expected_x}",
                    path.display(),
                    position.x
                );
                assert!(
                    (position.y - expected_y).abs() <= 1e-6,
                    "{}: node {id} y: {} != {expected_y}",
                    path.display(),
                    position.y
                );
            }

            count += 1;
        }

        assert!(count >= 8, "expected at least 8 fixtures, got {count}");
    }
}
